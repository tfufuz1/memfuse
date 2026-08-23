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
use std::path::{Path, PathBuf};
#[cfg(feature = "onnx")]
use tokenizers::Tokenizer;
#[cfg(feature = "onnx")]
use tracing::{debug, info};

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
/// 1. Stores only the model **path** and a shared **tokenizer**.
/// 2. Creates a fresh ONNX `Session` inside each `spawn_blocking` task.
/// 3. Uses a `Semaphore` to cap concurrent inferences (default: 2).
#[cfg(feature = "onnx")]
#[derive(Debug)]
pub struct TextEmbedder {
    /// Path to the resolved ONNX model file.
    session_path: PathBuf,
    /// Shared tokenizer instance (thread-safe via `Arc`).
    tokenizer: Arc<Tokenizer>,
    /// Semaphore limiting parallel ONNX inference threads.
    semaphore: Arc<tokio::sync::Semaphore>,
}

#[cfg(feature = "onnx")]
#[async_trait]
impl TextEmbeddingEngine for TextEmbedder {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        self.embed_async(text).await
    }
}

#[cfg(feature = "onnx")]
impl TextEmbedder {
    /// Creates a new embedder by loading a tokenizer from the specified directory.
    ///
    /// The directory should contain `model.onnx` and `tokenizer.json`.
    /// The ONNX session is created per-inference call via `spawn_blocking`,
    /// so only the model path is stored here.
    pub fn load(model_dir: impl AsRef<Path>) -> Result<Self> {
        let model_dir = model_dir.as_ref();
        let model_path = if model_dir.join("model.onnx").exists() {
            model_dir.join("model.onnx")
        } else if model_dir.join("onnx/model.onnx").exists() {
            model_dir.join("onnx/model.onnx")
        } else {
            model_dir.to_path_buf() // Assume the path itself is the model
        };

        let tokenizer_path = model_dir.join("tokenizer.json");

        if !model_path.exists() {
            return Err(MemFuseError::Internal(format!(
                "Model file not found at {:?}",
                model_path
            )));
        }
        if !tokenizer_path.exists() {
            return Err(MemFuseError::Internal(format!(
                "Tokenizer file not found at {:?}",
                tokenizer_path
            )));
        }

        info!("Loading tokenizer from {:?}", tokenizer_path);
        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| MemFuseError::Internal(format!("Failed to load tokenizer: {}", e)))?;

        info!(
            "Model path registered: {:?} (session created per inference call)",
            model_path
        );

        Ok(Self {
            session_path: model_path,
            tokenizer: Arc::new(tokenizer),
            semaphore: Arc::new(tokio::sync::Semaphore::new(2)), // max 2 parallel inferences
        })
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
        let session_path = self.session_path.clone();
        let tokenizer = self.tokenizer.clone();

        tokio::task::spawn_blocking(move || {
            // ONNX-Session pro spawn_blocking-Aufruf (thread-local, kein Mutex nötig)
            let mut session = ort::session::Session::builder()
                .map_err(|e| MemFuseError::Internal(format!("Session builder: {}", e)))?
                .commit_from_file(&session_path)
                .map_err(|e| MemFuseError::Internal(format!("Model load: {}", e)))?;

            Self::run_inference(&mut session, &tokenizer, &text)
        })
        .await
        .map_err(|e| MemFuseError::Internal(format!("spawn_blocking join: {}", e)))?
    }

    /// Performs tokenization, ONNX forward pass, mean pooling, and L2 normalization.
    ///
    /// This is a synchronous function intended to run inside `spawn_blocking`.
    fn run_inference(
        session: &mut ort::session::Session,
        tokenizer: &Tokenizer,
        text: &str,
    ) -> Result<Vec<f32>> {
        debug!("Embedding text: {:?}", text);

        let encoding = tokenizer
            .encode(text, true)
            .map_err(|e| MemFuseError::Internal(format!("Tokenization failed: {}", e)))?;

        let input_ids = encoding.get_ids();
        let attention_mask = encoding.get_attention_mask();
        let seq_len = input_ids.len();

        let input_ids_vec: Vec<i64> = input_ids.iter().map(|&id| id as i64).collect();
        let attention_mask_vec: Vec<i64> = attention_mask.iter().map(|&m| m as i64).collect();

        let input_ids_tensor = Value::from_array(([1, seq_len], input_ids_vec))
            .map_err(|e| MemFuseError::Internal(format!("Failed to create tensor: {}", e)))?;
        let attention_mask_tensor = Value::from_array(([1, seq_len], attention_mask_vec))
            .map_err(|e| MemFuseError::Internal(format!("Failed to create tensor: {}", e)))?;

        let outputs = session
            .run(ort::inputs![
                "input_ids" => input_ids_tensor,
                "attention_mask" => attention_mask_tensor,
            ])
            .map_err(|e| MemFuseError::Internal(format!("ONNX inference failed: {}", e)))?;

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
    fn test_text_embedder_load_missing_files() {
        let dir = tempdir().expect("tempdir creation failed in test");

        // Empty directory — should fail at tokenizer existence check first
        // because model_path defaults to model_dir if model.onnx is missing.
        let res = TextEmbedder::load(dir.path());
        match res {
            Err(e) => assert!(e.to_string().contains("Tokenizer file not found")),
            Ok(_) => panic!("Should have failed"),
        }

        // Create tokenizer but still missing model.onnx
        File::create(dir.path().join("tokenizer.json")).expect("file creation failed in test");
        let res = TextEmbedder::load(dir.path());
        match res {
            Err(e) => {
                let msg = e.to_string();
                // Should fail at tokenizer loading because it's empty
                assert!(msg.contains("Failed to load tokenizer"));
            }
            Ok(_) => panic!("Should have failed"),
        }
    }

    #[cfg(feature = "onnx")]
    #[test]
    fn test_text_embedder_load_invalid_content() {
        use std::io::Write;

        let dir = tempdir().expect("tempdir creation failed in test");
        File::create(dir.path().join("model.onnx"))
            .expect("file creation failed in test")
            .write_all(b"invalid")
            .expect("write failed in test");
        File::create(dir.path().join("tokenizer.json"))
            .expect("file creation failed in test")
            .write_all(b"invalid")
            .expect("write failed in test");

        let res = TextEmbedder::load(dir.path());
        assert!(res.is_err());
        // Error could be from tokenizer or ONNX
        let err_msg = res.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(
            err_msg.contains("Failed to load tokenizer")
                || err_msg.contains("Failed to load model")
        );
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
    async fn test_mock_embedding_engine() {
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
        let res = engine.embed("memfuse").await.expect("mock embed failed");
        assert_eq!(res, vec![7.0]);
    }
}
