use crate::android_bridge;
use crate::config::{SyncJob, SyncSource};
use crate::error::{AppError, AppResult};
use crate::smb_service::{join_rel, RemoteStat};
use crate::state::AppState;
use crate::zt_proxy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncProgress {
    pub phase: String,
    pub current: usize,
    pub total: usize,
    pub file: String,
    pub bytes: u64,
    pub skipped: usize,
    pub failed: usize,
    pub message: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncSummary {
    pub uploaded: usize,
    pub downloaded: usize,
    pub deleted: usize,
    pub skipped: usize,
    pub failed: usize,
    pub bytes: u64,
    pub errors: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TreeListResponse {
    #[serde(default)]
    entries: Vec<TreeEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TreeEntry {
    relative_path: String,
    uri: String,
    name: String,
    size: u64,
    last_modified: i64,
    is_directory: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportedFile {
    path: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GalleryListResponse {
    #[serde(default)]
    items: Vec<GalleryItem>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryItem {
    pub uri: String,
    pub name: String,
    pub mime: String,
    pub size: u64,
    pub date_modified: i64,
    #[serde(default)]
    pub date_created: i64,
    #[serde(default)]
    pub relative_path: String,
    #[serde(default)]
    pub bucket: String,
}

#[derive(Clone)]
struct LocalFile {
    relative: String,
    path: PathBuf,
    size: u64,
    modified: i64,
}

#[derive(Clone, Copy, Serialize, Deserialize, Default)]
struct FileStamp {
    size: u64,
    modified: i64,
}

impl FileStamp {
    fn matches(self, size: u64, modified: i64) -> bool {
        self.size == size && (self.modified - modified).abs() <= 2
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
struct SyncState {
    local: HashMap<String, FileStamp>,
    remote: HashMap<String, FileStamp>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SyncMode {
    UploadManual,
    DownloadManual,
    UploadAuto,
    DownloadAuto,
    MirrorAuto,
}

impl SyncMode {
    fn parse(value: &str) -> Self {
        match value {
            "download_manual" => Self::DownloadManual,
            "upload_auto" => Self::UploadAuto,
            "download_auto" => Self::DownloadAuto,
            "mirror_auto" => Self::MirrorAuto,
            _ => Self::UploadManual,
        }
    }

    fn is_auto(self) -> bool {
        matches!(self, Self::UploadAuto | Self::DownloadAuto | Self::MirrorAuto)
    }
}

pub fn normalize_mode(value: Option<&str>) -> String {
    match value {
        Some("download_manual") => "download_manual",
        Some("upload_auto") => "upload_auto",
        Some("download_auto") => "download_auto",
        Some("mirror_auto") => "mirror_auto",
        _ => "upload_manual",
    }
    .to_string()
}

enum SyncAction {
    Upload(LocalFile),
    Download(String, RemoteStat),
    DeleteLocal(String),
    DeleteRemote(String),
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

pub async fn run_job(
    app: &AppHandle,
    state: &AppState,
    job: &SyncJob,
    channel: &Channel<SyncProgress>,
) -> AppResult<SyncSummary> {
    let _guard = state.inner.sync_lock.lock().await;
    let mode = SyncMode::parse(&job.mode);
    let result = match &job.source {
        SyncSource::LocalPath(path) => run_local(app, state, path, job, channel).await,
        SyncSource::AndroidTree(uri) => {
            if matches!(
                mode,
                SyncMode::DownloadManual | SyncMode::DownloadAuto | SyncMode::MirrorAuto
            ) {
                Err(AppError::msg(
                    "Синхронизация из NAS в локальную папку на Android пока не поддерживается",
                ))
            } else {
                run_android_tree(app, state, uri, &job.remote_path, channel).await
            }
        }
    };
    match &result {
        Ok(summary) => crate::integration::notify(
            app,
            if summary.failed > 0 { "error" } else { "sync" },
            "Синхронизация ZeroSMB",
            &format!(
                "{}: загружено {}, скачано {}, удалено {}, ошибок {}",
                job.label, summary.uploaded, summary.downloaded, summary.deleted, summary.failed
            ),
        ),
        Err(error) => crate::integration::notify(
            app,
            "error",
            "Ошибка синхронизации",
            &format!("{}: {error}", job.label),
        ),
    }
    mark_job_run(state, &job.id);
    result
}

fn mark_job_run(state: &AppState, id: &str) {
    let _ = state.update_config(|cfg| {
        if let Some(job) = cfg.sync_jobs.iter_mut().find(|job| job.id == id) {
            job.last_sync = now_secs();
        }
    });
}

pub fn start_scheduler(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(20)).await;
            let state = app.state::<AppState>().inner().clone();
            let Some(path) = state.inner.data_dir.read().ok().and_then(|dir| dir.clone()) else {
                continue;
            };
            if !path.exists() {
                continue;
            }
            let cfg = state.config();
            if !cfg.setup_complete {
                continue;
            }
            let now = now_secs();
            for job in cfg.sync_jobs.clone() {
                let mode = SyncMode::parse(&job.mode);
                if !mode.is_auto() {
                    continue;
                }
                let interval = job.interval_minutes.max(1) as i64 * 60;
                if job.last_sync > 0 && now.saturating_sub(job.last_sync) < interval {
                    continue;
                }
                let channel = Channel::<SyncProgress>::new(|_| Ok(()));
                if let Err(error) = run_job(&app, &state, &job, &channel).await {
                    log::warn!("sync job {} failed: {error}", job.id);
                }
            }
        }
    });
}

pub async fn run_gallery(
    app: &AppHandle,
    state: &AppState,
    remote_path: &str,
    selection: Option<Vec<GalleryItem>>,
    conflict_policy: &str,
    channel: &Channel<SyncProgress>,
) -> AppResult<SyncSummary> {
    let cfg = state.config();
    let password = state.password()?;
    let effective =
        zt_proxy::effective_smb_config(app, &cfg.smb, &cfg.zerotier_network_id).await?;
    let client = state.inner.smb.connect(&effective, &password).await?;
    let base = cfg.smb.normalized_base();
    let remote_base = join_rel(&base, remote_path);
    state
        .inner
        .smb
        .create_dir_path(&client, &remote_base)
        .await?;
    let mut remote = state
        .inner
        .smb
        .list_tree(&client, &base, remote_path)
        .await
        .unwrap_or_default();

    let items: Vec<GalleryItem> = if let Some(selection) = selection {
        selection
            .into_iter()
            .filter(|item| item.mime.starts_with("image/") || item.mime.starts_with("video/"))
            .collect()
    } else {
        let payload = serde_json::json!({});
        let response: GalleryListResponse =
            android_bridge::call(app, "listMedia", payload).await?;
        response
            .items
            .into_iter()
            .filter(|item| item.mime.starts_with("image/") || item.mime.starts_with("video/"))
            .collect()
    };

    let mut summary = SyncSummary::default();
    let total = items.len();
    crate::android_progress::update(
        app,
        "Загрузка галереи на NAS",
        &format!("Файлов: {total}"),
        0,
        total,
        false,
    )
    .await;
    for (index, item) in items.iter().enumerate() {
        let folder = gallery_folder(item);
        let name = safe_filename(&item.name);
        let target = join_rel(&remote_base, &format!("{folder}/{name}"));
        let created = if item.date_created > 0 {
            item.date_created
        } else {
            item.date_modified
        };
        let mut target = target;
        if remote.contains_key(&target) {
            if conflict_policy == "rename" {
                target = unique_remote_target(&remote, &target);
            } else {
                summary.skipped += 1;
                send_progress(
                    channel,
                    SyncProgress {
                        phase: "gallery".to_string(),
                        current: index + 1,
                        total,
                        file: target,
                        bytes: summary.bytes,
                        skipped: summary.skipped,
                        failed: summary.failed,
                        message: Some("Файл уже существует — пропущен".to_string()),
                    },
                );
                crate::android_progress::update(
                    app,
                    "Загрузка галереи на NAS",
                    "Пропуск существующих файлов",
                    index + 1,
                    total,
                    false,
                )
                .await;
                continue;
            }
        }

        let result = async {
            let exported: ExportedFile = android_bridge::call(
                app,
                "exportMedia",
                serde_json::json!({ "uri": item.uri, "name": item.name }),
            )
            .await?;
            let local = PathBuf::from(&exported.path);
            let bytes = state
                .inner
                .smb
                .upload_file_with_time(&client, &target, &local, Some(created))
                .await?;
            let _ = tokio::fs::remove_file(&local).await;
            Ok::<u64, AppError>(bytes)
        }
        .await;

        match result {
            Ok(bytes) => {
                summary.uploaded += 1;
                summary.bytes += bytes;
                remote.insert(
                    target.clone(),
                    RemoteStat {
                        size: item.size,
                        modified: created,
                    },
                );
            }
            Err(error) => {
                summary.failed += 1;
                summary.errors.push(format!("{}: {error}", item.name));
            }
        }
        send_progress(
            channel,
            SyncProgress {
                phase: "gallery".to_string(),
                current: index + 1,
                total,
                file: target,
                bytes: summary.bytes,
                skipped: summary.skipped,
                failed: summary.failed,
                message: summary.errors.last().cloned(),
            },
        );
        crate::android_progress::update(
            app,
            "Загрузка галереи на NAS",
            &format!("Загружено: {} из {total}", index + 1),
            index + 1,
            total,
            false,
        )
        .await;
    }

    crate::android_progress::update(
        app,
        "Загрузка галереи завершена",
        &format!("Загружено: {} из {total}", summary.uploaded),
        total,
        total,
        true,
    )
    .await;
    send_progress(
        channel,
        SyncProgress {
            phase: "done".to_string(),
            current: total,
            total,
            file: String::new(),
            bytes: summary.bytes,
            skipped: summary.skipped,
            failed: summary.failed,
            message: None,
        },
    );
    Ok(summary)
}

async fn run_local(
    app: &AppHandle,
    state: &AppState,
    local_path: &str,
    job: &SyncJob,
    channel: &Channel<SyncProgress>,
) -> AppResult<SyncSummary> {
    let root = PathBuf::from(local_path);
    if !root.is_dir() {
        return Err(AppError::msg(format!(
            "Локальная папка не найдена: {local_path}"
        )));
    }
    let cfg = state.config();
    let password = state.password()?;
    let effective =
        zt_proxy::effective_smb_config(app, &cfg.smb, &cfg.zerotier_network_id).await?;
    let client = state.inner.smb.connect(&effective, &password).await?;
    let base = cfg.smb.normalized_base();
    let remote_base = join_rel(&base, &job.remote_path);
    let remote = load_remote_map(state, &client, &base, &job.remote_path).await?;
    let files = collect_local_files(root.clone()).await?;
    let local: HashMap<String, LocalFile> = files
        .into_iter()
        .map(|file| (file.relative.clone(), file))
        .collect();
    let previous = load_sync_state(state, &job.id);
    let mode = SyncMode::parse(&job.mode);
    let (actions, skipped) = plan_actions(mode, job.delete_enabled, &local, &remote, &previous);
    let total = actions.len();
    let mut summary = SyncSummary {
        skipped,
        ..Default::default()
    };
    crate::android_progress::update(
        app,
        "Синхронизация с NAS",
        &job.label,
        0,
        total,
        false,
    )
    .await;

    for (index, action) in actions.into_iter().enumerate() {
        let (phase, label) = match &action {
            SyncAction::Upload(file) => ("upload", file.relative.clone()),
            SyncAction::Download(relative, _) => ("download", relative.clone()),
            SyncAction::DeleteLocal(relative) => ("delete", relative.clone()),
            SyncAction::DeleteRemote(relative) => ("delete", relative.clone()),
        };
        let outcome: AppResult<()> = async {
            match &action {
            SyncAction::Upload(file) => {
                let target = join_rel(&remote_base, &file.relative);
                let bytes = state
                    .inner
                    .smb
                    .upload_file_with_time(&client, &target, &file.path, Some(file.modified))
                    .await?;
                summary.uploaded += 1;
                summary.bytes += bytes;
                Ok(())
            }
            SyncAction::Download(relative, remote_stat) => {
                let target = local_target(&root, relative);
                let bytes = state
                    .inner
                    .smb
                    .download_file(&client, &join_rel(&remote_base, relative), &target)
                    .await?;
                if let Err(error) = set_local_modified(&target, remote_stat.modified) {
                    log::warn!("failed to set mtime for {}: {error}", target.display());
                }
                summary.downloaded += 1;
                summary.bytes += bytes;
                Ok(())
            }
            SyncAction::DeleteLocal(relative) => {
                let target = local_target(&root, relative);
                tokio::fs::remove_file(&target).await?;
                summary.deleted += 1;
                Ok(())
            }
            SyncAction::DeleteRemote(relative) => {
                state
                    .inner
                    .smb
                    .delete_path(&client, &join_rel(&remote_base, relative))
                    .await?;
                summary.deleted += 1;
                Ok(())
            }
            }
        }
        .await;
        if let Err(error) = outcome {
            summary.failed += 1;
            summary.errors.push(format!("{label}: {error}"));
        }
        send_progress(
            channel,
            SyncProgress {
                phase: phase.to_string(),
                current: index + 1,
                total,
                file: label,
                bytes: summary.bytes,
                skipped: summary.skipped,
                failed: summary.failed,
                message: summary.errors.last().cloned(),
            },
        );
        crate::android_progress::update(
            app,
            "Синхронизация с NAS",
            &format!("Готово: {} из {total}", index + 1),
            index + 1,
            total,
            false,
        )
        .await;
    }

    if summary.failed == 0 {
        if let Ok(local_after) = collect_local_files(root.clone()).await {
            if let Ok(remote_after) =
                load_remote_map(state, &client, &base, &job.remote_path).await
            {
                save_sync_state(state, &job.id, &local_after, &remote_after);
            }
        }
    }

    crate::android_progress::update(
        app,
        "Синхронизация завершена",
        &format!(
            "Загружено: {}, скачано: {}, удалено: {}",
            summary.uploaded, summary.downloaded, summary.deleted
        ),
        total,
        total,
        true,
    )
    .await;
    send_progress(
        channel,
        SyncProgress {
            phase: "done".to_string(),
            current: total,
            total,
            file: String::new(),
            bytes: summary.bytes,
            skipped: summary.skipped,
            failed: summary.failed,
            message: None,
        },
    );
    Ok(summary)
}

fn plan_actions(
    mode: SyncMode,
    delete_enabled: bool,
    local: &HashMap<String, LocalFile>,
    remote: &HashMap<String, RemoteStat>,
    previous: &SyncState,
) -> (Vec<SyncAction>, usize) {
    let mut actions = Vec::new();
    let mut skipped = 0usize;
    match mode {
        SyncMode::UploadManual | SyncMode::UploadAuto => {
            for (relative, file) in local {
                match remote.get(relative) {
                    Some(stat) if same_file(file.size, file.modified, stat) => skipped += 1,
                    _ => actions.push(SyncAction::Upload(file.clone())),
                }
            }
            if delete_enabled {
                for relative in remote.keys() {
                    if !local.contains_key(relative) && previous.local.contains_key(relative) {
                        actions.push(SyncAction::DeleteRemote(relative.clone()));
                    }
                }
            }
        }
        SyncMode::DownloadManual | SyncMode::DownloadAuto => {
            for (relative, stat) in remote {
                match local.get(relative) {
                    Some(file) if same_file(file.size, file.modified, stat) => skipped += 1,
                    _ => actions.push(SyncAction::Download(relative.clone(), stat.clone())),
                }
            }
            if delete_enabled {
                for relative in local.keys() {
                    if !remote.contains_key(relative) && previous.remote.contains_key(relative) {
                        actions.push(SyncAction::DeleteLocal(relative.clone()));
                    }
                }
            }
        }
        SyncMode::MirrorAuto => {
            let mut paths: Vec<String> = local.keys().chain(remote.keys()).cloned().collect();
            paths.sort();
            paths.dedup();
            for relative in paths {
                match (local.get(&relative), remote.get(&relative)) {
                    (Some(file), Some(stat)) => {
                        if same_file(file.size, file.modified, stat) {
                            skipped += 1;
                        } else if file.modified > stat.modified + 1 {
                            actions.push(SyncAction::Upload(file.clone()));
                        } else if stat.modified > file.modified + 1 {
                            actions.push(SyncAction::Download(relative, stat.clone()));
                        } else if file.size != stat.size {
                            actions.push(SyncAction::Upload(file.clone()));
                        } else {
                            skipped += 1;
                        }
                    }
                    (Some(file), None) => {
                        let locally_unchanged = previous
                            .local
                            .get(&relative)
                            .map(|stamp| stamp.matches(file.size, file.modified))
                            .unwrap_or(false);
                        if previous.remote.contains_key(&relative) && locally_unchanged {
                            if delete_enabled {
                                actions.push(SyncAction::DeleteLocal(relative));
                            } else {
                                skipped += 1;
                            }
                        } else {
                            actions.push(SyncAction::Upload(file.clone()));
                        }
                    }
                    (None, Some(stat)) => {
                        let remotely_unchanged = previous
                            .remote
                            .get(&relative)
                            .map(|stamp| stamp.matches(stat.size, stat.modified))
                            .unwrap_or(false);
                        if previous.local.contains_key(&relative) && remotely_unchanged {
                            if delete_enabled {
                                actions.push(SyncAction::DeleteRemote(relative));
                            } else {
                                skipped += 1;
                            }
                        } else {
                            actions.push(SyncAction::Download(relative, stat.clone()));
                        }
                    }
                    (None, None) => {}
                }
            }
        }
    }
    (actions, skipped)
}

fn same_file(size: u64, modified: i64, remote: &RemoteStat) -> bool {
    size == remote.size && (modified - remote.modified).abs() <= 2
}

fn local_target(root: &std::path::Path, relative: &str) -> PathBuf {
    relative
        .split('/')
        .filter(|part| !part.is_empty())
        .fold(root.to_path_buf(), |path, part| path.join(part))
}

fn set_local_modified(path: &std::path::Path, modified: i64) -> AppResult<()> {
    if modified <= 0 {
        return Ok(());
    }
    let file = std::fs::OpenOptions::new().write(true).open(path)?;
    let time = UNIX_EPOCH + Duration::from_secs(modified as u64);
    file.set_times(std::fs::FileTimes::new().set_modified(time))?;
    Ok(())
}

async fn load_remote_map(
    state: &AppState,
    client: &smb::Client,
    base: &str,
    relative: &str,
) -> AppResult<HashMap<String, RemoteStat>> {
    let prefix = join_rel(base, relative);
    let entries = state
        .inner
        .smb
        .list_tree(client, base, relative)
        .await
        .unwrap_or_default();
    Ok(entries
        .into_iter()
        .filter_map(|(path, stat)| {
            let relative = path
                .strip_prefix(&prefix)
                .unwrap_or(&path)
                .trim_matches(|ch| ch == '/' || ch == '\\')
                .to_string();
            (!relative.is_empty()).then_some((relative, stat))
        })
        .collect())
}

fn sync_state_path(state: &AppState, id: &str) -> AppResult<PathBuf> {
    Ok(state
        .data_dir()?
        .join("sync-state")
        .join(format!("{id}.json")))
}

fn load_sync_state(state: &AppState, id: &str) -> SyncState {
    sync_state_path(state, id)
        .ok()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|value| serde_json::from_str(&value).ok())
        .unwrap_or_default()
}

fn save_sync_state(
    state: &AppState,
    id: &str,
    local: &[LocalFile],
    remote: &HashMap<String, RemoteStat>,
) {
    let Ok(path) = sync_state_path(state, id) else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let snapshot = SyncState {
        local: local
            .iter()
            .map(|file| {
                (
                    file.relative.clone(),
                    FileStamp {
                        size: file.size,
                        modified: file.modified,
                    },
                )
            })
            .collect(),
        remote: remote
            .iter()
            .map(|(relative, stat)| {
                (
                    relative.clone(),
                    FileStamp {
                        size: stat.size,
                        modified: stat.modified,
                    },
                )
            })
            .collect(),
    };
    if let Ok(value) = serde_json::to_vec_pretty(&snapshot) {
        let _ = std::fs::write(path, value);
    }
}

async fn run_android_tree(
    app: &AppHandle,
    state: &AppState,
    tree_uri: &str,
    remote_path: &str,
    channel: &Channel<SyncProgress>,
) -> AppResult<SyncSummary> {
    let cfg = state.config();
    let password = state.password()?;
    let effective =
        zt_proxy::effective_smb_config(app, &cfg.smb, &cfg.zerotier_network_id).await?;
    let client = state.inner.smb.connect(&effective, &password).await?;
    let base = cfg.smb.normalized_base();
    let remote_base = join_rel(&base, remote_path);
    let mut remote = state
        .inner
        .smb
        .list_tree(&client, &base, remote_path)
        .await
        .unwrap_or_default();

    let response: TreeListResponse = android_bridge::call(
        app,
        "listTree",
        serde_json::json!({ "uri": tree_uri }),
    )
    .await?;
    let files: Vec<TreeEntry> = response
        .entries
        .into_iter()
        .filter(|entry| !entry.is_directory)
        .collect();
    let total = files.len();
    let mut summary = SyncSummary::default();
    crate::android_progress::update(
        app,
        "Загрузка на NAS",
        &format!("Файлов: {total}"),
        0,
        total,
        false,
    )
    .await;

    for (index, entry) in files.iter().enumerate() {
        let target = join_rel(&remote_base, &entry.relative_path);
        if up_to_date(&remote, &target, entry.size, entry.last_modified / 1000) {
            summary.skipped += 1;
            send_progress(
                channel,
                SyncProgress {
                    phase: "android".to_string(),
                    current: index + 1,
                    total,
                    file: entry.relative_path.clone(),
                    bytes: summary.bytes,
                    skipped: summary.skipped,
                    failed: summary.failed,
                    message: None,
                },
            );
            crate::android_progress::update(
                app,
                "Загрузка на NAS",
                &format!("Пропущено: {}", summary.skipped),
                index + 1,
                total,
                false,
            )
            .await;
            continue;
        }

        let result = async {
            let exported: ExportedFile = android_bridge::call(
                app,
                "exportTreeFile",
                serde_json::json!({ "uri": entry.uri, "name": entry.name }),
            )
            .await?;
            let local = PathBuf::from(&exported.path);
            let bytes = state
                .inner
                .smb
                .upload_file_with_time(&client, &target, &local, Some(entry.last_modified / 1000))
                .await?;
            let _ = tokio::fs::remove_file(&local).await;
            Ok::<u64, AppError>(bytes)
        }
        .await;
        match result {
            Ok(bytes) => {
                summary.uploaded += 1;
                summary.bytes += bytes;
            }
            Err(error) => {
                summary.failed += 1;
                summary
                    .errors
                    .push(format!("{}: {error}", entry.relative_path));
            }
        }
        send_progress(
            channel,
            SyncProgress {
                phase: "android".to_string(),
                current: index + 1,
                total,
                file: entry.relative_path.clone(),
                bytes: summary.bytes,
                skipped: summary.skipped,
                failed: summary.failed,
                message: summary.errors.last().cloned(),
            },
        );
        crate::android_progress::update(
            app,
            "Загрузка на NAS",
            &format!("Готово: {} из {total}", index + 1),
            index + 1,
            total,
            false,
        )
        .await;
    }

    crate::android_progress::update(
        app,
        "Загрузка завершена",
        &format!("Загружено: {}", summary.uploaded),
        total,
        total,
        true,
    )
    .await;
    send_progress(
        channel,
        SyncProgress {
            phase: "done".to_string(),
            current: total,
            total,
            file: String::new(),
            bytes: summary.bytes,
            skipped: summary.skipped,
            failed: summary.failed,
            message: None,
        },
    );
    Ok(summary)
}

fn gallery_folder(item: &GalleryItem) -> String {
    let raw = if item.relative_path.trim().is_empty() {
        item.bucket.trim()
    } else {
        item.relative_path.trim()
    };
    let mut parts: Vec<String> = raw
        .replace('\\', "/")
        .split('/')
        .map(str::trim)
        .filter(|part| !part.is_empty() && *part != "." && *part != "..")
        .map(safe_filename)
        .collect();
    if parts.len() > 1 {
        let first = parts[0].to_lowercase();
        if [
            "dcim",
            "pictures",
            "download",
            "downloads",
            "movies",
            "music",
            "documents",
        ]
        .contains(&first.as_str())
        {
            parts.remove(0);
        }
    }
    if parts.is_empty() {
        let fallback = safe_filename(item.bucket.trim());
        if fallback.trim().is_empty() {
            "Без альбома".to_string()
        } else {
            fallback
        }
    } else {
        parts.join("/")
    }
}
fn unique_remote_target(remote: &HashMap<String, RemoteStat>, target: &str) -> String {
    let (parent, file) = target.rsplit_once('/').unwrap_or(("", target));
    let (stem, extension) = file
        .rsplit_once('.')
        .map(|(stem, extension)| (stem, format!(".{extension}")))
        .unwrap_or((file, String::new()));
    for index in 1..10_000 {
        let candidate = if parent.is_empty() {
            format!("{stem} ({index}){extension}")
        } else {
            format!("{parent}/{stem} ({index}){extension}")
        };
        if !remote.contains_key(&candidate) {
            return candidate;
        }
    }
    target.to_string()
}
fn up_to_date(
    remote: &HashMap<String, RemoteStat>,
    target: &str,
    size: u64,
    modified: i64,
) -> bool {
    remote
        .get(target)
        .map(|item| item.size == size && (item.modified - modified).abs() <= 2)
        .unwrap_or(false)
}

async fn collect_local_files(root: PathBuf) -> AppResult<Vec<LocalFile>> {
    tokio::task::spawn_blocking(move || {
        let mut files = Vec::new();
        for entry in walkdir::WalkDir::new(&root)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path().to_path_buf();
            let relative = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let metadata = entry.metadata().ok();
            let size = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
            let modified = metadata
                .as_ref()
                .and_then(|m| m.modified().ok())
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|duration| duration.as_secs() as i64)
                .unwrap_or(0);
            files.push(LocalFile {
                relative,
                path,
                size,
                modified,
            });
        }
        files.sort_by(|a, b| a.relative.cmp(&b.relative));
        Ok(files)
    })
    .await
    .map_err(|error| AppError::msg(error.to_string()))?
}

fn safe_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|ch| {
            if ch == '/' || ch == '\\' || ch.is_control() {
                '_'
            } else {
                ch
            }
        })
        .collect();
    if cleaned.trim().is_empty() {
        "media".to_string()
    } else {
        cleaned
    }
}

fn month_folder(unix_seconds: i64) -> String {
    if let Ok(dt) = time::OffsetDateTime::from_unix_timestamp(unix_seconds) {
        format!("{:04}-{:02}", dt.year(), dt.month() as u8)
    } else {
        "unknown".to_string()
    }
}

fn send_progress(channel: &Channel<SyncProgress>, progress: SyncProgress) {
    let _ = channel.send(progress);
}
