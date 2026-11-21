use crate::android_bridge;
use crate::android_progress;
use crate::config::{self, AppConfig, SmbConfig, SyncJob, SyncSource};
use crate::error::{AppError, AppResult};
use crate::smb_service::{file_kind, join_rel, normalize_rel, FileEntry, SmbService};
use crate::state::AppState;
use crate::sync::{self, SyncProgress, SyncSummary};
use crate::zerotier::{self, ZtStatus};
use crate::zt_proxy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInfo {
    pub platform: String,
    pub android: bool,
    pub version: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicConfig {
    pub setup_complete: bool,
    pub zerotier_network_id: String,
    pub connection_mode: String,
    pub smb: SmbConfig,
    pub sync_jobs: Vec<SyncJob>,
    pub remote_gallery_path: String,
    pub suggested_gallery_path: String,
    pub gallery_enabled: bool,
    pub gallery_columns: u8,
    pub file_name_scale: u8,
    pub file_view_mode: String,
    pub tree_show_files: bool,
    pub tree_font_scale: u8,
    pub theme_color: String,
    pub external_documents: bool,
    pub external_save_enabled: bool,
    pub external_save_mode: String,
    pub download_dir: String,
    pub download_auto_save: bool,
    pub download_keep_structure: bool,
    pub system_install_dir: String,
    pub system_installed: bool,
    pub system_autostart: bool,
    pub system_start_menu: bool,
    pub system_desktop_shortcut: bool,
    pub system_tray: bool,
    pub system_notifications: bool,
    pub notify_downloads: bool,
    pub notify_uploads: bool,
    pub notify_sync: bool,
    pub notify_errors: bool,
    pub wireguard_config_path: String,
    pub wireguard_enabled: bool,
    pub ai_enabled: bool,
    pub ai_allow_download: bool,
    pub ai_allow_modify: bool,
    pub ai_allow_upload: bool,
    pub ai_allow_mkdir: bool,
    pub ai_allow_delete: bool,
    pub ai_deny_paths: Vec<String>,
    pub android_tree_uri: Option<String>,
    pub update_repo: String,
    pub custom_roots: bool,
    pub planet_configured: bool,
    pub has_password: bool,
    pub platform: String,
    pub android: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupInput {
    pub zerotier_network_id: String,
    pub connection_mode: Option<String>,
    pub smb: SmbConfig,
    pub password: String,
    pub remote_gallery_path: Option<String>,
    pub system_install_dir: Option<String>,
    pub system_autostart: Option<bool>,
    pub system_start_menu: Option<bool>,
    pub system_desktop_shortcut: Option<bool>,
    pub system_tray: Option<bool>,
    pub system_notifications: Option<bool>,
    pub notify_downloads: Option<bool>,
    pub notify_uploads: Option<bool>,
    pub notify_sync: Option<bool>,
    pub notify_errors: Option<bool>,
    pub download_dir: Option<String>,
    pub download_auto_save: Option<bool>,
    pub download_keep_structure: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsInput {
    pub smb: SmbConfig,
    pub password: Option<String>,
    pub remote_gallery_path: Option<String>,
    pub gallery_enabled: Option<bool>,
    pub gallery_columns: Option<u8>,
    pub file_name_scale: Option<u8>,
    pub file_view_mode: Option<String>,
    pub tree_show_files: Option<bool>,
    pub tree_font_scale: Option<u8>,
    pub theme_color: Option<String>,
    pub external_documents: Option<bool>,
    pub external_save_enabled: Option<bool>,
    pub external_save_mode: Option<String>,
    pub download_dir: Option<String>,
    pub download_auto_save: Option<bool>,
    pub download_keep_structure: Option<bool>,
    pub system_install_dir: Option<String>,
    pub system_autostart: Option<bool>,
    pub system_start_menu: Option<bool>,
    pub system_desktop_shortcut: Option<bool>,
    pub system_tray: Option<bool>,
    pub system_notifications: Option<bool>,
    pub notify_downloads: Option<bool>,
    pub notify_uploads: Option<bool>,
    pub notify_sync: Option<bool>,
    pub notify_errors: Option<bool>,
    pub wireguard_config_path: Option<String>,
    pub wireguard_enabled: Option<bool>,
    pub zerotier_network_id: Option<String>,
    pub connection_mode: Option<String>,
    pub ai_enabled: Option<bool>,
    pub ai_allow_download: Option<bool>,
    pub ai_allow_modify: Option<bool>,
    pub ai_allow_upload: Option<bool>,
    pub ai_allow_mkdir: Option<bool>,
    pub ai_allow_delete: Option<bool>,
    pub ai_deny_paths: Option<Vec<String>>,
    pub update_repo: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaRef {
    pub path: String,
    pub kind: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanetInfo {
    pub configured: bool,
    pub custom_roots: bool,
    pub size: u64,
    pub message: String,
}

#[derive(Deserialize, Serialize)]
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

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryList {
    #[serde(default)]
    pub items: Vec<GalleryItem>,
}

fn platform() -> String {
    if cfg!(target_os = "android") {
        "android".to_string()
    } else if cfg!(target_os = "windows") {
        "windows".to_string()
    } else if cfg!(target_os = "linux") {
        "linux".to_string()
    } else {
        std::env::consts::OS.to_string()
    }
}

#[cfg(not(target_os = "android"))]
async fn embedded_desktop_ensure(
    state: &AppState,
    network: &str,
    require_ready: bool,
) -> AppResult<ZtStatus> {
    let roots = state.planet_path()?;
    let storage = state.data_dir()?.join("zerotier");
    let zt = state.inner.zt.clone();
    let network = network.to_string();
    tokio::task::spawn_blocking(move || {
        zt.ensure(&network, roots.as_deref(), &storage, require_ready)
    })
    .await
    .map_err(|error| AppError::msg(error.to_string()))?
}
fn planet_info(state: &AppState) -> PlanetInfo {
    let path = state.planet_file().ok();
    let size = path
        .as_ref()
        .and_then(|path| std::fs::metadata(path).ok())
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    let configured = path.as_ref().map(|path| path.is_file()).unwrap_or(false);
    PlanetInfo {
        configured,
        custom_roots: state.config().custom_roots,
        size,
        message: if configured {
            "Свой planet установлен; встроенные корни отключены".to_string()
        } else {
            "Используются встроенные корни ZeroTier".to_string()
        },
    }
}
fn public_config(state: &AppState) -> PublicConfig {
    let cfg = state.config();
    let has_password = state.inner.password.read().unwrap().is_some();
    let suggested_gallery_path = config::suggested_gallery_path(&cfg.smb);
    let installed = system_installed(&cfg);
    let connection_mode = cfg.resolved_connection_mode().to_string();
    PublicConfig {
        setup_complete: cfg.setup_complete,
        zerotier_network_id: cfg.zerotier_network_id,
        connection_mode,
        smb: cfg.smb,
        sync_jobs: cfg.sync_jobs,
        remote_gallery_path: cfg.remote_gallery_path,
        suggested_gallery_path,
        gallery_enabled: cfg.gallery_enabled,
        gallery_columns: cfg.gallery_columns,
        file_name_scale: cfg.file_name_scale,
        file_view_mode: cfg.file_view_mode,
        tree_show_files: cfg.tree_show_files,
        tree_font_scale: cfg.tree_font_scale,
        theme_color: cfg.theme_color,
        external_documents: cfg.external_documents,
        external_save_enabled: cfg.external_save_enabled,
        external_save_mode: cfg.external_save_mode,
        download_dir: cfg.download_dir,
        download_auto_save: cfg.download_auto_save,
        download_keep_structure: cfg.download_keep_structure,
        system_install_dir: cfg.system_install_dir.clone(),
        system_installed: installed,
        system_autostart: cfg.system_autostart,
        system_start_menu: cfg.system_start_menu,
        system_desktop_shortcut: cfg.system_desktop_shortcut,
        system_tray: cfg.system_tray,
        system_notifications: cfg.system_notifications,
        notify_downloads: cfg.notify_downloads,
        notify_uploads: cfg.notify_uploads,
        notify_sync: cfg.notify_sync,
        notify_errors: cfg.notify_errors,
        wireguard_config_path: cfg.wireguard_config_path,
        wireguard_enabled: cfg.wireguard_enabled,
        ai_enabled: cfg.ai_enabled,
        ai_allow_download: cfg.ai_allow_download,
        ai_allow_modify: cfg.ai_allow_modify,
        ai_allow_upload: cfg.ai_allow_upload,
        ai_allow_mkdir: cfg.ai_allow_mkdir,
        ai_allow_delete: cfg.ai_allow_delete,
        ai_deny_paths: cfg.ai_deny_paths,
        android_tree_uri: cfg.android_tree_uri,
        update_repo: cfg.update_repo,
        custom_roots: cfg.custom_roots,
        planet_configured: state.planet_configured(),
        has_password,
        platform: platform(),
        android: cfg!(target_os = "android"),
    }
}

fn system_installed(cfg: &AppConfig) -> bool {
    if cfg.system_install_dir.trim().is_empty() {
        return false;
    }
    let Some(current) = std::env::current_exe().ok() else {
        return false;
    };
    let Some(parent) = current.parent() else {
        return false;
    };
    parent == std::path::Path::new(cfg.system_install_dir.trim())
}

#[tauri::command]
pub async fn request_notification_permission(app: AppHandle) -> AppResult<()> {
    #[cfg(target_os = "android")]
    {
        let _: Value =
            android_bridge::call(&app, "requestNotificationPermission", serde_json::json!({}))
                .await?;
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
    }
    Ok(())
}

#[tauri::command]
pub async fn test_notification(app: AppHandle) -> AppResult<String> {
    #[cfg(target_os = "android")]
    {
        let _: Value = android_bridge::call(
            &app,
            "transferProgress",
            serde_json::json!({
                "title": "ZeroSMB",
                "text": "Уведомления работают",
                "current": 1,
                "total": 1,
                "done": true
            }),
        )
        .await?;
    }
    #[cfg(target_os = "windows")]
    {
        tauri_winrt_notification::Toast::new(
            tauri_winrt_notification::Toast::POWERSHELL_APP_ID,
        )
        .title("ZeroSMB")
        .text1("Уведомления работают")
        .show()
        .map_err(|error| AppError::msg(error.to_string()))?;
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "android")))]
    {
        use tauri_plugin_notification::NotificationExt;
        app.notification()
            .builder()
            .title("ZeroSMB")
            .body("Уведомления работают")
            .show()
            .map_err(|error| AppError::msg(error.to_string()))?;
    }
    Ok("Тестовое уведомление отправлено".to_string())
}

#[tauri::command]
pub fn runtime_info() -> RuntimeInfo {
    RuntimeInfo {
        platform: platform(),
        android: cfg!(target_os = "android"),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

#[tauri::command]
pub fn get_config(state: State<'_, AppState>) -> PublicConfig {
    public_config(&state)
}

#[tauri::command]
pub async fn test_connection(
    app: AppHandle,
    smb: SmbConfig,
    password: String,
    zerotier_network_id: Option<String>,
    connection_mode: Option<String>,
) -> AppResult<()> {
    let network_id = zerotier_network_id.as_deref().unwrap_or("");
    let state = app.state::<AppState>();
    let mode = connection_mode
        .as_deref()
        .and_then(config::normalize_connection_mode)
        .unwrap_or_else(|| state.config().resolved_connection_mode());
    let effective =
        zt_proxy::effective_smb_config_for(&app, &smb, mode, network_id).await?;
    if mode == "local" {
        probe_tcp(&smb).await?;
    }
    SmbService::test_connection(&effective, &password).await
}

#[tauri::command]
pub async fn test_current_connection(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<()> {
    let cfg = state.config();
    let mode = cfg.resolved_connection_mode();
    let effective =
        zt_proxy::effective_smb_config_for(&app, &cfg.smb, mode, &cfg.zerotier_network_id)
            .await?;
    if mode == "local" {
        probe_tcp(&cfg.smb).await?;
    }
    SmbService::test_connection(&effective, &state.password()?).await
}

#[tauri::command]
pub async fn test_connection_saved(
    app: AppHandle,
    state: State<'_, AppState>,
    smb: SmbConfig,
    zerotier_network_id: Option<String>,
    connection_mode: Option<String>,
) -> AppResult<()> {
    let network_id = match zerotier_network_id.as_ref() {
        Some(value) => value.trim().to_string(),
        None => state.config().zerotier_network_id,
    };
    let mode = connection_mode
        .as_deref()
        .and_then(config::normalize_connection_mode)
        .unwrap_or_else(|| state.config().resolved_connection_mode());
    let effective =
        zt_proxy::effective_smb_config_for(&app, &smb, mode, &network_id).await?;
    if mode == "local" {
        probe_tcp(&smb).await?;
    }
    SmbService::test_connection(&effective, &state.password()?).await
}

#[tauri::command]
pub fn get_planet(state: State<'_, AppState>) -> PlanetInfo {
    planet_info(&state)
}

#[tauri::command]
pub async fn pick_planet(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<PlanetInfo> {
    let data: Vec<u8>;

    #[cfg(not(target_os = "android"))]
    {
        use tauri_plugin_dialog::DialogExt;
        let app_for_dialog = app.clone();
        let picked = tokio::task::spawn_blocking(move || {
            app_for_dialog
                .dialog()
                .file()
                .set_title("Выберите файл planet (ZeroTier root set)")
                .blocking_pick_file()
        })
        .await
        .map_err(|error| AppError::msg(error.to_string()))?;
        let path = picked
            .and_then(|file| file.into_path().ok())
            .ok_or_else(|| AppError::msg("Файл planet не выбран"))?;
        data = std::fs::read(path)?;
    }

    #[cfg(target_os = "android")]
    {
        let value: Value = android_bridge::call(&app, "pickPlanet", serde_json::json!({})).await?;
        if value
            .get("cancelled")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
        {
            return Err(AppError::msg("Файл planet не выбран"));
        }
        let path = value
            .get("path")
            .and_then(|value| value.as_str())
            .ok_or_else(|| AppError::msg("Android не вернул путь к planet"))?;
        data = std::fs::read(path)?;
    }

    if data.is_empty() || data.len() > 4096 {
        return Err(AppError::msg(format!(
            "Некорректный planet: {} байт (ожидается 1..4096)",
            data.len()
        )));
    }
    let destination = state.planet_file()?;
    std::fs::write(&destination, &data)?;
    state.update_config(|cfg| cfg.custom_roots = true)?;
    #[cfg(target_os = "android")]
    {
        let _: Value = android_bridge::call(&app, "ztStop", serde_json::json!({})).await?;
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = &app;
        state.inner.zt.stop();
    }
    Ok(planet_info(&state))
}

#[tauri::command]
pub async fn clear_planet(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<PlanetInfo> {
    let path = state.planet_file()?;
    let _ = std::fs::remove_file(path);
    state.update_config(|cfg| cfg.custom_roots = false)?;
    #[cfg(target_os = "android")]
    {
        let _: Value = android_bridge::call(&app, "ztStop", serde_json::json!({})).await?;
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = &app;
        state.inner.zt.stop();
    }
    let mut info = planet_info(&state);
    info.message =
        "Свой planet удалён. Перезапустите приложение, чтобы вернуть встроенные корни."
            .to_string();
    Ok(info)
}
#[tauri::command]
pub async fn save_setup(
    app: AppHandle,
    state: State<'_, AppState>,
    input: SetupInput,
) -> AppResult<PublicConfig> {
    let network = input.zerotier_network_id.trim().to_string();
    let mode = input
        .connection_mode
        .as_deref()
        .and_then(config::normalize_connection_mode)
        .unwrap_or_else(|| {
            if network.is_empty() {
                "local"
            } else {
                "zerotier"
            }
        })
        .to_string();
    input.smb.validate()?;

    let dir = state.data_dir()?;
    config::save_password(&dir, &input.smb, &input.password)?;
    state.set_password(Some(input.password));

    let mut cfg = state.config();
    cfg.setup_complete = true;
    cfg.zerotier_network_id = network;
    cfg.connection_mode = mode.clone();
    cfg.smb = input.smb;
    cfg.remote_gallery_path = input
        .remote_gallery_path
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| config::suggested_gallery_path(&cfg.smb));
    if let Some(dir) = input.system_install_dir {
        cfg.system_install_dir = dir.trim().to_string();
    }
    if let Some(value) = input.system_autostart {
        cfg.system_autostart = value;
    }
    if let Some(value) = input.system_start_menu {
        cfg.system_start_menu = value;
    }
    if let Some(value) = input.system_desktop_shortcut {
        cfg.system_desktop_shortcut = value;
    }
    if let Some(value) = input.system_tray {
        cfg.system_tray = value;
    }
    if let Some(value) = input.system_notifications {
        cfg.system_notifications = value;
    }
    if let Some(value) = input.notify_downloads {
        cfg.notify_downloads = value;
    }
    if let Some(value) = input.notify_uploads {
        cfg.notify_uploads = value;
    }
    if let Some(value) = input.notify_sync {
        cfg.notify_sync = value;
    }
    if let Some(value) = input.notify_errors {
        cfg.notify_errors = value;
    }
    if let Some(dir) = input.download_dir {
        cfg.download_dir = dir.trim().to_string();
    }
    if let Some(value) = input.download_auto_save {
        cfg.download_auto_save = value;
    }
    if let Some(value) = input.download_keep_structure {
        cfg.download_keep_structure = value;
    }
    if mode != "wireguard" {
        cfg.wireguard_enabled = false;
    }
    state.set_config(cfg)?;
    if mode != "wireguard" {
        state.inner.wireguard.stop().await;
    }
    if mode != "zerotier" {
        #[cfg(target_os = "android")]
        {
            let _: Value = android_bridge::call(&app, "ztStop", serde_json::json!({})).await?;
        }
        #[cfg(not(target_os = "android"))]
        {
            state.inner.zt.stop();
        }
    }
    #[cfg(not(target_os = "android"))]
    {
        let current = state.config();
        crate::integration::apply_tray(&app, current.system_tray);
        if let Ok(executable) = std::env::current_exe() {
            let _ = crate::integration::set_autostart(current.system_autostart, &executable);
        }
    }
    state.inner.smb.reset().await;
    state.inner.media_smb.reset().await;
    Ok(public_config(&state))
}

#[tauri::command]
pub async fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    input: SettingsInput,
) -> AppResult<PublicConfig> {
    input.smb.validate()?;
    let old_smb = state.config().smb;
    let password = input
        .password
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let dir = state.data_dir()?;
    if old_smb.account() != input.smb.account() && password.is_none() {
        config::clear_password(&dir, &old_smb);
        state.set_password(None);
    }
    if let Some(password) = password {
        config::save_password(&dir, &input.smb, &password)?;
        state.set_password(Some(password));
    }

    let mut cfg = state.config();
    cfg.smb = input.smb;
    if let Some(path) = input.remote_gallery_path {
        let path = path.trim();
        cfg.remote_gallery_path = if path.is_empty() {
            config::suggested_gallery_path(&cfg.smb)
        } else {
            path.to_string()
        };
    }
    if let Some(enabled) = input.gallery_enabled {
        cfg.gallery_enabled = enabled;
    }
    if let Some(columns) = input.gallery_columns {
        cfg.gallery_columns = columns.clamp(2, 12);
    }
    if let Some(scale) = input.file_name_scale {
        cfg.file_name_scale = scale.clamp(80, 180);
    }
    if let Some(mode) = input.file_view_mode {
        cfg.file_view_mode = if mode == "details" { "details" } else { "list" }.to_string();
    }
    if let Some(show_files) = input.tree_show_files {
        cfg.tree_show_files = show_files;
    }
    if let Some(scale) = input.tree_font_scale {
        cfg.tree_font_scale = scale.clamp(80, 180);
    }
    if let Some(value) = input.external_documents {
        cfg.external_documents = value;
    }
    if let Some(value) = input.external_save_enabled {
        cfg.external_save_enabled = value;
    }
    if let Some(mode) = input.external_save_mode {
        cfg.external_save_mode = if mode == "auto" {
            "auto".to_string()
        } else {
            "button".to_string()
        };
    }
    if let Some(color) = input.theme_color {
        cfg.theme_color = config::normalize_theme_color(&color);
    }
    if let Some(dir) = input.download_dir {
        cfg.download_dir = dir.trim().to_string();
    }
    if let Some(auto) = input.download_auto_save {
        cfg.download_auto_save = auto;
    }
    if let Some(keep) = input.download_keep_structure {
        cfg.download_keep_structure = keep;
    }
    if let Some(dir) = input.system_install_dir {
        cfg.system_install_dir = dir.trim().to_string();
    }
    if let Some(value) = input.system_autostart {
        cfg.system_autostart = value;
    }
    if let Some(value) = input.system_start_menu {
        cfg.system_start_menu = value;
    }
    if let Some(value) = input.system_desktop_shortcut {
        cfg.system_desktop_shortcut = value;
    }
    if let Some(value) = input.system_tray {
        cfg.system_tray = value;
    }
    if let Some(value) = input.system_notifications {
        cfg.system_notifications = value;
    }
    if let Some(value) = input.notify_downloads {
        cfg.notify_downloads = value;
    }
    if let Some(value) = input.notify_uploads {
        cfg.notify_uploads = value;
    }
    if let Some(value) = input.notify_sync {
        cfg.notify_sync = value;
    }
    if let Some(value) = input.notify_errors {
        cfg.notify_errors = value;
    }
    if let Some(value) = input.wireguard_config_path {
        cfg.wireguard_config_path = value.trim().to_string();
    }
    if let Some(value) = input.wireguard_enabled {
        cfg.wireguard_enabled = value;
    }
    if let Some(network) = input.zerotier_network_id {
        cfg.zerotier_network_id = network.trim().to_string();
    }
    if let Some(mode) = input.connection_mode {
        if let Some(mode) = config::normalize_connection_mode(&mode) {
            cfg.connection_mode = mode.to_string();
        }
    }
    if let Some(value) = input.ai_enabled {
        cfg.ai_enabled = value;
    }
    if let Some(value) = input.ai_allow_download {
        cfg.ai_allow_download = value;
    }
    if let Some(value) = input.ai_allow_modify {
        cfg.ai_allow_modify = value;
    }
    if let Some(value) = input.ai_allow_upload {
        cfg.ai_allow_upload = value;
    }
    if let Some(value) = input.ai_allow_mkdir {
        cfg.ai_allow_mkdir = value;
    }
    if let Some(value) = input.ai_allow_delete {
        cfg.ai_allow_delete = value;
    }
    if let Some(paths) = input.ai_deny_paths {
        let mut seen = HashSet::new();
        cfg.ai_deny_paths = paths
            .into_iter()
            .map(|path| normalize_rel(&path))
            .filter(|path| !path.is_empty() && seen.insert(path.clone()))
            .collect();
    }
    if let Some(repo) = input.update_repo {
        if !repo.trim().is_empty() {
            cfg.update_repo = repo.trim().to_string();
        }
    }
    if cfg.resolved_connection_mode() != "wireguard" {
        cfg.wireguard_enabled = false;
    }
    state.set_config(cfg)?;
    #[cfg(not(target_os = "android"))]
    {
        let current = state.config();
        let executable = if current.system_install_dir.trim().is_empty() {
            std::env::current_exe().ok()
        } else {
            std::env::current_exe().ok().map(|exe| {
                let name = exe.file_name().unwrap_or_default();
                PathBuf::from(current.system_install_dir.trim()).join(name)
            })
        };
        if let Some(executable) = executable {
            let _ = crate::integration::set_autostart(current.system_autostart, &executable);
        }
        crate::integration::apply_tray(&app, current.system_tray);
    }
    let mode = state.config().resolved_connection_mode();
    if mode != "wireguard" {
        state.inner.wireguard.stop().await;
    }
    if mode != "zerotier" {
        #[cfg(target_os = "android")]
        {
            let _: Value =
                android_bridge::call(&app, "ztStop", serde_json::json!({})).await?;
        }
        #[cfg(not(target_os = "android"))]
        {
            state.inner.zt.stop();
        }
    }
    if let Err(error) = state.inner.ai.sync(&app).await {
        log::warn!("AI API: {error}");
    }
    state.inner.smb.reset().await;
    state.inner.media_smb.reset().await;
    Ok(public_config(&state))
}

#[tauri::command]
pub async fn pick_local_folder(app: AppHandle) -> AppResult<Option<String>> {
    #[cfg(not(target_os = "android"))]
    {
        use tauri_plugin_dialog::DialogExt;
        let picked = tokio::task::spawn_blocking(move || {
            app.dialog()
                .file()
                .set_title("Выберите папку для скачивания")
                .blocking_pick_folder()
        })
        .await
        .map_err(|error| AppError::msg(error.to_string()))?;
        Ok(picked
            .and_then(|path| path.into_path().ok())
            .map(|path| path.to_string_lossy().to_string()))
    }
    #[cfg(target_os = "android")]
    {
        let _ = app;
        Ok(None)
    }
}

#[tauri::command]
pub async fn smb_list(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> AppResult<Vec<FileEntry>> {
    let cfg = state.config();
    let password = state.password()?;
    let relative = if path.trim().is_empty() {
        cfg.smb.normalized_base()
    } else {
        normalize_rel(&path)
    };
    let operation = async {
        let effective =
            zt_proxy::effective_smb_config(&app, &cfg.smb, &cfg.zerotier_network_id).await?;
        let client = state.inner.smb.connect(&effective, &password).await?;
        match state.inner.smb.list_dir(&client, "", &relative).await {
            Ok(entries) => Ok(entries),
            Err(_) => {
                state.inner.smb.reset().await;
                let client = state.inner.smb.connect(&effective, &password).await?;
                state.inner.smb.list_dir(&client, "", &relative).await
            }
        }
    };
    match tokio::time::timeout(std::time::Duration::from_secs(30), operation).await {
        Ok(result) => result,
        Err(_) => {
            state.inner.smb.reset().await;
            Err(AppError::msg(
                "SMB: список файлов не получен за 30 секунд. Соединение сброшено, повторите.",
            ))
        }
    }
}

#[tauri::command]
pub async fn ensure_gallery_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> AppResult<bool> {
    let cfg = state.config();
    let password = state.password()?;
    let effective =
        zt_proxy::effective_smb_config(&app, &cfg.smb, &cfg.zerotier_network_id).await?;
    let client = state.inner.smb.connect(&effective, &password).await?;
    let relative = normalize_rel(&path);
    if relative.is_empty() {
        return Err(AppError::msg("Пустой путь галереи"));
    }
    match state.inner.smb.list_dir(&client, "", &relative).await {
        Ok(_) => Ok(false),
        Err(_) => {
            state
                .inner
                .smb
                .create_dir_path(&client, &relative)
                .await?;
            Ok(true)
        }
    }
}

#[tauri::command]
pub async fn gallery_list(app: AppHandle) -> AppResult<GalleryList> {
    if !cfg!(target_os = "android") {
        return Err(AppError::msg(
            "Галерея телефона доступна только в Android-версии",
        ));
    }
    android_bridge::call(&app, "listMedia", serde_json::json!({})).await
}

#[tauri::command]
pub async fn gallery_thumbnail(
    app: AppHandle,
    uri: String,
    size: Option<u32>,
) -> AppResult<String> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Response {
        data_url: String,
    }
    let response: Response = android_bridge::call(
        &app,
        "getThumbnail",
        serde_json::json!({ "uri": uri, "size": size.unwrap_or(256) }),
    )
    .await?;
    Ok(response.data_url)
}

#[tauri::command]
pub async fn gallery_export(
    app: AppHandle,
    state: State<'_, AppState>,
    uri: String,
    name: String,
    mime: String,
) -> AppResult<MediaRef> {
    #[derive(Deserialize)]
    struct Response {
        path: String,
    }
    let response: Response = android_bridge::call(
        &app,
        "exportMedia",
        serde_json::json!({ "uri": uri, "name": name }),
    )
    .await?;
    let token = state.register_local_media(PathBuf::from(response.path));
    let kind = if mime.starts_with("video/") {
        "video"
    } else if mime == "image/gif" {
        "gif"
    } else {
        file_kind(&name)
    };
    Ok(MediaRef {
        path: format!("local/{token}"),
        kind: kind.to_string(),
    })
}

#[tauri::command]
pub async fn sync_gallery(
    app: AppHandle,
    state: State<'_, AppState>,
    remote_path: String,
    items: Option<Vec<GalleryItem>>,
    conflict_policy: Option<String>,
    on_progress: Channel<SyncProgress>,
) -> AppResult<SyncSummary> {
    if !cfg!(target_os = "android") {
        return Err(AppError::msg(
            "Автоматическая загрузка галереи доступна только на Android",
        ));
    }
    let selection = items.map(|items| {
        items
            .into_iter()
            .map(|item| sync::GalleryItem {
                uri: item.uri,
                name: item.name,
                mime: item.mime,
                size: item.size,
                date_modified: item.date_modified,
                date_created: item.date_created,
                relative_path: item.relative_path,
                bucket: item.bucket,
            })
            .collect()
    });
    sync::run_gallery(
        &app,
        &state,
        &remote_path,
        selection,
        conflict_policy.as_deref().unwrap_or("skip"),
        &on_progress,
    )
    .await
}

#[tauri::command]
pub async fn android_pick_tree(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<Option<String>> {
    if !cfg!(target_os = "android") {
        return Ok(None);
    }
    let value: Value = android_bridge::call(&app, "pickDirectory", serde_json::json!({})).await?;
    if value
        .get("cancelled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return Ok(None);
    }
    let uri = value
        .get("uri")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    if let Some(uri) = &uri {
        let mut cfg = state.config();
        cfg.android_tree_uri = Some(uri.clone());
        state.set_config(cfg)?;
    }
    Ok(uri)
}

#[tauri::command]
pub fn android_tree_uri(state: State<'_, AppState>) -> Option<String> {
    state.config().android_tree_uri
}

#[tauri::command]
pub fn sync_add_job(
    state: State<'_, AppState>,
    label: Option<String>,
    local_path: Option<String>,
    android_tree_uri: Option<String>,
    remote_path: String,
    mode: Option<String>,
    interval_minutes: Option<u32>,
    delete_enabled: Option<bool>,
) -> AppResult<SyncJob> {
    if remote_path.trim().is_empty() {
        return Err(AppError::msg("Укажите папку назначения на NAS"));
    }
    let source = sync_source(local_path, android_tree_uri)?;
    let default_label = sync_source_label(&source);
    let job = SyncJob {
        id: uuid::Uuid::new_v4().to_string(),
        label: label
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or(default_label),
        source,
        remote_path: remote_path.trim().replace('\\', "/"),
        mode: sync::normalize_mode(mode.as_deref()),
        interval_minutes: interval_minutes.unwrap_or(15).clamp(1, 1440),
        delete_enabled: delete_enabled.unwrap_or(false),
        last_sync: 0,
    };
    let mut cfg = state.config();
    cfg.sync_jobs.push(job.clone());
    state.set_config(cfg)?;
    Ok(job)
}

#[tauri::command]
pub fn sync_update_job(
    state: State<'_, AppState>,
    id: String,
    label: Option<String>,
    local_path: Option<String>,
    android_tree_uri: Option<String>,
    remote_path: String,
    mode: Option<String>,
    interval_minutes: Option<u32>,
    delete_enabled: Option<bool>,
) -> AppResult<PublicConfig> {
    if remote_path.trim().is_empty() {
        return Err(AppError::msg("Укажите папку назначения на NAS"));
    }
    let source = sync_source(local_path, android_tree_uri)?;
    let mut cfg = state.config();
    let job = cfg
        .sync_jobs
        .iter_mut()
        .find(|job| job.id == id)
        .ok_or_else(|| AppError::msg("Задание синхронизации не найдено"))?;
    let previous_source = job.source.clone();
    let previous_remote = job.remote_path.clone();
    if let Some(label) = label
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        job.label = label;
    }
    job.source = source;
    job.remote_path = remote_path.trim().replace('\\', "/");
    job.mode = sync::normalize_mode(mode.as_deref());
    job.interval_minutes = interval_minutes.unwrap_or(job.interval_minutes).clamp(1, 1440);
    job.delete_enabled = delete_enabled.unwrap_or(job.delete_enabled);
    let source_changed =
        previous_source != job.source || previous_remote != job.remote_path;
    state.set_config(cfg)?;
    if source_changed {
        if let Ok(dir) = state.data_dir() {
            let _ = std::fs::remove_file(dir.join("sync-state").join(format!("{id}.json")));
        }
    }
    Ok(public_config(&state))
}

fn sync_source(
    local_path: Option<String>,
    android_tree_uri: Option<String>,
) -> AppResult<SyncSource> {
    match (local_path, android_tree_uri) {
        (_, Some(uri)) if cfg!(target_os = "android") => Ok(SyncSource::AndroidTree(uri)),
        (Some(path), _) if !cfg!(target_os = "android") => Ok(SyncSource::LocalPath(path)),
        _ => Err(AppError::msg("Выберите папку для синхронизации")),
    }
}

fn sync_source_label(source: &SyncSource) -> String {
    match source {
        SyncSource::LocalPath(path) => path
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or("Папка")
            .to_string(),
        SyncSource::AndroidTree(_) => "Папка Android".to_string(),
    }
}

#[tauri::command]
pub async fn sync_list_remote_dirs(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<Vec<String>> {
    let cfg = state.config();
    let password = state.password()?;
    let effective =
        zt_proxy::effective_smb_config(&app, &cfg.smb, &cfg.zerotier_network_id).await?;
    let client = state.inner.smb.connect(&effective, &password).await?;
    let base = cfg.smb.normalized_base();
    let mut queue = VecDeque::from([String::new()]);
    let mut seen = HashSet::new();
    let mut dirs = Vec::new();
    while let Some(dir) = queue.pop_front() {
        let entries = match state.inner.smb.list_dir(&client, &base, &dir).await {
            Ok(entries) => entries,
            Err(error) if dir.is_empty() => return Err(error),
            Err(_) => continue,
        };
        for entry in entries {
            if !entry.is_dir {
                continue;
            }
            let relative = if base.is_empty() {
                entry.path.clone()
            } else {
                entry
                    .path
                    .strip_prefix(&format!("{base}/"))
                    .unwrap_or(&entry.path)
                    .to_string()
            };
            if seen.insert(relative.clone()) {
                queue.push_back(relative.clone());
                dirs.push(relative);
            }
        }
    }
    dirs.sort_by_key(|value| value.to_lowercase());
    Ok(dirs)
}

#[tauri::command]
pub fn sync_remove_job(state: State<'_, AppState>, id: String) -> AppResult<PublicConfig> {
    let mut cfg = state.config();
    cfg.sync_jobs.retain(|job| job.id != id);
    state.set_config(cfg)?;
    if let Ok(dir) = state.data_dir() {
        let _ = std::fs::remove_file(dir.join("sync-state").join(format!("{id}.json")));
    }
    Ok(public_config(&state))
}

#[tauri::command]
pub async fn sync_run_job(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    on_progress: Channel<SyncProgress>,
) -> AppResult<SyncSummary> {
    let job = state
        .config()
        .sync_jobs
        .into_iter()
        .find(|job| job.id == id)
        .ok_or_else(|| AppError::msg("Задание синхронизации не найдено"))?;
    if matches!(&job.source, SyncSource::AndroidTree(_)) && !cfg!(target_os = "android") {
        return Err(AppError::msg("Задание Android-папки доступно только на Android"));
    }
    sync::run_job(&app, &state, &job, &on_progress).await
}

#[tauri::command]
pub async fn zt_status(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<ZtStatus> {
    let network_id = state.config().zerotier_network_id;
    #[cfg(target_os = "android")]
    {
        let planet = state
            .planet_path()?
            .map(|path| path.to_string_lossy().to_string());
        let id = if network_id.trim().is_empty() {
            None
        } else {
            Some(network_id.as_str())
        };
        zerotier::embedded_status(&app, id, planet.as_deref()).await
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        if network_id.trim().is_empty() {
            let mut status = ZtStatus::default();
            status.message = "Сначала укажите Network ID ZeroTier".to_string();
            return Ok(status);
        }
        embedded_desktop_ensure(&state, &network_id, false).await
    }
}

#[tauri::command]
pub async fn zt_join(
    app: AppHandle,
    state: State<'_, AppState>,
    network_id: String,
) -> AppResult<ZtStatus> {
    #[cfg(target_os = "android")]
    {
        let planet = state
            .planet_path()?
            .map(|path| path.to_string_lossy().to_string());
        zerotier::embedded_join(&app, &network_id, planet.as_deref()).await
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        embedded_desktop_ensure(&state, &network_id, false).await
    }
}

#[tauri::command]
pub async fn zt_leave(
    app: AppHandle,
    state: State<'_, AppState>,
    network_id: String,
) -> AppResult<ZtStatus> {
    #[cfg(target_os = "android")]
    {
        let _ = (state, network_id);
        zerotier::embedded_leave(&app).await
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, network_id);
        let zt = state.inner.zt.clone();
        tokio::task::spawn_blocking(move || zt.leave())
            .await
            .map_err(|error| AppError::msg(error.to_string()))?
    }
}

#[tauri::command]
pub async fn zt_open_android(app: AppHandle, network_id: String) -> AppResult<Value> {
    android_bridge::call(
        &app,
        "openZeroTier",
        serde_json::json!({ "networkId": network_id }),
    )
    .await
}

#[tauri::command]
pub async fn zt_open_help(app: AppHandle) -> AppResult<()> {
    #[cfg(target_os = "android")]
    {
        let _: Value = android_bridge::call(
            &app,
            "openZeroTier",
            serde_json::json!({ "networkId": "" }),
        )
        .await?;
        Ok(())
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        open::that("https://www.zerotier.com/download/")
            .map_err(|error| AppError::msg(error.to_string()))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadResult {
    pub uploaded: usize,
    pub failed: usize,
    pub errors: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PickedFile {
    path: String,
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PickFilesResponse {
    #[serde(default)]
    files: Vec<PickedFile>,
}

fn remote_file_name(name: &str) -> String {
    let cleaned: String = name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("file")
        .chars()
        .map(|ch| if ch.is_control() { '_' } else { ch })
        .collect();
    if cleaned.trim().is_empty() {
        "file".to_string()
    } else {
        cleaned
    }
}

#[tauri::command]
pub async fn upload_files(
    app: AppHandle,
    state: State<'_, AppState>,
    remote_dir: String,
) -> AppResult<UploadResult> {
    let mut files: Vec<(PathBuf, String)> = Vec::new();

    #[cfg(not(target_os = "android"))]
    {
        use tauri_plugin_dialog::DialogExt;
        let app_for_dialog = app.clone();
        let picked = tokio::task::spawn_blocking(move || {
            app_for_dialog.dialog().file().blocking_pick_files()
        })
        .await
        .map_err(|error| AppError::msg(error.to_string()))?;
        for file in picked.unwrap_or_default() {
            if let Ok(path) = file.into_path() {
                let name = path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("file")
                    .to_string();
                files.push((path, name));
            }
        }
    }

    #[cfg(target_os = "android")]
    {
        let response: PickFilesResponse =
            android_bridge::call(&app, "pickFiles", serde_json::json!({})).await?;
        for file in response.files {
            files.push((PathBuf::from(file.path), file.name));
        }
    }

    if files.is_empty() {
        return Err(AppError::msg("Файлы не выбраны"));
    }
    let total = files.len();
    android_progress::update(
        &app,
        "Загрузка на NAS",
        &format!("Файлов: {total}"),
        0,
        total,
        false,
    )
    .await;

    let cfg = state.config();
    let password = state.password()?;
    let effective =
        zt_proxy::effective_smb_config(&app, &cfg.smb, &cfg.zerotier_network_id).await?;
    let client = state.inner.smb.connect(&effective, &password).await?;
    let dir = normalize_rel(&remote_dir);
    let transfer_ids = state.inner.transfers.begin_many(
        &app,
        "upload",
        files
            .iter()
            .map(|(path, name)| {
                (
                    remote_file_name(name),
                    std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0),
                )
            })
            .collect(),
    );
    let mut result = UploadResult {
        uploaded: 0,
        failed: 0,
        errors: Vec::new(),
    };
    for (index, (path, name)) in files.into_iter().enumerate() {
        let remote_name = remote_file_name(&name);
        let target = join_rel(&dir, &remote_name);
        let transfer_id = transfer_ids.get(index).cloned().unwrap_or_default();
        state.inner.transfers.start(&app, &transfer_id);
        let progress = |sent: u64, total: u64| {
            state
                .inner
                .transfers
                .progress(&app, &transfer_id, sent, total);
        };
        match state
            .inner
            .smb
            .upload_file_with_progress(&client, &target, &path, &progress)
            .await
        {
            Ok(_) => {
                state.inner.transfers.finish(&app, &transfer_id, None);
                result.uploaded += 1;
            }
            Err(error) => {
                let message = error.to_string();
                state
                    .inner
                    .transfers
                    .finish(&app, &transfer_id, Some(message.clone()));
                result.failed += 1;
                result.errors.push(format!("{remote_name}: {message}"));
            }
        }
        #[cfg(target_os = "android")]
        {
            let _ = tokio::fs::remove_file(&path).await;
        }
        android_progress::update(
            &app,
            "Загрузка на NAS",
            &result
                .errors
                .last()
                .cloned()
                .unwrap_or_else(|| format!("Загружено: {}", result.uploaded)),
            index + 1,
            total,
            false,
        )
        .await;
    }
    state.inner.smb.reset().await;
    android_progress::update(
        &app,
        "Загрузка завершена",
        &format!("Загружено: {} из {total}", result.uploaded),
        total,
        total,
        true,
    )
    .await;
    crate::integration::notify(
        &app,
        if result.failed > 0 { "error" } else { "upload" },
        "Загрузка на NAS",
        &format!(
            "Загружено: {}, ошибок: {}",
            result.uploaded, result.failed
        ),
    );
    Ok(result)
}

fn unique_local_path(path: &std::path::Path) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| std::path::Path::new(""));
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("file");
    let extension = path.extension().and_then(|value| value.to_str());
    for index in 1..10_000 {
        let name = match extension {
            Some(extension) => format!("{stem} ({index}).{extension}"),
            None => format!("{stem} ({index})"),
        };
        let candidate = parent.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    path.to_path_buf()
}
#[tauri::command]
pub async fn download_file(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    relative_dir: Option<String>,
    conflict_policy: Option<String>,
) -> AppResult<String> {
    let cfg = state.config();
    let password = state.password()?;
    let effective =
        zt_proxy::effective_smb_config(&app, &cfg.smb, &cfg.zerotier_network_id).await?;
    let client = state.inner.smb.connect(&effective, &password).await?;
    let mut name = remote_file_name(&path);
    if std::path::Path::new(&name).extension().is_none() {
        if let Some(extension) = std::path::Path::new(&path).extension() {
            name = format!("{}.{}", name, extension.to_string_lossy());
        }
    }
    let temp_dir = app
        .path()
        .app_cache_dir()
        .map_err(|error| AppError::msg(error.to_string()))?
        .join("downloads");
    std::fs::create_dir_all(&temp_dir)?;
    let temp_path = temp_dir.join(format!("{}-{name}", uuid::Uuid::new_v4()));
    let transfer_id = state
        .inner
        .transfers
        .begin_many(&app, "download", vec![(name.clone(), 0)])
        .into_iter()
        .next()
        .unwrap_or_default();
    android_progress::update(&app, "Скачивание файла", &name, 0, 0, false).await;
    let progress = |sent: u64, total: u64| {
        state
            .inner
            .transfers
            .progress(&app, &transfer_id, sent, total);
    };
    if let Err(error) = state
        .inner
        .smb
        .download_file_with_progress(&client, &path, &temp_path, &progress)
        .await
    {
        state
            .inner
            .transfers
            .finish(&app, &transfer_id, Some(error.to_string()));
        return Err(error);
    }
    state.inner.transfers.finish(&app, &transfer_id, None);

    #[cfg(target_os = "android")]
    {
        let mime = mime_guess::from_path(&name)
            .first_or_octet_stream()
            .to_string();
        let is_media = mime.starts_with("image/") || mime.starts_with("video/");
        let command = if is_media && relative_dir.is_some() {
            "saveToGallery"
        } else {
            "saveToDownloads"
        };
        let value: Value = android_bridge::call(
            &app,
            command,
            serde_json::json!({
                "path": temp_path.to_string_lossy(),
                "name": name,
                "mime": mime,
                "relativePath": relative_dir,
                "conflictPolicy": conflict_policy.unwrap_or_else(|| "skip".to_string()),
            }),
        )
        .await?;
        let _ = tokio::fs::remove_file(&temp_path).await;
        state.inner.transfers.finish(&app, &transfer_id, None);
        android_progress::update(&app, "Скачивание файла", "Готово", 1, 1, true).await;
        return Ok(value
            .get("path")
            .and_then(|value| value.as_str())
            .unwrap_or("Download/ZeroSMB")
            .to_string());
    }

    #[cfg(not(target_os = "android"))]
    {
        let configured_dir = cfg.download_dir.trim().to_string();
        let mut target = if !cfg.download_auto_save || configured_dir.is_empty() {
            use tauri_plugin_dialog::DialogExt;
            let app_for_dialog = app.clone();
            let name_for_dialog = name.clone();
            let initial_dir = if configured_dir.is_empty() {
                None
            } else {
                Some(PathBuf::from(&configured_dir))
            };
            let picked = tokio::task::spawn_blocking(move || {
                let mut dialog = app_for_dialog
                    .dialog()
                    .file()
                    .set_file_name(name_for_dialog);
                if let Some(dir) = initial_dir {
                    dialog = dialog.set_directory(dir);
                }
                dialog.blocking_save_file()
            })
            .await
            .map_err(|error| AppError::msg(error.to_string()))?;
            picked
                .and_then(|file| file.into_path().ok())
                .ok_or_else(|| AppError::msg("Скачивание отменено"))?
        } else {
            let mut dir = PathBuf::from(configured_dir);
            let relative = relative_dir
                .as_deref()
                .unwrap_or("")
                .trim_matches(|ch| ch == '/' || ch == '\\');
            if cfg.download_keep_structure && !relative.is_empty() {
                dir = dir.join(relative);
            }
            std::fs::create_dir_all(&dir)?;
            dir.join(&name)
        };
        if target.extension().is_none() {
            if let Some(extension) = std::path::Path::new(&name).extension() {
                target.set_extension(extension);
            }
        }
        if target.exists() {
            if conflict_policy.as_deref().unwrap_or("skip") == "rename" {
                target = unique_local_path(&target);
            } else {
                let _ = std::fs::remove_file(&temp_path);
                state.inner.transfers.finish(&app, &transfer_id, None);
                return Ok(format!("Пропущено: {}", target.to_string_lossy()));
            }
        }
        std::fs::copy(&temp_path, &target)?;
        let _ = std::fs::remove_file(&temp_path);
        state.inner.transfers.finish(&app, &transfer_id, None);
        android_progress::update(&app, "Скачивание файла", "Готово", 1, 1, true).await;
        #[cfg(not(target_os = "android"))]
        crate::integration::notify(
            &app,
            "download",
            "Скачивание завершено",
            &format!("Файл сохранён: {}", target.display()),
        );
        Ok(target.to_string_lossy().to_string())
    }
}

#[tauri::command]
pub async fn read_text_file(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> AppResult<String> {
    let cfg = state.config();
    let password = state.password()?;
    let effective =
        zt_proxy::effective_smb_config(&app, &cfg.smb, &cfg.zerotier_network_id).await?;
    let client = state.inner.smb.connect(&effective, &password).await?;
    let len = state.inner.smb.file_len(&client, "", &path).await?;
    const LIMIT: u64 = 512 * 1024;
    let read = len.min(LIMIT) as usize;
    let data = state
        .inner
        .smb
        .read_range(&client, "", &path, 0, read)
        .await?;
    let mut text = String::from_utf8_lossy(&data).to_string();
    if len > LIMIT {
        text.push_str("\n\n… файл обрезан для предпросмотра …");
    }
    Ok(text)
}

#[tauri::command]
pub async fn read_file_bytes(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> AppResult<tauri::ipc::Response> {
    let operation = async {
        let cfg = state.config();
        let password = state.password()?;
        let effective =
            zt_proxy::effective_smb_config(&app, &cfg.smb, &cfg.zerotier_network_id).await?;
        let client = state.inner.smb.connect(&effective, &password).await?;
        let len = state.inner.smb.file_len(&client, "", &path).await?;
        const LIMIT: u64 = 32 * 1024 * 1024;
        if len > LIMIT {
            return Err(AppError::msg("Файл больше 32 МБ, скачайте его для просмотра"));
        }
        let data = state
            .inner
            .smb
            .read_range(&client, "", &path, 0, len as usize)
            .await?;
        Ok::<_, AppError>(tauri::ipc::Response::new(data))
    };
    tokio::time::timeout(std::time::Duration::from_secs(30), operation)
        .await
        .map_err(|_| AppError::msg("Файл не загрузился за 30 секунд. Повторите."))?
}
#[tauri::command]
pub async fn read_media_bytes(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> AppResult<tauri::ipc::Response> {
    let operation = async {
        let _guard = state.inner.media_lock.lock().await;
        let cfg = state.config();
        let password = state.password()?;
        let effective =
            zt_proxy::effective_smb_config(&app, &cfg.smb, &cfg.zerotier_network_id).await?;
        let client = state.inner.media_smb.connect(&effective, &password).await?;
        let len = state.inner.media_smb.file_len(&client, "", &path).await?;
        const LIMIT: u64 = 32 * 1024 * 1024;
        if len > LIMIT {
            return Err(AppError::msg("Файл больше 32 МБ, скачайте его для просмотра"));
        }
        let data = state
            .inner
            .media_smb
            .read_range(&client, "", &path, 0, len as usize)
            .await?;
        Ok::<_, AppError>(tauri::ipc::Response::new(data))
    };
    let result = tokio::time::timeout(std::time::Duration::from_secs(30), operation)
        .await
        .map_err(|_| AppError::msg("Фото не загрузилось за 30 секунд. Повторите."))?;
    if result.is_err() {
        state.inner.media_smb.reset().await;
    }
    result
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeResult {
    pub host: String,
    pub port: u16,
    pub address: String,
}

async fn probe_tcp(smb: &SmbConfig) -> AppResult<ProbeResult> {
    let host = smb.normalized_host();
    if host.is_empty() {
        return Err(AppError::msg("Укажите host/IP NAS"));
    }
    let port = if smb.port == 0 { 445 } else { smb.port };
    let addresses = tokio::net::lookup_host((host.as_str(), port))
        .await
        .map_err(|error| {
            AppError::msg(format!(
                "Не удалось разрешить {host}:{port}: {error}. Укажите LAN-IP NAS, а не имя."
            ))
        })?;
    let mut last_error = String::from("нет адресов");
    let mut resolved = String::new();
    for address in addresses {
        resolved = address.to_string();
        match tokio::time::timeout(
            std::time::Duration::from_secs(6),
            tokio::net::TcpStream::connect(address),
        )
        .await
        {
            Ok(Ok(stream)) => {
                drop(stream);
                return Ok(ProbeResult {
                    host: host.clone(),
                    port,
                    address: resolved,
                });
            }
            Ok(Err(error)) => last_error = error.to_string(),
            Err(_) => last_error = "таймаут 6 секунд".to_string(),
        }
    }
    Err(AppError::msg(format!(
        "TCP {host}:{port} недоступен ({last_error}). Проверьте, что телефон в той же Wi-Fi/LAN-сети, укажите LAN-IP NAS и порт 445."
    )))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadResult {
    pub downloaded: usize,
    pub skipped: usize,
    pub failed: usize,
    pub bytes: u64,
    pub destination: String,
    pub errors: Vec<String>,
}

#[tauri::command]
pub async fn download_gallery(
    app: AppHandle,
    state: State<'_, AppState>,
    remote_path: String,
    conflict_policy: Option<String>,
    on_progress: Channel<SyncProgress>,
) -> AppResult<DownloadResult> {
    let cfg = state.config();
    let password = state.password()?;
    let effective =
        zt_proxy::effective_smb_config(&app, &cfg.smb, &cfg.zerotier_network_id).await?;
    let client = state.inner.smb.connect(&effective, &password).await?;
    let root = normalize_rel(&remote_path);
    let root_name = remote_file_name(root.rsplit('/').next().unwrap_or("Gallery"));

    let mut queue = VecDeque::from([root.clone()]);
    let mut files = Vec::new();
    let mut folders = Vec::new();
    while let Some(dir) = queue.pop_front() {
        match state.inner.smb.list_dir(&client, "", &dir).await {
            Ok(entries) => {
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
                if dir == root {
                    return Err(error);
                }
            }
        }
    }

    #[cfg(not(target_os = "android"))]
    let destination = if !cfg.download_auto_save || cfg.download_dir.trim().is_empty() {
        use tauri_plugin_dialog::DialogExt;
        let app_for_dialog = app.clone();
        let picked = tokio::task::spawn_blocking(move || {
            app_for_dialog.dialog().file().blocking_pick_folder()
        })
        .await
        .map_err(|error| AppError::msg(error.to_string()))?;
        picked
            .and_then(|folder| folder.into_path().ok())
            .ok_or_else(|| AppError::msg("Скачивание отменено"))?
    } else {
        PathBuf::from(cfg.download_dir.trim())
    };

    #[cfg(target_os = "android")]
    let destination = PathBuf::from(format!("Download/ZeroSMB/{root_name}"));

    #[cfg(not(target_os = "android"))]
    {
        std::fs::create_dir_all(&destination)?;
        if cfg.download_keep_structure {
            let base = destination.join(&root_name);
            std::fs::create_dir_all(&base)?;
            for folder in &folders {
                if let Some(relative) = folder.strip_prefix(&root) {
                    let relative =
                        relative.trim_start_matches(|ch| ch == '/' || ch == '\\');
                    if !relative.is_empty() {
                        std::fs::create_dir_all(base.join(relative))?;
                    }
                }
            }
        }
    }

    let total = files.len();
    android_progress::update(
        &app,
        "Скачивание с NAS",
        &format!("Файлов: {total}"),
        0,
        total,
        false,
    )
    .await;
    let mut result = DownloadResult {
        downloaded: 0,
        skipped: 0,
        failed: 0,
        bytes: 0,
        destination: destination.to_string_lossy().to_string(),
        errors: Vec::new(),
    };
    let temp_dir = app
        .path()
        .app_cache_dir()
        .map_err(|error| AppError::msg(error.to_string()))?
        .join("downloads");
    for (index, file) in files.iter().enumerate() {
        let relative = file
            .strip_prefix(&root)
            .unwrap_or(file)
            .trim_start_matches(|ch| ch == '/' || ch == '\\')
            .to_string();
        let outcome: AppResult<Option<u64>> = async {
            #[cfg(not(target_os = "android"))]
            {
                let mut target = if cfg.download_keep_structure {
                    destination.join(&root_name).join(&relative)
                } else {
                    let name = relative.rsplit('/').next().unwrap_or(&relative);
                    destination.join(name)
                };
                if target.exists() {
                    if conflict_policy.as_deref().unwrap_or("skip") == "rename" {
                        target = unique_local_path(&target);
                    } else {
                        return Ok(None);
                    }
                }
                let bytes = state.inner.smb.download_file(&client, file, &target).await?;
                Ok(Some(bytes))
            }
            #[cfg(target_os = "android")]
            {
                std::fs::create_dir_all(&temp_dir)?;
                let name = remote_file_name(&relative);
                let temp = temp_dir.join(format!("{}-{name}", uuid::Uuid::new_v4()));
                let bytes = state.inner.smb.download_file(&client, file, &temp).await?;
                let parent = std::path::Path::new(&relative)
                    .parent()
                    .map(|path| path.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_default();
                let relative_dir = if parent.is_empty() {
                    root_name.clone()
                } else {
                    format!("{root_name}/{parent}")
                };
                let mime = mime_guess::from_path(&name)
                    .first_or_octet_stream()
                    .to_string();
                let is_media = mime.starts_with("image/") || mime.starts_with("video/");
                let command = if is_media {
                    "saveToGallery"
                } else {
                    "saveToDownloads"
                };
                let value: Value = android_bridge::call(
                    &app,
                    command,
                    serde_json::json!({
                        "path": temp.to_string_lossy(),
                        "name": name,
                        "mime": mime,
                        "relativePath": relative_dir,
                        "conflictPolicy": conflict_policy.as_deref().unwrap_or("skip"),
                    }),
                )
                .await?;
                let _ = tokio::fs::remove_file(&temp).await;
                if value
                    .get("skipped")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    Ok(None)
                } else {
                    Ok(Some(bytes))
                }
            }
        }
        .await;
        match outcome {
            Ok(Some(bytes)) => {
                result.downloaded += 1;
                result.bytes += bytes;
            }
            Ok(None) => {
                result.skipped += 1;
            }
            Err(error) => {
                result.failed += 1;
                result.errors.push(format!("{relative}: {error}"));
            }
        }
        let _ = on_progress.send(SyncProgress {
            phase: "download".to_string(),
            current: index + 1,
            total,
            file: relative,
            bytes: result.bytes,
            skipped: result.skipped,
            failed: result.failed,
            message: result.errors.last().cloned(),
        });
        android_progress::update(
            &app,
            "Скачивание с NAS",
            &result
                .errors
                .last()
                .cloned()
                .unwrap_or_else(|| format!("Скачано: {}", result.downloaded)),
            index + 1,
            total,
            false,
        )
        .await;
    }
    android_progress::update(
        &app,
        "Скачивание завершено",
        &format!("Скачано: {} из {total}", result.downloaded),
        total,
        total,
        true,
    )
    .await;
    crate::integration::notify(
        &app,
        if result.failed > 0 { "error" } else { "download" },
        "Скачивание с NAS",
        &format!(
            "Скачано: {}, пропущено: {}, ошибок: {}",
            result.downloaded, result.skipped, result.failed
        ),
    );
    let _ = on_progress.send(SyncProgress {
        phase: "done".to_string(),
        current: total,
        total,
        file: String::new(),
        bytes: result.bytes,
        skipped: result.skipped,
        failed: result.failed,
        message: None,
    });
    Ok(result)
}
#[tauri::command]
pub async fn probe_connection(smb: SmbConfig) -> AppResult<ProbeResult> {
    probe_tcp(&smb).await
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteResult {
    pub deleted: usize,
    pub failed: usize,
    pub errors: Vec<String>,
}

#[tauri::command]
pub async fn delete_paths(
    app: AppHandle,
    state: State<'_, AppState>,
    paths: Vec<String>,
) -> AppResult<DeleteResult> {
    let cfg = state.config();
    let password = state.password()?;
    let effective =
        zt_proxy::effective_smb_config(&app, &cfg.smb, &cfg.zerotier_network_id).await?;
    let client = state.inner.smb.connect(&effective, &password).await?;
    let mut result = DeleteResult {
        deleted: 0,
        failed: 0,
        errors: Vec::new(),
    };
    for path in paths {
        let relative = normalize_rel(&path);
        if relative.is_empty() {
            result.failed += 1;
            result.errors.push("Пустой путь".to_string());
            continue;
        }
        match state.inner.smb.delete_tree(&client, &relative).await {
            Ok(()) => result.deleted += 1,
            Err(error) => {
                result.failed += 1;
                result.errors.push(format!("{relative}: {error}"));
            }
        }
    }
    Ok(result)
}
#[tauri::command]
pub async fn create_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> AppResult<()> {
    let cfg = state.config();
    let password = state.password()?;
    let effective =
        zt_proxy::effective_smb_config(&app, &cfg.smb, &cfg.zerotier_network_id).await?;
    let client = state.inner.smb.connect(&effective, &password).await?;
    let relative = normalize_rel(&path);
    if relative.is_empty() {
        return Err(AppError::msg("Пустое имя папки"));
    }
    state.inner.smb.create_dir_path(&client, &relative).await
}