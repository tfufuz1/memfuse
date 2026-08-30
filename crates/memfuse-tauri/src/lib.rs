// FILE-CONTEXT
// STAND: 2026-08-30T14:38:30Z (SESSION: 45595f71)
// ZWECK: Tauri Desktop application entry point & plugin initialization.
// INVARIANTEN: Desktop UI app must never crash from backend panics; commands return Result<T, MemFuseErrorDto>.
// NICHT-OFFENSICHTLICH: Background async tasks must log/trace unhandled errors instead of swallowing silent failures.
// SIEHE AUCH: crates/memfuse-tauri/AGENTS.md, rules/async-io.md

pub mod commands;
pub mod ingestion;
pub mod ollama;
mod state;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt::init();

    let res = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(AppState::new())
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let bridge = crate::ollama::OllamaBridge::localhost();
                let status_msg = match bridge.list_models().await {
                    Ok(models) if !models.is_empty() => {
                        tracing::info!(count = models.len(), "Ollama erreichbar beim Start");
                        format!("Ollama ready: {} models", models.len())
                    }
                    Ok(_) => {
                        tracing::warn!("Ollama erreichbar, aber keine Modelle installiert");
                        "Ollama ready: no models installed".to_string()
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Ollama beim Start nicht erreichbar");
                        format!("Ollama unreachable: {e}")
                    }
                };
                use tauri::Emitter;
                if let Err(e) = handle.emit("ollama-status", status_msg) {
                    tracing::debug!(error = %e, "Failed to emit ollama-status event");
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
        .run(tauri::generate_context!());

    if let Err(e) = res {
        tracing::error!(error = %e, "Fatal error while running memfuse-brain application");
        eprintln!("[CRITICAL] MemFuse Brain Application Error: {e}");
    }
}
