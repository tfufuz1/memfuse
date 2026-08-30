// FILE-CONTEXT
// ZWECK: Multi-Step Iterative Retrieval Engine für komplexe Agenten-Abfragen (o-series Pattern).
// INVARIANTEN: RRF-Fusion über alle Runden; Abbruch bei Erreichen des Qualitätsschwellenwerts.
// NICHT-OFFENSICHTLICH: Sub-Queries nutzen BM25-only (leerer Vektor), da sie textuelle Umformulierungen darstellen.
// STAND: TS:2026-08-29T17:22:29Z (SESSION: 0dcb9f3b)

// memfuse-db/src/multistep.rs
// Multi-Step Iterative Retrieval Engine (OpenAI o-series Pattern)

use crate::{Collection, SearchResult};
use memfuse_core::{Result, StorageEngine};
use std::sync::Arc;

/// Konfiguration für Multi-Step Retrieval.
#[derive(Debug, Clone)]
pub struct MultiStepConfig {
    /// Maximale Iterationsrunden (OpenAI-Pattern: 3).
    pub max_rounds: usize,
    /// Mindest-Score-Schwellenwert: unter diesem Wert gilt Runde als unzureichend.
    pub quality_threshold: f32,
    /// Minimale Anzahl an Treffern die den Threshold überschreiten müssen.
    pub min_quality_hits: usize,
}

impl Default for MultiStepConfig {
    fn default() -> Self {
        Self {
            max_rounds: 3,
            quality_threshold: 0.5,
            min_quality_hits: 2,
        }
    }
}

/// Ergebnis einer Multi-Step-Suche mit Audit-Informationen.
#[derive(Debug)]
pub struct MultiStepResult {
    pub results: Vec<SearchResult>,
    /// Anzahl der tatsächlich durchgeführten Runden.
    pub rounds_executed: usize,
    /// Queries die in den Folgerunden verwendet wurden.
    pub sub_queries: Vec<String>,
}

// ANCHOR[MULTISTEP:QUERY-REWRITER] STATUS:DONE (TS:2026-06-01T00:00:00Z) — External QueryRewriter trait contract and error isolation.
// TRACKING-ISSUE: #142 (Ollama / LLM-based QueryRewriter implementation in memfuse-ollama crate)

/// Multi-Step Retrieval Engine.
///
/// Implementiert iteratives Query-Rewriting für komplexe Agenten-Abfragen.
/// Erfordert ein `QueryRewriter`-Trait für LLM-basiertes Rewriting.
pub struct MultiStepEngine<S: StorageEngine> {
    collection: Arc<Collection<S>>,
    config: MultiStepConfig,
}

/// Trait für Query-Rewriting (LLM-agnostisch).
#[async_trait::async_trait]
pub trait QueryRewriter: Send + Sync {
    /// Generiert alternative Teil-Queries basierend auf bisherigen Ergebnissen.
    ///
    /// `original_query` – die ursprüngliche Anfrage
    /// `current_results` – bisherige Ergebnisse (leer bei erstem Aufruf)
    /// Gibt leeren Vec zurück wenn kein Rewriting nötig.
    async fn rewrite(
        &self,
        original_query: &str,
        current_results: &[SearchResult],
    ) -> Result<Vec<String>>;
}

impl<S: StorageEngine> MultiStepEngine<S> {
    pub fn new(collection: Arc<Collection<S>>, config: MultiStepConfig) -> Self {
        Self { collection, config }
    }

    /// Führt iterative Hybrid-Suche durch.
    ///
    /// Runde 1: Standard-Hybrid-Suche mit `original_query`.
    /// Runde 2–N: Falls Qualität unzureichend, QueryRewriter generiert Sub-Queries.
    /// Ergebnisse werden via RRF über alle Runden fusioniert.
    ///
    /// # Note on Sub-Query Embeddings
    /// Sub-queries (Rounds 2-N) use BM25-only search (empty vector).
    /// The original query's vector search results contribute via RRF from Round 1.
    /// For full semantic sub-query search, see TRACKING-ISSUE #143.
    pub async fn search(
        &self,
        original_query: &str,
        vector: &[f32],
        k: usize,
        rewriter: Option<&dyn QueryRewriter>,
    ) -> Result<MultiStepResult> {
        use crate::fusion::reciprocal_rank_fusion;

        let k = k.min(memfuse_core::MAX_SEARCH_K);
        let mut all_result_sets: Vec<Vec<SearchResult>> = Vec::new();
        let mut sub_queries: Vec<String> = Vec::new();
        let mut rounds_executed = 0;

        // Runde 1: Standard-Suche
        let round1 = self
            .collection
            .hybrid_search(original_query, vector, k * 2, None)
            .await?;
        all_result_sets.push(round1.clone());
        rounds_executed += 1;

        // Qualitätsprüfung
        if self.quality_sufficient(&round1) || rewriter.is_none() {
            let fused = reciprocal_rank_fusion(all_result_sets, k);
            return Ok(MultiStepResult {
                results: fused,
                rounds_executed,
                sub_queries,
            });
        }

        // Runde 2–max_rounds: Query-Rewriting
        let rewriter = match rewriter {
            Some(r) => r,
            None => {
                let fused = reciprocal_rank_fusion(all_result_sets, k);
                return Ok(MultiStepResult {
                    results: fused,
                    rounds_executed,
                    sub_queries,
                });
            }
        };

        let mut current_results = round1;

        for _round in 2..=self.config.max_rounds {
            let sub_qs = match rewriter.rewrite(original_query, &current_results).await {
                Ok(qs) => qs,
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "QueryRewriter.rewrite() failed in round; stopping expansion gracefully"
                    );
                    break;
                }
            };

            if sub_qs.is_empty() {
                break;
            }

            let mut executed_sub_query = false;
            for sub_q in &sub_qs {
                // Sub-queries use BM25-only (empty vector slice) because:
                // 1. Sub-queries are textual reformulations, not semantic shifts
                // 2. Generating sub-query embeddings requires an embedder dependency
                // 3. The original vector contributes via Round-1 results in RRF fusion
                // ANCHOR[MULTISTEP:SUBQUERY-EMBEDDING] STATUS:DONE (TS:2026-06-01T00:00:00Z) — See TRACKING-ISSUE #143 for
                // future improvement: inject TextEmbeddingEngine for sub-query vectors.
                match self.collection.hybrid_search(sub_q, &[], k, None).await {
                    Ok(sub_results) => {
                        all_result_sets.push(sub_results.clone());
                        current_results = sub_results;
                        sub_queries.push(sub_q.clone());
                        executed_sub_query = true;
                    }
                    Err(e) => {
                        tracing::warn!(
                            sub_query = %sub_q,
                            error = %e,
                            "Sub-query search failed in multi-step execution; skipping sub-query"
                        );
                    }
                }
            }

            if executed_sub_query {
                rounds_executed += 1;
            }

            if self.quality_sufficient(&current_results) {
                break;
            }
        }

        let fused = reciprocal_rank_fusion(all_result_sets, k);
        Ok(MultiStepResult {
            results: fused,
            rounds_executed,
            sub_queries,
        })
    }

    fn quality_sufficient(&self, results: &[SearchResult]) -> bool {
        let high_quality = results
            .iter()
            .filter(|r| r.score >= self.config.quality_threshold)
            .count();
        high_quality >= self.config.min_quality_hits
    }
}

/// Ollama-basierter QueryRewriter.
/// Implementierung in `memfuse-ollama` – hier nur Stub-Trait.
pub struct OllamaQueryRewriter {
    // client: Arc<memfuse_ollama::OllamaClient>,
    // model: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use memfuse_graph::CsrGraph;
    use memfuse_index::{HnswConfig, HnswIndex};
    use memfuse_store::{LsmConfig, LsmStorage};
    use std::sync::atomic::AtomicU64;
    use tempfile::tempdir;

    struct DummyRewriter {
        responses: std::sync::Mutex<Vec<Vec<String>>>,
    }

    #[async_trait::async_trait]
    impl QueryRewriter for DummyRewriter {
        async fn rewrite(
            &self,
            _original_query: &str,
            _current_results: &[SearchResult],
        ) -> Result<Vec<String>> {
            let mut guard = self.responses.lock().unwrap(); // unwrap
            if !guard.is_empty() {
                Ok(guard.remove(0))
            } else {
                Ok(vec![])
            }
        }
    }

    async fn create_test_collection() -> Arc<Collection<LsmStorage>> {
        let dir = tempdir().expect("tempdir"); // expect
        let lsm_config = LsmConfig {
            path: dir.path().to_path_buf(),
            ..Default::default()
        };
        let storage = Arc::new(LsmStorage::new(lsm_config).await.expect("lsm storage")); // expect
        let hnsw_config = HnswConfig {
            dimension: 4,
            ..Default::default()
        };
        let index = Arc::new(HnswIndex::try_new(hnsw_config).expect("hnsw index")); // expect
        let graph = Arc::new(CsrGraph::new());
        let next_tx = Arc::new(AtomicU64::new(1));

        let col = Collection::new(
            "default".to_string(),
            storage,
            index,
            graph,
            next_tx,
            4,
            memfuse_text::Language::English,
        );

        Arc::new(col)
    }

    #[tokio::test]
    async fn test_multistep_single_round_sufficient() {
        let col = create_test_collection().await;
        col.insert(
            "doc1",
            &[1.0, 0.0, 0.0, 0.0],
            Some(serde_json::json!({"text": "rust programming"})),
        )
        .await
        .expect("insert"); // expect
        col.insert(
            "doc2",
            &[0.9, 0.1, 0.0, 0.0],
            Some(serde_json::json!({"text": "rust language"})),
        )
        .await
        .expect("insert"); // expect

        let config = MultiStepConfig {
            max_rounds: 3,
            quality_threshold: 0.001,
            min_quality_hits: 1,
        };
        let engine = MultiStepEngine::new(col, config);

        let rewriter = DummyRewriter {
            responses: std::sync::Mutex::new(vec![vec!["sub query 1".to_string()]]),
        };

        let result = engine
            .search("rust", &[1.0, 0.0, 0.0, 0.0], 5, Some(&rewriter))
            .await
            .expect("search"); // expect

        assert_eq!(result.rounds_executed, 1);
        assert!(result.sub_queries.is_empty());
        assert!(!result.results.is_empty());
    }

    #[tokio::test]
    async fn test_multistep_query_rewriting_triggers() {
        let col = create_test_collection().await;
        col.insert(
            "doc1",
            &[1.0, 0.0, 0.0, 0.0],
            Some(serde_json::json!({"text": "rust programming"})),
        )
        .await
        .expect("insert"); // expect

        let config = MultiStepConfig {
            max_rounds: 3,
            quality_threshold: 0.99, // high threshold, round 1 won't meet min_quality_hits=2
            min_quality_hits: 2,
        };
        let engine = MultiStepEngine::new(col, config);

        let rewriter = DummyRewriter {
            responses: std::sync::Mutex::new(vec![
                vec!["rust programming".to_string()], // round 2
                vec![],                               // round 3 (stop)
            ]),
        };

        let result = engine
            .search("rust", &[1.0, 0.0, 0.0, 0.0], 5, Some(&rewriter))
            .await
            .expect("search"); // expect

        assert_eq!(result.rounds_executed, 2);
        assert_eq!(result.sub_queries, vec!["rust programming"]);
        assert!(!result.results.is_empty());
    }

    #[tokio::test]
    async fn test_multistep_no_rewriter_provided() {
        let col = create_test_collection().await;
        let config = MultiStepConfig::default();
        let engine = MultiStepEngine::new(col, config);

        let result = engine
            .search("query", &[1.0, 0.0, 0.0, 0.0], 5, None)
            .await
            .expect("search"); // expect

        assert_eq!(result.rounds_executed, 1);
        assert!(result.sub_queries.is_empty());
    }

    #[tokio::test]
    async fn test_multistep_subquery_uses_bm25_only() {
        let col = create_test_collection().await;
        col.insert(
            "doc1",
            &[1.0, 0.0, 0.0, 0.0],
            Some(serde_json::json!({"text": "rust programming language"})),
        )
        .await
        .expect("insert"); // expect

        let config = MultiStepConfig {
            max_rounds: 2,
            quality_threshold: 0.99,
            min_quality_hits: 2,
        };
        let engine = MultiStepEngine::new(col, config);

        let rewriter = DummyRewriter {
            responses: std::sync::Mutex::new(vec![vec!["rust programming".to_string()]]),
        };

        let result = engine
            .search("original", &[0.1, 0.2, 0.3, 0.4], 5, Some(&rewriter))
            .await
            .expect("search"); // expect

        assert_eq!(result.rounds_executed, 2);
        assert_eq!(result.sub_queries, vec!["rust programming"]);
        assert!(!result.results.is_empty());
    }

    struct FailingRewriter;

    #[async_trait::async_trait]
    impl QueryRewriter for FailingRewriter {
        async fn rewrite(
            &self,
            _original_query: &str,
            _current_results: &[SearchResult],
        ) -> Result<Vec<String>> {
            Err(memfuse_core::MemFuseError::Internal(
                "Rewriter error".into(),
            ))
        }
    }

    #[tokio::test]
    async fn test_multistep_failing_rewriter_gracefully_stops() {
        let col = create_test_collection().await;
        col.insert(
            "doc1",
            &[1.0, 0.0, 0.0, 0.0],
            Some(serde_json::json!({"text": "rust programming"})),
        )
        .await
        .expect("insert");

        let config = MultiStepConfig {
            max_rounds: 3,
            quality_threshold: 0.99,
            min_quality_hits: 2,
        };
        let engine = MultiStepEngine::new(col, config);
        let rewriter = FailingRewriter;

        let result = engine
            .search("rust", &[1.0, 0.0, 0.0, 0.0], 5, Some(&rewriter))
            .await
            .expect("search should succeed gracefully even if rewriter fails");

        assert_eq!(result.rounds_executed, 1);
        assert!(result.sub_queries.is_empty());
    }
}
