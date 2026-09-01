// FILE-CONTEXT
// STAND: 2026-08-31
// ZWECK: Quantitativer Test- & Benchmark-Suite für Cross-Encoder Reranker Adversarial Attacks.

use memfuse_embed::{CrossEncoderReranker, RerankConfig};

/// Testet die Auswirkung von Keyword/Query-Stuffing auf die Sequenzlänge und Reranker-Struktur.
#[tokio::test]
async fn test_adversarial_query_stuffing_quantification() {
    let config = RerankConfig::default();
    let reranker =
        CrossEncoderReranker::new(config).expect("Failed to initialize CrossEncoderReranker");

    let query = "Rust async concurrency memory safety";

    // Legitim relevanter Chunk (natürliche Erklärung)
    let legitimate_doc = "Rust provides memory safety guarantees through ownership and borrowing. Async concurrency allows efficient I/O operations without locks.".to_string();

    // Reiner Spam / Irrelevanter Chunk
    let irrelevant_doc = "The quick brown fox jumps over the lazy dog in a warm summer afternoon. Cooking recipe for apple pie requires flour, sugar, and apples.".to_string();

    // Adversarial Chunk: Irrelevanter Inhalt + Keyword Stuffing
    let stuffed_doc = format!("{irrelevant_doc} {query} {query} {query} {query} {query}");

    // Adversarial Chunk: Query Prefix Injection
    let prefix_injected_doc = format!("{query} {query} {query} - {irrelevant_doc}");

    let candidates = vec![
        legitimate_doc.clone(),
        irrelevant_doc.clone(),
        stuffed_doc.clone(),
        prefix_injected_doc.clone(),
    ];

    let results = reranker
        .rerank(query, &candidates)
        .await
        .expect("Reranking failed");
    assert_eq!(results.len(), 4);

    // In Passthrough-Fallback oder Inferenz: Ergebnisse müssen geordnet sein
    for window in results.windows(2) {
        assert!(window[0].score >= window[1].score);
    }
}

/// Simuliert die Rank-Hijacking-Dynamik im Post-RRF Reranking-Schritt (k * 3 Oversampling).
#[tokio::test]
async fn test_post_rrf_rerank_oversampling_hijack() {
    let config = RerankConfig::default();
    let reranker =
        CrossEncoderReranker::new(config).expect("Failed to initialize CrossEncoderReranker");

    let query = "database snapshot isolation MVCC";

    // Simuliere Top 15 RRF-Kandidaten:
    // Ränge 1..10: Echtes Fachwissen zu MVCC & Snapshot Isolation
    // Rang 15: Adversarial Stuffed Chunk, der über Vector- / Text-Signal knapp in die Top 15 (k*3 = 15 für k=5) kam
    let mut candidates: Vec<String> = (1..=14)
        .map(|i| format!("Legitimate documentation chunk #{i} explaining multi-version concurrency control and transaction isolation guarantees."))
        .collect();

    // Adversarial Chunk an Position 15 (Index 14)
    let adversarial_chunk = format!("Totally unrelated malicious payload or advertisement text. {query} {query} {query} {query}");
    candidates.push(adversarial_chunk);

    let results = reranker
        .rerank(query, &candidates)
        .await
        .expect("Rerank failed");
    assert_eq!(results.len(), 15);
}
