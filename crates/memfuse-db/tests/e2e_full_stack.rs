use memfuse_db::{MemFuse, MemFuseConfig, DistanceMetric, json};
use tempfile::TempDir;

#[tokio::test]
async fn test_full_stack_e2e_flow() {
    let tmp = TempDir::new().unwrap();
    let config = MemFuseConfig {
        dimension: 3,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };

    // 1. Open Database
    let db = MemFuse::open_with_config(tmp.path(), config).await.unwrap();

    // 2. Create and work with multiple collections (Isolation)
    let col_tech = db.collection("tech").await.unwrap();
    let col_bio = db.collection("bio").await.unwrap();

    // 3. Insert documents into "tech" collection
    col_tech.insert(
        "rust-doc",
        &[1.0, 0.0, 0.0],
        Some(json!({"text": "Rust is a systems programming language focused on safety and speed.", "tags": ["programming", "safety"]}))
    ).await.unwrap();

    col_tech.insert(
        "python-doc",
        &[0.9, 0.1, 0.0],
        Some(json!({"content": "Python is a high-level programming language known for its readability.", "tags": ["programming", "easy"]}))
    ).await.unwrap();

    // 4. Insert documents into "bio" collection
    col_bio.insert(
        "human-doc",
        &[0.0, 0.0, 1.0],
        Some(json!({"text": "The human body is complex.", "category": "biology"}))
    ).await.unwrap();

    // 5. Verify Isolation
    assert_eq!(col_tech.len().await, 2);
    assert_eq!(col_bio.len().await, 1);

    assert!(col_tech.get("human-doc").await.unwrap().is_none());
    assert!(col_bio.get("rust-doc").await.unwrap().is_none());

    // 6. Hybrid Search in "tech" collection
    // Vector search for something similar to [1.0, 0.0, 0.0] + Text search for "readability"
    let results = col_tech.hybrid_search("readability", &[1.0, 0.0, 0.0], 5).await.unwrap();

    assert!(!results.is_empty());
    // Both rust-doc (vector match) and python-doc (text match) should be present
    let ids: Vec<String> = results.iter().map(|r| r.id.clone()).collect();
    assert!(ids.contains(&"rust-doc".to_string()));
    assert!(ids.contains(&"python-doc".to_string()));

    // Python-doc should have a good score due to "readability" text match
    assert_eq!(results[0].id, "python-doc"); // BM25 usually ranks highly if keywords match exactly

    // 7. Update document
    col_tech.update(
        "rust-doc",
        &[1.0, 0.0, 0.0],
        Some(json!({"text": "Rust is fast and safe.", "tags": ["programming", "performance"]}))
    ).await.unwrap();

    let updated = col_tech.get("rust-doc").await.unwrap().unwrap();
    assert_eq!(updated.metadata.unwrap()["tags"][1], "performance");

    // 8. Relationships
    col_tech.relate("rust-doc", "python-doc", "alternative_to").await.unwrap();

    let relations = col_tech.scan_prefix("__rel:rust-doc:alternative_to:").await.unwrap();
    assert_eq!(relations.len(), 1);
    assert_eq!(relations[0].1["to"], "python-doc");

    // 9. Delete and verify
    col_tech.delete("python-doc").await.unwrap();
    assert_eq!(col_tech.len().await, 1);
    assert!(col_tech.get("python-doc").await.unwrap().is_none());

    // Search should no longer find it
    let search_after_delete = col_tech.search(&[0.9, 0.1, 0.0], 5).await.unwrap();
    assert_eq!(search_after_delete.len(), 1);
    assert_eq!(search_after_delete[0].id, "rust-doc");
}
