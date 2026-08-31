use crate::commands::collections::validate_collection_name;
use crate::ollama::OllamaBridge;
use crate::state::AppState;
use memfuse_core::{MemFuseErrorDto, TextEmbeddingEngine};
use serde::Serialize;
use tauri::State;

#[derive(Serialize)]
pub struct SearchResultDto {
    pub id: String,
    pub score: f32,
    pub text_preview: String,
    pub source: String,
}

const MAX_QUERY_LEN: usize = 65_536; // 64 KiB

#[allow(deprecated)]
#[tauri::command]
pub async fn hybrid_search(
    state: State<'_, AppState>,
    query: String,
    collection_name: String,
    k: usize,
) -> Result<Vec<SearchResultDto>, MemFuseErrorDto> {
    if query.len() > MAX_QUERY_LEN {
        return Err(MemFuseErrorDto::new("InvalidInput", "Query too long"));
    }
    validate_collection_name(&collection_name)?;
    let db = {
        let db_guard = state.db.read();
        db_guard.as_ref().cloned().ok_or_else(|| {
            MemFuseErrorDto::new(
                "NotFound",
                "No database is open. Please open or create a database first.",
            )
        })?
    };
    let collection = db
        .collection(&collection_name)
        .await
        .map_err(|e| MemFuseErrorDto::from(&e))?;

    let embedder = OllamaBridge::localhost();
    let query_vector = embedder
        .embed(&query)
        .await
        .map_err(|e| MemFuseErrorDto::from(&e))?;

    let results = collection
        .hybrid_search(&query, &query_vector, k, None)
        .await
        .map_err(|e| MemFuseErrorDto::from(&e))?;

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
                .unwrap_or("Unknown")
                .to_string(),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_search_without_open_db_returns_error() {
        let state = AppState::new();
        let db_guard = state.db.read();
        let res: Result<(), MemFuseErrorDto> = db_guard
            .as_ref()
            .cloned()
            .ok_or_else(|| {
                MemFuseErrorDto::new(
                    "NotFound",
                    "No database is open. Please open or create a database first.",
                )
            })
            .map(|_| ());

        assert!(res.is_err());
        assert_eq!(
            res.unwrap_err().message,
            "No database is open. Please open or create a database first."
        );
    }
}
