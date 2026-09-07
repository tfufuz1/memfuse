// FILE-CONTEXT
// ZWECK: Corruption detection and DiskAnnFallbackPolicy test (Pflichttest 2).
// INVARIANTEN: Verifies that loading a corrupted/bit-flipped DiskANN file returns typed MemFuseError for FailFast, and logs error & transparently falls back to HNSW for UseHnswOnFailure.

#![cfg(feature = "experimental-diskann")]

use memfuse_core::{DistanceMetric, DocId, MemFuseError, VectorIndex};
use memfuse_index::diskann::{DiskAnnConfig, DiskAnnFallbackPolicy, DiskAnnIndex};

#[tokio::test]
async fn test_corrupted_diskann_fail_fast_policy_returns_typed_error() {
    let temp_dir = tempfile::tempdir().expect("tempdir creation");
    let index_path = temp_dir.path().join("corrupted_fail_fast.idx");

    let config = DiskAnnConfig {
        index_path: index_path.clone(),
        dimension: 8,
        max_degree: 4,
        distance_metric: DistanceMetric::Euclidean,
        fallback_policy: DiskAnnFallbackPolicy::FailFast,
        ..DiskAnnConfig::default()
    };

    // 1. Build a valid DiskANN index
    let index = DiskAnnIndex::try_new(config.clone()).expect("try_new");
    let vectors = vec![vec![1.0f32; 8]; 3];
    let ids = vec![DocId::from(1), DocId::from(2), DocId::from(3)];
    index.build(&vectors, &ids).await.expect("build");

    // 2. Deliberately corrupt bytes (bit-flip) in the index file payload
    let mut file_bytes = tokio::fs::read(&index_path).await.expect("read file");
    assert!(file_bytes.len() > 20);
    file_bytes[15] ^= 0xFF; // Bit flip in vector payload
    tokio::fs::write(&index_path, &file_bytes)
        .await
        .expect("write corrupt file");

    // 3. Reload with FailFast policy
    let reloaded = DiskAnnIndex::try_new(config).expect("try_new reloaded");
    let load_res = reloaded.load().await;

    // Verify corruption is detected and returns a typed error (not panic)
    assert!(
        load_res.is_err(),
        "Loading corrupted DiskANN index with FailFast must return Err"
    );
    let err = load_res.err().unwrap();
    match err {
        MemFuseError::Storage(msg) => {
            assert!(
                msg.contains("integrity validation failed")
                    || msg.contains("HMAC checksum mismatch")
                    || msg.contains("DiskANN"),
                "Expected integrity validation / HMAC mismatch error message, got: {}",
                msg
            );
        }
        other => panic!("Expected MemFuseError::Storage, got {:?}", other),
    }
}

#[tokio::test]
async fn test_corrupted_diskann_use_hnsw_fallback_policy() {
    let temp_dir = tempfile::tempdir().expect("tempdir creation");
    let index_path = temp_dir.path().join("corrupted_use_hnsw.idx");

    let config = DiskAnnConfig {
        index_path: index_path.clone(),
        dimension: 8,
        max_degree: 4,
        distance_metric: DistanceMetric::Euclidean,
        fallback_policy: DiskAnnFallbackPolicy::UseHnswOnFailure,
        ..DiskAnnConfig::default()
    };

    // 1. Build a valid DiskANN index
    let index = DiskAnnIndex::try_new(config.clone()).expect("try_new");
    let vectors = vec![vec![0.5f32; 8], vec![1.5f32; 8]];
    let ids = vec![DocId::from(10), DocId::from(20)];
    index.build(&vectors, &ids).await.expect("build");

    // 2. Corrupt index file
    let mut file_bytes = tokio::fs::read(&index_path).await.expect("read file");
    file_bytes[10] ^= 0xAA;
    tokio::fs::write(&index_path, &file_bytes)
        .await
        .expect("write corrupt file");

    // 3. Reload with UseHnswOnFailure policy
    let reloaded = DiskAnnIndex::try_new(config).expect("try_new reloaded");
    let load_res = reloaded.load().await;

    // Must succeed transparently by falling back to HNSW
    assert!(
        load_res.is_ok(),
        "Loading corrupted DiskANN index with UseHnswOnFailure must succeed via fallback"
    );

    // Dynamic operations (e.g. insert, search) on the HNSW fallback must now work seamlessly
    let tx = memfuse_core::TxId::new(1);
    let doc_id = DocId::from(30);
    let new_vec = vec![0.5f32; 8];

    reloaded
        .insert(tx, doc_id, &new_vec)
        .await
        .expect("insert on fallback HNSW must succeed");

    reloaded
        .commit(tx)
        .await
        .expect("commit on fallback HNSW must succeed");

    let search_res = reloaded
        .search(&new_vec, 1)
        .await
        .expect("search on fallback HNSW must succeed");
    assert_eq!(search_res.len(), 1);
    assert_eq!(search_res[0].doc_id, DocId::from(30));
}
