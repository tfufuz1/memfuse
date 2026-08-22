pub mod commands;
pub mod ingestion;
pub mod ollama;
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
            commands::open_database,
            commands::list_collections,
            commands::create_collection,
            commands::drop_collection,
            commands::ingest_file,
            commands::ingest_folder,
            commands::hybrid_search,
            commands::chat_with_rag,
            commands::list_ollama_models,
        ])
        .run(tauri::generate_context!())
        .expect("error while running memfuse-brain application");
}
