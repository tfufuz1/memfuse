// FILE-CONTEXT: Helper process for chaos testing simulating mid-transaction process kills. (TS: 2026-09-05) (SESSION: chaos_power_cut)
//! Helper worker binary for `chaos_power_cut` test.
//!
//! Writes sequential key-value entries to `LsmStorage` while logging transaction lifecycle
//! states (`START` / `COMMITTED`) to an external ground-truth file for crash recovery verification.

use memfuse_core::{MemFuseError, StorageEngine, TxId};
use memfuse_store::lsm::{LsmConfig, LsmStorage};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("Usage: chaos_writer <storage_dir> <n_writes> <ground_truth_log_path>");
        std::process::exit(1);
    }

    let storage_dir = PathBuf::from(&args[1]);
    let n_writes: usize = args[2].parse().map_err(|e| {
        MemFuseError::InvalidInput(format!("Invalid n_writes argument '{}': {}", args[2], e))
    })?;
    let ground_truth_log_path = PathBuf::from(&args[3]);

    let config = LsmConfig {
        path: storage_dir,
        ..Default::default()
    };

    let storage = LsmStorage::new(config).await?;

    let mut log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&ground_truth_log_path)?;

    for i in 1..=n_writes {
        let key_str = format!("key-{:06}", i);
        let val_str = format!("val-{:06}-{}", i, rand::random::<u64>());

        // 1. Log START to external ground-truth file
        writeln!(log_file, "START {} {} {}", i, key_str, val_str)?;
        log_file.flush()?;
        log_file.sync_all()?;

        // 2. Perform LSM storage put and commit
        let tx = TxId::new(i as u64);
        storage
            .put(tx, key_str.as_bytes(), val_str.as_bytes())
            .await?;
        storage.commit(tx).await?;

        // 3. Log COMMITTED to external ground-truth file
        writeln!(log_file, "COMMITTED {} {} {}", i, key_str, val_str)?;
        log_file.flush()?;
        log_file.sync_all()?;

        // Short sleep window to allow process kill injection
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    Ok(())
}
