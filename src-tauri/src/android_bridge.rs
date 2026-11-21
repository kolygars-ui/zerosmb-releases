use crate::error::{AppError, AppResult};
use serde::de::DeserializeOwned;
use serde_json::Value;
use tauri::plugin::{Builder, TauriPlugin};

#[cfg(target_os = "android")]
pub struct AndroidBridge {
    handle: tauri::plugin::PluginHandle<tauri::Wry>,
}

pub fn init() -> TauriPlugin<tauri::Wry> {
    Builder::new("zerosmb-bridge")
        .setup(|app, api| {
            #[cfg(target_os = "android")]
            {
                use tauri::Manager;
                let handle = api.register_android_plugin("com.zerosmb.app", "ZeroSmbPlugin")?;
                app.manage(AndroidBridge { handle });
            }
            #[cfg(not(target_os = "android"))]
            {
                let _ = (app, api);
            }
            Ok(())
        })
        .build()
}

#[cfg(target_os = "android")]
pub async fn call<T: DeserializeOwned>(
    app: &tauri::AppHandle,
    command: &str,
    payload: Value,
) -> AppResult<T> {
    use tauri::Manager;
    let bridge = app.state::<AndroidBridge>();
    bridge
        .handle
        .run_mobile_plugin_async(command, payload)
        .await
        .map_err(|e| AppError::msg(format!("Android bridge: {e}")))
}

#[cfg(not(target_os = "android"))]
pub async fn call<T: DeserializeOwned>(
    _app: &tauri::AppHandle,
    _command: &str,
    _payload: Value,
) -> AppResult<T> {
    Err(AppError::msg("Эта функция доступна только на Android"))
}
