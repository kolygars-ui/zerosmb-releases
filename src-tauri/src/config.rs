use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmbConfig {
    pub host: String,
    pub share: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub domain: String,
    pub username: String,
    #[serde(default)]
    pub base_path: String,
}

fn default_port() -> u16 {
    445
}

impl Default for SmbConfig {
    fn default() -> Self {
        Self {
            host: String::new(),
            share: String::new(),
            port: 445,
            domain: String::new(),
            username: String::new(),
            base_path: String::new(),
        }
    }
}

impl SmbConfig {
    pub fn account(&self) -> String {
        format!("{}|{}@{}", self.domain, self.username, self.host)
    }

    pub fn legacy_account(&self) -> String {
        format!(
            "{}|{}@{}|{}",
            self.domain, self.username, self.host, self.share
        )
    }

    pub fn auth_user(&self) -> String {
        if self.domain.trim().is_empty() {
            self.username.clone()
        } else {
            format!("{}\\{}", self.domain.trim(), self.username)
        }
    }

    pub fn normalized_host(&self) -> String {
        let mut host = self.host.trim().to_string();
        for prefix in ["smb://", "cifs://", "\\\\"] {
            if let Some(rest) = host.strip_prefix(prefix) {
                host = rest.to_string();
                break;
            }
        }
        host = host
            .trim_matches(|ch| ch == '/' || ch == '\\')
            .to_string();
        if let Some((head, _tail)) = host.split_once('/') {
            host = head.to_string();
        }
        host
    }

    pub fn normalized_base(&self) -> String {
        self.base_path
            .replace('\\', "/")
            .trim_matches('/')
            .to_string()
    }

    pub fn validate(&self) -> AppResult<()> {
        if self.host.trim().is_empty() {
            return Err(AppError::msg("Укажите адрес NAS (host/IP)"));
        }
        if self.username.trim().is_empty() {
            return Err(AppError::msg("Укажите имя пользователя SMB"));
        }
        if self.port == 0 {
            return Err(AppError::msg("Некорректный порт SMB"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub enum SyncSource {
    LocalPath(String),
    AndroidTree(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncJob {
    pub id: String,
    pub label: String,
    pub source: SyncSource,
    pub remote_path: String,
    #[serde(default = "default_sync_mode")]
    pub mode: String,
    #[serde(default = "default_sync_interval")]
    pub interval_minutes: u32,
    #[serde(default)]
    pub delete_enabled: bool,
    #[serde(default)]
    pub last_sync: i64,
}

fn default_sync_mode() -> String {
    "upload_manual".to_string()
}

fn default_sync_interval() -> u32 {
    15
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    #[serde(default)]
    pub setup_complete: bool,
    #[serde(default)]
    pub zerotier_network_id: String,
    #[serde(default)]
    pub connection_mode: String,
    #[serde(default)]
    pub zerotier_mode: String,
    #[serde(default)]
    pub smb: SmbConfig,
    #[serde(default)]
    pub sync_jobs: Vec<SyncJob>,
    #[serde(default)]
    pub gallery_share: String,
    #[serde(default = "default_gallery_path")]
    pub remote_gallery_path: String,
    #[serde(default = "default_true")]
    pub gallery_enabled: bool,
    #[serde(default = "default_gallery_columns")]
    pub gallery_columns: u8,
    #[serde(default = "default_file_name_scale")]
    pub file_name_scale: u8,
    #[serde(default = "default_file_view_mode")]
    pub file_view_mode: String,
    #[serde(default = "default_true")]
    pub tree_show_files: bool,
    #[serde(default = "default_tree_font_scale")]
    pub tree_font_scale: u8,
    #[serde(default)]
    pub theme_color: String,
    #[serde(default = "default_true")]
    pub external_documents: bool,
    #[serde(default = "default_true")]
    pub external_save_enabled: bool,
    #[serde(default = "default_external_save_mode")]
    pub external_save_mode: String,
    #[serde(default)]
    pub download_dir: String,
    #[serde(default = "default_diagnostics_path")]
    pub diagnostics_path: String,
    #[serde(default = "default_true")]
    pub download_auto_save: bool,
    #[serde(default = "default_true")]
    pub download_keep_structure: bool,
    #[serde(default)]
    pub system_install_dir: String,
    #[serde(default)]
    pub system_autostart: bool,
    #[serde(default = "default_true")]
    pub system_start_menu: bool,
    #[serde(default)]
    pub system_desktop_shortcut: bool,
    #[serde(default = "default_true")]
    pub system_tray: bool,
    #[serde(default = "default_true")]
    pub system_notifications: bool,
    #[serde(default)]
    pub wireguard_config_path: String,
    #[serde(default)]
    pub wireguard_enabled: bool,
    #[serde(default)]
    pub ai_enabled: bool,
    #[serde(default)]
    pub ai_allow_download: bool,
    #[serde(default)]
    pub ai_allow_modify: bool,
    #[serde(default)]
    pub ai_allow_upload: bool,
    #[serde(default)]
    pub ai_allow_mkdir: bool,
    #[serde(default)]
    pub ai_allow_delete: bool,
    #[serde(default)]
    pub ai_deny_paths: Vec<String>,
    #[serde(default = "default_true")]
    pub notify_downloads: bool,
    #[serde(default = "default_true")]
    pub notify_uploads: bool,
    #[serde(default = "default_true")]
    pub notify_sync: bool,
    #[serde(default = "default_true")]
    pub notify_errors: bool,
    #[serde(default)]
    pub android_tree_uri: Option<String>,
    #[serde(default = "default_update_repo")]
    pub update_repo: String,
    #[serde(default)]
    pub custom_roots: bool,
}

fn default_gallery_path() -> String {
    "Gallery".to_string()
}

fn default_diagnostics_path() -> String {
    "Diagnostics".to_string()
}

fn default_true() -> bool {
    true
}

fn default_zerotier_mode() -> String {
    if cfg!(target_os = "android") {
        "embedded".to_string()
    } else {
        "system".to_string()
    }
}

fn default_gallery_columns() -> u8 {
    3
}

fn default_file_name_scale() -> u8 {
    100
}

fn default_file_view_mode() -> String {
    "list".to_string()
}

fn default_tree_font_scale() -> u8 {
    100
}

fn default_external_save_mode() -> String {
    "button".to_string()
}

pub fn normalize_theme_color(value: &str) -> String {
    let value = value.trim();
    if value.len() == 7
        && value.starts_with('#')
        && value[1..].chars().all(|ch| ch.is_ascii_hexdigit())
    {
        value.to_ascii_lowercase()
    } else {
        String::new()
    }
}

pub fn normalize_connection_mode(mode: &str) -> Option<&'static str> {
    match mode.trim().to_ascii_lowercase().as_str() {
        "local" => Some("local"),
        "zerotier" => Some("zerotier"),
        "wireguard" => Some("wireguard"),
        _ => None,
    }
}

pub fn normalize_zerotier_mode(mode: &str) -> Option<&'static str> {
    match mode.trim().to_ascii_lowercase().as_str() {
        "embedded" => Some("embedded"),
        "system" => Some("system"),
        _ => None,
    }
}

pub fn suggested_gallery_path(smb: &SmbConfig) -> String {
    let user = smb
        .username
        .trim()
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or("")
        .trim()
        .trim_matches(|ch| ch == '\\' || ch == '/');
    if user.is_empty() {
        "Галерея".to_string()
    } else {
        format!("{user}/Галерея")
    }
}

fn default_update_repo() -> String {
    "kolygars-ui/zerosmb-releases".to_string()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            setup_complete: false,
            zerotier_network_id: String::new(),
            connection_mode: String::new(),
            zerotier_mode: default_zerotier_mode(),
            smb: SmbConfig::default(),
            sync_jobs: Vec::new(),
            gallery_share: String::new(),
            remote_gallery_path: default_gallery_path(),
            gallery_enabled: true,
            gallery_columns: default_gallery_columns(),
            file_name_scale: default_file_name_scale(),
            file_view_mode: default_file_view_mode(),
            tree_show_files: true,
            tree_font_scale: default_tree_font_scale(),
            theme_color: String::new(),
            external_documents: true,
            external_save_enabled: true,
            external_save_mode: default_external_save_mode(),
            download_dir: String::new(),
            diagnostics_path: default_diagnostics_path(),
            download_auto_save: true,
            download_keep_structure: true,
            system_install_dir: String::new(),
            system_autostart: false,
            system_start_menu: true,
            system_desktop_shortcut: false,
            system_tray: true,
            system_notifications: true,
            wireguard_config_path: String::new(),
            wireguard_enabled: false,
            ai_enabled: false,
            ai_allow_download: false,
            ai_allow_modify: false,
            ai_allow_upload: false,
            ai_allow_mkdir: false,
            ai_allow_delete: false,
            ai_deny_paths: Vec::new(),
            notify_downloads: true,
            notify_uploads: true,
            notify_sync: true,
            notify_errors: true,
            android_tree_uri: None,
            update_repo: default_update_repo(),
            custom_roots: false,
        }
    }
}

impl AppConfig {
    pub fn resolved_connection_mode(&self) -> &'static str {
        if let Some(mode) = normalize_connection_mode(&self.connection_mode) {
            return mode;
        }
        if self.wireguard_enabled {
            "wireguard"
        } else if !self.zerotier_network_id.trim().is_empty() {
            "zerotier"
        } else {
            "local"
        }
    }

    pub fn resolved_zerotier_mode(&self) -> &'static str {
        normalize_zerotier_mode(&self.zerotier_mode).unwrap_or_else(|| {
            if cfg!(target_os = "android") {
                "embedded"
            } else {
                "system"
            }
        })
    }

    pub fn load(dir: &Path) -> Self {
        let path = dir.join("config.json");
        std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) -> AppResult<()> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join("config.json");
        let tmp = dir.join("config.json.tmp");
        let bytes = serde_json::to_vec_pretty(self)?;
        std::fs::write(&tmp, &bytes)?;
        restrict_permissions(&tmp);
        std::fs::rename(&tmp, &path)?;
        restrict_permissions(&path);
        Ok(())
    }
}

fn restrict_permissions(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

fn secrets_path(dir: &Path) -> PathBuf {
    dir.join("secrets.json")
}

#[derive(Serialize, Deserialize)]
struct SecretsFile {
    password: String,
}

pub fn load_password(dir: &Path, smb: &SmbConfig) -> Option<String> {
    #[cfg(not(target_os = "android"))]
    {
        let current = smb.account();
        let legacy = smb.legacy_account();
        for account in [&current, &legacy] {
            if let Ok(entry) = keyring::Entry::new("ZeroSMB", account) {
                if let Ok(password) = entry.get_password() {
                    if !password.is_empty() {
                        if account == &legacy {
                            if let Ok(migrated) = keyring::Entry::new("ZeroSMB", &current) {
                                let _ = migrated.set_password(&password);
                            }
                        }
                        return Some(password);
                    }
                }
            }
        }
    }

    let bytes = std::fs::read(secrets_path(dir)).ok()?;
    let file: SecretsFile = serde_json::from_slice(&bytes).ok()?;
    if file.password.is_empty() {
        None
    } else {
        Some(file.password)
    }
}

pub fn save_password(dir: &Path, smb: &SmbConfig, password: &str) -> AppResult<()> {
    #[cfg(not(target_os = "android"))]
    {
        if let Ok(entry) = keyring::Entry::new("ZeroSMB", &smb.account()) {
            if entry.set_password(password).is_ok() {
                let _ = std::fs::remove_file(secrets_path(dir));
                return Ok(());
            }
        }
    }

    std::fs::create_dir_all(dir)?;
    let path = secrets_path(dir);
    let tmp = dir.join("secrets.json.tmp");
    let bytes = serde_json::to_vec(&SecretsFile {
        password: password.to_string(),
    })?;
    std::fs::write(&tmp, &bytes)?;
    restrict_permissions(&tmp);
    std::fs::rename(&tmp, &path)?;
    restrict_permissions(&path);
    Ok(())
}

pub fn clear_password(dir: &Path, smb: &SmbConfig) {
    #[cfg(not(target_os = "android"))]
    {
        for account in [smb.account(), smb.legacy_account()] {
            if let Ok(entry) = keyring::Entry::new("ZeroSMB", &account) {
                let _ = entry.delete_credential();
            }
        }
    }
    let _ = std::fs::remove_file(secrets_path(dir));
}

pub fn config_dir(app: &tauri::AppHandle) -> AppResult<PathBuf> {
    use tauri::Manager;
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| AppError::msg(format!("Не удалось получить каталог настроек: {e}")))?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}
