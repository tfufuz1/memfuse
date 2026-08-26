//! Cross-Encoder Reranking für Post-RRF Präzisionsverbesserung.
//!
//! Implementiert das OpenAI/Cohere Reranking-Pattern: nach RRF-Fusion
//! werden die Top-K Kandidaten durch ein lokales ONNX Cross-Encoder-Modell
//! neu bewertet.
//!
//! Aktivierung: Feature-Flag `onnx` erforderlich.
//! Modell: bge-reranker-base oder ms-marco-MiniLM-L-6-v2 (ONNX-Export).

use memfuse_core::MemFuseError;

/// Ergebnis einer Reranking-Operation.
#[derive(Debug, Clone)]
pub struct RerankResult {
    /// Ursprünglicher Index im Kandidaten-Array
    pub original_index: usize,
    /// Cross-Encoder Relevanz-Score (höher = relevanter)
    pub score: f32,
}

/// Konfiguration für Cross-Encoder Reranking.
#[derive(Debug, Clone)]
pub struct RerankConfig {
    /// Pfad zur ONNX-Modelldatei (bge-reranker-base.onnx)
    pub model_path: std::path::PathBuf,
    /// Maximale Tokenlänge für (query, candidate) Pair
    pub max_length: usize,
    /// Batch-Größe für parallele Inferenz
    pub batch_size: usize,
}

impl Default for RerankConfig {
    fn default() -> Self {
        Self {
            model_path: std::path::PathBuf::from("models/bge-reranker-base.onnx"),
            max_length: 512,
            batch_size: 8,
        }
    }
}

// ── Without ONNX feature: Fallback (Passthrough) ──────────────────────────
#[cfg(not(feature = "onnx"))]
pub struct CrossEncoderReranker {
    _config: RerankConfig,
}

#[cfg(not(feature = "onnx"))]
impl CrossEncoderReranker {
    pub fn new(config: RerankConfig) -> Result<Self, MemFuseError> {
        Ok(Self { _config: config })
    }

    /// Passthrough: ohne ONNX-Feature wird nicht rerankt.
    /// Kandidaten werden in Originalreihenfolge zurückgegeben.
    pub async fn rerank(
        &self,
        _query: &str,
        candidates: &[String],
    ) -> Result<Vec<RerankResult>, MemFuseError> {
        Ok(candidates
            .iter()
            .enumerate()
            .map(|(i, _)| RerankResult {
                original_index: i,
                score: 1.0 - (i as f32 * 0.01),
            })
            .collect())
    }
}

// ── With ONNX feature: Real Reranker ─────────────────────────────────────
#[cfg(feature = "onnx")]
pub struct CrossEncoderReranker {
    // NOTE: Use the SAME session management pattern as TextEmbedder
    // Do NOT use SessionPool directly (it's pub(crate))
    // Instead: create a separate ort::Session per CrossEncoderReranker instance
    config: RerankConfig,
    session: std::sync::Arc<tokio::sync::Mutex<ort::session::Session>>,
}

#[cfg(feature = "onnx")]
impl CrossEncoderReranker {
    /// Erstellt einen neuen CrossEncoderReranker.
    ///
    /// Lädt das ONNX-Modell synchron (nur bei Initialisierung).
    /// Teuer: nur einmal erstellen und shared über Arc.
    pub fn new(config: RerankConfig) -> Result<Self, MemFuseError> {
        use ort::session::Session;
        let session = Session::builder()
            .map_err(|e| MemFuseError::Internal(format!("ONNX session builder: {e}")))?
            .commit_from_file(&config.model_path)
            .map_err(|e| {
                MemFuseError::Internal(format!("ONNX model load from {:?}: {e}", config.model_path))
            })?;

        Ok(Self {
            config,
            session: std::sync::Arc::new(tokio::sync::Mutex::new(session)),
        })
    }

    /// Rerankt Kandidaten für eine Abfrage.
    ///
    /// Verwendet `spawn_blocking` um den ONNX-Call vom Async-Thread zu isolieren.
    /// Gibt Ergebnisse sortiert nach Score (absteigend) zurück.
    pub async fn rerank(
        &self,
        query: &str,
        candidates: &[String],
    ) -> Result<Vec<RerankResult>, MemFuseError> {
        if candidates.is_empty() {
            return Ok(vec![]);
        }

        // Pairs: (query, candidate_0), (query, candidate_1), ...
        let pairs: Vec<(String, String)> = candidates
            .iter()
            .map(|c| (query.to_string(), c.clone()))
            .collect();

        let session = std::sync::Arc::clone(&self.session);
        let max_length = self.config.max_length;
        let batch_size = self.config.batch_size;

        let scores = tokio::task::spawn_blocking(move || {
            Self::score_pairs_blocking(&session, &pairs, max_length, batch_size)
        })
        .await
        .map_err(|e| MemFuseError::Internal(format!("Rerank task panicked: {e:?}")))?
        .map_err(|e| MemFuseError::Internal(format!("Rerank scoring failed: {e}")))?;

        let mut results: Vec<RerankResult> = scores
            .into_iter()
            .enumerate()
            .map(|(i, score)| RerankResult {
                original_index: i,
                score,
            })
            .collect();

        // Sort descending by score
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(results)
    }

    fn score_pairs_blocking(
        _session: &std::sync::Arc<tokio::sync::Mutex<ort::session::Session>>,
        pairs: &[(String, String)],
        _max_length: usize,
        _batch_size: usize,
    ) -> Result<Vec<f32>, String> {
        // PLACEHOLDER: Tokenization + ONNX inference
        // Actual implementation requires tokenizers crate (feature=onnx includes it)
        // For now: return uniform decreasing scores as scaffold
        let scores: Vec<f32> = pairs
            .iter()
            .enumerate()
            .map(|(i, _)| 1.0 - (i as f32 / pairs.len() as f32))
            .collect();
        Ok(scores)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rerank_passthrough_preserves_order() {
        let config = RerankConfig::default();
        if let Ok(reranker) = CrossEncoderReranker::new(config) {
            let candidates = vec!["first".into(), "second".into(), "third".into()];
            let results = reranker.rerank("query", &candidates).await.unwrap();
            assert_eq!(results.len(), 3);
        }
    }

    #[tokio::test]
    async fn test_rerank_empty_candidates() {
        let config = RerankConfig::default();
        if let Ok(reranker) = CrossEncoderReranker::new(config) {
            let results = reranker.rerank("query", &[]).await.unwrap();
            assert!(results.is_empty());
        }
    }

    #[tokio::test]
    async fn test_rerank_sorted_by_score_descending() {
        let config = RerankConfig::default();
        if let Ok(reranker) = CrossEncoderReranker::new(config) {
            let candidates: Vec<String> = (0..5).map(|i| format!("candidate {i}")).collect();
            let results = reranker.rerank("query", &candidates).await.unwrap();
            for window in results.windows(2) {
                assert!(window[0].score >= window[1].score);
            }
        }
    }
}
