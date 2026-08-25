use crate::ollama::OllamaBridge;
use crate::state::AppState;
use memfuse_core::TextEmbeddingEngine;
use serde::Serialize;
use tauri::State;

#[derive(Serialize)]
pub struct SearchResultDto {
    pub id: String,
    pub score: f32,
    pub text_preview: String,
    pub source: String,
}

#[tauri::command]
pub async fn hybrid_search(
    state: State<'_, AppState>,
    query: String,
    collection_name: String,
    k: usize,
) -> Result<Vec<SearchResultDto>, String> {
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
    let query_vector = embedder.embed(&query).await.map_err(|e| e.to_string())?;

    let results = collection
        .hybrid_search(&query, &query_vector, k, None)
        .await
        .map_err(|e| e.to_string())?;

    Ok(results
        .into_iter()
        .map(|r| SearchResultDto {
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
                .unwrap_or("Unbekannt")
                .to_string(),
        })
        .collect())
}
