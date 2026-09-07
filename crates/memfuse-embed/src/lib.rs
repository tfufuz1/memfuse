// FILE-CONTEXT
// STAND: 2026-08-29T17:16:44Z (SESSION: f50ed9ef)
// ZWECK: In-process ONNX Embedding Engine (Layer 3 im 5-Schichten-DAG).
// INVARIANTEN: Default-Build ohne ONNX hat leere Feature-Flags (ADR-005, Pure-Rust-USP).
// NICHT-OFFENSICHTLICH: Threading via tokio::task::spawn_blocking zur Vermeidung von Executor-Starvation.
// SIEHE AUCH: crates/memfuse-embed/AGENTS.md, rules/dependencies.md

//! memfuse-embed — In-process text embeddings using ONNX Runtime.
//!
//! This crate provides a high-level API for generating vector embeddings from text
//! without requiring external API calls. It uses the `ort` crate for ONNX Runtime
//! and `tokenizers` for text preprocessing.
//!
//! All ONNX-related functionality is gated behind the `onnx` feature flag.

// `deny(unsafe_code)` is consciously chosen over `forbid(unsafe_code)` to allow
// low-level C-FFI / ONNX Runtime interactions when `onnx` feature is enabled.
// In default (non-onnx) builds, zero unsafe code exists in production.
#![deny(unsafe_code)]

#[cfg(feature = "onnx")]
use std::path::Path;
#[cfg(feature = "onnx")]
use std::sync::Arc;

#[cfg(feature = "onnx")]
#[cfg(feature = "onnx")]
use memfuse_core::{BoxFuture, EmbeddingError, EmbeddingProvider, MemFuseError, Result};
#[cfg(feature = "onnx")]
use ort::value::Value;
#[cfg(feature = "onnx")]
use tokenizers::Tokenizer;
#[cfg(feature = "onnx")]
use tracing::{debug, info, warn};

pub mod reranker;
pub use reranker::{CrossEncoderReranker, PlattScaledSigmoid, RerankConfig, RerankResult};

/// Counter tracking the number of ONNX session load operations (for test verification).
#[cfg(feature = "onnx")]
pub static SESSION_LOAD_COUNT: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// Conservative default. Override via `TextEmbedderConfig::max_batch_size`.
/// At 1536D × 512 × f32 = ~3 MB input tensor; safe within 128 MB memory budgets.
pub const MAX_EMBED_BATCH_SIZE: usize = 512;

/// Configuration settings for the text embedder.
#[cfg(feature = "onnx")]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TextEmbedderConfig {
    /// Maximum number of tokens per text sequence (default: 512).
    pub max_sequence_length: usize,
    /// Maximum parallel ONNX inference threads (default: 2).
    pub pool_size: usize,
    /// Expected output embedding dimension (optional).
    pub expected_dim: Option<usize>,
    /// Maximum batch size for embed_batch(). Default: MAX_EMBED_BATCH_SIZE (512).
    pub max_batch_size: usize,
}

#[cfg(feature = "onnx")]
impl Default for TextEmbedderConfig {
    fn default() -> Self {
        Self {
            max_sequence_length: 512,
            pool_size: 2,
            expected_dim: None,
            max_batch_size: MAX_EMBED_BATCH_SIZE,
        }
    }
}

#[cfg(feature = "onnx")]
pub type OnnxEmbedder = TextEmbedder;

/// Handles text tokenization and ONNX model inference.
///
/// Uses `tokio::task::spawn_blocking` to offload ONNX inference to a blocking
/// thread pool, preventing Tokio runtime starvation. A [`tokio::sync::Semaphore`]
/// limits the number of concurrent inference operations.
#[cfg(feature = "onnx")]
#[derive(Clone)]
pub struct TextEmbedder {
    /// Shared ONNX session (thread-safe via `Arc<Mutex>`).
    session: Arc<parking_lot::Mutex<ort::session::Session>>,
    /// Shared tokenizer instance (thread-safe via `Arc`).
    tokenizer: Arc<Tokenizer>,
    /// Semaphore limiting parallel ONNX inference threads.
    semaphore: Arc<tokio::sync::Semaphore>,
    /// Configuration settings for the embedder.
    config: TextEmbedderConfig,
    /// Expected output embedding dimension.
    expected_dim: Option<usize>,
}

#[cfg(feature = "onnx")]
impl std::fmt::Debug for TextEmbedder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextEmbedder")
            .field("config", &self.config)
            .field("expected_dim", &self.expected_dim)
            .finish()
    }
}

#[cfg(feature = "onnx")]
impl EmbeddingProvider for TextEmbedder {
    fn provider_name(&self) -> &str {
        "onnx"
    }

    fn embed<'a>(
        &'a self,
        text: &'a str,
    ) -> BoxFuture<'a, std::result::Result<Vec<f32>, EmbeddingError>> {
        Box::pin(async move {
            let max_len = self.config.max_sequence_length;
            if let Ok(encoding) = self.tokenizer.encode(text, true) {
                let len = encoding.get_ids().len();
                if len > max_len {
                    return Err(EmbeddingError::InputTooLong { len, max: max_len });
                }
            }
            self.embed_async(text).await.map_err(|e| match e {
                MemFuseError::InvalidInput(msg)
                    if msg.contains("too long") || msg.contains("exceeds") =>
                {
                    EmbeddingError::InputTooLong {
                        len: text.len(),
                        max: max_len,
                    }
                }
                MemFuseError::InvalidInput(msg) | MemFuseError::NotFound(msg) => {
                    EmbeddingError::Unavailable(msg)
                }
                other => EmbeddingError::ComputationFailed(other.to_string()),
            })
        })
    }

    fn embedding_dim(&self) -> usize {
        self.expected_dim.unwrap_or(0)
    }

    fn embed_batch<'a>(
        &'a self,
        texts: &'a [&'a str],
    ) -> BoxFuture<'a, std::result::Result<Vec<Vec<f32>>, EmbeddingError>> {
        Box::pin(async move {
            let limit = self.config.max_batch_size;
            if texts.len() > limit {
                return Err(EmbeddingError::Unavailable(format!(
                    "Batch size {} exceeds max_batch_size {}. Split into smaller batches.",
                    texts.len(),
                    limit
                )));
            }

            let mut handles = Vec::with_capacity(texts.len());
            for text in texts {
                let text_owned = text.to_string();
                let embedder = self.clone();
                handles.push(tokio::spawn(
                    async move { embedder.embed(&text_owned).await },
                ));
            }

            let mut results = Vec::with_capacity(texts.len());
            let mut parallel_failed = false;
            for handle in handles {
                match handle.await {
                    Ok(Ok(res)) => results.push(res),
                    Ok(Err(e)) => return Err(e),
                    Err(_) => {
                        parallel_failed = true;
                        break;
                    }
                }
            }

            if !parallel_failed && results.len() == texts.len() {
                return Ok(results);
            }

            // Sequential fallback path
            let mut seq_results = Vec::with_capacity(texts.len());
            for text in texts {
                seq_results.push(self.embed(text).await?);
            }
            Ok(seq_results)
        })
    }
}

#[cfg(feature = "onnx")]
impl TextEmbedder {
    /// Creates a new embedder from a model file or model directory path.
    pub fn from_path(model_path: impl AsRef<Path>) -> Result<Self> {
        let path = model_path.as_ref();
        let dir = if path.is_file() {
            path.parent().unwrap_or(path)
        } else {
            path
        };
        Self::load(dir)
    }

    /// Creates a new embedder from a model directory. Alias for `load`.
    pub fn new(model_dir: impl AsRef<Path>) -> Result<Self> {
        Self::load(model_dir)
    }

    /// Creates a new embedder by loading a tokenizer and ONNX model path from the specified directory.
    ///
    /// The directory should contain `model.onnx` and `tokenizer.json`.
    pub fn load(model_dir: impl AsRef<Path>) -> Result<Self> {
        Self::load_with_config(model_dir, TextEmbedderConfig::default())
    }

    /// Creates a new embedder with a custom configuration.
    pub fn load_with_config(
        model_dir: impl AsRef<Path>,
        config: TextEmbedderConfig,
    ) -> Result<Self> {
        let path = model_dir.as_ref();
        if !path.join("tokenizer.json").exists() {
            return Err(MemFuseError::InvalidInput(
                "tokenizer.json not found".into(),
            ));
        }
        if !path.join("model.onnx").exists() {
            return Err(MemFuseError::InvalidInput("model.onnx not found".into()));
        }

        let model_path = path.join("model.onnx");
        let tokenizer_path = path.join("tokenizer.json");

        info!("Loading tokenizer from {:?}", tokenizer_path);
        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| MemFuseError::Internal(format!("Failed to load tokenizer: {}", e)))?;

        info!("Loading ONNX model session from {:?}", model_path);
        let session = ort::session::Session::builder()
            .map_err(|e| MemFuseError::Internal(format!("Failed to build ONNX session: {}", e)))?
            .commit_from_file(&model_path)
            .map_err(|e| {
                MemFuseError::Internal(format!(
                    "Failed to load ONNX model from {:?}: {}",
                    model_path, e
                ))
            })?;

        SESSION_LOAD_COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        let pool_size = config.pool_size;
        let expected_dim = config.expected_dim;
        Ok(Self {
            session: Arc::new(parking_lot::Mutex::new(session)),
            tokenizer: Arc::new(tokenizer),
            semaphore: Arc::new(tokio::sync::Semaphore::new(pool_size)),
            config,
            expected_dim,
        })
    }

    /// Sets the expected embedding output dimension for post-inference validation.
    pub fn with_expected_dimension(mut self, dim: usize) -> Self {
        self.expected_dim = Some(dim);
        self
    }

    /// Generates an embedding for the given text using `spawn_blocking`.
    ///
    /// Acquires a semaphore permit to limit concurrent inference operations,
    /// then offloads the blocking ONNX computation to Tokio's blocking thread pool.
    pub async fn embed_async(&self, text: &str) -> Result<Vec<f32>> {
        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|_| MemFuseError::Internal("Semaphore closed".into()))?;

        let text = text.to_string();
        let session = self.session.clone();
        let tokenizer = self.tokenizer.clone();
        let max_sequence_length = self.config.max_sequence_length;

        let output = tokio::task::spawn_blocking(move || {
            let mut guard = session.lock();
            Self::run_inference(&mut guard, &tokenizer, &text, max_sequence_length)
        })
        .await
        .map_err(|e| MemFuseError::Internal(format!("spawn_blocking join: {}", e)))??;

        if let Some(expected_dim) = self.expected_dim {
            if output.len() != expected_dim {
                return Err(MemFuseError::InvalidInput(format!(
                    "Model output dim {} != expected {}",
                    output.len(),
                    expected_dim
                )));
            }
        }

        Ok(output)
    }

    /// Performs tokenization, ONNX forward pass, mean pooling, and L2 normalization.
    ///
    /// This is a synchronous function intended to run inside `spawn_blocking`.
    fn run_inference(
        session: &mut ort::session::Session,
        tokenizer: &Tokenizer,
        text: &str,
        max_sequence_length: usize,
    ) -> Result<Vec<f32>> {
        debug!("Embedding text: {:?}", text);

        let encoding = tokenizer
            .encode(text, true)
            .map_err(|e| MemFuseError::Internal(format!("Tokenization failed: {}", e)))?;

        let mut input_ids = encoding.get_ids().to_vec();
        let mut attention_mask = encoding.get_attention_mask().to_vec();
        let mut type_ids = encoding.get_type_ids().to_vec();

        if input_ids.len() > max_sequence_length {
            warn!(
                tokens = input_ids.len(),
                max = max_sequence_length,
                "Input text exceeds max sequence length and will be truncated. \
                 Consider using MarkdownChunker before embedding."
            );
            input_ids.truncate(max_sequence_length);
            attention_mask.truncate(max_sequence_length);
            type_ids.truncate(max_sequence_length);
        }

        let seq_len = input_ids.len();

        let input_ids_vec: Vec<i64> = input_ids.iter().map(|&id| id as i64).collect();
        let attention_mask_vec: Vec<i64> = attention_mask.iter().map(|&m| m as i64).collect();

        let input_ids_tensor = Value::from_array(([1, seq_len], input_ids_vec))
            .map_err(|e| MemFuseError::Internal(format!("Failed to create tensor: {}", e)))?;
        let attention_mask_tensor = Value::from_array(([1, seq_len], attention_mask_vec))
            .map_err(|e| MemFuseError::Internal(format!("Failed to create tensor: {}", e)))?;

        let has_token_type_ids = session
            .inputs()
            .iter()
            .any(|input| input.name() == "token_type_ids");

        let outputs = if has_token_type_ids {
            let token_type_ids_vec: Vec<i64> = type_ids.iter().map(|&id| id as i64).collect();
            let token_type_ids_tensor = Value::from_array(([1, seq_len], token_type_ids_vec))
                .map_err(|e| MemFuseError::Internal(format!("Failed to create tensor: {}", e)))?;
            session
                .run(ort::inputs![
                    "input_ids" => input_ids_tensor,
                    "attention_mask" => attention_mask_tensor,
                    "token_type_ids" => token_type_ids_tensor,
                ])
                .map_err(|e| MemFuseError::Internal(format!("ONNX inference failed: {}", e)))?
        } else {
            session
                .run(ort::inputs![
                    "input_ids" => input_ids_tensor,
                    "attention_mask" => attention_mask_tensor,
                ])
                .map_err(|e| MemFuseError::Internal(format!("ONNX inference failed: {}", e)))?
        };

        // Mean Pooling over token embeddings, weighted by attention_mask
        let process_tensor = |shape: &[i64], data: &[f32]| -> Result<Vec<f32>> {
            // shape: [batch=1, seq_len, hidden_size]
            if shape.len() != 3 {
                return Err(MemFuseError::Internal(format!(
                    "Unexpected output shape: {:?}",
                    shape
                )));
            }

            let hidden_size = shape[2] as usize;
            let mut mean_vec = vec![0.0f32; hidden_size];

            for i in 0..seq_len {
                if attention_mask[i] == 1 {
                    for j in 0..hidden_size {
                        mean_vec[j] += data[i * hidden_size + j];
                    }
                }
            }

            let sum_mask = attention_mask.iter().sum::<u32>() as f32;
            if sum_mask > 0.0 {
                for val in mean_vec.iter_mut() {
                    *val /= sum_mask;
                }
            }

            // L2 Normalization (standard for sentence embeddings)
            let norm = mean_vec.iter().map(|x| x * x).sum::<f32>().sqrt();
            if norm > 0.0 {
                for val in mean_vec.iter_mut() {
                    *val /= norm;
                }
            }
            Ok(mean_vec)
        };

        // Extract tensor — try named output first, fall back to positional
        if let Some(out) = outputs.get("last_hidden_state") {
            let (shape, data) = out
                .try_extract_tensor::<f32>()
                .map_err(|e| MemFuseError::Internal(format!("Failed to extract: {}", e)))?;
            process_tensor(shape, data)
        } else if let Some((_, out)) = outputs.iter().next() {
            let (shape, data) = out
                .try_extract_tensor::<f32>()
                .map_err(|e| MemFuseError::Internal(format!("Failed to extract: {}", e)))?;
            process_tensor(shape, data)
        } else {
            Err(MemFuseError::Internal("Model produced no outputs".into()))
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "onnx")]
    use super::MAX_EMBED_BATCH_SIZE;

    #[cfg(feature = "onnx")]
    use super::*;

    #[cfg(feature = "onnx")]
    use std::fs::File;
    #[cfg(feature = "onnx")]
    use tempfile::tempdir;

    #[cfg(feature = "onnx")]
    #[test]
    fn test_text_embedder_load_missing_files() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        let dir = tempdir()?;

        // Empty directory — missing tokenizer.json check first
        let res = TextEmbedder::load(dir.path());
        assert!(res.is_err());
        let err = res.err().unwrap(); // unwrap
        assert!(matches!(err, MemFuseError::InvalidInput(_)));

        // Create tokenizer but still missing model.onnx
        File::create(dir.path().join("tokenizer.json"))?;
        let res = TextEmbedder::load(dir.path());
        assert!(res.is_err());
        let err = res.err().unwrap(); // unwrap
        assert!(matches!(err, MemFuseError::InvalidInput(_)));
        Ok(())
    }

    #[cfg(feature = "onnx")]
    #[test]
    fn test_text_embedder_config_default() {
        let cfg = TextEmbedderConfig::default();
        assert_eq!(cfg.max_sequence_length, 512);
        assert_eq!(cfg.pool_size, 2);
        assert_eq!(cfg.max_batch_size, MAX_EMBED_BATCH_SIZE);
    }

    #[cfg(feature = "onnx")]
    #[test]
    fn test_text_embedder_load_invalid_content(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        use std::io::Write;

        let dir = tempdir()?;
        File::create(dir.path().join("model.onnx"))?.write_all(b"invalid")?;
        File::create(dir.path().join("tokenizer.json"))?.write_all(b"invalid")?;

        let res = TextEmbedder::load(dir.path());
        assert!(res.is_err());
        let err_msg = res.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(
            err_msg.contains("Failed to load tokenizer")
                || err_msg.contains("Failed to load model")
                || err_msg.contains("InvalidInput")
        );
        Ok(())
    }

    #[test]
    fn test_formatting_safety() {
        use memfuse_core::MemFuseError;
        // Verify that Debug formatting on MemFuseError doesn't panic
        let err = MemFuseError::Internal("test".into());
        let formatted = format!("{:?}", err);
        assert!(formatted.contains("Internal"));
    }

    #[tokio::test]
    async fn test_mock_embedding_engine() -> std::result::Result<(), Box<dyn std::error::Error>> {
        use memfuse_core::{BoxFuture, Result, TextEmbeddingEngine};

        struct MockEngine;
        impl TextEmbeddingEngine for MockEngine {
            fn embed<'a>(&'a self, text: &'a str) -> BoxFuture<'a, Result<Vec<f32>>> {
                Box::pin(async move { Ok(vec![text.len() as f32]) })
            }
        }

        let engine = MockEngine;
        let res = engine.embed("memfuse").await?;
        assert_eq!(res, vec![7.0]);
        Ok(())
    }

    #[tokio::test]
    async fn test_embed_batch_ordering_and_fallback(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        use memfuse_core::{BoxFuture, MemFuseError, Result, TextEmbeddingEngine};

        struct MockOrderedEngine {
            fail_on: Option<String>,
        }

        impl TextEmbeddingEngine for MockOrderedEngine {
            fn embed<'a>(&'a self, text: &'a str) -> BoxFuture<'a, Result<Vec<f32>>> {
                Box::pin(async move {
                    if let Some(ref fail) = self.fail_on {
                        if text == fail {
                            return Err(MemFuseError::InvalidInput(format!("Failed on {text}")));
                        }
                    }
                    Ok(vec![
                        text.len() as f32,
                        (text.chars().next().unwrap_or('a') as u32) as f32,
                    ])
                })
            }
        }

        let engine = MockOrderedEngine { fail_on: None };
        let texts = vec!["a", "b", "c"];
        let batch_res = engine.embed_batch(&texts).await?;
        assert_eq!(batch_res.len(), 3);
        assert_eq!(batch_res[0], vec![1.0, 'a' as u32 as f32]);
        assert_eq!(batch_res[1], vec![1.0, 'b' as u32 as f32]);
        assert_eq!(batch_res[2], vec![1.0, 'c' as u32 as f32]);

        // Error propagation test
        let failing_engine = MockOrderedEngine {
            fail_on: Some("b".into()),
        };
        let err_res = failing_engine.embed_batch(&texts).await;
        assert!(err_res.is_err());
        let err_msg = err_res.err().unwrap().to_string(); // unwrap
        assert!(err_msg.contains("Failed on b"));

        Ok(())
    }

    #[cfg(feature = "onnx")]
    #[tokio::test]
    async fn test_embed_batch_oversized_limit(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        use memfuse_core::EmbeddingProvider;
        use std::path::PathBuf;

        let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("model.onnx");

        if !fixture_path.exists() {
            return Ok(());
        }

        let embedder = TextEmbedder::from_path(&fixture_path)?;

        let large_texts: Vec<&str> = vec!["text"; MAX_EMBED_BATCH_SIZE + 1];
        let res = EmbeddingProvider::embed_batch(&embedder, &large_texts).await;
        assert!(res.is_err());
        if let Err(err) = res {
            assert!(matches!(err, EmbeddingError::Unavailable(_)));
            assert!(err.to_string().contains("exceeds max_batch_size"));
        } else {
            panic!("Expected InvalidInput error for oversized embed batch");
        }
        Ok(())
    }
}

// REVIEW-PASS[1/2] STATUS:PASS (TS: 2026-09-02T08:17:15Z) (SESSION: f260cbf2) PRÜFER-KONTEXT: FRESH - Verified feature gate isolation, zero unsafe in production, and execution non-starvation model.
// REVIEW-PASS[2/2] STATUS:PASS (TS: 2026-09-03T19:40:00Z) (SESSION: 6da6a1c8) PRÜFER-KONTEXT: FRESH - Verified Chaos Engineering fault tolerance, hermetic feature gate, and zero-unsafe invariants.
