use criterion::{criterion_group, criterion_main, Criterion, black_box};
use memfuse_store::wal::{Wal, WalEntry, WalOp};
use memfuse_core::TxId;
use tempfile::TempDir;
use tokio::runtime::Runtime;

fn bench_wal_append(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let tmp = TempDir::new().unwrap();
    let wal_path = tmp.path().join("bench.wal");
    let wal = rt.block_on(Wal::open(&wal_path)).unwrap();
    let data = vec![0u8; 1024]; // 1KB entry

    let mut seq_no = 1;

    c.bench_function("wal_append_1kb", |b| {
        b.to_async(&rt).iter(|| {
            let op = WalOp::Put {
                tx_id: TxId::new(seq_no),
                key: b"key".to_vec(),
                value: data.clone(),
            };
            let entry = WalEntry::new(op, seq_no);
            seq_no += 1;
            let wal_ref = &wal;
            async move {
                wal_ref.append(black_box(&entry)).await.unwrap();
            }
        })
    });
}

criterion_group!(benches, bench_wal_append);
criterion_main!(benches);
