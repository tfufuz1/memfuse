use memfuse_db::{MemFuse, MemFuseConfig};
use serde_json::json;
use tempfile::TempDir;

#[tokio::test]
async fn test_hybrid_search_integration() {
    let tmp = TempDir::new().unwrap();
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(tmp.path(), config).await.unwrap();

    // Insert documents
    db.insert("d1", &[1.0, 0.0, 0.0, 0.0], Some(json!({"text": "Rust programming language is fast"}))).await.unwrap();
    db.insert("d2", &[0.0, 1.0, 0.0, 0.0], Some(json!({"text": "Python is dynamically typed"}))).await.unwrap();
    db.insert("d3", &[0.9, 0.1, 0.0, 0.0], Some(json!({"text": "Rust is also memory safe"}))).await.unwrap();

    // 1. Vector only search (text is empty)
    let vec_results = db.hybrid_search("", &[1.0, 0.0, 0.0, 0.0], 2).await.unwrap();
    assert_eq!(vec_results.len(), 2);
    assert_eq!(vec_results[0].id, "d1");
    assert_eq!(vec_results[1].id, "d3");

    // 2. Text only search (vector is all zeros)
    let text_results = db.hybrid_search("python", &[0.0, 0.0, 0.0, 0.0], 2).await.unwrap();
    assert_eq!(text_results.len(), 1);
    assert_eq!(text_results[0].id, "d2");

    // 3. Hybrid search
    // Query "rust" should favor d1 and d3 via BM25
    // Vector [0.0, 1.0, 0.0, 0.0] should favor d2 via vector search
    let hybrid_results = db.hybrid_search("rust", &[0.0, 1.0, 0.0, 0.0], 3).await.unwrap();

    // We expect all 3 to be present
    assert_eq!(hybrid_results.len(), 3);

    let ids: Vec<String> = hybrid_results.iter().map(|r| r.id.clone()).collect();
    assert!(ids.contains(&"d1".to_string()));
    assert!(ids.contains(&"d2".to_string()));
    assert!(ids.contains(&"d3".to_string()));
}

#[tokio::test]
async fn test_hybrid_search_update_cleanup() {
    let tmp = TempDir::new().unwrap();
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(tmp.path(), config).await.unwrap();

    // 1. Insert doc with "rust"
    db.insert("d1", &[1.0, 0.0, 0.0, 0.0], Some(json!({"text": "rust language"}))).await.unwrap();

    let res = db.hybrid_search("rust", &[0.0, 0.0, 0.0, 0.0], 10).await.unwrap();
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].id, "d1");

    // 2. Overwrite doc with "python"
    db.insert("d1", &[1.0, 0.0, 0.0, 0.0], Some(json!({"text": "python language"}))).await.unwrap();

    // Should NOT find "d1" for "rust" anymore
    let res_rust = db.hybrid_search("rust", &[0.0, 0.0, 0.0, 0.0], 10).await.unwrap();
    assert_eq!(res_rust.len(), 0);

    // Should find "d1" for "python"
    let res_python = db.hybrid_search("python", &[0.0, 0.0, 0.0, 0.0], 10).await.unwrap();
    assert_eq!(res_python.len(), 1);
    assert_eq!(res_python[0].id, "d1");
}

#[tokio::test]
async fn test_hybrid_search_delete_cleanup() {
    let tmp = TempDir::new().unwrap();
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(tmp.path(), config).await.unwrap();

    db.insert("d1", &[1.0, 0.0, 0.0, 0.0], Some(json!({"text": "rust language"}))).await.unwrap();
    db.delete("d1").await.unwrap();

    let res = db.hybrid_search("rust", &[0.0, 0.0, 0.0, 0.0], 10).await.unwrap();
    assert_eq!(res.len(), 0);
}
