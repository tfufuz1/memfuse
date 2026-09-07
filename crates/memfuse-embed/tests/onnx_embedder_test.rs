#![cfg(feature = "onnx")]

use memfuse_core::{EmbeddingError, EmbeddingProvider};
use memfuse_embed::{OnnxEmbedder, TextEmbedderConfig, SESSION_LOAD_COUNT};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;

fn fixture_path() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests");
    p.push("fixtures");
    p.push("model.onnx");
    p
}

static TEST_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[tokio::test]
async fn test_onnx_embedder_fixture_inference() {
    let _guard = TEST_MUTEX.lock().unwrap();
    let model_file = fixture_path();
    assert!(
        model_file.exists(),
        "Fixture model.onnx missing. Run download_test_model.sh or ensure fixtures exist."
    );

    let embedder = OnnxEmbedder::from_path(&model_file)
        .expect("Failed to create OnnxEmbedder from model path fixture");

    assert_eq!(embedder.provider_name(), "onnx");

    // 1. Valid input test - no panic and correct non-empty output shape
    let text = "Hello world memfuse ONNX test";
    let embedding = embedder
        .embed(text)
        .await
        .expect("Embedding inference failed for valid input");

    assert!(!embedding.is_empty(), "Output vector must not be empty");
    assert!(
        embedding.iter().all(|&x| x.is_finite()),
        "Output vector elements must be finite numbers"
    );
}

#[tokio::test]
async fn test_onnx_embedder_input_too_long() {
    let _guard = TEST_MUTEX.lock().unwrap();
    let model_file = fixture_path();
    assert!(model_file.exists(), "Fixture model.onnx missing.");

    let config = TextEmbedderConfig {
        max_sequence_length: 5,
        ..TextEmbedderConfig::default()
    };

    let dir = model_file.parent().unwrap();
    let embedder = OnnxEmbedder::load_with_config(dir, config)
        .expect("Failed to create OnnxEmbedder with small max_sequence_length");

    let long_text = "word ".repeat(50);
    let res = embedder.embed(&long_text).await;

    assert!(
        res.is_err(),
        "Expected error when input token count exceeds max_sequence_length"
    );
    match res.unwrap_err() {
        EmbeddingError::InputTooLong { len, max } => {
            assert!(len > max);
            assert_eq!(max, 5);
        }
        other => panic!("Expected EmbeddingError::InputTooLong, got {:?}", other),
    }
}

#[tokio::test]
async fn test_single_session_load_across_multiple_embed_calls() {
    let _guard = TEST_MUTEX.lock().unwrap();
    let model_file = fixture_path();
    assert!(model_file.exists(), "Fixture model.onnx missing.");

    let initial_loads = SESSION_LOAD_COUNT.load(Ordering::SeqCst);

    let embedder = OnnxEmbedder::from_path(&model_file)
        .expect("Failed to create OnnxEmbedder from model path fixture");

    let loads_after_creation = SESSION_LOAD_COUNT.load(Ordering::SeqCst);
    assert_eq!(
        loads_after_creation,
        initial_loads + 1,
        "Creating TextEmbedder must perform exactly one session load"
    );

    // Perform multiple embed_async calls
    let res1 = embedder.embed_async("First test text").await;
    assert!(res1.is_ok());

    let res2 = embedder.embed_async("Second test text").await;
    assert!(res2.is_ok());

    let res3 = embedder.embed_async("Third test text").await;
    assert!(res3.is_ok());

    let loads_after_embeds = SESSION_LOAD_COUNT.load(Ordering::SeqCst);
    assert_eq!(
        loads_after_embeds, loads_after_creation,
        "embed_async calls must NOT trigger additional session loads"
    );
}

#[tokio::test]
async fn test_concurrent_embed_async_mutex_contention() {
    let _guard = TEST_MUTEX.lock().unwrap();
    let model_file = fixture_path();
    assert!(model_file.exists(), "Fixture model.onnx missing.");

    let embedder = Arc::new(
        OnnxEmbedder::from_path(&model_file)
            .expect("Failed to create OnnxEmbedder from model path fixture"),
    );

    let mut handles = Vec::new();

    // Spawn 20 concurrent tasks against the same shared embedder instance
    for i in 0..20 {
        let embedder_clone = Arc::clone(&embedder);
        handles.push(tokio::spawn(async move {
            let text = format!("Concurrent embed request number {i}");
            embedder_clone.embed_async(&text).await
        }));
    }

    for handle in handles {
        let res = handle.await.expect("Task panicked or failed to join");
        assert!(
            res.is_ok(),
            "Concurrent embed_async call failed: {:?}",
            res.err()
        );
        let vector = res.unwrap(); // unwrap
        assert!(!vector.is_empty());
        assert!(vector.iter().all(|&x| x.is_finite()));
    }
}
