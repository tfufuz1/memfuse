#![cfg(feature = "onnx")]

use memfuse_core::{EmbeddingError, EmbeddingProvider};
use memfuse_embed::{OnnxEmbedder, TextEmbedderConfig};
use std::path::PathBuf;

fn fixture_path() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests");
    p.push("fixtures");
    p.push("model.onnx");
    p
}

#[tokio::test]
async fn test_onnx_embedder_fixture_inference() {
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
