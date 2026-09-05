use memfuse_core::{Result as MemResult, StorageEngine, TxId};
use memfuse_store::lsm::{LsmConfig, LsmStorage};
use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use tokio::time::{sleep, Duration};

const SLEEP_BETWEEN_WRITES_MS: u64 = 10;

#[tokio::main]
async fn main() -> MemResult<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 4 {
        eprintln!("Usage: chaos_writer <storage_dir> <n_writes> <ground_truth_log_path>");
        std::process::exit(1);
    }

    let storage_dir = PathBuf::from(&args[1]);
    let n_writes: usize = match args[2].parse() {
        Ok(val) => val,
        Err(e) => return Err(memfuse_core::MemFuseError::invalid_input(format!("Invalid n_writes: {e}"))),
    };
    let ground_truth_log_path = PathBuf::from(&args[3]);

    let config = LsmConfig {
        path: storage_dir,
        ..Default::default()
    };
    let storage = LsmStorage::new(config).await?;

    let mut log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&ground_truth_log_path)
        .map_err(|e| memfuse_core::MemFuseError::Storage(format!("Failed to open log file: {e}")))?;

    for i in 0..n_writes {
        let tx_id = TxId::new((i + 1) as u64);
        let key = format!("chaos_key_{:06}", i);
        let val = format!("val_{:06}_{}", i, rand::random::<u64>());

        // Log PREPARE before write
        writeln!(log_file, "PREPARE:{}:{}", key, val)
            .map_err(|e| memfuse_core::MemFuseError::Storage(format!("Log write failed: {e}")))?;
        log_file
            .flush()
            .map_err(|e| memfuse_core::MemFuseError::Storage(format!("Log flush failed: {e}")))?;
        log_file
            .sync_all()
            .map_err(|e| memfuse_core::MemFuseError::Storage(format!("Log fsync failed: {e}")))?;

        // Write and commit to LsmStorage
        storage.put(tx_id, key.as_bytes(), val.as_bytes()).await?;
        storage.commit(tx_id).await?;

        // Log COMMIT after successful commit
        writeln!(log_file, "COMMIT:{}:{}", key, val)
            .map_err(|e| memfuse_core::MemFuseError::Storage(format!("Log write failed: {e}")))?;
        log_file
            .flush()
            .map_err(|e| memfuse_core::MemFuseError::Storage(format!("Log flush failed: {e}")))?;
        log_file
            .sync_all()
            .map_err(|e| memfuse_core::MemFuseError::Storage(format!("Log fsync failed: {e}")))?;

        sleep(Duration::from_millis(SLEEP_BETWEEN_WRITES_MS)).await;
    }

    Ok(())
}
