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

use tauri::Manager;

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(not(target_os = "android"))]
    {
        let _ = env_logger::Builder::from_env(
            env_logger::Env::default().default_filter_or("zerosmb=info,smb=warn"),
        )
        .try_init();
    }

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
            commands::delete_paths,
            commands::create_folder,
            commands::upload_files,
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
