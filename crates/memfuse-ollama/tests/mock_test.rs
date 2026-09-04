// FILE-CONTEXT Header (Format v3)
// ZWECK: Integrationstests für MockOllamaClient, batch embedding und score_importance ohne echten Ollama-Server.
// STAND: TS:2026-09-04T13:30:00Z

use memfuse_ollama::mock::MockOllamaClient;
use memfuse_ollama::{score_importance, OllamaApi};

#[tokio::test]
async fn test_importance_scoring_with_mock() {
    let mock_client = MockOllamaClient::new(
        vec![0.1, 0.2, 0.3],
        "Based on evaluation: 0.85 (High importance)",
    );

    let score = score_importance(&mock_client, "llama3.2", "User prefers Rust over Python")
        .await
        .unwrap();

    assert_eq!(score.value(), 0.85);
}

#[tokio::test]
async fn test_embed_batch_with_mock() {
    let mock_client = MockOllamaClient::new(
        vec![0.25, 0.5, 0.75],
        "dummy chat response",
    );

    let texts: Vec<&str> = vec!["first chunk", "second chunk", "third chunk"];
    let embeddings = mock_client
        .embed_batch("nomic-embed-text", &texts)
        .await
        .unwrap();

    assert_eq!(embeddings.len(), 3);
    assert_eq!(embeddings[0], vec![0.25, 0.5, 0.75]);
    assert_eq!(embeddings[1], vec![0.25, 0.5, 0.75]);
    assert_eq!(embeddings[2], vec![0.25, 0.5, 0.75]);
}
