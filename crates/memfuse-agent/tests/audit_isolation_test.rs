use memfuse_agent::audit::{migrate_legacy_audit_entries, AuditEntry, AuditLog};
use memfuse_core::{BoxFuture, DocId, MemFuseError, Result, StorageEngine, VectorIndex};
use memfuse_db::{MemFuse, MemFuseConfig};
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
    let migrated_count = migrate_legacy_audit_entries(&col).await?;
    assert_eq!(
        migrated_count, 10,
        "Should migrate all 10 legacy audit entries"
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
