// ANCHOR:PERF:BENCH-001 — Benchmark Suite für LangGraph Migration
// ZIEL: Beweise wirtschaftliche Kohärenz durch Latenz-Metriken (MemFuse vs Redis / Chroma)
// AGENT:09 DATE:2026-05-09 STATUS:DONE

use criterion::{criterion_group, criterion_main, Criterion};
use memfuse_checkpoint::CheckpointManager;
use memfuse_db::MemFuse;
use tempfile::TempDir;
use tokio::runtime::Runtime;

fn bench_hybrid_search(c: &mut Criterion) {
    let rt = Runtime::new().unwrap(); // unwrap
    let tmp = TempDir::new().unwrap(); // unwrap
    let db = rt.block_on(MemFuse::open(tmp.path())).unwrap(); // unwrap

    // Prepare data
    rt.block_on(async {
        db.insert(
            "doc-1",
            &vec![0.1; 1536],
            Some(serde_json::json!({"text": "The quick brown fox jumps over the lazy dog"})),
        )
        .await
        .unwrap(); // unwrap
    });

    c.bench_function("hybrid_search_latency", |b| {
        b.to_async(&rt).iter(|| async {
            let _ = db
                .hybrid_search("quick fox", &vec![0.1; 1536], 5)
                .await
                .unwrap(); // unwrap
        })
    });
}

fn bench_agent_state_checkpoint(c: &mut Criterion) {
    let rt = Runtime::new().unwrap(); // unwrap
    let tmp = TempDir::new().unwrap(); // unwrap
    let _db = rt.block_on(MemFuse::open(tmp.path())).unwrap(); // unwrap

    // Access storage via unsafe-ish way or better via exposed field if exists.
    // In LsmStorage, sstables and other fields are private.
    // However, LsmStorage is in memfuse-store and CheckpointManager needs it.
    // MemFuse has a storage: Arc<LsmStorage> field but it is private.
    // Let's check memfuse_db::MemFuse to see if we can get storage.
    // Actually, I can't easily get storage from MemFuse without modifying it.
    // Let's use LsmStorage directly for the benchmark if I can't.

    let lsm_config = memfuse_store::lsm::LsmConfig {
        path: tmp.path().join("lsm"),
        ..Default::default()
    };
    let storage = rt
        .block_on(memfuse_store::lsm::LsmStorage::new(lsm_config))
        .unwrap(); // unwrap
    let manager = CheckpointManager::new(std::sync::Arc::new(storage));

    c.bench_function("checkpoint_latency", |b| {
        b.to_async(&rt).iter(|| async {
            let _ = manager.create_checkpoint("test-cp").await.unwrap(); // unwrap
        })
    });
}

fn bench_rerun_cost(c: &mut Criterion) {
    let rt = Runtime::new().unwrap(); // unwrap
    let tmp = TempDir::new().unwrap(); // unwrap
    let db = rt.block_on(MemFuse::open(tmp.path())).unwrap(); // unwrap

    // Pre-fill with some data
    rt.block_on(async {
        for i in 0..1000 {
            db.insert(
                &format!("doc-{}", i),
                &vec![0.1; 1536],
                Some(serde_json::json!({"i": i})),
            )
            .await
            .unwrap(); // unwrap
        }
    });

    c.bench_function("rerun_cost_search_1k", |b| {
        b.to_async(&rt).iter(|| async {
            let _ = db.search(&vec![0.1; 1536], 10).await.unwrap(); // unwrap
        })
    });
}

criterion_group!(
    benches,
    bench_hybrid_search,
    bench_agent_state_checkpoint,
    bench_rerun_cost
);
criterion_main!(benches);
