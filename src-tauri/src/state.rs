use crate::config::{self, AppConfig, SmbConfig};
use crate::error::{AppError, AppResult};
use crate::smb_service::SmbService;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use zeroize::Zeroize;

pub struct Inner {
    pub config: RwLock<AppConfig>,
    pub password: RwLock<Option<String>>,
    pub data_dir: RwLock<Option<PathBuf>>,
    pub smb: SmbService,
    pub media_smb: SmbService,
    pub media_lock: tokio::sync::Mutex<()>,
    pub smb_lock: tokio::sync::Mutex<()>,
    pub sync_lock: tokio::sync::Mutex<()>,
    pub wireguard: crate::wireguard::WireGuardNode,
    pub ai: crate::ai_api::AiServer,
    pub transfers: crate::transfer::TransferManager,
    pub local_media: Mutex<HashMap<String, PathBuf>>,
    #[cfg(not(target_os = "android"))]
    pub zt: crate::zerotier_embedded::ZtNode,
}

#[derive(Clone)]
pub struct AppState {
    pub inner: Arc<Inner>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                config: RwLock::new(AppConfig::default()),
                password: RwLock::new(None),
                data_dir: RwLock::new(None),
                smb: SmbService::new(),
                media_smb: SmbService::new(),
                media_lock: tokio::sync::Mutex::new(()),
                smb_lock: tokio::sync::Mutex::new(()),
                sync_lock: tokio::sync::Mutex::new(()),
                wireguard: crate::wireguard::WireGuardNode::new(),
                ai: crate::ai_api::AiServer::new(),
                transfers: crate::transfer::TransferManager::new(),
                local_media: Mutex::new(HashMap::new()),
                #[cfg(not(target_os = "android"))]
                zt: crate::zerotier_embedded::ZtNode::new(),
            }),
        }
    }

    pub fn init(&self, app: &tauri::AppHandle) -> AppResult<()> {
        let dir = config::config_dir(app)?;
        let cfg = AppConfig::load(&dir);
        let password = config::load_password(&dir, &cfg.smb);
        *self.inner.config.write().unwrap() = cfg;
        *self.inner.password.write().unwrap() = password;
        *self.inner.data_dir.write().unwrap() = Some(dir);
        Ok(())
    }

    pub fn config(&self) -> AppConfig {
        self.inner.config.read().unwrap().clone()
    }

    pub fn smb_config(&self) -> SmbConfig {
        self.inner.config.read().unwrap().smb.clone()
    }

    pub fn password(&self) -> AppResult<String> {
        self.inner
            .password
            .read()
            .unwrap()
            .clone()
            .ok_or_else(|| AppError::msg("Нет сохранённого пароля SMB. Выполните вход в настройках."))
    }

    pub fn set_password(&self, password: Option<String>) {
        let mut guard = self.inner.password.write().unwrap();
        if let Some(mut old) = guard.take() {
            old.zeroize();
        }
        *guard = password;
    }

    pub fn data_dir(&self) -> AppResult<PathBuf> {
        self.inner
            .data_dir
            .read()
            .unwrap()
            .clone()
            .ok_or_else(|| AppError::msg("Приложение ещё не инициализировано"))
    }

    pub fn set_config(&self, cfg: AppConfig) -> AppResult<()> {
        cfg.save(&self.data_dir()?)?;
        *self.inner.config.write().unwrap() = cfg;
        Ok(())
    }

    pub fn update_config(&self, f: impl FnOnce(&mut AppConfig)) -> AppResult<AppConfig> {
        let mut cfg = self.config();
        f(&mut cfg);
        self.set_config(cfg.clone())?;
        Ok(cfg)
    }

    pub fn register_local_media(&self, path: PathBuf) -> String {
        let token = uuid::Uuid::new_v4().to_string();
        let mut guard = self.inner.local_media.lock().unwrap();
        if guard.len() > 512 {
            guard.clear();
        }
        guard.insert(token.clone(), path);
        token
    }

    pub fn local_media(&self, token: &str) -> Option<PathBuf> {
        self.inner.local_media.lock().unwrap().get(token).cloned()
    }

    pub fn planet_file(&self) -> AppResult<PathBuf> {
        Ok(self.data_dir()?.join("planet"))
    }

    pub fn planet_path(&self) -> AppResult<Option<PathBuf>> {
        if !self.config().custom_roots {
            return Ok(None);
        }
        let path = self.planet_file()?;
        if !path.is_file() {
            return Err(AppError::msg(
                "Файл planet не найден. Выберите его в настройках ZeroTier.",
            ));
        }
        Ok(Some(path))
    }

    pub fn planet_configured(&self) -> bool {
        self.planet_file().map(|path| path.is_file()).unwrap_or(false)
    }
}
