mod support;

use memfuse_core::{Result, StorageEngine, TxId};
use memfuse_store::lsm::{LsmConfig, LsmStorage};
use support::get_rss_bytes;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tempfile::TempDir;
use tracing::Subscriber;
use tracing_subscriber::layer::Context;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::Layer;

#[derive(Clone, Default)]
struct LogCaptureLayer {
    logs: Arc<Mutex<Vec<String>>>,
}

impl<S: Subscriber> Layer<S> for LogCaptureLayer {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = StringVisitor(String::new());
        event.record(&mut visitor);
        if let Ok(mut guard) = self.logs.lock() {
            guard.push(visitor.0);
        }
    }
}

struct StringVisitor(String);
impl tracing::field::Visit for StringVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if !self.0.is_empty() {
            self.0.push_str(" ");
        }
        self.0.push_str(&format!("{}: {:?}", field.name(), value));
    }
}

#[tokio::test]
async fn test_chaos_memory_pressure_sequential() -> Result<()> {
    let capture_layer = LogCaptureLayer::default();
    let subscriber = tracing_subscriber::registry().with(capture_layer.clone());
    let _guard = tracing::subscriber::set_default(subscriber);

    // --- Scenario A: Incremental writes crossing 95% threshold ---
    let tmp1 = TempDir::new().expect("temp dir");
    let config1 = LsmConfig {
        path: tmp1.path().to_path_buf(),
        max_ram_mb: 1, // 1 MB limit (95% threshold = 996,147 bytes)
        memtable_size_limit: 10 * 1024 * 1024,
        ..Default::default()
    };

    let storage1 = LsmStorage::new(config1).await?;
    let initial_rss = get_rss_bytes();

    let mut incremental_error = None;
    let mut successful_commits = 0;

    // Write 10 KB entries incrementally so memory_used increases past 95% without exceeding 100% in a single step
    for i in 1..=200u64 {
        let tx = TxId::new(i);
        let k = format!("inc_key_{:05}", i).into_bytes();
        let v = vec![b'a'; 10 * 1024]; // 10 KB payload

        if let Err(e) = storage1.put(tx, &k, &v).await {
            incremental_error = Some(e);
            break;
        }

        if let Err(e) = storage1.commit(tx).await {
            incremental_error = Some(e);
            break;
        }

        successful_commits += 1;
    }

    println!("Incremental writes: {} successful commits before 95% capacity rejection", successful_commits);

    let err = incremental_error.expect("Incremental memory pressure MUST trigger a capacity error");
    let err_msg = format!("{}", err);
    assert!(
        err_msg.contains("Memory budget exceeded"),
        "Expected error message to contain 'Memory budget exceeded', got: {}",
        err_msg
    );

    // --- Scenario B: Soft-fail warning during commit on single large jump exceeding 100% ---
    let tmp2 = TempDir::new().expect("temp dir");
    let config2 = LsmConfig {
        path: tmp2.path().to_path_buf(),
        max_ram_mb: 1, // 1 MB limit (1,048,576 bytes)
        memtable_size_limit: 10 * 1024 * 1024,
        ..Default::default()
    };

    let storage2 = LsmStorage::new(config2).await?;

    // Fill to ~900 KB
    let tx_fill = TxId::new(1000);
    let k_fill = b"fill_key_900k".to_vec();
    let v_fill = vec![b'x'; 900 * 1024];
    storage2.put(tx_fill, &k_fill, &v_fill).await?;
    storage2.commit(tx_fill).await?;

    // Write a 200 KB entry when usage is at 900 KB (900 KB + 200 KB = 1100 KB > 1024 KB limit).
    // put() passes (900 KB < 996 KB 95% threshold).
    // commit() Phase 3 consume_memory() returns Err(MemoryBudgetExceeded),
    // which commit() catches, logs as tracing::warn!, and soft-fails (returns Ok(())).
    let tx_over = TxId::new(1001);
    let k_over = b"over_key_200k".to_vec();
    let v_over = vec![b'y'; 200 * 1024];
    storage2.put(tx_over, &k_over, &v_over).await?;
    let commit_res = storage2.commit(tx_over).await;
    assert!(
        commit_res.is_ok(),
        "commit() soft-fails when budget.consume_memory() fails, returning Ok(())"
    );

    // Verify soft-fail warning in tracing logs
    let logs = capture_layer.logs.lock().unwrap();
    let soft_fail_logged = logs
        .iter()
        .any(|msg| msg.contains("Memory budget tracking warning during commit"));
    println!("Soft-fail warning logged during commit: {}", soft_fail_logged);
    assert!(
        soft_fail_logged,
        "Expected tracing::warn! log for commit consume_memory soft-fail in lsm.rs"
    );

    // Assertion 2b: Process RSS does not go out of memory
    let final_rss = get_rss_bytes();
    if initial_rss > 0 {
        let rss_increase_mb = (final_rss.saturating_sub(initial_rss)) / (1024 * 1024);
        println!("RSS Increase under memory pressure: {} MB", rss_increase_mb);
        assert!(
            rss_increase_mb <= 100,
            "Process RSS growth exceeded 100 MB under memory pressure: {} MB",
            rss_increase_mb
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_chaos_memory_pressure_concurrent_write_flood() -> Result<()> {
    // Wrap entire test in timeout to enforce Assertion 3a (no deadlocks)
    tokio::time::timeout(Duration::from_secs(30), async {
        let tmp = TempDir::new().expect("temp dir");
        let config = LsmConfig {
            path: tmp.path().to_path_buf(),
            max_ram_mb: 1, // 1 MB budget
            memtable_size_limit: 10 * 1024 * 1024,
            ..Default::default()
        };

        let storage = Arc::new(LsmStorage::new(config.clone()).await?);
        let ground_truth = Arc::new(Mutex::new(HashMap::<Vec<u8>, Vec<u8>>::new()));

        let num_tasks = 20;
        let mut handles = Vec::new();

        for task_id in 1..=num_tasks {
            let storage_clone = Arc::clone(&storage);
            let ground_truth_clone = Arc::clone(&ground_truth);

            let handle = tokio::spawn(async move {
                for item_idx in 1..=30u64 {
                    let tx_num = (task_id as u64) * 1000 + item_idx;
                    let tx = TxId::new(tx_num);
                    let k = format!("flood_k_{}_{}", task_id, item_idx).into_bytes();
                    let v = format!("flood_v_{}_{}_payload_{}", task_id, item_idx, "x".repeat(2048)).into_bytes();

                    let put_res = storage_clone.put(tx, &k, &v).await;
                    if put_res.is_ok() {
                        let commit_res = storage_clone.commit(tx).await;
                        if commit_res.is_ok() {
                            let mut gt = ground_truth_clone.lock().unwrap();
                            gt.insert(k, v);
                        }
                    }
                }
            });

            handles.push(handle);
        }

        for handle in handles {
            handle.await.expect("task join");
        }

        let gt_map = ground_truth.lock().unwrap().clone();
        println!("Concurrent write flood: {} total successfully committed keys", gt_map.len());

        // Assertion 3b: Successful tasks maintained consistent state on live storage
        for (k, expected_v) in &gt_map {
            let val = storage.get(k).await?;
            assert_eq!(
                val,
                Some(expected_v.clone()),
                "Live storage lookup mismatch for committed key {:?}",
                String::from_utf8_lossy(k)
            );
        }

        // Assertion 3c: Reopen delivers EXACT state of committed keys
        storage.close().await?;
        drop(storage);

        let reopened = LsmStorage::new(config).await?;

        for (k, expected_v) in &gt_map {
            let val = reopened.get(k).await?;
            assert_eq!(
                val,
                Some(expected_v.clone()),
                "Reopened storage lookup mismatch for committed key {:?}",
                String::from_utf8_lossy(k)
            );
        }

        let all_scanned = reopened.scan_prefix(b"flood_k_").await?;
        assert_eq!(
            all_scanned.len(),
            gt_map.len(),
            "Reopened storage key count ({}) does not match ground truth count ({})",
            all_scanned.len(),
            gt_map.len()
        );

        Ok::<(), memfuse_core::MemFuseError>(())
    })
    .await
    .expect("Test execution timed out! Possible deadlock in ConcurrentWriteFlood")?;

    Ok(())
}
