//! # MemFuse Quickstart
//!
//! Demonstrates the core workflow: open a database, insert documents
//! with embeddings and metadata, then perform semantic search.
//!
//! Run with: `cargo run --example quickstart`

use memfuse_db::{MemFuse, MemFuseConfig};

#[tokio::main]
async fn main() -> memfuse_core::Result<()> {
    // 1. Open (or create) a database with 4-dimensional vectors
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = MemFuse::open_with_config("./example_data", config).await?;

    // 2. Insert documents with embeddings and metadata
    db.insert(
        "rust-lang",
        &[1.0, 0.0, 0.0, 0.0],
        Some(serde_json::json!({"topic": "programming", "text": "Rust is a systems programming language"})),
    )
    .await?;

    db.insert(
        "python-lang",
        &[0.9, 0.1, 0.0, 0.0],
        Some(serde_json::json!({"topic": "programming", "text": "Python is great for AI and data science"})),
    )
    .await?;

    db.insert(
        "cooking-101",
        &[0.0, 0.0, 1.0, 0.0],
        Some(serde_json::json!({"topic": "cooking", "text": "How to make perfect pasta"})),
    )
    .await?;

    // 3. Semantic search — find the 2 most similar documents
    let query = [0.95, 0.05, 0.0, 0.0];
    let results = db.search(&query, 2).await?;

    println!("=== Semantic Search Results ===");
    for result in &results {
        println!("  {} (score: {:.4})", result.id, result.score);
        if let Some(meta) = &result.metadata {
            println!("    metadata: {}", meta);
        }
    }

    // 4. Retrieve a specific document by key
    if let Some(doc) = db.get("rust-lang").await? {
        println!("\n=== Direct Lookup ===");
        println!("  Found: {} {:?}", doc.id, doc.metadata);
    }

    // 5. Clean up
    db.close().await?;

    // Remove example data directory
    let _ = tokio::fs::remove_dir_all("./example_data").await;

    println!("\n✅ Quickstart complete!");
    Ok(())
}
