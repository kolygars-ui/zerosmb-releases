#![cfg(not(target_os = "android"))]

use crate::error::{AppError, AppResult};
use crate::zerotier::{ZtNetwork, ZtStatus};
use std::ffi::{CStr, CString};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::raw::{c_char, c_int, c_uint, c_void};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use std::time::Duration;

const ZTS_AF_INET: c_int = 2;
const ZTS_SOCK_STREAM: c_int = 1;
const ZTS_STORE_DATA_LEN: usize = 4096;

extern "C" {
    fn zts_node_start() -> c_int;
    fn zts_node_stop() -> c_int;
    fn zts_node_is_online() -> c_int;
    fn zts_node_get_id() -> u64;
    fn zts_init_from_storage(path: *const c_char) -> c_int;
    fn zts_init_set_roots(data: *const c_void, len: c_uint) -> c_int;
    fn zts_init_allow_roots_cache(allowed: c_uint) -> c_int;
    fn zts_init_set_port(port: u16) -> c_int;
    fn zts_net_join(net_id: u64) -> c_int;
    fn zts_net_leave(net_id: u64) -> c_int;
    fn zts_net_transport_is_ready(net_id: u64) -> c_int;
    fn zts_addr_get_str(net_id: u64, family: c_uint, dst: *mut c_char, len: c_uint) -> c_int;
    fn zts_bsd_socket(family: c_int, type_: c_int, protocol: c_int) -> c_int;
    fn zts_connect(fd: c_int, ipstr: *const c_char, port: u16, timeout_ms: c_int) -> c_int;
    fn zts_set_blocking(fd: c_int, blocking: c_int) -> c_int;
    fn zts_set_no_delay(fd: c_int, enabled: c_int) -> c_int;
    fn zts_bsd_read(fd: c_int, buf: *mut c_void, len: usize) -> isize;
    fn zts_bsd_write(fd: c_int, buf: *const c_void, len: usize) -> isize;
    fn zts_bsd_shutdown(fd: c_int, how: c_int) -> c_int;
    fn zts_bsd_close(fd: c_int) -> c_int;
}

#[derive(Default)]
struct NodeState {
    started: bool,
    network: u64,
    roots: Option<String>,
    custom_roots_seen: bool,
    node_id: u64,
    ipv4: String,
}

struct ProxyState {
    port: u16,
    host: String,
    target_port: u16,
    stop: Arc<AtomicBool>,
}

#[derive(Clone)]
pub struct ZtNode {
    state: Arc<Mutex<NodeState>>,
    proxy: Arc<Mutex<Option<ProxyState>>>,
}

impl ZtNode {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(NodeState::default())),
            proxy: Arc::new(Mutex::new(None)),
        }
    }

    pub fn probe(&self, storage: &Path) -> AppResult<String> {
        std::fs::create_dir_all(storage)?;
        let storage_c = CString::new(storage.to_string_lossy().as_bytes().to_vec())
            .map_err(|_| AppError::msg("Некорректный путь хранилища ZeroTier"))?;
        let result = unsafe { zts_init_from_storage(storage_c.as_ptr()) };
        if result < 0 {
            return Err(AppError::msg(format!(
                "ZeroTier: init_from_storage ({result})"
            )));
        }
        let result = unsafe { zts_node_start() };
        if result < 0 {
            return Err(AppError::msg(format!("ZeroTier: node_start ({result})")));
        }
        let id = unsafe { zts_node_get_id() };
        unsafe {
            zts_node_stop();
        }
        Ok(format!("{:010x}", id))
    }
    pub fn ensure(
        &self,
        network_hex: &str,
        roots: Option<&Path>,
        storage: &Path,
        require_ready: bool,
    ) -> AppResult<ZtStatus> {
        let network = parse_network_id(network_hex)?;
        let roots_key = roots.map(|path| path.to_string_lossy().to_string());
        let mut state = self
            .state
            .lock()
            .map_err(|_| AppError::msg("ZeroTier state lock poisoned"))?;

        if state.started && state.roots != roots_key {
            if state.roots.is_some() && roots_key.is_none() {
                return Err(AppError::msg(
                    "Для возврата встроенных корней ZeroTier перезапустите приложение",
                ));
            }
            self.stop_proxy();
            unsafe {
                zts_node_stop();
            }
            state.started = false;
            state.network = 0;
            state.roots = None;
        }

        if !roots_key.is_some() && state.custom_roots_seen {
            return Err(AppError::msg(
                "��� ������ ���஥���� ��୥� ZeroTier ��१������ �ਫ������",
            ));
        }

        if state.started && unsafe { zts_node_is_online() } != 1 {
            self.stop_proxy();
            unsafe {
                zts_node_stop();
            }
            state.started = false;
            state.network = 0;
            state.roots = None;
            state.ipv4.clear();
        }

        if state.started
            && state.network != 0
            && state.network == network
            && unsafe { zts_net_transport_is_ready(network) } != 1
        {
            unsafe {
                zts_net_leave(network);
            }
            state.network = 0;
            state.ipv4.clear();
        }

        if !state.started {
            std::fs::create_dir_all(storage)?;
            let storage_c = CString::new(storage.to_string_lossy().as_bytes().to_vec())
                .map_err(|_| AppError::msg("Некорректный путь хранилища ZeroTier"))?;
            let result = unsafe { zts_init_from_storage(storage_c.as_ptr()) };
            if result < 0 {
                return Err(AppError::msg(format!(
                    "ZeroTier: init_from_storage ({result})"
                )));
            }
            if let Some(path) = roots {
                let data = std::fs::read(path)?;
                if data.is_empty() || data.len() > ZTS_STORE_DATA_LEN {
                    return Err(AppError::msg(format!(
                        "ZeroTier: некорректный planet (размер {})",
                        data.len()
                    )));
                }
                let result = unsafe {
                    zts_init_set_roots(data.as_ptr() as *const c_void, data.len() as c_uint)
                };
                if result != 0 {
                    return Err(AppError::msg(format!("ZeroTier: set_roots ({result})")));
                }
                unsafe {
                    zts_init_allow_roots_cache(0);
                }
                state.custom_roots_seen = true;
            }
            unsafe {
                zts_init_set_port(9994);
            }
            let result = unsafe { zts_node_start() };
            if result < 0 {
                return Err(AppError::msg(format!("ZeroTier: node_start ({result})")));
            }
            state.started = true;
            state.roots = roots_key;
        }

        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        while unsafe { zts_node_is_online() } != 1 {
            if std::time::Instant::now() >= deadline {
                return Err(AppError::msg(
                    "ZeroTier: нет соединения с корневыми серверами",
                ));
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        if state.network != network {
            let result = unsafe { zts_net_join(network) };
            if result < 0 {
                return Err(AppError::msg(format!("ZeroTier: net_join ({result})")));
            }
            state.network = network;
        }

        let deadline = std::time::Instant::now() + Duration::from_secs(25);
        while unsafe { zts_net_transport_is_ready(network) } != 1 {
            if std::time::Instant::now() >= deadline {
                if require_ready {
                    return Err(AppError::msg(
                        "ZeroTier: сеть не готова. Авторизуйте устройство в ZeroTier Central.",
                    ));
                }
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        state.node_id = unsafe { zts_node_get_id() };
        state.ipv4 = read_ipv4(network);
        Ok(status_from(&state))
    }

    pub fn leave(&self) -> AppResult<ZtStatus> {
        self.stop_proxy();
        let mut state = self
            .state
            .lock()
            .map_err(|_| AppError::msg("ZeroTier state lock poisoned"))?;
        if state.started && state.network != 0 {
            unsafe {
                zts_net_leave(state.network);
            }
            state.network = 0;
            state.ipv4.clear();
        }
        Ok(status_from(&state))
    }

    pub fn stop(&self) {
        self.stop_proxy();
        if let Ok(mut state) = self.state.lock() {
            if state.started {
                unsafe {
                    zts_node_stop();
                }
            }
            state.started = false;
            state.network = 0;
            state.roots = None;
            state.ipv4.clear();
        }
    }

    pub fn start_proxy(&self, host: &str, target_port: u16) -> AppResult<u16> {
        let mut guard = self
            .proxy
            .lock()
            .map_err(|_| AppError::msg("ZeroTier proxy lock poisoned"))?;
        if let Some(proxy) = guard.as_ref() {
            if proxy.host == host && proxy.target_port == target_port {
                return Ok(proxy.port);
            }
        }
        if let Some(proxy) = guard.take() {
            proxy.stop.store(true, Ordering::SeqCst);
        }

        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = stop.clone();
        let target_host = host.to_string();
        std::thread::spawn(move || {
            while !stop_thread.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((client, _)) => {
                        let _ = client.set_nonblocking(false);
                        let host = target_host.clone();
                        std::thread::spawn(move || bridge_tcp(client, &host, target_port));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(40));
                    }
                    Err(_) => {
                        if stop_thread.load(Ordering::SeqCst) {
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(20));
                    }
                }
            }
        });
        *guard = Some(ProxyState {
            port,
            host: host.to_string(),
            target_port,
            stop,
        });
        Ok(port)
    }

    fn stop_proxy(&self) {
        if let Ok(mut guard) = self.proxy.lock() {
            if let Some(proxy) = guard.take() {
                proxy.stop.store(true, Ordering::SeqCst);
            }
        }
    }
}

fn parse_network_id(value: &str) -> AppResult<u64> {
    let cleaned = value.trim().trim_start_matches("0x").trim_start_matches("0X");
    if cleaned.len() < 10 || cleaned.len() > 16 || !cleaned.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AppError::msg("Некорректный ZeroTier Network ID"));
    }
    u64::from_str_radix(cleaned, 16)
        .map_err(|_| AppError::msg("Некорректный ZeroTier Network ID"))
}

fn read_ipv4(network: u64) -> String {
    let mut buffer = [0i8; 64];
    let result = unsafe {
        zts_addr_get_str(
            network,
            ZTS_AF_INET as c_uint,
            buffer.as_mut_ptr() as *mut c_char,
            buffer.len() as c_uint,
        )
    };
    if result != 0 {
        return String::new();
    }
    unsafe { CStr::from_ptr(buffer.as_ptr()) }
        .to_string_lossy()
        .to_string()
}

fn status_from(state: &NodeState) -> ZtStatus {
    let online = state.started && unsafe { zts_node_is_online() } == 1;
    let ready = state.started
        && state.network != 0
        && unsafe { zts_net_transport_is_ready(state.network) } == 1;
    let mut networks = Vec::new();
    if state.network != 0 {
        networks.push(ZtNetwork {
            id: format!("{:016x}", state.network),
            name: "ZeroTier (встроенный)".to_string(),
            status: if ready {
                "OK".to_string()
            } else if online {
                "WAITING".to_string()
            } else {
                "OFFLINE".to_string()
            },
            assigned_addresses: if state.ipv4.is_empty() {
                Vec::new()
            } else {
                vec![state.ipv4.clone()]
            },
        });
    }
    ZtStatus {
        available: state.started,
        service_online: online,
        version: Some("libzt embedded".to_string()),
        address: if state.node_id == 0 {
            None
        } else {
            Some(format!("{:010x}", state.node_id))
        },
        networks,
        message: if ready {
            format!("ZeroTier подключён (встроенный libzt). IP: {}", state.ipv4)
        } else if online {
            "ZeroTier онлайн, ожидание сети или авторизации в ZeroTier Central".to_string()
        } else if state.started {
            "ZeroTier запускается…".to_string()
        } else {
            "ZeroTier не запущен".to_string()
        },
        android: false,
    }
}

fn bridge_tcp(client: TcpStream, host: &str, port: u16) {
    let fd = unsafe { zts_bsd_socket(ZTS_AF_INET, ZTS_SOCK_STREAM, 0) };
    if fd < 0 {
        return;
    }
    let host_c = match CString::new(host) {
        Ok(value) => value,
        Err(_) => {
            unsafe {
                zts_bsd_close(fd);
            }
            return;
        }
    };
    let result = unsafe { zts_connect(fd, host_c.as_ptr(), port, 15000) };
    if result < 0 {
        unsafe {
            zts_bsd_close(fd);
        }
        return;
    }
    unsafe {
        zts_set_blocking(fd, 1);
        zts_set_no_delay(fd, 1);
    }
    let _ = client.set_nodelay(true);
    let mut client_write = match client.try_clone() {
        Ok(value) => value,
        Err(_) => {
            unsafe {
                zts_bsd_close(fd);
            }
            return;
        }
    };
    let mut client_read = client;
    let writer = std::thread::spawn(move || {
        local_to_zt(&mut client_write, fd);
        unsafe {
            zts_bsd_shutdown(fd, 2);
        }
        let _ = client_write.shutdown(std::net::Shutdown::Both);
    });
    zt_to_local(fd, &mut client_read);
    let _ = client_read.shutdown(std::net::Shutdown::Both);
    unsafe {
        zts_bsd_shutdown(fd, 2);
    }
    let _ = writer.join();
    unsafe {
        zts_bsd_close(fd);
    }
}

fn local_to_zt(client: &mut TcpStream, fd: c_int) {
    let mut buffer = vec![0u8; 32768];
    loop {
        let read = match client.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(value) => value,
        };
        let mut offset = 0;
        while offset < read {
            let written = unsafe {
                zts_bsd_write(
                    fd,
                    buffer[offset..read].as_ptr() as *const c_void,
                    read - offset,
                )
            };
            if written <= 0 {
                return;
            }
            offset += written as usize;
        }
    }
}

fn zt_to_local(fd: c_int, client: &mut TcpStream) {
    let mut buffer = vec![0u8; 32768];
    loop {
        let read = unsafe {
            zts_bsd_read(fd, buffer.as_mut_ptr() as *mut c_void, buffer.len())
        };
        if read <= 0 {
            break;
        }
        if client.write_all(&buffer[..read as usize]).is_err() {
            break;
        }
        let _ = client.flush();
    }
}

pub fn planet_default_path(data_dir: &Path) -> PathBuf {
    data_dir.join("planet")
}
