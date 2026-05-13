use criterion::{black_box, criterion_group, criterion_main, Criterion};
use memfuse_core::TxId;
use memfuse_store::wal::{Wal, WalEntry, WalOp};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::runtime::Runtime;

fn bench_wal_append_parallel(c: &mut Criterion) {
    let rt = Runtime::new().unwrap(); // unwrap
    let tmp = TempDir::new().unwrap(); // unwrap
    let path = tmp.path().join("bench_parallel.wal");

    let wal = rt.block_on(async { Arc::new(Wal::open(&path).await.unwrap()) }); // unwrap

    let entry = WalEntry::new(
        WalOp::Put {
            tx_id: TxId::new(1),
            key: b"key".to_vec(),
            value: vec![0u8; 1024],
        },
        1,
    );

    c.bench_function("wal_append_1kb_parallel_10", |b| {
        b.to_async(&rt).iter(|| async {
            let mut handles = Vec::new();
            for _ in 0..10 {
                let wal = wal.clone();
                let entry = entry.clone();
                handles.push(tokio::spawn(async move {
                    wal.append(black_box(&entry)).await.unwrap(); // unwrap
                }));
            }
            for h in handles {
                h.await.unwrap(); // unwrap
            }
        })
    });
}

criterion_group!(benches, bench_wal_append_parallel);
criterion_main!(benches);
