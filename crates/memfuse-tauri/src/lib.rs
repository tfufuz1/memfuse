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
        .setup(|app| {
            let _handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let bridge = crate::ollama::OllamaBridge::localhost();
                match bridge.list_models().await {
                    Ok(models) if !models.is_empty() => {
                        tracing::info!(count = models.len(), "Ollama erreichbar beim Start");
                    }
                    Ok(_) => {
                        tracing::warn!("Ollama erreichbar, aber keine Modelle installiert");
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Ollama beim Start nicht erreichbar");
                    }
                }
            });
            Ok(())
        })
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
            commands::run_regex_transform,
            commands::run_bulk_regex_transform,
            commands::validate_regex_pattern,
        ])
        .run(tauri::generate_context!())
        // SAFETY-approved expect for main application loop
        .expect("error while running memfuse-brain application");
}
