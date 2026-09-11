// FILE-CONTEXT
// STAND: 2026-09-11
// ZWECK: Integration tests for Candle real forward-pass GGUF LLM and Bert embedding inference.
// INVARIANTEN: Ignored by default unless binary model fixtures are present; executable with real-inference-tests flag / --include-ignored.

use memfuse_candle::{CandleEmbedClient, CandleLlmClient, CandleQuantization};
use memfuse_core::traits::{EmbeddingProvider, LlmTextGenerator};
use std::path::Path;

#[tokio::test]
#[ignore]
async fn test_real_llm_inference() {
    let model_dir = std::env::var("CANDLE_LLM_MODEL_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| Path::new("tests/fixtures/tiny_llm").to_path_buf());

    if !model_dir.exists() {
        eprintln!(
            "Skipping test_real_llm_inference: model directory {} does not exist",
            model_dir.display()
        );
        return;
    }

    let client = CandleLlmClient::from_dir(&model_dir, CandleQuantization::Q4KM)
        .expect("Failed to initialize CandleLlmClient from model dir");

    let prompt = "Explain quantum computing in one sentence.";
    let response = client
        .generate(prompt)
        .await
        .expect("LLM generation failed");

    assert!(
        !response.contains("[Candle] Response for prompt:"),
        "LLM output must not return placeholder stub string: {response}"
    );
    assert!(
        !response.contains("[MockCandle] Response for prompt:"),
        "LLM output must not return mock stub string when real model directory is supplied: {response}"
    );
    assert!(
        !response.trim().is_empty(),
        "Generated response should not be empty"
    );
}

#[tokio::test]
#[ignore]
async fn test_real_embedding_inference() {
    let model_dir = std::env::var("CANDLE_EMBED_MODEL_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| Path::new("tests/fixtures/tiny_embed").to_path_buf());

    if !model_dir.exists() {
        eprintln!(
            "Skipping test_real_embedding_inference: model directory {} does not exist",
            model_dir.display()
        );
        return;
    }

    let client = CandleEmbedClient::from_dir(&model_dir, CandleQuantization::Q8_0)
        .expect("Failed to initialize CandleEmbedClient from model dir");

    let text_a = "The quick brown fox jumps over the lazy dog";
    let text_b = "Memfuse cognitive memory engine for high performance LLM agents";

    let emb_a = client.embed(text_a).await.expect("Embedding text_a failed");
    let emb_b = client.embed(text_b).await.expect("Embedding text_b failed");

    assert_eq!(emb_a.len(), client.embedding_dim());
    assert_eq!(emb_b.len(), client.embedding_dim());

    // Assert non-constant non-zero vectors
    assert!(
        emb_a.iter().any(|&x| x != 0.0f32),
        "emb_a must not be all zeros"
    );
    assert!(
        emb_b.iter().any(|&x| x != 0.0f32),
        "emb_b must not be all zeros"
    );

    // Assert L2 norm is 1.0 (unit vector for Cosine similarity)
    let norm_a: f32 = emb_a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = emb_b.iter().map(|x| x * x).sum::<f32>().sqrt();
    assert!(
        (norm_a - 1.0f32).abs() < 1e-4,
        "emb_a must be L2 normalized: {norm_a}"
    );
    assert!(
        (norm_b - 1.0f32).abs() < 1e-4,
        "emb_b must be L2 normalized: {norm_b}"
    );

    // Assert different texts produce non-identical embedding vectors
    assert_ne!(
        emb_a, emb_b,
        "Different input texts must yield distinct embedding vectors"
    );
}
