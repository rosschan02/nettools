// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(target_os = "linux")]
    std::process::exit(netools_lib::cli::main());

    #[cfg(not(target_os = "linux"))]
    netools_lib::run()
}
