use crate::error::{AppError, AppResult};
use crate::state::AppState;
use base64::{engine::general_purpose::STANDARD, Engine};
use onetun::config::{
    Config, PortForwardConfig, PortProtocol, PublicKey, StaticSecret,
};
use onetun::events::Bus;
use onetun::virtual_device::VirtualIpDevice;
use onetun::virtual_iface::tcp::TcpVirtualInterface;
use onetun::virtual_iface::VirtualInterfacePoll;
use onetun::wg::WireGuardTunnel;
use serde::Serialize;
use std::net::{IpAddr, SocketAddr, TcpListener, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, State};
use tokio::task::JoinHandle;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireGuardStatus {
    pub configured: bool,
    pub connected: bool,
    pub config_path: String,
    pub message: String,
}

struct Running {
    port: u16,
    key: String,
    handles: Vec<JoinHandle<()>>,
}

pub struct WireGuardNode {
    running: tokio::sync::Mutex<Option<Running>>,
}

impl WireGuardNode {
    pub fn new() -> Self {
        Self {
            running: tokio::sync::Mutex::new(None),
        }
    }

    pub async fn status(&self, state: &AppState) -> WireGuardStatus {
        let cfg = state.config();
        let configured = !cfg.wireguard_config_path.trim().is_empty()
            && Path::new(cfg.wireguard_config_path.trim()).is_file();
        let running = self.running.lock().await.is_some();
        WireGuardStatus {
            configured,
            connected: running,
            config_path: cfg.wireguard_config_path,
            message: if !configured {
                "Выберите конфигурацию WireGuard (.conf)".to_string()
            } else if running {
                "Встроенный WireGuard-туннель активен".to_string()
            } else {
                "WireGuard-туннель отключён".to_string()
            },
        }
    }

    pub async fn ensure(
        &self,
        state: &AppState,
        host: &str,
        port: u16,
    ) -> AppResult<u16> {
        let cfg = state.config();
        if !cfg.wireguard_enabled {
            return Err(AppError::msg("WireGuard отключён"));
        }
        let config_path = cfg.wireguard_config_path.trim().to_string();
        if config_path.is_empty() {
            return Err(AppError::msg("WireGuard-конфигурация не выбрана"));
        }
        let key = format!("{config_path}|{host}|{port}");
        let mut running = self.running.lock().await;
        if let Some(current) = running.as_ref() {
            if current.key == key {
                return Ok(current.port);
            }
            stop_running(current);
            *running = None;
        }

        let host = host.to_string();
        let path = PathBuf::from(&config_path);
        let config = tokio::task::spawn_blocking(move || parse_config(&path, &host, port))
            .await
            .map_err(|error| AppError::msg(error.to_string()))??;
        let local_port = config.port_forwards[0].source.port();
        let handles = start_tasks(config).await?;
        *running = Some(Running {
            port: local_port,
            key,
            handles,
        });
        Ok(local_port)
    }

    pub async fn stop(&self) {
        let mut running = self.running.lock().await;
        if let Some(current) = running.as_ref() {
            stop_running(current);
        }
        *running = None;
    }
}

fn stop_running(running: &Running) {
    for handle in &running.handles {
        handle.abort();
    }
}

async fn start_tasks(config: Config) -> AppResult<Vec<JoinHandle<()>>> {
    let bus = Bus::default();
    let wg = Arc::new(
        WireGuardTunnel::new(&config, bus.clone())
            .await
            .map_err(|error| AppError::msg(format!("WireGuard: {error}")))?,
    );
    let mut handles = Vec::new();
    {
        let wg = wg.clone();
        handles.push(tokio::spawn(async move { wg.routine_task().await }));
    }
    {
        let wg = wg.clone();
        handles.push(tokio::spawn(async move { wg.consume_task().await }));
    }
    {
        let wg = wg.clone();
        handles.push(tokio::spawn(async move { wg.produce_task().await }));
    }
    let device =
        VirtualIpDevice::new(PortProtocol::Tcp, bus.clone(), config.max_transmission_unit);
    let iface = TcpVirtualInterface::new(
        config.port_forwards.clone(),
        bus.clone(),
        config.source_peer_ip,
    );
    handles.push(tokio::spawn(async move {
        let _ = iface.poll_loop(device).await;
    }));
    let forward = config.port_forwards[0].clone();
    let source_ip = config.source_peer_ip;
    handles.push(tokio::spawn(async move {
        let tcp_pool = onetun::tunnel::tcp::TcpPortPool::new();
        let udp_pool = onetun::tunnel::udp::UdpPortPool::new();
        let _ = onetun::tunnel::port_forward(
            forward,
            source_ip,
            tcp_pool,
            udp_pool,
            wg,
            bus,
        )
        .await;
    }));
    Ok(handles)
}

fn parse_config(path: &Path, host: &str, port: u16) -> AppResult<Config> {
    let text = std::fs::read_to_string(path)?;
    let mut private_key = None;
    let mut address = None;
    let mut peer_public_key = None;
    let mut preshared_key = None;
    let mut endpoint = None;
    let mut keepalive = None;
    let mut mtu = 1420usize;
    let mut section = "";
    for raw in text.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            section = line;
            continue;
        }
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        let name = name.trim().to_ascii_lowercase();
        let value = value.trim();
        match (section, name.as_str()) {
            ("[Interface]", "privatekey") => private_key = Some(decode_key(value)?),
            ("[Interface]", "address") => {
                address = value
                    .split(',')
                    .next()
                    .and_then(|value| value.split('/').next())
                    .and_then(|value| value.parse::<IpAddr>().ok());
            }
            ("[Interface]", "mtu") => {
                if let Ok(value) = value.parse::<usize>() {
                    mtu = value.clamp(576, 9216);
                }
            }
            ("[Peer]", "publickey") => peer_public_key = Some(decode_key(value)?),
            ("[Peer]", "presharedkey") => preshared_key = Some(decode_key(value)?),
            ("[Peer]", "endpoint") => endpoint = Some(value.to_string()),
            ("[Peer]", "persistentkeepalive") => {
                keepalive = value.parse::<u16>().ok().filter(|value| *value > 0)
            }
            _ => {}
        }
    }
    let private_key = private_key.ok_or_else(|| AppError::msg("WireGuard: нет PrivateKey"))?;
    let peer_public_key =
        peer_public_key.ok_or_else(|| AppError::msg("WireGuard: нет PublicKey"))?;
    let source_ip = address.ok_or_else(|| AppError::msg("WireGuard: нет Address"))?;
    let endpoint_addr = endpoint
        .ok_or_else(|| AppError::msg("WireGuard: нет Endpoint"))?
        .to_socket_addrs()
        .map_err(|error| AppError::msg(format!("WireGuard Endpoint: {error}")))?
        .next()
        .ok_or_else(|| AppError::msg("WireGuard: Endpoint не разрешён"))?;
    let destination = format!("{host}:{port}")
        .to_socket_addrs()
        .map_err(|error| AppError::msg(format!("SMB host: {error}")))?
        .find(|addr| addr.is_ipv4())
        .ok_or_else(|| AppError::msg("SMB host: нет IPv4 адреса"))?;
    let local_port = free_port()?;
    let bind = if endpoint_addr.is_ipv4() {
        SocketAddr::from(([0, 0, 0, 0], 0))
    } else {
        SocketAddr::from(([0u16; 8], 0))
    };
    Ok(Config {
        port_forwards: vec![PortForwardConfig {
            source: SocketAddr::from(([127, 0, 0, 1], local_port)),
            destination,
            protocol: PortProtocol::Tcp,
            remote: false,
        }],
        remote_port_forwards: Vec::new(),
        private_key: Arc::new(StaticSecret::from(private_key)),
        endpoint_public_key: Arc::new(PublicKey::from(peer_public_key)),
        preshared_key,
        endpoint_addr,
        endpoint_bind_addr: bind,
        source_peer_ip: source_ip,
        keepalive_seconds: keepalive,
        max_transmission_unit: mtu,
        log: "warn".to_string(),
        warnings: Vec::new(),
        pcap_file: None,
    })
}

fn decode_key(value: &str) -> AppResult<[u8; 32]> {
    let bytes = STANDARD
        .decode(value.trim())
        .map_err(|error| AppError::msg(format!("WireGuard key: {error}")))?;
    bytes
        .try_into()
        .map_err(|_| AppError::msg("WireGuard: ключ должен быть 32 байта"))
}

fn free_port() -> AppResult<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

#[tauri::command]
pub async fn wireguard_status(
    _app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<WireGuardStatus> {
    Ok(state.inner.wireguard.status(&state).await)
}

#[derive(serde::Deserialize)]
struct PickedFile {
    path: String,
}

#[derive(serde::Deserialize)]
struct PickedFiles {
    #[serde(default)]
    files: Vec<PickedFile>,
}

#[tauri::command]
pub async fn wireguard_pick_config(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<Option<String>> {
    let source = if cfg!(target_os = "android") {
        let response: PickedFiles =
            crate::android_bridge::call(&app, "pickFiles", serde_json::json!({})).await?;
        response
            .files
            .into_iter()
            .find(|file| file.path.to_ascii_lowercase().ends_with(".conf"))
            .map(|file| PathBuf::from(file.path))
    } else {
        use tauri_plugin_dialog::DialogExt;
        let app_for_dialog = app.clone();
        let picked = tokio::task::spawn_blocking(move || {
            app_for_dialog
                .dialog()
                .file()
                .add_filter("WireGuard", &["conf"])
                .blocking_pick_file()
        })
        .await
        .map_err(|error| AppError::msg(error.to_string()))?;
        picked.and_then(|file| file.into_path().ok())
    };
    let Some(source) = source else {
        return Ok(None);
    };
    let target = state.data_dir()?.join("wireguard").join("zerosmb.conf");
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(source, &target)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600));
    }
    state.inner.wireguard.stop().await;
    let mut cfg = state.config();
    cfg.wireguard_config_path = target.to_string_lossy().to_string();
    cfg.wireguard_enabled = false;
    state.set_config(cfg)?;
    Ok(Some(target.to_string_lossy().to_string()))
}

#[tauri::command]
pub async fn wireguard_connect(
    _app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<String> {
    let mut cfg = state.config();
    if cfg.wireguard_config_path.trim().is_empty() {
        return Err(AppError::msg("Сначала выберите WireGuard-конфигурацию (.conf)"));
    }
    cfg.wireguard_enabled = true;
    cfg.connection_mode = "wireguard".to_string();
    state.set_config(cfg.clone())?;
    let port = state
        .inner
        .wireguard
        .ensure(&state, &cfg.smb.normalized_host(), cfg.smb.port)
        .await?;
    Ok(format!("Встроенный WireGuard-туннель запущен (127.0.0.1:{port})"))
}

#[tauri::command]
pub async fn wireguard_disconnect(state: State<'_, AppState>) -> AppResult<String> {
    state.inner.wireguard.stop().await;
    let mut cfg = state.config();
    cfg.wireguard_enabled = false;
    state.set_config(cfg)?;
    Ok("WireGuard-туннель остановлен".to_string())
}

#[tauri::command]
pub fn wireguard_open_help() -> AppResult<()> {
    #[cfg(not(target_os = "android"))]
    {
        return open::that("https://www.wireguard.com/install/")
            .map_err(|error| AppError::msg(error.to_string()));
    }
    #[cfg(target_os = "android")]
    {
        Ok(())
    }
}
