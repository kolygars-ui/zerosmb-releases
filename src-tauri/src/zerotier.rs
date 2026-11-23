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
fn system_dirs() -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    if let Ok(home) = std::env::var("ZEROTIER_HOME") {
        out.push(std::path::PathBuf::from(home));
    }
    #[cfg(windows)]
    {
        out.push(std::path::PathBuf::from(r"C:\ProgramData\ZeroTier\One"));
        if let Ok(program_data) = std::env::var("ProgramData") {
            out.push(
                std::path::PathBuf::from(program_data)
                    .join("ZeroTier")
                    .join("One"),
            );
        }
    }
    #[cfg(not(windows))]
    {
        out.push(std::path::PathBuf::from("/var/lib/zerotier-one"));
        out.push(std::path::PathBuf::from("/var/db/zerotier-one"));
        out.push(std::path::PathBuf::from("/var/snap/zerotier/current"));
        out.push(std::path::PathBuf::from("/var/snap/zerotier/common"));
        out.push(std::path::PathBuf::from(
            "/var/snap/zerotier/current/zerotier-one",
        ));
        out.push(std::path::PathBuf::from(
            "/var/snap/zerotier/common/zerotier-one",
        ));
    }
    out
}

#[cfg(not(target_os = "android"))]
fn token_candidates() -> Vec<std::path::PathBuf> {
    system_dirs()
        .into_iter()
        .map(|dir| dir.join("authtoken.secret"))
        .collect()
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
        out.push(std::path::PathBuf::from(
            r"C:\Program Files (x86)\ZeroTier\One\zerotier-cli.bat",
        ));
        out.push(std::path::PathBuf::from(
            r"C:\Program Files\ZeroTier\One\zerotier-cli.bat",
        ));
        out.push(std::path::PathBuf::from("zerotier-cli.bat"));
        out.push(std::path::PathBuf::from("zerotier-cli"));
    }
    #[cfg(not(windows))]
    {
        out.push(std::path::PathBuf::from("/snap/bin/zerotier-cli"));
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
#[cfg(not(target_os = "android"))]
fn system_planet_path() -> Option<std::path::PathBuf> {
    let dirs = system_dirs();
    let found = dirs
        .iter()
        .find(|dir| dir.join("authtoken.secret").is_file() || dir.join("identity.public").is_file())
        .or_else(|| dirs.iter().find(|dir| dir.is_dir()))?;
    Some(found.join("planet"))
}

#[cfg(not(target_os = "android"))]
fn system_planet_backup(path: &std::path::Path) -> std::path::PathBuf {
    path.with_file_name("planet.zerosmb-backup")
}

#[cfg(not(target_os = "android"))]
pub fn system_planet_location() -> Option<String> {
    system_planet_path().map(|path| path.display().to_string())
}

#[cfg(not(target_os = "android"))]
const DEFAULT_PLANET_WORLD_ID: u64 = 149604618;

#[cfg(not(target_os = "android"))]
fn planet_world_id(path: &std::path::Path) -> Option<u64> {
    let data = std::fs::read(path).ok()?;
    if data.len() < 12 {
        return None;
    }
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&data[4..12]);
    Some(u64::from_be_bytes(bytes))
}

#[cfg(not(target_os = "android"))]
pub fn system_planet_world_id() -> Option<u64> {
    system_planet_path().and_then(|path| planet_world_id(&path))
}

#[cfg(not(target_os = "android"))]
pub fn system_planet_managed() -> bool {
    system_planet_path()
        .map(|path| system_planet_backup(&path).is_file())
        .unwrap_or(false)
}

#[cfg(not(target_os = "android"))]
pub fn system_planet_configured() -> bool {
    if let Some(world_id) = system_planet_world_id() {
        return world_id != DEFAULT_PLANET_WORLD_ID;
    }
    system_planet_managed()
}

#[cfg(not(target_os = "android"))]
pub fn system_planet_size() -> u64 {
    system_planet_path()
        .and_then(|path| std::fs::metadata(path).ok())
        .map(|metadata| metadata.len())
        .unwrap_or(0)
}



#[cfg(not(target_os = "android"))]
fn script_temp_path(name: &str) -> std::path::PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("zerosmb-{name}-{}-{stamp}", std::process::id()))
}

#[cfg(not(target_os = "android"))]
fn read_script_result(path: &std::path::Path) -> AppResult<String> {
    let result = std::fs::read_to_string(path).unwrap_or_default();
    let result = result.trim().trim_start_matches('\u{feff}').trim().to_string();
    if result.is_empty() {
        return Err(AppError::msg(
            "ZeroTier: действие отменено или завершилось без результата",
        ));
    }
    if result.eq_ignore_ascii_case("OK") {
        Ok(result)
    } else {
        Err(AppError::msg(format!("ZeroTier: {result}")))
    }
}

#[cfg(windows)]
fn apply_system_planet_blocking(
    source: Option<&std::path::Path>,
    destination: &std::path::Path,
    backup: &std::path::Path,
) -> AppResult<String> {
    let script_path = script_temp_path("zt-planet.ps1");
    let result_path = script_temp_path("zt-planet-result.txt");
    let _ = std::fs::remove_file(&result_path);

    let action = if let Some(source) = source {
        format!(
            "New-Item -ItemType Directory -Force -Path (Split-Path -Parent $dst) | Out-Null\r\nif ((Test-Path -LiteralPath $dst) -and -not (Test-Path -LiteralPath $backup)) {{ Copy-Item -LiteralPath $dst -Destination $backup -Force }}\r\nCopy-Item -LiteralPath '{}' -Destination $dst -Force",
            source.display().to_string().replace('\'', "''")
        )
    } else {
        "if (Test-Path -LiteralPath $backup) { Move-Item -LiteralPath $backup -Destination $dst -Force } else { Remove-Item -LiteralPath $dst -Force -ErrorAction SilentlyContinue }".to_string()
    };

    let template = r#"$ErrorActionPreference = 'Stop'
$result = '__RESULT__'
try {
  $dst = '__DST__'
  $backup = '__BACKUP__'
  __ACTION__
  $svc = Get-Service | Where-Object { $_.Name -eq 'ZeroTierOneService' -or $_.DisplayName -like 'ZeroTier One*' } | Select-Object -First 1
  if (-not $svc) { throw 'Служба ZeroTier One не найдена' }
  Restart-Service -Name $svc.Name -Force
  Set-Content -LiteralPath $result -Value 'OK' -Encoding UTF8
} catch {
  Set-Content -LiteralPath $result -Value $_.Exception.Message -Encoding UTF8
  exit 1
}
"#;
    let script = template
        .replace("__RESULT__", &result_path.display().to_string().replace('\'', "''"))
        .replace("__DST__", &destination.display().to_string().replace('\'', "''"))
        .replace("__BACKUP__", &backup.display().to_string().replace('\'', "''"))
        .replace("__ACTION__", &action);
    std::fs::write(&script_path, script)?;

    let escaped = script_path.display().to_string().replace('\'', "''");
    let command = format!(
        "$p = Start-Process -FilePath 'powershell.exe' -ArgumentList @('-NoProfile','-ExecutionPolicy','Bypass','-File','{escaped}') -Verb RunAs -Wait -PassThru; exit $p.ExitCode"
    );
    let status = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &command])
        .status()?;
    let result = read_script_result(&result_path);
    let _ = std::fs::remove_file(&script_path);
    let _ = std::fs::remove_file(&result_path);
    if !status.success() && result.is_err() {
        return Err(AppError::msg(
            "ZeroTier: запрос прав администратора отклонён или служба не перезапущена",
        ));
    }
    result
}

#[cfg(all(unix, not(target_os = "android")))]
fn apply_system_planet_blocking(
    source: Option<&std::path::Path>,
    destination: &std::path::Path,
    backup: &std::path::Path,
) -> AppResult<String> {
    if !which("pkexec") {
        return Err(AppError::msg(
            "ZeroTier: для изменения системного planet нужен pkexec (polkit).",
        ));
    }
    let script_path = script_temp_path("zt-planet.sh");
    let result_path = script_temp_path("zt-planet-result.txt");
    let _ = std::fs::remove_file(&result_path);

    let action = if let Some(source) = source {
        format!(
            "mkdir -p \"$(dirname \"$dst\")\"\nif [ -f \"$dst\" ] && [ ! -f \"$backup\" ]; then cp -p \"$dst\" \"$backup\"; fi\ncp -f '{}' \"$dst\"",
            source.display().to_string().replace('\'', "'\\''")
        )
    } else {
        "if [ -f \"$backup\" ]; then mv -f \"$backup\" \"$dst\"; else rm -f \"$dst\"; fi"
            .to_string()
    };
    let template = r#"#!/bin/sh
set -eu
result='__RESULT__'
dst='__DST__'
backup='__BACKUP__'
__ACTION__
case "$dst" in
  /var/snap/*)
    if command -v systemctl >/dev/null 2>&1 && systemctl list-unit-files 2>/dev/null | grep -q '^snap.zerotier.zerotier-one'; then
      systemctl restart snap.zerotier.zerotier-one
    elif command -v snap >/dev/null 2>&1; then
      snap restart zerotier
    fi
    ;;
  *)
    if command -v systemctl >/dev/null 2>&1; then
      systemctl restart zerotier-one
    fi
    ;;
esac
printf 'OK\n' > "$result"
"#;
    let script = template
        .replace("__RESULT__", &result_path.display().to_string().replace('\'', "'\\''"))
        .replace("__DST__", &destination.display().to_string().replace('\'', "'\\''"))
        .replace("__BACKUP__", &backup.display().to_string().replace('\'', "'\\''"))
        .replace("__ACTION__", &action);
    std::fs::write(&script_path, script)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o700));
    }
    let status = std::process::Command::new("pkexec")
        .arg("/bin/sh")
        .arg(&script_path)
        .status()?;
    let result = read_script_result(&result_path);
    let _ = std::fs::remove_file(&script_path);
    let _ = std::fs::remove_file(&result_path);
    if !status.success() && result.is_err() {
        return Err(AppError::msg(
            "ZeroTier: запрос прав root отклонён или служба не перезапущена",
        ));
    }
    result
}

#[cfg(not(target_os = "android"))]
pub async fn install_planet(source: std::path::PathBuf) -> AppResult<String> {
    tokio::task::spawn_blocking(move || {
        if !source.is_file() {
            return Err(AppError::msg("ZeroTier: файл planet не найден"));
        }
        let destination = system_planet_path()
            .ok_or_else(|| AppError::msg("ZeroTier One не установлен в систему"))?;
        let backup = system_planet_backup(&destination);
        apply_system_planet_blocking(Some(&source), &destination, &backup)?;
        Ok("OK".to_string())
    })
    .await
    .map_err(|error| AppError::msg(error.to_string()))?
}

#[cfg(not(target_os = "android"))]
pub async fn remove_planet() -> AppResult<String> {
    tokio::task::spawn_blocking(move || {
        let destination = system_planet_path()
            .ok_or_else(|| AppError::msg("ZeroTier One не установлен в систему"))?;
        let backup = system_planet_backup(&destination);
        if !backup.is_file() {
            return Err(AppError::msg(
                "Этот planet установлен вручную. Автоматическое удаление доступно только для planet, установленного через ZeroSMB.",
            ));
        }
        apply_system_planet_blocking(None, &destination, &backup)?;
        Ok("OK".to_string())
    })
    .await
    .map_err(|error| AppError::msg(error.to_string()))?
}
