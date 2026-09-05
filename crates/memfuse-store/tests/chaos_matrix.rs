//! Combined Chaos Matrix Fault-Injection Test Suite (`crates/memfuse-store/tests/chaos_matrix.rs`).
//!
//! Executes multiple fault-injection scenarios (TaskMassacre, PowerCutSimulation,
//! BitFlipInjection, MemoryPressure / ConcurrentWriteFlood) in a randomized order
//! driven by a logged, reproducible seed.
//!
//! Ground-Truth Discipline:
//! - All expected committed KV state is tracked in an independent in-memory ground-truth map
//!   outside `LsmStorage`.
//! - Committed keys are verified against `LsmStorage::get()` after fault injection and recovery.
//! - Random bit-flips on SSTable/WAL files must be handled cleanly (zero panic).

use memfuse_core::{Result, StorageEngine, TxId};
use memfuse_store::lsm::{LsmConfig, LsmStorage};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::Mutex;

/// Scenario A: Task Massacre + Concurrent Write Flood.
/// Spawns worker tasks performing concurrent writes/commits while a massacre controller
/// randomly aborts writer handles. Verifies all successfully committed keys survive in storage.
async fn run_scenario_task_massacre(
    rng: &mut StdRng,
    step_path: std::path::PathBuf,
    tx_counter: Arc<AtomicU64>,
) -> Result<()> {
    tracing::info!("Executing Scenario: Task Massacre + Concurrent Write Flood");

    let config = LsmConfig {
        path: step_path.clone(),
        memtable_size_limit: 64 * 1024,
        max_ram_mb: 64,
        tx_timeout: Duration::from_secs(60),
        encryption_passphrase: None,
        ..Default::default()
    };

    let storage = Arc::new(LsmStorage::new(config).await?);
    let ground_truth = Arc::new(Mutex::new(HashMap::new()));
    let mut handles = Vec::new();
    let num_tasks = rng.gen_range(6..=12);

    for task_id in 0..num_tasks {
        let storage_cl = storage.clone();
        let ground_truth_cl = ground_truth.clone();
        let tx_counter_cl = tx_counter.clone();

        let handle = tokio::spawn(async move {
            for i in 0..20 {
                let tx_num = tx_counter_cl.fetch_add(1, Ordering::SeqCst);
                let tx = TxId::new(tx_num);
                let key = format!("massacre-task-{}-key-{}", task_id, i).into_bytes();
                let val = format!("massacre-task-{}-val-{}", task_id, i).into_bytes();

                if storage_cl.put(tx, &key, &val).await.is_err() {
                    break;
                }

                if storage_cl.commit(tx).await.is_ok() {
                    // Record in ground-truth ONLY after successful commit
                    let mut gt = ground_truth_cl.lock().await;
                    gt.insert(key, val);
                }
            }
        });
        handles.push(handle);
    }

    // Abort roughly half of the tasks midway
    let abort_count = num_tasks / 2;
    for _ in 0..abort_count {
        if !handles.is_empty() {
            let victim_idx = rng.gen_range(0..handles.len());
            let victim_handle = handles.remove(victim_idx);
            victim_handle.abort();
        }
    }

    // Wait for remaining handles
    for handle in handles {
        let _ = handle.await;
    }

    // Force flush to ensure state persistence
    storage.force_flush().await?;

    // Verify ground truth state against storage
    let gt = ground_truth.lock().await;
    for (k, expected_v) in gt.iter() {
        let actual_v = storage.get(k).await?;
        assert_eq!(
            actual_v.as_ref(),
            Some(expected_v),
            "Ground-truth key mismatch post Task Massacre"
        );
    }

    Ok(())
}

/// Scenario B: Power Cut Simulation.
/// Writes batch operations with commits, abruptly drops `LsmStorage` instance without
/// graceful shutdown/flush, reopens storage from the same directory, and verifies WAL recovery.
async fn run_scenario_power_cut(
    rng: &mut StdRng,
    step_path: std::path::PathBuf,
    tx_counter: Arc<AtomicU64>,
) -> Result<()> {
    tracing::info!("Executing Scenario: Power Cut Simulation");

    let config = LsmConfig {
        path: step_path.clone(),
        memtable_size_limit: 128 * 1024,
        max_ram_mb: 64,
        tx_timeout: Duration::from_secs(60),
        encryption_passphrase: None,
        ..Default::default()
    };

    let ground_truth = Arc::new(Mutex::new(HashMap::new()));

    // Phase 1: Write and abruptly drop storage (simulating sudden SIGKILL / power loss)
    {
        let storage = LsmStorage::new(config.clone()).await?;
        let batch_count = rng.gen_range(15..=30);

        for i in 0..batch_count {
            let tx_num = tx_counter.fetch_add(1, Ordering::SeqCst);
            let tx = TxId::new(tx_num);
            let key = format!("powercut-key-{}", i).into_bytes();
            let val = format!("powercut-val-{}", i).into_bytes();

            storage.put(tx, &key, &val).await?;
            storage.commit(tx).await?;

            let mut gt = ground_truth.lock().await;
            gt.insert(key, val);
        }

        // Abrupt drop without calling force_flush or wait_shutdown
        drop(storage);
    }

    // Phase 2: Reopen storage from disk and verify WAL replay
    {
        let storage = LsmStorage::new(config).await?;
        let gt = ground_truth.lock().await;

        for (k, expected_v) in gt.iter() {
            let actual_v = storage.get(k).await?;
            assert_eq!(
                actual_v.as_ref(),
                Some(expected_v),
                "Ground-truth key missing after power cut reopen and WAL replay"
            );
        }
    }

    Ok(())
}

/// Scenario C: Bit-Flip Corruption Injection.
/// Injects random bit-flips into SSTable or WAL files on disk and verifies that reopening
/// and querying `LsmStorage` NEVER panics (zero-panic invariant).
async fn run_scenario_bit_flip_injection(
    rng: &mut StdRng,
    step_path: std::path::PathBuf,
    tx_counter: Arc<AtomicU64>,
) -> Result<()> {
    tracing::info!("Executing Scenario: Bit-Flip Corruption Injection");

    let config = LsmConfig {
        path: step_path.clone(),
        memtable_size_limit: 16 * 1024,
        max_ram_mb: 64,
        tx_timeout: Duration::from_secs(60),
        encryption_passphrase: None,
        ..Default::default()
    };

    // Phase 1: Write data and force flush to generate SSTable and WAL files
    {
        let storage = LsmStorage::new(config.clone()).await?;
        for i in 0..20 {
            let tx_num = tx_counter.fetch_add(1, Ordering::SeqCst);
            let tx = TxId::new(tx_num);
            let key = format!("bitflip-key-{}", i).into_bytes();
            let val = format!("bitflip-val-{}", i).into_bytes();

            storage.put(tx, &key, &val).await?;
            storage.commit(tx).await?;
        }
        storage.force_flush().await?;
        drop(storage);
    }

    // Phase 2: Identify files on disk and corrupt random bytes in SST or WAL files
    let mut files_to_corrupt = Vec::new();
    let mut entries = tokio::fs::read_dir(&step_path).await?;
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".sst") || (name.starts_with("wal-") && name.ends_with(".log")) {
            files_to_corrupt.push(entry.path());
        }
    }

    if !files_to_corrupt.is_empty() {
        let target_path = &files_to_corrupt[rng.gen_range(0..files_to_corrupt.len())];
        if let Ok(mut bytes) = tokio::fs::read(target_path).await {
            if bytes.len() > 16 {
                // Flip random bit in payload (skip header bytes)
                let flip_offset = rng.gen_range(12..bytes.len());
                let bit_idx = rng.gen_range(0..8);
                bytes[flip_offset] ^= 1 << bit_idx;
                tokio::fs::write(target_path, bytes).await?;
            }
        }
    }

    // Phase 3: Reopen storage and query — MUST NOT panic!
    let reopen_res = LsmStorage::new(config).await;
    match reopen_res {
        Ok(storage) => {
            // Attempt gets on key space — errors or None are acceptable, panics are forbidden
            for i in 0..20 {
                let key = format!("bitflip-key-{}", i).into_bytes();
                let _ = storage.get(&key).await;
            }
        }
        Err(e) => {
            tracing::info!(
                "Reopen gracefully rejected corrupted storage as expected: {}",
                e
            );
        }
    }

    Ok(())
}

/// Scenario D: Memory Pressure & Low Budget Flushes.
/// Executes heavy writes with an artificially low memtable size limit (512 bytes)
/// to trigger continuous automatic flushes. Verifies ground-truth state consistency.
async fn run_scenario_memory_pressure(
    rng: &mut StdRng,
    step_path: std::path::PathBuf,
    tx_counter: Arc<AtomicU64>,
) -> Result<()> {
    tracing::info!("Executing Scenario: Memory Pressure & Low Budget Flushes");

    let config = LsmConfig {
        path: step_path.clone(),
        memtable_size_limit: 512, // Ultra low budget forcing rapid flushes
        max_ram_mb: 16,
        tx_timeout: Duration::from_secs(60),
        encryption_passphrase: None,
        ..Default::default()
    };

    let storage = LsmStorage::new(config).await?;
    let ground_truth = Arc::new(Mutex::new(HashMap::new()));
    let write_count = rng.gen_range(25..=50);

    for i in 0..write_count {
        let tx_num = tx_counter.fetch_add(1, Ordering::SeqCst);
        let tx = TxId::new(tx_num);
        let key = format!("mempress-key-{}", i).into_bytes();
        let val = format!("mempress-val-{}", i).into_bytes();

        storage.put(tx, &key, &val).await?;
        storage.commit(tx).await?;

        let mut gt = ground_truth.lock().await;
        gt.insert(key, val);
    }

    storage.force_flush().await?;

    // Verify all ground truth entries
    let gt = ground_truth.lock().await;
    for (k, expected_v) in gt.iter() {
        let actual_v = storage.get(k).await?;
        assert_eq!(
            actual_v.as_ref(),
            Some(expected_v),
            "Ground-truth key missing under memory pressure"
        );
    }

    Ok(())
}

/// Main entry point for the combined chaos matrix test suite.
///
/// Logged Seed Requirement:
/// The seed MUST be logged via `println!` / `tracing::info!` so any failure is reproducible.
///
/// Seed Override:
/// Can be overridden via `CHAOS_SEED=<u64>` environment variable.
#[tokio::test]
#[ignore]
async fn test_chaos_matrix_combined() -> Result<()> {
    // 1. Determine Seed (ENV override or random)
    let seed: u64 = if let Ok(s) = std::env::var("CHAOS_SEED") {
        s.parse()
            .expect("Invalid CHAOS_SEED environment variable, must be u64")
    } else {
        rand::thread_rng().gen::<u64>()
    };

    println!("\n=======================================================");
    println!("CHAOS MATRIX SEED: {}", seed);
    println!("=======================================================\n");
    tracing::info!("CHAOS MATRIX SEED: {}", seed);

    let mut rng = StdRng::seed_from_u64(seed);
    let tmp_dir = TempDir::new().expect("temp dir for chaos matrix");
    let base_path = tmp_dir.path().to_path_buf();

    let tx_counter = Arc::new(AtomicU64::new(1));

    // 2. Randomized Scenario Order
    let mut scenario_ids = vec!["TaskMassacre", "PowerCut", "MemoryPressure", "BitFlip"];

    // Fisher-Yates shuffle using seeded RNG
    for i in (1..scenario_ids.len()).rev() {
        let j = rng.gen_range(0..=i);
        scenario_ids.swap(i, j);
    }

    println!("Randomized Scenario Sequence: {:?}", scenario_ids);

    // 3. Execute Scenarios in Randomized Order with isolated step directories
    for (step_num, scenario_id) in scenario_ids.iter().enumerate() {
        println!(
            "--- Step {}/{} Scenario: {} ---",
            step_num + 1,
            scenario_ids.len(),
            scenario_id
        );
        let step_path = base_path.join(format!("step-{}-{}", step_num + 1, scenario_id));
        tokio::fs::create_dir_all(&step_path).await?;

        match *scenario_id {
            "TaskMassacre" => {
                run_scenario_task_massacre(&mut rng, step_path, tx_counter.clone()).await?;
            }
            "PowerCut" => {
                run_scenario_power_cut(&mut rng, step_path, tx_counter.clone()).await?;
            }
            "MemoryPressure" => {
                run_scenario_memory_pressure(&mut rng, step_path, tx_counter.clone()).await?;
            }
            "BitFlip" => {
                run_scenario_bit_flip_injection(&mut rng, step_path, tx_counter.clone()).await?;
            }
            _ => unreachable!(),
        }
    }

    println!("\n=======================================================");
    println!("CHAOS MATRIX PASSED (Seed: {})", seed);
    println!("=======================================================\n");

    Ok(())
}
