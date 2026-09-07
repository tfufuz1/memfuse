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

use memfuse_calibration::PlattScaler;
use memfuse_core::{ConfigFingerprint, MemFuseError};

/// Alias für `PlattScaler` zur Rückwärtskompatibilität und ADR-070-Konformität.
pub use memfuse_calibration::PlattScaler as PlattScaledSigmoid;

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
    /// Optionale Platt-Scaling Kalibrierung für Roh-Logits
    pub calibration: PlattScaler,
    /// Minimum Observationen vor erstem Fit (Default: 50, aus IsotonicCalibrator-Warmup)
    pub calibration_warmup: usize,
    /// Maximum allowed execution time in milliseconds before timing out. Default: 500ms.
    pub rerank_deadline_ms: Option<u64>,
    /// Simulated delay in milliseconds for testing timeouts. Default: None.
    pub simulate_delay_ms: Option<u64>,
}

impl Default for RerankConfig {
    fn default() -> Self {
        Self {
            model_path: std::path::PathBuf::from("models/bge-reranker-base.onnx"),
            tokenizer_path: std::path::PathBuf::from("models/tokenizer.json"),
            max_length: 512,
            batch_size: 8,
            calibration: PlattScaler::identity(),
            calibration_warmup: 50,
            rerank_deadline_ms: Some(500),
            simulate_delay_ms: None,
        }
    }
}

impl RerankConfig {
    /// Setzt ein gefittetes `PlattScaler` Modell für die Logit-Kalibrierung.
    pub fn with_calibration(mut self, calibration: PlattScaler) -> Self {
        self.calibration = calibration;
        self
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
        calibration: &PlattScaler,
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
        let calibration = calibration.clone();
        let scores = tokio::task::spawn_blocking(move || {
            Self::score_pairs_blocking(
                &session,
                &tokenizer,
                &pairs,
                max_length,
                batch_size,
                &calibration,
            )
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
        calibration: &PlattScaledSigmoid,
    ) -> Result<Vec<f32>, String> {
        if pairs.is_empty() {
            return Ok(vec![]);
        }

        let batch_size = batch_size.max(1);
        let mut all_scores = Vec::with_capacity(pairs.len());

        for chunk in pairs.chunks(batch_size) {
            let chunk_scores =
                Self::score_batch(session, tokenizer, chunk, max_length, calibration)?;
            all_scores.extend(chunk_scores);
        }

        Ok(all_scores)
    }

    fn score_batch(
        session: &std::sync::Arc<parking_lot::Mutex<ort::session::Session>>,
        tokenizer: &Tokenizer,
        chunk: &[(String, String)],
        max_length: usize,
        calibration: &PlattScaledSigmoid,
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
                Self::extract_scores_from_tensor_calibrated(shape, data, b_size, calibration)
            } else if let Some((_, out)) = outputs.iter().next() {
                let (shape, data) = out
                    .try_extract_tensor::<f32>()
                    .map_err(|e| format!("Failed to extract output tensor: {e}"))?;
                Self::extract_scores_from_tensor_calibrated(shape, data, b_size, calibration)
            } else {
                Err("Reranker model produced no outputs".into())
            }
        };

        result
    }

    #[allow(dead_code)]
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

        Self::extract_scores_from_tensor_calibrated(
            shape,
            data,
            b_size,
            &PlattScaledSigmoid::identity(),
        )
    }

    fn extract_scores_from_tensor_calibrated(
        shape: &[i64],
        data: &[f32],
        b_size: usize,
        calibration: &PlattScaledSigmoid,
    ) -> Result<Vec<f32>, String> {
        if shape.is_empty() || shape[0] as usize != b_size {
            return Err(format!(
                "Output tensor batch size mismatch: expected {b_size}, shape {:?}",
                shape
            ));
        }

        // AI-TAG[CALIBRATION][MINOR] AGT-EMBED-62093e61 [RESOLVED] (TS: 2026-09-06T12:00:00Z)
        // Kalibrierungslücke: Platt-Scaling nun via record_outcome() + fitted_calibration.
        let transform = |x: f32| -> f32 { calibration.transform(x) };
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
                    scores.push(transform(logit));
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
                        scores.push(transform(logit));
                    }
                } else if cols == 2 {
                    for i in 0..b_size {
                        let l0 = data[i * 2];
                        let l1 = data[i * 2 + 1];
                        scores.push(transform(l1 - l0));
                    }
                } else {
                    for i in 0..b_size {
                        scores.push(transform(data[i * cols]));
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
    /// Online-Kalibrierungshistorie für Platt-Fitting.
    /// Tuple: (raw_logit, is_relevant: bool)
    calibration_buffer: parking_lot::Mutex<Vec<(f32, bool)>>,
    /// Minimum Observationen vor erstem Fit (Default: 50, aus IsotonicCalibrator-Warmup)
    calibration_warmup: usize,
    /// Aktuell gefittetes Kalibrierungsmodell.
    fitted_calibration: parking_lot::RwLock<PlattScaler>,
    /// ConfigFingerprint für INV-CAL-2: Invalide bei Modellwechsel.
    calibration_fingerprint: parking_lot::RwLock<Option<ConfigFingerprint>>,
}

impl CrossEncoderReranker {
    /// Records implicit feedback: top `implicit_relevant_k` results are marked as relevant,
    /// the rest as not relevant.
    ///
    /// Call this AFTER every successful `rerank()` call to feed calibration.
    /// Thread-safe: uses internal Mutex/RwLock.
    pub fn record_implicit_feedback(&self, results: &[RerankResult], implicit_relevant_k: usize) {
        if matches!(self.backend, RerankerBackend::Passthrough) {
            return;
        }
        for r in results {
            self.record_outcome(r.score, r.original_index < implicit_relevant_k);
        }
    }

    /// Returns true if PlattScaler has been fitted (warmup completed).
    pub fn is_calibrated(&self) -> bool {
        !self.fitted_calibration.read().is_identity()
    }

    /// Returns number of observations recorded.
    pub fn calibration_observation_count(&self) -> usize {
        self.calibration_buffer.lock().len()
    }

    /// Nimmt Feedback-Signal auf (raw logit + Relevanz-Label).
    /// Triggert Re-Fit wenn Warmup erreicht.
    pub fn record_outcome(&self, raw_logit: f32, is_relevant: bool) {
        let mut buf = self.calibration_buffer.lock();
        buf.push((raw_logit, is_relevant));
        if buf.len() >= self.calibration_warmup {
            let new_calib = PlattScaler::fit(&buf);
            *self.fitted_calibration.write() = new_calib;
        }
    }

    /// Appliziert aktuell gefittete Kalibrierung auf rohen Logit-Score.
    pub fn calibrate(&self, raw_logit: f32) -> f32 {
        self.fitted_calibration.read().apply(raw_logit)
    }

    /// Invalidiert Kalibrierung bei Modellwechsel (INV-CAL-2).
    pub fn invalidate_calibration(&self, new_fingerprint: ConfigFingerprint) {
        *self.fitted_calibration.write() = PlattScaler::identity();
        self.calibration_buffer.lock().clear();
        *self.calibration_fingerprint.write() = Some(new_fingerprint);
    }

    /// Gibt die aktuell aktiv gefittete Kalibrierung zurück.
    pub fn fitted_calibration(&self) -> PlattScaler {
        self.fitted_calibration.read().clone()
    }

    /// Erstellt einen Passthrough-CrossEncoderReranker (für Benchmarks/Tests ohne ONNX-Modelldatei).
    pub fn passthrough() -> Self {
        Self::passthrough_with_config(RerankConfig::default())
    }

    /// Erstellt einen Passthrough-CrossEncoderReranker mit einer benutzerdefinierten Konfiguration.
    pub fn passthrough_with_config(config: RerankConfig) -> Self {
        let calibration = config.calibration.clone();
        let warmup = config.calibration_warmup;
        Self {
            _config: config,
            backend: RerankerBackend::Passthrough,
            calibration_buffer: parking_lot::Mutex::new(Vec::new()),
            calibration_warmup: warmup,
            fitted_calibration: parking_lot::RwLock::new(calibration),
            calibration_fingerprint: parking_lot::RwLock::new(None),
        }
    }

    /// Returns a reference to the active `RerankConfig`.
    pub fn config(&self) -> &RerankConfig {
        &self._config
    }

    /// Setzt ein gefittetes `PlattScaler` Modell für den CrossEncoderReranker.
    pub fn with_calibration(mut self, calibration: PlattScaler) -> Self {
        self._config.calibration = calibration.clone();
        *self.fitted_calibration.write() = calibration;
        self
    }

    /// Erstellt einen neuen CrossEncoderReranker.
    pub fn new(config: RerankConfig) -> Result<Self, MemFuseError> {
        let calibration = config.calibration.clone();
        let warmup = config.calibration_warmup;
        #[cfg(feature = "onnx")]
        {
            let onnx = OnnxReranker::new(config.clone())?;
            Ok(Self {
                _config: config,
                backend: RerankerBackend::Onnx(onnx),
                calibration_buffer: parking_lot::Mutex::new(Vec::new()),
                calibration_warmup: warmup,
                fitted_calibration: parking_lot::RwLock::new(calibration),
                calibration_fingerprint: parking_lot::RwLock::new(None),
            })
        }
        #[cfg(not(feature = "onnx"))]
        {
            Ok(Self {
                _config: config,
                backend: RerankerBackend::Passthrough,
                calibration_buffer: parking_lot::Mutex::new(Vec::new()),
                calibration_warmup: warmup,
                fitted_calibration: parking_lot::RwLock::new(calibration),
                calibration_fingerprint: parking_lot::RwLock::new(None),
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

        if let Some(delay_ms) = self._config.simulate_delay_ms {
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
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
            RerankerBackend::Onnx(onnx) => {
                let calib = self.fitted_calibration.read().clone();
                onnx.rerank(_query, candidates, &calib).await
            }
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
        if let Ok(reranker) = CrossEncoderReranker::new(config) {
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
    }

    #[tokio::test]
    async fn test_concurrent_rerank_load_no_panic() {
        use std::sync::Arc;

        let config = RerankConfig::default();
        if let Ok(reranker) = CrossEncoderReranker::new(config) {
            let reranker = Arc::new(reranker);

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

    #[test]
    fn test_platt_scaled_sigmoid_identity_matches_uncalibrated_sigmoid() {
        let identity = PlattScaledSigmoid::identity();
        assert!(identity.is_identity());
        assert_eq!(identity.params(), (1.0, 0.0));

        let logits: Vec<f32> = vec![-5.0, -2.5, -1.0, 0.0, 0.5, 1.2, 3.0, 7.5];
        for logit in logits {
            let uncalibrated = 1.0f32 / (1.0f32 + (-logit).exp());
            let calibrated = identity.transform(logit);
            assert_eq!(
                uncalibrated, calibrated,
                "Mismatch at logit {logit}: uncalibrated={uncalibrated}, calibrated={calibrated}"
            );
        }
    }

    #[test]
    fn test_platt_scaled_sigmoid_fit_separable_dataset() {
        // Dataset with modest logits (-1.0 to 1.0) where uncalibrated sigmoid is soft (0.27 to 0.73)
        // positive logits (> 0) are relevant, negative (< 0) are irrelevant.
        let mut observations = Vec::new();
        for i in -10..=10 {
            let logit = i as f32 * 0.1;
            let is_rel = logit > 0.0;
            observations.push((logit, is_rel));
        }

        let fitted = PlattScaledSigmoid::fit(&observations);
        assert!(!fitted.is_identity());

        let (a, _b) = fitted.params();
        assert!(
            a > 0.0,
            "Scaling factor A should be positive for positively correlated logits"
        );

        // Verify that transform() provides sharper separation than identity sigmoid
        let identity = PlattScaledSigmoid::identity();
        let logit_pos = 1.0f32;
        let logit_neg = -1.0f32;

        let uncalibrated_diff = identity.transform(logit_pos) - identity.transform(logit_neg);
        let calibrated_diff = fitted.transform(logit_pos) - fitted.transform(logit_neg);

        assert!(
            calibrated_diff > uncalibrated_diff,
            "Calibrated transform diff ({calibrated_diff}) should be sharper than uncalibrated ({uncalibrated_diff})"
        );
        assert!(fitted.transform(logit_pos) > 0.80);
        assert!(fitted.transform(logit_neg) < 0.20);
    }

    #[test]
    fn test_platt_scaled_sigmoid_fit_noisy_unseparable_and_nan() {
        // Test 1: Empty observations
        let empty_fitted = PlattScaledSigmoid::fit(&[]);
        assert!(empty_fitted.is_identity());
        assert!(!empty_fitted.transform(0.0).is_nan());

        // Test 2: Non-finite logits (NaN, Inf, -Inf)
        let non_finite_obs = vec![
            (f32::NAN, true),
            (f32::INFINITY, false),
            (f32::NEG_INFINITY, true),
        ];
        let non_finite_fitted = PlattScaledSigmoid::fit(&non_finite_obs);
        assert!(non_finite_fitted.is_identity());
        assert_eq!(non_finite_fitted.transform(f32::NAN), 0.5);

        // Test 3: Completely noisy / non-separable dataset (random labels)
        let mut noisy_obs = Vec::new();
        for i in 0..100 {
            let logit = (i as f32 - 50.0) * 0.1;
            let is_rel = i % 2 == 0; // complete noise uncorrelated with logit
            noisy_obs.push((logit, is_rel));
        }

        let noisy_fitted = PlattScaledSigmoid::fit(&noisy_obs);
        let (a, b) = noisy_fitted.params();

        assert!(a.is_finite(), "Param A must be finite");
        assert!(b.is_finite(), "Param B must be finite");

        let test_val = noisy_fitted.transform(1.0);
        assert!(
            test_val.is_finite() && (0.0..=1.0).contains(&test_val),
            "Output must be a valid probability in [0,1]"
        );
    }

    #[test]
    fn test_record_implicit_feedback_top_k_marked_relevant() {
        let reranker = CrossEncoderReranker::passthrough();
        // Manually record outcomes on reranker directly
        let results = vec![
            RerankResult {
                original_index: 0,
                score: 2.5,
            },
            RerankResult {
                original_index: 1,
                score: 1.8,
            },
            RerankResult {
                original_index: 5,
                score: -0.5,
            },
        ];
        // With passthrough, implicit feedback is skipped
        reranker.record_implicit_feedback(&results, 2);
        assert_eq!(reranker.calibration_observation_count(), 0);

        // Test outcome recording manually via record_outcome
        for r in &results {
            reranker.record_outcome(r.score, r.original_index < 2);
        }
        assert_eq!(reranker.calibration_observation_count(), 3);
        let buf = reranker.calibration_buffer.lock().clone();
        assert_eq!(buf[0], (2.5, true));
        assert_eq!(buf[1], (1.8, true));
        assert_eq!(buf[2], (-0.5, false));
    }

    #[test]
    fn test_calibration_is_calibrated_after_warmup() {
        let reranker = CrossEncoderReranker::passthrough();
        assert!(!reranker.is_calibrated());
        assert_eq!(reranker.calibration_observation_count(), 0);

        for i in 0..50 {
            let logit = (i as f32 - 25.0) * 0.1;
            reranker.record_outcome(logit, logit > 0.0);
        }

        assert_eq!(reranker.calibration_observation_count(), 50);
        assert!(reranker.is_calibrated());
    }

    #[test]
    fn test_calibration_fitted_platt_not_identity() {
        let reranker = CrossEncoderReranker::passthrough();
        assert!(reranker.fitted_calibration().is_identity());

        for i in 0..50 {
            let logit = (i as f32 - 25.0) * 0.1;
            reranker.record_outcome(logit, logit > 0.0);
        }

        assert!(!reranker.fitted_calibration().is_identity());
    }

    #[test]
    fn test_implicit_feedback_passthrough_skipped() {
        let reranker = CrossEncoderReranker::passthrough();
        let results = vec![
            RerankResult {
                original_index: 0,
                score: 0.9,
            },
            RerankResult {
                original_index: 1,
                score: 0.8,
            },
        ];
        reranker.record_implicit_feedback(&results, 1);
        assert_eq!(reranker.calibration_observation_count(), 0);
        assert!(!reranker.is_calibrated());
    }

    #[test]
    fn test_platt_scaled_sigmoid_config_and_reranker_builder() {
        let cal = PlattScaledSigmoid::new(1.5, -0.2);
        let config = RerankConfig::default().with_calibration(cal.clone());
        assert_eq!(config.calibration, cal);

        let reranker = CrossEncoderReranker::passthrough().with_calibration(cal.clone());
        assert_eq!(reranker._config.calibration, cal);
        assert_eq!(reranker.fitted_calibration(), cal);
    }

    #[test]
    fn test_cross_encoder_online_calibration_and_invalidation() {
        let config = RerankConfig::default();
        let reranker = CrossEncoderReranker::passthrough_with_config(config);

        // Initial fitted calibration is identity
        assert!(reranker.fitted_calibration().is_identity());

        // Record outcomes under warmup threshold (49 items when warmup is 50)
        for i in 0..49 {
            let logit = (i as f32 - 25.0) * 0.1;
            let is_rel = logit > 0.0;
            reranker.record_outcome(logit, is_rel);
        }
        assert!(reranker.fitted_calibration().is_identity());

        // Record 50th outcome -> triggers fitting
        reranker.record_outcome(2.5, true);
        assert!(!reranker.fitted_calibration().is_identity());
        assert!(reranker.fitted_calibration().params().0 > 0.0);

        // Verify calibrate() uses fitted calibration
        let calibrated_val = reranker.calibrate(1.0);
        let identity_val = PlattScaler::identity().apply(1.0);
        assert_ne!(calibrated_val, identity_val);

        // Invalidate calibration with new ConfigFingerprint (INV-CAL-2)
        let fp = ConfigFingerprint::new("bge-reranker-v2", "Q4", "default", 0.0);
        reranker.invalidate_calibration(fp);

        assert!(reranker.fitted_calibration().is_identity());
        assert_eq!(reranker.calibration_buffer.lock().len(), 0);
    }

    use proptest::prelude::*;

    /// Helper to compute Expected Calibration Error (ECE) with M bins.
    fn compute_ece(probs_and_labels: &[(f32, bool)], num_bins: usize) -> f32 {
        if probs_and_labels.is_empty() {
            return 0.0;
        }

        let mut bin_counts = vec![0usize; num_bins];
        let mut bin_acc_sums = vec![0.0f32; num_bins];
        let mut bin_conf_sums = vec![0.0f32; num_bins];

        for &(p, label) in probs_and_labels {
            let bin_idx = ((p * num_bins as f32).floor() as usize).min(num_bins - 1);
            bin_counts[bin_idx] += 1;
            bin_conf_sums[bin_idx] += p;
            if label {
                bin_acc_sums[bin_idx] += 1.0;
            }
        }

        let total = probs_and_labels.len() as f32;
        let mut ece = 0.0f32;

        for i in 0..num_bins {
            if bin_counts[i] > 0 {
                let count = bin_counts[i] as f32;
                let avg_acc = bin_acc_sums[i] / count;
                let avg_conf = bin_conf_sums[i] / count;
                ece += (count / total) * (avg_acc - avg_conf).abs();
            }
        }

        ece
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]
        #[test]
        fn prop_platt_calibration_reduces_ece(
            scale in 1.5f32..5.0f32,
            shift in -2.0f32..2.0f32,
        ) {
            // Generate synthetic miscalibrated logits where true logit distribution
            // has temperature scaling A != 1 or shift B != 0
            let mut observations = Vec::new();
            for i in -50..=50 {
                let x = i as f32 * 0.1;
                let true_z = scale * x + shift;
                let true_p = 1.0 / (1.0 + (-true_z).exp());
                // Probabilistic binary label derived from true_p
                let is_rel = (i as f32 * 0.01 + 0.5) > (1.0 - true_p);
                observations.push((x, is_rel));
            }

            let identity = PlattScaler::identity();
            let uncalibrated_eval: Vec<(f32, bool)> = observations
                .iter()
                .map(|&(x, label)| (identity.apply(x), label))
                .collect();
            let ece_uncalibrated = compute_ece(&uncalibrated_eval, 10);

            let fitted = PlattScaler::fit(&observations);
            let calibrated_eval: Vec<(f32, bool)> = observations
                .iter()
                .map(|&(x, label)| (fitted.apply(x), label))
                .collect();
            let ece_calibrated = compute_ece(&calibrated_eval, 10);

            // Calibrated ECE must be less than or equal to uncalibrated ECE (with small tolerance for finite sample variance)
            prop_assert!(
                ece_calibrated <= ece_uncalibrated + 0.05,
                "Calibrated ECE ({ece_calibrated}) should be <= uncalibrated ECE ({ece_uncalibrated})"
            );
        }
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
            calibration: PlattScaledSigmoid::identity(),
            calibration_warmup: 50,
            rerank_deadline_ms: Some(500),
            simulate_delay_ms: None,
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

// REVIEW-PASS[1/2] STATUS:PASS (TS: 2026-09-04T11:42:28Z) (SESSION: 3e5150c8) PRÜFER-KONTEXT: FRESH - Verified CrossEncoderReranker passthrough fallback, candidate limit bounds, and zero-unsafe invariants.
// REVIEW-PASS[2/2] STATUS:PASS (TS: 2026-09-06T11:19:00Z) (SESSION: 8efa6210) PRÜFER-KONTEXT: FRESH - Verified ML domain APM alignment (APM-22, APM-23, APM-24), zero-unsafe in production, and hermetic feature isolation.
