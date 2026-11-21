use crate::error::{AppError, AppResult};
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub available: bool,
    pub current_version: String,
    pub latest_version: String,
    pub notes: String,
    pub published_at: Option<String>,
    pub asset_name: Option<String>,
    pub asset_url: Option<String>,
    pub repo: String,
    pub platform: String,
    pub message: String,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

fn platform() -> &'static str {
    if cfg!(target_os = "android") {
        "android"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "unknown"
    }
}

fn asset_name() -> &'static str {
    if cfg!(target_os = "android") {
        "zerosmb-android-arm64.apk"
    } else if cfg!(target_os = "windows") {
        "zerosmb-windows-x64-setup.exe"
    } else if cfg!(target_os = "linux") {
        "zerosmb-linux-x64"
    } else {
        ""
    }
}

fn valid_repo(repo: &str) -> bool {
    let mut parts = repo.split('/');
    let owner = parts.next().unwrap_or_default();
    let name = parts.next().unwrap_or_default();
    parts.next().is_none()
        && !owner.is_empty()
        && !name.is_empty()
        && owner
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

fn parse_version(value: &str) -> Vec<u64> {
    value
        .trim()
        .trim_start_matches('v')
        .split(|c: char| c == '.' || c == '-' || c == '+')
        .take(3)
        .map(|part| part.parse::<u64>().unwrap_or(0))
        .collect()
}

fn is_newer(latest: &str, current: &str) -> bool {
    let latest = parse_version(latest);
    let current = parse_version(current);
    latest > current
}

#[tauri::command]
pub async fn check_update(
    state: State<'_, AppState>,
    repo: Option<String>,
) -> AppResult<UpdateInfo> {
    let repo = repo
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| state.config().update_repo);
    let repo = repo.trim().to_string();
    if !valid_repo(&repo) {
        return Err(AppError::msg("Укажите репозиторий в формате owner/repo"));
    }

    let repo_for_request = repo.clone();
    let release = tokio::task::spawn_blocking(move || -> AppResult<GithubRelease> {
        let url = format!("https://api.github.com/repos/{repo_for_request}/releases/latest");
        let response = ureq::get(&url)
            .header("User-Agent", "ZeroSMB")
            .header("Accept", "application/vnd.github+json")
            .call()
            .map_err(|error| AppError::msg(format!("GitHub API: {error}")))?;
        response
            .into_body()
            .read_json::<GithubRelease>()
            .map_err(|error| AppError::msg(format!("GitHub API: {error}")))
    })
    .await
    .map_err(|error| AppError::msg(error.to_string()))??;

    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let latest_version = release
        .tag_name
        .trim()
        .trim_start_matches('v')
        .to_string();
    let available = is_newer(&latest_version, &current_version);
    let wanted = asset_name();
    let asset = release
        .assets
        .iter()
        .find(|asset| asset.name == wanted);

    let message = if !available {
        "Установлена последняя версия".to_string()
    } else if asset.is_none() {
        "Доступна новая версия, но подходящий файл не найден в релизе".to_string()
    } else {
        format!(
            "Доступна новая версия {} (установлена {})",
            latest_version, current_version
        )
    };

    Ok(UpdateInfo {
        available,
        current_version,
        latest_version: latest_version.clone(),
        notes: release
            .body
            .or(release.name)
            .unwrap_or_default(),
        published_at: release.published_at,
        asset_name: asset.map(|a| a.name.clone()),
        asset_url: asset.map(|a| a.browser_download_url.clone()),
        repo,
        platform: platform().to_string(),
        message,
    })
}

#[tauri::command]
pub async fn install_update(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    url: String,
    filename: String,
) -> AppResult<String> {
    if !url.starts_with("https://") {
        return Err(AppError::msg("Обновление должно скачиваться по HTTPS"));
    }
    if !(url.starts_with("https://github.com/")
        || url.starts_with("https://objects.githubusercontent.com/")
        || url.starts_with("https://release-assets.githubusercontent.com/"))
    {
        return Err(AppError::msg("Недопустимый источник обновления"));
    }
    let filename = std::path::Path::new(&filename)
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| AppError::msg("Некорректное имя файла обновления"))?
        .to_string();
    if !(filename.ends_with(".exe") || filename.ends_with(".apk") || filename == "zerosmb-linux-x64") {
        return Err(AppError::msg("Неподдерживаемый файл обновления"));
    }

    use tauri::Manager;
    let dir = app
        .path()
        .app_cache_dir()
        .map_err(|error| AppError::msg(format!("Каталог обновлений: {error}")))?
        .join("updates");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(&filename);

    let url_for_download = url.clone();
    let path_for_download = path.clone();
    tokio::task::spawn_blocking(move || -> AppResult<()> {
        let mut response = ureq::get(&url_for_download)
            .header("User-Agent", "ZeroSMB")
            .call()
            .map_err(|error| AppError::msg(format!("Скачивание обновления: {error}")))?;
        let mut reader = response.body_mut().as_reader();
        let mut file = std::fs::File::create(&path_for_download)?;
        std::io::copy(&mut reader, &mut file)?;
        file.sync_all()?;
        Ok(())
    })
    .await
    .map_err(|error| AppError::msg(error.to_string()))??;

    #[cfg(target_os = "android")]
    {
        let value: serde_json::Value = crate::android_bridge::call(
            &app,
            "installApk",
            serde_json::json!({ "path": path.to_string_lossy() }),
        )
        .await?;
        let _ = value;
        return Ok("Установщик APK открыт. Подтвердите установку на телефоне.".to_string());
    }

    #[cfg(target_os = "windows")]
    {
        let install_dir = state.config().system_install_dir;
        let mut command = std::process::Command::new(&path);
        command.arg("/S");
        if !install_dir.trim().is_empty() {
            command.arg(format!("/D={}", install_dir.trim()));
        }
        command
            .spawn()
            .map_err(|error| AppError::msg(format!("Запуск установщика: {error}")))?;
        let app_for_exit = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(800)).await;
            app_for_exit.exit(0);
        });
        return Ok("Обновление устанавливается. ZeroSMB сейчас закроется.".to_string());
    }

    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::PermissionsExt;
        let current = std::env::current_exe()
            .map_err(|error| AppError::msg(error.to_string()))?;
        let staged = current.with_extension("update-new");
        std::fs::copy(&path, &staged)?;
        std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755))?;
        std::fs::rename(&staged, &current)?;
        std::process::Command::new(&current)
            .spawn()
            .map_err(|error| AppError::msg(format!("Запуск обновления: {error}")))?;
        let app_for_exit = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(800)).await;
            app_for_exit.exit(0);
        });
        return Ok("Обновление установлено. ZeroSMB сейчас перезапустится.".to_string());
    }

    #[allow(unreachable_code)]
    {
        let _ = (app, path);
        Err(AppError::msg("Обновление не поддерживается на этой платформе"))
    }
}
