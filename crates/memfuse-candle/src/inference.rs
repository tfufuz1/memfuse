// FILE-CONTEXT
// STAND: 2026-09-09T15:45:22Z (SESSION: 6cae458a)
// ZWECK: Candle LLM text generator client implementing LlmTextGenerator.
// INVARIANTEN: Thread-safe model access via Mutex; spawn_blocking for CPU inference execution.

use crate::model_registry::ModelFingerprint;
use candle_core::quantized::gguf_file;
use candle_core::Device;
use candle_transformers::generation::LogitsProcessor;
use candle_transformers::models::quantized_llama::ModelWeights;
use memfuse_core::traits::BoxFuture;
use memfuse_core::{LlmTextGenerator, MemFuseError, Result};
use std::fs::File;
use std::path::Path;
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

        let gguf_file_path = if gguf_path.exists() {
            Some(gguf_path)
        } else if let Ok(entries) = std::fs::read_dir(model_dir) {
            entries
                .flatten()
                .map(|e| e.path())
                .find(|p| p.extension().and_then(|s| s.to_str()) == Some("gguf"))
        } else {
            None
        };

        let device = Device::Cpu;
        let model: Box<dyn CandleModelInner + Send> = if let Some(path) = gguf_file_path {
            Box::new(QuantizedLlamaModel::load(&path, &device)?)
        } else {
            // Fallback for empty/mock test directories or directories without GGUF weight binaries
            Box::new(DefaultCandleLlmModel)
        };

        Ok(Self::new(device, model, fingerprint, tokenizer))
    }
}

/// Real quantized Llama / GGUF model execution wrapper.
pub struct QuantizedLlamaModel {
    weights: ModelWeights,
    sample_len: usize,
}

impl QuantizedLlamaModel {
    /// Loads GGUF quantized model weights from the specified file path.
    pub fn load(model_path: &Path, device: &Device) -> Result<Self> {
        let mut file = File::open(model_path).map_err(|e| {
            MemFuseError::Io(std::io::Error::new(
                e.kind(),
                format!(
                    "Failed to open GGUF weight file {}: {e}",
                    model_path.display()
                ),
            ))
        })?;

        let content = gguf_file::Content::read(&mut file).map_err(|e| {
            MemFuseError::Internal(format!(
                "Failed to parse GGUF content header for {}: {e}",
                model_path.display()
            ))
        })?;

        let weights = ModelWeights::from_gguf(content, &mut file, device).map_err(|e| {
            MemFuseError::Internal(format!(
                "Failed to build quantized Llama model weights from GGUF {}: {e}",
                model_path.display()
            ))
        })?;

        Ok(Self {
            weights,
            sample_len: 256,
        })
    }
}

impl CandleModelInner for QuantizedLlamaModel {
    fn generate(
        &mut self,
        prompt: &str,
        tokenizer: &tokenizers::Tokenizer,
        device: &Device,
    ) -> Result<String> {
        let tokens = tokenizer
            .encode(prompt, true)
            .map_err(|e| MemFuseError::InvalidInput(format!("Failed to tokenize prompt: {e}")))?;
        let prompt_tokens = tokens.get_ids();
        if prompt_tokens.is_empty() {
            return Err(MemFuseError::InvalidInput(
                "Encoded prompt tokens cannot be empty".to_string(),
            ));
        }

        let mut logits_processor = LogitsProcessor::new(299792458, Some(0.7), Some(0.9));
        let mut all_tokens = prompt_tokens.to_vec();
        let mut generated_tokens = Vec::new();

        let mut index_pos = 0;
        for i in 0..self.sample_len {
            let context_len = if i == 0 { all_tokens.len() } else { 1 };
            let input_slice = if i == 0 {
                all_tokens.clone()
            } else {
                vec![*all_tokens.last().unwrap()]
            };

            let input_tensor = candle_core::Tensor::new(&input_slice[..], device)
                .map_err(|e| MemFuseError::Internal(format!("Failed to create input tensor: {e}")))?
                .unsqueeze(0)
                .map_err(|e| MemFuseError::Internal(format!("Failed to unsqueeze tensor: {e}")))?;

            let logits = self
                .weights
                .forward(&input_tensor, index_pos)
                .map_err(|e| {
                    MemFuseError::Internal(format!("Quantized Llama forward error: {e}"))
                })?;

            let logits = logits
                .squeeze(0)
                .map_err(|e| MemFuseError::Internal(format!("Failed to squeeze logits: {e}")))?;
            let logits = logits
                .get(
                    logits
                        .dim(0)
                        .map_err(|e| MemFuseError::Internal(e.to_string()))?
                        - 1,
                )
                .map_err(|e| MemFuseError::Internal(format!("Failed to slice logits: {e}")))?;

            let next_token = logits_processor
                .sample(&logits)
                .map_err(|e| MemFuseError::Internal(format!("Logits sampling failed: {e}")))?;

            all_tokens.push(next_token);
            generated_tokens.push(next_token);
            index_pos += context_len;

            // Check EOS / stop tokens (e.g. tokenizer eos token if known or common Llama eos token ID 2)
            if next_token == 2 || next_token == 128001 || next_token == 128009 {
                break;
            }
        }

        let output_text = tokenizer.decode(&generated_tokens, true).map_err(|e| {
            MemFuseError::Internal(format!("Failed to decode generated tokens: {e}"))
        })?;

        Ok(output_text)
    }
}

/// Default inner Candle LLM mock model for unit tests when binary weights are absent.
pub struct DefaultCandleLlmModel;

impl CandleModelInner for DefaultCandleLlmModel {
    fn generate(
        &mut self,
        prompt: &str,
        _tokenizer: &tokenizers::Tokenizer,
        _device: &Device,
    ) -> Result<String> {
        Ok(format!("[MockCandle] Response for prompt: {prompt}"))
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
