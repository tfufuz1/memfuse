use memfuse_core::StorageEngine;
use memfuse_store::lsm::{LsmConfig, LsmStorage};
use rand::Rng;
use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;
use tempfile::tempdir;

const ITERATIONS: usize = 10;
const WRITES_PER_RUN: usize = 100;

fn parse_ground_truth_log(path: &Path) -> (HashMap<String, Vec<u8>>, Option<usize>) {
    let mut confirmed_commits = HashMap::new();
    let mut max_prepared_idx = None;

    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return (confirmed_commits, max_prepared_idx),
    };

    let has_trailing_newline = content.ends_with('\n');
    let mut lines: Vec<&str> = content.lines().collect();
    if !has_trailing_newline && !lines.is_empty() {
        // Discard incomplete last line resulting from crash during log write
        lines.pop();
    }

    for line in lines {
        let parts: Vec<&str> = line.splitn(3, ':').collect();
        if parts.len() == 3 {
            let status = parts[0];
            let key = parts[1];
            let val = parts[2];

            if let Some(idx_str) = key.strip_prefix("chaos_key_") {
                if let Ok(idx) = idx_str.parse::<usize>() {
                    max_prepared_idx = Some(max_prepared_idx.map_or(idx, |m: usize| m.max(idx)));
                }
            }

            if status == "COMMIT" {
                confirmed_commits.insert(key.to_string(), val.as_bytes().to_vec());
            }
        }
    }

    (confirmed_commits, max_prepared_idx)
}

#[tokio::test]
async fn test_chaos_power_cut_recovery() {
    // 1. Ensure chaos_writer example binary is built before running iterations
    let build_status = Command::new("cargo")
        .args(["build", "--quiet", "-p", "memfuse-store", "--example", "chaos_writer"])
        .status()
        .expect("Failed to build chaos_writer example");
    assert!(
        build_status.success(),
        "Building chaos_writer example failed"
    );

    let mut rng = rand::thread_rng();

    for iter in 0..ITERATIONS {
        println!("--- Chaos Power-Cut Iteration {}/{} ---", iter + 1, ITERATIONS);

        let tmp = tempdir().expect("tempdir");
        let storage_path = tmp.path().join("storage");
        let log_path = tmp.path().join("ground_truth.log");

        std::fs::create_dir_all(&storage_path).expect("create storage dir");

        // 2. Spawn chaos_writer subprocess
        let mut child = Command::new("cargo")
            .args([
                "run",
                "--quiet",
                "-p",
                "memfuse-store",
                "--example",
                "chaos_writer",
                "--",
                storage_path.to_str().expect("valid storage path"),
                &WRITES_PER_RUN.to_string(),
                log_path.to_str().expect("valid log path"),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("Failed to spawn chaos_writer subprocess");

        // 3. Wait a randomized duration between 50ms and 500ms
        let sleep_ms: u64 = rng.gen_range(50..=500);
        tokio::time::sleep(Duration::from_millis(sleep_ms)).await;

        // 4. Force SIGKILL / TerminateProcess
        let _ = child.kill();
        let _ = child.wait(); // Reap zombie child process

        // 5. Read ground-truth log file before reopen
        let (confirmed_commits, max_prepared_idx) = parse_ground_truth_log(&log_path);
        println!(
            "Iteration {}: Found {} confirmed commits, max prepared index: {:?}",
            iter + 1,
            confirmed_commits.len(),
            max_prepared_idx
        );

        // 6. Reopen LsmStorage at the same path
        let config = LsmConfig {
            path: storage_path.clone(),
            ..Default::default()
        };

        let reopen_res = LsmStorage::new(config).await;
        let storage = match reopen_res {
            Ok(s) => s,
            Err(e) => {
                panic!(
                    "Iteration {}: LsmStorage reopen failed with unexpected error: {:?}",
                    iter + 1,
                    e
                );
            }
        };

        // 7. Verification:
        // a. Every confirmed COMMIT entry MUST be readable with exact value
        for (key, expected_val) in &confirmed_commits {
            let actual = storage
                .get(key.as_bytes())
                .await
                .unwrap_or_else(|e| panic!("Iteration {}: get({}) failed: {:?}", iter + 1, key, e));

            assert_eq!(
                actual,
                Some(expected_val.clone()),
                "Iteration {}: Confirmed commit for key {} missing or value mismatch after crash recovery",
                iter + 1,
                key
            );
        }

        // b. No entry beyond max_prepared_idx MUST be visible (no phantom commits)
        let start_phantom_idx = max_prepared_idx.map_or(0, |m| m + 1);
        for phantom_idx in start_phantom_idx..(start_phantom_idx + 20) {
            let phantom_key = format!("chaos_key_{:06}", phantom_idx);
            let actual = storage
                .get(phantom_key.as_bytes())
                .await
                .unwrap_or_else(|e| panic!("Iteration {}: get({}) failed: {:?}", iter + 1, phantom_key, e));

            assert_eq!(
                actual,
                None,
                "Iteration {}: Phantom commit detected for key {} which was never written before process kill",
                iter + 1,
                phantom_key
            );
        }
    }
}
