//! Comprehensive E2E integration tests for MemFuse.

use memfuse_checkpoint::CheckpointManager;
use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use tempfile::TempDir;

#[tokio::test]
async fn test_comprehensive_e2e_flow() {
    let tmp = TempDir::new().expect("temp dir");
    let path = tmp.path().to_owned();

    let config = MemFuseConfig {
        dimension: 4,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };

    // 1. Setup - Open DB and create collections
    let db = MemFuse::open_with_config(&path, config.clone())
        .await
        .expect("open db");
    let tech_col = db.collection("tech").await.expect("tech col");
    let nature_col = db.collection("nature").await.expect("nature col");

    // 2. Ingest Data
    // Tech data
    tech_col
        .insert(
            "rust-lang",
            &[1.0, 0.9, 0.0, 0.0],
            Some(json!({"text": "Rust is a systems programming language focusing on safety and speed."})),
        )
        .await
        .expect("insert rust");
    tech_col
        .insert(
            "python-lang",
            &[0.9, 0.8, 0.0, 0.0],
            Some(json!({"text": "Python is a versatile programming language with great library support."})),
        )
        .await
        .expect("insert python");

    // Nature data
    nature_col
        .insert(
            "forest",
            &[0.0, 0.0, 1.0, 0.9],
            Some(json!({"text": "The forest is full of green trees and wildlife."})),
        )
        .await
        .expect("insert forest");
    nature_col
        .insert(
            "ocean",
            &[0.0, 0.0, 0.9, 0.8],
            Some(json!({"text": "The ocean covers most of our planet and is home to many fish."})),
        )
        .await
        .expect("insert ocean");

    // 3. Hybrid Search - Verify RRF
    // Search in tech for "systems" and a tech-like vector
    let query_vec = vec![1.0, 1.0, 0.0, 0.0];
    let results = tech_col
        .hybrid_search("systems", &query_vec, 10)
        .await
        .expect("hybrid search tech");

    assert!(!results.is_empty());
    assert_eq!(results[0].id, "rust-lang"); // "systems" is in rust-lang text

    // Search in nature for "fish" and an ocean-like vector
    let query_vec_nature = vec![0.0, 0.0, 1.0, 1.0];
    let results_nature = nature_col
        .hybrid_search("fish", &query_vec_nature, 10)
        .await
        .expect("hybrid search nature");

    assert!(!results_nature.is_empty());
    assert_eq!(results_nature[0].id, "ocean"); // "fish" is in ocean text

    // 4. Isolation Check
    // Searching "forest" in tech_col should yield nothing or low rank if vector matches (but query_vec is tech-like)
    let cross_results = tech_col
        .hybrid_search("forest", &query_vec, 10)
        .await
        .expect("cross search");
    for res in cross_results {
        assert_ne!(res.id, "forest");
    }

    // 5. Relationships
    tech_col
        .relate("rust-lang", "python-lang", "alternative")
        .await
        .expect("relate");
    let rels = tech_col
        .scan_prefix("__rel:rust-lang:alternative:")
        .await
        .expect("scan rels");
    assert_eq!(rels.len(), 1);
    assert_eq!(rels[0].1["to"], "python-lang");

    // 6. Checkpoint Integration
    let cp_manager = CheckpointManager::new(db.inner_storage());
    let cp1 = cp_manager
        .create_checkpoint("base_state")
        .await
        .expect("create cp");
    assert!(cp1.seq_no > 0);

    // 7. Post-checkpoint changes
    tech_col
        .insert(
            "cpp-lang",
            &[1.0, 0.8, 0.1, 0.0],
            Some(json!({"text": "C++ is a classic systems language."})),
        )
        .await
        .expect("insert cpp");
    assert_eq!(tech_col.len().await, 3);

    // 8. Stats and Verification
    let _stats = db.stats().await.expect("db stats");
    // default col is always created, plus tech and nature
    let cols = db.list_collections().await.expect("list cols");
    assert!(cols.contains(&"tech".to_string()));
    assert!(cols.contains(&"nature".to_string()));
    assert!(cols.contains(&"default".to_string()));

    println!("E2E Comprehensive Test Passed!");
}
