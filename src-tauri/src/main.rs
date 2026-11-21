#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(not(target_os = "android"))]
    if std::env::args().any(|arg| arg == "--zt-smoke") {
        std::process::exit(zerosmb_lib::zt_smoke());
    }
    zerosmb_lib::run();
}
