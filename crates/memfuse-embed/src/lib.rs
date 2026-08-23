//! memfuse-embed — In-process text embeddings using ONNX Runtime.
//!
//! This crate provides a high-level API for generating vector embeddings from text
//! without requiring external API calls. It uses the `ort` crate for ONNX Runtime
//! and `tokenizers` for text preprocessing.

#![deny(unsafe_code)]

use async_trait::async_trait;
use memfuse_core::{MemFuseError, Result, TextEmbeddingEngine};
use ort::session::Session;
use ort::value::Value;
use std::path::Path;
use tokenizers::Tokenizer;
use tracing::{debug, info};

/// Handles text tokenization and ONNX model inference.
#[derive(Debug)]
pub struct TextEmbedder {
    session: std::sync::Mutex<Session>,
    tokenizer: Tokenizer,
}

#[async_trait]
impl TextEmbeddingEngine for TextEmbedder {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        self.embed(text)
    }
}

impl TextEmbedder {
    /// Creates a new embedder by loading a model and tokenizer from the specified directory.
    ///
    /// The directory should contain `model.onnx` and `tokenizer.json`.
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
        let tokenizer = Tokenizer::from_file(tokenizer_path)
            .map_err(|e| MemFuseError::Internal(format!("Failed to load tokenizer: {}", e)))?;

        info!("Initializing ONNX session from {:?}", model_path);
        let session = Session::builder()
            .map_err(|e| {
                MemFuseError::Internal(format!("Failed to create session builder: {}", e))
            })?
            .commit_from_file(model_path)
            .map_err(|e| MemFuseError::Internal(format!("Failed to load model: {}", e)))?;

        Ok(Self {
            session: std::sync::Mutex::new(session),
            tokenizer,
        })
    }

    /// Generates an embedding for the given text.
    ///
    /// This performs tokenization, forward pass, and mean pooling.
    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        debug!("Embedding text: {:?}", text);

        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| MemFuseError::Internal(format!("Tokenization failed: {}", e)))?;

        let input_ids = encoding.get_ids();
        let attention_mask = encoding.get_attention_mask();
        let seq_len = input_ids.len();

        let input_ids_vec: Vec<i64> = input_ids.iter().map(|&id| id as i64).collect();
        let attention_mask_vec: Vec<i64> = attention_mask.iter().map(|&m| m as i64).collect();

        // Run inference
        let input_ids_tensor = Value::from_array(([1, seq_len], input_ids_vec))
            .map_err(|e| MemFuseError::Internal(format!("Failed to create tensor: {}", e)))?;
        let attention_mask_tensor = Value::from_array(([1, seq_len], attention_mask_vec))
            .map_err(|e| MemFuseError::Internal(format!("Failed to create tensor: {}", e)))?;

        let mut session_guard = self
            .session
            .lock()
            .map_err(|_| MemFuseError::Internal("Session lock poisoned".into()))?;
        let outputs = session_guard
            .run(ort::inputs![
                "input_ids" => input_ids_tensor,
                "attention_mask" => attention_mask_tensor,
            ])
            .map_err(|e| MemFuseError::Internal(format!("ONNX inference failed: {}", e)))?;

        let process_tensor = |shape: &[i64], data: &[f32]| -> Result<Vec<f32>> {
            // Mean Pooling
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

            // L2 Normalization (typical for sentence embeddings)
            let norm = mean_vec.iter().map(|x| x * x).sum::<f32>().sqrt();
            if norm > 0.0 {
                for val in mean_vec.iter_mut() {
                    *val /= norm;
                }
            }
            Ok(mean_vec)
        };

        // Extract tensor
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
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_text_embedder_load_missing_files() {
        let dir = tempdir().unwrap();

        // Empty directory - should fail at tokenizer existence check first
        // because model_path defaults to model_dir if model.onnx is missing.
        let res = TextEmbedder::load(dir.path());
        match res {
            Err(e) => assert!(e.to_string().contains("Tokenizer file not found")),
            Ok(_) => panic!("Should have failed"),
        }

        // Create tokenizer but still missing model.onnx
        File::create(dir.path().join("tokenizer.json")).unwrap();
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

    #[test]
    fn test_text_embedder_load_invalid_content() {
        let dir = tempdir().unwrap();
        File::create(dir.path().join("model.onnx"))
            .unwrap()
            .write_all(b"invalid")
            .unwrap();
        File::create(dir.path().join("tokenizer.json"))
            .unwrap()
            .write_all(b"invalid")
            .unwrap();

        let res = TextEmbedder::load(dir.path());
        assert!(res.is_err());
        // Error could be from tokenizer or ONNX
        let err_msg = res.unwrap_err().to_string();
        assert!(
            err_msg.contains("Failed to load tokenizer")
                || err_msg.contains("Failed to load model")
        );
    }

    #[test]
    fn test_formatting_safety() {
        // Just verify that using std::fmt::Debug on a pseudo-initialized or similar struct doesn't panic.
        let err = MemFuseError::Internal("test".into());
        let formatted = format!("{:?}", err);
        assert!(formatted.contains("Internal"));
    }

    #[tokio::test]
    async fn test_mock_embedding_engine() {
        struct MockEngine;
        #[async_trait]
        impl TextEmbeddingEngine for MockEngine {
            async fn embed(&self, text: &str) -> Result<Vec<f32>> {
                Ok(vec![text.len() as f32])
            }
        }

        let engine = MockEngine;
        let res = engine.embed("memfuse").await.unwrap();
        assert_eq!(res, vec![7.0]);
    }
}
