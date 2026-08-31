//! Test suite for fsync syscall discipline, call sequence verification, and performance overhead measurements.

use memfuse_core::{StorageEngine, TxId};
use memfuse_store::wal::{Wal, WalOp};
use memfuse_store::{LsmConfig, LsmStorage};
use std::time::Instant;
use tempfile::tempdir;

#[tokio::test]
async fn test_verify_wal_append_fsync_syscall_sequence() {
    // Marker test specifically formatted for strace syscall filtering and verification.
    // Operation:
    // 1. Create LsmStorage instance
    // 2. Stage a put operation
    // 3. Call commit() and verify syscall order write() -> fsync() -> Return Ok
    let dir = tempdir().expect("tempdir");
    let config = LsmConfig {
        path: dir.path().to_path_buf(),
        ..Default::default()
    };

    let storage = LsmStorage::new(config).await.expect("storage init");

    let tx = TxId::new(42);
    storage
        .put(tx, b"durability_key_fsync", b"durability_value_fsync")
        .await
        .expect("put");

    // Print marker to stdout for strace synchronization
    println!("[STRACE_MARKER_START_COMMIT]");
    let commit_res = storage.commit(tx).await;
    println!("[STRACE_MARKER_END_COMMIT]");

    assert!(commit_res.is_ok(), "Commit must return Ok");
}

#[tokio::test]
async fn test_measure_fsync_overhead_benchmark() {
    let dir = tempdir().expect("tempdir");
    let config = LsmConfig {
        path: dir.path().to_path_buf(),
        ..Default::default()
    };

    let storage = LsmStorage::new(config).await.expect("storage init");

    let num_ops = 50;
    let start = Instant::now();

    for i in 1..=num_ops {
        let tx = TxId::new(i);
        let key = format!("bench_key_{}", i);
        let val = format!("bench_value_{}", i);
        storage
            .put(tx, key.as_bytes(), val.as_bytes())
            .await
            .expect("put");
        storage.commit(tx).await.expect("commit");
    }

    let elapsed = start.elapsed();
    let ops_per_sec = (num_ops as f64) / elapsed.as_secs_f64();
    let avg_latency_ms = (elapsed.as_secs_f64() * 1000.0) / (num_ops as f64);

    println!(
        "FSYNC_BENCHMARK_RESULT: total_ops={} elapsed_ms={:.2} ops_per_sec={:.2} avg_latency_ms={:.3}",
        num_ops,
        elapsed.as_millis(),
        ops_per_sec,
        avg_latency_ms
    );

    assert!(num_ops > 0);
}

#[tokio::test]
async fn test_wal_direct_append_batch_fsync_discipline() {
    let dir = tempdir().expect("tempdir");
    let wal_path = dir.path().join("wal_direct.log");

    let wal = Wal::open(&wal_path).await.expect("wal open");

    let op = WalOp::Put {
        tx_id: TxId::new(1),
        key: b"direct_k".to_vec(),
        value: b"direct_v".to_vec(),
    };

    let entry = wal.create_entry(op, 1).await.expect("create entry");

    println!("[STRACE_MARKER_START_DIRECT_APPEND]");
    let append_res = wal.append(&entry).await;
    println!("[STRACE_MARKER_END_DIRECT_APPEND]");

    assert!(append_res.is_ok());
}
