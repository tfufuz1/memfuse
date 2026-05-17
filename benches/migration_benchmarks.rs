// ANCHOR:PERF:BENCH-001 — Benchmark Suite für LangGraph Migration
// ZIEL: Beweise wirtschaftliche Kohärenz durch Latenz-Metriken (MemFuse vs Redis / Chroma)
// AGENT:09 DATE:2026-05-15 STATUS:DONE

use criterion::{criterion_group, criterion_main, Criterion};
use memfuse_db::{memfuse_checkpoint::CheckpointManager, MemFuse};
use tempfile::TempDir;
use tokio::runtime::Runtime;

fn bench_hybrid_search(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let tmp = TempDir::new().unwrap();
    let db = rt.block_on(MemFuse::open(tmp.path())).unwrap();

    // Prepare data
    rt.block_on(async {
        db.insert(
            "doc-1",
            &vec![0.1; 1536],
            Some(serde_json::json!({"text": "The quick brown fox jumps over the lazy dog"})),
        )
        .await
        .unwrap();
    });

    c.bench_function("hybrid_search_latency", |b| {
        b.to_async(&rt).iter(|| async {
            let _ = db
                .hybrid_search("quick fox", &vec![0.1; 1536], 5)
                .await
                .unwrap();
        })
    });
}

fn bench_agent_state_checkpoint(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let tmp = TempDir::new().unwrap();
    let db = rt.block_on(MemFuse::open(tmp.path())).unwrap();
    let storage = db.inner_storage();
    let manager = CheckpointManager::new(storage);

    c.bench_function("checkpoint_latency", |b| {
        b.to_async(&rt).iter(|| async {
            let _ = manager.create_checkpoint("test-cp").await.unwrap();
        })
    });
}

fn bench_rerun_cost(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let tmp = TempDir::new().unwrap();
    let db = rt.block_on(MemFuse::open(tmp.path())).unwrap();

    // Prepare data
    rt.block_on(async {
        db.insert(
            "doc-1",
            &vec![0.1; 1536],
            Some(serde_json::json!({"text": "The quick brown fox jumps over the lazy dog"})),
        )
        .await
        .unwrap();
    });

    c.bench_function("rerun_cost_get_latency", |b| {
        b.to_async(&rt).iter(|| async {
            let _ = db.get("doc-1").await.unwrap();
        })
    });
}

fn bench_wal_replay(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let tmp = TempDir::new().unwrap();
    let wal_path = tmp.path().join("bench.wal");

    rt.block_on(async {
        let wal = memfuse_store::wal::Wal::open(&wal_path).await.unwrap();
        let integrity_key = b"memfuse-integrity-key-v1\0\0\0\0\0\0\0\0";
        for i in 0..1000 {
            let op = memfuse_store::wal::WalOp::Put {
                tx_id: memfuse_core::TxId::new(i),
                key: format!("key{:05}", i).into_bytes(),
                value: vec![0u8; 100],
            };
            let entry = memfuse_store::wal::WalEntry::try_new(op, i, integrity_key).unwrap();
            wal.append(&entry).await.unwrap();
        }
    });

    c.bench_function("wal_replay_1000_entries", |b| {
        b.to_async(&rt).iter(|| async {
            let wal = memfuse_store::wal::Wal::open(&wal_path).await.unwrap();
            let _ = wal.replay().await.unwrap();
        })
    });
}

fn bench_collection_scan(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let tmp = TempDir::new().unwrap();
    let db = rt.block_on(MemFuse::open(tmp.path())).unwrap();
    let col = rt.block_on(db.collection("default")).unwrap();

    rt.block_on(async {
        for i in 0..1000 {
            col.insert(
                &format!("doc-{}", i),
                &vec![0.1; 1536],
                Some(serde_json::json!({"i": i})),
            )
            .await
            .unwrap();
        }
    });

    c.bench_function("collection_scan_1000_docs", |b| {
        b.to_async(&rt).iter(|| async {
            let _ = col
                .scan(std::ops::Bound::Unbounded, std::ops::Bound::Unbounded)
                .await
                .unwrap();
        })
    });
}

criterion_group!(
    benches,
    bench_hybrid_search,
    bench_agent_state_checkpoint,
    bench_rerun_cost,
    bench_wal_replay,
    bench_collection_scan
);
criterion_main!(benches);
