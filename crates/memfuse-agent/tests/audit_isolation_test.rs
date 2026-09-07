use memfuse_agent::audit::{migrate_legacy_audit_entries, AuditEntry, AuditLog};
use memfuse_core::{
    DocId, MemFuseError, Result, ScoredDocument, StorageEngine, TxId, VectorIndex,
    VectorIndexStats,
};
use memfuse_db::{Collection, MemFuse, MemFuseConfig};
use memfuse_graph::CsrGraph;
use memfuse_index::{HnswConfig, HnswIndex};
use std::collections::HashSet;
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_audit_logging_does_not_pollute_hnsw_index_count() -> Result<()> {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = Arc::new(MemFuse::open_with_config(tmp.path(), config).await?);
    let col = db.collection("main_col").await?;

    // Insert 5 regular documents
    for i in 0..5 {
        let id = format!("doc_{i}");
        let vec = vec![1.0, (i + 1) as f32, 0.0, 0.0];
        col.insert(&id, &vec, None).await?;
    }

    let initial_len = col.len().await;
    assert_eq!(initial_len, 5, "Initial collection length must be 5");

    let audit_log = AuditLog::new(col.clone());

    // Insert 1,000 audit entries via AuditLog
    for step in 0..1000 {
        let entry = AuditEntry {
            task_id: "task-stress-1000".to_string(),
            step_count: step,
            node_id: format!("node_{step}"),
            tokens_consumed: 10,
            payload: serde_json::json!({"step": step}),
            error: None,
        };
        audit_log.append(&entry).await?;
    }

    // Verify HNSW vector index node count remains exactly 5!
    let final_len = col.len().await;
    assert_eq!(
        final_len, initial_len,
        "HNSW index node count must remain unchanged at 5 after 1,000 audit entries"
    );

    // Verify all 1,000 audit entries can be replayed cleanly
    let replayed = audit_log.replay_task("task-stress-1000").await?;
    assert_eq!(replayed.len(), 1000);
    assert_eq!(replayed[0].step_count, 0);
    assert_eq!(replayed[999].step_count, 999);

    Ok(())
}

#[tokio::test]
async fn test_vector_search_results_unaffected_by_massive_audit_logging() -> Result<()> {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = Arc::new(MemFuse::open_with_config(tmp.path(), config).await?);
    let col = db.collection("main_col").await?;

    // Insert regular documents with distinct embeddings
    col.insert(
        "doc_alpha",
        &[1.0, 0.0, 0.0, 0.0],
        Some(serde_json::json!({"tag": "alpha"})),
    )
    .await?;
    col.insert(
        "doc_beta",
        &[0.0, 1.0, 0.0, 0.0],
        Some(serde_json::json!({"tag": "beta"})),
    )
    .await?;
    col.insert(
        "doc_gamma",
        &[0.0, 0.0, 1.0, 0.0],
        Some(serde_json::json!({"tag": "gamma"})),
    )
    .await?;

    // Baseline vector search
    let query_vec = vec![0.9, 0.1, 0.0, 0.0];
    let baseline_results = col.query().embedding(&query_vec).k(3).execute().await?;

    assert_eq!(baseline_results.len(), 3);
    assert_eq!(baseline_results[0].id, "doc_alpha");

    // Perform massive audit logging (10,000 entries)
    let audit_log = AuditLog::new(col.clone());
    for step in 0..10_000 {
        let entry = AuditEntry {
            task_id: "task-massive".to_string(),
            step_count: step,
            node_id: "proc_node".to_string(),
            tokens_consumed: 15,
            payload: serde_json::json!({"idx": step}),
            error: None,
        };
        audit_log.append(&entry).await?;
    }

    // Post-audit vector search
    let post_audit_results = col.query().embedding(&query_vec).k(3).execute().await?;

    assert_eq!(
        post_audit_results.len(),
        baseline_results.len(),
        "Result count must be identical"
    );
    for (a, b) in post_audit_results.iter().zip(baseline_results.iter()) {
        assert_eq!(
            a.id, b.id,
            "Result ranking and document IDs must match exactly"
        );
        assert!(
            (a.score - b.score).abs() < 1e-6,
            "Search scores must match exactly"
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_migration_removes_legacy_zero_vectors_from_hnsw() -> Result<()> {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = Arc::new(MemFuse::open_with_config(tmp.path(), config).await?);
    let col = db.collection("mig_col").await?;

    // 1. Insert 2 regular documents
    col.insert("reg_1", &[1.0, 0.0, 0.0, 0.0], None).await?;
    col.insert("reg_2", &[0.0, 1.0, 0.0, 0.0], None).await?;

    // 2. Synthetically inject 10 legacy zero-vector audit entries directly into storage and HNSW
    let zero_vec = vec![0.0f32; 4];
    for i in 0..10 {
        let key = format!("audit:mig-task:step:{i}");
        let entry = AuditEntry {
            task_id: "mig-task".to_string(),
            step_count: i,
            node_id: "legacy_node".to_string(),
            tokens_consumed: 10,
            payload: serde_json::json!({"step": i}),
            error: None,
        };
        let doc_id = DocId::from_key(&key)?;
        let tx = col.allocate_tx()?;

        let stored = serde_json::json!({
            "id": key,
            "embedding": zero_vec,
            "metadata": serde_json::to_value(&entry)?
        });
        let meta_only = serde_json::json!({
            "id": key,
            "metadata": serde_json::to_value(&entry)?
        });

        let user_key = col.namespaced_key(key.as_bytes(), 0);
        let doc_key = col.namespaced_key(&doc_id.inner().to_le_bytes(), 1);

        col.storage()
            .put(tx, &user_key, &serde_json::to_vec(&stored)?)
            .await?;
        col.storage()
            .put(tx, &doc_key, &serde_json::to_vec(&meta_only)?)
            .await?;
        col.vector_index().insert(tx, doc_id, &zero_vec).await?;
        col.storage().commit(tx).await?;
        col.vector_index().commit(tx).await?;
    }

    // Verify HNSW count initially contains 2 regular + 10 legacy audit = 12
    assert_eq!(col.len().await, 12);

    // 3. Run migration tool
    let stats = migrate_legacy_audit_entries(&col).await?;
    assert_eq!(
        stats.migrated, 10,
        "Should migrate all 10 legacy audit entries"
    );
    assert_eq!(
        stats.failed, 0,
        "Zero entries should fail during migration"
    );

    // 4. Verify HNSW index node count is restored to 2!
    assert_eq!(
        col.len().await,
        2,
        "HNSW index count must be reduced back to 2 after migration"
    );

    // 5. Verify audit entries remain fully replayable via AuditLog
    let audit_log = AuditLog::new(col.clone());
    let replayed = audit_log.replay_task("mig-task").await?;
    assert_eq!(replayed.len(), 10);
    assert_eq!(replayed[0].node_id, "legacy_node");

    Ok(())
}

struct FaultyVectorIndex {
    inner: HnswIndex,
    fail_doc_ids: HashSet<DocId>,
}

impl VectorIndex for FaultyVectorIndex {
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
        if self.fail_doc_ids.contains(&id) {
            return Err(MemFuseError::Internal(format!(
                "Simulated HNSW deletion failure for doc_id {:?}",
                id
            )));
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

    async fn len(&self) -> usize {
        self.inner.len().await
    }

    async fn last_tx_id(&self) -> Result<TxId> {
        self.inner.last_tx_id().await
    }

    async fn stats(&self) -> Result<VectorIndexStats> {
        self.inner.stats().await
    }
}

#[tokio::test]
async fn test_migration_hnsw_delete_failure_prevents_orphan_state() -> Result<()> {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = Arc::new(MemFuse::open_with_config(tmp.path(), config).await?);
    let real_col = db.collection("faulty_mig_col").await?;

    let fail_key = "audit:fail-task:step:1";
    let fail_doc_id = DocId::from_key(fail_key)?;

    let mut fail_set = HashSet::new();
    fail_set.insert(fail_doc_id);

    let inner_hnsw = HnswIndex::try_new(HnswConfig {
        dimension: 4,
        ..Default::default()
    })
    .expect("HnswIndex creation");

    let faulty_index = Arc::new(FaultyVectorIndex {
        inner: inner_hnsw,
        fail_doc_ids: fail_set,
    });

    let graph_index = Arc::new(CsrGraph::new());
    let next_tx = Arc::new(std::sync::atomic::AtomicU64::new(1));

    let col = Collection::new(
        "faulty_mig_col".to_string(),
        real_col.storage().clone(),
        faulty_index,
        graph_index,
        next_tx,
        4,
        memfuse_text::Language::English,
    );

    // Inject 3 legacy zero-vector audit entries (step 0: ok, step 1: fail delete, step 2: ok)
    let zero_vec = vec![0.0f32; 4];
    for i in 0..3 {
        let key = format!("audit:fail-task:step:{i}");
        let entry = AuditEntry {
            task_id: "fail-task".to_string(),
            step_count: i,
            node_id: format!("node_{i}"),
            tokens_consumed: 10,
            payload: serde_json::json!({"step": i}),
            error: None,
        };
        let doc_id = DocId::from_key(&key)?;
        let tx = col.allocate_tx()?;

        let stored = serde_json::json!({
            "id": key,
            "embedding": zero_vec,
            "metadata": serde_json::to_value(&entry)?
        });
        let meta_only = serde_json::json!({
            "id": key,
            "metadata": serde_json::to_value(&entry)?
        });

        let user_key = col.namespaced_key(key.as_bytes(), 0);
        let doc_key = col.namespaced_key(&doc_id.inner().to_le_bytes(), 1);

        col.storage()
            .put(tx, &user_key, &serde_json::to_vec(&stored)?)
            .await?;
        col.storage()
            .put(tx, &doc_key, &serde_json::to_vec(&meta_only)?)
            .await?;
        col.vector_index().insert(tx, doc_id, &zero_vec).await?;
        col.storage().commit(tx).await?;
        col.vector_index().commit(tx).await?;
    }

    assert_eq!(col.len().await, 3);

    // Execute migration
    let stats = migrate_legacy_audit_entries(&col).await?;

    assert_eq!(stats.migrated, 2, "2 entries should succeed");
    assert_eq!(stats.failed, 1, "1 entry should fail");

    // Check that the failed entry (step 1) retained its legacy doc_key mapping and user_key StoredDocument embedding
    let doc_key_1 = col.namespaced_key(&fail_doc_id.inner().to_le_bytes(), 1);
    let doc_key_val = col.storage().get(&doc_key_1).await?;
    assert!(
        doc_key_val.is_some(),
        "Legacy doc_key mapping for failed entry must NOT be deleted"
    );

    let user_key_1 = col.namespaced_key(fail_key.as_bytes(), 0);
    let user_key_val = col.storage().get(&user_key_1).await?.expect("user_key exists");
    let val_json: serde_json::Value = serde_json::from_slice(&user_key_val)?;
    assert!(
        val_json.as_object().unwrap().contains_key("embedding"),
        "Failed entry must remain in legacy StoredDocument format with embedding, not converted to pure KV"
    );

    // Verify HNSW count is 1 (the failed entry remaining in HNSW)
    assert_eq!(col.len().await, 1);

    Ok(())
}

#[tokio::test]
async fn test_zero_vector_insertion_rejected_with_invalid_input() -> Result<()> {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = Arc::new(MemFuse::open_with_config(tmp.path(), config).await?);
    let col = db.collection("test_col").await?;

    let zero_vec = vec![0.0f32; 4];

    // Regular insert with zero vector must be rejected
    let res = col.insert("zero_doc", &zero_vec, None).await;
    assert!(
        res.is_err(),
        "Zero vector insertion into Collection::insert must be rejected"
    );
    assert!(matches!(res, Err(MemFuseError::InvalidInput(_))));

    if let Err(MemFuseError::InvalidInput(msg)) = res {
        assert!(
            msg.contains("Zero vector embeddings are not allowed"),
            "Error message should explain rejection, got: {msg}"
        );
    }

    Ok(())
}
