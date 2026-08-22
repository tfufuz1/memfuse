//! # Hybrid Search — BM25 + Vector Fusion
//!
//! Demonstrates MemFuse's 4-Signal Fusion: combining semantic vector search
//! with keyword-based BM25 scoring via Reciprocal Rank Fusion (RRF).
//!
//! Run with: `cargo run --example hybrid_search`

use memfuse_db::{MemFuse, MemFuseConfig};

#[tokio::main]
async fn main() -> memfuse_core::Result<()> {
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = MemFuse::open_with_config("./hybrid_data", config).await?;

    // Insert documents with both embeddings AND text metadata.
    // MemFuse automatically indexes the "text" field for BM25 search.
    let docs = vec![
        (
            "doc-rust".to_string(),
            vec![1.0, 0.0, 0.0, 0.0],
            Some(serde_json::json!({
                "text": "Rust provides memory safety without garbage collection through ownership",
                "category": "systems"
            })),
        ),
        (
            "doc-python".to_string(),
            vec![0.8, 0.2, 0.0, 0.0],
            Some(serde_json::json!({
                "text": "Python excels at rapid prototyping with its dynamic type system",
                "category": "scripting"
            })),
        ),
        (
            "doc-gc".to_string(),
            vec![0.5, 0.5, 0.0, 0.0],
            Some(serde_json::json!({
                "text": "Garbage collection strategies in modern programming languages",
                "category": "systems"
            })),
        ),
        (
            "doc-ml".to_string(),
            vec![0.0, 1.0, 0.0, 0.0],
            Some(serde_json::json!({
                "text": "Machine learning frameworks for training neural networks",
                "category": "ai"
            })),
        ),
    ];

    // Batch insert for efficiency
    db.insert_many(&docs).await?;

    // --- Hybrid Search ---
    // Combines:
    //   • BM25 keyword matching on "memory safety garbage collection"
    //   • Vector similarity to the query embedding [0.9, 0.1, 0.0, 0.0]
    // Results are fused via Reciprocal Rank Fusion (RRF) — no manual tuning needed.
    let text_query = "memory safety garbage collection";
    let vector_query = [0.9, 0.1, 0.0, 0.0];

    let results = db.hybrid_search(text_query, &vector_query, 3, None).await?;

    println!("=== Hybrid Search: '{}' ===", text_query);
    for (i, result) in results.iter().enumerate() {
        println!(
            "  {}. {} (fused score: {:.4})",
            i + 1,
            result.id,
            result.score
        );
        if let Some(meta) = &result.metadata {
            if let Some(text) = meta.get("text").and_then(|v| v.as_str()) {
                println!("     text: \"{}\"", text);
            }
        }
    }

    // --- Compare: Pure vector search (no text) ---
    let vector_only = db.search(&vector_query, 3).await?;

    println!("\n=== Vector-Only Search (same embedding) ===");
    for (i, result) in vector_only.iter().enumerate() {
        println!("  {}. {} (score: {:.4})", i + 1, result.id, result.score);
    }

    db.close().await?;
    let _ = tokio::fs::remove_dir_all("./hybrid_data").await;

    println!("\n✅ Hybrid search demo complete!");
    Ok(())
}
