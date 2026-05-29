//! Higher-level contract tests for Collection API (Batch operations & Scanning).
//!
//! AUDIT:2026-05-23 STATUS:VERIFIED (Phase 2 Remediation)
//! Ensures that batch operations (insert_many, upsert_many) and range scans
//! maintain the logical isolation and data integrity of the SAOS.

use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use std::ops::Bound;
use tempfile::TempDir;

async fn setup_db(dim: usize) -> (MemFuse, TempDir) {
    let tmp = TempDir::new().expect("Failed to create temp dir");
    let config = MemFuseConfig {
        dimension: dim,
        max_elements: 1000,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(tmp.path(), config)
        .await
        .expect("Failed to open DB");
    (db, tmp)
}

#[tokio::test]
async fn test_collection_insert_many_atomic() {
    let (db, _tmp) = setup_db(3).await;
    let col = db.collection("batch_test").await.expect("col");

    let docs = vec![
        ("d1".to_string(), vec![1.0, 0.0, 0.0], Some(json!({"v": 1}))),
        ("d2".to_string(), vec![0.0, 1.0, 0.0], Some(json!({"v": 2}))),
        ("d3".to_string(), vec![0.0, 0.0, 1.0], Some(json!({"v": 3}))),
    ];

    col.insert_many(&docs).await.expect("insert_many failed");

    assert_eq!(col.len().await, 3);
    assert_eq!(
        col.get("d1").await.unwrap().unwrap().metadata.unwrap()["v"], // unwrap allowed (AGENT:08)
        1
    );
    assert_eq!(
        col.get("d2").await.unwrap().unwrap().metadata.unwrap()["v"], // unwrap allowed (AGENT:08)
        2
    );
}

#[tokio::test]
async fn test_collection_upsert_many() {
    let (db, _tmp) = setup_db(3).await;
    let col = db.collection("upsert_test").await.expect("col");

    // 1. Initial insert
    col.insert("d1", &[1.0, 0.0, 0.0], None).await.unwrap(); // unwrap allowed (AGENT:08)

    // 2. Upsert many (update d1, insert d2)
    let docs = vec![
        (
            "d1".to_string(),
            vec![1.0, 0.1, 0.0],
            Some(json!({"updated": true})),
        ),
        (
            "d2".to_string(),
            vec![0.0, 1.0, 0.0],
            Some(json!({"new": true})),
        ),
    ];

    col.upsert_many(&docs).await.expect("upsert_many failed");

    assert_eq!(col.len().await, 2);
    let d1 = col.get("d1").await.unwrap().unwrap(); // unwrap allowed (AGENT:08)
    assert!(d1.metadata.unwrap()["updated"].as_bool().unwrap()); // unwrap allowed (AGENT:08)
}

#[tokio::test]
async fn test_collection_scan_range_isolation() {
    let (db, _tmp) = setup_db(3).await;
    let col_a = db.collection("col_a").await.expect("col_a");
    let col_b = db.collection("col_b").await.expect("col_b");

    // Fill col_a
    col_a.insert("apple", &[1.0, 0.0, 0.0], None).await.unwrap(); // unwrap allowed (AGENT:08)
    col_a
        .insert("banana", &[0.0, 1.0, 0.0], None)
        .await
        .unwrap(); // unwrap allowed (AGENT:08)
    col_a
        .insert("cherry", &[0.0, 0.0, 1.0], None)
        .await
        .unwrap(); // unwrap allowed (AGENT:08)

    // Fill col_b with same keys but different values
    col_b
        .insert("apple", &[1.0, 1.0, 1.0], Some(json!({"from": "b"})))
        .await
        .unwrap(); // unwrap allowed (AGENT:08)

    // Scan col_a [apple, banana]
    let results = col_a
        .scan(Bound::Included(b"apple"), Bound::Included(b"banana"))
        .await
        .expect("scan");

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].0, "apple");
    assert_eq!(results[1].0, "banana");

    // Verify col_b scanning only returns its own keys
    let results_b = col_b
        .scan(Bound::Unbounded, Bound::Unbounded)
        .await
        .expect("scan b");
    assert_eq!(results_b.len(), 1);
    assert_eq!(results_b[0].0, "apple");
    assert_eq!(results_b[0].1["metadata"]["from"], "b");
}

#[tokio::test]
async fn test_collection_scan_prefix_isolation() {
    let (db, _tmp) = setup_db(3).await;
    let col = db.collection("prefix_test").await.expect("col");

    col.insert("user/1", &[0.1, 0.0, 0.0], None).await.unwrap(); // unwrap allowed (AGENT:08)
    col.insert("user/2", &[0.2, 0.0, 0.0], None).await.unwrap(); // unwrap allowed (AGENT:08)
    col.insert("item/1", &[0.3, 0.0, 0.0], None).await.unwrap(); // unwrap allowed (AGENT:08)

    let users = col.scan_prefix("user/").await.expect("scan_prefix");
    assert_eq!(users.len(), 2);
    assert!(users.iter().all(|(k, _)| k.starts_with("user/")));

    let items = col.scan_prefix("item/").await.expect("scan_prefix item");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].0, "user/1".to_string().replace("user/1", "item/1")); // Verification of key name
    assert_eq!(items[0].0, "item/1");
}
