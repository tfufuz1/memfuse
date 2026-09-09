#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(deprecated)]

fn main() {
    memfuse_tauri_lib::run();
}
