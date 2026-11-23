mod ai_api;
mod android_bridge;
mod android_progress;
mod commands;
mod config;
mod error;
mod integration;
mod media;
mod preview;
mod smb_service;
mod state;
mod sync;
mod transfer;
mod updates;
mod wireguard;
mod zerotier;
mod zt_proxy;
#[cfg(not(target_os = "android"))]
mod zerotier_embedded;

use std::io::Write;
use std::sync::{Mutex, OnceLock};
use tauri::Manager;

struct FileLogger {
    file: Mutex<std::fs::File>,
}

impl log::Log for FileLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        if metadata.target().starts_with("zerosmb") {
            metadata.level() <= log::Level::Info
        } else {
            metadata.level() <= log::Level::Warn
        }
    }

    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|value| value.as_secs())
            .unwrap_or(0);
        if let Ok(mut file) = self.file.lock() {
            let _ = writeln!(
                file,
                "{timestamp} {:?} {}: {}",
                record.level(),
                record.target(),
                record.args()
            );
            let _ = file.flush();
        }
        #[cfg(not(target_os = "android"))]
        eprintln!("{:?} {}: {}", record.level(), record.target(), record.args());
    }

    fn flush(&self) {
        if let Ok(mut file) = self.file.lock() {
            let _ = file.flush();
        }
    }
}

static LOGGER: OnceLock<FileLogger> = OnceLock::new();

fn install_file_logger() {
    let path = std::env::temp_dir().join("zerosmb.log");
    if std::fs::metadata(&path)
        .map(|metadata| metadata.len() > 4 * 1024 * 1024)
        .unwrap_or(false)
    {
        let _ = std::fs::remove_file(&path);
    }
    let file = match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        Ok(file) => file,
        Err(_) => return,
    };
    let logger = LOGGER.get_or_init(|| FileLogger {
        file: Mutex::new(file),
    });
    if log::set_logger(logger).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
}

#[cfg(windows)]
fn install_native_crash_logger() {
    let _ = std::fs::remove_file(std::env::temp_dir().join("zerosmb-native-crash.log"));
    #[repr(C)]
    struct ExceptionRecord {
        code: u32,
        flags: u32,
        record: *mut ExceptionRecord,
        address: *mut std::ffi::c_void,
        number_parameters: u32,
        information: [usize; 15],
    }

    #[repr(C)]
    struct ExceptionPointers {
        record: *mut ExceptionRecord,
        context: *mut std::ffi::c_void,
    }

    unsafe extern "system" fn handler(info: *mut std::ffi::c_void) -> i32 {
        let pointers = info as *mut ExceptionPointers;
        if pointers.is_null() {
            return 0;
        }
        let record = (*pointers).record;
        let (code, address) = if record.is_null() {
            (0, std::ptr::null_mut())
        } else {
            ((*record).code, (*record).address)
        };
        let mut frames = [std::ptr::null_mut(); 32];
        let count = unsafe {
            RtlCaptureStackBackTrace(
                0,
                frames.len() as u32,
                frames.as_mut_ptr(),
                std::ptr::null_mut(),
            )
        };
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(std::env::temp_dir().join("zerosmb-native-crash.log"))
        {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|value| value.as_secs())
                .unwrap_or(0);
            let _ = writeln!(
                file,
                "native crash unix={timestamp} code=0x{code:08x} address=0x{:x} thread={}",
                address as usize,
                GetCurrentThreadId()
            );
            for frame in frames.iter().take(count as usize) {
                let pc = *frame;
                let mut base = std::ptr::null_mut();
                unsafe {
                    RtlPcToFileHeader(pc, &mut base);
                }
                if base.is_null() {
                    let _ = writeln!(file, "  frame=0x{:x} module=?", pc as usize);
                    continue;
                }
                let mut name = [0u16; 260];
                let len = unsafe {
                    GetModuleFileNameW(base as _, name.as_mut_ptr(), name.len() as u32)
                } as usize;
                let module = String::from_utf16_lossy(&name[..len]);
                let _ = writeln!(
                    file,
                    "  frame=0x{:x} module={module}+0x{:x}",
                    pc as usize,
                    pc as usize - base as usize
                );
            }
            let _ = file.flush();
        }
        0
    }

    extern "system" {
        fn SetUnhandledExceptionFilter(
            filter: Option<unsafe extern "system" fn(*mut std::ffi::c_void) -> i32>,
        ) -> *mut std::ffi::c_void;
        fn GetCurrentThreadId() -> u32;
        fn RtlCaptureStackBackTrace(
            frames_to_skip: u32,
            frames_to_capture: u32,
            backtrace: *mut *mut std::ffi::c_void,
            backtrace_hash: *mut u32,
        ) -> u16;
        fn RtlPcToFileHeader(
            pc_value: *mut std::ffi::c_void,
            base_of_image: *mut *mut std::ffi::c_void,
        ) -> *mut std::ffi::c_void;
        fn GetModuleFileNameW(
            module: *mut std::ffi::c_void,
            filename: *mut u16,
            size: u32,
        ) -> u32;
    }

    unsafe {
        SetUnhandledExceptionFilter(Some(handler));
    }
}

#[cfg(not(target_os = "android"))]
pub fn zt_smoke() -> i32 {
    let storage = std::env::temp_dir().join("zerosmb-zt-smoke");
    let node = zerotier_embedded::ZtNode::new();
    match node.probe(&storage) {
        Ok(id) => {
            println!("libzt ok, node {id}");
            0
        }
        Err(error) => {
            eprintln!("libzt error: {error}");
            1
        }
    }
}

fn install_panic_logger() {
    let path = std::env::temp_dir().join("zerosmb-crash.log");
    std::panic::set_hook(Box::new(move |info| {
        use std::io::Write;
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            let location = info
                .location()
                .map(|location| format!("{}:{}", location.file(), location.line()))
                .unwrap_or_else(|| "unknown".to_string());
            let payload = info
                .payload()
                .downcast_ref::<&str>()
                .map(|value| value.to_string())
                .or_else(|| info.payload().downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown".to_string());
            let _ = writeln!(
                file,
                "panic thread={:?} location={location} message={payload}",
                std::thread::current().name()
            );
        }
    }));
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    install_file_logger();
    install_panic_logger();
    #[cfg(windows)]
    install_native_crash_logger();

    let builder = tauri::Builder::default();
    #[cfg(not(target_os = "android"))]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
    }));
    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(android_bridge::init())
        .manage(state::AppState::new())
        .register_asynchronous_uri_scheme_protocol("media", |context, request, responder| {
            media::handle(context, request, responder);
        })
        .invoke_handler(tauri::generate_handler![
            commands::runtime_info,
            ai_api::ai_status,
            transfer::transfer_snapshot,
            transfer::transfer_clear_completed,
            commands::request_notification_permission,
            commands::test_notification,
            commands::get_config,
            commands::get_planet,
            commands::pick_planet,
            commands::clear_planet,
            commands::test_connection,
            commands::test_current_connection,
            commands::test_connection_saved,
            commands::save_setup,
            commands::save_settings,
            commands::pick_local_folder,
            commands::smb_list,
            commands::select_smb_share,
            commands::clear_smb_share,
            commands::send_error_log,
            commands::delete_paths,
            commands::create_folder,
            commands::upload_files,
            commands::upload_folder,
            commands::download_file,
            commands::read_text_file,
            preview::preview_document,
            preview::archive_list,
            preview::archive_read,
            preview::archive_preview_entry,
            preview::archive_save_entry,
            preview::archive_open_entry,
            preview::open_remote_file,
            preview::external_file_signature,
            preview::save_external_file,
            commands::read_file_bytes,
            commands::read_media_bytes,
            commands::probe_connection,
            commands::download_gallery,
            commands::ensure_gallery_folder,
            commands::gallery_list,
            commands::gallery_thumbnail,
            commands::gallery_export,
            commands::sync_gallery,
            commands::android_pick_tree,
            commands::android_tree_uri,
            commands::sync_add_job,
            commands::sync_update_job,
            commands::sync_remove_job,
            commands::sync_run_job,
            commands::sync_list_remote_dirs,
            commands::zt_status,
            commands::zt_join,
            commands::zt_leave,
            commands::zt_open_android,
            commands::zt_open_help,
            updates::check_update,
            updates::install_update,
            integration::install,
            integration::uninstall,
            integration::clear_app_data,
            wireguard::wireguard_status,
            wireguard::wireguard_pick_config,
            wireguard::wireguard_connect,
            wireguard::wireguard_disconnect,
            wireguard::wireguard_open_help,
        ])
        .setup(|app| {
            app.state::<state::AppState>().init(app.handle())?;
            #[cfg(not(target_os = "android"))]
            integration::setup(app.handle())?;
            sync::start_scheduler(app.handle().clone());
            let ai_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let state = ai_handle.state::<state::AppState>();
                if let Err(error) = state.inner.ai.sync(&ai_handle).await {
                    log::warn!("AI API: {error}");
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running ZeroSMB");
}
