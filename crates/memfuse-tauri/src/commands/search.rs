// FILE-CONTEXT
// STAND: 2026-08-30T14:38:30Z (SESSION: 45595f71)
// ZWECK: Hybrid vector and BM25 search Tauri IPC command.
// INVARIANTEN: Parameter k must be bounded (1 <= k <= 1,000); query length limited to MAX_QUERY_LEN.
// NICHT-OFFENSICHTLICH: Search converts results to SearchResultDto with truncated text previews.
// SIEHE AUCH: crates/memfuse-db/src/collection.rs

use crate::commands::collections::validate_collection_name;
use crate::ollama::OllamaBridge;
use crate::state::AppState;
use memfuse_core::{MemFuseErrorDto, TextEmbeddingEngine};
use serde::Serialize;
use tauri::State;

#[derive(Debug, Serialize)]
pub struct SearchResultDto {
    pub id: String,
    pub score: f32,
    pub text_preview: String,
    pub source: String,
}

const MAX_QUERY_LEN: usize = 65_536; // 64 KiB

pub const MAX_SEARCH_K: usize = 1_000;

pub fn validate_search_params(query: &str, k: usize) -> Result<(), MemFuseErrorDto> {
    if query.len() > MAX_QUERY_LEN {
        return Err(MemFuseErrorDto::new("InvalidInput", "Query too long"));
    }
    if k == 0 || k > MAX_SEARCH_K {
        return Err(MemFuseErrorDto::new(
            "InvalidInput",
            format!("Parameter k must be between 1 and {MAX_SEARCH_K}"),
        ));
    }
    Ok(())
}

#[tauri::command]
pub async fn hybrid_search(
    state: State<'_, AppState>,
    query: String,
    collection_name: String,
    k: usize,
) -> Result<Vec<SearchResultDto>, MemFuseErrorDto> {
    validate_search_params(&query, k)?;
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

    #[test]
    fn test_validate_search_params_k_bounds() {
        let res_zero = validate_search_params("query", 0);
        assert!(res_zero.is_err());
        assert_eq!(
            res_zero.unwrap_err().message,
            "Parameter k must be between 1 and 1000"
        );

        let res_too_large = validate_search_params("query", 1001);
        assert!(res_too_large.is_err());
        assert_eq!(
            res_too_large.unwrap_err().message,
            "Parameter k must be between 1 and 1000"
        );

        let res_valid = validate_search_params("query", 10);
        assert!(res_valid.is_ok());
    }
}
