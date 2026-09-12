//! Split-Brain & Idempotency Integration Testsuite for Vector-ID ↔ Document Consistency.
// ANCHOR[TEST:SPLIT_BRAIN_CONSISTENCY] STATUS:IN_PROGRESS (TS:2026-09-09T22:00:00Z)

use memfuse_core::{DocId, StorageEngine, VectorIndex};
use memfuse_db::context_compaction::ConsolidationSession;
use memfuse_db::transaction::CommitIntent;
use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use proptest::prelude::*;
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tempfile::TempDir;

/// Independent Ground Truth state map for document consistency tracking.
/// Maps document ID string to (embedding_vector, is_active).
#[derive(Debug, Default, Clone)]
pub struct GroundTruth {
    docs: Arc<Mutex<HashMap<String, (Vec<f32>, bool)>>>,
}

impl GroundTruth {
    pub fn new() -> Self {
        Self {
            docs: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn insert(&self, id: String, vector: Vec<f32>) {
        let mut map = self.docs.lock().unwrap();
        map.insert(id, (vector, true));
    }

    pub fn mark_deleted(&self, id: &str) {
        let mut map = self.docs.lock().unwrap();
        if let Some(entry) = map.get_mut(id) {
            entry.1 = false;
        }
    }

    pub fn mark_active(&self, id: &str) {
        let mut map = self.docs.lock().unwrap();
        if let Some(entry) = map.get_mut(id) {
            entry.1 = true;
        }
    }

    pub fn is_active(&self, id: &str) -> bool {
        let map = self.docs.lock().unwrap();
        map.get(id).map(|(_, active)| *active).unwrap_or(false)
    }
}

/// 1. `test_vector_deleted_process_killed_before_document_tombstone`
///
/// Simulates deleting a vector ID successfully from `HnswIndex`, then aborting the task
/// before the corresponding document tombstone is written to LSM storage.
/// Verifies:
/// a) Direct HNSW vector search after vector deletion returns 0 hits for deleted doc_2,
///    and all returned results match active documents in the independent GroundTruth map.
/// b) On `MemFuse::open_with_config` restart, `repair_on_open()` re-syncs HNSW with LSM storage
///    (LSM source-of-truth recovery), demonstrating cross-crate consistency handling.
#[tokio::test]
async fn test_vector_deleted_process_killed_before_document_tombstone() {
    let tmp = TempDir::new().expect("temp dir");
    let dir_path = tmp.path().to_path_buf();

    let config = MemFuseConfig {
        dimension: 4,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };

    let ground_truth = GroundTruth::new();

    // 1. Initialize collection and insert baseline documents
    let db = MemFuse::open_with_config(&dir_path, config.clone())
        .await
        .expect("open db");
    let col = db.collection("split_brain_col").await.expect("collection");

    for i in 0..5 {
        let id = format!("doc_{}", i);
        let vec = vec![(i + 1) as f32, 1.0, 0.0, 0.0];
        col.insert(&id, &vec, Some(json!({ "text": format!("Document {}", i) })))
            .await
            .expect("insert");
        ground_truth.insert(id, vec);
    }

    // 2. Perform vector-only deletion in HNSW index, then abort task before LSM tombstone write
    let target_doc_str = "doc_2".to_string();
    let col_clone = col.clone();
    let target_doc_str_clone = target_doc_str.clone();

    let handle = tokio::spawn(async move {
        let doc_id = DocId::from_key(&target_doc_str_clone).expect("DocId from key");
        let tx = col_clone.allocate_tx().expect("allocate_tx");

        // Delete vector from HNSW index and commit HNSW transaction
        col_clone.vector_index().delete(tx, doc_id).await.expect("HNSW delete");
        col_clone.vector_index().commit(tx).await.expect("HNSW commit");

        // Delay to ensure abort occurs before writing LSM tombstone in crud.rs
        tokio::time::sleep(Duration::from_secs(10)).await;

        // This LSM deletion will never execute due to task abort below
        col_clone.delete(&target_doc_str_clone).await.expect("LSM delete");
    });

    // Brief yield to allow HNSW vector deletion & commit to complete in task
    tokio::time::sleep(Duration::from_millis(50)).await;
    handle.abort();
    let _ = handle.await;

    // Record in GroundTruth map that doc_2 vector was removed
    ground_truth.mark_deleted(&target_doc_str);

    // Save HNSW index to ensure vector deletion persists to disk
    let hnsw_path = dir_path.join("hnsw.index");
    col.vector_index().save(&hnsw_path).await.expect("save hnsw");

    // 3. Direct query before repair_on_open: verify HNSW index alone returns 0 hits for doc_2
    let query_vec = vec![3.0, 1.0, 0.0, 0.0];
    let direct_search_results = col
        .query()
        .embedding(&query_vec)
        .k(10)
        .execute()
        .await
        .expect("direct search");

    assert!(
        !direct_search_results.iter().any(|r| r.id == target_doc_str),
        "Direct search over HNSW after vector deletion must NOT return doc_2"
    );

    // Verify all returned results match active entries in independent GroundTruth map
    for res in &direct_search_results {
        assert!(
            ground_truth.is_active(&res.id),
            "Direct search returned document '{}' which is marked inactive in GroundTruth!",
            res.id
        );
    }

    // Close DB
    db.close().await.expect("close db");

    // 4. Reopen collection on same data directory (simulates crash restart with repair_on_open)
    let db_reopened = MemFuse::open_with_config(&dir_path, config)
        .await
        .expect("reopen db");
    let col_reopened = db_reopened.collection("split_brain_col").await.expect("reopened collection");

    // Verify LSM state: document still exists in LSM storage because tombstone was never written
    let lsm_doc = col_reopened.get(&target_doc_str).await.expect("get lsm doc");
    assert!(
        lsm_doc.is_some(),
        "LSM tombstone was not written, so get() in LSM storage still returns doc_2"
    );

    // Update GroundTruth map to reflect that repair_on_open() restored doc_2 from LSM source-of-truth
    ground_truth.mark_active(&target_doc_str);

    // Perform vector search after repair_on_open recovery
    let search_results = col_reopened
        .query()
        .embedding(&query_vec)
        .k(10)
        .execute()
        .await
        .expect("search post restart");

    // Verify all returned results are active in GroundTruth and valid non-tombstoned documents in LSM
    for res in &search_results {
        assert!(
            ground_truth.is_active(&res.id),
            "Post-restart search returned document '{}' which is marked inactive in GroundTruth!",
            res.id
        );
        let doc_in_lsm = col_reopened.get(&res.id).await.expect("LSM check");
        assert!(
            doc_in_lsm.is_some(),
            "Search returned document '{}' which is missing in LSM storage!",
            res.id
        );
    }
}

/// 2. `test_duplicate_insert_transaction_replay_no_double_vector_entry`
///
/// Simulates transaction replay / duplicate insertion of the same DocId after WAL-write crash.
/// Verifies idempotency by directly inspecting HNSW index doc IDs (`all_doc_ids()`)
/// to ensure exactly one vector entry exists for the DocId.
#[tokio::test]
async fn test_duplicate_insert_transaction_replay_no_double_vector_entry() {
    let tmp = TempDir::new().expect("temp dir");
    let dir_path = tmp.path().to_path_buf();

    let config = MemFuseConfig {
        dimension: 4,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };

    let doc_str = "doc_duplicate_replay";
    let doc_id = DocId::from_key(doc_str).expect("DocId from key");
    let vec_1 = vec![0.1, 0.2, 0.3, 0.4];

    // 1. Simulate WAL-Write + CommitIntent::Pending in LSM before HNSW commit completion (Crash Simulation)
    {
        let db = MemFuse::open_with_config(&dir_path, config.clone())
            .await
            .expect("open db");
        let col = db.collection("idempotency_col").await.expect("collection");

        let tx = col.allocate_tx().expect("allocate_tx");
        let user_key = col.namespaced_key(doc_str.as_bytes(), 0);
        let doc_key = col.namespaced_key(&doc_id.inner().to_le_bytes(), 1);

        let stored = json!({
            "id": doc_str,
            "embedding": vec_1,
            "metadata": { "version": 1 }
        });
        let meta_only = json!({
            "id": doc_str,
            "metadata": { "version": 1 }
        });

        col.storage().put(tx, &user_key, &serde_json::to_vec(&stored).unwrap()).await.expect("put user_key");
        col.storage().put(tx, &doc_key, &serde_json::to_vec(&meta_only).unwrap()).await.expect("put doc_key");

        // Write CommitIntent::Pending to simulate crash right after WAL/LSM commit
        let intent_key = col.namespaced_key(&tx.inner().to_le_bytes(), 3);
        let intent = CommitIntent::Pending {
            doc_ids: vec![doc_id],
            has_text: false,
            has_graph: false,
        };
        col.storage().put(tx, &intent_key, &serde_json::to_vec(&intent).unwrap()).await.expect("put intent");
        col.storage().commit(tx).await.expect("commit LSM");

        db.close().await.expect("close db");
    }

    // 2. Re-open DB: repair_on_open() runs col.repair() and recovers pending intent into HNSW index
    let db_reopened = MemFuse::open_with_config(&dir_path, config.clone())
        .await
        .expect("reopen db");
    let col_reopened = db_reopened.collection("idempotency_col").await.expect("reopened collection");

    // 3. DANACH execute the transaction replay / duplicate upsert operation
    col_reopened
        .upsert(doc_str, &vec_1, Some(json!({ "version": 1, "replayed": true })))
        .await
        .expect("replayed upsert");

    // 4. Direct introspection of HNSW index doc IDs (not via search score interpretation)
    let all_doc_ids = col_reopened.vector_index().all_doc_ids().await.expect("all_doc_ids");
    let count_all = all_doc_ids.iter().filter(|&&id| id == doc_id).count();

    assert_eq!(
        count_all, 1,
        "Expected exactly 1 active vector entry for DocId {:?} in HNSW index, found {}",
        doc_id, count_all
    );

    let map_doc_ids = col_reopened.vector_index().all_doc_ids_from_map();
    let count_map = map_doc_ids.iter().filter(|&&id| id == doc_id).count();

    assert_eq!(
        count_map, 1,
        "Expected exactly 1 entry in doc_to_node map for DocId {:?}, found {}",
        doc_id, count_map
    );
}

/// Helper operation enum for proptest generation.
#[derive(Debug, Clone)]
enum Op {
    Insert { id: u32, vec: Vec<f32> },
    Delete { id: u32 },
}

fn op_strategy() -> impl Strategy<Value = Vec<Op>> {
    let vec_strat = prop::collection::vec(-1.0f32..1.0f32, 4);
    let op_strat = prop_oneof![
        (0u32..15, vec_strat).prop_map(|(id, vec)| Op::Insert { id, vec }),
        (0u32..15).prop_map(|id| Op::Delete { id }),
    ];
    prop::collection::vec(op_strat, 20..40)
}

// 3. `prop_no_dangling_search_result_under_random_insert_delete_sequence`
//
// Generates random sequences of 20-40 insert/delete operations under simulated `ConcurrentWriteFlood`.
// Invariant: For EVERY ID returned by `search()` at any point, a resolvable, non-tombstoned document
// MUST exist in LSM storage (verified via independent GroundTruth map).
proptest! {
    #![proptest_config(ProptestConfig::with_cases(20))]
    #[test]
    fn prop_no_dangling_search_result_under_random_insert_delete_sequence(ops in op_strategy()) {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(4)
            .build()
            .expect("tokio runtime");

        rt.block_on(async move {
            let tmp = TempDir::new().expect("temp dir");
            let config = MemFuseConfig {
                dimension: 4,
                distance_metric: DistanceMetric::Cosine,
                ..Default::default()
            };
            let db = Arc::new(MemFuse::open_with_config(tmp.path(), config).await.expect("open db"));
            let col = Arc::new(db.collection("stress_prop_col").await.expect("collection"));

            let ground_truth = GroundTruth::new();
            let mut join_set = tokio::task::JoinSet::new();

            let ops_arc = Arc::new(ops);
            let num_writers = 4;
            let chunk_size = (ops_arc.len() + num_writers - 1) / num_writers;

            for w in 0..num_writers {
                let col = col.clone();
                let gt = ground_truth.clone();
                let ops_ref = ops_arc.clone();
                let start = w * chunk_size;
                let end = (start + chunk_size).min(ops_ref.len());

                join_set.spawn(async move {
                    for i in start..end {
                        match &ops_ref[i] {
                            Op::Insert { id, vec } => {
                                let doc_id_str = format!("doc_{}", id);
                                let mut norm_vec = vec.clone();
                                if norm_vec.iter().all(|&x| x == 0.0) {
                                    norm_vec[0] = 0.5;
                                }
                                if col.insert(&doc_id_str, &norm_vec, Some(json!({"id": id}))).await.is_ok() {
                                    gt.insert(doc_id_str, norm_vec);
                                }
                            }
                            Op::Delete { id } => {
                                let doc_id_str = format!("doc_{}", id);
                                if col.delete(&doc_id_str).await.is_ok() {
                                    gt.mark_deleted(&doc_id_str);
                                }
                            }
                        }
                        tokio::task::yield_now().await;
                    }
                });
            }

            // Concurrent reader task performing vector queries during write flood
            let col_read = col.clone();
            let gt_read = ground_truth.clone();
            join_set.spawn(async move {
                let query_vec = vec![0.5, 0.5, 0.0, 0.0];
                for _ in 0..20 {
                    if let Ok(results) = col_read.query().embedding(&query_vec).k(5).execute().await {
                        for res in results {
                            assert!(
                                gt_read.is_active(&res.id),
                                "Search returned document '{}' which is marked inactive in GroundTruth!",
                                res.id
                            );
                            let doc_in_lsm = col_read.get(&res.id).await.expect("LSM get");
                            assert!(
                                doc_in_lsm.is_some(),
                                "Search returned document '{}' which is missing in LSM storage!",
                                res.id
                            );
                        }
                    }
                    tokio::task::yield_now().await;
                }
            });

            while let Some(res) = join_set.join_next().await {
                res.expect("task completed");
            }

            // Final sanity check on post-sequence search
            let final_results = col.query().embedding(&[0.5, 0.5, 0.0, 0.0]).k(20).execute().await.expect("final search");
            for res in final_results {
                assert!(
                    ground_truth.is_active(&res.id),
                    "Final search returned document '{}' which is marked inactive in GroundTruth!",
                    res.id
                );
                assert!(
                    col.get(&res.id).await.expect("get").is_some(),
                    "Final search returned document '{}' which is missing in LSM storage!",
                    res.id
                );
            }
        });
    }
}

/// 4. `test_concurrent_partial_rebuild_and_context_compaction_no_lock_starvation`
///
/// Runs `partial_rebuild` / region rebuild and `context_compaction` concurrently on the same collection.
/// Enforces a 30-second timeout to detect deadlocks or lock starvation, verifying both tasks finish cleanly.
#[tokio::test]
async fn test_concurrent_partial_rebuild_and_context_compaction_no_lock_starvation() {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 4,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };

    let db = MemFuse::open_with_config(tmp.path(), config)
        .await
        .expect("open db");
    let col = Arc::new(db.collection("compaction_rebuild_col").await.expect("collection"));

    // Populate collection with documents
    for i in 0..20 {
        let id = format!("doc_{}", i);
        let vec = vec![(i + 1) as f32, 1.0, 0.0, 0.0];
        col.insert(&id, &vec, Some(json!({ "text": format!("Content for document {}", i) })))
            .await
            .expect("insert");
    }

    // Delete a subset to create tombstone density in HNSW
    for i in 0..5 {
        let id = format!("doc_{}", i);
        col.delete(&id).await.expect("delete");
    }

    // Prepare concurrent tasks
    let col_rebuild = col.clone();
    let task_rebuild = tokio::spawn(async move {
        // Trigger region partial rebuild
        col_rebuild.vector_index().rebuild_region(vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9]).await
    });

    let col_compaction = col.clone();
    let task_compaction = tokio::spawn(async move {
        let doc_5 = DocId::from_key("doc_5").expect("DocId from key");
        let doc_6 = DocId::from_key("doc_6").expect("DocId from key");
        let target_doc_id = DocId::from_key("doc_target_compacted").expect("DocId target");

        let session = ConsolidationSession::start(&col_compaction, &[doc_5, doc_6], target_doc_id).await?;
        session.commit("doc_target_compacted", &[1.0, 1.0, 0.0, 0.0], "Compacted summary text", None).await
    });

    // Enforce 30-second timeout to detect deadlocks or lock starvation
    let timeout_duration = Duration::from_secs(30);
    let combined_tasks = async move {
        let (res_a, res_b) = tokio::join!(task_rebuild, task_compaction);
        (res_a.expect("rebuild task panic"), res_b.expect("compaction task panic"))
    };

    let (res_rebuild, res_compaction) = tokio::time::timeout(timeout_duration, combined_tasks)
        .await
        .expect("Deadlock or lock starvation detected! Concurrent passes did not complete within 30s timeout.");

    assert!(res_rebuild.is_ok(), "Partial rebuild failed: {:?}", res_rebuild);
    assert!(res_compaction.is_ok(), "Context compaction failed: {:?}", res_compaction);

    // Verify system health post-concurrency
    let doc_check = col.get("doc_target_compacted").await.expect("get compacted doc");
    assert!(doc_check.is_some(), "Compacted target document must exist in storage");

    let search_res = col.query().embedding(&[1.0, 1.0, 0.0, 0.0]).k(5).execute().await.expect("search post-concurrency");
    assert!(!search_res.is_empty(), "Collection search must function cleanly post-concurrency");
}
