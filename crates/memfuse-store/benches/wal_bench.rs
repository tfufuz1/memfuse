use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use memfuse_core::TxId;
use memfuse_crypto::crypto::KeyManager;
use memfuse_store::wal::{Wal, WalOp};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::runtime::Runtime;

fn bench_wal_encryption(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let batch_sizes = [8, 32, 128];

    let mut group = c.benchmark_group("WAL_Encryption");

    for size in batch_sizes {
        group.bench_with_input(
            BenchmarkId::new("single_entry_encryption_loop", size),
            &size,
            |b, &s| {
                b.to_async(&rt).iter(|| async move {
                    let tmp = TempDir::new().unwrap();
                    let wal_path = tmp.path().join("single_entry.wal");
                    let km = Arc::new(
                        KeyManager::try_new("passphrase123", b"salt123456789012345678901234567890")
                            .unwrap(),
                    );
                    let wal = Wal::open_with_key_manager(&wal_path, Some(km))
                        .await
                        .unwrap();

                    let ops: Vec<_> = (0..s)
                        .map(|i| {
                            (
                                WalOp::Put {
                                    tx_id: TxId::new(i as u64),
                                    key: format!("key_{:05}", i).into_bytes(),
                                    value: format!("value_{:05}", i).into_bytes(),
                                },
                                i as u64,
                            )
                        })
                        .collect();

                    let entries = wal.prepare_batch(ops).await.unwrap();

                    // Single entry loop: call append() for each entry individually
                    for entry in black_box(&entries) {
                        wal.append(entry).await.unwrap();
                    }
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("batch_encryption", size),
            &size,
            |b, &s| {
                b.to_async(&rt).iter(|| async move {
                    let tmp = TempDir::new().unwrap();
                    let wal_path = tmp.path().join("batch_enc.wal");
                    let km = Arc::new(
                        KeyManager::try_new("passphrase123", b"salt123456789012345678901234567890")
                            .unwrap(),
                    );
                    let wal = Wal::open_with_key_manager(&wal_path, Some(km))
                        .await
                        .unwrap();

                    let ops: Vec<_> = (0..s)
                        .map(|i| {
                            (
                                WalOp::Put {
                                    tx_id: TxId::new(i as u64),
                                    key: format!("key_{:05}", i).into_bytes(),
                                    value: format!("value_{:05}", i).into_bytes(),
                                },
                                i as u64,
                            )
                        })
                        .collect();

                    let entries = wal.prepare_batch(ops).await.unwrap();

                    // Batch encryption: single append_batch() call
                    wal.append_batch(black_box(&entries)).await.unwrap();
                });
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_wal_encryption);
criterion_main!(benches);
