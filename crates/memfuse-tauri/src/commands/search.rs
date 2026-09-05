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

#[derive(Serialize)]
pub struct MultiStepSearchResultDto {
    pub results: Vec<SearchResultDto>,
    pub rounds_executed: usize,
    pub sub_queries: Vec<String>,
}

const MAX_QUERY_LEN: usize = 65_536; // 64 KiB

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
        .query()
        .text(&query)
        .embedding(&query_vector)
        .k(k)
        .execute()
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

#[tauri::command]
pub async fn multi_step_search(
    state: State<'_, AppState>,
    query: String,
    collection_name: String,
    k: usize,
    max_rounds: Option<usize>,
) -> Result<MultiStepSearchResultDto, MemFuseErrorDto> {
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

    let rounds = max_rounds.unwrap_or(3).clamp(1, 5);
    let config = memfuse_db::MultiStepConfig {
        max_rounds: rounds,
        ..Default::default()
    };
    let engine = memfuse_db::MultiStepEngine::new(collection, config);

    let multi_res = engine
        .search(&query, &query_vector, k, Some(&embedder))
        .await
        .map_err(|e| MemFuseErrorDto::from(&e))?;

    let results = multi_res
        .results
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
        .collect();

    Ok(MultiStepSearchResultDto {
        results,
        rounds_executed: multi_res.rounds_executed,
        sub_queries: multi_res.sub_queries,
    })
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

    #[tokio::test]
    async fn test_query_builder_search_mapping() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempfile::tempdir()?;
        let config = memfuse_db::MemFuseConfig {
            dimension: 4,
            ..Default::default()
        };
        let db = memfuse_db::MemFuse::open_with_config(temp_dir.path(), config).await?;
        let collection = db.collection("test_search").await?;

        collection
            .insert(
                "doc-1",
                &[1.0, 0.0, 0.0, 0.0],
                Some(serde_json::json!({
                    "text": "Sovereign AI Memory Operating System",
                    "source": "docs/architecture.md"
                })),
            )
            .await?;

        let query_vec = vec![1.0, 0.0, 0.0, 0.0];
        let search_results = collection
            .query()
            .text("Sovereign AI")
            .embedding(&query_vec)
            .k(5)
            .execute()
            .await?;

        assert_eq!(search_results.len(), 1);
        let dto: SearchResultDto = SearchResultDto {
            id: search_results[0].id.clone(),
            score: search_results[0].score,
            text_preview: search_results[0]
                .metadata
                .as_ref()
                .and_then(|m| m.get("text"))
                .and_then(|t| t.as_str())
                .map(|s| s.chars().take(200).collect())
                .unwrap_or_default(),
            source: search_results[0]
                .metadata
                .as_ref()
                .and_then(|m| m.get("source"))
                .and_then(|s| s.as_str())
                .unwrap_or("Unknown")
                .to_string(),
        };

        assert_eq!(dto.id, "doc-1");
        assert_eq!(dto.source, "docs/architecture.md");
        assert_eq!(dto.text_preview, "Sovereign AI Memory Operating System");
        Ok(())
    }
}
