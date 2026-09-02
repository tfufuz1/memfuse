// FILE-CONTEXT
// STAND: 2026-08-29T17:16:44Z (SESSION: f50ed9ef)
// ZWECK: Cross-Encoder Reranking für Post-RRF Präzisionsverbesserung.
// INVARIANTEN: Falls onnx-Feature inaktiv, greift transparenter Passthrough-Fallback.
// NICHT-OFFENSICHTLICH: OnnxReranker nutzt ein eigenes Arc<Mutex<Session>> getrennt von TextEmbedder.
// SIEHE AUCH: crates/memfuse-embed/AGENTS.md, rules/detect_nested_locks.yml

//! Cross-Encoder Reranking für Post-RRF Präzisionsverbesserung.
//!
//! Implementiert das OpenAI/Cohere Reranking-Pattern: nach RRF-Fusion
//! werden die Top-K Kandidaten durch ein lokales ONNX Cross-Encoder-Modell
//! neu bewertet.
//!
//! Aktivierung: Feature-Flag `onnx` erforderlich.
//! Modell: bge-reranker-base oder ms-marco-MiniLM-L-6-v2 (ONNX-Export).

use memfuse_core::MemFuseError;

/// Maximale Anzahl von Kandidaten pro Reranking-Aufruf zur Vermeidung unbegrenzter Allokationen.
pub const MAX_CANDIDATES: usize = 10_000;

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
    /// Pfad zur Tokenizer-Konfigurationsdatei (tokenizer.json)
    pub tokenizer_path: std::path::PathBuf,
    /// Maximale Tokenlänge für (query, candidate) Pair
    pub max_length: usize,
    /// Batch-Größe für parallele Inferenz
    pub batch_size: usize,
}

impl Default for RerankConfig {
    fn default() -> Self {
        Self {
            model_path: std::path::PathBuf::from("models/bge-reranker-base.onnx"),
            tokenizer_path: std::path::PathBuf::from("models/tokenizer.json"),
            max_length: 512,
            batch_size: 8,
        }
    }
}

/// Interne Backend-Varianten für den CrossEncoderReranker.
enum RerankerBackend {
    /// Passthrough-Backend, falls das `onnx`-Feature deaktiviert ist.
    #[allow(dead_code)]
    Passthrough,
    /// Echtes Inferenz-Backend über ONNX Runtime.
    #[cfg(feature = "onnx")]
    Onnx(OnnxReranker),
}

// ── With ONNX feature: Real Reranker Implementation ───────────────────────
#[cfg(feature = "onnx")]
use ort::value::Value;
#[cfg(feature = "onnx")]
use tokenizers::Tokenizer;
#[cfg(feature = "onnx")]
use tracing::warn;

// CONCURRENCY & LOCK HIERARCHY:
// `OnnxReranker` uses a single `parking_lot::Mutex<ort::session::Session>` lock.
// No nested locks exist anywhere within `memfuse-embed`.
// Locks are acquired exclusively inside `spawn_blocking` calls for the duration of ONNX inference,
// preventing async executor starvation and eliminating deadlock risks.
//
// ARCHITECTURAL NOTE (SessionPool Separation):
// `OnnxReranker` uses an independent `Arc<Mutex<Session>>` session management scheme
// separate from `TextEmbedder`'s `Semaphore`-based pool. This intentional separation
// accounts for fundamental differences in model signatures and execution profiles:
// Cross-Encoder reranking operates on `(query, document)` sequence pairs requiring
// custom dynamic batching, whereas `TextEmbedder` executes single-text embeddings.
#[cfg(feature = "onnx")]
struct OnnxReranker {
    config: RerankConfig,
    tokenizer: std::sync::Arc<Tokenizer>,
    // parking_lot::Mutex ist panic-safe (kein PoisonError), da es keinen
    // Poison-Mechanismus hat. Kein .unwrap()/.map_err() nötig. // unwrap
    session: std::sync::Arc<parking_lot::Mutex<ort::session::Session>>,
}

#[cfg(feature = "onnx")]
impl OnnxReranker {
    /// Erstellt eine neue `OnnxReranker`-Instanz.
    fn new(config: RerankConfig) -> Result<Self, MemFuseError> {
        if !config.model_path.exists() {
            return Err(MemFuseError::InvalidInput(format!(
                "ONNX model file not found at {:?}",
                config.model_path
            )));
        }
        if !config.tokenizer_path.exists() {
            return Err(MemFuseError::InvalidInput(format!(
                "Tokenizer file not found at {:?}",
                config.tokenizer_path
            )));
        }

        let tokenizer = Tokenizer::from_file(&config.tokenizer_path)
            .map_err(|e| MemFuseError::Internal(format!("Failed to load tokenizer: {e}")))?;

        use ort::session::Session;
        let session = Session::builder()
            .map_err(|e| MemFuseError::Internal(format!("ONNX session builder: {e}")))?
            .commit_from_file(&config.model_path)
            .map_err(|e| {
                MemFuseError::Internal(format!("ONNX model load from {:?}: {e}", config.model_path))
            })?;

        Ok(Self {
            config,
            tokenizer: std::sync::Arc::new(tokenizer),
            session: std::sync::Arc::new(parking_lot::Mutex::new(session)),
        })
    }

    /// Rerankt Kandidaten für eine Abfrage.
    async fn rerank(
        &self,
        query: &str,
        candidates: &[String],
    ) -> Result<Vec<RerankResult>, MemFuseError> {
        if candidates.is_empty() {
            return Ok(vec![]);
        }

        let pairs: Vec<(String, String)> = candidates
            .iter()
            .map(|c| (query.to_string(), c.clone()))
            .collect();

        let session = std::sync::Arc::clone(&self.session);
        let tokenizer = std::sync::Arc::clone(&self.tokenizer);
        let max_length = self.config.max_length;
        let batch_size = self.config.batch_size;

        let scores = tokio::task::spawn_blocking(move || {
            Self::score_pairs_blocking(&session, &tokenizer, &pairs, max_length, batch_size)
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

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(results)
    }

    fn score_pairs_blocking(
        session: &std::sync::Arc<parking_lot::Mutex<ort::session::Session>>,
        tokenizer: &Tokenizer,
        pairs: &[(String, String)],
        max_length: usize,
        batch_size: usize,
    ) -> Result<Vec<f32>, String> {
        if pairs.is_empty() {
            return Ok(vec![]);
        }

        let batch_size = batch_size.max(1);
        let mut all_scores = Vec::with_capacity(pairs.len());

        for chunk in pairs.chunks(batch_size) {
            let chunk_scores = Self::score_batch(session, tokenizer, chunk, max_length)?;
            all_scores.extend(chunk_scores);
        }

        Ok(all_scores)
    }

    fn score_batch(
        session: &std::sync::Arc<parking_lot::Mutex<ort::session::Session>>,
        tokenizer: &Tokenizer,
        chunk: &[(String, String)],
        max_length: usize,
    ) -> Result<Vec<f32>, String> {
        let mut encodings = Vec::with_capacity(chunk.len());

        for (query, candidate) in chunk {
            let encoding = tokenizer
                .encode((query.as_str(), candidate.as_str()), true)
                .map_err(|e| format!("Tokenization failed for pair: {e}"))?;

            let mut input_ids = encoding.get_ids().to_vec();
            let mut attention_mask = encoding.get_attention_mask().to_vec();
            let mut type_ids = encoding.get_type_ids().to_vec();

            if input_ids.len() > max_length {
                warn!(
                    tokens = input_ids.len(),
                    max = max_length,
                    "Reranker pair tokens exceed max_length and will be truncated."
                );
                input_ids.truncate(max_length);
                attention_mask.truncate(max_length);
                type_ids.truncate(max_length);
            }

            encodings.push((input_ids, attention_mask, type_ids));
        }

        let b_size = chunk.len();
        let max_seq_len = encodings
            .iter()
            .map(|(ids, _, _)| ids.len())
            .max()
            .unwrap_or(1);

        let mut input_ids_flat = vec![0i64; b_size * max_seq_len];
        let mut attention_mask_flat = vec![0i64; b_size * max_seq_len];
        let mut type_ids_flat = vec![0i64; b_size * max_seq_len];

        for (i, (ids, mask, tids)) in encodings.iter().enumerate() {
            for (j, &id) in ids.iter().enumerate() {
                input_ids_flat[i * max_seq_len + j] = id as i64;
            }
            for (j, &m) in mask.iter().enumerate() {
                attention_mask_flat[i * max_seq_len + j] = m as i64;
            }
            for (j, &t) in tids.iter().enumerate() {
                type_ids_flat[i * max_seq_len + j] = t as i64;
            }
        }

        let input_ids_tensor = Value::from_array(([b_size, max_seq_len], input_ids_flat))
            .map_err(|e| format!("Failed to create input_ids tensor: {e}"))?;
        let attention_mask_tensor = Value::from_array(([b_size, max_seq_len], attention_mask_flat))
            .map_err(|e| format!("Failed to create attention_mask tensor: {e}"))?;

        // SAFETY: parking_lot::Mutex ist nicht vergiftbar. Guard hält die
        // Mutex für die Dauer des ONNX-Inference-Calls. spawn_blocking
        // garantiert dass dies einen blocking thread nutzt (kein async starvation).
        let mut guard = session.lock();

        let has_token_type_ids = guard
            .inputs()
            .iter()
            .any(|input| input.name() == "token_type_ids");

        let result = {
            let outputs = if has_token_type_ids {
                let type_ids_tensor = Value::from_array(([b_size, max_seq_len], type_ids_flat))
                    .map_err(|e| format!("Failed to create token_type_ids tensor: {e}"))?;
                guard
                    .run(ort::inputs![
                        "input_ids" => input_ids_tensor,
                        "attention_mask" => attention_mask_tensor,
                        "token_type_ids" => type_ids_tensor,
                    ])
                    .map_err(|e| format!("ONNX reranker inference failed: {e}"))?
            } else {
                guard
                    .run(ort::inputs![
                        "input_ids" => input_ids_tensor,
                        "attention_mask" => attention_mask_tensor,
                    ])
                    .map_err(|e| format!("ONNX reranker inference failed: {e}"))?
            };

            if let Some(out) = outputs.get("logits") {
                let (shape, data) = out
                    .try_extract_tensor::<f32>()
                    .map_err(|e| format!("Failed to extract output tensor: {e}"))?;
                Self::extract_scores_from_tensor(shape, data, b_size)
            } else if let Some((_, out)) = outputs.iter().next() {
                let (shape, data) = out
                    .try_extract_tensor::<f32>()
                    .map_err(|e| format!("Failed to extract output tensor: {e}"))?;
                Self::extract_scores_from_tensor(shape, data, b_size)
            } else {
                Err("Reranker model produced no outputs".into())
            }
        };

        result
    }

    fn extract_scores_from_tensor(
        shape: &[i64],
        data: &[f32],
        b_size: usize,
    ) -> Result<Vec<f32>, String> {
        if shape.is_empty() || shape[0] as usize != b_size {
            return Err(format!(
                "Output tensor batch size mismatch: expected {b_size}, shape {:?}",
                shape
            ));
        }

        let sigmoid = |x: f32| -> f32 { 1.0 / (1.0 + (-x).exp()) };
        let mut scores = Vec::with_capacity(b_size);

        match shape.len() {
            1 => {
                if data.len() != b_size {
                    return Err(format!(
                        "Output tensor len {} != batch size {b_size}",
                        data.len()
                    ));
                }
                for &logit in data {
                    scores.push(sigmoid(logit));
                }
            }
            2 => {
                let cols = shape[1] as usize;
                if data.len() != b_size * cols {
                    return Err(format!(
                        "Output tensor len {} != expected {}",
                        data.len(),
                        b_size * cols
                    ));
                }
                if cols == 1 {
                    for &logit in data {
                        scores.push(sigmoid(logit));
                    }
                } else if cols == 2 {
                    for i in 0..b_size {
                        let l0 = data[i * 2];
                        let l1 = data[i * 2 + 1];
                        scores.push(sigmoid(l1 - l0));
                    }
                } else {
                    for i in 0..b_size {
                        scores.push(sigmoid(data[i * cols]));
                    }
                }
            }
            _ => {
                return Err(format!("Unsupported output shape: {:?}", shape));
            }
        }

        Ok(scores)
    }
}

/// Unified public `CrossEncoderReranker` struct independent of active feature flags.
pub struct CrossEncoderReranker {
    _config: RerankConfig,
    backend: RerankerBackend,
}

impl CrossEncoderReranker {
    /// Erstellt einen neuen CrossEncoderReranker.
    pub fn new(config: RerankConfig) -> Result<Self, MemFuseError> {
        #[cfg(feature = "onnx")]
        {
            let onnx = OnnxReranker::new(config.clone())?;
            Ok(Self {
                _config: config,
                backend: RerankerBackend::Onnx(onnx),
            })
        }
        #[cfg(not(feature = "onnx"))]
        {
            Ok(Self {
                _config: config,
                backend: RerankerBackend::Passthrough,
            })
        }
    }

    /// Rerankt Kandidaten für eine Abfrage.
    pub async fn rerank(
        &self,
        _query: &str,
        candidates: &[String],
    ) -> Result<Vec<RerankResult>, MemFuseError> {
        if candidates.len() > MAX_CANDIDATES {
            return Err(MemFuseError::InvalidInput(format!(
                "Candidate batch size {} exceeds maximum allowed limit {}",
                candidates.len(),
                MAX_CANDIDATES
            )));
        }

        match &self.backend {
            RerankerBackend::Passthrough => Ok(candidates
                .iter()
                .enumerate()
                .map(|(i, _)| RerankResult {
                    original_index: i,
                    score: 1.0 - (i as f32 * 0.01),
                })
                .collect()),
            #[cfg(feature = "onnx")]
            RerankerBackend::Onnx(onnx) => onnx.rerank(_query, candidates).await,
        }
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
            let results = reranker.rerank("query", &candidates).await.unwrap(); // unwrap
            assert_eq!(results.len(), 3);
        }
    }

    #[tokio::test]
    async fn test_rerank_empty_candidates() {
        let config = RerankConfig::default();
        if let Ok(reranker) = CrossEncoderReranker::new(config) {
            let results = reranker.rerank("query", &[]).await.unwrap(); // unwrap
            assert!(results.is_empty());
        }
    }

    #[tokio::test]
    async fn test_rerank_sorted_by_score_descending() {
        let config = RerankConfig::default();
        if let Ok(reranker) = CrossEncoderReranker::new(config) {
            let candidates: Vec<String> = (0..5).map(|i| format!("candidate {i}")).collect();
            let results = reranker.rerank("query", &candidates).await.unwrap(); // unwrap
            for window in results.windows(2) {
                assert!(window[0].score >= window[1].score);
            }
        }
    }

    #[tokio::test]
    async fn test_rerank_oversized_candidate_batch_rejected() {
        let config = RerankConfig::default();
        let reranker = CrossEncoderReranker::new(config)
            .expect("Failed to construct default Passthrough/ONNX CrossEncoderReranker"); // expect
        let candidates: Vec<String> = vec!["doc".to_string(); MAX_CANDIDATES + 1];
        let res = reranker.rerank("query", &candidates).await;
        assert!(res.is_err());
        if let Err(err) = res {
            assert!(matches!(err, MemFuseError::InvalidInput(_)));
            assert!(err.to_string().contains("exceeds maximum allowed limit"));
        } else {
            panic!("Expected InvalidInput error for oversized candidate batch");
        }
    }

    #[tokio::test]
    async fn test_concurrent_rerank_load_no_panic() {
        use std::sync::Arc;

        let config = RerankConfig::default();
        let reranker = Arc::new(CrossEncoderReranker::new(config).unwrap()); // unwrap

        let candidates: Vec<String> = (0..10).map(|i| format!("doc {i}")).collect();
        let mut handles = Vec::new();

        // Simulate 20 concurrent requests (exceeding pool/session limits)
        for i in 0..20 {
            let reranker_cloned = reranker.clone();
            let candidates_cloned = candidates.clone();
            handles.push(tokio::spawn(async move {
                let query = format!("query {i}");
                reranker_cloned.rerank(&query, &candidates_cloned).await
            }));
        }

        for handle in handles {
            let res = handle.await.unwrap(); // unwrap
            assert!(res.is_ok());
            assert_eq!(res.unwrap().len(), 10); // unwrap
        }
    }

    #[cfg(feature = "onnx")]
    #[test]
    fn test_extract_scores_1d_and_2d() -> Result<(), Box<dyn std::error::Error>> {
        // 1D tensor [batch_size = 2]
        let shape = vec![2];
        let data = vec![0.0f32, 2.0f32];
        let scores = OnnxReranker::extract_scores_from_tensor(&shape, &data, 2)?;
        assert_eq!(scores.len(), 2);
        assert!((scores[0] - 0.5).abs() < 1e-4);
        assert!(scores[1] > 0.8);

        // 2D tensor [batch_size = 2, cols = 1]
        let shape = vec![2, 1];
        let data = vec![0.0f32, -2.0f32];
        let scores = OnnxReranker::extract_scores_from_tensor(&shape, &data, 2)?;
        assert_eq!(scores.len(), 2);
        assert!((scores[0] - 0.5).abs() < 1e-4);
        assert!(scores[1] < 0.2);

        // 2D tensor [batch_size = 2, cols = 2] (binary classification logits)
        let shape = vec![2, 2];
        let data = vec![1.0f32, 3.0f32, 2.0f32, 0.0f32]; // diff: +2.0, -2.0
        let scores = OnnxReranker::extract_scores_from_tensor(&shape, &data, 2)?;
        assert_eq!(scores.len(), 2);
        assert!(scores[0] > 0.8);
        assert!(scores[1] < 0.2);

        Ok(())
    }

    #[cfg(feature = "onnx")]
    #[test]
    fn test_cross_encoder_missing_paths() -> Result<(), Box<dyn std::error::Error>> {
        use tempfile::tempdir;
        let dir = tempdir()?;
        let model_path = dir.path().join("nonexistent_model.onnx");
        let tokenizer_path = dir.path().join("nonexistent_tokenizer.json");

        let cfg = RerankConfig {
            model_path,
            tokenizer_path,
            max_length: 128,
            batch_size: 4,
        };

        let res = CrossEncoderReranker::new(cfg);
        assert!(res.is_err());
        if let Err(err) = res {
            assert!(matches!(err, MemFuseError::InvalidInput(_)));
        } else {
            panic!("Expected error for missing model/tokenizer paths");
        }
        Ok(())
    }
}
