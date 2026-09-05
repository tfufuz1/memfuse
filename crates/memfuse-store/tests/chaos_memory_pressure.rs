mod support;

use memfuse_core::{MemFuseError, StorageEngine, TxId};
use memfuse_store::lsm::{LsmConfig, LsmStorage};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tracing::Subscriber;

/// Custom subscriber to capture `tracing::warn!` events during testing.
struct WarnCaptureSubscriber {
    warn_detected: Arc<AtomicBool>,
}

impl Subscriber for WarnCaptureSubscriber {
    fn enabled(&self, _metadata: &tracing::Metadata<'_>) -> bool {
        true
    }
    fn new_span(&self, _span: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }
    fn record(&self, _span: &tracing::span::Id, _values: &tracing::span::Record<'_>) {}
    fn record_follows_from(&self, _span: &tracing::span::Id, _follows: &tracing::span::Id) {}
    fn event(&self, event: &tracing::Event<'_>) {
        if *event.metadata().level() == tracing::Level::WARN {
            self.warn_detected.store(true, Ordering::SeqCst);
        }
    }
    fn enter(&self, _span: &tracing::span::Id) {}
    fn exit(&self, _span: &tracing::span::Id) {}
}

#[tokio::test]
async fn test_chaos_memory_pressure_sequential_and_concurrent() {
    // Enable tracing warning capture
    let warn_detected = Arc::new(AtomicBool::new(false));
    let subscriber = WarnCaptureSubscriber {
        warn_detected: Arc::clone(&warn_detected),
    };
    let _ = tracing::subscriber::set_global_default(subscriber);

    // =========================================================================
    // Teil A — Sequential Memory Pressure Test
    // =========================================================================
    {
        let tmp = TempDir::new().expect("temp dir");
        let config = LsmConfig {
            path: tmp.path().to_path_buf(),
            // Set memory limit artificially small: 1 MB
            max_ram_mb: 1,
            // Large memtable limit so automatic flushes don't clear memory during loop
            memtable_size_limit: 10 * 1024 * 1024,
            ..Default::default()
        };

        let storage = LsmStorage::new(config)
            .await
            .expect("storage creation must succeed");

        let initial_rss = support::get_rss_bytes();
        let payload = vec![b'x'; 200 * 1024]; // 200 KB payload per item

        let hit_storage_error: bool;
        let mut committed_count = 0;

        // 1. Fill memory up to ~80% capacity using single-item transactions
        for i in 1..=4 {
            let tx = TxId::new(i);
            let key = format!("seq_key_{:03}", i).into_bytes();
            storage.put(tx, &key, &payload).await.expect("put ok");
            storage.commit(tx).await.expect("commit ok");
            committed_count += 1;
        }

        // 2. Stage a large multi-item transaction (5x 200KB = 1MB) while current memory is ~800KB (< 95% threshold).
        // `put()` will succeed for all items because staged memory is not yet tracked in ResourceTracker.
        let multi_tx = TxId::new(99);
        for j in 1..=5 {
            let key = format!("multi_key_{:03}", j).into_bytes();
            storage
                .put(multi_tx, &key, &payload)
                .await
                .expect("staging put must succeed because staged items do not update budget yet");
        }

        // 3. Commit multi_tx:
        // `commit()` passes initial `has_memory_capacity()` check (~800KB < 95% of 1MB).
        // In Phase 3, `budget.consume_memory()` is called for each item.
        // It succeeds for the first item (~1MB), but fails for subsequent items (> 1MB limit).
        // Per lsm.rs line 1005 observation: the error is logged as `tracing::warn!` and NOT propagated as Err.
        let commit_res = storage.commit(multi_tx).await;
        assert!(
            commit_res.is_ok(),
            "commit() succeeds even when Phase 3 consume_memory() exceeds budget limit (soft-fail warn)"
        );
        committed_count += 1;

        // Confirm that tracing::warn! was captured during commit
        assert!(
            warn_detected.load(Ordering::SeqCst),
            "Expected tracing::warn! log event when consume_memory exceeds budget limit in commit() Phase 3"
        );

        // 4. Now memory usage is > 95% limit. Subsequent put() calls MUST fail with Memory budget exceeded error.
        let overflow_tx = TxId::new(100);
        let put_overflow = storage.put(overflow_tx, b"overflow_key", &payload).await;
        if let Err(MemFuseError::Storage(err_msg)) = &put_overflow {
            assert!(
                err_msg.contains("Memory budget exceeded"),
                "Expected memory budget error message, got: {err_msg}"
            );
            hit_storage_error = true;
        } else {
            panic!(
                "Expected Err(MemFuseError::Storage(\"Memory budget exceeded...\")) on put after capacity exceeded, got: {:?}",
                put_overflow
            );
        }

        // Assertions for Teil A
        assert!(
            hit_storage_error,
            "Memory pressure error path MUST be triggered when budget capacity is exceeded!"
        );
        assert!(
            committed_count > 0,
            "At least one commit should succeed before memory budget is exhausted"
        );

        // RSS sanity check: process RSS must remain within reasonable bounds
        let current_rss = support::get_rss_bytes();
        if initial_rss > 0 {
            let rss_diff_mb = (current_rss.saturating_sub(initial_rss)) / (1024 * 1024);
            assert!(
                rss_diff_mb < 200,
                "Process RSS grew unexpectedly large under memory pressure: {rss_diff_mb} MB"
            );
        }
    }

    // =========================================================================
    // Teil B — ConcurrentWriteFlood (Replacement for RogueAgentFlood)
    // =========================================================================
    let flood_result = tokio::time::timeout(Duration::from_secs(30), async {
        let tmp = TempDir::new().expect("temp dir");
        let db_path = tmp.path().to_path_buf();
        let config = LsmConfig {
            path: db_path.clone(),
            // Constrain budget to 1 MB so concurrent writes contend on capacity
            max_ram_mb: 1,
            memtable_size_limit: 10 * 1024 * 1024,
            ..Default::default()
        };

        let storage = Arc::new(
            LsmStorage::new(config.clone())
                .await
                .expect("storage creation"),
        );

        // Thread-safe ground-truth list of successfully committed keys
        let ground_truth = Arc::new(tokio::sync::Mutex::new(Vec::<(Vec<u8>, Vec<u8>)>::new()));

        let num_tasks = 20;
        let mut handles = Vec::new();

        for task_idx in 1..=num_tasks {
            let storage = Arc::clone(&storage);
            let ground_truth = Arc::clone(&ground_truth);

            handles.push(tokio::spawn(async move {
                let tx = TxId::new(task_idx);
                let key = format!("flood_key_{:02}", task_idx).into_bytes();
                let val = format!("flood_val_{:02}_payload_data", task_idx).into_bytes();

                // Stage put
                if storage.put(tx, &key, &val).await.is_err() {
                    return;
                }

                // Commit
                if storage.commit(tx).await.is_ok() {
                    let mut gt = ground_truth.lock().await;
                    gt.push((key, val));
                }
            }));
        }

        for handle in handles {
            handle.await.expect("task join");
        }

        let gt_keys = ground_truth.lock().await.clone();

        // Assertion B.b: Successful commits must be fully readable, failed commits must not corrupt state
        for (key, expected_val) in &gt_keys {
            let retrieved = storage.get(key).await.expect("get operation");
            assert_eq!(
                retrieved,
                Some(expected_val.clone()),
                "Ground-truth key {:?} must be correctly stored and retrieved",
                String::from_utf8_lossy(key)
            );
        }

        // Close storage cleanly to flush memtables / WAL to disk
        storage.close().await.expect("close storage");
        drop(storage);

        // Assertion B.c: Reopening storage must deliver EXACTLY the ground-truth committed keys
        let reopened_config = LsmConfig {
            path: db_path,
            ..Default::default()
        };
        let reopened_storage = LsmStorage::new(reopened_config)
            .await
            .expect("reopen storage");

        let scanned_entries = reopened_storage
            .scan_prefix(b"flood_key_")
            .await
            .expect("scan_prefix on reopened storage");

        assert_eq!(
            scanned_entries.len(),
            gt_keys.len(),
            "Reopened storage key count ({}) must match ground-truth count ({}) exactly!",
            scanned_entries.len(),
            gt_keys.len()
        );

        let mut expected_sorted = gt_keys.clone();
        expected_sorted.sort_by(|a, b| a.0.cmp(&b.0));

        let mut actual_sorted = scanned_entries;
        actual_sorted.sort_by(|a, b| a.0.cmp(&b.0));

        assert_eq!(
            actual_sorted, expected_sorted,
            "Reopened storage state must match ground-truth committed data exactly!"
        );
    })
    .await;

    assert!(
        flood_result.is_ok(),
        "ConcurrentWriteFlood test timed out! Possible deadlock detected."
    );
}
