//! E2E Lifecycle Integration Test for MemFuse.
use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use tempfile::TempDir;

#[tokio::test]
async fn test_e2e_lifecycle() {
    let tmp = TempDir::new().expect("temp dir");
    let path = tmp.path().to_path_buf();

    let config = MemFuseConfig {
        dimension: 4,
        max_elements: 1000,
        distance_metric: DistanceMetric::Cosine,
    };

    // 1. Open Database
    let db = MemFuse::open_with_config(&path, config.clone())
        .await
        .expect("open db");

    // 2. Create and use collections
    let col_memories = db.collection("memories").await.expect("col memories");
    let col_tasks = db.collection("tasks").await.expect("col tasks");

    // 3. Insert Documents (Hybrid: Vector + Text)
    col_memories
        .insert(
            "mem-1",
            &[1.0, 0.0, 0.0, 0.0],
            Some(json!({"text": "The quick brown fox jumps over the lazy dog", "importance": 0.8})),
        )
        .await
        .expect("insert mem-1");

    col_memories.insert(
        "mem-2",
        &[0.0, 1.0, 0.0, 0.0],
        Some(json!({"text": "Rust is a systems programming language focused on safety", "importance": 0.9}))
    ).await.expect("insert mem-2");

    col_tasks
        .insert(
            "task-1",
            &[0.0, 0.0, 1.0, 0.0],
            Some(
                json!({"content": "Implement hybrid search integration tests", "priority": "high"}),
            ),
        )
        .await
        .expect("insert task-1");

    // 4. Vector Search
    let results = col_memories
        .search(&[0.9, 0.1, 0.0, 0.0], 1)
        .await
        .expect("search memories");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "mem-1");

    // 5. Text/Hybrid Search
    let hybrid_results = col_memories
        .hybrid_search("safety", &[0.0, 0.9, 0.1, 0.0], 2)
        .await
        .expect("hybrid search");
    // mem-2 matches "safety" AND is near the vector.
    // mem-1 has no "safety" and vector is orthogonal.
    assert!(!hybrid_results.is_empty());
    assert_eq!(hybrid_results[0].id, "mem-2");

    // 6. Update
    col_memories
        .update(
            "mem-1",
            &[1.0, 0.0, 0.0, 0.0],
            Some(json!({"text": "The quick brown fox is very fast", "importance": 0.95})),
        )
        .await
        .expect("update mem-1");

    let updated_doc = col_memories
        .get("mem-1")
        .await
        .expect("get mem-1")
        .expect("exists");
    assert_eq!(updated_doc.metadata.unwrap()["importance"], 0.95);

    // 7. Isolation check
    let task_search = col_tasks
        .search(&[1.0, 0.0, 0.0, 0.0], 10)
        .await
        .expect("search tasks");
    assert!(task_search
        .iter()
        .all(|r| r.id != "mem-1" && r.id != "mem-2"));

    // 8. Delete
    col_memories.delete("mem-2").await.expect("delete mem-2");
    let deleted_doc = col_memories.get("mem-2").await.expect("get deleted");
    assert!(deleted_doc.is_none());

    // 9. Persistence (Reload)
    drop(col_memories);
    drop(col_tasks);
    drop(db);

    let db_reopened = MemFuse::open_with_config(&path, config)
        .await
        .expect("reopen db");
    let col_reopened = db_reopened
        .collection("memories")
        .await
        .expect("re-get collection");

    assert_eq!(col_reopened.len().await, 1);
    let doc = col_reopened
        .get("mem-1")
        .await
        .expect("get mem-1 after reload")
        .expect("exists");
    assert_eq!(doc.id, "mem-1");

    let search_reload = col_reopened
        .search(&[1.0, 0.0, 0.0, 0.0], 1)
        .await
        .expect("search after reload");
    assert_eq!(search_reload[0].id, "mem-1");
}
