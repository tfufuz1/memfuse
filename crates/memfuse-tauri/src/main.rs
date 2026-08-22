// Crate memfuse-tauri binary entrypoint
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    memfuse_tauri_lib::run();
}
