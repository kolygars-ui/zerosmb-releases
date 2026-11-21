use crate::config::{self, AppConfig};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use std::path::{Path, PathBuf};
#[cfg(not(target_os = "android"))]
use tauri::menu::{Menu, MenuItem};
#[cfg(not(target_os = "android"))]
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, State};
#[cfg(not(target_os = "android"))]
use tauri::WindowEvent;

#[cfg(target_os = "linux")]
fn refresh_linux_icon() {
    let Ok(home) = std::env::var("HOME") else {
        return;
    };
    let home = PathBuf::from(home);
    let icon_path = home.join(".local/share/icons/hicolor/128x128/apps/zerosmb.png");
    if let Some(parent) = icon_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&icon_path, include_bytes!("../icons/128x128.png"));
    let hicolor = home.join(".local/share/icons/hicolor");
    let _ = std::process::Command::new("gtk-update-icon-cache")
        .args(["-f", "-t"])
        .arg(&hicolor)
        .status();
    let applications = home.join(".local/share/applications");
    let _ = std::process::Command::new("update-desktop-database")
        .arg(&applications)
        .status();
}

#[cfg(not(target_os = "android"))]
pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Показать ZeroSMB", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Выход", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;
    let mut builder = TrayIconBuilder::with_id("main")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_window(app),
            "quit" => app.exit(0),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    #[cfg(target_os = "linux")]
    refresh_linux_icon();
    apply_tray(app, app.state::<AppState>().config().system_tray);

    if let Some(window) = app.get_webview_window("main") {
        if let Some(icon) = app.default_window_icon() {
            let _ = window.set_icon(icon.clone());
        }
        let app_handle = app.clone();
        window.on_window_event(move |event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let state = app_handle.state::<AppState>();
                if state.config().system_tray {
                    api.prevent_close();
                    if let Some(window) = app_handle.get_webview_window("main") {
                        let _ = window.hide();
                    }
                }
            }
        });
    }
    Ok(())
}

#[cfg(not(target_os = "android"))]
fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

#[cfg(not(target_os = "android"))]
pub fn apply_tray(app: &AppHandle, visible: bool) {
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_visible(visible);
    }
}

#[cfg(target_os = "android")]
pub fn apply_tray(_app: &AppHandle, _visible: bool) {}

pub fn set_autostart(enabled: bool, executable: &Path) -> AppResult<()> {
    #[cfg(target_os = "windows")]
    {
        let run_key = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
        let status = if enabled {
            std::process::Command::new("reg")
                .args([
                    "add",
                    run_key,
                    "/v",
                    "ZeroSMB",
                    "/t",
                    "REG_SZ",
                    "/d",
                    &executable.to_string_lossy(),
                    "/f",
                ])
                .status()
        } else {
            std::process::Command::new("reg")
                .args(["delete", run_key, "/v", "ZeroSMB", "/f"])
                .status()
        };
        status.map_err(|error| AppError::msg(error.to_string()))?;
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    {
        let home = std::env::var("HOME").map_err(|error| AppError::msg(error.to_string()))?;
        let path = PathBuf::from(home)
            .join(".config")
            .join("autostart")
            .join("zerosmb.desktop");
        if enabled {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let content = format!(
                "[Desktop Entry]\nType=Application\nName=ZeroSMB\nExec=\"{}\"\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
                executable.display()
            );
            std::fs::write(path, content)?;
        } else {
            let _ = std::fs::remove_file(path);
        }
        return Ok(());
    }
    #[allow(unreachable_code)]
    {
        let _ = (enabled, executable);
        Ok(())
    }
}

#[tauri::command]
pub async fn uninstall(
    app: AppHandle,
    state: State<'_, AppState>,
    delete_data: Option<bool>,
) -> AppResult<String> {
    let delete_data = delete_data.unwrap_or(false);
    let current = std::env::current_exe().map_err(|error| AppError::msg(error.to_string()))?;
    let filename = current
        .file_name()
        .ok_or_else(|| AppError::msg("Не удалось определить имя приложения"))?
        .to_owned();
    let mut cfg = state.config();
    let install_dir = cfg.system_install_dir.trim().to_string();

    create_shortcuts(&current, false, false)?;
    let _ = set_autostart(false, &current);
    cfg.system_install_dir.clear();
    cfg.system_autostart = false;
    cfg.system_start_menu = false;
    cfg.system_desktop_shortcut = false;
    cfg.system_tray = false;
    state.set_config(cfg)?;
    apply_tray(&app, false);
    if delete_data {
        clear_data(&app, &state, false)?;
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(home) = std::env::var("HOME") {
            let _ = std::fs::remove_file(
                PathBuf::from(home).join(".local/share/icons/hicolor/128x128/apps/zerosmb.png"),
            );
        }
    }

    if !install_dir.is_empty() {
        let installed_exe = PathBuf::from(&install_dir).join(&filename);
        if installed_exe != current && installed_exe.exists() {
            let _ = std::fs::remove_file(&installed_exe);
            let _ = std::fs::remove_dir(&install_dir);
        } else if installed_exe == current {
            schedule_cleanup(Path::new(&install_dir), &filename);
            let app_for_exit = app.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(800)).await;
                app_for_exit.exit(0);
            });
            return Ok("ZeroSMB удалён из системы. Приложение сейчас закроется.".to_string());
        }
    }
    if delete_data {
        let app_for_restart = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(800)).await;
            app_for_restart.restart();
        });
        return Ok("ZeroSMB удалён из системы. Кэш и настройки очищены.".to_string());
    }
    Ok("Ярлыки и автозапуск удалены. Настройки и данные сохранены.".to_string())
}

#[tauri::command]
pub async fn clear_app_data(app: AppHandle, state: State<'_, AppState>) -> AppResult<String> {
    clear_data(&app, &state, true)?;
    let app_for_restart = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        app_for_restart.restart();
    });
    Ok("Кэш, настройки и пароли очищены. Приложение перезапустится.".to_string())
}

fn clear_data(app: &AppHandle, state: &AppState, reinitialize: bool) -> AppResult<()> {
    let cfg = state.config();
    let dir = state.data_dir()?;
    config::clear_password(&dir, &cfg.smb);
    state.set_password(None);
    let _ = std::fs::remove_dir_all(&dir);
    if let Ok(cache) = app.path().app_cache_dir() {
        let _ = std::fs::remove_dir_all(cache);
    }
    if reinitialize {
        std::fs::create_dir_all(&dir)?;
        state.set_config(AppConfig::default())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn install(
    app: AppHandle,
    state: State<'_, AppState>,
    target_dir: String,
) -> AppResult<String> {
    let current = std::env::current_exe().map_err(|error| AppError::msg(error.to_string()))?;
    let filename = current
        .file_name()
        .ok_or_else(|| AppError::msg("Не удалось определить имя приложения"))?
        .to_owned();
    let target_dir = if target_dir.trim().is_empty() {
        current
            .parent()
            .ok_or_else(|| AppError::msg("Не удалось определить папку установки"))?
            .to_path_buf()
    } else {
        PathBuf::from(target_dir.trim())
    };
    std::fs::create_dir_all(&target_dir)?;
    let target = target_dir.join(&filename);

    let mut cfg = state.config();
    let previous_dir = cfg.system_install_dir.trim().to_string();
    let same_file = target == current;
    if !same_file {
        std::fs::copy(&current, &target)?;
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755))?;
        }
    }
    cfg.system_install_dir = target_dir.to_string_lossy().to_string();
    state.set_config(cfg.clone())?;
    create_shortcuts(&target, cfg.system_start_menu, cfg.system_desktop_shortcut)?;
    set_autostart(cfg.system_autostart, &target)?;
    apply_tray(&app, cfg.system_tray);

    if !same_file {
        std::process::Command::new(&target)
            .spawn()
            .map_err(|error| AppError::msg(format!("Не удалось запустить обновлённую копию: {error}")))?;
        if !previous_dir.is_empty() && previous_dir != cfg.system_install_dir {
            schedule_cleanup(&PathBuf::from(previous_dir), &filename);
        }
        let app_for_exit = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(800)).await;
            app_for_exit.exit(0);
        });
        return Ok("Приложение установлено. ZeroSMB будет перезапущен.".to_string());
    }
    Ok("Системная интеграция применена.".to_string())
}

fn create_shortcuts(executable: &Path, start_menu: bool, desktop: bool) -> AppResult<()> {
    #[cfg(target_os = "windows")]
    {
        let target = executable.to_string_lossy().replace('\'', "''");
        let working = executable
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .to_string_lossy()
            .replace('\'', "''");
        let script = format!(
            r#"$shell = New-Object -ComObject WScript.Shell
$start = Join-Path ([Environment]::GetFolderPath('Programs')) 'ZeroSMB.lnk'
$desk = Join-Path ([Environment]::GetFolderPath('Desktop')) 'ZeroSMB.lnk'
if ({start_menu}) {{
  $s = $shell.CreateShortcut($start)
  $s.TargetPath = '{target}'
  $s.WorkingDirectory = '{working}'
  $s.IconLocation = '{target},0'
  $s.Save()
}} else {{
  Remove-Item -LiteralPath $start -Force -ErrorAction SilentlyContinue
}}
if ({desktop}) {{
  $s = $shell.CreateShortcut($desk)
  $s.TargetPath = '{target}'
  $s.WorkingDirectory = '{working}'
  $s.IconLocation = '{target},0'
  $s.Save()
}} else {{
  Remove-Item -LiteralPath $desk -Force -ErrorAction SilentlyContinue
}}"#,
            start_menu = if start_menu { "$true" } else { "$false" },
            desktop = if desktop { "$true" } else { "$false" },
            target = target,
            working = working
        );
        std::process::Command::new("powershell")
            .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &script])
            .status()
            .map_err(|error| AppError::msg(error.to_string()))?;
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    {
        let home = std::env::var("HOME").map_err(|error| AppError::msg(error.to_string()))?;
        let home = PathBuf::from(home);
        let icon_path = home
            .join(".local/share/icons/hicolor/128x128/apps/zerosmb.png");
        let app_path = home.join(".local/share/applications/zerosmb.desktop");
        let desktop_path = home.join("Desktop/zerosmb.desktop");
        if start_menu {
            if let Some(parent) = icon_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&icon_path, include_bytes!("../icons/128x128.png"))?;
            if let Some(parent) = app_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let entry = format!(
                "[Desktop Entry]\nType=Application\nName=ZeroSMB\nComment=ZeroTier + SMB NAS client\nExec=\"{}\"\nIcon={}\nTerminal=false\nCategories=Network;Utility;\nStartupWMClass=ZeroSMB\n",
                executable.display(),
                icon_path.display()
            );
            std::fs::write(&app_path, entry)?;
        } else {
            let _ = std::fs::remove_file(&app_path);
        }
        if desktop {
            if let Some(parent) = desktop_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let entry = format!(
                "[Desktop Entry]\nType=Application\nName=ZeroSMB\nExec=\"{}\"\nIcon={}\nTerminal=false\n",
                executable.display(),
                icon_path.display()
            );
            std::fs::write(&desktop_path, entry)?;
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                &desktop_path,
                std::fs::Permissions::from_mode(0o755),
            )?;
        } else {
            let _ = std::fs::remove_file(&desktop_path);
        }
        return Ok(());
    }
    #[allow(unreachable_code)]
    {
        let _ = (executable, start_menu, desktop);
        Ok(())
    }
}
fn schedule_cleanup(old_dir: &Path, filename: &std::ffi::OsStr) {
    let old_exe = old_dir.join(filename);
    #[cfg(target_os = "windows")]
    {
        let script = format!(
            "Start-Sleep -Seconds 3; Remove-Item -LiteralPath '{}' -Force -ErrorAction SilentlyContinue; Remove-Item -LiteralPath '{}' -Force -ErrorAction SilentlyContinue",
            old_exe.display(),
            old_dir.display()
        );
        let _ = std::process::Command::new("powershell")
            .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &script])
            .spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let script = format!(
            "sleep 3; rm -f '{}'; rmdir '{}' 2>/dev/null || true",
            old_exe.display(),
            old_dir.display()
        );
        let _ = std::process::Command::new("sh").args(["-c", &script]).spawn();
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        let _ = (old_exe, old_dir);
    }
}

pub fn notify(app: &AppHandle, kind: &str, title: &str, body: &str) {
    let cfg = app.state::<AppState>().config();
    if !cfg.system_notifications {
        return;
    }
    let enabled = match kind {
        "download" => cfg.notify_downloads,
        "upload" => cfg.notify_uploads,
        "sync" => cfg.notify_sync,
        "error" => cfg.notify_errors,
        _ => true,
    };
    if !enabled {
        return;
    }
    #[cfg(target_os = "windows")]
    {
        let _ = tauri_winrt_notification::Toast::new(
            tauri_winrt_notification::Toast::POWERSHELL_APP_ID,
        )
        .title(title)
        .text1(body)
        .show();
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "android")))]
    {
        use tauri_plugin_notification::NotificationExt;
        let _ = app
            .notification()
            .builder()
            .title(title)
            .body(body)
            .show();
    }
    #[cfg(target_os = "android")]
    {
        let _ = (app, title, body);
    }
}
