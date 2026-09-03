//! # Collections — Multi-Namespace Workflow
//!
//! Demonstrates MemFuse's Collection system for logically isolated
//! namespaces within a single database. Each collection has its own
//! HNSW index and BM25 index while sharing the underlying storage.
//!
//! Run with: `cargo run --example collections`

#![allow(deprecated)]

use memfuse_db::{MemFuse, MemFuseConfig};

#[tokio::main]
async fn main() -> memfuse_core::Result<()> {
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = MemFuse::open_with_config("./collections_data", config).await?;

    // --- Create two isolated collections ---
    let agents = db.collection("agents").await?;
    let tools = db.collection("tools").await?;

    // Insert into "agents" collection
    agents
        .insert(
            "planner",
            &[1.0, 0.0, 0.0, 0.0],
            Some(serde_json::json!({"role": "planning", "text": "Decomposes complex tasks into subtasks"})),
        )
        .await?;

    agents
        .insert(
            "coder",
            &[0.0, 1.0, 0.0, 0.0],
            Some(serde_json::json!({"role": "execution", "text": "Writes and debugs code"})),
        )
        .await?;

    // Insert into "tools" collection
    tools
        .insert(
            "web-search",
            &[0.5, 0.5, 0.0, 0.0],
            Some(serde_json::json!({"type": "retrieval", "text": "Searches the web for information"})),
        )
        .await?;

    tools
        .insert(
            "code-exec",
            &[0.0, 0.9, 0.1, 0.0],
            Some(serde_json::json!({"type": "execution", "text": "Executes code in a sandbox"})),
        )
        .await?;

    // --- Search within each collection (isolated) ---
    let query = [0.1, 0.9, 0.0, 0.0]; // "coding-like" vector

    let agent_results = agents.search(&query, 2).await?;
    println!("=== Agents Collection ===");
    for r in &agent_results {
        println!("  {} (score: {:.4})", r.id, r.score);
    }

    let tool_results = tools.search(&query, 2).await?;
    println!("\n=== Tools Collection ===");
    for r in &tool_results {
        println!("  {} (score: {:.4})", r.id, r.score);
    }

    // --- List all collections ---
    let all = db.list_collections().await?;
    println!("\n=== All Collections ===");
    for name in &all {
        println!("  • {}", name);
    }

    // --- Snapshot isolation: read at a point in time ---
    let snapshot = db.create_snapshot().await?;
    println!("\n=== Snapshot created at seq={} ===", snapshot);

    // Insert more data AFTER the snapshot
    agents
        .insert(
            "reviewer",
            &[0.5, 0.0, 0.5, 0.0],
            Some(serde_json::json!({"role": "review", "text": "Reviews code for correctness"})),
        )
        .await?;

    // Reading at snapshot does NOT see the new data
    let at_snapshot = agents.get_at_snapshot("reviewer", snapshot).await?;
    println!(
        "  'reviewer' visible at snapshot? {}",
        if at_snapshot.is_some() {
            "yes"
        } else {
            "no (correct — inserted after snapshot)"
        }
    );

    // Reading at current DOES see it
    let current = agents.get("reviewer").await?;
    println!(
        "  'reviewer' visible at current?  {}",
        if current.is_some() { "yes" } else { "no" }
    );

    // --- Drop a collection ---
    db.drop_collection("tools").await?;
    let remaining = db.list_collections().await?;
    println!("\n=== After dropping 'tools' ===");
    for name in &remaining {
        println!("  • {}", name);
    }

    db.close().await?;
    let _ = tokio::fs::remove_dir_all("./collections_data").await;

    println!("\n✅ Collections demo complete!");
    Ok(())
}
