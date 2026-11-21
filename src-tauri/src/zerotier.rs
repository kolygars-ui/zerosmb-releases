use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZtNetwork {
    pub id: String,
    pub name: String,
    pub status: String,
    pub assigned_addresses: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZtStatus {
    pub available: bool,
    pub service_online: bool,
    pub version: Option<String>,
    pub address: Option<String>,
    pub networks: Vec<ZtNetwork>,
    pub message: String,
    pub android: bool,
}

#[cfg(not(target_os = "android"))]
fn token_candidates() -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    if let Ok(home) = std::env::var("ZEROTIER_HOME") {
        out.push(std::path::PathBuf::from(home).join("authtoken.secret"));
    }
    #[cfg(windows)]
    {
        out.push(std::path::PathBuf::from(
            r"C:\ProgramData\ZeroTier\One\authtoken.secret",
        ));
        if let Ok(program_data) = std::env::var("ProgramData") {
            out.push(
                std::path::PathBuf::from(program_data)
                    .join("ZeroTier")
                    .join("One")
                    .join("authtoken.secret"),
            );
        }
    }
    #[cfg(not(windows))]
    {
        out.push(std::path::PathBuf::from(
            "/var/lib/zerotier-one/authtoken.secret",
        ));
        out.push(std::path::PathBuf::from(
            "/var/db/zerotier-one/authtoken.secret",
        ));
    }
    out
}

#[cfg(not(target_os = "android"))]
fn read_token() -> Option<String> {
    token_candidates()
        .into_iter()
        .find_map(|path| std::fs::read_to_string(path).ok())
        .map(|token| token.trim().to_string())
        .filter(|token| !token.is_empty())
}

#[cfg(not(target_os = "android"))]
fn cli_candidates() -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    #[cfg(windows)]
    {
        out.push(std::path::PathBuf::from(
            r"C:\ProgramData\ZeroTier\One\zerotier-cli.bat",
        ));
        out.push(std::path::PathBuf::from("zerotier-cli.bat"));
        out.push(std::path::PathBuf::from("zerotier-cli"));
    }
    #[cfg(not(windows))]
    {
        out.push(std::path::PathBuf::from("/usr/sbin/zerotier-cli"));
        out.push(std::path::PathBuf::from("/usr/bin/zerotier-cli"));
        out.push(std::path::PathBuf::from("zerotier-cli"));
    }
    out
}

#[cfg(not(target_os = "android"))]
async fn zt_http(
    method: &str,
    path: &str,
    token: &str,
    body: Option<&str>,
) -> AppResult<Value> {
    let mut stream = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        tokio::net::TcpStream::connect("127.0.0.1:9993"),
    )
    .await
    .map_err(|_| AppError::msg("ZeroTier: таймаут подключения к локальному сервису"))?
    .map_err(|e| AppError::msg(format!("ZeroTier: сервис не запущен ({e})")))?;

    let body = body.unwrap_or("");
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:9993\r\nX-ZT1-AUTH: {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).await?;
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await?;
    let pos = buf
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| AppError::msg("ZeroTier: некорректный ответ локального API"))?;
    let head = String::from_utf8_lossy(&buf[..pos]);
    let status_line = head.lines().next().unwrap_or_default().to_string();
    let status_code = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .unwrap_or(0);
    let payload = &buf[pos + 4..];
    if status_code >= 400 {
        let text = String::from_utf8_lossy(payload);
        return Err(AppError::msg(format!(
            "ZeroTier API: {status_line} {}",
            text.chars().take(300).collect::<String>()
        )));
    }
    if payload.is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_slice(payload)
        .map_err(|e| AppError::msg(format!("ZeroTier API: ответ не JSON ({e})")))
}

#[cfg(not(target_os = "android"))]
fn find_cli() -> Option<std::path::PathBuf> {
    cli_candidates().into_iter().find(|path| {
        path.is_absolute()
            && path.exists()
            || (!path.is_absolute() && which(path.to_string_lossy().as_ref()))
    })
}

#[cfg(not(target_os = "android"))]
fn which(name: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|dir| {
                let candidate = dir.join(name);
                candidate.exists()
                    || (cfg!(windows)
                        && dir.join(format!("{name}.bat")).exists())
            })
        })
        .unwrap_or(false)
}

#[cfg(not(target_os = "android"))]
async fn run_cli(args: &[&str]) -> AppResult<String> {
    let cli = find_cli().ok_or_else(|| {
        AppError::msg(
            "ZeroTier One не найден. Установите ZeroTier One и запустите сервис.",
        )
    })?;
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    tokio::task::spawn_blocking(move || {
        let output = std::process::Command::new(&cli).args(&args).output()?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if output.status.success() {
            Ok((stdout + &stderr).trim().to_string())
        } else {
            Err(AppError::msg(format!(
                "zerotier-cli: {}",
                (stderr + &stdout).trim()
            )))
        }
    })
    .await
    .map_err(|e| AppError::msg(e.to_string()))?
}

#[cfg(not(target_os = "android"))]
async fn run_cli_elevated(args: &[&str]) -> AppResult<()> {
    let cli = find_cli().ok_or_else(|| {
        AppError::msg(
            "ZeroTier One не найден. Установите ZeroTier One и запустите сервис.",
        )
    })?;
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    tokio::task::spawn_blocking(move || {
        #[cfg(windows)]
        {
            let arg_line = args.join(" ").replace('\'', "''");
            let script = format!(
                "Start-Process -FilePath '{}' -ArgumentList '{}' -Verb RunAs -Wait",
                cli.display().to_string().replace('\'', "''"),
                arg_line
            );
            let status = std::process::Command::new("powershell")
                .args(["-NoProfile", "-NonInteractive", "-Command", &script])
                .status()?;
            if status.success() {
                Ok(())
            } else {
                Err(AppError::msg(
                    "ZeroTier: команда с правами администратора отклонена",
                ))
            }
        }
        #[cfg(not(windows))]
        {
            if !which("pkexec") {
                return Err(AppError::msg(
                    "Для подключения сети нужны права root. Установите pkexec или выполните: sudo zerotier-cli join <id>",
                ));
            }
            let status = std::process::Command::new("pkexec")
                .arg(&cli)
                .args(&args)
                .status()?;
            if status.success() {
                Ok(())
            } else {
                Err(AppError::msg("ZeroTier: pkexec завершился с ошибкой"))
            }
        }
    })
    .await
    .map_err(|e| AppError::msg(e.to_string()))?
}

#[cfg(not(target_os = "android"))]
fn parse_networks(value: &Value) -> Vec<ZtNetwork> {
    let mut out = Vec::new();
    let list = if let Some(list) = value.as_array() {
        list.clone()
    } else if let Some(list) = value.get("networks").and_then(|v| v.as_array()) {
        list.clone()
    } else {
        return out;
    };
    for item in list {
        let id = item
            .get("id")
            .or_else(|| item.get("nwid"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let name = item
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let status = item
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("UNKNOWN")
            .to_string();
        let assigned_addresses = item
            .get("assignedAddresses")
            .and_then(|v| v.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        if !id.is_empty() {
            out.push(ZtNetwork {
                id,
                name,
                status,
                assigned_addresses,
            });
        }
    }
    out
}

#[cfg(not(target_os = "android"))]
pub async fn status() -> ZtStatus {
    let mut status = ZtStatus::default();
    let token = read_token();
    let mut messages = Vec::new();

    if let Some(token) = token {
        match zt_http("GET", "/status", &token, None).await {
            Ok(value) => {
                status.available = true;
                status.service_online = value
                    .get("online")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                status.version = value
                    .get("version")
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
                status.address = value
                    .get("address")
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
            }
            Err(e) => messages.push(e.to_string()),
        }
        match zt_http("GET", "/network", &token, None).await {
            Ok(value) => {
                status.available = true;
                status.networks = parse_networks(&value);
            }
            Err(e) => messages.push(e.to_string()),
        }
    }

    if !status.available {
        match run_cli(&["info"]).await {
            Ok(info) => {
                status.available = true;
                status.service_online = info.contains("ONLINE");
                status.message = info;
            }
            Err(e) => messages.push(e.to_string()),
        }
        if status.available {
            if let Ok(info) = run_cli(&["listnetworks"]).await {
                for line in info.lines() {
                    let columns: Vec<&str> = line.split_whitespace().collect();
                    if columns.len() >= 2 && columns[0].len() >= 10 {
                        status.networks.push(ZtNetwork {
                            id: columns[0].to_string(),
                            name: columns.get(2).unwrap_or(&"").to_string(),
                            status: columns.get(1).unwrap_or(&"?").to_string(),
                            assigned_addresses: columns
                                .iter()
                                .skip(3)
                                .filter(|value| value.contains('/') || value.contains(':'))
                                .map(|value| value.to_string())
                                .collect(),
                        });
                    }
                }
            }
        }
    }

    if !status.available {
        let reachable = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            tokio::net::TcpStream::connect("127.0.0.1:9993"),
        )
        .await
        .map(|result| result.is_ok())
        .unwrap_or(false);
        if reachable {
            status.available = true;
            status.message = "Сервис ZeroTier запущен, но для управления нужны права администратора. Нажмите «Подключить» — Windows запросит UAC, Linux — pkexec.".to_string();
        }
    }

    if status.message.is_empty() {
        status.message = if messages.is_empty() {
            if status.available {
                "ZeroTier запущен".to_string()
            } else {
                "ZeroTier One не найден или сервис недоступен".to_string()
            }
        } else {
            messages.join("; ")
        };
    }
    status
}

#[cfg(not(target_os = "android"))]
pub async fn join(network_id: &str) -> AppResult<ZtStatus> {
    let network_id = network_id.trim();
    if network_id.is_empty() {
        return Err(AppError::msg("Укажите ZeroTier Network ID"));
    }
    if let Some(token) = read_token() {
        zt_http("POST", &format!("/network/{network_id}"), &token, Some("{}")).await?;
        let mut last = ZtStatus::default();
        for _ in 0..20 {
            tokio::time::sleep(std::time::Duration::from_millis(750)).await;
            last = status().await;
            if last.networks.iter().any(|n| {
                n.id.eq_ignore_ascii_case(network_id)
                    && (n.status == "OK" || n.status == "ACCESS_DENIED")
            }) {
                break;
            }
        }
        return Ok(last);
    }
    let mut elevated = false;
    if let Err(first) = run_cli(&["join", network_id]).await {
        run_cli_elevated(&["join", network_id])
            .await
            .map_err(|elevated_error| {
                AppError::msg(format!(
                    "{first}. Повышение прав: {elevated_error}"
                ))
            })?;
        elevated = true;
    }
    tokio::time::sleep(std::time::Duration::from_millis(800)).await;
    let mut result = status().await;
    if elevated {
        result.message = format!("Команда join выполнена. {}", result.message);
    }
    Ok(result)
}

#[cfg(not(target_os = "android"))]
pub async fn leave(network_id: &str) -> AppResult<ZtStatus> {
    let network_id = network_id.trim();
    if network_id.is_empty() {
        return Err(AppError::msg("Укажите ZeroTier Network ID"));
    }
    if let Some(token) = read_token() {
        zt_http(
            "DELETE",
            &format!("/network/{network_id}"),
            &token,
            None,
        )
        .await?;
    } else {
        let mut elevated = false;
        if let Err(first) = run_cli(&["leave", network_id]).await {
            run_cli_elevated(&["leave", network_id])
                .await
                .map_err(|elevated_error| {
                    AppError::msg(format!(
                        "{first}. Повышение прав: {elevated_error}"
                    ))
                })?;
            elevated = true;
        }
        if elevated {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        let mut result = status().await;
        if elevated {
            result.message = format!("Команда leave выполнена. {}", result.message);
        }
        return Ok(result);
    }
    Ok(status().await)
}

#[cfg(target_os = "android")]
pub async fn status() -> ZtStatus {
    ZtStatus {
        available: false,
        service_online: false,
        version: None,
        address: None,
        networks: Vec::new(),
        message: "На Android подключение к сети ZeroTier выполняет приложение ZeroTier One. Откройте его, добавьте Network ID и авторизуйте устройство в ZeroTier Central. После этого вернитесь в ZeroSMB.".to_string(),
        android: true,
    }
}

#[cfg(target_os = "android")]
pub async fn join(_network_id: &str) -> AppResult<ZtStatus> {
    Err(AppError::msg(
        "На Android сеть ZeroTier подключается в приложении ZeroTier One",
    ))
}

#[cfg(target_os = "android")]
pub async fn leave(_network_id: &str) -> AppResult<ZtStatus> {
    Err(AppError::msg(
        "На Android сеть ZeroTier отключается в приложении ZeroTier One",
    ))
}

#[cfg(target_os = "android")]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EmbeddedStatus {
    started: bool,
    online: bool,
    network_ready: bool,
    node_id: String,
    ipv4: String,
    network_id: String,
    message: String,
}

#[cfg(target_os = "android")]
fn map_embedded(status: EmbeddedStatus) -> ZtStatus {
    let network_id = status.network_id.trim().to_string();
    let mut networks = Vec::new();
    if !network_id.is_empty() {
        networks.push(ZtNetwork {
            id: network_id,
            name: "ZeroTier (встроенный)".to_string(),
            status: if status.network_ready {
                "OK".to_string()
            } else if status.online {
                "WAITING".to_string()
            } else {
                "OFFLINE".to_string()
            },
            assigned_addresses: if status.ipv4.is_empty() {
                Vec::new()
            } else {
                vec![status.ipv4.clone()]
            },
        });
    }
    ZtStatus {
        available: status.started || status.online,
        service_online: status.online,
        version: Some("libzt embedded".to_string()),
        address: if status.node_id.is_empty() {
            None
        } else {
            Some(status.node_id)
        },
        networks,
        message: status.message,
        android: true,
    }
}

#[cfg(target_os = "android")]
pub async fn embedded_status(
    app: &tauri::AppHandle,
    network_id: Option<&str>,
    planet_path: Option<&str>,
) -> AppResult<ZtStatus> {
    let status: EmbeddedStatus = crate::android_bridge::call(
        app,
        "ztStatus",
        serde_json::json!({ "networkId": network_id, "planetPath": planet_path }),
    )
    .await?;
    Ok(map_embedded(status))
}

#[cfg(target_os = "android")]
pub async fn embedded_join(
    app: &tauri::AppHandle,
    network_id: &str,
    planet_path: Option<&str>,
) -> AppResult<ZtStatus> {
    let status: EmbeddedStatus = crate::android_bridge::call(
        app,
        "ztJoin",
        serde_json::json!({ "networkId": network_id, "planetPath": planet_path }),
    )
    .await?;
    Ok(map_embedded(status))
}

#[cfg(target_os = "android")]
pub async fn embedded_leave(app: &tauri::AppHandle) -> AppResult<ZtStatus> {
    let status: EmbeddedStatus =
        crate::android_bridge::call(app, "ztLeave", serde_json::json!({})).await?;
    Ok(map_embedded(status))
}

#[cfg(target_os = "android")]
pub async fn embedded_stop(app: &tauri::AppHandle) -> AppResult<ZtStatus> {
    let status: EmbeddedStatus =
        crate::android_bridge::call(app, "ztStop", serde_json::json!({})).await?;
    Ok(map_embedded(status))
}