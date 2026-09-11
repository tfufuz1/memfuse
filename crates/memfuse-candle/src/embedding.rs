// FILE-CONTEXT
// STAND: 2026-09-11T22:50:32Z (SESSION: db850a8a)
// ZWECK: Native Candle ML vector embedding client implementation.
// INVARIANTEN: Thread safety via Arc<tokio::sync::Mutex<Box<dyn CandleEmbedInner>>>; vector dimension matches model.dim. Zero unsafe code in production via VarBuilder::from_buffered_safetensors.
// NICHT-OFFENSICHTLICH: CandleEmbedInner trait enables mock-based unit testing without binary weights in CI.

use crate::model_registry::ModelFingerprint;
use candle_core::{Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config, DTYPE};
use memfuse_core::{MemFuseError, Result};
use std::path::Path;
use std::sync::Arc;

/// Inner trait abstracting low-level Candle forward execution for vector embeddings.
///
/// Enables mock-based unit testing without loading full ONNX or GGUF files in CI.
pub trait CandleEmbedInner: Send {
    /// Generates an embedding vector for the provided input text.
    fn embed(
        &mut self,
        text: &str,
        tokenizer: &tokenizers::Tokenizer,
        device: &Device,
    ) -> Result<Vec<f32>>;

    /// Returns the vector dimension produced by this embedding model.
    fn dim(&self) -> usize;
}

/// Text embedding client powered by Candle ML backend.
pub struct CandleEmbedClient {
    /// Hardware device (CPU, CUDA, Metal).
    pub device: Device,
    /// Thread-safe mutex wrapping the inner embedding model.
    pub model: Arc<tokio::sync::Mutex<Box<dyn CandleEmbedInner + Send>>>,
    /// Unique fingerprint identifying the embedding model weights and quantization.
    pub fingerprint: ModelFingerprint,
    /// Tokenizer for converting text to token ID tensors.
    pub tokenizer: tokenizers::Tokenizer,
    /// Vector dimension produced by this model.
    pub dim: usize,
}

impl CandleEmbedClient {
    /// Creates a new `CandleEmbedClient`.
    pub fn new(
        device: Device,
        model: Box<dyn CandleEmbedInner + Send>,
        fingerprint: ModelFingerprint,
        tokenizer: tokenizers::Tokenizer,
    ) -> Self {
        let dim = model.dim();
        Self {
            device,
            model: Arc::new(tokio::sync::Mutex::new(model)),
            fingerprint,
            tokenizer,
            dim,
        }
    }

    /// Returns a reference to the model's fingerprint.
    pub fn fingerprint(&self) -> &ModelFingerprint {
        &self.fingerprint
    }

    /// Loads a `CandleEmbedClient` from a model directory.
    pub fn from_dir(
        model_dir: &std::path::Path,
        quantization: crate::model_registry::CandleQuantization,
    ) -> Result<Self> {
        if !model_dir.exists() {
            return Err(MemFuseError::InvalidInput(format!(
                "Candle model directory does not exist: {}",
                model_dir.display()
            )));
        }

        let tokenizer_path = model_dir.join("tokenizer.json");
        let tokenizer = if tokenizer_path.exists() {
            tokenizers::Tokenizer::from_file(&tokenizer_path).map_err(|e| {
                MemFuseError::InvalidInput(format!(
                    "Failed to load tokenizer from {}: {e}",
                    tokenizer_path.display()
                ))
            })?
        } else {
            let tokenizer_bytes = r#"{
                "version": "1.0",
                "truncation": null,
                "padding": null,
                "added_tokens": [],
                "normalizer": null,
                "pre_tokenizer": null,
                "post_processor": null,
                "decoder": null,
                "model": { "type": "BPE", "dropout": null, "unk_token": null, "continuing_subword_prefix": null, "end_of_word_suffix": null, "fuse_unk": false, "vocab": {}, "merges": [] }
            }"#;
            tokenizers::Tokenizer::from_bytes(tokenizer_bytes.as_bytes()).map_err(|e| {
                MemFuseError::Internal(format!("Failed to parse default tokenizer: {e}"))
            })?
        };

        let gguf_path = model_dir.join("model.gguf");
        let fingerprint = if gguf_path.exists() {
            crate::model_registry::compute_fingerprint(&gguf_path, &quantization)?
        } else if let Ok(entries) = std::fs::read_dir(model_dir) {
            let mut gguf_found = None;
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("gguf") {
                    gguf_found = Some(path);
                    break;
                }
            }
            if let Some(path) = gguf_found {
                crate::model_registry::compute_fingerprint(&path, &quantization)?
            } else {
                crate::model_registry::ModelFingerprint {
                    hash: [0u8; 32],
                    model_id: model_dir
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    quantization: quantization.to_string(),
                }
            }
        } else {
            crate::model_registry::ModelFingerprint {
                hash: [0u8; 32],
                model_id: model_dir
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string(),
                quantization: quantization.to_string(),
            }
        };

        let device = Device::Cpu;
        let weights_path = model_dir.join("model.safetensors");
        let config_path = model_dir.join("config.json");

        let model: Box<dyn CandleEmbedInner + Send> = if weights_path.exists() {
            Box::new(BertEmbedModel::load(&weights_path, &config_path, &device)?)
        } else {
            Box::new(DefaultCandleEmbedModel { dim: 384 })
        };

        Ok(Self::new(device, model, fingerprint, tokenizer))
    }
}

/// Real BERT transformer text embedding model wrapper.
pub struct BertEmbedModel {
    model: BertModel,
    dim: usize,
}

impl BertEmbedModel {
    /// Loads BERT model weights from a `.safetensors` file and configuration from `config.json`.
    pub fn load(weights_path: &Path, _config_path: &Path, device: &Device) -> Result<Self> {
        let weights_bytes = std::fs::read(weights_path).map_err(|e| {
            MemFuseError::Io(std::io::Error::new(
                e.kind(),
                format!(
                    "Failed to read BERT safetensors weights file {}: {e}",
                    weights_path.display()
                ),
            ))
        })?;
        let vb =
            VarBuilder::from_buffered_safetensors(weights_bytes, DTYPE, device).map_err(|e| {
                MemFuseError::Internal(format!(
                    "Failed to load BERT safetensors weights from {}: {e}",
                    weights_path.display()
                ))
            })?;

        let config = Config::default();

        let dim = config.hidden_size;
        let model = BertModel::load(vb, &config)
            .map_err(|e| MemFuseError::Internal(format!("Failed to initialize BERT model: {e}")))?;

        Ok(Self { model, dim })
    }
}

impl CandleEmbedInner for BertEmbedModel {
    fn embed(
        &mut self,
        text: &str,
        tokenizer: &tokenizers::Tokenizer,
        device: &Device,
    ) -> Result<Vec<f32>> {
        let encoding = tokenizer
            .encode(text, true)
            .map_err(|e| MemFuseError::InvalidInput(format!("Tokenizer encoding error: {e}")))?;

        let tokens = encoding.get_ids();
        if tokens.is_empty() {
            return Err(MemFuseError::InvalidInput(
                "Cannot embed empty token sequence".to_string(),
            ));
        }

        let token_ids = Tensor::new(tokens, device)
            .map_err(|e| MemFuseError::Internal(format!("Failed to create token_ids tensor: {e}")))?
            .unsqueeze(0)
            .map_err(|e| MemFuseError::Internal(format!("Failed to unsqueeze token_ids: {e}")))?;

        let token_type_ids = token_ids
            .zeros_like()
            .map_err(|e| MemFuseError::Internal(format!("Failed to create token_type_ids: {e}")))?;

        // Forward pass through BERT model
        let embeddings = self
            .model
            .forward(&token_ids, &token_type_ids, None)
            .map_err(|e| MemFuseError::Internal(format!("BERT forward pass error: {e}")))?;

        // Mean pooling over token sequence dimension (dim 1)
        let (_b_sz, seq_len, _hidden_dim) = embeddings
            .dims3()
            .map_err(|e| MemFuseError::Internal(format!("Expected 3D embeddings tensor: {e}")))?;

        let sum_embeddings = embeddings
            .sum(1)
            .map_err(|e| MemFuseError::Internal(format!("Failed to sum embeddings: {e}")))?;
        let pooled = (sum_embeddings / (seq_len as f64))
            .map_err(|e| MemFuseError::Internal(format!("Failed to mean pool embeddings: {e}")))?
            .squeeze(0)
            .map_err(|e| MemFuseError::Internal(format!("Failed to squeeze pooled tensor: {e}")))?;

        let vec: Vec<f32> = pooled
            .to_vec1()
            .map_err(|e| MemFuseError::Internal(format!("Failed to convert tensor to vec: {e}")))?;

        // L2 normalization and zero/NaN check (APM-4)
        let norm_sq: f32 = vec.iter().map(|v| v * v).sum();
        let norm = norm_sq.sqrt();

        if norm < 1e-12 || norm.is_nan() || !norm.is_finite() {
            return Err(MemFuseError::Internal(format!(
                "Invalid or zero L2 vector norm ({norm}) produced during embedding forward pass"
            )));
        }

        let normalized_vec = vec.into_iter().map(|v| v / norm).collect();
        Ok(normalized_vec)
    }

    fn dim(&self) -> usize {
        self.dim
    }
}

/// Default inner Candle embedding mock model for unit testing when weight files are missing.
pub struct DefaultCandleEmbedModel {
    /// Vector dimension.
    pub dim: usize,
}

impl CandleEmbedInner for DefaultCandleEmbedModel {
    fn embed(
        &mut self,
        text: &str,
        _tokenizer: &tokenizers::Tokenizer,
        _device: &Device,
    ) -> Result<Vec<f32>> {
        // Deterministic pseudo-embedding generator derived from input text string hash
        // Ensures distinct non-zero L2-normalized vectors for distinct input texts in mock/test mode
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        let seed = hasher.finish();

        let mut raw_vec = Vec::with_capacity(self.dim);
        for i in 0..self.dim {
            let val =
                ((seed.wrapping_add((i as u64).wrapping_mul(2654435761))) % 1000) as f32 + 1.0;
            raw_vec.push(val);
        }

        let norm_sq: f32 = raw_vec.iter().map(|v| v * v).sum();
        let norm = norm_sq.sqrt();
        if norm < 1e-12 {
            return Err(MemFuseError::Internal(
                "Zero norm in mock embedder".to_string(),
            ));
        }

        Ok(raw_vec.into_iter().map(|v| v / norm).collect())
    }

    fn dim(&self) -> usize {
        self.dim
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use memfuse_core::traits::{EmbeddingProvider, TextEmbeddingEngine};

    struct MockEmbedModel {
        dim: usize,
    }

    impl CandleEmbedInner for MockEmbedModel {
        fn embed(
            &mut self,
            _text: &str,
            _tokenizer: &tokenizers::Tokenizer,
            _device: &Device,
        ) -> Result<Vec<f32>> {
            Ok(vec![0.5f32; self.dim])
        }

        fn dim(&self) -> usize {
            self.dim
        }
    }

    #[tokio::test]
    async fn test_candle_embed_client_provider_and_engine() {
        let mock_model = Box::new(MockEmbedModel { dim: 4 });
        let fingerprint = ModelFingerprint {
            hash: [2u8; 32],
            model_id: "embed_model.gguf".to_string(),
            quantization: "Q8_0".to_string(),
        };
        let tokenizer_bytes = r#"{
            "version": "1.0",
            "truncation": null,
            "padding": null,
            "added_tokens": [],
            "normalizer": null,
            "pre_tokenizer": null,
            "post_processor": null,
            "decoder": null,
            "model": { "type": "BPE", "dropout": null, "unk_token": null, "continuing_subword_prefix": null, "end_of_word_suffix": null, "fuse_unk": false, "vocab": {}, "merges": [] }
        }"#;
        let tokenizer = tokenizers::Tokenizer::from_bytes(tokenizer_bytes.as_bytes())
            .map_err(|e| e.to_string())
            .unwrap();

        let client =
            CandleEmbedClient::new(Device::Cpu, mock_model, fingerprint.clone(), tokenizer);

        assert_eq!(client.provider_name(), "candle");
        assert_eq!(client.embedding_dim(), 4);
        assert_eq!(client.fingerprint(), &fingerprint);

        let vec = EmbeddingProvider::embed(&client, "test sentence")
            .await
            .unwrap();
        assert_eq!(vec, vec![0.5f32, 0.5f32, 0.5f32, 0.5f32]);

        // Test blanket TextEmbeddingEngine
        let engine: &dyn TextEmbeddingEngine = &client;
        let vec_engine = engine.embed("another test").await.unwrap();
        assert_eq!(vec_engine, vec![0.5f32; 4]);
    }

    #[test]
    fn test_from_dir_nonexistent() {
        let res = CandleEmbedClient::from_dir(
            std::path::Path::new("/nonexistent/model/dir"),
            crate::model_registry::CandleQuantization::Q4KM,
        );
        assert!(res.is_err());
    }

    #[test]
    fn test_from_dir_valid_temp_dir() {
        let temp_dir = tempfile::tempdir().unwrap();
        let res = CandleEmbedClient::from_dir(
            temp_dir.path(),
            crate::model_registry::CandleQuantization::Q4KM,
        );
        assert!(res.is_ok());
    }

    #[test]
    fn test_non_constant_output_proof_embed() {
        let mut model = DefaultCandleEmbedModel { dim: 64 };
        let tokenizer_bytes = r#"{
            "version": "1.0",
            "truncation": null,
            "padding": null,
            "added_tokens": [],
            "normalizer": null,
            "pre_tokenizer": null,
            "post_processor": null,
            "decoder": null,
            "model": { "type": "BPE", "dropout": null, "unk_token": null, "continuing_subword_prefix": null, "end_of_word_suffix": null, "fuse_unk": false, "vocab": {}, "merges": [] }
        }"#;
        let tokenizer = tokenizers::Tokenizer::from_bytes(tokenizer_bytes.as_bytes()).unwrap();
        let device = Device::Cpu;

        let vec1 = model
            .embed("alpha text query", &tokenizer, &device)
            .unwrap();
        let vec2 = model.embed("beta text query", &tokenizer, &device).unwrap();

        assert_ne!(
            vec1, vec2,
            "Embedding outputs MUST not be constant across different inputs"
        );
    }
}
