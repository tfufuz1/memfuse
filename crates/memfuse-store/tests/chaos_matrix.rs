// FILE-CONTEXT: Chaos testing matrix combining randomized fault scenarios with seed logging and ground truth verification. (TS: 2026-08-30) (SESSION: 283abf0f)
//! Chaos matrix integration tests combining randomized fault injection scenarios.
//!
//! Evaluates store durability, crash recovery, and non-corruption invariants under combined
//! fault injection (task cancellation, memory pressure, bit-flips, crash cycles).

use memfuse_core::{MemFuseError, StorageEngine, TxId};
use memfuse_store::lsm::{LsmConfig, LsmStorage};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tempfile::TempDir;

/// Resolves the test seed from `CHAOS_SEED` environment variable or generates a new random seed.
/// Logs and prints the seed to ensure reproducibility of any test failure.
fn resolve_and_log_seed() -> u64 {
    let seed: u64 = std::env::var("CHAOS_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(rand::random);

    println!("CHAOS_SEED={}", seed);
    tracing::info!("CHAOS_SEED={}", seed);
    seed
}

/// Independent ground truth map tracking successfully committed key-value pairs.
#[derive(Clone, Default)]
struct GroundTruth {
    inner: Arc<Mutex<BTreeMap<Vec<u8>, Vec<u8>>>>,
    tx_counter: Arc<AtomicU64>,
}

impl GroundTruth {
    fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(BTreeMap::new())),
            tx_counter: Arc::new(AtomicU64::new(1)),
        }
    }

    fn next_tx(&self) -> TxId {
        TxId::new(self.tx_counter.fetch_add(1, Ordering::SeqCst))
    }

    fn record_commit(&self, key: Vec<u8>, value: Vec<u8>) {
        let mut map = self.inner.lock().expect("ground truth lock");
        map.insert(key, value);
    }

    fn snapshot(&self) -> BTreeMap<Vec<u8>, Vec<u8>> {
        let map = self.inner.lock().expect("ground truth lock");
        map.clone()
    }
}

/// Combined Chaos Matrix Test: Task Massacre + Memory Pressure
#[tokio::test]
#[ignore]
async fn test_chaos_matrix_task_massacre_and_memory_pressure() {
    let seed = resolve_and_log_seed();
    let mut rng = StdRng::seed_from_u64(seed);

    let tmp = TempDir::new().expect("temp dir");
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        memtable_size_limit: 16 * 1024, // 16 KB tiny limit to force frequent flushes
        max_ram_mb: 32,                 // Tight RAM limit
        tx_timeout: Duration::from_secs(5),
        ..Default::default()
    };

    let storage = Arc::new(LsmStorage::new(config.clone()).await.expect("storage init"));
    let ground_truth = GroundTruth::new();

    let num_tasks = rng.gen_range(10..20);
    let mut handles = Vec::new();

    for task_idx in 0..num_tasks {
        let storage_clone = Arc::clone(&storage);
        let gt_clone = ground_truth.clone();
        let task_seed = rng.gen::<u64>();

        let handle = tokio::spawn(async move {
            let mut task_rng = StdRng::seed_from_u64(task_seed);
            for i in 0..20 {
                let tx = gt_clone.next_tx();
                let key = format!("task_{}_{}", task_idx, i).into_bytes();
                let val = format!("payload_data_val_{}_{}", task_idx, i).into_bytes();

                if storage_clone.put(tx, &key, &val).await.is_ok() {
                    // Small random delay before commit
                    if task_rng.gen_bool(0.3) {
                        tokio::time::sleep(Duration::from_millis(1)).await;
                    }
                    if storage_clone.commit(tx).await.is_ok() {
                        gt_clone.record_commit(key, val);
                    }
                }
            }
        });

        handles.push(handle);
    }

    // Task Massacre: Randomly abort 40% of running tasks
    for handle in &handles {
        if rng.gen_bool(0.4) {
            handle.abort();
        }
    }

    // Await non-aborted tasks
    for handle in handles {
        let _ = handle.await;
    }

    // Force flush to exercise memory pressure handling
    storage.force_flush().await.ok();

    // Ground Truth Discipline Check: All recorded committed keys MUST match ground truth exactly
    let expected = ground_truth.snapshot();
    for (k, expected_v) in &expected {
        let actual_v = storage.get(k).await.expect("get committed key");
        assert_eq!(
            actual_v,
            Some(expected_v.clone()),
            "Ground truth violation for key {:?}",
            String::from_utf8_lossy(k)
        );
    }
}

/// Combined Chaos Matrix Test: Bit-Flip Fault Injection + Crash Recovery Cycles
#[tokio::test]
#[ignore]
async fn test_chaos_matrix_bitflip_and_crash_recovery() {
    let seed = resolve_and_log_seed();
    let mut rng = StdRng::seed_from_u64(seed);

    let tmp = TempDir::new().expect("temp dir");
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        memtable_size_limit: 8 * 1024,
        max_ram_mb: 64,
        ..Default::default()
    };

    let ground_truth = GroundTruth::new();

    // Cycle 1: Populating initial data
    {
        let storage = LsmStorage::new(config.clone()).await.expect("storage init");
        for i in 0..15 {
            let tx = ground_truth.next_tx();
            let k = format!("cycle1_k_{}", i).into_bytes();
            let v = format!("cycle1_v_{}", i).into_bytes();
            storage.put(tx, &k, &v).await.expect("put");
            storage.commit(tx).await.expect("commit");
            ground_truth.record_commit(k, v);
        }
        storage.force_flush().await.expect("flush");
        // Ungraceful drop without close()
        drop(storage);
    }

    // Cycle 2: Corrupting non-essential / tail bytes in WAL or SSTable and reopening
    let mut data_entries = tokio::fs::read_dir(tmp.path()).await.expect("read dir");
    let mut target_file = None;

    while let Ok(Some(entry)) = data_entries.next_entry().await {
        let path = entry.path();
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if name.ends_with(".wal") || name.ends_with(".sst") || name.ends_with(".log") {
            target_file = Some(path);
            break;
        }
    }

    if let Some(file_path) = target_file {
        if let Ok(mut file_bytes) = tokio::fs::read(&file_path).await {
            if file_bytes.len() > 16 {
                // Flip a bit in the file
                let flip_idx = rng.gen_range(8..file_bytes.len());
                file_bytes[flip_idx] ^= 0x01 << rng.gen_range(0..8);
                tokio::fs::write(&file_path, &file_bytes).await.ok();
            }
        }
    }

    // Reopen storage post corruption: Must either report clean error or recover without panic
    let reopen_res = LsmStorage::new(config.clone()).await;
    match reopen_res {
        Ok(storage) => {
            // If storage opened successfully, verify that queries do not panic
            let _ = storage.get(b"cycle1_k_0").await;
        }
        Err(e) => {
            // Clean error propagation (WalCorruption, Storage, ChecksumMismatch, Crypto, or Io error)
            assert!(
                matches!(
                    e,
                    MemFuseError::Storage(_)
                        | MemFuseError::Io(_)
                        | MemFuseError::WalCorruption { .. }
                        | MemFuseError::ChecksumMismatch { .. }
                        | MemFuseError::Crypto(_)
                ),
                "Expected clean MemFuseError on corrupted storage reopen, got: {:?}",
                e
            );
        }
    }
}

/// Combined Chaos Matrix Test: Multi-scenario sequence (Task Massacre -> Power Cut -> Recovery)
#[tokio::test]
#[ignore]
async fn test_chaos_matrix_full_combos() {
    let seed = resolve_and_log_seed();
    let mut rng = StdRng::seed_from_u64(seed);

    let tmp = TempDir::new().expect("temp dir");
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        memtable_size_limit: 32 * 1024,
        max_ram_mb: 64,
        tx_timeout: Duration::from_secs(10),
        ..Default::default()
    };

    let ground_truth = GroundTruth::new();

    // Stage 1: Write initial batch
    {
        let storage = LsmStorage::new(config.clone()).await.expect("init stage 1");
        for i in 0..30 {
            let tx = ground_truth.next_tx();
            let k = format!("combo_k_{:03}", i).into_bytes();
            let v = format!("combo_v_{:03}", i).into_bytes();
            storage.put(tx, &k, &v).await.expect("put");
            storage.commit(tx).await.expect("commit");
            ground_truth.record_commit(k, v);

            if i % 10 == 0 {
                storage.force_flush().await.ok();
            }
        }
        // Simulated power-cut: drop storage without close()
        drop(storage);
    }

    // Stage 2: Reopen and perform randomized operations
    let storage = Arc::new(LsmStorage::new(config.clone()).await.expect("init stage 2"));

    // Verify Stage 1 ground truth
    let expected_stage1 = ground_truth.snapshot();
    for (k, v) in &expected_stage1 {
        let actual = storage.get(k).await.expect("get stage 1");
        assert_eq!(actual, Some(v.clone()), "Stage 1 ground truth match");
    }

    // Concurrent writing with random aborts
    let mut worker_handles = Vec::new();
    for worker_id in 0..5 {
        let s_clone = Arc::clone(&storage);
        let gt_clone = ground_truth.clone();
        let worker_seed = rng.gen::<u64>();

        let handle = tokio::spawn(async move {
            let mut w_rng = StdRng::seed_from_u64(worker_seed);
            for i in 0..10 {
                let tx = gt_clone.next_tx();
                let k = format!("stage2_w{}_{}", worker_id, i).into_bytes();
                let v = format!("val_w{}_{}", worker_id, i).into_bytes();

                if s_clone.put(tx, &k, &v).await.is_ok() {
                    if w_rng.gen_bool(0.2) {
                        tokio::task::yield_now().await;
                    }
                    if s_clone.commit(tx).await.is_ok() {
                        gt_clone.record_commit(k, v);
                    }
                }
            }
        });

        worker_handles.push(handle);
    }

    // Abort 2 workers
    if worker_handles.len() >= 2 {
        worker_handles[0].abort();
        worker_handles[1].abort();
    }

    for h in worker_handles {
        let _ = h.await;
    }

    // Final ground truth verification
    let final_expected = ground_truth.snapshot();
    for (k, v) in &final_expected {
        let actual = storage.get(k).await.expect("get final");
        assert_eq!(
            actual,
            Some(v.clone()),
            "Final ground truth match for key {:?}",
            String::from_utf8_lossy(k)
        );
    }
}
