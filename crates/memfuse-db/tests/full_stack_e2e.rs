//! End-to-End integration tests for the full MemFuse stack.
// ANCHOR:INTEGRATION:E2E-001 STATUS:DONE AGENT:12 DATE:2026-05-18

use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use tempfile::TempDir;

#[tokio::test]
async fn test_full_stack_document_lifecycle() {
    let tmp = TempDir::new().expect("Failed to create temp dir"); // expect #[cfg(test)] // expect #[cfg(test)]
    let config = MemFuseConfig {
        dimension: 3,
        max_elements: 100,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };

    let db = MemFuse::open_with_config(tmp.path(), config)
        .await
        .expect("Failed to open DB"); // expect #[cfg(test)]
    let col = db
        .collection("e2e-test")
        .await
        .expect("Failed to get collection"); // expect #[cfg(test)]

    // 1. Insert documents with embeddings and text metadata
    col.insert(
        "doc1",
        &[1.0, 0.0, 0.0],
        Some(json!({"text": "Rust is a fast and safe language", "tags": ["systems"]})),
    )
    .await
    .expect("Insert doc1 failed"); // expect #[cfg(test)]

    col.insert(
        "doc2",
        &[0.0, 1.0, 0.0],
        Some(json!({"content": "Python is easy to learn and versatile", "tags": ["scripting"]})),
    )
    .await
    .expect("Insert doc2 failed"); // expect #[cfg(test)]

    col.insert(
        "doc3",
        &[0.0, 0.0, 1.0],
        Some(json!({"text": "Learning new technologies is fun", "tags": ["general"]})),
    )
    .await
    .expect("Insert doc3 failed"); // expect #[cfg(test)]

    // 2. Hybrid Search (Vector + Text)
    // Querying for "Rust" should rank doc1 first due to text match and vector match (if vector is close)
    let results = col
        .hybrid_search("Rust", &[1.0, 0.1, 0.0], 2)
        .await
        .expect("Hybrid search failed"); // expect #[cfg(test)]
    assert!(!results.is_empty(), "Search results should not be empty");
    assert_eq!(results[0].id, "doc1");
    assert!(
        results[0].metadata.as_ref().unwrap()["tags"] // unwrap #[cfg(test)]
            .as_array()
            .unwrap() // unwrap #[cfg(test)]
            .contains(&json!("systems"))
    );

    // 3. Bidirectional Relationships
    col.relate("doc1", "doc2", "cousin")
        .await
        .expect("Relate failed"); // expect #[cfg(test)]

    // Check forward relationship
    let relations = col
        .scan_prefix("__rel:doc1:cousin:")
        .await
        .expect("Scan forward relations failed"); // expect #[cfg(test)]
    assert_eq!(relations.len(), 1);
    assert_eq!(relations[0].1["to"], "doc2");

    // 4. Update Document
    col.update(
        "doc1",
        &[0.9, 0.1, 0.0],
        Some(json!({"text": "Rust is incredibly fast", "tags": ["systems", "performance"]})),
    )
    .await
    .expect("Update failed"); // expect #[cfg(test)]

    let doc = col
        .get("doc1")
        .await
        .expect("Get failed") // expect #[cfg(test)]
        .expect("Doc1 not found"); // expect #[cfg(test)]
    assert_eq!(
        doc.metadata.unwrap()["tags"], // unwrap #[cfg(test)]
        json!(["systems", "performance"])
    );

    // 5. Search after update
    let results_after = col
        .hybrid_search("incredibly", &[0.9, 0.1, 0.0], 1)
        .await
        .expect("Search after update failed"); // expect #[cfg(test)]
    assert_eq!(results_after[0].id, "doc1");

    // 6. List collections
    let collections = db
        .list_collections()
        .await
        .expect("List collections failed"); // expect #[cfg(test)]
    assert!(collections.contains(&"e2e-test".to_string()));
    assert!(collections.contains(&"default".to_string()));

    // 7. Delete Document
    col.delete("doc1").await.expect("Delete failed"); // expect #[cfg(test)]
    let deleted_doc = col.get("doc1").await.expect("Get after delete failed"); // expect #[cfg(test)]
    assert!(deleted_doc.is_none());

    // 8. Stats check
    let stats = db.stats().await.expect("Stats failed"); // expect #[cfg(test)]
    assert!(stats.storage_stats.memtable_size_bytes > 0);
}
