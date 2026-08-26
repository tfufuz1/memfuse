//! memfuse-embed — In-process text embeddings using ONNX Runtime.
//!
//! This crate provides a high-level API for generating vector embeddings from text
//! without requiring external API calls. It uses the `ort` crate for ONNX Runtime
//! and `tokenizers` for text preprocessing.
//!
//! All ONNX-related functionality is gated behind the `onnx` feature flag.

#![deny(unsafe_code)]

#[cfg(feature = "onnx")]
use std::sync::Arc;

#[cfg(feature = "onnx")]
use async_trait::async_trait;
#[cfg(feature = "onnx")]
use memfuse_core::{MemFuseError, Result, TextEmbeddingEngine};
#[cfg(feature = "onnx")]
use ort::value::Value;
#[cfg(feature = "onnx")]
use std::path::Path;
#[cfg(feature = "onnx")]
use tokenizers::Tokenizer;
#[cfg(feature = "onnx")]
use tracing::{debug, info, warn};

#[cfg(feature = "onnx")]
struct SessionPool {
    sessions: std::sync::Mutex<Vec<ort::session::Session>>,
}

#[cfg(feature = "onnx")]
impl SessionPool {
    fn new(sessions: Vec<ort::session::Session>) -> Self {
        Self {
            sessions: std::sync::Mutex::new(sessions),
        }
    }

    fn pop(&self) -> Result<ort::session::Session> {
        let mut pool = self.sessions.lock().map_err(|_| {
            MemFuseError::Internal("SessionPool-Mutex vergiftet (Panic in Worker-Thread?)".into())
        })?;

        pool.pop().ok_or_else(|| {
            MemFuseError::Internal(
                "SessionPool erschöpft — Semaphore-Leck im Embedder-Code?".into(),
            )
        })
    }

    fn push(&self, session: ort::session::Session) {
        if let Ok(mut guard) = self.sessions.lock() {
            guard.push(session);
        } else {
            tracing::error!("SessionPool lock poisoned during push");
        }
    }
}

#[cfg(feature = "onnx")]
impl std::fmt::Debug for SessionPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionPool").finish_non_exhaustive()
    }
}

#[cfg(feature = "onnx")]
struct SessionGuard {
    pool: Arc<SessionPool>,
    session: Option<ort::session::Session>,
}

#[cfg(feature = "onnx")]
impl SessionGuard {
    fn new(pool: Arc<SessionPool>) -> Result<Self> {
        let session = pool.pop()?;
        Ok(Self {
            pool,
            session: Some(session),
        })
    }
}

#[cfg(feature = "onnx")]
impl Drop for SessionGuard {
    fn drop(&mut self) {
        if let Some(session) = self.session.take() {
            self.pool.push(session);
        }
    }
}

#[cfg(feature = "onnx")]
impl std::ops::Deref for SessionGuard {
    type Target = ort::session::Session;
    fn deref(&self) -> &Self::Target {
        self.session.as_ref().unwrap()
    }
}

#[cfg(feature = "onnx")]
impl std::ops::DerefMut for SessionGuard {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.session.as_mut().unwrap()
    }
}

/// Handles text tokenization and ONNX model inference.
///
/// Uses `tokio::task::spawn_blocking` to offload ONNX inference to a blocking
/// thread pool, preventing Tokio runtime starvation. A [`tokio::sync::Semaphore`]
/// limits the number of concurrent inference operations.
///
/// # Architecture
///
/// Instead of holding a `std::sync::Mutex<Session>` (which blocks the Tokio
/// thread on every `embed` call), this design:
///
/// 1. Stores a pre-loaded `SessionPool` populated at initialization.
/// 2. Uses a `Semaphore` to limit concurrent inferences.
/// 3. Safely lends sessions out of the pool for the duration of inference inside
///    `spawn_blocking` via a RAII guard, returning them even on panics.
#[cfg(feature = "onnx")]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TextEmbedderConfig {
    /// Maximum number of tokens per text sequence (default: 512).
    pub max_sequence_length: usize,
    /// Number of pre-allocated ONNX sessions in the pool (default: 2).
    pub pool_size: usize,
    /// Expected output embedding dimension (optional).
    pub expected_dim: Option<usize>,
}

#[cfg(feature = "onnx")]
impl Default for TextEmbedderConfig {
    fn default() -> Self {
        Self {
            max_sequence_length: 512,
            pool_size: 2,
            expected_dim: None,
        }
    }
}

#[cfg(feature = "onnx")]
pub type OnnxEmbedder = TextEmbedder;

#[cfg(feature = "onnx")]
#[derive(Debug, Clone)]
pub struct TextEmbedder {
    /// Shared tokenizer instance (thread-safe via `Arc`).
    tokenizer: Arc<Tokenizer>,
    /// Semaphore limiting parallel ONNX inference threads.
    semaphore: Arc<tokio::sync::Semaphore>,
    /// Pre-loaded Session Pool.
    pool: Arc<SessionPool>,
    /// Configuration settings for the embedder.
    config: TextEmbedderConfig,
    /// Expected output embedding dimension.
    expected_dim: Option<usize>,
}

#[cfg(feature = "onnx")]
#[async_trait]
impl TextEmbeddingEngine for TextEmbedder {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        self.embed_async(text).await
    }

    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let mut handles = Vec::with_capacity(texts.len());
        for text in texts {
            let text_owned = text.to_string();
            let embedder = self.clone();
            handles.push(tokio::spawn(async move {
                embedder.embed_async(&text_owned).await
            }));
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
            seq_results.push(self.embed_async(text).await?);
        }
        Ok(seq_results)
    }
}

#[cfg(feature = "onnx")]
impl TextEmbedder {
    /// Creates a new embedder from a model directory. Alias for `load`.
    pub fn new(model_dir: impl AsRef<Path>) -> Result<Self> {
        Self::load(model_dir)
    }

    /// Creates a new embedder by loading a tokenizer and ONNX models from the specified directory.
    ///
    /// The directory should contain `model.onnx` and `tokenizer.json`.
    /// ONNX sessions are loaded upfront into a pool to prevent per-inference overhead.
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

        let pool_size = config.pool_size;
        info!(
            "Model path registered: {:?} (pre-loading {} sessions)",
            model_path, pool_size
        );

        let mut sessions = Vec::with_capacity(pool_size);
        for _ in 0..pool_size {
            let session = ort::session::Session::builder()
                .map_err(|e| MemFuseError::Internal(format!("Session builder: {}", e)))?
                .commit_from_file(&model_path)
                .map_err(|e| MemFuseError::Internal(format!("Model load: {}", e)))?;
            sessions.push(session);
        }

        let expected_dim = config.expected_dim;
        Ok(Self {
            tokenizer: Arc::new(tokenizer),
            semaphore: Arc::new(tokio::sync::Semaphore::new(pool_size)),
            pool: Arc::new(SessionPool::new(sessions)),
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
        let _permit =
            tokio::time::timeout(std::time::Duration::from_secs(30), self.semaphore.acquire())
                .await
                .map_err(|_| {
                    MemFuseError::Internal("ONNX session pool exhausted (timeout 30s)".into())
                })?
                .map_err(|e| MemFuseError::Internal(format!("Semaphore closed: {e}")))?;

        let text = text.to_string();
        let pool = self.pool.clone();
        let tokenizer = self.tokenizer.clone();
        let max_sequence_length = self.config.max_sequence_length;

        let output = tokio::task::spawn_blocking(move || {
            // Guard borrows session from pool, restores it on drop
            let mut session_guard = SessionGuard::new(pool)?;
            Self::run_inference(&mut session_guard, &tokenizer, &text, max_sequence_length)
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
        let err = res.err().unwrap();
        assert!(matches!(err, MemFuseError::InvalidInput(_)));

        // Create tokenizer but still missing model.onnx
        File::create(dir.path().join("tokenizer.json"))?;
        let res = TextEmbedder::load(dir.path());
        assert!(res.is_err());
        let err = res.err().unwrap();
        assert!(matches!(err, MemFuseError::InvalidInput(_)));
        Ok(())
    }

    #[cfg(feature = "onnx")]
    #[test]
    fn test_text_embedder_config_default() {
        let cfg = TextEmbedderConfig::default();
        assert_eq!(cfg.max_sequence_length, 512);
        assert_eq!(cfg.pool_size, 2);
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
        // Error could be from tokenizer or ONNX
        let err_msg = res.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(
            err_msg.contains("Failed to load tokenizer")
                || err_msg.contains("Failed to load model")
        );
        Ok(())
    }

    // ANCHOR[TEST:EMB-001]
    #[cfg(feature = "onnx")]
    #[test]
    fn test_session_pool_exhaustion() {
        let pool = SessionPool::new(vec![]);
        let res = pool.pop();
        assert!(res.is_err());
        let err_msg = res.err().unwrap().to_string();
        assert!(err_msg.contains("SessionPool erschöpft"));
    }

    // ANCHOR[TEST:EMB-001]
    #[cfg(feature = "onnx")]
    #[test]
    fn test_session_pool_poisoned() {
        let pool = Arc::new(SessionPool::new(vec![]));
        let pool_clone = pool.clone();

        let _ = std::thread::spawn(move || {
            let _guard = pool_clone.sessions.lock().unwrap();
            panic!("Poisoning mutex for testing");
        })
        .join();

        let res = pool.pop();
        assert!(res.is_err());
        let err_msg = res.err().unwrap().to_string();
        assert!(err_msg.contains("SessionPool-Mutex vergiftet"));
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
        use async_trait::async_trait;
        use memfuse_core::{Result, TextEmbeddingEngine};

        struct MockEngine;
        #[async_trait]
        impl TextEmbeddingEngine for MockEngine {
            async fn embed(&self, text: &str) -> Result<Vec<f32>> {
                Ok(vec![text.len() as f32])
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
        use async_trait::async_trait;
        use memfuse_core::{MemFuseError, Result, TextEmbeddingEngine};

        struct MockOrderedEngine {
            fail_on: Option<String>,
        }

        #[async_trait]
        impl TextEmbeddingEngine for MockOrderedEngine {
            async fn embed(&self, text: &str) -> Result<Vec<f32>> {
                if let Some(ref fail) = self.fail_on {
                    if text == fail {
                        return Err(MemFuseError::InvalidInput(format!("Failed on {text}")));
                    }
                }
                Ok(vec![
                    text.len() as f32,
                    (text.chars().next().unwrap_or('a') as u32) as f32,
                ])
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
        let err_msg = err_res.err().unwrap().to_string();
        assert!(err_msg.contains("Failed on b"));

        Ok(())
    }
}
