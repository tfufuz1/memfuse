pub mod commands;
pub mod ingestion;
mod state;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            // Commands werden in Prompt 10 ergänzt
        ])
        .run(tauri::generate_context!())
        .expect("error while running memfuse-brain application");
}
