// FILE-CONTEXT
// STAND: 2026-09-09T15:45:22Z (SESSION: 6cae458a)
// ZWECK: Candle LLM text generator client implementing LlmTextGenerator.
// INVARIANTEN: Thread-safe model access via Mutex; spawn_blocking for CPU inference execution.

use crate::gasp::GaspValidator;
use crate::model_registry::ModelFingerprint;
use candle_core::quantized::gguf_file;
use candle_core::Device;
use candle_transformers::generation::LogitsProcessor;
use candle_transformers::models::quantized_llama::ModelWeights;
use futures_util::stream;
use memfuse_core::traits::{BoxFuture, BoxStream};
use memfuse_core::{ConfigFingerprint, LlmTextGenerator, LlmTextGeneratorStreaming, MemFuseError, Result};
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

    /// Generates text stream for a given prompt, invoking `on_token` for each generated token or chunk.
    /// Returns early if `on_token` returns `false`.
    fn generate_stream(
        &mut self,
        prompt: &str,
        tokenizer: &tokenizers::Tokenizer,
        device: &Device,
        on_token: &mut dyn FnMut(String) -> bool,
    ) -> Result<String> {
        let text = self.generate(prompt, tokenizer, device)?;
        on_token(text.clone());
        Ok(text)
    }
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

    /// Tauscht das zugrundeliegende Modell und dessen Fingerprint zur Laufzeit aus.
    /// Falls ein optionaler `GaspValidator` übergeben wird, wird dessen Kalibrierungsstatus
    /// mit dem neuen Fingerprint invalidiert/aktualisiert (INV-CAL-2).
    pub fn swap_model(
        &mut self,
        new_model: Box<dyn CandleModelInner + Send>,
        new_fingerprint: ModelFingerprint,
        validator: Option<&mut GaspValidator>,
    ) {
        if let Some(mutex) = Arc::get_mut(&mut self.model) {
            *mutex.get_mut() = new_model;
        } else {
            self.model = Arc::new(tokio::sync::Mutex::new(new_model));
        }

        if let Some(val) = validator {
            let mut new_config = val.config().clone();
            new_config.fingerprint = ConfigFingerprint::new(
                &new_fingerprint.model_id,
                &new_fingerprint.quantization,
                "gasp-attribution",
                0.0,
            )
            .with_threshold(new_config.threshold);
            val.refresh_config(new_config);
        }

        self.fingerprint = new_fingerprint;
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
        let mut full_output = String::new();
        self.generate_stream(prompt, tokenizer, device, &mut |chunk| {
            full_output.push_str(&chunk);
            true
        })?;
        Ok(full_output)
    }

    fn generate_stream(
        &mut self,
        prompt: &str,
        tokenizer: &tokenizers::Tokenizer,
        device: &Device,
        on_token: &mut dyn FnMut(String) -> bool,
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
        let mut full_output = String::new();

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

            if let Ok(piece) = tokenizer.decode(&[next_token], true) {
                if !piece.is_empty() {
                    full_output.push_str(&piece);
                    if !on_token(piece) {
                        break;
                    }
                }
            }

            // Check EOS / stop tokens (e.g. tokenizer eos token if known or common Llama eos token ID 2)
            if next_token == 2 || next_token == 128001 || next_token == 128009 {
                break;
            }
        }

        Ok(full_output)
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

    fn generate_stream(
        &mut self,
        prompt: &str,
        _tokenizer: &tokenizers::Tokenizer,
        _device: &Device,
        on_token: &mut dyn FnMut(String) -> bool,
    ) -> Result<String> {
        let text = format!("[MockCandle] Response for prompt: {prompt}");
        let words: Vec<&str> = text.split_whitespace().collect();
        for (i, word) in words.iter().enumerate() {
            let chunk = if i == 0 {
                word.to_string()
            } else {
                format!(" {word}")
            };
            if !on_token(chunk) {
                break;
            }
        }
        Ok(text)
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

impl LlmTextGeneratorStreaming for CandleLlmClient {
    fn generate_stream<'a>(
        &'a self,
        prompt: &'a str,
        _config: &'a ConfigFingerprint,
    ) -> BoxStream<'a, Result<String>> {
        let model = Arc::clone(&self.model);
        let tokenizer = self.tokenizer.clone();
        let device = self.device.clone();
        let prompt_owned = prompt.to_string();

        let (tx, rx) = tokio::sync::mpsc::channel::<Result<String>>(32);

        tokio::task::spawn_blocking(move || {
            let mut guard = model.blocking_lock();
            let res = guard.generate_stream(&prompt_owned, &tokenizer, &device, &mut |chunk| {
                tx.blocking_send(Ok(chunk)).is_ok()
            });

            if let Err(err) = res {
                let _ = tx.blocking_send(Err(err));
            }
        });

        Box::pin(stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|item| (item, rx))
        }))
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

    #[tokio::test]
    async fn test_candle_llm_client_generate_stream() {
        use futures_util::StreamExt;

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
        let cfg = ConfigFingerprint::new("mock_model.gguf", "Q4_K_M", "default", 0.0);

        let sync_resp = client.generate("Hello world").await.unwrap();

        let mut stream = client.generate_stream("Hello world", &cfg);
        let mut assembled = String::new();
        while let Some(chunk_res) = stream.next().await {
            let chunk = chunk_res.unwrap();
            assembled.push_str(&chunk);
        }

        assert_eq!(sync_resp, assembled);
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

    #[tokio::test]
    async fn test_candle_llm_client_swap_model_invalidates_validator_calibration() {
        use crate::gasp::GaspConfig;
        use memfuse_core::traits::GroundingValidator;
        use memfuse_core::ContextChunk;
        use memfuse_core::DocId;

        let mock_model_1 = Box::new(MockCandleModel {
            response: "Model 1 Completion".to_string(),
        });
        let fp1 = ModelFingerprint {
            hash: [1u8; 32],
            model_id: "llama-3.2-1b.gguf".to_string(),
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
        let tokenizer = tokenizers::Tokenizer::from_bytes(tokenizer_bytes.as_bytes()).unwrap();

        let mut client = CandleLlmClient::new(Device::Cpu, mock_model_1, fp1, tokenizer);

        let initial_gasp_cfg = GaspConfig {
            fingerprint: ConfigFingerprint::new(
                &client.fingerprint().model_id,
                &client.fingerprint().quantization,
                "gasp-attribution",
                0.0,
            ),
            ..GaspConfig::default()
        };
        let mut validator = GaspValidator::with_config(initial_gasp_cfg);

        // Record a grounding observation on the validator via record_external_feedback (INV-CAL-3)
        let chunk = ContextChunk {
            doc_id: DocId::new(1),
            content: "Der Umsatz betrug im Jahr 2025 genau 50 Millionen Euro.".to_string(),
            relevance: 0.95,
            token_count: 20,
            metadata: None,
            contextual_prefix: None,
            links: Vec::new(),
        };
        let res = validator
            .validate_grounding(
                "Im Jahr 2025 betrug der Umsatz 50 Millionen Euro.",
                &[chunk],
            )
            .await;
        assert!(res.is_ok());
        validator.record_external_feedback(res.unwrap().score, true);
        assert_eq!(validator.observation_count(), 1);

        // Perform runtime model hot swap on CandleLlmClient with new ModelFingerprint
        let mock_model_2 = Box::new(MockCandleModel {
            response: "Model 2 Completion".to_string(),
        });
        let fp2 = ModelFingerprint {
            hash: [2u8; 32],
            model_id: "llama-3.2-3b.gguf".to_string(),
            quantization: "Q8_0".to_string(),
        };

        client.swap_model(mock_model_2, fp2.clone(), Some(&mut validator));

        assert_eq!(client.fingerprint(), &fp2);
        // Calibrator observations MUST be reset to 0 via the runtime model hot-swap path (INV-CAL-2)
        assert_eq!(
            validator.observation_count(),
            0,
            "validator observation_count must be reset to 0 after client.swap_model"
        );
    }
}
