//! Chaos writer example binary for fault-injection testing.
//! Writes monotonically increasing keys and random payloads into `LsmStorage`,
//! logging ground truth synchronously (with fsync) prior to staging and committing transactions.
// FILE-CONTEXT
// STAND: 2026-09-02T00:00:00Z
// ZWECK: Fault-Injection & Chaos Testing Subprozess für PowerCut / SIGKILL Simulation
// INVARIANTEN: Ground Truth fsync vor put/commit; Keine unwrap/expect/unsafe
// SIEHE AUCH: rules/chaos_testing.md, AGENTS.md §5

#![forbid(unsafe_code)]

use memfuse_core::{MemFuseError, Result, StorageEngine, TxId};
use memfuse_store::lsm::{LsmConfig, LsmStorage};
use rand::Rng;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Inter-commit delay allowing external test orchestrator to terminate process during active window.
const INTER_COMMIT_DELAY: Duration = Duration::from_millis(10);

/// Length of random payload bytes generated for each written entry.
const PAYLOAD_SIZE_BYTES: usize = 64;

fn write_ground_truth(
    path: &Path,
    counter: u64,
    key: &str,
    val: &[u8],
) -> Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| MemFuseError::Storage(format!("Failed to open ground truth log file {:?}: {}", path, e)))?;

    let val_hex: String = val.iter().map(|b| format!("{:02x}", b)).collect();
    let line = format!("{}:{}:{}\n", counter, key, val_hex);

    file.write_all(line.as_bytes())
        .map_err(|e| MemFuseError::Storage(format!("Failed to write ground truth entry: {}", e)))?;

    file.sync_all()
        .map_err(|e| MemFuseError::Storage(format!("Failed to fsync ground truth log: {}", e)))?;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        return Err(MemFuseError::InvalidInput(
            "Usage: chaos_writer <storage_dir> <n_writes> <ground_truth_log_path>".into(),
        ));
    }

    let storage_dir = PathBuf::from(&args[1]);
    let n_writes: u64 = args[2].parse().map_err(|e| {
        MemFuseError::InvalidInput(format!("Invalid n_writes argument '{}': {}", args[2], e))
    })?;
    let ground_truth_log_path = PathBuf::from(&args[3]);

    let config = LsmConfig {
        path: storage_dir,
        ..Default::default()
    };

    let storage = LsmStorage::new(config).await?;
    let mut rng = rand::thread_rng();

    for i in 1..=n_writes {
        let key = format!("key_{:010}", i);
        let mut payload = vec![0u8; PAYLOAD_SIZE_BYTES];
        rng.fill(&mut payload[..]);

        // VOR jedem put/commit-Aufruf: Ground truth synchron mit fsync schreiben
        write_ground_truth(&ground_truth_log_path, i, &key, &payload)?;

        let tx_id = TxId::new(i);
        storage.put(tx_id, key.as_bytes(), &payload).await?;
        storage.commit(tx_id).await?;

        tokio::time::sleep(INTER_COMMIT_DELAY).await;
    }

    Ok(())
}
