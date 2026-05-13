use criterion::{black_box, criterion_group, criterion_main, Criterion};
use memfuse_store::wal::{Wal, WalEntry, WalOp};
use memfuse_core::TxId;
use tempfile::TempDir;
use tokio::runtime::Runtime;

fn bench_wal_append(c: &mut Criterion) {
    let rt = Runtime::new().unwrap(); // unwrap
    let tmp = TempDir::new().unwrap(); // unwrap
    let path = tmp.path().join("bench.wal");

    let wal = rt.block_on(async { Wal::open(&path).await.unwrap() }); // unwrap

    let entry = WalEntry::new(
        WalOp::Put {
            tx_id: TxId::new(1),
            key: b"key".to_vec(),
            value: vec![0u8; 1024],
        },
        1,
    );

    c.bench_function("wal_append_1kb", |b| {
        b.to_async(&rt).iter(|| async {
            wal.append(black_box(&entry)).await.unwrap(); // unwrap
        })
    });
}

criterion_group!(benches, bench_wal_append);
criterion_main!(benches);
