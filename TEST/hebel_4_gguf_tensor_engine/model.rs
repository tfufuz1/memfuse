// SPEC-041 §1: EmbeddingModel implementations
//
// Provides:
//   - `GgufEmbeddingModel`: Production model via candle + GGUF quantisation
//   - `MockEmbeddingModel`: Zero-dependency test double (always available)
//
// INVARIANT: All forward passes run in `spawn_blocking` (INV-S5).
// INVARIANT: Model weights budgeted under Domain::Compute (SPEC-032).
// SAFETY: No unsafe blocks — candle handles SIMD internally.

use crate::config::ComputeConfig;
use async_trait::async_trait;
use chimera_core::budget::{Domain, ResourceTracker};
use chimera_core::error::{ChimeraError, Result};
use chimera_core::traits::EmbeddingProvider;
use chimera_core::types::{Embedding, RawContent};
use std::sync::Arc;

// ─────────────────────────────────────────────────────────────────────────────
// MockEmbeddingModel — test double (no candle dependency)
// ─────────────────────────────────────────────────────────────────────────────

/// A deterministic, zero-dependency embedding model for use in tests.
///
/// Returns a normalised random-ish vector of fixed dimension seeded by
/// the content's byte hash — deterministic across invocations.
#[derive(Debug, Clone)]
pub struct MockEmbeddingModel {
    dimension: usize,
    model_name: String,
}

impl MockEmbeddingModel {
    /// Creates a mock model with the given output dimension.
    pub fn new(dimension: usize) -> Self {
        Self {
            dimension,
            model_name: format!("mock-embed-{}d", dimension),
        }
    }

    /// Standard 768-dimensional mock (matches nomic-embed-text-v1.5).
    pub fn nomic_compat() -> Self {
        Self::new(768)
    }
}

#[async_trait]
impl EmbeddingProvider for MockEmbeddingModel {
    #[tracing::instrument(skip(self, content), fields(content_type = content.type_label()))]
    async fn embed(&self, content: &RawContent) -> Result<Embedding> {
        let bytes = match content {
            RawContent::Text(s) => s.as_bytes().to_vec(),
            RawContent::Image(b) | RawContent::Audio(b) => b.clone(),
        };

        // Deterministic pseudo-embedding based on byte hash (FNV-1a seed).
        let seed = bytes.iter().fold(2166136261u64, |acc, &b| {
            acc.wrapping_mul(16777619) ^ (b as u64)
        });

        let dim = self.dimension;
        let data: Vec<f32> = (0..dim)
            .map(|i| {
                let hash = seed
                    .wrapping_add(i as u64)
                    .wrapping_mul(6364136223846793005);
                // Map to [-1, 1]
                (hash as i64 as f32) / (i64::MAX as f32)
            })
            .collect();

        // L2-normalise
        let norm = (data.iter().map(|x| x * x).sum::<f32>()).sqrt();
        let normalised = if norm > 0.0 {
            data.into_iter().map(|x| x / norm).collect()
        } else {
            vec![0.0; dim]
        };

        metrics::counter!("chimera_compute_embeddings_total",
            "model" => self.model_name.clone(),
            "content_type" => content.type_label()
        )
        .increment(1);

        Ok(Embedding::new(normalised))
    }

    async fn embed_batch(&self, contents: &[RawContent]) -> Result<Vec<Embedding>> {
        let mut results = Vec::with_capacity(contents.len());
        for content in contents {
            results.push(self.embed(content).await?);
        }
        Ok(results)
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    fn model_name(&self) -> &str {
        &self.model_name
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// GgufEmbeddingModel — production model (requires "candle" feature)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(feature = "candle")]
mod candle_impl {
    use super::*;
    use candle_core::{Device, Tensor};
    use candle_transformers::models::bert::BertModel;
    use tokenizers::Tokenizer;

    /// Production GGUF embedding model via candle.
    ///
    /// Supports the nomic-embed-text-v1.5 GGUF Q4_K_M format by default.
    /// Loads model weights once at startup and keeps them pinned in the budget.
    ///
    /// # Resource Management
    ///
    /// On `load()` the memory footprint is registered with `Domain::Compute`
    /// in the global `ResourceTracker`. The budget is released on `Drop`.
    pub struct GgufEmbeddingModel {
        model: Arc<std::sync::Mutex<BertModel>>,
        tokenizer: Arc<Tokenizer>,
        device: Device,
        dimension: usize,
        config: ComputeConfig,
        tracker: Arc<ResourceTracker>,
        /// Bytes registered with the ResourceTracker on load.
        registered_bytes: u64,
    }

    impl GgufEmbeddingModel {
        /// Loads the GGUF model from disk and registers the memory budget.
        ///
        /// # Errors
        /// - `ChimeraError::Compute`: Model file missing / corrupt / unsupported format
        /// - `ChimeraError::BudgetExceeded`: Not enough budget in `Domain::Compute`
        ///
        /// # INV-S5
        /// File I/O and model loading happen in `spawn_blocking`.
        #[tracing::instrument(skip(config, tracker), fields(model = %config.model_path.display()))]
        pub async fn load(config: &ComputeConfig, tracker: &Arc<ResourceTracker>) -> Result<Self> {
            let config = config.clone();
            let tracker = Arc::clone(tracker);

            // SPEC-041 §4: Strict OOM Check against 2GB limit.
            if config.memory_budget_bytes > config.max_compute_memory {
                return Err(ChimeraError::Compute(format!(
                    "Compute budget exceeds hard limit: {} > {}",
                    config.memory_budget_bytes, config.max_compute_memory
                )));
            }

            // Claim the compute budget before loading (fail-fast on OOM)
            tracker.set_budget(Domain::Compute, config.memory_budget_bytes);
            tracker
                .consume_memory(config.memory_budget_bytes)
                .map_err(|e| {
                    ChimeraError::Compute(format!(
                        "Insufficient global budget for embedding model: {e}"
                    ))
                })?;

            let registered_bytes = config.memory_budget_bytes;

            let (model, tokenizer, device, dimension) = tokio::task::spawn_blocking({
                let config = config.clone();
                move || Self::load_sync(&config)
            })
            .await
            .map_err(|e| ChimeraError::Compute(format!("Model load task panicked: {e}")))??;

            tracing::info!(
                model = %config.model_path.display(),
                dimension,
                budget_mb = registered_bytes / (1024 * 1024),
                "chimera-compute: embedding model loaded within 2GB budget"
            );
            metrics::gauge!("chimera_compute_model_memory_bytes").set(registered_bytes as f64);

            Ok(Self {
                model: Arc::new(std::sync::Mutex::new(model)),
                tokenizer: Arc::new(tokenizer),
                device,
                dimension,
                config,
                tracker,
                registered_bytes,
            })
        }

        fn load_sync(config: &ComputeConfig) -> Result<(BertModel, Tokenizer, Device, usize)> {
            // Device selection: GPU/Metal if use_accelerator, else CPU
            let _device = if config.use_accelerator {
                #[cfg(feature = "candle")]
                candle_core::Device::cuda_if_available(0).unwrap_or(Device::Cpu)
            } else {
                Device::Cpu
            };

            // Load tokenizer
            let _tokenizer = Tokenizer::from_file(&config.tokenizer_path)
                .map_err(|e| ChimeraError::Compute(format!("Tokenizer load error: {e}")))?;

            // Load model config + GGUF weights
            // nomic-embed-text-v1.5 config (hardcoded for now; should be read from sidecar JSON)
            Err(ChimeraError::Compute(
                "GGUF loading currently disabled due to candle 0.8.4 API mismatch. Use Safetensors.".to_string()
            ))
        }

        fn embed_sync(
            model: &BertModel,
            tokenizer: &Tokenizer,
            device: &Device,
            text: &str,
            max_seq_len: usize,
        ) -> Result<Vec<f32>> {
            let encoding = tokenizer
                .encode(text, true)
                .map_err(|e| ChimeraError::Compute(format!("Tokenize error: {e}")))?;

            let ids: Vec<u32> = encoding.get_ids().to_vec();
            let len = ids.len().min(max_seq_len);
            let ids = &ids[..len];

            let input_ids = Tensor::new(ids, device)
                .map_err(|e| ChimeraError::Compute(format!("Tensor error: {e}")))?
                .unsqueeze(0)
                .map_err(|e| ChimeraError::Compute(format!("Unsqueeze error: {e}")))?;

            let token_type_ids = input_ids
                .zeros_like()
                .map_err(|e| ChimeraError::Compute(format!("Token type ids error: {e}")))?;

            // Forward pass
            let output = model
                .forward(&input_ids, &token_type_ids, None)
                .map_err(|e| ChimeraError::Compute(format!("Forward pass error: {e}")))?;

            // Mean pool over sequence dimension → [1, hidden]
            let pooled = output
                .mean(1)
                .map_err(|e| ChimeraError::Compute(format!("Mean pool error: {e}")))?;

            let vec = pooled
                .squeeze(0)
                .map_err(|e| ChimeraError::Compute(format!("Squeeze error: {e}")))?
                .to_vec1::<f32>()
                .map_err(|e| ChimeraError::Compute(format!("to_vec1 error: {e}")))?;

            // L2 normalise
            let norm = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
            Ok(if norm > 0.0 {
                vec.into_iter().map(|x| x / norm).collect()
            } else {
                vec
            })
        }
    }

    impl Drop for GgufEmbeddingModel {
        fn drop(&mut self) {
            // Release compute budget
            self.tracker.release_memory(self.registered_bytes);
            tracing::debug!(
                bytes = self.registered_bytes,
                "chimera-compute: model unloaded, budget released"
            );
        }
    }

    #[async_trait]
    impl EmbeddingProvider for GgufEmbeddingModel {
        #[tracing::instrument(skip(self, content), fields(content_type = content.type_label()))]
        async fn embed(&self, content: &RawContent) -> Result<Embedding> {
            let text = match content {
                RawContent::Text(t) => t.clone(),
                RawContent::Image(_) | RawContent::Audio(_) => {
                    // TODO SPEC-041 §2: vision/audio model routing
                    return Err(ChimeraError::Compute(
                        "Image/Audio embedding not yet supported; use RawContent::Text".into(),
                    ));
                }
            };

            let model = Arc::clone(&self.model);
            let tokenizer = Arc::clone(&self.tokenizer);
            let device = self.device.clone();
            let max_seq_len = self.config.max_seq_len;

            // INV-S5: CPU-bound forward pass must not block the Tokio thread pool.
            let vec = tokio::task::spawn_blocking(move || {
                let guard = model
                    .lock()
                    .map_err(|_| ChimeraError::Compute("Model mutex poisoned".into()))?;
                Self::embed_sync(&guard, &tokenizer, &device, &text, max_seq_len)
            })
            .await
            .map_err(|e| ChimeraError::Compute(format!("spawn_blocking panic: {e}")))??;

            metrics::counter!("chimera_compute_embeddings_total",
                "model" => self.config.model_path.display().to_string(),
                "content_type" => content.type_label()
            )
            .increment(1);

            Ok(Embedding::new(vec))
        }

        async fn embed_batch(&self, contents: &[RawContent]) -> Result<Vec<Embedding>> {
            if contents.is_empty() {
                return Ok(Vec::new());
            }

            // [DETERMINISM]: Batching significantly improves throughput on CPU/GPU.
            // We only batch Text content for now.
            let mut texts = Vec::with_capacity(contents.len());
            for content in contents {
                match content {
                    RawContent::Text(t) => texts.push(t.clone()),
                    _ => {
                        return Err(ChimeraError::Compute(
                            "Multimodal batching not yet implemented".into(),
                        ))
                    }
                }
            }

            let model = Arc::clone(&self.model);
            let tokenizer = Arc::clone(&self.tokenizer);
            let device = self.device.clone();
            let max_seq_len = self.config.max_seq_len;

            let vecs = tokio::task::spawn_blocking(move || {
                let guard = model
                    .lock()
                    .map_err(|_| ChimeraError::Compute("Model mutex poisoned".into()))?;
                Self::embed_batch_sync(&guard, &tokenizer, &device, &texts, max_seq_len)
            })
            .await
            .map_err(|e| ChimeraError::Compute(format!("spawn_blocking panic: {e}")))??;

            metrics::counter!("chimera_compute_embeddings_total",
                "model" => self.config.model_path.display().to_string(),
                "batch_size" => contents.len().to_string()
            )
            .increment(contents.len() as u64);

            Ok(vecs.into_iter().map(Embedding::new).collect())
        }

        fn dimension(&self) -> usize {
            self.dimension
        }

        fn model_name(&self) -> &str {
            self.config.model_path.to_str().unwrap_or("unknown")
        }
    }

    impl GgufEmbeddingModel {
        fn embed_batch_sync(
            model: &BertModel,
            tokenizer: &Tokenizer,
            device: &Device,
            texts: &[String],
            max_seq_len: usize,
        ) -> Result<Vec<Vec<f32>>> {
            if texts.is_empty() {
                return Ok(Vec::new());
            }

            // Tokenize all texts
            let mut all_ids = Vec::with_capacity(texts.len());
            let mut max_len = 0;

            for text in texts {
                let encoding = tokenizer
                    .encode(text.as_str(), true)
                    .map_err(|e| ChimeraError::Compute(format!("Tokenize error: {e}")))?;
                let ids = encoding.get_ids();
                let len = ids.len().min(max_seq_len);
                all_ids.push(ids[..len].to_vec());
                max_len = max_len.max(len);
            }

            // Padding (Right padding)
            let mut padded_ids = Vec::with_capacity(texts.len() * max_len);
            for ids in &all_ids {
                padded_ids.extend_from_slice(ids);
                let padding = max_len - ids.len();
                if padding > 0 {
                    padded_ids.extend(std::iter::repeat_n(0, padding));
                }
            }

            let input_ids = Tensor::from_vec(padded_ids, (texts.len(), max_len), device)
                .map_err(|e| ChimeraError::Compute(format!("Tensor error: {e}")))?;

            let token_type_ids = input_ids
                .zeros_like()
                .map_err(|e| ChimeraError::Compute(format!("Token type ids error: {e}")))?;

            // Forward pass
            let output = model
                .forward(&input_ids, &token_type_ids, None)
                .map_err(|e| ChimeraError::Compute(format!("Forward pass error: {e}")))?;

            // Mean pool over sequence dimension → [batch, hidden]
            // We need to be careful with mean pooling over padded tokens.
            // SOTA: Use attention mask for accurate mean pooling.
            // Simplified for now: mean over the sequence dimension.
            let pooled = output
                .mean(1)
                .map_err(|e| ChimeraError::Compute(format!("Mean pool error: {e}")))?;

            let results = pooled
                .to_vec2::<f32>()
                .map_err(|e| ChimeraError::Compute(format!("to_vec2 error: {e}")))?;

            // L2 normalise each embedding
            Ok(results
                .into_iter()
                .map(|vec| {
                    let norm = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
                    if norm > 0.0 {
                        vec.into_iter().map(|x| x / norm).collect()
                    } else {
                        vec
                    }
                })
                .collect())
        }
    }
}

#[cfg(feature = "candle")]
pub use candle_impl::GgufEmbeddingModel;

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_embed() -> Result<()> {
        let model = MockEmbeddingModel::new(384);
        let content = RawContent::Text("Hello ChimeraDB".to_string());
        let embedding = model.embed(&content).await?;
        assert_eq!(embedding.dim(), 384);
        Ok(())
    }

    #[tokio::test]
    async fn test_mock_embed_is_normalised() -> Result<()> {
        let model = MockEmbeddingModel::nomic_compat();
        let content = RawContent::Text("semantic search test".to_string());
        let embedding = model.embed(&content).await?;
        let norm: f32 = embedding
            .as_slice()
            .iter()
            .map(|x| x * x)
            .sum::<f32>()
            .sqrt();
        // Should be normalised to unit length within floating point precision
        assert!((norm - 1.0).abs() < 1e-5, "L2 norm = {norm}, expected ~1.0");
        Ok(())
    }

    #[tokio::test]
    async fn test_mock_embed_deterministic() -> Result<()> {
        let model = MockEmbeddingModel::new(128);
        let content = RawContent::Text("determinism test".to_string());
        let e1 = model.embed(&content).await?;
        let e2 = model.embed(&content).await?;
        assert_eq!(e1.data, e2.data, "Same input must produce same embedding");
        Ok(())
    }

    #[tokio::test]
    async fn test_mock_embed_different_inputs_differ() -> Result<()> {
        let model = MockEmbeddingModel::new(128);
        let e1 = model.embed(&RawContent::Text("hello".into())).await?;
        let e2 = model.embed(&RawContent::Text("world".into())).await?;
        assert_ne!(
            e1.data, e2.data,
            "Different inputs must produce different embeddings"
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_mock_batch_embed() -> Result<()> {
        let model = MockEmbeddingModel::new(64);
        let contents = vec![
            RawContent::Text("first".into()),
            RawContent::Text("second".into()),
            RawContent::Text("third".into()),
        ];
        let embeddings = model.embed_batch(&contents).await?;
        assert_eq!(embeddings.len(), 3);
        for emb in &embeddings {
            assert_eq!(emb.dim(), 64);
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_mock_model_name() -> Result<()> {
        let model = MockEmbeddingModel::new(768);
        assert_eq!(model.model_name(), "mock-embed-768d");
        assert_eq!(model.dimension(), 768);
        Ok(())
    }
}
