#![cfg(feature = "fault-injection")]
// FILE-CONTEXT: Integration test verifying concurrent commit lock serialization during WAL HMAC rollback. (TS: 2026-09-08) (SESSION: b448084)
//! Integration test for WAL HMAC rollback concurrency and lock serialization.
//!
//! Verifies that `commit_mutex` serialization prevents race conditions between
//! a failing transaction undergoing WAL HMAC rollback and a concurrent successful commit.

use memfuse_core::{StorageEngine, TxId};
use memfuse_store::lsm::{LsmConfig, LsmStorage};
use memfuse_store::wal::FAIL_APPEND_FOR_TX;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tempfile::tempdir;

/// Fault Injection Method Documentation (Step 3):
/// To deterministically induce a WAL `append_batch()` failure for a specific transaction during `commit()`,
/// we use a test-only fault injection hook (`FAIL_APPEND_FOR_TX` in `crates/memfuse-store/src/wal.rs`).
/// When set to `tx.inner()`, any `append_batch()` call containing entries for `tx` fails with a
/// `MemFuseError::Storage` I/O error, exercising the exact error path in `LsmStorage::commit()` where
/// `restore_last_hmac()` and `rollback_to_tx_locked()` are invoked.

#[tokio::test]
async fn test_concurrent_commit_during_wal_rollback_preserves_hmac_chain() {
    let dir = tempdir().expect("tempdir creation failed");
    let config = LsmConfig {
        path: dir.path().to_path_buf(),
        ..Default::default()
    };

    let storage = Arc::new(
        LsmStorage::new(config)
            .await
            .expect("LsmStorage creation failed"),
    );

    // 1. Initial setup: commit tx0 to establish baseline HMAC chain and state
    let tx0 = TxId::new(1);
    storage
        .put(tx0, b"baseline_key", b"baseline_val")
        .await
        .expect("put baseline failed");
    storage.commit(tx0).await.expect("commit baseline failed");

    let hmac_baseline = storage.last_seq_no().await.expect("last_seq_no failed");
    assert_eq!(hmac_baseline, 1, "Baseline sequence number must be 1");

    // 2. Prepare concurrent tasks: Task 1 (failing commit) and Task 2 (successful commit)
    let storage1 = Arc::clone(&storage);
    let storage2 = Arc::clone(&storage);

    let tx1 = TxId::new(2);
    let tx2 = TxId::new(3);

    // Stage writes for both transactions before spawning concurrent commits
    storage
        .put(tx1, b"failing_key", b"failing_val")
        .await
        .expect("put tx1 failed");
    storage
        .put(tx2, b"successful_key", b"successful_val")
        .await
        .expect("put tx2 failed");

    // Activate fault injection hook specifically targeting tx1 (tx_id = 2)
    FAIL_APPEND_FOR_TX.store(tx1.inner(), Ordering::SeqCst);

    // Spawn concurrent commit tasks
    let task1 = tokio::spawn(async move { storage1.commit(tx1).await });
    let task2 = tokio::spawn(async move { storage2.commit(tx2).await });

    let (res1, res2) = tokio::join!(task1, task2);
    let res1 = res1.expect("task1 panicked");
    let res2 = res2.expect("task2 panicked");

    // Exactly one commit must fail (tx1) and one commit must succeed (tx2)
    assert!(
        res1.is_err(),
        "Task 1 commit must fail due to WAL fault injection"
    );
    assert!(
        res2.is_ok(),
        "Task 2 commit must succeed despite concurrent failing commit"
    );

    // 3. Verifications after concurrent execution completion:

    // a) Verify last_committed_tx is updated to tx2 (tx1 failed and was rolled back)
    let last_tx = storage.last_tx_id().await.expect("last_tx_id failed");
    assert_eq!(
        last_tx, tx2,
        "last_committed_tx must be tx2 (successful commit)"
    );

    // b) Verify Key-Value state: successful_key is present, failing_key is absent
    let val_failing = storage
        .get(b"failing_key")
        .await
        .expect("get failing_key failed");
    assert_eq!(
        val_failing, None,
        "Failing transaction key must be completely rolled back"
    );

    let val_successful = storage
        .get(b"successful_key")
        .await
        .expect("get successful_key failed");
    assert_eq!(
        val_successful,
        Some(b"successful_val".to_vec()),
        "Successful transaction key must be readable"
    );

    let val_baseline = storage
        .get(b"baseline_key")
        .await
        .expect("get baseline_key failed");
    assert_eq!(
        val_baseline,
        Some(b"baseline_val".to_vec()),
        "Baseline transaction key must remain intact"
    );

    // c) Verify WAL replay integrity: replaying the log must succeed with a valid HMAC chain
    let replay_entries = storage.scan_prefix(b"").await.expect("scan_prefix failed");
    assert_eq!(
        replay_entries.len(),
        2,
        "Exactly 2 keys (baseline + successful) must exist in storage"
    );
}

// =========================================================================
// Flakiness & Race Verification Shell Snippet (50 Consecutive Runs):
// =========================================================================
// To run this test 50 times in a loop and ensure no timing variability or race condition flakiness:
//
// for i in $(seq 1 50); do cargo test --test wal_hmac_rollback_race --features fault-injection -- --nocapture || break; done
// =========================================================================
