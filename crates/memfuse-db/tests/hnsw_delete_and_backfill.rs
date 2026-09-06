// FILE-CONTEXT: HNSW Soft-Delete Error Propagation & Search Backfill Verification
// ZWECK: Verifiziert, dass ein Fehler beim HNSW-Delete nicht verschluckt wird und Vektorsuchen bei Tombstones durch Backfill k valide Ergebnisse liefern.

use memfuse_core::{
    BoxFuture,
    DocId, MemFuseError, Result, ScoredDocument, TxId, VectorIndex, VectorIndexStats,
};
use memfuse_db::{MemFuse, MemFuseConfig};
use memfuse_graph::CsrGraph;
use memfuse_index::{HnswConfig, HnswIndex};
use memfuse_store::{LsmConfig, LsmStorage};
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tempfile::tempdir;

/// VectorIndex wrapper allowing injected delete failures.
struct FaultyDeleteVectorIndex {
    inner: Arc<HnswIndex>,
    fail_delete: AtomicBool,
}

impl VectorIndex for FaultyDeleteVectorIndex {
    async fn insert(&self, tx: TxId, id: DocId, embedding: &[f32]) -> Result<()> {
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
        if self.fail_delete.load(Ordering::SeqCst) {
            return Err(MemFuseError::Transaction(
                "INJECTED FAULT: HNSW vector index soft-delete failure".into(),
            ));
        }
        self.inner.delete(tx, id).await
    }

    async fn commit(&self, tx: TxId) -> Result<()> {
        self.inner.commit(tx).await
    }

    async fn rollback(&self, tx: TxId) -> Result<()> {
        self.inner.rollback(tx).await
    }

    async fn rollback_to_tx(&self, tx_id: TxId) -> Result<()> {
        self.inner.rollback_to_tx(tx_id).await
    }

    async fn last_tx_id(&self) -> Result<TxId> {
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

/// Test 1: Fault-injection scenario where index.delete fails during col.delete(),
/// verifying that the delete operation reports an error to the caller instead of swallowing it.
#[tokio::test]
async fn test_hnsw_delete_failure_propagates_error() {
    let tmp = tempdir().unwrap();
    let dim = 4;

    let lsm_config = LsmConfig {
        path: tmp.path().to_path_buf(),
        ..Default::default()
    };
    let storage = Arc::new(LsmStorage::new(lsm_config).await.unwrap());

    let hnsw_config = HnswConfig {
        dimension: dim,
        ..Default::default()
    };
    let real_hnsw = Arc::new(HnswIndex::try_new(hnsw_config).unwrap());
    let faulty_hnsw = Arc::new(FaultyDeleteVectorIndex {
        inner: real_hnsw,
        fail_delete: AtomicBool::new(false),
    });

    let graph = CsrGraph::with_storage(storage.clone());
    let next_tx = Arc::new(std::sync::atomic::AtomicU64::new(1));

    let col = memfuse_db::Collection::new(
        "fault_del_col".to_string(),
        storage,
        faulty_hnsw.clone(),
        Arc::new(graph),
        next_tx,
        dim,
        memfuse_text::Language::English,
    );

    // Insert a document
    col.insert(
        "doc_fault_1",
        &[0.1, 0.2, 0.3, 0.4],
        Some(json!({ "text": "test delete propagation" })),
    )
    .await
    .unwrap();

    // Enable HNSW delete fault injection
    faulty_hnsw.fail_delete.store(true, Ordering::SeqCst);

    // Attempt to delete doc_fault_1
    let delete_res = col.delete("doc_fault_1").await;

    // Assert that the delete error is returned to the caller
    assert!(
        delete_res.is_err(),
        "Delete operation MUST return an Err when index.delete fails"
    );
    let err_str = delete_res.unwrap_err().to_string();
    assert!(
        err_str.contains("INJECTED FAULT: HNSW vector index soft-delete failure"),
        "Unexpected error message: {err_str}"
    );

    // Disable fault injection and verify delete succeeds
    faulty_hnsw.fail_delete.store(false, Ordering::SeqCst);
    let delete_retry = col.delete("doc_fault_1").await;
    assert!(
        delete_retry.is_ok(),
        "Delete operation should succeed after resolving fault"
    );
}

/// Test 2: Search with k=10 when tombstones exist in the candidate window.
/// Verifies that iterative backfill fetches enough candidates to return k=10 valid results.
#[tokio::test]
#[allow(deprecated)]
async fn test_vector_search_backfill_with_tombstones() {
    let tmp = tempdir().unwrap();
    let dim = 4;
    let config = MemFuseConfig {
        dimension: dim,
        ..Default::default()
    };

    let db = MemFuse::open_with_config(tmp.path(), config).await.unwrap();
    let col = db.collection("backfill_col").await.unwrap();

    // Insert 20 documents with nearly identical embeddings
    for i in 0..20 {
        let id = format!("doc_{:02}", i);
        let vec = vec![0.1 + (i as f32) * 0.001, 0.2, 0.3, 0.4];
        col.insert(&id, &vec, Some(json!({ "index": i })))
            .await
            .unwrap();
    }

    // Verify initial search for k=10 yields 10 results
    let initial_results = col.search(&[0.1, 0.2, 0.3, 0.4], 10).await.unwrap();
    assert_eq!(
        initial_results.len(),
        10,
        "Expected 10 initial search results"
    );

    // Soft-delete 5 of the top-ranked candidates (doc_00 through doc_04)
    for i in 0..5 {
        let id = format!("doc_{:02}", i);
        col.delete(&id).await.unwrap();
    }

    // Search with k=10 again: even though the top 5 candidates in HNSW are now tombstoned,
    // backfilling should fetch additional candidates so that exactly 10 valid results are returned!
    let backfill_results = col.search(&[0.1, 0.2, 0.3, 0.4], 10).await.unwrap();
    assert_eq!(
        backfill_results.len(),
        10,
        "Search with k=10 MUST return 10 valid results after 5 candidates are tombstoned (via backfill)"
    );

    // Ensure none of the tombstoned documents are present in the search results
    for res in &backfill_results {
        assert!(
            !res.id.starts_with("doc_00")
                && !res.id.starts_with("doc_01")
                && !res.id.starts_with("doc_02")
                && !res.id.starts_with("doc_03")
                && !res.id.starts_with("doc_04"),
            "Deleted document '{}' found in search results!",
            res.id
        );
    }

    // Soft-delete 8 more documents (doc_05 through doc_12), leaving only 7 active documents in total (doc_13..doc_19)
    for i in 5..13 {
        let id = format!("doc_{:02}", i);
        col.delete(&id).await.unwrap();
    }

    // Search with k=10 when corpus only contains 7 valid active documents:
    // Should return all 7 remaining documents without error.
    let remaining_results = col.search(&[0.1, 0.2, 0.3, 0.4], 10).await.unwrap();
    assert_eq!(
        remaining_results.len(),
        7,
        "Search when corpus only has 7 valid documents should return all 7"
    );
}
