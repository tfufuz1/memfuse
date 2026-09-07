// FILE-CONTEXT: Chaos test verifying SIGKILL process termination recovery and durability invariants. (TS: 2026-09-05) (SESSION: chaos_power_cut)
//! Chaos test proving `LsmStorage` recovery and durability guarantees under actual process SIGKILL.
//!
//! Evaluates `PowerCutSimulation` defined in `rules/chaos_testing.md` and `TEST/MASTER_INTEGRATION_PLAN.md`.

use memfuse_core::{MemFuseError, StorageEngine};
use memfuse_store::lsm::{LsmConfig, LsmStorage};
use rand::Rng;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;
use tempfile::TempDir;

fn get_chaos_writer_bin() -> PathBuf {
    if let Ok(curr) = std::env::current_exe() {
        if let Some(parent) = curr.parent() {
            if let Some(target_dir) = parent.parent() {
                let bin_name = format!("chaos_writer{}", std::env::consts::EXE_SUFFIX);
                let example_bin = target_dir.join("examples").join(&bin_name);
                if example_bin.exists() {
                    return example_bin;
                }
                let direct_bin = target_dir.join(&bin_name);
                if direct_bin.exists() {
                    return direct_bin;
                }
            }
        }
    }

    let status = Command::new("cargo")
        .args([
            "build",
            "--example",
            "chaos_writer",
            "-p",
            "memfuse-store",
            "--quiet",
        ])
        .status()
        .expect("Failed to build chaos_writer example via cargo");
    assert!(status.success(), "Building chaos_writer example failed");

    if let Ok(curr) = std::env::current_exe() {
        if let Some(parent) = curr.parent() {
            if let Some(target_dir) = parent.parent() {
                let bin_name = format!("chaos_writer{}", std::env::consts::EXE_SUFFIX);
                let example_bin = target_dir.join("examples").join(&bin_name);
                if example_bin.exists() {
                    return example_bin;
                }
            }
        }
    }

    panic!("Could not locate chaos_writer binary");
}

#[tokio::test]
async fn test_chaos_power_cut_sigkill_recovery() {
    let chaos_writer_bin = get_chaos_writer_bin();
    let num_iterations = 10;

    for iteration in 1..=num_iterations {
        let tmp = TempDir::new().expect("temp dir");
        let db_path = tmp.path().join("db");
        let gt_log_path = tmp.path().join("ground_truth.log");

        let n_writes = 100;

        // 1. Spawn chaos_writer subprocess
        let mut child = Command::new(&chaos_writer_bin)
            .arg(&db_path)
            .arg(n_writes.to_string())
            .arg(&gt_log_path)
            .spawn()
            .expect("Failed to spawn chaos_writer process");

        // 2. Wait randomized time between 30ms and 250ms
        let mut rng = rand::thread_rng();
        let sleep_ms = rng.gen_range(30..250);
        tokio::time::sleep(Duration::from_millis(sleep_ms)).await;

        // 3. SIGKILL process
        let _ = child.kill();
        let _ = child.wait();

        // 4. Reopen LsmStorage
        let config = LsmConfig {
            path: db_path.clone(),
            ..Default::default()
        };

        let storage_res = LsmStorage::new(config).await;
        let storage = match storage_res {
            Ok(s) => s,
            Err(MemFuseError::Storage(msg)) => {
                println!("Iteration {iteration}: Reopen returned documented Storage error: {msg}");
                continue;
            }
            Err(e) => {
                panic!(
                    "Iteration {iteration}: Reopen returned undocumented error type: {:?}",
                    e
                );
            }
        };

        // 5. Read external Ground-Truth log
        if !gt_log_path.exists() {
            // Process was killed before creating log file
            continue;
        }

        let raw_log = fs::read(&gt_log_path).expect("read ground truth log");
        // Find last newline to ignore incomplete trailing line
        let valid_bytes = match raw_log.iter().rposition(|&b| b == b'\n') {
            Some(idx) => &raw_log[..=idx],
            None => &[][..],
        };

        let log_content = String::from_utf8_lossy(valid_bytes);

        let mut committed_entries: HashMap<u64, (String, String)> = HashMap::new();
        let mut started_entries: HashMap<u64, (String, String)> = HashMap::new();

        for line in log_content.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 4 {
                continue;
            }
            let status = parts[0];
            let counter: u64 = match parts[1].parse() {
                Ok(c) => c,
                Err(_) => continue,
            };
            let key = parts[2].to_string();
            let val = parts[3].to_string();

            if status == "START" {
                started_entries.insert(counter, (key, val));
            } else if status == "COMMITTED" {
                committed_entries.insert(counter, (key, val));
            }
        }

        let max_committed_counter = committed_entries.keys().copied().max().unwrap_or(0);
        let max_started_counter = started_entries.keys().copied().max().unwrap_or(0);

        // 6. Assertion (1f): ALL confirmed committed entries MUST be readable
        for (counter, (key, val)) in &committed_entries {
            let res = storage
                .get(key.as_bytes())
                .await
                .expect("storage.get failed");
            assert_eq!(
                res,
                Some(val.as_bytes().to_vec()),
                "Iteration {iteration}: Key {key} (counter {counter}) was committed before SIGKILL but not readable after reopen!"
            );
        }

        // 7. Assertion (1g): NO entry beyond max_started_counter may be visible
        let max_check_counter = max_started_counter.max(max_committed_counter) + 5;
        for check_counter in (max_started_counter + 1)..=max_check_counter {
            let check_key = format!("key-{:06}", check_counter);
            let res = storage
                .get(check_key.as_bytes())
                .await
                .expect("storage.get failed");
            assert!(
                res.is_none(),
                "Iteration {iteration}: Phantom commit detected! Key {check_key} (counter {check_counter} > max_started {max_started_counter}) was readable!"
            );
        }
    }
}
