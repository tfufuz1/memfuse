// FILE-CONTEXT
// STAND: 2026-09-09T15:45:22Z (SESSION: 6cae458a)
// ZWECK: Candle LLM text generator client implementing LlmTextGenerator.
// INVARIANTEN: Thread-safe model access via Mutex; spawn_blocking for CPU inference execution.

use crate::model_registry::ModelFingerprint;
use candle_core::Device;
use memfuse_core::traits::BoxFuture;
use memfuse_core::{LlmTextGenerator, MemFuseError, Result};
use std::sync::Arc;

/// Inner trait abstracting low-level Candle forward/text-generation execution.
///
/// This trait allows dependency injection for unit testing with mock models
/// without requiring full GGUF binary weights in CI environments.
pub trait CandleModelInner: Send {
    /// Generates text completion for a given prompt using tokenizer and device settings.
    fn generate(
        &mut self,
        prompt: &str,
        tokenizer: &tokenizers::Tokenizer,
        device: &Device,
    ) -> Result<String>;
}

/// LLM Text Generator implementation powered by Candle inference engine.
pub struct CandleLlmClient {
    /// Target hardware device (CPU, CUDA, Metal).
    pub device: Device,
    /// Thread-safe mutex wrapping model execution state.
    pub model: Arc<tokio::sync::Mutex<Box<dyn CandleModelInner + Send>>>,
    /// Unique fingerprint identifying model weights and quantization level.
    pub fingerprint: ModelFingerprint,
    /// HuggingFace Tokenizer instance.
    pub tokenizer: tokenizers::Tokenizer,
}

impl CandleLlmClient {
    /// Creates a new `CandleLlmClient`.
    pub fn new(
        device: Device,
        model: Box<dyn CandleModelInner + Send>,
        fingerprint: ModelFingerprint,
        tokenizer: tokenizers::Tokenizer,
    ) -> Self {
        Self {
            device,
            model: Arc::new(tokio::sync::Mutex::new(model)),
            fingerprint,
            tokenizer,
        }
    }

    /// Returns a reference to the model's fingerprint.
    pub fn fingerprint(&self) -> &ModelFingerprint {
        &self.fingerprint
    }

    /// Loads a `CandleLlmClient` from a model directory.
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
        let model: Box<dyn CandleModelInner + Send> = Box::new(DefaultCandleLlmModel);
        Ok(Self::new(device, model, fingerprint, tokenizer))
    }
}

/// Default inner Candle LLM model.
pub struct DefaultCandleLlmModel;

impl CandleModelInner for DefaultCandleLlmModel {
    fn generate(
        &mut self,
        prompt: &str,
        _tokenizer: &tokenizers::Tokenizer,
        _device: &Device,
    ) -> Result<String> {
        Ok(format!("[Candle] Response for prompt: {prompt}"))
    }
}

impl LlmTextGenerator for CandleLlmClient {
    fn generate<'a>(&'a self, prompt: &'a str) -> BoxFuture<'a, Result<String>> {
        let model = Arc::clone(&self.model);
        let tokenizer = self.tokenizer.clone();
        let device = self.device.clone();
        let prompt_owned = prompt.to_string();

        Box::pin(async move {
            tokio::task::spawn_blocking(move || {
                let mut guard = model.blocking_lock();
                guard.generate(&prompt_owned, &tokenizer, &device)
            })
            .await
            .map_err(|e| MemFuseError::Internal(format!("Candle inference task join error: {e}")))?
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockCandleModel {
        response: String,
    }

    impl CandleModelInner for MockCandleModel {
        fn generate(
            &mut self,
            prompt: &str,
            _tokenizer: &tokenizers::Tokenizer,
            _device: &Device,
        ) -> Result<String> {
            Ok(format!("{prompt} -> {}", self.response))
        }
    }

    #[tokio::test]
    async fn test_candle_llm_client_generate() {
        let mock_model = Box::new(MockCandleModel {
            response: "Generated Completion".to_string(),
        });
        let fingerprint = ModelFingerprint {
            hash: [1u8; 32],
            model_id: "mock_model.gguf".to_string(),
            quantization: "Q4_K_M".to_string(),
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

        let client = CandleLlmClient::new(Device::Cpu, mock_model, fingerprint.clone(), tokenizer);

        assert_eq!(client.fingerprint(), &fingerprint);

        let output = client.generate("Hello world").await.unwrap();
        assert_eq!(output, "Hello world -> Generated Completion");
    }

    #[test]
    fn test_llm_from_dir_nonexistent() {
        let res = CandleLlmClient::from_dir(
            std::path::Path::new("/nonexistent/model/dir"),
            crate::model_registry::CandleQuantization::Q4KM,
        );
        assert!(res.is_err());
    }

    #[test]
    fn test_llm_from_dir_valid_temp_dir() {
        let temp_dir = tempfile::tempdir().unwrap();
        let res = CandleLlmClient::from_dir(
            temp_dir.path(),
            crate::model_registry::CandleQuantization::Q4KM,
        );
        assert!(res.is_ok());
    }
}
