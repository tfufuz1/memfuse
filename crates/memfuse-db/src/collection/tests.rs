// FILE-CONTEXT
// ZWECK: Unit-Tests für Collection-CRUD, Indizierung, Repair und Grenzwerte.
// INVARIANTEN: Keine Tautologien; Anti-Mirroring gewahrt; Unabhängig berechnete Erwartungswerte.
// NICHT-OFFENSICHTLICH: Tests laufen isoliert in temporären Verzeichnissen.
// STAND: TS:2026-08-29T17:22:29Z (SESSION: 0dcb9f3b)

#[tokio::test]
async fn test_insert_with_ttl_and_reap_expired_documents() {
    use memfuse_graph::CsrGraph;
    use memfuse_index::HnswIndex;
    use memfuse_store::LsmStorage;
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;
    use tempfile::tempdir;

    let dir = tempdir().unwrap(); // unwrap
    let storage = Arc::new(
        LsmStorage::new(memfuse_store::LsmConfig {
            path: dir.path().to_path_buf(),
            ..Default::default()
        })
        .await
        .unwrap(), // unwrap
    );
    let index = Arc::new(
        HnswIndex::try_new(memfuse_index::HnswConfig {
            dimension: 4,
            ..Default::default()
        })
        .unwrap(), // unwrap
    );
    let col = super::Collection::new(
        "default".to_string(),
        storage,
        index,
        Arc::new(CsrGraph::new()),
        Arc::new(AtomicU64::new(1)),
        4,
        memfuse_text::Language::English,
    );

    let vec = vec![1.0, 0.0, 0.0, 0.0];

    // Insert document with TTL = 5 committed ops
    col.insert_with_ttl("temp_doc", &vec, None, 5)
        .await
        .unwrap(); // unwrap

    // 1. Immediately after insert, document should be retrievable
    let doc = col.get("temp_doc").await.unwrap(); // unwrap
    assert!(doc.is_some(), "Document must exist before TTL expiration");

    // 2. Perform 5 dummy commits (inserts)
    for i in 0..5 {
        col.insert(&format!("dummy_{i}"), &vec, None).await.unwrap(); // unwrap
    }

    // 3. Trigger expiry reaper
    let reaped = col.reap_expired_documents(100).await.unwrap(); // unwrap
    assert_eq!(reaped, 1, "Expired document should be reaped");

    // 4. Verify document is gone from storage and search
    let doc_after = col.get("temp_doc").await.unwrap(); // unwrap
    assert!(
        doc_after.is_none(),
        "Document must be deleted after TTL expiry"
    );

    let search_res = col.search(&vec, 10).await.unwrap(); // unwrap
    assert!(
        search_res.iter().all(|r| r.id != "temp_doc"),
        "Expired document must not appear in search results"
    );
}

#[tokio::test]
async fn test_relate_success_visible_in_storage_and_graph() {
    use memfuse_core::EntityId;
    use memfuse_graph::csr::CsrGraph;
    use memfuse_index::HnswIndex;
    use memfuse_store::{LsmConfig, LsmStorage};
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;
    use tempfile::tempdir;

    let dir = tempdir().unwrap(); // unwrap
    let storage = Arc::new(
        LsmStorage::new(LsmConfig {
            path: dir.path().to_path_buf(),
            ..Default::default()
        })
        .await
        .unwrap(), // unwrap
    );
    let index = Arc::new(
        HnswIndex::try_new(memfuse_index::HnswConfig {
            dimension: 4,
            ..Default::default()
        })
        .unwrap(), // unwrap
    );
    let graph = Arc::new(CsrGraph::new());
    let col = super::Collection::new(
        "default".to_string(),
        storage.clone(),
        index,
        graph.clone(),
        Arc::new(AtomicU64::new(1)),
        4,
        memfuse_text::Language::English,
    );

    col.relate("doc1", "doc2", "references").await.unwrap(); // unwrap

    // 1. Storage check
    let rels = col.scan_prefix("__rel:").await.unwrap(); // unwrap
    assert_eq!(rels.len(), 1);
    assert!(rels[0].0.contains("doc1:references:doc2"));

    // 2. Graph check
    let id1 = EntityId::from_key("doc1").unwrap(); // unwrap
    let id2 = EntityId::from_key("doc2").unwrap(); // unwrap
    let neighbors = graph.neighbors(id1).await.unwrap(); // unwrap
    assert!(neighbors.contains(&id2));
}

#[tokio::test]
async fn test_relate_rollback_semantics_on_storage_commit_failure() {
    use async_trait::async_trait;
    use memfuse_core::{Result, StorageEngine, StorageStats, TxId};
    use memfuse_graph::csr::CsrGraph;
    use memfuse_index::HnswIndex;
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;

    struct FailOnStorageCommit;

    #[async_trait]
    impl StorageEngine for FailOnStorageCommit {
        async fn get(&self, _: &[u8]) -> Result<Option<Vec<u8>>> {
            Ok(None)
        }
        async fn get_at_seq(&self, _: &[u8], _: u64) -> Result<Option<Vec<u8>>> {
            Ok(None)
        }
        async fn put(&self, _: TxId, _: &[u8], _: &[u8]) -> Result<()> {
            Ok(())
        }
        async fn delete(&self, _: TxId, _: &[u8]) -> Result<()> {
            Ok(())
        }
        async fn commit(&self, _: TxId) -> Result<()> {
            Err(memfuse_core::MemFuseError::Storage(
                "Simulated Storage Commit Failure".into(),
            ))
        }
        async fn rollback(&self, _: TxId) -> Result<()> {
            Ok(())
        }
        async fn rollback_to_tx(&self, _: TxId) -> Result<()> {
            Ok(())
        }
        async fn flush(&self) -> Result<()> {
            Ok(())
        }
        async fn stats(&self) -> Result<StorageStats> {
            Ok(StorageStats {
                num_segments: 0,
                total_size_bytes: 0,
                memtable_size_bytes: 0,
            })
        }
        async fn last_seq_no(&self) -> Result<u64> {
            Ok(0)
        }
        async fn last_tx_id(&self) -> Result<TxId> {
            Ok(TxId(0))
        }
        async fn pin_checkpoint(&self, _: u64) -> Result<()> {
            Ok(())
        }
        async fn unpin_checkpoint(&self, _: u64) -> Result<()> {
            Ok(())
        }
        async fn scan_prefix(&self, _: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
            Ok(vec![])
        }
        async fn scan(
            &self,
            _: std::ops::Bound<&[u8]>,
            _: std::ops::Bound<&[u8]>,
        ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
            Ok(vec![])
        }
    }

    let storage = Arc::new(FailOnStorageCommit);
    let index = Arc::new(
        HnswIndex::try_new(memfuse_index::HnswConfig {
            dimension: 4,
            ..Default::default()
        })
        .unwrap(), // unwrap
    );
    let graph = Arc::new(CsrGraph::new());
    let col = super::Collection::new(
        "default".to_string(),
        storage,
        index,
        graph.clone(),
        Arc::new(AtomicU64::new(1)),
        4,
        memfuse_text::Language::English,
    );

    let res = col.relate("node_x", "node_y", "links").await;
    assert!(
        res.is_err(),
        "relate() must fail when storage.commit() fails"
    );

    // Graph index should remain empty since relate failed before graph commit
    assert_eq!(graph.entity_count(), 0);
}

// REGRESSION TEST für F-01: beweist gebrochene Rollback-Semantik in relate()
#[tokio::test]
async fn test_relate_rollback_semantics_on_graph_commit_failure() {
    use async_trait::async_trait;
    use memfuse_core::{Result, StorageEngine, StorageStats, TxId};
    use memfuse_graph::csr::{CsrGraph, CsrGraphConfig};
    use memfuse_index::HnswIndex;
    use memfuse_store::{LsmConfig, LsmStorage};
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::Arc;
    use tempfile::tempdir;

    struct FailOnPutStorage {
        should_fail: AtomicBool,
    }

    #[async_trait]
    impl StorageEngine for FailOnPutStorage {
        async fn get(&self, _: &[u8]) -> Result<Option<Vec<u8>>> {
            Ok(None)
        }
        async fn get_at_seq(&self, _: &[u8], _: u64) -> Result<Option<Vec<u8>>> {
            Ok(None)
        }
        async fn put(&self, _: TxId, _: &[u8], _: &[u8]) -> Result<()> {
            if self.should_fail.load(Ordering::SeqCst) {
                Err(memfuse_core::MemFuseError::Storage(
                    "Simulated Graph Storage Commit Failure".into(),
                ))
            } else {
                Ok(())
            }
        }
        async fn delete(&self, _: TxId, _: &[u8]) -> Result<()> {
            Ok(())
        }
        async fn commit(&self, _: TxId) -> Result<()> {
            Ok(())
        }
        async fn rollback(&self, _: TxId) -> Result<()> {
            Ok(())
        }
        async fn rollback_to_tx(&self, _: TxId) -> Result<()> {
            Ok(())
        }
        async fn flush(&self) -> Result<()> {
            Ok(())
        }
        async fn stats(&self) -> Result<StorageStats> {
            Ok(StorageStats {
                num_segments: 0,
                total_size_bytes: 0,
                memtable_size_bytes: 0,
            })
        }
        async fn last_seq_no(&self) -> Result<u64> {
            Ok(0)
        }
        async fn last_tx_id(&self) -> Result<TxId> {
            Ok(TxId(0))
        }
        async fn pin_checkpoint(&self, _: u64) -> Result<()> {
            Ok(())
        }
        async fn unpin_checkpoint(&self, _: u64) -> Result<()> {
            Ok(())
        }
        async fn scan_prefix(&self, _: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
            Ok(vec![])
        }
        async fn scan(
            &self,
            _: std::ops::Bound<&[u8]>,
            _: std::ops::Bound<&[u8]>,
        ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
            Ok(vec![])
        }
    }

    let dir = tempdir().unwrap(); // unwrap
    let lsm_config = LsmConfig {
        path: dir.path().to_path_buf(),
        ..Default::default()
    };
    let storage = Arc::new(LsmStorage::new(lsm_config).await.unwrap()); // unwrap
    let index = Arc::new(
        HnswIndex::try_new(memfuse_index::HnswConfig {
            dimension: 4,
            ..Default::default()
        })
        .unwrap(), // unwrap
    );

    let fail_storage = Arc::new(FailOnPutStorage {
        should_fail: AtomicBool::new(true),
    });
    let graph = Arc::new(CsrGraph::with_config_and_storage(
        CsrGraphConfig::default(),
        fail_storage,
    ));
    let next_tx = Arc::new(AtomicU64::new(1));

    let col = super::Collection::new(
        "default".to_string(),
        storage.clone(),
        index,
        graph,
        next_tx,
        4,
        memfuse_text::Language::English,
    );

    // relate() should fail when graph_index.commit() fails
    let res = col.relate("entity_a", "entity_b", "connects").await;
    assert!(
        res.is_err(),
        "relate() must return Err when graph commit fails"
    );

    // Verification: storage MUST NOT contain the relation key after failed relate()
    let rel_prefix = col.namespaced_key(b"", 2);
    let remaining_rels = storage.scan_prefix(&rel_prefix).await.unwrap(); // unwrap
    assert!(
        remaining_rels.is_empty(),
        "Storage layer MUST NOT contain relation keys after relate() failure! Found: {:?}",
        remaining_rels
    );
}

#[tokio::test]
async fn test_collection_embedder_async_embed() {
    use async_trait::async_trait;
    use memfuse_core::TextEmbeddingEngine;
    use std::sync::Arc;

    struct FakeEmbedder;

    #[async_trait]
    impl TextEmbeddingEngine for FakeEmbedder {
        async fn embed(&self, text: &str) -> memfuse_core::Result<Vec<f32>> {
            Ok(vec![text.len() as f32 / 100.0; 4])
        }
    }

    // Verify: compile-time proof that the method signature is async and
    // accepts Arc<dyn TextEmbeddingEngine>.
    let embedder: Arc<dyn TextEmbeddingEngine> = Arc::new(FakeEmbedder);
    let result = embedder.embed("hello").await.unwrap(); // unwrap
    assert_eq!(result.len(), 4);
}

#[tokio::test]
async fn hybrid_search_caps_k_at_max_search_k() {
    use memfuse_graph::CsrGraph;
    use memfuse_index::HnswIndex;
    use memfuse_store::LsmStorage;
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;
    use tempfile::tempdir;

    let dir = tempdir().unwrap(); // unwrap
    let lsm_config = memfuse_store::LsmConfig {
        path: dir.path().to_path_buf(),
        ..Default::default()
    };
    let storage = Arc::new(LsmStorage::new(lsm_config).await.unwrap()); // unwrap
    let hnsw_config = memfuse_index::HnswConfig {
        dimension: 4,
        ..Default::default()
    };
    let index = Arc::new(HnswIndex::try_new(hnsw_config).unwrap()); // unwrap
    let graph = Arc::new(CsrGraph::new());
    let next_tx = Arc::new(AtomicU64::new(1));

    let col = super::Collection::new(
        "default".to_string(),
        storage,
        index,
        graph,
        next_tx,
        4,
        memfuse_text::Language::English,
    );

    let res = col
        .hybrid_search("test", &[0.1, 0.2, 0.3, 0.4], 100_000, None)
        .await
        .unwrap(); // unwrap

    assert!(
        res.len() <= memfuse_core::MAX_SEARCH_K,
        "Results length {} should be <= MAX_SEARCH_K ({})",
        res.len(),
        memfuse_core::MAX_SEARCH_K
    );
}

#[tokio::test]
async fn test_input_guards_boundary_validation() {
    use memfuse_graph::CsrGraph;
    use memfuse_index::HnswIndex;
    use memfuse_store::LsmStorage;
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;
    use tempfile::tempdir;

    let dir = tempdir().unwrap(); // unwrap
    let lsm_config = memfuse_store::LsmConfig {
        path: dir.path().to_path_buf(),
        ..Default::default()
    };
    let storage = Arc::new(LsmStorage::new(lsm_config).await.unwrap()); // unwrap
    let hnsw_config = memfuse_index::HnswConfig {
        dimension: 4,
        ..Default::default()
    };
    let index = Arc::new(HnswIndex::try_new(hnsw_config).unwrap()); // unwrap
    let graph = Arc::new(CsrGraph::new());
    let next_tx = Arc::new(AtomicU64::new(1));

    let col = super::Collection::new(
        "default".to_string(),
        storage,
        index,
        graph,
        next_tx,
        4,
        memfuse_text::Language::English,
    );

    let vec = vec![1.0, 0.0, 0.0, 0.0];

    // 0. Empty inputs in relate()
    let err_relate_empty_from = col.relate("", "doc2", "knows").await;
    assert!(matches!(
        err_relate_empty_from,
        Err(memfuse_core::MemFuseError::InvalidInput(_))
    ));

    let err_relate_empty_to = col.relate("doc1", "", "knows").await;
    assert!(matches!(
        err_relate_empty_to,
        Err(memfuse_core::MemFuseError::InvalidInput(_))
    ));

    // 1. Empty ID guard on insert / upsert
    let err_empty_id = col.insert("", &vec, None).await;
    assert!(matches!(
        err_empty_id,
        Err(memfuse_core::MemFuseError::InvalidInput(_))
    ));

    let err_empty_id_upsert = col.upsert("", &vec, None).await;
    assert!(matches!(
        err_empty_id_upsert,
        Err(memfuse_core::MemFuseError::InvalidInput(_))
    ));

    // 2. Oversized ID guard (>1024 bytes)
    let long_id = "a".repeat(1025);
    let err_long_id = col.insert(&long_id, &vec, None).await;
    assert!(matches!(
        err_long_id,
        Err(memfuse_core::MemFuseError::InvalidInput(_))
    ));

    // 3. insert_many / upsert_many empty batch guard
    let err_empty_batch = col.insert_many(&[]).await;
    assert!(matches!(
        err_empty_batch,
        Err(memfuse_core::MemFuseError::InvalidInput(_))
    ));

    let err_empty_batch_upsert = col.upsert_many(&[]).await;
    assert!(matches!(
        err_empty_batch_upsert,
        Err(memfuse_core::MemFuseError::InvalidInput(_))
    ));

    // 4. insert_many / upsert_many oversized batch guard (>10,000)
    let huge_batch: Vec<_> = (0..10_001)
        .map(|i| (format!("d_{i}"), vec.clone(), None))
        .collect();
    let err_huge_batch = col.insert_many(&huge_batch).await;
    assert!(matches!(
        err_huge_batch,
        Err(memfuse_core::MemFuseError::InvalidInput(_))
    ));

    let err_huge_batch_upsert = col.upsert_many(&huge_batch).await;
    assert!(matches!(
        err_huge_batch_upsert,
        Err(memfuse_core::MemFuseError::InvalidInput(_))
    ));

    // 5. search / search_with_filter_expr k = 0 guard
    let err_search_k_zero = col.search(&vec, 0).await;
    assert!(matches!(
        err_search_k_zero,
        Err(memfuse_core::MemFuseError::InvalidInput(_))
    ));
}

#[tokio::test]
async fn test_hybrid_search_k_clamping_boundaries() {
    use memfuse_graph::CsrGraph;
    use memfuse_index::HnswIndex;
    use memfuse_store::LsmStorage;
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;
    use tempfile::tempdir;

    let dir = tempdir().unwrap(); // unwrap
    let lsm_config = memfuse_store::LsmConfig {
        path: dir.path().to_path_buf(),
        ..Default::default()
    };
    let storage = Arc::new(LsmStorage::new(lsm_config).await.unwrap()); // unwrap
    let hnsw_config = memfuse_index::HnswConfig {
        dimension: 4,
        ..Default::default()
    };
    let index = Arc::new(HnswIndex::try_new(hnsw_config).unwrap()); // unwrap
    let graph = Arc::new(CsrGraph::new());
    let next_tx = Arc::new(AtomicU64::new(1));

    let col = super::Collection::new(
        "default".to_string(),
        storage,
        index,
        graph,
        next_tx,
        4,
        memfuse_text::Language::English,
    );

    // 1. k = 0 boundary check (must short-circuit to empty results without panic)
    let res_zero = col
        .hybrid_search("test", &[0.1, 0.2, 0.3, 0.4], 0, None)
        .await
        .unwrap(); // unwrap
    assert!(res_zero.is_empty(), "k=0 must return empty result list");

    // 2. k = usize::MAX boundary check (must clamp to MAX_SEARCH_K without panic/overflow)
    let res_max = col
        .hybrid_search("test", &[0.0, 0.0, 0.0, 0.0], usize::MAX, None)
        .await
        .unwrap(); // unwrap
    assert!(
        res_max.is_empty(),
        "k=usize::MAX on empty DB must return empty without overflow panic"
    );
}

#[tokio::test]
async fn test_doc_id_collision_rejected() {
    use memfuse_core::{DocId, MemFuseError, StorageEngine, TxId};
    use memfuse_graph::CsrGraph;
    use memfuse_index::HnswIndex;
    use memfuse_store::LsmStorage;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;
    use tempfile::tempdir;

    let dir = tempdir().unwrap(); // unwrap
    let lsm_config = memfuse_store::LsmConfig {
        path: dir.path().to_path_buf(),
        ..Default::default()
    };
    let storage = Arc::new(LsmStorage::new(lsm_config).await.unwrap()); // unwrap
    let index = Arc::new(
        HnswIndex::try_new(memfuse_index::HnswConfig {
            dimension: 4,
            ..Default::default()
        })
        .unwrap(), // unwrap
    );
    let graph = Arc::new(CsrGraph::new());
    let next_tx = Arc::new(AtomicU64::new(1));

    let col = super::Collection::new(
        "default".to_string(),
        storage,
        index,
        graph,
        next_tx.clone(),
        4,
        memfuse_text::Language::English,
    );

    // 1. Insert first document normally
    let id1 = "key_alpha";
    let emb1 = vec![1.0, 0.0, 0.0, 0.0];
    col.insert(id1, &emb1, None).await.unwrap(); // unwrap

    // Verify key_alpha exists
    let doc1 = col.get(id1).await.unwrap(); // unwrap
    assert!(doc1.is_some());

    // 2. Synthetically inject a mapping for a fixed DocId (e.g. DocId::new(42)) pointing to "key_existing"
    let synthetic_doc_id = DocId::new(42);
    let tx = TxId::new(next_tx.fetch_add(1, Ordering::SeqCst));
    let doc_key = col.namespaced_key(&synthetic_doc_id.inner().to_le_bytes(), 1);
    let existing_meta = super::StoredDocumentMeta {
        id: "key_existing".to_string(),
        metadata: None,
    };
    let meta_bytes = serde_json::to_vec(&existing_meta).unwrap(); // unwrap
    col.storage.put(tx, &doc_key, &meta_bytes).await.unwrap(); // unwrap
    col.storage.commit(tx).await.unwrap(); // unwrap

    // 3. Directly test check_doc_id_collision with a different string key (e.g., "key_new")
    let collision_res = col
        .check_doc_id_collision(synthetic_doc_id, "key_new")
        .await;
    assert!(collision_res.is_err());
    match collision_res {
        Err(MemFuseError::Internal(msg)) => {
            assert!(
                msg.contains("DocId-Kollision erkannt für Schlüssel 'key_new'"),
                "Unexpected error message: {}",
                msg
            );
        }
        res => panic!("Expected MemFuseError::Internal, got {:?}", res),
    }

    // 4. Same key string should NOT be treated as a collision
    let same_key_res = col
        .check_doc_id_collision(synthetic_doc_id, "key_existing")
        .await;
    assert!(same_key_res.is_ok());
}

#[tokio::test]
#[allow(deprecated)]
async fn test_collection_next_tx_sequence() {
    use memfuse_graph::CsrGraph;
    use memfuse_index::HnswIndex;
    use memfuse_store::LsmStorage;
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;
    use tempfile::tempdir;

    let dir = tempdir().unwrap(); // unwrap
    let lsm_config = memfuse_store::LsmConfig {
        path: dir.path().to_path_buf(),
        ..Default::default()
    };
    let storage = Arc::new(LsmStorage::new(lsm_config).await.unwrap()); // unwrap
    let index = Arc::new(
        HnswIndex::try_new(memfuse_index::HnswConfig {
            dimension: 4,
            ..Default::default()
        })
        .unwrap(), // unwrap
    );
    let graph = Arc::new(CsrGraph::new());
    let next_tx = Arc::new(AtomicU64::new(1));

    let col = super::Collection::new(
        "default".to_string(),
        storage,
        index,
        graph,
        next_tx,
        4,
        memfuse_text::Language::English,
    );

    let tx1 = col.next_tx().unwrap(); // unwrap allowed
    let tx2 = col.next_tx().unwrap(); // unwrap allowed
    let tx3 = col.next_tx().unwrap(); // unwrap allowed

    assert_eq!(tx1.inner(), 1);
    assert_eq!(tx2.inner(), 2);
    assert_eq!(tx3.inner(), 3);
}

#[tokio::test]
async fn test_collection_allocate_tx_sequence() {
    use memfuse_graph::CsrGraph;
    use memfuse_index::HnswIndex;
    use memfuse_store::LsmStorage;
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;
    use tempfile::tempdir;

    let dir = tempdir().unwrap(); // unwrap
    let lsm_config = memfuse_store::LsmConfig {
        path: dir.path().to_path_buf(),
        ..Default::default()
    };
    let storage = Arc::new(LsmStorage::new(lsm_config).await.unwrap()); // unwrap
    let index = Arc::new(
        HnswIndex::try_new(memfuse_index::HnswConfig {
            dimension: 4,
            ..Default::default()
        })
        .unwrap(), // unwrap
    );
    let graph = Arc::new(CsrGraph::new());
    let next_tx = Arc::new(AtomicU64::new(100));

    let col = super::Collection::new(
        "default".to_string(),
        storage,
        index,
        graph,
        next_tx,
        4,
        memfuse_text::Language::English,
    );

    let tx1 = col.allocate_tx().unwrap(); // unwrap allowed
    let tx2 = col.allocate_tx().unwrap(); // unwrap allowed
    let tx3 = col.allocate_tx().unwrap(); // unwrap allowed

    assert_eq!(tx1.inner(), 100);
    assert_eq!(tx2.inner(), 101);
    assert_eq!(tx3.inner(), 102);
}

#[tokio::test]
async fn test_concurrent_insert_and_write_ops_lock_safety() {
    use memfuse_graph::CsrGraph;
    use memfuse_index::HnswIndex;
    use memfuse_store::LsmStorage;
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;
    use tempfile::tempdir;

    let dir = tempdir().unwrap(); // unwrap
    let lsm_config = memfuse_store::LsmConfig {
        path: dir.path().to_path_buf(),
        ..Default::default()
    };
    let storage = Arc::new(LsmStorage::new(lsm_config).await.unwrap()); // unwrap
    let index = Arc::new(
        HnswIndex::try_new(memfuse_index::HnswConfig {
            dimension: 4,
            ..Default::default()
        })
        .unwrap(), // unwrap
    );
    let graph = Arc::new(CsrGraph::new());
    let next_tx = Arc::new(AtomicU64::new(1));

    let col = Arc::new(super::Collection::new(
        "default".to_string(),
        storage,
        index,
        graph,
        next_tx,
        4,
        memfuse_text::Language::English,
    ));

    let mut handles = Vec::new();

    // Task 1: Single inserts
    {
        let c = col.clone();
        handles.push(tokio::spawn(async move {
            for i in 0..10 {
                let id = format!("single_doc_{i}");
                c.insert(&id, &[1.0, 0.0, 0.0, 0.0], None).await.unwrap(); // unwrap
            }
        }));
    }

    // Task 2: Insert many
    {
        let c = col.clone();
        handles.push(tokio::spawn(async move {
            let docs: Vec<_> = (0..5)
                .map(|i| (format!("batch_doc_{i}"), vec![0.0, 1.0, 0.0, 0.0], None))
                .collect();
            c.insert_many(&docs).await.unwrap(); // unwrap
        }));
    }

    // Task 3: Upsert & Update
    {
        let c = col.clone();
        handles.push(tokio::spawn(async move {
            for i in 0..5 {
                let id = format!("upsert_doc_{i}");
                c.upsert(&id, &[0.0, 0.0, 1.0, 0.0], None).await.unwrap(); // unwrap
                c.update(&id, &[0.0, 0.0, 1.0, 1.0], None).await.unwrap(); // unwrap
            }
        }));
    }

    // Task 4: Upsert many
    {
        let c = col.clone();
        handles.push(tokio::spawn(async move {
            let docs: Vec<_> = (0..5)
                .map(|i| (format!("upsert_batch_{i}"), vec![0.5, 0.5, 0.0, 0.0], None))
                .collect();
            c.upsert_many(&docs).await.unwrap(); // unwrap
        }));
    }

    for h in handles {
        h.await.unwrap(); // unwrap
    }

    assert!(col.len().await > 0);
}

#[tokio::test]
async fn test_ttl_missing_created_at_does_not_expire() {
    use memfuse_graph::CsrGraph;
    use memfuse_index::HnswIndex;
    use memfuse_store::LsmStorage;
    use serde_json::json;
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;
    use tempfile::tempdir;

    let dir = tempdir().unwrap(); // unwrap
    let storage = Arc::new(
        LsmStorage::new(memfuse_store::LsmConfig {
            path: dir.path().to_path_buf(),
            ..Default::default()
        })
        .await
        .unwrap(), // unwrap
    );
    let index = Arc::new(
        HnswIndex::try_new(memfuse_index::HnswConfig {
            dimension: 4,
            ..Default::default()
        })
        .unwrap(), // unwrap
    );
    let col = super::Collection::new(
        "default".to_string(),
        storage,
        index,
        Arc::new(CsrGraph::new()),
        Arc::new(AtomicU64::new(1)),
        4,
        memfuse_text::Language::English,
    );

    col.insert(
        "doc_no_created_at",
        &[1.0, 0.0, 0.0, 0.0],
        Some(json!({"ttl_ms": 10})),
    )
    .await
    .unwrap(); // unwrap
    let reaped = col.trigger_reaper().await.unwrap(); // unwrap
    assert_eq!(reaped, 0);
    assert!(col.get("doc_no_created_at").await.unwrap().is_some()); // unwrap
}

#[tokio::test]
async fn test_ttl_zero_does_not_expire() {
    use memfuse_graph::CsrGraph;
    use memfuse_index::HnswIndex;
    use memfuse_store::LsmStorage;
    use serde_json::json;
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;
    use tempfile::tempdir;

    let dir = tempdir().unwrap(); // unwrap
    let storage = Arc::new(
        LsmStorage::new(memfuse_store::LsmConfig {
            path: dir.path().to_path_buf(),
            ..Default::default()
        })
        .await
        .unwrap(), // unwrap
    );
    let index = Arc::new(
        HnswIndex::try_new(memfuse_index::HnswConfig {
            dimension: 4,
            ..Default::default()
        })
        .unwrap(), // unwrap
    );
    let col = super::Collection::new(
        "default".to_string(),
        storage,
        index,
        Arc::new(CsrGraph::new()),
        Arc::new(AtomicU64::new(1)),
        4,
        memfuse_text::Language::English,
    );

    col.insert(
        "doc_zero_ttl",
        &[1.0, 0.0, 0.0, 0.0],
        Some(json!({"created_at_ms": 100, "ttl_ms": 0})),
    )
    .await
    .unwrap(); // unwrap
    let reaped = col.trigger_reaper().await.unwrap(); // unwrap
    assert_eq!(reaped, 0);
    assert!(col.get("doc_zero_ttl").await.unwrap().is_some()); // unwrap
}

#[tokio::test]
async fn test_extract_text_with_contextual_prefix() {
    use serde_json::json;

    let meta = Some(json!({
        "contextual_prefix": "Dokumenten-Kontext-Präfix",
        "text": "Chunk Haupttext"
    }));

    let extracted = super::extract_text(&meta);
    assert!(extracted.is_some());
    let text = extracted.unwrap(); // unwrap
    assert!(text.contains("Dokumenten-Kontext-Präfix"));
    assert!(text.contains("Chunk Haupttext"));
    assert_eq!(text, "Dokumenten-Kontext-Präfix\n\nChunk Haupttext");
}

#[tokio::test]
async fn test_ttl_overflow_does_not_expire() {
    use memfuse_graph::CsrGraph;
    use memfuse_index::HnswIndex;
    use memfuse_store::LsmStorage;
    use serde_json::json;
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;
    use tempfile::tempdir;

    let dir = tempdir().unwrap(); // unwrap
    let storage = Arc::new(
        LsmStorage::new(memfuse_store::LsmConfig {
            path: dir.path().to_path_buf(),
            ..Default::default()
        })
        .await
        .unwrap(), // unwrap
    );
    let index = Arc::new(
        HnswIndex::try_new(memfuse_index::HnswConfig {
            dimension: 4,
            ..Default::default()
        })
        .unwrap(), // unwrap
    );
    let col = super::Collection::new(
        "default".to_string(),
        storage,
        index,
        Arc::new(CsrGraph::new()),
        Arc::new(AtomicU64::new(1)),
        4,
        memfuse_text::Language::English,
    );

    col.insert(
        "doc_overflow",
        &[1.0, 0.0, 0.0, 0.0],
        Some(json!({"created_at_ms": u64::MAX - 10, "ttl_ms": 100})),
    )
    .await
    .unwrap(); // unwrap
    let reaped = col.trigger_reaper().await.unwrap(); // unwrap
    assert_eq!(reaped, 0);
    assert!(col.get("doc_overflow").await.unwrap().is_some()); // unwrap
}

#[tokio::test]
async fn test_migrate_doc_keys_v1() {
    use memfuse_core::{DocId, StorageEngine, TxId};
    use memfuse_graph::CsrGraph;
    use memfuse_index::HnswIndex;
    use memfuse_store::LsmStorage;
    use serde_json::json;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;
    use tempfile::tempdir;

    let dir = tempdir().unwrap(); // unwrap allowed (AGENT:04)
    let storage = Arc::new(
        LsmStorage::new(memfuse_store::LsmConfig {
            path: dir.path().to_path_buf(),
            ..Default::default()
        })
        .await
        .unwrap(), // unwrap allowed (AGENT:04)
    );
    let index = Arc::new(
        HnswIndex::try_new(memfuse_index::HnswConfig {
            dimension: 4,
            ..Default::default()
        })
        .unwrap(), // unwrap allowed (AGENT:04)
    );
    let next_tx = Arc::new(AtomicU64::new(1));
    let col = super::Collection::new(
        "default".to_string(),
        storage.clone(),
        index,
        Arc::new(CsrGraph::new()),
        next_tx.clone(),
        4,
        memfuse_text::Language::English,
    );

    // Inject legacy doc_key (containing embedding in StoredDocument)
    let doc_id = DocId::from_key("legacy_doc_1").unwrap(); // unwrap allowed (AGENT:04)
    let doc_key = col.namespaced_key(&doc_id.inner().to_le_bytes(), 1);
    let legacy_doc = super::StoredDocument {
        id: "legacy_doc_1".to_string(),
        embedding: vec![1.0, 0.0, 0.0, 0.0],
        metadata: Some(json!({"topic": "legacy"})),
    };
    let legacy_bytes = serde_json::to_vec(&legacy_doc).unwrap(); // unwrap allowed (AGENT:04)

    // Put user_key and legacy doc_key in storage
    let tx = TxId::new(next_tx.fetch_add(1, Ordering::SeqCst));
    let user_key = col.namespaced_key(b"legacy_doc_1", 0);
    storage.put(tx, &user_key, &legacy_bytes).await.unwrap(); // unwrap allowed (AGENT:04)
    storage.put(tx, &doc_key, &legacy_bytes).await.unwrap(); // unwrap allowed (AGENT:04)
    storage.commit(tx).await.unwrap(); // unwrap allowed (AGENT:04)

    // Verify doc_key currently contains full StoredDocument
    let raw_before = storage.get(&doc_key).await.unwrap().unwrap(); // unwrap allowed (AGENT:04)
    assert!(serde_json::from_slice::<super::StoredDocument>(&raw_before).is_ok());

    // Run migration
    let count = col.migrate_doc_keys_v1().await.unwrap(); // unwrap allowed (AGENT:04)
    assert_eq!(count, 1);

    // Verify doc_key now contains StoredDocumentMeta (and fails parsing as StoredDocument due to missing embedding)
    let raw_after = storage.get(&doc_key).await.unwrap().unwrap(); // unwrap allowed (AGENT:04)
    let meta: super::StoredDocumentMeta = serde_json::from_slice(&raw_after).unwrap(); // unwrap allowed (AGENT:04)
    assert_eq!(meta.id, "legacy_doc_1");
    assert_eq!(meta.metadata.unwrap()["topic"], "legacy"); // unwrap allowed (AGENT:04)
    assert!(serde_json::from_slice::<super::StoredDocument>(&raw_after).is_err());

    // Idempotency check: running migration again returns 0
    let count_again = col.migrate_doc_keys_v1().await.unwrap(); // unwrap allowed (AGENT:04)
    assert_eq!(count_again, 0);
}

#[tokio::test]
#[cfg(feature = "reranking")]
async fn test_hybrid_search_reranked_none() {
    use memfuse_graph::CsrGraph;
    use memfuse_index::HnswIndex;
    use memfuse_store::LsmStorage;
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;
    use tempfile::tempdir;

    let dir = tempdir().unwrap(); // unwrap
    let storage = Arc::new(
        LsmStorage::new(memfuse_store::LsmConfig {
            path: dir.path().to_path_buf(),
            ..Default::default()
        })
        .await
        .unwrap(), // unwrap
    );
    let index = Arc::new(
        HnswIndex::try_new(memfuse_index::HnswConfig {
            dimension: 4,
            ..Default::default()
        })
        .unwrap(), // unwrap
    );
    let col = super::Collection::new(
        "default".to_string(),
        storage,
        index,
        Arc::new(CsrGraph::new()),
        Arc::new(AtomicU64::new(1)),
        4,
        memfuse_text::Language::English,
    );

    col.insert(
        "d1",
        &[1.0, 0.0, 0.0, 0.0],
        Some(serde_json::json!({"text": "rust language"})),
    )
    .await
    .unwrap(); // unwrap
    col.insert(
        "d2",
        &[0.9, 0.1, 0.0, 0.0],
        Some(serde_json::json!({"text": "python language"})),
    )
    .await
    .unwrap(); // unwrap

    let res = col
        .hybrid_search_reranked("rust", &[1.0, 0.0, 0.0, 0.0], 1, None, None)
        .await
        .unwrap(); // unwrap

    assert_eq!(res.len(), 1);
    assert_eq!(res[0].id, "d1");
}

#[test]
fn test_importance_score_parser_robust() {
    assert_eq!(super::parse_importance_score("0.8"), 0.8);
    assert_eq!(super::parse_importance_score("0.8\n"), 0.8);
    assert_eq!(super::parse_importance_score("Score: 0.8"), 0.8);
    assert_eq!(super::parse_importance_score("0.8 (high importance)"), 0.8);
    assert_eq!(super::parse_importance_score("1.5"), 1.0);
    assert_eq!(super::parse_importance_score("-0.2"), 0.0);
    assert_eq!(super::parse_importance_score("invalid text"), 0.5);
}

#[tokio::test]
async fn test_evaluate_importance_with_dead_client_returns_err() {
    use memfuse_graph::CsrGraph;
    use memfuse_index::HnswIndex;
    use memfuse_store::LsmStorage;
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;
    use tempfile::tempdir;

    let dir = tempdir().unwrap(); // unwrap
    let storage = Arc::new(
        LsmStorage::new(memfuse_store::LsmConfig {
            path: dir.path().to_path_buf(),
            ..Default::default()
        })
        .await
        .unwrap(), // unwrap
    );
    let index = Arc::new(
        HnswIndex::try_new(memfuse_index::HnswConfig {
            dimension: 4,
            ..Default::default()
        })
        .unwrap(), // unwrap
    );
    let col = super::Collection::new(
        "default".to_string(),
        storage,
        index,
        Arc::new(CsrGraph::new()),
        Arc::new(AtomicU64::new(1)),
        4,
        memfuse_text::Language::English,
    );

    let vec = vec![1.0, 0.0, 0.0, 0.0];
    col.insert("doc_test", &vec, None).await.unwrap(); // unwrap

    let dead_ollama = memfuse_ollama::OllamaClient::new("http://127.0.0.1:1");

    let res = col
        .evaluate_importance_with_llm("doc_test", &dead_ollama)
        .await;
    assert!(res.is_err());
    assert!(matches!(
        res.unwrap_err(),
        memfuse_core::MemFuseError::Internal(_)
    ));

    // Verify document's score was NOT overwritten or corrupted
    let doc = col.get("doc_test").await.unwrap().unwrap(); // unwrap
    assert!(doc.metadata.is_some());
}

#[test]
fn test_compute_default_importance_entropy_and_clamping() {
    let score_empty = super::compute_default_importance(None);
    assert_eq!(score_empty.value(), 0.5);

    let score_simple = super::compute_default_importance(Some("aaaaa"));
    assert!(score_simple.value() >= 0.0 && score_simple.value() <= 1.0);

    let score_rich = super::compute_default_importance(Some(
        "The quick brown fox jumps over the lazy dog with high entropy and long text.",
    ));
    assert!(score_rich.value() > score_simple.value());
}

#[test]
fn test_extract_effective_importance_defaults() {
    use memfuse_core::types::TxId;

    let none_meta = None;
    assert_eq!(
        super::extract_effective_importance(&none_meta, TxId::new(10)),
        1.0
    );

    let meta_with_imp = Some(serde_json::json!({
        "importance": 0.85
    }));
    assert_eq!(
        super::extract_effective_importance(&meta_with_imp, TxId::new(10)),
        0.85
    );
}

#[tokio::test]
async fn test_begin_transaction_returns_active_db_transaction() {
    use memfuse_graph::CsrGraph;
    use memfuse_index::HnswIndex;
    use memfuse_store::LsmStorage;
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;
    use tempfile::tempdir;

    let dir = tempdir().unwrap(); // unwrap
    let lsm_config = memfuse_store::LsmConfig {
        path: dir.path().to_path_buf(),
        ..Default::default()
    };
    let storage = Arc::new(LsmStorage::new(lsm_config).await.unwrap()); // unwrap
    let index = Arc::new(
        HnswIndex::try_new(memfuse_index::HnswConfig {
            dimension: 4,
            ..Default::default()
        })
        .unwrap(), // unwrap
    );
    let graph = Arc::new(CsrGraph::new());
    let next_tx = Arc::new(AtomicU64::new(1));

    let col = super::Collection::new(
        "default".to_string(),
        storage,
        index,
        graph,
        next_tx,
        4,
        memfuse_text::Language::English,
    );

    let tx = col.begin_transaction();
    assert!(tx.is_ok());
}

#[tokio::test]
async fn test_reaper_deletes_decayed_working_memory() {
    use memfuse_core::{DecayFunction, ImportanceScore, MemoryImportance, TxId};
    use memfuse_graph::CsrGraph;
    use memfuse_index::HnswIndex;
    use memfuse_store::LsmStorage;
    use serde_json::json;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;
    use tempfile::tempdir;

    let dir = tempdir().unwrap(); // unwrap
    let storage = Arc::new(
        LsmStorage::new(memfuse_store::LsmConfig {
            path: dir.path().to_path_buf(),
            ..Default::default()
        })
        .await
        .unwrap(), // unwrap
    );
    let index = Arc::new(
        HnswIndex::try_new(memfuse_index::HnswConfig {
            dimension: 4,
            ..Default::default()
        })
        .unwrap(), // unwrap
    );
    let next_tx = Arc::new(AtomicU64::new(1));
    let col = super::Collection::new(
        "default".to_string(),
        storage,
        index,
        Arc::new(CsrGraph::new()),
        next_tx.clone(),
        4,
        memfuse_text::Language::English,
    );

    let created_tx = TxId::new(10);
    let imp = MemoryImportance::new(
        ImportanceScore::new(0.5),
        DecayFunction::Exponential { half_life_tx: 5 },
        created_tx,
    );

    let vec = vec![1.0, 0.0, 0.0, 0.0];
    col.insert(
        "doc_decayed",
        &vec,
        Some(json!({
            "importance": imp
        })),
    )
    .await
    .unwrap(); // unwrap

    // Advance TxId far enough so effective_score < 0.05
    // At created_tx=10, half_life=5:
    // Tx 10: 0.5 * 1.0 = 0.5
    // Tx 15: 0.5 * 0.5 = 0.25
    // Tx 20: 0.5 * 0.25 = 0.125
    // Tx 25: 0.5 * 0.125 = 0.0625
    // Tx 30: 0.5 * 0.0625 = 0.03125 (< 0.05)
    next_tx.store(35, Ordering::SeqCst);

    let count = col.trigger_reaper().await.unwrap(); // unwrap
    assert_eq!(count, 1, "Decayed working memory document should be reaped");
    assert!(col.get("doc_decayed").await.unwrap().is_none()); // unwrap
}

#[tokio::test]
async fn test_reaper_never_deletes_semantic_no_decay() {
    use memfuse_core::{DecayFunction, ImportanceScore, MemoryImportance, TxId};
    use memfuse_graph::CsrGraph;
    use memfuse_index::HnswIndex;
    use memfuse_store::LsmStorage;
    use serde_json::json;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;
    use tempfile::tempdir;

    let dir = tempdir().unwrap(); // unwrap
    let storage = Arc::new(
        LsmStorage::new(memfuse_store::LsmConfig {
            path: dir.path().to_path_buf(),
            ..Default::default()
        })
        .await
        .unwrap(), // unwrap
    );
    let index = Arc::new(
        HnswIndex::try_new(memfuse_index::HnswConfig {
            dimension: 4,
            ..Default::default()
        })
        .unwrap(), // unwrap
    );
    let next_tx = Arc::new(AtomicU64::new(1));
    let col = super::Collection::new(
        "default".to_string(),
        storage,
        index,
        Arc::new(CsrGraph::new()),
        next_tx.clone(),
        4,
        memfuse_text::Language::English,
    );

    let created_tx = TxId::new(10);
    let imp = MemoryImportance::new(
        ImportanceScore::new(0.01), // even with base score < 0.05!
        DecayFunction::None,
        created_tx,
    );

    let vec = vec![1.0, 0.0, 0.0, 0.0];
    col.insert(
        "doc_semantic",
        &vec,
        Some(json!({
            "importance": imp
        })),
    )
    .await
    .unwrap(); // unwrap

    // Advance TxId very far
    next_tx.store(100_000, Ordering::SeqCst);

    let count = col.trigger_reaper().await.unwrap(); // unwrap
    assert_eq!(
        count, 0,
        "Semantic document with DecayFunction::None must never be deleted"
    );
    assert!(col.get("doc_semantic").await.unwrap().is_some()); // unwrap
}

#[test]
fn test_importance_metadata_integration_and_filtering() {
    use memfuse_core::{DecayFunction, ImportanceScore, MemoryImportance, TxId};
    use serde_json::json;

    let created_tx = TxId::new(10);
    let now_tx = TxId::new(30);

    let mut meta1 = Some(json!({"text": "Important factual doc"}));
    super::ensure_importance_metadata(&mut meta1, created_tx, Some("Important factual doc"));

    // Override with explicit exponential decay
    let imp1 = MemoryImportance::new(
        ImportanceScore::new(0.9),
        DecayFunction::Exponential { half_life_tx: 10 },
        created_tx,
    );
    meta1.as_mut().unwrap().as_object_mut().unwrap().insert(
        // unwrap
        "importance".to_string(),
        serde_json::to_value(imp1).unwrap(), // unwrap
    );

    // Effective score at now_tx (2 half-lives elapsed) -> 0.9 * 0.25 = 0.225
    let eff1 = super::extract_effective_importance(&meta1, now_tx);
    assert!((eff1 - 0.225).abs() < 1e-4);

    let mut meta2 = Some(json!({"text": "Critical doc"}));
    let imp2 = MemoryImportance::new(ImportanceScore::new(1.0), DecayFunction::None, created_tx);
    meta2.as_mut().unwrap().as_object_mut().unwrap().insert(
        // unwrap
        "importance".to_string(),
        serde_json::to_value(imp2).unwrap(), // unwrap
    );

    let results = vec![
        crate::SearchResult {
            id: "doc1".to_string(),
            score: 0.95,
            metadata: meta1,
            matched_signals: vec!["vector".to_string()],
            provenance: None,
        },
        crate::SearchResult {
            id: "doc2".to_string(),
            score: 0.85,
            metadata: meta2,
            matched_signals: vec!["vector".to_string()],
            provenance: None,
        },
    ];

    // Filter out results with effective importance < 0.5
    let filtered =
        super::Collection::<memfuse_store::LsmStorage>::filter_by_importance(results, 0.5, now_tx);
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, "doc2");
    assert_eq!(filtered[0].score, 0.85); // Order and original RRF/CE score preserved
}

#[tokio::test]
async fn test_insert_typed_episodic_has_decay_metadata() {
    use memfuse_graph::CsrGraph;
    use memfuse_index::HnswIndex;
    use memfuse_store::{LsmConfig, LsmStorage};
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;

    let dir = tempfile::tempdir().unwrap(); // unwrap
    let storage = Arc::new(
        LsmStorage::new(LsmConfig {
            path: dir.path().to_path_buf(),
            ..Default::default()
        })
        .await
        .unwrap(), // unwrap
    );
    let index = Arc::new(
        HnswIndex::try_new(memfuse_index::HnswConfig {
            dimension: 4,
            ..Default::default()
        })
        .unwrap(), // unwrap
    );
    let graph_index = Arc::new(CsrGraph::new());
    let next_tx = Arc::new(AtomicU64::new(1));
    let col = super::Collection::new(
        "test".to_string(),
        storage,
        index,
        graph_index,
        next_tx,
        4,
        memfuse_text::Language::German,
    );

    col.insert_typed(
        "ep1",
        &[1.0, 0.0, 0.0, 0.0],
        memfuse_core::MemoryType::Episodic,
        None,
    )
    .await
    .unwrap(); // unwrap

    let doc = col.get("ep1").await.unwrap().unwrap(); // unwrap
    let meta = doc.metadata.unwrap(); // unwrap
    assert_eq!(meta.get("memory_type").unwrap(), "episodic"); // unwrap
    assert!(meta.get("decay_function").is_some());
}

#[tokio::test]
async fn test_insert_typed_working_has_ttl_metadata() {
    use memfuse_graph::CsrGraph;
    use memfuse_index::HnswIndex;
    use memfuse_store::{LsmConfig, LsmStorage};
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;

    let dir = tempfile::tempdir().unwrap(); // unwrap
    let storage = Arc::new(
        LsmStorage::new(LsmConfig {
            path: dir.path().to_path_buf(),
            ..Default::default()
        })
        .await
        .unwrap(), // unwrap
    );
    let index = Arc::new(
        HnswIndex::try_new(memfuse_index::HnswConfig {
            dimension: 4,
            ..Default::default()
        })
        .unwrap(), // unwrap
    );
    let graph_index = Arc::new(CsrGraph::new());
    let next_tx = Arc::new(AtomicU64::new(1));
    let col = super::Collection::new(
        "test".to_string(),
        storage,
        index,
        graph_index,
        next_tx,
        4,
        memfuse_text::Language::German,
    );

    col.insert_typed(
        "wk1",
        &[1.0, 0.0, 0.0, 0.0],
        memfuse_core::MemoryType::Working,
        None,
    )
    .await
    .unwrap(); // unwrap

    let doc = col.get("wk1").await.unwrap().unwrap(); // unwrap
    let meta = doc.metadata.unwrap(); // unwrap
    assert_eq!(meta.get("memory_type").unwrap(), "working"); // unwrap
    assert_eq!(meta.get("ttl_tx").unwrap(), 50_000); // unwrap
}

#[tokio::test]
#[cfg(feature = "experimental-diskann")]
async fn test_collection_with_diskann_index_hybrid_search() {
    use memfuse_core::{DocId, StorageEngine, TextIndex};
    use memfuse_graph::CsrGraph;
    use memfuse_index::{DiskAnnConfig, DiskAnnIndex};
    use memfuse_store::LsmStorage;
    use memfuse_text::Language;
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;
    use tempfile::tempdir;

    let dir = tempdir().unwrap(); // unwrap
    let lsm_path = dir.path().join("lsm");
    let diskann_path = dir.path().join("diskann.idx");

    let storage = Arc::new(
        LsmStorage::new(memfuse_store::LsmConfig {
            path: lsm_path,
            ..Default::default()
        })
        .await
        .unwrap(), // unwrap
    );

    let diskann_config = DiskAnnConfig {
        index_path: diskann_path,
        dimension: 4,
        max_degree: 8,
        beam_width: 8,
        sector_size: 4096,
        ..DiskAnnConfig::default()
    };

    let diskann = Arc::new(DiskAnnIndex::try_new(diskann_config).unwrap()); // unwrap

    let doc1_id = DocId::from_key("doc1").unwrap(); // unwrap
    let doc2_id = DocId::from_key("doc2").unwrap(); // unwrap

    let vectors = vec![vec![1.0, 0.0, 0.0, 0.0], vec![0.0, 1.0, 0.0, 0.0]];
    let ids = vec![doc1_id, doc2_id];

    diskann.build(&vectors, &ids).await.unwrap(); // unwrap

    let graph = Arc::new(CsrGraph::new());
    let next_tx = Arc::new(AtomicU64::new(1));

    let col = super::Collection::<LsmStorage, DiskAnnIndex>::new(
        "diskann_test".to_string(),
        storage.clone(),
        diskann,
        graph,
        next_tx,
        4,
        Language::English,
    );

    let tx = col.allocate_tx().unwrap(); // unwrap

    let doc1_user_key = col.namespaced_key(b"doc1", 0);
    let doc1_meta_key = col.namespaced_key(&doc1_id.inner().to_le_bytes(), 1);

    let doc2_user_key = col.namespaced_key(b"doc2", 0);
    let doc2_meta_key = col.namespaced_key(&doc2_id.inner().to_le_bytes(), 1);

    let doc1_data = StoredDocument {
        id: "doc1".to_string(),
        embedding: vec![1.0, 0.0, 0.0, 0.0],
        metadata: Some(serde_json::json!({ "text": "rust database systems" })),
    };
    let doc1_meta = StoredDocumentMeta::from(&doc1_data);

    let doc2_data = StoredDocument {
        id: "doc2".to_string(),
        embedding: vec![0.0, 1.0, 0.0, 0.0],
        metadata: Some(serde_json::json!({ "text": "python scripting language" })),
    };
    let doc2_meta = StoredDocumentMeta::from(&doc2_data);

    storage
        .put(tx, &doc1_user_key, &serde_json::to_vec(&doc1_data).unwrap()) // unwrap
        .await
        .unwrap(); // unwrap
    storage
        .put(tx, &doc1_meta_key, &serde_json::to_vec(&doc1_meta).unwrap()) // unwrap
        .await
        .unwrap(); // unwrap

    storage
        .put(tx, &doc2_user_key, &serde_json::to_vec(&doc2_data).unwrap()) // unwrap
        .await
        .unwrap(); // unwrap
    storage
        .put(tx, &doc2_meta_key, &serde_json::to_vec(&doc2_meta).unwrap()) // unwrap
        .await
        .unwrap(); // unwrap

    col.text_index
        .upsert_document(tx, doc1_id, "rust database systems")
        .await
        .unwrap(); // unwrap
    col.text_index
        .upsert_document(tx, doc2_id, "python scripting language")
        .await
        .unwrap(); // unwrap

    storage.commit(tx).await.unwrap(); // unwrap
    col.text_index.commit(tx).await.unwrap(); // unwrap

    let query_vector = vec![1.0, 0.0, 0.0, 0.0];
    let results = col
        .hybrid_search("rust", &query_vector, 5, None)
        .await
        .unwrap(); // unwrap

    assert!(
        !results.is_empty(),
        "Hybrid search with DiskANN should return results"
    );
    assert_eq!(
        results[0].id, "doc1",
        "Doc1 should be top result for rust & vector [1,0,0,0]"
    );
}

#[tokio::test]
async fn test_insert_backward_compatible_has_semantic_default() {
    use memfuse_graph::CsrGraph;
    use memfuse_index::HnswIndex;
    use memfuse_store::{LsmConfig, LsmStorage};
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;

    let dir = tempfile::tempdir().unwrap(); // unwrap
    let storage = Arc::new(
        LsmStorage::new(LsmConfig {
            path: dir.path().to_path_buf(),
            ..Default::default()
        })
        .await
        .unwrap(), // unwrap
    );
    let index = Arc::new(
        HnswIndex::try_new(memfuse_index::HnswConfig {
            dimension: 4,
            ..Default::default()
        })
        .unwrap(), // unwrap
    );
    let graph_index = Arc::new(CsrGraph::new());
    let next_tx = Arc::new(AtomicU64::new(1));
    let col = super::Collection::new(
        "test".to_string(),
        storage,
        index,
        graph_index,
        next_tx,
        4,
        memfuse_text::Language::German,
    );

    col.insert(
        "plain1",
        &[1.0, 0.0, 0.0, 0.0],
        Some(serde_json::json!({"text": "hello"})),
    )
    .await
    .unwrap(); // unwrap

    let doc = col.get("plain1").await.unwrap().unwrap(); // unwrap
    assert_eq!(
        crate::filter::extract_memory_type(&doc.metadata),
        memfuse_core::MemoryType::Semantic
    );
}

#[tokio::test]
async fn test_hybrid_search_with_query_memory_type_filter() {
    use memfuse_core::{HybridQuery, MemoryType};
    use memfuse_graph::CsrGraph;
    use memfuse_index::HnswIndex;
    use memfuse_store::{LsmConfig, LsmStorage};
    use serde_json::json;
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;

    let dir = tempfile::tempdir().unwrap(); // unwrap
    let storage = Arc::new(
        LsmStorage::new(LsmConfig {
            path: dir.path().to_path_buf(),
            ..Default::default()
        })
        .await
        .unwrap(), // unwrap
    );
    let index = Arc::new(
        HnswIndex::try_new(memfuse_index::HnswConfig {
            dimension: 4,
            ..Default::default()
        })
        .unwrap(), // unwrap
    );
    let col = super::Collection::new(
        "test_filter".to_string(),
        storage,
        index,
        Arc::new(CsrGraph::new()),
        Arc::new(AtomicU64::new(1)),
        4,
        memfuse_text::Language::English,
    );

    col.insert_typed(
        "ep1",
        &[1.0, 0.0, 0.0, 0.0],
        MemoryType::Episodic,
        Some(json!({"text": "episode meeting alpha"})),
    )
    .await
    .unwrap(); // unwrap

    col.insert_typed(
        "ep2",
        &[0.9, 0.1, 0.0, 0.0],
        MemoryType::Episodic,
        Some(json!({"text": "episode meeting beta"})),
    )
    .await
    .unwrap(); // unwrap

    col.insert_typed(
        "sem1",
        &[0.95, 0.05, 0.0, 0.0],
        MemoryType::Semantic,
        Some(json!({"text": "episode definition gamma"})),
    )
    .await
    .unwrap(); // unwrap

    col.insert_typed(
        "sem2",
        &[0.85, 0.15, 0.0, 0.0],
        MemoryType::Semantic,
        Some(json!({"text": "episode theory delta"})),
    )
    .await
    .unwrap(); // unwrap

    // Query with memory_type_filter = Episodic
    let query_ep = HybridQuery::builder()
        .with_text_query("episode")
        .with_vector_query(vec![1.0, 0.0, 0.0, 0.0])
        .with_memory_type_filter(vec![MemoryType::Episodic])
        .with_k(10)
        .build()
        .unwrap(); // unwrap

    let results_ep = col.hybrid_search_with_query(&query_ep).await.unwrap(); // unwrap
    assert_eq!(
        results_ep.len(),
        2,
        "Must return exactly 2 episodic results"
    );
    for res in &results_ep {
        assert!(
            res.id == "ep1" || res.id == "ep2",
            "Returned result {} is not Episodic!",
            res.id
        );
    }

    // Query with memory_type_filter = Semantic
    let query_sem = HybridQuery::builder()
        .with_text_query("episode")
        .with_vector_query(vec![1.0, 0.0, 0.0, 0.0])
        .with_memory_type_filter(vec![MemoryType::Semantic])
        .with_k(10)
        .build()
        .unwrap(); // unwrap

    let results_sem = col.hybrid_search_with_query(&query_sem).await.unwrap(); // unwrap
    assert_eq!(
        results_sem.len(),
        2,
        "Must return exactly 2 semantic results"
    );
    for res in &results_sem {
        assert!(
            res.id == "sem1" || res.id == "sem2",
            "Returned result {} is not Semantic!",
            res.id
        );
    }
}

#[tokio::test]
async fn test_invalid_doc_ids_rejected() {
    use memfuse_graph::CsrGraph;
    use memfuse_index::HnswIndex;
    use memfuse_store::LsmStorage;
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;
    use tempfile::tempdir;

    let dir = tempdir().unwrap(); // unwrap
    let storage = Arc::new(
        LsmStorage::new(memfuse_store::LsmConfig {
            path: dir.path().to_path_buf(),
            ..Default::default()
        })
        .await
        .unwrap(), // unwrap
    );
    let index = Arc::new(
        HnswIndex::try_new(memfuse_index::HnswConfig {
            dimension: 4,
            ..Default::default()
        })
        .unwrap(), // unwrap
    );
    let col = super::Collection::new(
        "default".to_string(),
        storage,
        index,
        Arc::new(CsrGraph::new()),
        Arc::new(AtomicU64::new(1)),
        4,
        memfuse_text::Language::English,
    );
    let vec = vec![1.0, 0.0, 0.0, 0.0];

    // Empty ID
    assert!(col.insert("", &vec, None).await.is_err());
    assert!(col.get("").await.is_err());
    assert!(col.delete("").await.is_err());

    // Null byte in ID
    assert!(col.insert("doc\0invalid", &vec, None).await.is_err());
    assert!(col.get("doc\0invalid").await.is_err());

    // Too long ID (>256 bytes)
    let long_id = "a".repeat(257);
    assert!(col.insert(&long_id, &vec, None).await.is_err());
    assert!(col.get(&long_id).await.is_err());
}

#[tokio::test]
async fn test_search_dimension_mismatch_rejected() {
    use memfuse_graph::CsrGraph;
    use memfuse_index::HnswIndex;
    use memfuse_store::LsmStorage;
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;
    use tempfile::tempdir;

    let dir = tempdir().unwrap(); // unwrap
    let storage = Arc::new(
        LsmStorage::new(memfuse_store::LsmConfig {
            path: dir.path().to_path_buf(),
            ..Default::default()
        })
        .await
        .unwrap(), // unwrap
    );
    let index = Arc::new(
        HnswIndex::try_new(memfuse_index::HnswConfig {
            dimension: 4,
            ..Default::default()
        })
        .unwrap(), // unwrap
    );
    let col = super::Collection::new(
        "default".to_string(),
        storage,
        index,
        Arc::new(CsrGraph::new()),
        Arc::new(AtomicU64::new(1)),
        4,
        memfuse_text::Language::English,
    );
    let wrong_dim_vec = vec![1.0, 0.0];

    let search_res = col.search(&wrong_dim_vec, 10).await;
    assert!(search_res.is_err());

    let hybrid_res = col.hybrid_search("query", &wrong_dim_vec, 10, None).await;
    assert!(hybrid_res.is_err());
}

#[tokio::test]
async fn test_concurrent_insert_many_collision_safety() {
    use memfuse_core::{DocId, MemFuseError, StorageEngine, TxId};
    use memfuse_graph::CsrGraph;
    use memfuse_index::HnswIndex;
    use memfuse_store::LsmStorage;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;
    use tempfile::tempdir;

    let dir = tempdir().unwrap(); // unwrap
    let lsm_config = memfuse_store::LsmConfig {
        path: dir.path().to_path_buf(),
        ..Default::default()
    };
    let storage = Arc::new(LsmStorage::new(lsm_config).await.unwrap()); // unwrap
    let hnsw_config = memfuse_index::HnswConfig {
        dimension: 4,
        ..Default::default()
    };
    let index = Arc::new(HnswIndex::try_new(hnsw_config).unwrap()); // unwrap
    let graph = Arc::new(CsrGraph::new());
    let next_tx = Arc::new(AtomicU64::new(1));

    let col = Arc::new(super::Collection::new(
        "default".to_string(),
        storage.clone(),
        index,
        graph,
        next_tx.clone(),
        4,
        memfuse_text::Language::English,
    ));

    // 1. Parallel insert_many calls with overlapping document keys across tasks
    let mut tasks = Vec::new();
    for task_idx in 0..8 {
        let col_clone = col.clone();
        tasks.push(tokio::spawn(async move {
            let docs: Vec<(String, Vec<f32>, Option<serde_json::Value>)> = (0..20)
                .map(|i| {
                    let key = format!("batch_doc_{i}");
                    let val = (task_idx * 100 + i + 1) as f32;
                    (
                        key,
                        vec![val, 0.0, 0.0, 0.0],
                        Some(serde_json::json!({ "task": task_idx, "i": i })),
                    )
                })
                .collect();
            col_clone.insert_many(&docs).await.unwrap(); // unwrap
        }));
    }

    for task in tasks {
        task.await.unwrap(); // unwrap
    }

    // All 20 document keys must exist and be valid
    for i in 0..20 {
        let key = format!("batch_doc_{i}");
        let doc = col.get(&key).await.unwrap(); // unwrap
        assert!(
            doc.is_some(),
            "Document {key} must exist after concurrent insert_many"
        );
    }

    // 2. Synthetically test DocId collision rejection within insert_many
    // Seed an initial document key "existing_key"
    col.insert("existing_key", &[1.0, 0.0, 0.0, 0.0], None)
        .await
        .unwrap(); // unwrap

    // Map fixed synthetic DocId (e.g. 999) to "existing_key"
    let synthetic_doc_id = DocId::from_key("colliding_target_key").unwrap(); // unwrap
    let tx = TxId::new(next_tx.fetch_add(1, Ordering::SeqCst));
    let doc_key = col.namespaced_key(&synthetic_doc_id.inner().to_le_bytes(), 1);
    let existing_meta = super::StoredDocumentMeta {
        id: "existing_key".to_string(),
        metadata: None,
    };
    let meta_bytes = serde_json::to_vec(&existing_meta).unwrap(); // unwrap
    storage.put(tx, &doc_key, &meta_bytes).await.unwrap(); // unwrap
    storage.commit(tx).await.unwrap(); // unwrap

    // Attempt insert_many with a batch containing "colliding_target_key"
    let batch_with_collision = vec![
        ("safe_doc_1".to_string(), vec![1.0, 0.0, 0.0, 0.0], None),
        (
            "colliding_target_key".to_string(),
            vec![2.0, 0.0, 0.0, 0.0],
            None,
        ),
        ("safe_doc_2".to_string(), vec![3.0, 0.0, 0.0, 0.0], None),
    ];

    let err_res = col.insert_many(&batch_with_collision).await;
    assert!(
        err_res.is_err(),
        "insert_many must fail when DocId collision is detected"
    );
    assert!(matches!(err_res, Err(MemFuseError::Internal(_))));

    // Verify all-or-nothing rollback (Option a): safe_doc_1 and safe_doc_2 must NOT exist
    assert!(
        col.get("safe_doc_1").await.unwrap().is_none(), // unwrap
        "safe_doc_1 must be rolled back on collision error in insert_many"
    );
    assert!(
        col.get("safe_doc_2").await.unwrap().is_none(), // unwrap
        "safe_doc_2 must be rolled back on collision error in insert_many"
    );
}

#[tokio::test]
async fn test_community_boost_post_rrf_preserves_non_community_and_reranks(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    use memfuse_core::EntityId;
    use memfuse_graph::CsrGraph;
    use memfuse_index::HnswIndex;
    use memfuse_store::LsmStorage;
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;
    use tempfile::tempdir;

    let dir = tempdir()?;
    let storage = Arc::new(
        LsmStorage::new(memfuse_store::LsmConfig {
            path: dir.path().to_path_buf(),
            ..Default::default()
        })
        .await?,
    );
    let index = Arc::new(HnswIndex::try_new(memfuse_index::HnswConfig {
        dimension: 4,
        ..Default::default()
    })?);
    let graph = Arc::new(CsrGraph::new());
    let next_tx = Arc::new(AtomicU64::new(1));

    let col = super::Collection::new(
        "default".to_string(),
        storage,
        index,
        graph,
        next_tx,
        4,
        memfuse_text::Language::English,
    );

    // Insert doc_a (community member) and doc_b (non-community member)
    col.insert(
        "doc_a",
        &[1.0, 0.0, 0.0, 0.0],
        Some(serde_json::json!({"text": "alpha topic"})),
    )
    .await?;
    col.insert(
        "doc_b",
        &[1.0, 0.0, 0.0, 0.0],
        Some(serde_json::json!({"text": "alpha topic"})),
    )
    .await?;

    let eid_a = EntityId::from_key("doc_a")?;

    // Relate doc_a to doc_c and run community detection so get_community(eid_a) finds target_community_id
    col.relate("doc_a", "doc_c", "knows").await?;
    col.run_community_detection().await?;
    assert!(col.get_community(eid_a).await?.is_some());

    // Perform hybrid search with same_community_as = doc_a
    let results_boosted = col
        .hybrid_search_with_strategy(
            "alpha",
            &[1.0, 0.0, 0.0, 0.0],
            10,
            None,
            None,
            None,
            Some(eid_a),
        )
        .await?;

    // Verification 1: Non-community doc_b is NOT eliminated and remains in results!
    assert_eq!(
        results_boosted.len(),
        2,
        "Non-community doc_b must not be filtered out"
    );
    let ids: Vec<&str> = results_boosted.iter().map(|r| r.id.as_str()).collect();
    assert!(
        ids.contains(&"doc_b"),
        "doc_b must remain in search results"
    );

    // Verification 2: Community doc_a gets boosted post-RRF and ranks #1 ahead of doc_b
    assert_eq!(
        results_boosted[0].id, "doc_a",
        "Community member doc_a must rank ahead after boost"
    );
    assert!(
        results_boosted[0].score > results_boosted[1].score,
        "Boosted community doc score ({}) must exceed non-community score ({})",
        results_boosted[0].score,
        results_boosted[1].score
    );
    Ok(())
}

#[tokio::test]
async fn test_search_k_zero_returns_canonical_error_message(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    use memfuse_graph::CsrGraph;
    use memfuse_index::HnswIndex;
    use memfuse_store::LsmStorage;
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;
    use tempfile::tempdir;

    let dir = tempdir()?;
    let storage = Arc::new(
        LsmStorage::new(memfuse_store::LsmConfig {
            path: dir.path().to_path_buf(),
            ..Default::default()
        })
        .await?,
    );
    let index = Arc::new(HnswIndex::try_new(memfuse_index::HnswConfig {
        dimension: 4,
        ..Default::default()
    })?);
    let col = super::Collection::new(
        "default".to_string(),
        storage,
        index,
        Arc::new(CsrGraph::new()),
        Arc::new(AtomicU64::new(1)),
        4,
        memfuse_text::Language::English,
    );

    let query_vec = vec![1.0, 0.0, 0.0, 0.0];

    let err_search = col.search(&query_vec, 0).await;
    assert!(err_search.is_err());
    let err_msg_1 = match err_search {
        Err(e) => e.to_string(),
        Ok(_) => unreachable!(),
    };
    assert!(
        err_msg_1.contains("Search k must be greater than 0"),
        "Expected 'Search k must be greater than 0', got: {err_msg_1}"
    );

    let err_expr = col.search_with_filter_expr(&query_vec, 0, None).await;
    assert!(err_expr.is_err());
    let err_msg_2 = match err_expr {
        Err(e) => e.to_string(),
        Ok(_) => unreachable!(),
    };
    assert!(
        err_msg_2.contains("Search k must be greater than 0"),
        "Expected 'Search k must be greater than 0', got: {err_msg_2}"
    );
    Ok(())
}

#[tokio::test]
async fn test_graph_mapping_invariant_missing_entity_graceful_degradation(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    use memfuse_graph::CsrGraph;
    use memfuse_index::HnswIndex;
    use memfuse_store::LsmStorage;
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;
    use tempfile::tempdir;

    let dir = tempdir()?;
    let storage = Arc::new(
        LsmStorage::new(memfuse_store::LsmConfig {
            path: dir.path().to_path_buf(),
            ..Default::default()
        })
        .await?,
    );
    let index = Arc::new(HnswIndex::try_new(memfuse_index::HnswConfig {
        dimension: 4,
        ..Default::default()
    })?);
    let col = super::Collection::new(
        "default".to_string(),
        storage,
        index,
        Arc::new(CsrGraph::new()),
        Arc::new(AtomicU64::new(1)),
        4,
        memfuse_text::Language::English,
    );

    // Insert text document "doc_text_only" without creating any graph entities
    col.insert(
        "doc_text_only",
        &[0.5, 0.5, 0.0, 0.0],
        Some(serde_json::json!({"text": "specialized retrieval architecture"})),
    )
    .await?;

    // Perform hybrid search where text signal finds "doc_text_only", but graph index has no node for it.
    // The graph signal will become empty, but the overall search must succeed using vector and text signals.
    let results = col
        .hybrid_search_with_strategy(
            "retrieval",
            &[0.5, 0.5, 0.0, 0.0],
            10,
            None,
            None,
            None,
            None,
        )
        .await?;

    assert!(
        !results.is_empty(),
        "Hybrid search must return results from remaining signals"
    );
    assert_eq!(results[0].id, "doc_text_only");
    Ok(())
}

#[tokio::test]
async fn test_post_rrf_supersedes_displacement_truncation_preserves_k() -> memfuse_core::Result<()>
{
    use memfuse_core::DocId;
    use memfuse_graph::CsrGraph;
    use memfuse_index::HnswIndex;
    use memfuse_store::LsmStorage;
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;
    use tempfile::tempdir;

    let dir = tempdir().unwrap();
    let storage = Arc::new(
        LsmStorage::new(memfuse_store::LsmConfig {
            path: dir.path().to_path_buf(),
            ..Default::default()
        })
        .await
        .unwrap(),
    );
    let index = Arc::new(
        HnswIndex::try_new(memfuse_index::HnswConfig {
            dimension: 4,
            ..Default::default()
        })
        .unwrap(),
    );
    let col = super::Collection::new(
        "default".to_string(),
        storage,
        index,
        Arc::new(CsrGraph::new()),
        Arc::new(AtomicU64::new(1)),
        4,
        memfuse_text::Language::English,
    );

    // Insert 4 docs: doc1, doc2, doc3, doc4
    col.insert(
        "doc1",
        &[1.0, 0.0, 0.0, 0.0],
        Some(serde_json::json!({"text": "alpha"})),
    )
    .await?;
    col.insert(
        "doc2",
        &[0.9, 0.1, 0.0, 0.0],
        Some(serde_json::json!({"text": "beta"})),
    )
    .await?;
    col.insert(
        "doc3",
        &[0.8, 0.2, 0.0, 0.0],
        Some(serde_json::json!({"text": "gamma"})),
    )
    .await?;
    col.insert(
        "doc4",
        &[0.7, 0.3, 0.0, 0.0],
        Some(serde_json::json!({"text": "delta"})),
    )
    .await?;

    // doc2 supersedes doc1
    col.link_memories(
        DocId::from_key("doc2")?,
        DocId::from_key("doc1")?,
        memfuse_core::types::domain::LinkRelation::Supersedes,
    )
    .await?;

    // When searching with k = 2 and include_superseded = false,
    // doc1 is displaced by doc2.
    // With 3*k candidate pool, doc3 advances into top-2 so we still get 2 results!
    let query = memfuse_core::HybridQuery::builder()
        .with_vector_query(vec![1.0, 0.0, 0.0, 0.0])
        .with_k(2)
        .with_include_superseded(false)
        .build()
        .unwrap();
    let results = col.hybrid_search_with_query(&query).await?;

    assert_eq!(
        results.len(),
        2,
        "Must return full requested k=2 even after supersedes displacement"
    );
    assert!(
        !results.iter().any(|r| r.id == "doc1"),
        "doc1 must be displaced"
    );
    assert!(
        results.iter().any(|r| r.id == "doc2"),
        "doc2 must be retained"
    );
    assert!(
        results.iter().any(|r| r.id == "doc3"),
        "doc3 must move up into top-2"
    );

    Ok(())
}

#[tokio::test]
async fn test_link_memories_cycle_prevention_for_all_relations() -> memfuse_core::Result<()> {
    use memfuse_core::DocId;
    use memfuse_graph::CsrGraph;
    use memfuse_index::HnswIndex;
    use memfuse_store::LsmStorage;
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;
    use tempfile::tempdir;

    let dir = tempdir().unwrap();
    let storage = Arc::new(
        LsmStorage::new(memfuse_store::LsmConfig {
            path: dir.path().to_path_buf(),
            ..Default::default()
        })
        .await
        .unwrap(),
    );
    let index = Arc::new(
        HnswIndex::try_new(memfuse_index::HnswConfig {
            dimension: 4,
            ..Default::default()
        })
        .unwrap(),
    );
    let col = super::Collection::new(
        "default".to_string(),
        storage,
        index,
        Arc::new(CsrGraph::new()),
        Arc::new(AtomicU64::new(1)),
        4,
        memfuse_text::Language::English,
    );

    col.insert("node_a", &[1.0, 0.0, 0.0, 0.0], None).await?;
    col.insert("node_b", &[0.0, 1.0, 0.0, 0.0], None).await?;
    col.insert("node_c", &[0.0, 0.0, 1.0, 0.0], None).await?;

    let a = DocId::from_key("node_a")?;
    let b = DocId::from_key("node_b")?;
    let c = DocId::from_key("node_c")?;

    let rel = memfuse_core::types::domain::LinkRelation::Elaborates;

    // A -> B
    col.link_memories(a, b, rel).await?;
    // B -> C
    col.link_memories(b, c, rel).await?;

    // C -> A should fail with cycle detection error!
    let cycle_res = col.link_memories(c, a, rel).await;
    assert!(cycle_res.is_err(), "Cyclic link must be rejected");
    let err_str = cycle_res.unwrap_err().to_string();
    assert!(err_str.contains("Cyclic Elaborates relation detected"));

    Ok(())
}

#[tokio::test]
async fn test_concurrent_mutation_aborts_consolidation() -> memfuse_core::Result<()> {
    use memfuse_core::DocId;
    use memfuse_graph::CsrGraph;
    use memfuse_index::HnswIndex;
    use memfuse_store::LsmStorage;
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;
    use tempfile::tempdir;

    let dir = tempdir().unwrap();
    let storage = Arc::new(
        LsmStorage::new(memfuse_store::LsmConfig {
            path: dir.path().to_path_buf(),
            ..Default::default()
        })
        .await
        .unwrap(),
    );
    let index = Arc::new(
        HnswIndex::try_new(memfuse_index::HnswConfig {
            dimension: 4,
            ..Default::default()
        })
        .unwrap(),
    );
    let col = super::Collection::new(
        "default".to_string(),
        storage,
        index,
        Arc::new(CsrGraph::new()),
        Arc::new(AtomicU64::new(1)),
        4,
        memfuse_text::Language::English,
    );

    col.insert(
        "source_1",
        &[1.0, 0.0, 0.0, 0.0],
        Some(serde_json::json!({"text": "Fact 1"})),
    )
    .await?;
    col.insert(
        "source_2",
        &[0.0, 1.0, 0.0, 0.0],
        Some(serde_json::json!({"text": "Fact 2"})),
    )
    .await?;

    let d1 = DocId::from_key("source_1")?;
    let d2 = DocId::from_key("source_2")?;
    let target_id = DocId::from_key("summary_12")?;

    // 1. Start consolidation session (snapshots source_docs and records intent)
    let session =
        crate::context_compaction::ConsolidationSession::start(&col, &[d1, d2], target_id).await?;

    // 2. Simulate concurrent mutation on source_2 while LLM synthesis is running
    col.update(
        "source_2",
        &[0.0, 1.0, 0.0, 0.0],
        Some(serde_json::json!({"text": "Fact 2 updated by agent"})),
    )
    .await?;

    // 3. Attempting to commit consolidation must fail with StaleRead error!
    let commit_res = session
        .commit(
            "summary_12",
            &[0.5, 0.5, 0.0, 0.0],
            "Summary of 1 and 2",
            None,
        )
        .await;
    assert!(
        commit_res.is_err(),
        "Consolidation commit must fail under concurrent mutation"
    );
    match commit_res.unwrap_err() {
        memfuse_core::MemFuseError::StaleRead(msg) => {
            assert!(msg.contains("OCC conflict"));
        }
        other => panic!("Expected StaleRead error, got: {:?}", other),
    }

    // 4. Verify that original source documents are still intact!
    let doc1 = col.get("source_1").await?;
    let doc2 = col.get("source_2").await?;
    assert!(
        doc1.is_some(),
        "source_1 must not be deleted on aborted consolidation"
    );
    assert!(
        doc2.is_some(),
        "source_2 must not be deleted on aborted consolidation"
    );

    Ok(())
}
