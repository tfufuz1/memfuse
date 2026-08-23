use crate::ingestion::pipeline::EmbeddingProvider;
use crate::ollama::OllamaBridge;
use crate::state::AppState;
use tauri::{Emitter, State};

/// Streamt Chat-Antworten als Tauri-Events an das Frontend, statt sie
/// als einzelnen Rückgabewert zu liefern.
#[tauri::command]
pub async fn chat_with_rag(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    message: String,
    collection_name: String,
    model: String,
) -> Result<String, String> {
    let db = {
        let db_guard = state.db.read();
        db_guard
            .as_ref()
            .cloned()
            .ok_or("Keine Datenbank geöffnet")?
    };
    let collection = db
        .collection(&collection_name)
        .await
        .map_err(|e| e.to_string())?;

    let embedder = OllamaBridge::localhost();
    let query_vector = embedder.embed(&message).await.map_err(|e| e.to_string())?;

    let search_results = collection
        .hybrid_search(&message, &query_vector, 5, None)
        .await
        .map_err(|e| e.to_string())?;

    let chunks: Vec<memfuse_core::ContextChunk> =
        search_results.into_iter().map(Into::into).collect();

    // Nutzt den bestehenden ContextManager aus memfuse-db
    let context_manager = memfuse_db::context::ContextManager::default();
    let context = context_manager
        .prepare_context(chunks)
        .map_err(|e| e.to_string())?;

    let bridge = OllamaBridge::localhost();
    let app_clone = app.clone();
    let full_response = bridge
        .chat_with_rag_streaming(&model, &message, &context.to_string(), move |token| {
            let _ = app_clone.emit("chat-token", token);
        })
        .await
        .map_err(|e| e.to_string())?;

    Ok(full_response)
}

#[tauri::command]
pub async fn list_ollama_models() -> Result<Vec<String>, String> {
    let bridge = OllamaBridge::localhost();
    bridge.list_models().await.map_err(|e| e.to_string())
}
