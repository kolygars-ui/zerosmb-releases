use crate::config::SmbConfig;
use crate::error::{AppError, AppResult};
use futures_util::StreamExt;
use serde::Serialize;
use smb::{
    binrw_util::file_time::FileTime, Client, ClientConfig, CreateDisposition, CreateOptions,
    DirAccessMask, Directory, FileAccessMask, FileAttributes, FileBasicInformation, FileCreateArgs,
    FileDispositionInformation, FileIdBothDirectoryInformation, GetLen, ReadAt, Resource, UncPath,
    WriteAt,
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

    pub async fn test_connection(cfg: &SmbConfig, password: &str) -> AppResult<()> {
        cfg.validate()?;
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
        while done < wanted {
            let read = file.read_at(&mut data[done..], offset + done as u64).await?;
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
