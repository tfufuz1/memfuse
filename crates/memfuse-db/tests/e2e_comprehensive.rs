//! Comprehensive E2E tests for MemFuse DB.
// AGENT:12 DATE:2026-05-18 STATUS:READY

use memfuse_db::{MemFuse, MemFuseConfig, DistanceMetric};
use serde_json::json;
use tempfile::TempDir;

#[tokio::test]
async fn test_full_stack_e2e() -> memfuse_core::Result<()> {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 4,
        max_elements: 100,
        distance_metric: DistanceMetric::Cosine,
    };

    // 1. MemFuse::open()
    let db = MemFuse::open_with_config(tmp.path(), config).await?;

    // 2. Insert documents with embeddings + metadata
    // Doc 1: Vector match [1, 0, 0, 0], Text: "rust programming"
    db.insert(
        "doc-1",
        &[1.0, 0.0, 0.0, 0.0],
        Some(json!({"text": "rust programming language", "tags": ["coding"]})),
    ).await?;

    // Doc 2: Vector match [0, 1, 0, 0], Text: "python scripting"
    db.insert(
        "doc-2",
        &[0.0, 1.0, 0.0, 0.0],
        Some(json!({"text": "python scripting", "tags": ["scripting"]})),
    ).await?;

    // Doc 3: Vector match [0.9, 0.1, 0, 0], Text: "java software"
    db.insert(
        "doc-3",
        &[0.9, 0.1, 0.0, 0.0],
        Some(json!({"content": "java software engineering", "tags": ["enterprise"]})),
    ).await?;

    // 3. Hybrid Search (Vector + Text)
    // Query for "rust" and vector [1, 0, 0, 0]
    let results = db.hybrid_search("rust", &[1.0, 0.0, 0.0, 0.0], 2).await?;

    // 4. Verify results (Score, Metadata, Ordering)
    assert!(!results.is_empty());
    assert_eq!(results[0].id, "doc-1");
    assert!(results[0].score > 0.0);
    assert_eq!(results[0].metadata.as_ref().unwrap()["text"], "rust programming language");

    // 5. Update + Re-Search
    // Update doc-2 to be about rust too
    db.update(
        "doc-2",
        &[1.0, 0.0, 0.0, 0.0],
        Some(json!({"text": "rust is also great", "tags": ["rust"]})),
    ).await?;

    let results_after_update = db.hybrid_search("rust", &[1.0, 0.0, 0.0, 0.0], 5).await?;
    let ids: Vec<String> = results_after_update.iter().map(|r| r.id.clone()).collect();
    assert!(ids.contains(&"doc-1".to_string()));
    assert!(ids.contains(&"doc-2".to_string()));

    // 6. Delete + Verify Gone
    db.delete("doc-1").await?;
    let doc1 = db.get("doc-1").await?;
    assert!(doc1.is_none());

    let results_after_delete = db.hybrid_search("rust", &[1.0, 0.0, 0.0, 0.0], 5).await?;
    let ids_after_delete: Vec<String> = results_after_delete.iter().map(|r| r.id.clone()).collect();
    assert!(!ids_after_delete.contains(&"doc-1".to_string()));

    // 7. Collection Isolation
    let col_other = db.collection("other").await?;
    col_other.insert("doc-other", &[0.0, 0.0, 1.0, 1.0], Some(json!({"text": "isolated"}))).await?;

    let get_from_default = db.get("doc-other").await?;
    assert!(get_from_default.is_none());

    let get_from_other = col_other.get("doc-other").await?;
    assert!(get_from_other.is_some());

    Ok(())
}
