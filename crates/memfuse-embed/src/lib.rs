//! memfuse-embed — In-process text embeddings using ONNX Runtime.
//!
//! This crate provides a high-level API for generating vector embeddings from text
//! without requiring external API calls. It uses the `ort` crate for ONNX Runtime
//! and `tokenizers` for text preprocessing.

#![deny(unsafe_code)]

use memfuse_core::{MemFuseError, Result};
use ort::session::Session;
use ort::value::Value;
use std::path::Path;
use tokenizers::Tokenizer;
use tracing::{debug, info};

/// Handles text tokenization and ONNX model inference.
pub struct TextEmbedder {
    session: std::sync::Mutex<Session>,
    tokenizer: Tokenizer,
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

    /// Downloads and loads a model from HuggingFace Hub.
    pub fn from_hub(model_id: &str) -> Result<Self> {
        use hf_hub::api::sync::Api;
        use hf_hub::{Repo, RepoType};

        info!("Downloading model '{}' from HuggingFace Hub", model_id);
        let api = Api::new().map_err(|e| MemFuseError::Internal(format!("HF API error: {}", e)))?;
        let repo = api.repo(Repo::new(model_id.to_string(), RepoType::Model));

        let model_path = repo
            .get("onnx/model.onnx")
            .or_else(|_| repo.get("model.onnx"))
            .map_err(|e| MemFuseError::Internal(format!("Failed to download model: {}", e)))?;

        // Both files are in the same cache directory usually
        let parent = model_path
            .parent()
            .ok_or_else(|| MemFuseError::Internal("Model path has no parent directory".into()))?;
        Self::load(parent)
    }

    /// Loads the default embedding model (all-MiniLM-L6-v2).
    pub fn load_default() -> Result<Self> {
        Self::from_hub("sentence-transformers/all-MiniLM-L6-v2")
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
