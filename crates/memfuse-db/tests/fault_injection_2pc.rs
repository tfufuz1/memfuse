// FILE-CONTEXT: 2PC Fault-Injection Test Suite & Multi-Engine Rollback Verification
// ZWECK: Prüft atomare 2PC-Transaktions-Kompensation und Crash-Recovery (repair_on_open) über alle 4 Sub-Engines.
// STAND: TS:2026-08-31T22:30:00Z (SESSION: 0dcb9f3b)

use async_trait::async_trait;
use memfuse_core::{
    DocId, EntityId, MemFuseError, Result, ScoredDocument, StorageEngine, StorageStats, TxId,
    VectorIndex, VectorIndexStats,
};
use memfuse_db::{MemFuse, MemFuseConfig};
use memfuse_graph::CsrGraph;
use memfuse_index::{HnswConfig, HnswIndex};
use memfuse_store::{LsmConfig, LsmStorage};
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tempfile::tempdir;

// ============================================================================
// FAULT INJECTION TEST DOUBLES
// ============================================================================

/// Fault-Injecting StorageEngine Proxy wrapping LsmStorage.
pub struct FaultyStorage {
    pub inner: Arc<LsmStorage>,
    pub commit_count: Arc<parking_lot::Mutex<HashMap<TxId, usize>>>,
    pub fail_on_commit_index: Arc<parking_lot::Mutex<Option<usize>>>,
    pub fail_put_text: AtomicBool,
    pub fail_put_graph: AtomicBool,
    pub fail_all_commits: AtomicBool,
}

impl FaultyStorage {
    pub fn new(inner: Arc<LsmStorage>) -> Self {
        Self {
            inner,
            commit_count: Arc::new(parking_lot::Mutex::new(HashMap::new())),
            fail_on_commit_index: Arc::new(parking_lot::Mutex::new(None)),
            fail_put_text: AtomicBool::new(false),
            fail_put_graph: AtomicBool::new(false),
            fail_all_commits: AtomicBool::new(false),
        }
    }
}

#[async_trait]
impl StorageEngine for FaultyStorage {
    async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        self.inner.get(key).await
    }

    async fn get_at_seq(&self, key: &[u8], seq: u64) -> Result<Option<Vec<u8>>> {
        self.inner.get_at_seq(key, seq).await
    }

    async fn put(&self, tx_id: TxId, key: &[u8], value: &[u8]) -> Result<()> {
        if self.fail_put_text.load(Ordering::SeqCst) && key.starts_with(b"__txt:") {
            return Err(MemFuseError::Transaction(
                "INJECTED FAULT: BM25/Text staging storage put failure".into(),
            ));
        }
        if self.fail_put_graph.load(Ordering::SeqCst) && key.starts_with(b"__graph:") {
            return Err(MemFuseError::Transaction(
                "INJECTED FAULT: CSR-Graph staging storage put failure".into(),
            ));
        }
        self.inner.put(tx_id, key, value).await
    }

    async fn delete(&self, tx_id: TxId, key: &[u8]) -> Result<()> {
        self.inner.delete(tx_id, key).await
    }

    async fn commit(&self, tx_id: TxId) -> Result<()> {
        if self.fail_all_commits.load(Ordering::SeqCst) {
            return Err(MemFuseError::Transaction(
                "INJECTED FAULT: LSM commit failure (all)".into(),
            ));
        }

        let count = {
            let mut map = self.commit_count.lock();
            let c = map.entry(tx_id).or_insert(0);
            *c += 1;
            *c
        };

        {
            let target_idx = self.fail_on_commit_index.lock();
            if let Some(target) = *target_idx {
                if target == count {
                    return Err(MemFuseError::Transaction(format!(
                        "INJECTED FAULT: Commit #{} failed for tx {}",
                        count, tx_id
                    )));
                }
            }
        }

        self.inner.commit(tx_id).await
    }

    async fn rollback(&self, tx_id: TxId) -> Result<()> {
        self.inner.rollback(tx_id).await
    }

    async fn rollback_to_tx(&self, tx_id: TxId) -> Result<()> {
        self.inner.rollback_to_tx(tx_id).await
    }

    async fn flush(&self) -> Result<()> {
        self.inner.flush().await
    }

    async fn stats(&self) -> Result<StorageStats> {
        self.inner.stats().await
    }

    async fn last_seq_no(&self) -> Result<u64> {
        self.inner.last_seq_no().await
    }

    async fn last_tx_id(&self) -> Result<TxId> {
        self.inner.last_tx_id().await
    }

    async fn pin_checkpoint(&self, seq_no: u64) -> Result<()> {
        self.inner.pin_checkpoint(seq_no).await
    }

    async fn unpin_checkpoint(&self, seq_no: u64) -> Result<()> {
        self.inner.unpin_checkpoint(seq_no).await
    }

    async fn scan_prefix(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        self.inner.scan_prefix(prefix).await
    }

    async fn scan_prefix_at(&self, prefix: &[u8], seq_no: u64) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        self.inner.scan_prefix_at(prefix, seq_no).await
    }

    async fn scan(
        &self,
        start: std::ops::Bound<&[u8]>,
        end: std::ops::Bound<&[u8]>,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        self.inner.scan(start, end).await
    }
}

/// Fault-Injecting VectorIndex Proxy wrapping HnswIndex.
pub struct FaultyVectorIndex {
    pub inner: Arc<HnswIndex>,
    pub fail_insert: AtomicBool,
    pub fail_commit: AtomicBool,
}

impl FaultyVectorIndex {
    pub fn new(inner: Arc<HnswIndex>) -> Self {
        Self {
            inner,
            fail_insert: AtomicBool::new(false),
            fail_commit: AtomicBool::new(false),
        }
    }
}

#[async_trait]
impl VectorIndex for FaultyVectorIndex {
    async fn insert(&self, tx: TxId, id: DocId, embedding: &[f32]) -> Result<()> {
        if self.fail_insert.load(Ordering::SeqCst) {
            return Err(MemFuseError::Transaction(
                "INJECTED FAULT: HNSW vector staging insert failure".into(),
            ));
        }
        self.inner.insert(tx, id, embedding).await
    }

    async fn search(&self, query: &[f32], k: usize) -> Result<Vec<ScoredDocument>> {
        self.inner.search(query, k).await
    }

    async fn search_at(&self, query: &[f32], k: usize, seq_no: u64) -> Result<Vec<ScoredDocument>> {
        self.inner.search_at(query, k, seq_no).await
    }

    async fn search_filtered(
        &self,
        query: &[f32],
        k: usize,
        filter: Option<&(dyn Fn(DocId) -> bool + Send + Sync)>,
    ) -> Result<Vec<ScoredDocument>> {
        self.inner.search_filtered(query, k, filter).await
    }

    async fn delete(&self, tx: TxId, id: DocId) -> Result<()> {
        self.inner.delete(tx, id).await
    }

    async fn commit(&self, tx: TxId) -> Result<()> {
        if self.fail_commit.load(Ordering::SeqCst) {
            return Err(MemFuseError::Transaction(
                "INJECTED FAULT: HNSW vector index commit failure".into(),
            ));
        }
        self.inner.commit(tx).await
    }

    async fn rollback(&self, tx: TxId) -> Result<()> {
        self.inner.rollback(tx).await
    }

    async fn rollback_to_tx(&self, tx_id: TxId) -> Result<()> {
        self.inner.rollback_to_tx(tx_id).await
    }

    async fn last_tx_id(&self) -> Result<u64> {
        self.inner.last_tx_id().await
    }

    async fn len(&self) -> usize {
        self.inner.len().await
    }

    async fn stats(&self) -> Result<VectorIndexStats> {
        self.inner.stats().await
    }

    async fn all_doc_ids(&self) -> Result<Vec<DocId>> {
        self.inner.all_doc_ids().await
    }
}

// Helper to construct namespaced keys matching Collection's key schema
fn namespaced_key_helper(col_name: &str, key: &[u8], key_type: u8) -> Vec<u8> {
    if col_name == "default" {
        match key_type {
            0 => key.to_vec(),
            1 => [b"__docid:".as_slice(), key].concat(),
            2 => [b"__rel:".as_slice(), key].concat(),
            3 => [b"__tx_intent:".as_slice(), key].concat(),
            _ => key.to_vec(),
        }
    } else {
        let mut k = Vec::new();
        k.extend_from_slice(format!("__col:{}:\x00", col_name).as_bytes());
        k.push(key_type);
        k.extend_from_slice(key);
        k
    }
}

// Helper to create a test collection with FaultyStorage and FaultyVectorIndex
async fn create_faulty_collection(
    dir_path: PathBuf,
    dim: usize,
) -> (
    memfuse_db::Collection<FaultyStorage, FaultyVectorIndex>,
    Arc<FaultyStorage>,
    Arc<FaultyVectorIndex>,
    Arc<LsmStorage>,
) {
    let lsm_config = LsmConfig {
        path: dir_path,
        ..Default::default()
    };
    let real_storage = Arc::new(LsmStorage::new(lsm_config).await.unwrap());
    let faulty_storage = Arc::new(FaultyStorage::new(real_storage.clone()));

    let hnsw_config = HnswConfig {
        dimension: dim,
        ..Default::default()
    };
    let real_hnsw = Arc::new(HnswIndex::try_new(hnsw_config).unwrap());
    let faulty_hnsw = Arc::new(FaultyVectorIndex::new(real_hnsw));

    let graph = CsrGraph::with_storage(faulty_storage.clone());
    let next_tx = Arc::new(AtomicU64::new(1));

    let col = memfuse_db::Collection::new(
        "fault_test_col".to_string(),
        faulty_storage.clone(),
        faulty_hnsw.clone(),
        Arc::new(graph),
        next_tx,
        dim,
        memfuse_text::Language::English,
    );

    (col, faulty_storage, faulty_hnsw, real_storage)
}

/// Verification helper: checks whether a document is visible across all 4 search/storage signals.
#[allow(deprecated)]
async fn assert_4_signal_consistency(
    col: &memfuse_db::Collection<FaultyStorage, FaultyVectorIndex>,
    id: &str,
    expected_visible: bool,
) {
    // 1. LSM Storage Check
    let lsm_doc = col.get(id).await.unwrap();
    let lsm_visible = lsm_doc.is_some();

    // 2. HNSW Vector Index Check
    let search_res = col.search(&[0.1, 0.2, 0.3, 0.4], 10).await.unwrap();
    let hnsw_visible = search_res.iter().any(|r| r.id == id);

    // 3. BM25 Text Index Check (via hybrid_search_with_weights using text signal without requiring embedder)
    let text_res = col.hybrid_search("test", &[], 10, None).await.unwrap();
    let text_visible = text_res.iter().any(|r| r.id == id);

    // 4. CSR Graph Index Check
    let eid = EntityId::from_key(id).unwrap();
    let neighbors = col.graph_index().neighbors(eid).await.unwrap();
    let graph_visible = !neighbors.is_empty() || col.graph_index().entity_exists(eid);

    if expected_visible {
        assert!(
            lsm_visible,
            "Document '{}' expected in LSM, but was missing",
            id
        );
        assert!(
            hnsw_visible,
            "Document '{}' expected in HNSW, but was missing",
            id
        );
        assert!(
            text_visible,
            "Document '{}' expected in BM25, but was missing",
            id
        );
        assert!(
            graph_visible,
            "Document '{}' expected in CSR Graph, but was missing",
            id
        );
    } else {
        assert!(
            !lsm_visible,
            "CRITICAL: Phantom document '{}' found in LSM Storage after failure",
            id
        );
        assert!(
            !hnsw_visible,
            "CRITICAL: Phantom document '{}' found in HNSW Vector Index after failure",
            id
        );
        assert!(
            !text_visible,
            "CRITICAL: Phantom document '{}' found in BM25 Text Index after failure",
            id
        );
        assert!(
            !graph_visible,
            "CRITICAL: Phantom document '{}' found in CSR Graph Index after failure",
            id
        );
    }
}

// ============================================================================
// FAULT INJECTION TEST CASES (a-e)
// ============================================================================

/// Case 2a: HNSW Staging Fails (Regression Test)
#[tokio::test]
async fn test_2a_hnsw_staging_failure() {
    let tmp = tempdir().unwrap();
    let (col, _, faulty_hnsw, _) = create_faulty_collection(tmp.path().to_path_buf(), 4).await;

    // Inject failure at HNSW staging
    faulty_hnsw.fail_insert.store(true, Ordering::SeqCst);

    let res = col
        .insert(
            "doc_2a",
            &[0.1, 0.2, 0.3, 0.4],
            Some(json!({ "text": "test content doc_2a" })),
        )
        .await;

    assert!(res.is_err(), "Insert must fail when HNSW staging fails");

    // Verify 0 phantom hits across all 4 signals
    assert_4_signal_consistency(&col, "doc_2a", false).await;
}

/// Case 2b: BM25/Text Staging Fails AFTER HNSW Staging Succeeded
#[tokio::test]
async fn test_2b_text_staging_failure_after_hnsw_success() {
    let tmp = tempdir().unwrap();
    let (col, faulty_storage, _, _) = create_faulty_collection(tmp.path().to_path_buf(), 4).await;

    // Inject failure during BM25/Text staging storage put
    faulty_storage.fail_put_text.store(true, Ordering::SeqCst);

    let res = col
        .insert(
            "doc_2b",
            &[0.1, 0.2, 0.3, 0.4],
            Some(json!({ "text": "test content doc_2b" })),
        )
        .await;

    assert!(
        res.is_err(),
        "Insert must fail when BM25 text staging fails"
    );

    // Verify 0 phantom hits across all 4 signals (HNSW staged vector was rolled back)
    assert_4_signal_consistency(&col, "doc_2b", false).await;
}

/// Case 2c: CSR-Graph Staging / Persistence Fails AFTER HNSW & Text Staging Succeeded
#[tokio::test]
async fn test_2c_graph_staging_failure_after_hnsw_and_text_success() {
    let tmp = tempdir().unwrap();
    let (col, faulty_storage, _, _) = create_faulty_collection(tmp.path().to_path_buf(), 4).await;

    // Inject failure during CSR-Graph storage put
    faulty_storage.fail_put_graph.store(true, Ordering::SeqCst);

    let res = col
        .insert(
            "doc_2c",
            &[0.1, 0.2, 0.3, 0.4],
            Some(json!({ "text": "test content doc_2c" })),
        )
        .await;

    assert!(
        res.is_err(),
        "Insert must fail when CSR graph persistence fails"
    );

    // Verify 0 phantom hits across all 4 signals
    assert_4_signal_consistency(&col, "doc_2c", false).await;
}

/// Case 2d1: LSM Commit Fails during Phase 2 Commit
#[tokio::test]
async fn test_2d1_lsm_commit_failure() {
    let tmp = tempdir().unwrap();
    let (col, faulty_storage, _, _) = create_faulty_collection(tmp.path().to_path_buf(), 4).await;

    // Inject LSM commit failure
    faulty_storage
        .fail_all_commits
        .store(true, Ordering::SeqCst);

    let res = col
        .insert(
            "doc_2d1",
            &[0.1, 0.2, 0.3, 0.4],
            Some(json!({ "text": "test content doc_2d1" })),
        )
        .await;

    assert!(res.is_err(), "Insert must fail when LSM commit fails");

    // Verify 0 phantom hits across all 4 signals
    assert_4_signal_consistency(&col, "doc_2d1", false).await;
}

/// Case 2d2: HNSW Commit Fails AFTER LSM Commit Succeeded (Compensating LSM Tx)
#[tokio::test]
async fn test_2d2_hnsw_commit_failure_post_lsm_commit() {
    let tmp = tempdir().unwrap();
    let (col, _, faulty_hnsw, _) = create_faulty_collection(tmp.path().to_path_buf(), 4).await;

    // Inject failure at HNSW commit (LSM commit will succeed first)
    faulty_hnsw.fail_commit.store(true, Ordering::SeqCst);

    let res = col
        .insert(
            "doc_2d2",
            &[0.1, 0.2, 0.3, 0.4],
            Some(json!({ "text": "test content doc_2d2" })),
        )
        .await;

    assert!(
        res.is_err(),
        "Insert must fail when HNSW commit fails post LSM commit"
    );

    // Verify 0 phantom hits across all 4 signals (compensating LSM tx removed committed LSM keys)
    assert_4_signal_consistency(&col, "doc_2d2", false).await;
}

/// Case 2d3: BM25/Text Commit Fails AFTER LSM & HNSW Commits Succeeded (Compensating HNSW & LSM Txs)
#[tokio::test]
async fn test_2d3_text_commit_failure_post_lsm_and_hnsw_commit() {
    let tmp = tempdir().unwrap();
    let (col, faulty_storage, _, _) = create_faulty_collection(tmp.path().to_path_buf(), 4).await;

    // In DbTransaction::commit():
    // Commit #1 = storage.commit() (LSM)
    // Commit #2 = text_index.commit() -> storage.commit()
    // By failing commit #2 specifically for the transaction, LSM and HNSW are ALREADY COMMITTED when Text commit fails!
    *faulty_storage.fail_on_commit_index.lock() = Some(2);

    let res = col
        .insert(
            "doc_2d3",
            &[0.1, 0.2, 0.3, 0.4],
            Some(json!({ "text": "test content doc_2d3" })),
        )
        .await;

    assert!(
        res.is_err(),
        "Insert must fail when text commit fails post HNSW commit"
    );

    // Verify 0 phantom hits across all 4 signals
    assert_4_signal_consistency(&col, "doc_2d3", false).await;
}

/// Case 2d4: CSR-Graph Commit Fails AFTER LSM, HNSW & BM25 Commits ALL Succeeded (3-Engine Compensating Rollback!)
#[tokio::test]
async fn test_2d4_graph_commit_failure_post_all_three_commits() {
    let tmp = tempdir().unwrap();
    let (col, faulty_storage, _, _) = create_faulty_collection(tmp.path().to_path_buf(), 4).await;

    // Enable graph storage put failure.
    // Graph entities are staged in memory during insert_op/commit_graph_staged,
    // so storage put is only called when graph_index.commit() runs (Phase 2 Step 4),
    // AFTER LSM, HNSW, and BM25 have all physically committed!
    faulty_storage.fail_put_graph.store(true, Ordering::SeqCst);

    let res = col
        .insert(
            "doc_2d4",
            &[0.1, 0.2, 0.3, 0.4],
            Some(json!({ "text": "test content doc_2d4" })),
        )
        .await;

    assert!(
        res.is_err(),
        "Insert must fail when graph index commit fails post-all-three-commits"
    );

    // Verify 0 phantom hits across all 4 signals!
    // This proves that compensating transactions (compensate_text, compensate_hnsw, compensate_lsm)
    // cleanly reverted all 3 previously committed sub-engines!
    assert_4_signal_consistency(&col, "doc_2d4", false).await;
}

/// Case 2e: Simulated Process Crash across Crash Points & Verification via repair_on_open()
#[tokio::test]
#[allow(deprecated)]
async fn test_2e_crash_points_and_repair_on_open() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().to_path_buf();
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };

    // Simulate Crash Point: Document written to LSM with CommitIntent::Pending, but crash occurs before index commit
    {
        let db = MemFuse::open_with_config(&path, config.clone())
            .await
            .unwrap();
        let col = db.collection("crash_col").await.unwrap();

        let doc_id = DocId::from_key("crash_doc_1").unwrap();
        let stored_json = json!({
            "id": "crash_doc_1",
            "embedding": vec![0.1, 0.2, 0.3, 0.4],
            "metadata": { "text": "uncommitted text lost in crash" }
        });
        let meta_json = json!({
            "id": "crash_doc_1",
            "metadata": { "text": "uncommitted text lost in crash" }
        });

        let data = serde_json::to_vec(&stored_json).unwrap();
        let meta_data = serde_json::to_vec(&meta_json).unwrap();

        let user_key = namespaced_key_helper("crash_col", b"crash_doc_1", 0);
        let doc_key = namespaced_key_helper("crash_col", &doc_id.inner().to_le_bytes(), 1);

        let tx = db.allocate_tx().unwrap();

        // Write user key and doc key to LSM storage
        col.storage().put(tx, &user_key, &data).await.unwrap();
        col.storage().put(tx, &doc_key, &meta_data).await.unwrap();

        // Write CommitIntent::Pending (key_type = 3)
        let intent_key = namespaced_key_helper("crash_col", tx.inner().to_le_bytes().as_ref(), 3);
        let intent = memfuse_db::transaction::CommitIntent::Pending {
            doc_ids: vec![doc_id],
            has_text: true,
            has_graph: true,
        };
        let intent_bytes = serde_json::to_vec(&intent).unwrap();
        col.storage()
            .put(tx, &intent_key, &intent_bytes)
            .await
            .unwrap();

        // Commit LSM storage (simulating crash right after LSM commit)
        col.storage().commit(tx).await.unwrap();

        // Close DB without committing HNSW, BM25, or Graph in memory (simulating process crash)
        db.close().await.unwrap();
    }

    // Re-open database: repair_on_open must execute, resolve pending intent, re-sync all 4 signals
    {
        let db = MemFuse::open_with_config(&path, config).await.unwrap();
        let col = db.collection("crash_col").await.unwrap();

        // Verify document is 100% visible across ALL 4 signals after repair_on_open
        let lsm_doc = col.get("crash_doc_1").await.unwrap();
        assert!(
            lsm_doc.is_some(),
            "Document must be present in LSM after repair_on_open"
        );

        let search_res = col.search(&[0.1, 0.2, 0.3, 0.4], 10).await.unwrap();
        assert_eq!(
            search_res.len(),
            1,
            "Document must be repaired and visible in HNSW vector index"
        );
        assert_eq!(search_res[0].id, "crash_doc_1");

        let text_res = col
            .hybrid_search("uncommitted", &[], 10, None)
            .await
            .unwrap();
        assert_eq!(
            text_res.len(),
            1,
            "Document must be repaired and visible in BM25 text index"
        );

        let eid = EntityId::from_key("crash_doc_1").unwrap();
        assert!(
            col.graph_index().entity_exists(eid),
            "Entity must be repaired and visible in CSR graph index"
        );
    }
}

// ============================================================================
// BATCH TRANSACTION SEMANTICS TEST (Requirement 4)
// ============================================================================

/// Requirement 4: Test Multi-Document Batch Transactions (`insert_many`) with Failure at 50% Point
#[tokio::test]
#[allow(deprecated)]
async fn test_insert_many_atomic_all_or_nothing_at_50_percent_failure() {
    let tmp = tempdir().unwrap();
    let config = MemFuseConfig {
        dimension: 768,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(tmp.path(), config).await.unwrap();
    let col = db.collection("batch_col").await.unwrap();

    // Prepare 10 documents for insert_many, where document 5 (50% mark) triggers a dimension mismatch error
    let mut batch = Vec::new();
    for i in 1..=10 {
        let id = format!("batch_doc_{}", i);
        let text = format!("Batch document {} text content", i);

        if i == 5 {
            // Document 5 has wrong vector dimension (2 instead of 768) -> triggers error in insert_op
            batch.push((id, vec![0.1, 0.2], Some(json!({ "text": text }))));
        } else {
            batch.push((id, vec![0.1; 768], Some(json!({ "text": text }))));
        }
    }

    // Execute insert_many
    let res = col.insert_many(&batch).await;

    // 1. Assert insert_many returned an Error
    assert!(
        res.is_err(),
        "insert_many must fail when document 5 has dimension mismatch"
    );

    // 2. Verify ALL-OR-NOTHING atomicity: None of the documents (1..10) must be present in storage or search!
    for i in 1..=10 {
        let id = format!("batch_doc_{}", i);
        let doc = col.get(&id).await.unwrap();
        assert!(
            doc.is_none(),
            "CRITICAL: Document '{}' (index {}) was found in storage! insert_many violates All-or-Nothing atomicity!",
            id, i
        );
    }

    // 3. Verify BM25 Text Index isolation
    let text_res = col.hybrid_search("Batch", &[], 20, None).await.unwrap();
    assert!(
        text_res.is_empty(),
        "CRITICAL: Phantom text search hits found after insert_many failure!"
    );

    // 4. Verify Vector Index isolation
    let vec_res = col.search(&vec![0.1; 768], 20).await.unwrap();
    assert!(
        vec_res.is_empty(),
        "CRITICAL: Phantom vector search hits found after insert_many failure!"
    );
}

/// Verification of UPDATE Rollback: pre-existing document MUST be restored on update failure, NOT erased (no phantom-erasure).
#[tokio::test]
async fn test_update_rollback_restores_original_document_state() {
    let tmp = tempdir().unwrap();
    let (col, _, faulty_hnsw, _) = create_faulty_collection(tmp.path().to_path_buf(), 4).await;

    // 1. Initial Insert of Document "doc_update_test"
    col.insert(
        "doc_update_test",
        &[0.1, 0.2, 0.3, 0.4],
        Some(json!({ "text": "Original Content V1", "version": 1 })),
    )
    .await
    .expect("Initial insert must succeed");

    let doc_v1 = col
        .get("doc_update_test")
        .await
        .unwrap()
        .expect("Document must exist in V1");
    assert_eq!(
        doc_v1.metadata.as_ref().unwrap()["text"],
        "Original Content V1"
    );

    // 2. Inject failure at HNSW commit step for the subsequent update
    faulty_hnsw.fail_commit.store(true, Ordering::SeqCst);

    // 3. Attempt UPDATE on Document "doc_update_test"
    let update_res = col
        .update(
            "doc_update_test",
            &[0.5, 0.6, 0.7, 0.8],
            Some(json!({ "text": "Updated Content V2", "version": 2 })),
        )
        .await;

    assert!(
        update_res.is_err(),
        "Update must fail due to injected HNSW commit fault"
    );

    // 4. Verify that get("doc_update_test") returns Original Content V1 (NOT None / phantom erasure)
    let restored_doc = col
        .get("doc_update_test")
        .await
        .unwrap()
        .expect("Document MUST still exist after failed update rollback (no phantom erasure)");

    assert_eq!(
        restored_doc.metadata.as_ref().unwrap()["text"],
        "Original Content V1",
        "Document content must be rolled back to Original Content V1"
    );
}

/// Regression test for INSERT Rollback: failure during INSERT of a new document must still write a tombstone and return None.
#[tokio::test]
async fn test_insert_rollback_writes_tombstone_returns_none() {
    let tmp = tempdir().unwrap();
    let (col, _, faulty_hnsw, _) = create_faulty_collection(tmp.path().to_path_buf(), 4).await;

    // Inject failure at HNSW commit step
    faulty_hnsw.fail_commit.store(true, Ordering::SeqCst);

    // Attempt INSERT of new document
    let insert_res = col
        .insert(
            "doc_new_insert",
            &[0.1, 0.2, 0.3, 0.4],
            Some(json!({ "text": "New Doc" })),
        )
        .await;

    assert!(
        insert_res.is_err(),
        "Insert must fail due to injected fault"
    );

    // Verify document does NOT exist in storage (tombstone written properly)
    let doc = col.get("doc_new_insert").await.unwrap();
    assert!(
        doc.is_none(),
        "Failed insert must result in None (tombstone written)"
    );
}
