use crate::config::SmbConfig;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
#[cfg(target_os = "android")]
use serde::Deserialize;
#[cfg(target_os = "android")]
use serde_json::json;
use tauri::{AppHandle, Manager};

#[cfg(target_os = "android")]
#[derive(Deserialize)]
struct ProxyResponse {
    port: u16,
}

/// The embedded libzt node exposes an app-local TCP proxy. SMB connects to
/// 127.0.0.1:<port>, while the proxy forwards the byte stream through the
/// ZeroTier network. This keeps SMB unchanged and works on desktop and Android.
pub async fn effective_smb_config(
    app: &AppHandle,
    cfg: &SmbConfig,
    network_id: &str,
) -> AppResult<SmbConfig> {
    let mode = app
        .state::<AppState>()
        .config()
        .resolved_connection_mode()
        .to_string();
    effective_smb_config_for(app, cfg, &mode, network_id).await
}

pub async fn effective_smb_config_for(
    app: &AppHandle,
    cfg: &SmbConfig,
    mode: &str,
    network_id: &str,
) -> AppResult<SmbConfig> {
    let zerotier_mode = app
        .state::<AppState>()
        .config()
        .resolved_zerotier_mode()
        .to_string();
    effective_smb_config_with_provider(app, cfg, mode, network_id, &zerotier_mode).await
}

pub async fn effective_smb_config_with_provider(
    app: &AppHandle,
    cfg: &SmbConfig,
    mode: &str,
    network_id: &str,
    zerotier_mode: &str,
) -> AppResult<SmbConfig> {
    let state = app.state::<AppState>();
    match mode {
        "wireguard" => {
            let full_config = state.config();
            if !full_config.wireguard_enabled {
                return Err(AppError::msg(
                    "WireGuard выбран, но туннель не подключён",
                ));
            }
            if full_config.wireguard_config_path.trim().is_empty() {
                return Err(AppError::msg("Выберите конфигурацию WireGuard (.conf)"));
            }
            let port = state
                .inner
                .wireguard
                .ensure(&state, &cfg.normalized_host(), cfg.port)
                .await?;
            let mut local = cfg.clone();
            local.host = "127.0.0.1".to_string();
            local.port = port;
            return Ok(local);
        }
        "zerotier" => {
            if zerotier_mode == "system" {
                log::info!(
                    "SMB proxy: using system ZeroTier for {}:{}",
                    cfg.normalized_host(),
                    cfg.port
                );
                return Ok(cfg.clone());
            }
            if network_id.trim().is_empty() {
                return Err(AppError::msg("Укажите ZeroTier Network ID"));
            }
        }
        _ => {
            // Local network mode: use normal OS sockets and the configured host.
            #[cfg(target_os = "android")]
            {
                let _: serde_json::Value = crate::android_bridge::call(
                    app,
                    "ensureLocalNetworkPermission",
                    json!({}),
                )
                .await?;
            }
            return Ok(cfg.clone());
        }
    }
    #[cfg(target_os = "android")]
    {
        let planet_path = state
            .planet_path()?
            .map(|path| path.to_string_lossy().to_string());
        let response: ProxyResponse = crate::android_bridge::call(
            app,
            "startSmbProxy",
            json!({
                "host": cfg.normalized_host(),
                "port": cfg.port,
                "networkId": network_id.trim(),
                "planetPath": planet_path,
            }),
        )
        .await?;
        let mut local = cfg.clone();
        local.host = "127.0.0.1".to_string();
        local.port = response.port;
        Ok(local)
    }
    #[cfg(not(target_os = "android"))]
    {
        let roots = state.planet_path()?;
        let storage = state.data_dir()?.join("zerotier");
        let zt = state.inner.zt.clone();
        let host = cfg.normalized_host();
        let target_port = cfg.port;
        let network = network_id.trim().to_string();
        log::info!(
            "SMB proxy: mode=zerotier target={host}:{target_port} network={network}"
        );
        let port = tokio::task::spawn_blocking(move || -> AppResult<u16> {
            zt.ensure(&network, roots.as_deref(), &storage, true)?;
            zt.start_proxy(&host, target_port)
        })
        .await
        .map_err(|error| AppError::msg(error.to_string()))??;
        log::info!("SMB proxy: ready on 127.0.0.1:{port} -> {cfg_host}:{cfg_port}", cfg_host = cfg.normalized_host(), cfg_port = cfg.port);
        let mut local = cfg.clone();
        local.host = "127.0.0.1".to_string();
        local.port = port;
        Ok(local)
    }
}
