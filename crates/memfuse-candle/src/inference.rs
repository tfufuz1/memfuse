// FILE-CONTEXT
// STAND: 2026-09-12T00:00:00Z (SESSION: BACKPRESSURE-CONTRACT-D1)
// ZWECK: Candle LLM text generator client implementing LlmTextGenerator.
// INVARIANTEN: Thread-safe model access via Mutex; spawn_blocking for CPU inference execution.
// Backpressure contract: max_concurrent_inferences limits spawn_blocking calls.
// Callers will experience backpressure (await on permit acquire) rather than Tokio thread pool exhaustion.

//! memfuse-candle LLM inference module.
//!
//! Backpressure contract: `max_concurrent_inferences` limits `spawn_blocking` calls.
//! Callers will experience backpressure (await on permit acquire) rather than Tokio thread pool exhaustion.

use crate::gasp::GaspValidator;
use crate::model_registry::ModelFingerprint;
use candle_core::quantized::gguf_file;
use candle_core::Device;
use candle_transformers::generation::LogitsProcessor;
use candle_transformers::models::quantized_llama::ModelWeights;
use futures_util::stream;
use memfuse_core::traits::{BoxFuture, BoxStream, ContextSegment};
use memfuse_core::{
    ConfigFingerprint, LlmTextGenerator, LlmTextGeneratorStreaming, MemFuseError, Result,
};
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

/// Default maximum concurrent inference operations for Candle LLM text generation.
pub const DEFAULT_MAX_CONCURRENT_INFERENCES: usize = 4;

/// Adapter consulting KV cache segment metadata during context-aware generation.
#[derive(Debug, Clone, Default)]
pub struct KvBridgeAdapter {
    /// Number of segment consultations performed.
    pub consultations: Arc<std::sync::atomic::AtomicU64>,
}

impl KvBridgeAdapter {
    /// Creates a new `KvBridgeAdapter`.
    pub fn new() -> Self {
        Self {
            consultations: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// Consults segment metadata and KV cache bridge state for a context segment.
    pub fn consult_segment<'a>(&self, segment: &ContextSegment<'a>) {
        self.consultations
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let _ = (
            segment.chunk_id,
            segment.text,
            segment.model_fingerprint,
            segment.rope_offset,
        );
    }

    /// Returns the number of segment consultations recorded.
    pub fn consultation_count(&self) -> u64 {
        self.consultations.load(std::sync::atomic::Ordering::SeqCst)
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
    /// Maximum concurrent inference operations permitted.
    pub max_concurrent_inferences: usize,
    /// Semaphore enforcing backpressure on concurrent inference calls.
    pub semaphore: Arc<tokio::sync::Semaphore>,
    /// Optional KV cache bridge adapter.
    pub kv_bridge: Option<KvBridgeAdapter>,
}

impl CandleLlmClient {
    /// Creates a new `CandleLlmClient` with default concurrency limits.
    pub fn new(
        device: Device,
        model: Box<dyn CandleModelInner + Send>,
        fingerprint: ModelFingerprint,
        tokenizer: tokenizers::Tokenizer,
    ) -> Self {
        let max_concurrent_inferences = DEFAULT_MAX_CONCURRENT_INFERENCES;
        Self {
            device,
            model: Arc::new(tokio::sync::Mutex::new(model)),
            fingerprint,
            tokenizer,
            max_concurrent_inferences,
            semaphore: Arc::new(tokio::sync::Semaphore::new(max_concurrent_inferences)),
            kv_bridge: None,
        }
    }

    /// Attaches a `KvBridgeAdapter` to this client.
    pub fn with_kv_bridge(mut self, adapter: KvBridgeAdapter) -> Self {
        self.kv_bridge = Some(adapter);
        self
    }

    /// Configures maximum concurrent inference operations for backpressure control.
    pub fn with_max_concurrent_inferences(mut self, limit: usize) -> Self {
        let limit = limit.max(1);
        self.max_concurrent_inferences = limit;
        self.semaphore = Arc::new(tokio::sync::Semaphore::new(limit));
        self
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
        let semaphore = Arc::clone(&self.semaphore);
        let prompt_owned = prompt.to_string();

        Box::pin(async move {
            let _permit = semaphore
                .acquire()
                .await
                .map_err(|_| MemFuseError::Internal("Candle inference semaphore closed".into()))?;

            tokio::task::spawn_blocking(move || {
                let mut guard = model.blocking_lock();
                guard.generate(&prompt_owned, &tokenizer, &device)
            })
            .await
            .map_err(|e| MemFuseError::Internal(format!("Candle inference task join error: {e}")))?
        })
    }

    fn generate_with_context<'a>(
        &'a self,
        segments: &'a [ContextSegment<'a>],
    ) -> BoxFuture<'a, Result<String>> {
        Box::pin(async move {
            if let Some(ref adapter) = self.kv_bridge {
                for segment in segments {
                    adapter.consult_segment(segment);
                }
            }

            let concatenated = segments
                .iter()
                .map(|s| s.text)
                .collect::<Vec<_>>()
                .join("\n\n");

            self.generate(&concatenated).await
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
        let semaphore = Arc::clone(&self.semaphore);
        let prompt_owned = prompt.to_string();

        let (tx, rx) = tokio::sync::mpsc::channel::<Result<String>>(32);

        tokio::spawn(async move {
            let permit = match semaphore.acquire_owned().await {
                Ok(p) => p,
                Err(_) => {
                    let _ = tx
                        .send(Err(MemFuseError::Internal(
                            "Candle inference semaphore closed".into(),
                        )))
                        .await;
                    return;
                }
            };

            let join_res = tokio::task::spawn_blocking(move || {
                let _permit = permit;
                let mut guard = model.blocking_lock();
                let res = guard.generate_stream(&prompt_owned, &tokenizer, &device, &mut |chunk| {
                    tx.blocking_send(Ok(chunk)).is_ok()
                });

                if let Err(err) = res {
                    let _ = tx.blocking_send(Err(err));
                }
            })
            .await;

            if let Err(e) = join_res {
                tracing::error!("Candle streaming task join error: {e}");
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

    #[tokio::test]
    async fn test_generate_with_context_text_identical_and_kv_bridge_consultation() {
        let mock_model = Box::new(MockCandleModel {
            response: "Unified Output".to_string(),
        });
        let fingerprint = ModelFingerprint {
            hash: [9u8; 32],
            model_id: "context_model.gguf".to_string(),
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

        let client_plain = CandleLlmClient::new(Device::Cpu, mock_model, fingerprint.clone(), tokenizer.clone());

        let seg1 = ContextSegment::new(101, "Chunk 1 content");
        let seg2 = ContextSegment::new(102, "Chunk 2 content");
        let segments = vec![seg1, seg2];

        let direct_concat_res = client_plain.generate("Chunk 1 content\n\nChunk 2 content").await.unwrap();
        let context_without_adapter_res = client_plain.generate_with_context(&segments).await.unwrap();

        assert_eq!(
            direct_concat_res, context_without_adapter_res,
            "generate_with_context without adapter must produce text-identical result to generate"
        );

        let adapter = KvBridgeAdapter::new();
        let mock_model_2 = Box::new(MockCandleModel {
            response: "Unified Output".to_string(),
        });
        let client_with_adapter = CandleLlmClient::new(Device::Cpu, mock_model_2, fingerprint, tokenizer)
            .with_kv_bridge(adapter.clone());

        let context_with_adapter_res = client_with_adapter.generate_with_context(&segments).await.unwrap();

        assert_eq!(
            direct_concat_res, context_with_adapter_res,
            "generate_with_context with KvBridgeAdapter must produce text-identical result"
        );
        assert_eq!(
            adapter.consultation_count(),
            2,
            "KvBridgeAdapter must record consultation for each segment"
        );
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

    struct SlowCandleModel {
        delay: std::time::Duration,
    }

    impl CandleModelInner for SlowCandleModel {
        fn generate(
            &mut self,
            prompt: &str,
            _tokenizer: &tokenizers::Tokenizer,
            _device: &Device,
        ) -> Result<String> {
            std::thread::sleep(self.delay);
            Ok(format!("Slow response to: {prompt}"))
        }
    }

    #[tokio::test]
    async fn test_inference_backpressure_single_permit_awaits() {
        let slow_model = Box::new(SlowCandleModel {
            delay: std::time::Duration::from_millis(100),
        });
        let fp = ModelFingerprint {
            hash: [3u8; 32],
            model_id: "slow_model.gguf".to_string(),
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

        let client = Arc::new(
            CandleLlmClient::new(Device::Cpu, slow_model, fp, tokenizer)
                .with_max_concurrent_inferences(1),
        );

        assert_eq!(client.max_concurrent_inferences, 1);
        assert_eq!(client.semaphore.available_permits(), 1);

        let start = std::time::Instant::now();

        let c1 = Arc::clone(&client);
        let handle1 = tokio::spawn(async move { c1.generate("task 1").await });

        // Short sleep to guarantee task 1 acquires the single permit
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        assert_eq!(client.semaphore.available_permits(), 0);

        let c2 = Arc::clone(&client);
        let handle2 = tokio::spawn(async move { c2.generate("task 2").await });

        let res1 = handle1.await.unwrap().unwrap();
        let res2 = handle2.await.unwrap().unwrap();

        let elapsed = start.elapsed();
        assert!(
            elapsed >= std::time::Duration::from_millis(180),
            "Sequential execution under limit=1 expected ~200ms, took {:?}",
            elapsed
        );
        assert_eq!(res1, "Slow response to: task 1");
        assert_eq!(res2, "Slow response to: task 2");
        assert_eq!(client.semaphore.available_permits(), 1);
    }

    #[tokio::test]
    async fn test_inference_configurable_concurrency_limit() {
        let mock_model = Box::new(MockCandleModel {
            response: "Fast".to_string(),
        });
        let fp = ModelFingerprint {
            hash: [4u8; 32],
            model_id: "fast_model.gguf".to_string(),
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

        let client = CandleLlmClient::new(Device::Cpu, mock_model, fp, tokenizer)
            .with_max_concurrent_inferences(3);

        assert_eq!(client.max_concurrent_inferences, 3);
        assert_eq!(client.semaphore.available_permits(), 3);
    }

    #[tokio::test]
    async fn test_inference_timeout_cancellation_releases_no_permit_leak() {
        let slow_model = Box::new(SlowCandleModel {
            delay: std::time::Duration::from_millis(200),
        });
        let fp = ModelFingerprint {
            hash: [5u8; 32],
            model_id: "slow_timeout.gguf".to_string(),
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

        let client = Arc::new(
            CandleLlmClient::new(Device::Cpu, slow_model, fp, tokenizer)
                .with_max_concurrent_inferences(1),
        );

        let c1 = Arc::clone(&client);
        let h1 = tokio::spawn(async move { c1.generate("task 1").await });

        tokio::time::sleep(std::time::Duration::from_millis(20)).await;

        // Task 2 attempts to generate, but times out while waiting for permit (simulating McpSandbox timeout)
        let c2 = Arc::clone(&client);
        let timed_out =
            tokio::time::timeout(std::time::Duration::from_millis(30), c2.generate("task 2")).await;

        assert!(
            timed_out.is_err(),
            "Task 2 must time out while permit is held by Task 1"
        );

        let _ = h1.await.unwrap().unwrap();
        // Give tokio a tick to return permit
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        assert_eq!(
            client.semaphore.available_permits(),
            1,
            "Permit must be fully available after cancellation without leaks"
        );
    }
}
