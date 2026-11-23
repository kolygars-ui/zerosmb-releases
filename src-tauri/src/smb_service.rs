use crate::config::SmbConfig;
use crate::error::{AppError, AppResult};
use futures_util::StreamExt;
use serde::Serialize;
use smb::{
    binrw_util::file_time::FileTime, Client, ClientConfig, CreateDisposition, CreateOptions,
    DirAccessMask, Directory, FileAccessMask, FileAttributes, FileBasicInformation, FileCreateArgs,
    FileDispositionInformation, FileIdBothDirectoryInformation, GetLen, Guid, IoctlBuffer,
    PipeTransceiveRequest, ReadAt, Resource, UncPath, WriteAt,
};
use smb_rpc::pdu::{
    DcRpcCoPktBind, DcRpcCoPktBindContextElement, DceRpcCoPktBindAckDefResult, DcRpcCoPktRequest,
    DcRpcCoPktResponseContent, DceRpcCoPktFlags, DceRpcCoRequestPkt, DceRpcCoResponsePkt,
    DceRpcSyntaxId,
};
use std::collections::{HashMap, VecDeque};
use std::str::FromStr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub is_share: bool,
    pub size: u64,
    pub modified: i64,
    pub extension: String,
}

#[derive(Clone, Debug)]
pub struct RemoteStat {
    pub size: u64,
    pub modified: i64,
}

struct Cached {
    key: String,
    client: Arc<Client>,
    share: UncPath,
}

pub struct SmbService {
    cached: Mutex<Option<Cached>>,
}

impl SmbService {
    pub fn new() -> Self {
        Self {
            cached: Mutex::new(None),
        }
    }

    pub async fn reset(&self) {
        let mut guard = self.cached.lock().await;
        if let Some(cached) = guard.take() {
            let _ = cached.client.close().await;
        }
    }

    pub async fn connect(&self, cfg: &SmbConfig, password: &str) -> AppResult<Arc<Client>> {
        cfg.validate()?;
        if cfg.share.trim().is_empty() {
            return Err(AppError::msg("Выберите SMB-ресурс (share)"));
        }
        let key = format!(
            "{}|{}|{}|{}|{}",
            cfg.normalized_host(),
            cfg.port,
            cfg.share.trim(),
            cfg.auth_user(),
            password
        );

        let mut guard = self.cached.lock().await;
        if let Some(cached) = guard.as_ref() {
            if cached.key == key {
                return Ok(cached.client.clone());
            }
        }

        let mut client_config = ClientConfig::default();
        client_config.connection.port = Some(cfg.port);
        let client = Client::new(client_config);
        let host = cfg.normalized_host();
        let share = UncPath::from_str(&format!(
            r"\\{}\{}",
            host,
            cfg.share.trim()
        ))?;
        client
            .share_connect(&share, &cfg.auth_user(), password.to_string())
            .await
            .map_err(|error| {
                AppError::msg(format!(
                    "SMB {host}:{} (share {}): {error}. Проверьте LAN-IP, порт 445 и доступность NAS.",
                    cfg.port,
                    cfg.share.trim()
                ))
            })?;
        let client = Arc::new(client);
        *guard = Some(Cached {
            key,
            client: client.clone(),
            share,
        });
        Ok(client)
    }

    async fn share(&self) -> AppResult<UncPath> {
        let guard = self.cached.lock().await;
        guard
            .as_ref()
            .map(|c| c.share.clone())
            .ok_or_else(|| AppError::msg("SMB-соединение не установлено"))
    }

    pub async fn list_shares(
        cfg: &SmbConfig,
        server: &str,
        password: &str,
    ) -> AppResult<Vec<FileEntry>> {
        cfg.validate()?;
        let mut client_config = ClientConfig::default();
        client_config.connection.port = Some(cfg.port);
        let client = Client::new(client_config);
        let host = cfg.normalized_host();
        let ipc = UncPath::from_str(&format!(r"\\{}\IPC$", host))?;
        client
            .share_connect(&ipc, &cfg.auth_user(), password.to_string())
            .await
            .map_err(|error| {
                AppError::msg(format!(
                    "SMB {host}:{}. Не удалось подключиться к серверу для списка шеров: {error}",
                    cfg.port
                ))
            })?;
        let names = match Self::list_shares_ndr32(&client, &host, server).await {
            Ok(names) => names,
            Err(ndr32_error) => {
                log::warn!("NDR32 share enumeration failed: {ndr32_error}; trying NDR64");
                let shares = client.list_shares(server).await.map_err(|error| {
                    AppError::msg(format!(
                        "SMB {host}:{}. �� 㤠���� ������� ᯨ᮪ �஢: {error}",
                        cfg.port
                    ))
                })?;
                shares
                    .into_iter()
                    .filter_map(|share| {
                        share
                            .netname
                            .as_ref()
                            .map(|name| name.to_string())
                    })
                    .collect()
            }
        };
        let mut entries = Vec::new();
        for name in names {
            if name.trim().is_empty() || name.eq_ignore_ascii_case("IPC$") {
                continue;
            }
            entries.push(FileEntry {
                name: name.clone(),
                path: name,
                is_dir: true,
                is_share: true,
                size: 0,
                modified: 0,
                extension: String::new(),
            });
        }
        entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        Ok(entries)
    }

    async fn list_shares_ndr32(
        client: &Client,
        host: &str,
        server: &str,
    ) -> Result<Vec<String>, String> {
        let pipe = client
            .open_pipe(host, "srvsvc")
            .await
            .map_err(|error| format!("open srvsvc: {error}"))?;
        let ndr32 = DceRpcSyntaxId {
            uuid: "8a885d04-1ceb-11c9-9fe8-08002b104860"
                .parse::<Guid>()
                .map_err(|error| format!("NDR32 GUID: {error}"))?,
            version: 2,
        };
        let srvsvc = DceRpcSyntaxId {
            uuid: "4b324fc8-1670-01d3-1278-5a47bf6ee188"
                .parse::<Guid>()
                .map_err(|error| format!("srvsvc GUID: {error}"))?,
            version: 3,
        };
        let bind = DceRpcCoRequestPkt::new(
            DcRpcCoPktBind {
                max_xmit_frag: 4280,
                max_recv_frag: 4280,
                assoc_group_id: 0,
                context_elements: vec![DcRpcCoPktBindContextElement {
                    context_id: 0,
                    abstract_syntax: srvsvc,
                    transfer_syntaxes: vec![ndr32.clone()],
                }],
            }
            .into(),
            2,
            DceRpcCoPktFlags::new()
                .with_first_frag(true)
                .with_last_frag(true),
            0x10,
        );
        let bind_data: Vec<u8> = bind
            .try_into()
            .map_err(|error| format!("bind encode: {error}"))?;
        let response = pipe
            .fsctl_with_options(
                PipeTransceiveRequest::from(IoctlBuffer::from(bind_data)),
                65536,
            )
            .await
            .map_err(|error| format!("bind send: {error}"))?;
        let response = DceRpcCoResponsePkt::try_from(response.as_ref())
            .map_err(|error| format!("bind response: {error}"))?;
        let bind_ack = match response.content() {
            DcRpcCoPktResponseContent::BindAck(ack) => ack,
            other => return Err(format!("expected BindAck, got {other:?}")),
        };
        let accepted = bind_ack.results.iter().any(|result| {
            result.result == DceRpcCoPktBindAckDefResult::Acceptance
                && result.syntax.uuid == ndr32.uuid
                && result.syntax.version == ndr32.version
        });
        if !accepted {
            return Err(format!("NDR32 bind rejected: {:?}", bind_ack.results));
        }

        let stub = Self::netr_share_enum0_request(server);
        let call = DceRpcCoRequestPkt::new(
            DcRpcCoPktRequest {
                alloc_hint: 0,
                context_id: 0,
                opnum: 0x0f,
                stub_data: stub,
            }
            .into(),
            3,
            DceRpcCoPktFlags::new()
                .with_first_frag(true)
                .with_last_frag(true),
            0x10,
        );
        let call_data: Vec<u8> = call
            .try_into()
            .map_err(|error| format!("share enum encode: {error}"))?;
        let response = pipe
            .fsctl_with_options(
                PipeTransceiveRequest::from(IoctlBuffer::from(call_data)),
                65536,
            )
            .await
            .map_err(|error| format!("share enum send: {error}"))?;
        let _ = pipe.close().await;
        let response = DceRpcCoResponsePkt::try_from(response.as_ref())
            .map_err(|error| format!("share enum response: {error}"))?;
        let body = match response.content() {
            DcRpcCoPktResponseContent::Response(reply) => &reply.stub_data,
            other => return Err(format!("expected share enum response, got {other:?}")),
        };
        Self::parse_share_enum0(body)
    }

    fn netr_share_enum0_request(server: &str) -> Vec<u8> {
        let mut out = Vec::with_capacity(96);
        let server = server.trim();
        if server.is_empty() {
            out.extend_from_slice(&0u32.to_le_bytes());
        } else {
            let unc = if server.starts_with("\\\\") {
                server.to_string()
            } else {
                format!("\\\\{server}")
            };
            let units: Vec<u16> = unc.encode_utf16().chain(std::iter::once(0)).collect();
            out.extend_from_slice(&0x4bbau32.to_le_bytes());
            out.extend_from_slice(&(units.len() as u32).to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
            out.extend_from_slice(&(units.len() as u32).to_le_bytes());
            for unit in &units {
                out.extend_from_slice(&unit.to_le_bytes());
            }
            while out.len() % 4 != 0 {
                out.push(0xab);
            }
        }
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0x4bacu32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&u32::MAX.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out
    }

    fn parse_share_enum0(stub: &[u8]) -> Result<Vec<String>, String> {
        fn u32_at(data: &[u8], offset: usize) -> Option<u32> {
            let bytes = data.get(offset..offset + 4)?;
            Some(u32::from_le_bytes(bytes.try_into().ok()?))
        }

        if u32_at(stub, 0) != Some(0) || u32_at(stub, 4) != Some(0) {
            return Err("unexpected NetrShareEnum level".to_string());
        }
        let entries = u32_at(stub, 12).unwrap_or(0) as usize;
        if entries == 0 {
            return Ok(Vec::new());
        }
        let max_count = u32_at(stub, 20).unwrap_or(0) as usize;
        let count = entries.min(max_count);
        let entries_offset: usize = 24;
        let mut strings_offset = entries_offset
            .checked_add(count.saturating_mul(4))
            .ok_or_else(|| "share array overflow".to_string())?;
        let mut names = Vec::with_capacity(count);
        for index in 0..count {
            let pointer = u32_at(stub, entries_offset + index * 4).unwrap_or(0);
            if pointer == 0 {
                continue;
            }
            let (name, next) = Self::read_ndr32_string(stub, strings_offset)?;
            strings_offset = next;
            names.push(name);
        }
        Ok(names)
    }

    fn read_ndr32_string(stub: &[u8], offset: usize) -> Result<(String, usize), String> {
        fn u32_at(data: &[u8], offset: usize) -> Option<u32> {
            let bytes = data.get(offset..offset + 4)?;
            Some(u32::from_le_bytes(bytes.try_into().ok()?))
        }

        let actual = u32_at(stub, offset + 8).ok_or_else(|| "short NDR string".to_string())?;
        let start = offset
            .checked_add(12)
            .and_then(|value| value.checked_add(u32_at(stub, offset + 4).unwrap_or(0) as usize))
            .ok_or_else(|| "NDR string offset overflow".to_string())?;
        let byte_len = (actual as usize)
            .checked_mul(2)
            .ok_or_else(|| "NDR string length overflow".to_string())?;
        let bytes = stub
            .get(start..start + byte_len)
            .ok_or_else(|| "truncated NDR string".to_string())?;
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .take_while(|unit| *unit != 0)
            .collect();
        let next = (start + byte_len + 3) & !3;
        Ok((String::from_utf16_lossy(&units), next))
    }

    pub async fn test_connection(cfg: &SmbConfig, password: &str) -> AppResult<()> {
        cfg.validate()?;
        if cfg.share.trim().is_empty() {
            Self::list_shares(cfg, &cfg.normalized_host(), password).await?;
            return Ok(());
        }
        let mut client_config = ClientConfig::default();
        client_config.connection.port = Some(cfg.port);
        let client = Client::new(client_config);
        let host = cfg.normalized_host();
        let share = UncPath::from_str(&format!(
            r"\\{}\{}",
            host,
            cfg.share.trim()
        ))?;
        client
            .share_connect(&share, &cfg.auth_user(), password.to_string())
            .await
            .map_err(|error| {
                AppError::msg(format!(
                    "SMB {host}:{} (share {}): {error}. Проверьте LAN-IP, порт 445 и доступность NAS.",
                    cfg.port,
                    cfg.share.trim()
                ))
            })?;
        let root = if cfg.normalized_base().is_empty() {
            share
        } else {
            share.with_path(&cfg.normalized_base())
        };
        let resource = client
            .create_file(
                &root,
                &FileCreateArgs::make_open_existing(
                    DirAccessMask::new()
                        .with_list_directory(true)
                        .with_synchronize(true)
                        .into(),
                ),
            )
            .await?;
        match resource {
            Resource::Directory(dir) => {
                dir.close().await?;
            }
            other => {
                let _ = other;
                return Err(AppError::msg("Указанный путь не является каталогом"));
            }
        }
        let _ = client.close().await;
        Ok(())
    }

    pub async fn list_dir(
        &self,
        client: &Client,
        base: &str,
        relative: &str,
    ) -> AppResult<Vec<FileEntry>> {
        let share = self.share().await?;
        let rel = join_rel(base, relative);
        let target = if rel.is_empty() {
            share.with_no_path()
        } else {
            share.with_path(&rel)
        };
        let resource = client
            .create_file(
                &target,
                &FileCreateArgs::make_open_existing(
                    DirAccessMask::new()
                        .with_list_directory(true)
                        .with_synchronize(true)
                        .into(),
                ),
            )
            .await?;
        let dir: Directory = resource
            .try_into()
            .map_err(|(e, _)| AppError::Smb(e))?;
        let dir = Arc::new(dir);
        let mut stream = Directory::query::<FileIdBothDirectoryInformation>(&dir, "*").await?;
        let mut out = Vec::new();
        while let Some(item) = stream.next().await {
            let item = item?;
            let name = item.file_name.to_string();
            if name == "." || name == ".." || name.is_empty() {
                continue;
            }
            let is_dir = item.file_attributes.directory();
            let path = join_rel(&rel, &name);
            let modified: SystemTime = item.last_write_time.into();
            out.push(FileEntry {
                extension: name
                    .rsplit_once('.')
                    .map(|(_, ext)| ext.to_ascii_lowercase())
                    .unwrap_or_default(),
                name,
                path,
                is_dir,
                is_share: false,
                size: item.end_of_file,
                modified: modified
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0),
            });
        }
        dir.close().await?;
        out.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(out)
    }

    pub async fn list_tree(
        &self,
        client: &Client,
        base: &str,
        relative: &str,
    ) -> AppResult<HashMap<String, RemoteStat>> {
        let mut out = HashMap::new();
        let mut queue = VecDeque::new();
        queue.push_back(join_rel(base, relative));
        while let Some(dir) = queue.pop_front() {
            let entries = match self.list_dir(client, "", &dir).await {
                Ok(entries) => entries,
                Err(e) => {
                    if out.is_empty() && dir == join_rel(base, relative) {
                        return Err(e);
                    }
                    continue;
                }
            };
            for entry in entries {
                if entry.is_dir {
                    queue.push_back(entry.path);
                } else {
                    out.insert(
                        entry.path,
                        RemoteStat {
                            size: entry.size,
                            modified: entry.modified,
                        },
                    );
                }
            }
        }
        Ok(out)
    }

    pub async fn read_range(
        &self,
        client: &Client,
        base: &str,
        relative: &str,
        offset: u64,
        length: usize,
    ) -> AppResult<Vec<u8>> {
        let share = self.share().await?;
        let rel = join_rel(base, relative);
        let target = share.with_path(&rel);
        let resource = client
            .create_file(
                &target,
                &FileCreateArgs::make_open_existing(
                    FileAccessMask::new().with_generic_read(true),
                ),
            )
            .await?;
        let file: smb::File = resource
            .try_into()
            .map_err(|(e, _)| AppError::Smb(e))?;
        let file_len = file.get_len().await?;
        if offset >= file_len {
            file.close().await?;
            return Ok(Vec::new());
        }
        let wanted = (length as u64).min(file_len - offset) as usize;
        let mut data = vec![0u8; wanted];
        let mut done = 0usize;
        const READ_CHUNK: usize = 1024 * 1024;
        while done < wanted {
            let take = (wanted - done).min(READ_CHUNK);
            let read = file
                .read_at(&mut data[done..done + take], offset + done as u64)
                .await?;
            if read == 0 {
                break;
            }
            done += read;
        }
        data.truncate(done);
        file.close().await?;
        Ok(data)
    }

    pub async fn file_len(
        &self,
        client: &Client,
        base: &str,
        relative: &str,
    ) -> AppResult<u64> {
        let share = self.share().await?;
        let rel = join_rel(base, relative);
        let target = share.with_path(&rel);
        let resource = client
            .create_file(
                &target,
                &FileCreateArgs::make_open_existing(
                    FileAccessMask::new().with_generic_read(true),
                ),
            )
            .await?;
        let file: smb::File = resource
            .try_into()
            .map_err(|(e, _)| AppError::Smb(e))?;
        let len = file.get_len().await?;
        file.close().await?;
        Ok(len)
    }

    pub async fn upload_file(
        &self,
        client: &Client,
        remote_path: &str,
        local_path: &std::path::Path,
    ) -> AppResult<u64> {
        self.upload_file_with_time(client, remote_path, local_path, None)
            .await
    }

    pub async fn upload_file_with_progress(
        &self,
        client: &Client,
        remote_path: &str,
        local_path: &std::path::Path,
        progress: &(dyn Fn(u64, u64) + Send + Sync),
    ) -> AppResult<u64> {
        self.upload_file_with_time_progress(client, remote_path, local_path, None, progress)
            .await
    }

    pub async fn upload_file_with_time(
        &self,
        client: &Client,
        remote_path: &str,
        local_path: &std::path::Path,
        modified: Option<i64>,
    ) -> AppResult<u64> {
        self.upload_file_with_time_progress(client, remote_path, local_path, modified, &|_, _| {})
            .await
    }

    async fn upload_file_with_time_progress(
        &self,
        client: &Client,
        remote_path: &str,
        local_path: &std::path::Path,
        modified: Option<i64>,
        progress: &(dyn Fn(u64, u64) + Send + Sync),
    ) -> AppResult<u64> {
        let total = tokio::fs::metadata(local_path).await?.len();
        progress(0, total);
        let share = self.share().await?;
        self.ensure_parent(client, &share, remote_path).await?;
        let target = share.with_path(remote_path);
        let resource = client
            .create_file(
                &target,
                &FileCreateArgs {
                    disposition: CreateDisposition::OverwriteIf,
                    attributes: FileAttributes::new().with_archive(true),
                    options: CreateOptions::new(),
                    desired_access: FileAccessMask::new()
                        .with_file_read_data(true)
                        .with_file_write_data(true)
                        .with_file_append_data(true)
                        .with_file_read_attributes(true)
                        .with_file_write_attributes(true),
                },
            )
            .await?;
        let file: smb::File = resource
            .try_into()
            .map_err(|(e, _)| AppError::Smb(e))?;
        let mut local = tokio::fs::File::open(local_path).await?;
        let mut buffer = vec![0u8; 1024 * 1024];
        let mut offset = 0u64;
        loop {
            let read = local.read(&mut buffer).await?;
            if read == 0 {
                break;
            }
            let mut written = 0usize;
            while written < read {
                let n = file
                    .write_at(&buffer[written..read], offset + written as u64)
                    .await?;
                if n == 0 {
                    return Err(AppError::msg("SMB: сервер не принял данные файла"));
                }
                written += n;
            }
            offset += read as u64;
            progress(offset, total);
        }
        file.flush().await?;
        if let Some(seconds) = modified.filter(|value| *value > 0) {
            let filetime = (seconds as u64 + 11_644_473_600) * 10_000_000;
            let info = FileBasicInformation {
                creation_time: FileTime::from(filetime),
                last_access_time: FileTime::from(filetime),
                last_write_time: FileTime::from(filetime),
                change_time: FileTime::from(filetime),
                file_attributes: FileAttributes::new().with_archive(true),
            };
            let _ = file.set_info(info).await;
        }
        file.close().await?;
        Ok(offset)
    }

    pub async fn create_dir_path(&self, client: &Client, path: &str) -> AppResult<()> {
        let share = self.share().await?;
        let normalized = join_rel("", path);
        if normalized.is_empty() {
            return Err(AppError::msg("Пустое имя папки"));
        }
        self.ensure_parent(client, &share, &format!("{normalized}/placeholder"))
            .await?;
        self.ensure_dir(client, &share, &normalized).await
    }
    pub async fn delete_path(&self, client: &Client, path: &str) -> AppResult<()> {
        let share = self.share().await?;
        let target = share.with_path(path);
        let access = FileAccessMask::new().with_delete(true);
        let directory_args = FileCreateArgs {
            disposition: CreateDisposition::Open,
            attributes: FileAttributes::new(),
            options: CreateOptions::new().with_directory_file(true),
            desired_access: access,
        };
        match client.create_file(&target, &directory_args).await {
            Ok(Resource::Directory(dir)) => {
                let info = FileDispositionInformation {
                    delete_pending: true.into(),
                };
                dir.set_info(info).await?;
                let _ = dir.close().await;
                return Ok(());
            }
            Ok(_) => {}
            Err(_) => {}
        }

        let resource = client
            .create_file(&target, &FileCreateArgs::make_open_existing(access))
            .await?;
        let info = FileDispositionInformation {
            delete_pending: true.into(),
        };
        match resource {
            Resource::File(file) => {
                file.set_info(info).await?;
                let _ = file.close().await;
            }
            Resource::Directory(dir) => {
                dir.set_info(info).await?;
                let _ = dir.close().await;
            }
            Resource::Pipe(_) => {
                return Err(AppError::msg("SMB: нельзя удалить pipe"));
            }
        }
        Ok(())
    }

    pub async fn delete_tree(&self, client: &Client, path: &str) -> AppResult<()> {
        let mut queue = VecDeque::from([path.to_string()]);
        let mut folders = Vec::new();
        let mut files = Vec::new();
        let mut root_is_dir = false;
        while let Some(dir) = queue.pop_front() {
            match self.list_dir(client, "", &dir).await {
                Ok(entries) => {
                    if dir == path {
                        root_is_dir = true;
                    }
                    for entry in entries {
                        if entry.is_dir {
                            folders.push(entry.path.clone());
                            queue.push_back(entry.path);
                        } else {
                            files.push(entry.path);
                        }
                    }
                }
                Err(error) => {
                    if dir == path && !root_is_dir {
                        return self.delete_path(client, path).await;
                    }
                    return Err(error);
                }
            }
        }
        for file in files {
            self.delete_path(client, &file).await?;
        }
        for folder in folders.into_iter().rev() {
            self.delete_path(client, &folder).await?;
        }
        self.delete_path(client, path).await
    }
    pub async fn download_file(
        &self,
        client: &Client,
        remote_path: &str,
        local_path: &std::path::Path,
    ) -> AppResult<u64> {
        self.download_file_with_progress(client, remote_path, local_path, &|_, _| {})
            .await
    }

    pub async fn download_file_with_progress(
        &self,
        client: &Client,
        remote_path: &str,
        local_path: &std::path::Path,
        progress: &(dyn Fn(u64, u64) + Send + Sync),
    ) -> AppResult<u64> {
        let share = self.share().await?;
        let target = share.with_path(remote_path);
        let resource = client
            .create_file(
                &target,
                &FileCreateArgs::make_open_existing(
                    FileAccessMask::new().with_generic_read(true),
                ),
            )
            .await?;
        let file: smb::File = resource
            .try_into()
            .map_err(|(e, _)| AppError::Smb(e))?;
        let len = file.get_len().await?;
        progress(0, len);
        if let Some(parent) = local_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let mut local = tokio::fs::File::create(local_path).await?;
        let mut buffer = vec![0u8; 1024 * 1024];
        let mut offset = 0u64;
        while offset < len {
            let wanted = ((len - offset) as usize).min(buffer.len());
            let read = file.read_at(&mut buffer[..wanted], offset).await?;
            if read == 0 {
                break;
            }
            local.write_all(&buffer[..read]).await?;
            offset += read as u64;
            progress(offset, len);
        }
        local.flush().await?;
        file.close().await?;
        Ok(offset)
    }
    async fn ensure_parent(
        &self,
        client: &Client,
        share: &UncPath,
        remote_path: &str,
    ) -> AppResult<()> {
        let mut parts: Vec<&str> = remote_path
            .split(['/', '\\'])
            .filter(|part| !part.is_empty())
            .collect();
        parts.pop();
        let mut prefix = String::new();
        for part in parts {
            prefix = join_rel(&prefix, part);
            self.ensure_dir(client, share, &prefix).await?;
        }
        Ok(())
    }

    async fn ensure_dir(&self, client: &Client, share: &UncPath, path: &str) -> AppResult<()> {
        let target = share.clone().with_path(path);
        let open_args = FileCreateArgs::make_open_existing(
            DirAccessMask::new()
                .with_list_directory(true)
                .with_synchronize(true)
                .into(),
        );
        if let Ok(resource) = client.create_file(&target, &open_args).await {
            match resource {
                Resource::Directory(dir) => {
                    let _ = dir.close().await;
                    return Ok(());
                }
                _ => return Err(AppError::msg(format!("SMB: {path} не является каталогом"))),
            }
        }
        let create_args = FileCreateArgs::make_create_new(
            FileAttributes::new().with_directory(true),
            CreateOptions::new().with_directory_file(true),
        );
        match client.create_file(&target, &create_args).await {
            Ok(Resource::Directory(dir)) => {
                let _ = dir.close().await;
                Ok(())
            }
            Ok(_) => Ok(()),
            Err(e) => Err(AppError::Smb(e)),
        }
    }
}

pub fn join_rel(base: &str, relative: &str) -> String {
    let base = base.replace('\\', "/");
    let relative = relative.replace('\\', "/");
    let joined = format!(
        "{}/{}",
        base.trim_matches('/'),
        relative.trim_matches('/')
    );
    normalize_rel(&joined)
}

pub fn normalize_rel(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    let mut parts: Vec<&str> = Vec::new();
    for part in normalized.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

pub fn file_kind(name: &str) -> &'static str {
    let ext = name
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "jpg" | "jpeg" | "png" | "webp" | "bmp" | "avif" | "heic" | "heif" | "svg" => "image",
        "gif" => "gif",
        "mp4" | "m4v" | "webm" | "mkv" | "mov" | "avi" | "3gp" | "ogv" => "video",
        _ => "file",
    }
}

#[cfg(test)]
mod share_enum_tests {
    use super::SmbService;

    fn hex(value: &str) -> Vec<u8> {
        value
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect()
    }

    #[test]
    fn parses_ndr32_level0_response() {
        let stub = hex(
            "0000000000000000c4a0000002000000722f0000020000007bbc000006ea0000\
             0400000000000000040000006e00610073000000060000000000000006000000\
             6d0065006400690061000000020000000000000000000000",
        );
        let names = SmbService::parse_share_enum0(&stub).unwrap();
        assert_eq!(names, vec!["nas", "media"]);
    }
}
