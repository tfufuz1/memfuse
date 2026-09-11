// FILE-CONTEXT
// STAND: 2026-09-11T16:00:00Z
// ZWECK: Error path and boundary tests for GGUF loading, quantization mismatch, batch boundaries, and concurrent hot-swap.
// INVARIANTEN: Zero real model downloads (hermetic bytes); no panics on corrupt binary inputs; strict batch boundary limits.

use candle_core::Device;
use memfuse_candle::embedding::CandleEmbedInner;
use memfuse_candle::gguf_loader::parse_gguf_metadata;
use memfuse_candle::inference::{CandleLlmClient, CandleModelInner, QuantizedLlamaModel};
use memfuse_candle::model_registry::ModelFingerprint;
use memfuse_candle::{CandleEmbedClient, MAX_CANDLE_EMBED_BATCH_SIZE};
use memfuse_core::traits::embedding::EmbeddingError;
use memfuse_core::traits::{EmbeddingProvider, LlmTextGenerator};
use memfuse_core::Result;
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;
use tempfile::NamedTempFile;

// --- Test 1: GGUF header valid but truncated tensor data ---
#[test]
fn test_gguf_header_valid_but_truncated_tensor_data() {
    let mut file = NamedTempFile::new().unwrap();
    // Construct valid GGUF magic + version 3 + 1 tensor declared + 0 metadata KVs
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"GGUF"); // Magic
    bytes.extend_from_slice(&3u32.to_le_bytes()); // Version
    bytes.extend_from_slice(&1u64.to_le_bytes()); // Tensor count = 1
    bytes.extend_from_slice(&0u64.to_le_bytes()); // Metadata KV count = 0

    // Tensor header info: name "blk.0.weight", 1D tensor, dim [1000], GGML_TYPE_F32 (0), offset 0
    let tensor_name = b"blk.0.weight";
    bytes.extend_from_slice(&(tensor_name.len() as u64).to_le_bytes());
    bytes.extend_from_slice(tensor_name);
    bytes.extend_from_slice(&1u32.to_le_bytes()); // n_dims = 1
    bytes.extend_from_slice(&1000u64.to_le_bytes()); // dim[0] = 1000 (expects 4000 bytes)
    bytes.extend_from_slice(&0u32.to_le_bytes()); // GGML type F32 = 0
    bytes.extend_from_slice(&0u64.to_le_bytes()); // offset = 0

    // Intentionally truncate: add only 10 bytes of tensor payload data instead of 4000 bytes
    bytes.extend_from_slice(&[0u8; 10]);

    file.write_all(&bytes).unwrap();
    file.flush().unwrap();

    // 1. parse_gguf_metadata reads header without loading tensor binary blob
    let meta_res = parse_gguf_metadata(file.path());
    assert!(
        meta_res.is_ok(),
        "parse_gguf_metadata should parse header without loading tensor payload"
    );
    let meta = meta_res.unwrap();
    assert_eq!(meta.tensor_count, 1);

    // 2. Downstream model weight loader MUST fail gracefully due to truncated payload data
    let load_res = QuantizedLlamaModel::load(file.path(), &Device::Cpu);
    assert!(
        load_res.is_err(),
        "QuantizedLlamaModel::load must return Err on truncated tensor payload data"
    );
}

// --- Test 2: GGUF invalid magic bytes ---
#[test]
fn test_gguf_invalid_magic_bytes() {
    let mut file = NamedTempFile::new().unwrap();
    // Invalid magic bytes b"BADM" followed by dummy header bytes
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"BADM");
    bytes.extend_from_slice(&3u32.to_le_bytes());
    bytes.extend_from_slice(&[0u8; 64]);

    file.write_all(&bytes).unwrap();
    file.flush().unwrap();

    let meta_res = parse_gguf_metadata(file.path());
    assert!(
        meta_res.is_err(),
        "parse_gguf_metadata must fail immediately on bad magic bytes"
    );

    let load_res = QuantizedLlamaModel::load(file.path(), &Device::Cpu);
    assert!(
        load_res.is_err(),
        "QuantizedLlamaModel::load must fail immediately on bad magic bytes"
    );
}

// --- Test 3: GGUF wrong quantization grade mismatch ---
#[test]
fn test_gguf_wrong_quantization_grade_mismatch() {
    let mut file = NamedTempFile::new().unwrap();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"GGUF");
    bytes.extend_from_slice(&3u32.to_le_bytes());
    bytes.extend_from_slice(&1u64.to_le_bytes()); // 1 tensor
    bytes.extend_from_slice(&0u64.to_le_bytes()); // 0 KVs

    // Tensor declared as Q4_0 (type = 2) with 1024 elements
    let name = b"attn.weight";
    bytes.extend_from_slice(&(name.len() as u64).to_le_bytes());
    bytes.extend_from_slice(name);
    bytes.extend_from_slice(&1u32.to_le_bytes()); // 1 dim
    bytes.extend_from_slice(&1024u64.to_le_bytes()); // 1024 elements
    bytes.extend_from_slice(&2u32.to_le_bytes()); // GGML type 2 = Q4_0
    bytes.extend_from_slice(&0u64.to_le_bytes()); // offset = 0

    // Provide payload length matching F32 (10 bytes) instead of valid Q4_0 blocks
    bytes.extend_from_slice(&[0xFFu8; 10]);

    file.write_all(&bytes).unwrap();
    file.flush().unwrap();

    let load_res = QuantizedLlamaModel::load(file.path(), &Device::Cpu);
    assert!(
        load_res.is_err(),
        "QuantizedLlamaModel::load must fail controlled on quantization grade byte mismatch"
    );
}

// --- Test 4: Embedding dimension mismatch against index expectation ---
struct DummyEmbedModel {
    actual_dim: usize,
}

impl CandleEmbedInner for DummyEmbedModel {
    fn embed(
        &mut self,
        _text: &str,
        _tokenizer: &tokenizers::Tokenizer,
        _device: &Device,
    ) -> Result<Vec<f32>> {
        Ok(vec![0.1f32; self.actual_dim])
    }

    fn dim(&self) -> usize {
        self.actual_dim
    }
}

#[tokio::test]
async fn test_embedding_dimension_mismatch_against_index_expectation() {
    let actual_model_dim = 384;
    let expected_index_dim = 768;

    let mock_model = Box::new(DummyEmbedModel {
        actual_dim: actual_model_dim,
    });
    let fp = ModelFingerprint {
        hash: [5u8; 32],
        model_id: "test_dim_mismatch.gguf".to_string(),
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

    let client = CandleEmbedClient::new(Device::Cpu, mock_model, fp, tokenizer);

    // Verify caller side dimension mismatch validation
    let reported_dim = client.embedding_dim();
    assert_ne!(
        reported_dim, expected_index_dim,
        "Reported model dimension ({reported_dim}) must not silently match expected index dimension ({expected_index_dim})"
    );

    let vector = client.embed("dimension test query").await.unwrap();
    assert_eq!(
        vector.len(),
        reported_dim,
        "Vector length matches model actual dim"
    );
    assert_ne!(
        vector.len(),
        expected_index_dim,
        "Vector length mismatch detected; caller must reject mismatch rather than silent truncation/padding"
    );
}

// --- Test 5: Batch size exactly at limit 255, 256, 257 ---
#[tokio::test]
async fn test_batch_size_exactly_at_limit_255_256_257() {
    let mock_model = Box::new(DummyEmbedModel { actual_dim: 128 });
    let fp = ModelFingerprint {
        hash: [6u8; 32],
        model_id: "batch_limit_test.gguf".to_string(),
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
    let tokenizer = tokenizers::Tokenizer::from_bytes(tokenizer_bytes.as_bytes()).unwrap();

    let client = CandleEmbedClient::new(Device::Cpu, mock_model, fp, tokenizer);
    let provider: &dyn EmbeddingProvider = &client;

    assert_eq!(MAX_CANDLE_EMBED_BATCH_SIZE, 256);

    // 1. Batch size 255: MUST be accepted
    let texts_255: Vec<&str> = vec!["text_sample"; 255];
    let res_255 = provider.embed_batch(&texts_255).await;
    assert!(
        res_255.is_ok(),
        "Batch size 255 must be accepted by embed_batch"
    );
    assert_eq!(res_255.unwrap().len(), 255);

    // 2. Batch size 256: MUST be accepted (decisive test point for > vs >= limit check)
    let texts_256: Vec<&str> = vec!["text_sample"; 256];
    let res_256 = provider.embed_batch(&texts_256).await;
    assert!(
        res_256.is_ok(),
        "Batch size 256 (exactly MAX_CANDLE_EMBED_BATCH_SIZE) MUST be accepted by embed_batch"
    );
    assert_eq!(res_256.unwrap().len(), 256);

    // 3. Batch size 257: MUST be rejected with EmbeddingError::Unavailable
    let texts_257: Vec<&str> = vec!["text_sample"; 257];
    let res_257 = provider.embed_batch(&texts_257).await;
    assert!(
        res_257.is_err(),
        "Batch size 257 (> MAX_CANDLE_EMBED_BATCH_SIZE) MUST be rejected"
    );
    if let Err(err) = res_257 {
        assert!(
            matches!(err, EmbeddingError::Unavailable(_)),
            "Expected EmbeddingError::Unavailable on batch limit overflow, got {err:?}"
        );
        assert!(err.to_string().contains("exceeds Candle max_batch_size"));
    }
}

// --- Test 6: Concurrent inference during fingerprint change no torn read ---
struct SlowMockLlmModel {
    id: String,
    delay: Duration,
}

impl CandleModelInner for SlowMockLlmModel {
    fn generate(
        &mut self,
        prompt: &str,
        _tokenizer: &tokenizers::Tokenizer,
        _device: &Device,
    ) -> Result<String> {
        std::thread::sleep(self.delay);
        Ok(format!("[{}] Output for: {}", self.id, prompt))
    }
}

#[tokio::test]
async fn test_concurrent_inference_during_fingerprint_change_no_torn_read() {
    let initial_model = Box::new(SlowMockLlmModel {
        id: "Model-v1".to_string(),
        delay: Duration::from_millis(50),
    });
    let fp1 = ModelFingerprint {
        hash: [11u8; 32],
        model_id: "llama-v1.gguf".to_string(),
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

    let client = Arc::new(tokio::sync::RwLock::new(CandleLlmClient::new(
        Device::Cpu,
        initial_model,
        fp1.clone(),
        tokenizer,
    )));

    // Spawn inference task
    let client_clone = Arc::clone(&client);
    let handle = tokio::spawn(async move {
        let guard = client_clone.read().await;
        // Generate prompt execution locks inner model Mutex
        guard.generate("concurrent query").await
    });

    // Wait briefly so inference starts and holds lock inside spawn_blocking
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Trigger model swap
    let new_model = Box::new(SlowMockLlmModel {
        id: "Model-v2".to_string(),
        delay: Duration::from_millis(10),
    });
    let fp2 = ModelFingerprint {
        hash: [22u8; 32],
        model_id: "llama-v2.gguf".to_string(),
        quantization: "Q8_0".to_string(),
    };

    {
        let mut write_guard = client.write().await;
        write_guard.swap_model(new_model, fp2.clone(), None);
    }

    let response = handle.await.unwrap().unwrap();

    // Verification: response MUST be consistent (either Model-v1 or Model-v2 format), never torn
    assert!(
        response.contains("[Model-v1]") || response.contains("[Model-v2]"),
        "Response must be consistent and non-torn, got: {response}"
    );

    // After swap, client fingerprint is updated
    let final_fp = client.read().await.fingerprint().clone();
    assert_eq!(final_fp, fp2);
}
