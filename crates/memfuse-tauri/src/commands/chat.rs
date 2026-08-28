use crate::commands::collections::validate_collection_name;
use crate::ollama::OllamaBridge;
use crate::state::AppState;
use memfuse_core::TextEmbeddingEngine;
use tauri::{Emitter, State};

#[derive(serde::Serialize)]
pub struct ChatResponse {
    pub answer: String,
    pub sources: Vec<crate::commands::search::SearchResultDto>,
}

const MAX_QUERY_LEN: usize = 65_536; // 64 KiB

/// Streamt Chat-Antworten als Tauri-Events an das Frontend, statt sie
/// als einzelnen Rückgabewert zu liefern.
#[tauri::command]
pub async fn chat_with_rag(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    message: String,
    collection_name: String,
    model: String,
) -> Result<ChatResponse, String> {
    if message.len() > MAX_QUERY_LEN {
        return Err("Query too long".to_string());
    }
    validate_collection_name(&collection_name)?;
    let db = {
        let db_guard = state.db.read();
        db_guard.as_ref().cloned().ok_or_else(|| {
            "No database is open. Please open or create a database first.".to_string()
        })?
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

    let sources: Vec<crate::commands::search::SearchResultDto> = search_results
        .iter()
        .map(|r| crate::commands::search::SearchResultDto {
            id: r.id.clone(),
            score: r.score,
            text_preview: r
                .metadata
                .as_ref()
                .and_then(|m| m.get("text"))
                .and_then(|t| t.as_str())
                .map(|s| s.chars().take(200).collect())
                .unwrap_or_default(),
            source: r
                .metadata
                .as_ref()
                .and_then(|m| m.get("source"))
                .and_then(|s| s.as_str())
                .unwrap_or("Unknown")
                .to_string(),
        })
        .collect();

    let chunks: Vec<memfuse_core::ContextChunk> = search_results
        .into_iter()
        .filter_map(|r| r.try_into().ok())
        .collect();

    // Nutzt den bestehenden ContextManager aus memfuse-db
    let context_manager = memfuse_db::context::ContextManager::default();
    let context = context_manager
        .prepare_context(chunks)
        .map_err(|e| e.to_string())?;

    let bridge = OllamaBridge::localhost();
    let app_clone = app.clone();
    let full_response = bridge
        .chat_with_rag_streaming(&model, &message, &context.to_string(), move |token| {
            if let Err(e) = app_clone.emit("chat-token", token) {
                tracing::debug!("Chat token emit failed (client disconnected?): {}", e);
            }
        })
        .await
        .map_err(|e| e.to_string())?;

    Ok(ChatResponse {
        answer: full_response,
        sources,
    })
}

#[tauri::command]
pub async fn list_ollama_models() -> Result<Vec<String>, String> {
    let bridge = OllamaBridge::localhost();
    bridge.list_models().await.map_err(|e| e.to_string())
}
