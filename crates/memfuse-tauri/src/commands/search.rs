use crate::commands::collections::validate_collection_name;
use crate::ollama::OllamaBridge;
use crate::state::AppState;
use memfuse_core::{MemFuseErrorDto, TextEmbeddingEngine};
use serde::Serialize;
use tauri::State;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SignalContributionDto {
    pub raw_score: f32,
    pub rank: u32,
    pub rrf_contribution: f32,
}

impl From<&memfuse_db::SignalContribution> for SignalContributionDto {
    fn from(s: &memfuse_db::SignalContribution) -> Self {
        Self {
            raw_score: s.raw_score,
            rank: s.rank,
            rrf_contribution: s.rrf_contribution,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ProvenanceDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vector_distance: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bm25_score: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graph_score: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rerank_score: Option<f32>,
    #[serde(skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub signal_ranks: std::collections::HashMap<String, u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_collection: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_type: Option<String>,
    #[serde(skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub signal_contributions: std::collections::HashMap<String, SignalContributionDto>,
}

impl From<&memfuse_db::ProvenanceRecord> for ProvenanceDto {
    fn from(p: &memfuse_db::ProvenanceRecord) -> Self {
        Self {
            vector_distance: p.vector_distance,
            bm25_score: p.bm25_score,
            graph_score: p.graph_score,
            rerank_score: p.rerank_score,
            signal_ranks: p.signal_ranks.clone(),
            source_collection: p.source_collection.clone(),
            index_type: p.index_type.clone(),
            signal_contributions: p
                .signal_contributions
                .iter()
                .map(|(k, v)| (k.clone(), SignalContributionDto::from(v)))
                .collect(),
        }
    }
}

#[derive(Serialize)]
pub struct SearchResultDto {
    pub id: String,
    pub score: f32,
    pub text_preview: String,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<ProvenanceDto>,
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
        .include_provenance(true)
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
            provenance: r.provenance.as_ref().map(ProvenanceDto::from),
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
            provenance: search_results[0]
                .provenance
                .as_ref()
                .map(ProvenanceDto::from),
        };

        assert_eq!(dto.id, "doc-1");
        assert_eq!(dto.source, "docs/architecture.md");
        assert_eq!(dto.text_preview, "Sovereign AI Memory Operating System");
        Ok(())
    }

    #[tokio::test]
    async fn test_hybrid_search_populates_provenance() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempfile::tempdir()?;
        let config = memfuse_db::MemFuseConfig {
            dimension: 4,
            ..Default::default()
        };
        let db = memfuse_db::MemFuse::open_with_config(temp_dir.path(), config).await?;
        let collection = db.collection("test_provenance").await?;

        collection
            .insert(
                "doc-prov-1",
                &[1.0, 0.0, 0.0, 0.0],
                Some(serde_json::json!({
                    "text": "Sovereign AI Provenance Audit Trail System",
                    "source": "docs/provenance.md"
                })),
            )
            .await?;

        let query_vec = vec![1.0, 0.0, 0.0, 0.0];
        let search_results = collection
            .query()
            .text("Sovereign AI Provenance")
            .embedding(&query_vec)
            .include_provenance(true)
            .k(5)
            .execute()
            .await?;

        assert_eq!(search_results.len(), 1);
        let dto = SearchResultDto {
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
            provenance: search_results[0]
                .provenance
                .as_ref()
                .map(ProvenanceDto::from),
        };

        assert!(
            dto.provenance.is_some(),
            "SearchResultDto.provenance must be Some"
        );
        let prov = dto.provenance.as_ref().unwrap();
        assert!(
            prov.vector_distance.is_some()
                || prov.bm25_score.is_some()
                || !prov.signal_ranks.is_empty(),
            "SearchResultDto.provenance must contain at least one non-None signal field"
        );
        Ok(())
    }
}
