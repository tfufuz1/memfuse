use criterion::{criterion_group, criterion_main, Criterion};
use memfuse_core::TxId;
use memfuse_store::wal::{Wal, WalEntry, WalOp};
use tempfile::TempDir;
use tokio::runtime::Runtime;

fn bench_wal_append(c: &mut Criterion) {
    let rt = Runtime::new().unwrap(); // unwrap
    let tmp = TempDir::new().unwrap(); // unwrap
    let wal = rt
        .block_on(Wal::open(tmp.path().join("bench.wal")))
        .unwrap(); // unwrap

    let entry = WalEntry::new(
        WalOp::Put {
            tx_id: TxId::new(1),
            key: vec![b'k'; 32],
            value: vec![b'v'; 1024],
        },
        1,
    );

    c.bench_function("wal_append_latency", |b| {
        b.to_async(&rt).iter(|| async {
            wal.append(&entry).await.unwrap(); // unwrap
        })
    });
}

criterion_group!(benches, bench_wal_append);
criterion_main!(benches);
