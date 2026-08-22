// ANCHOR:PERF:BENCH-001 — Benchmark Suite für LangGraph Migration
// ZIEL: Beweise wirtschaftliche Kohärenz durch Latenz-Metriken (MemFuse vs Redis / Chroma)
// AGENT:09 DATE:2026-05-15 STATUS:DONE

use criterion::{criterion_group, criterion_main, Criterion};
use memfuse_checkpoint::PersistentCheckpointStore;
use memfuse_core::TxId;
use memfuse_db::MemFuse;
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
                .hybrid_search("quick fox", &vec![0.1; 1536], 5, None)
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
    let manager = PersistentCheckpointStore::new(storage, "test");

    c.bench_function("checkpoint_latency", |b| {
        b.to_async(&rt).iter(|| async {
            let _ = manager
                .create_checkpoint("test-cp", "default", 0, TxId::new(0), serde_json::json!({}))
                .await
                .unwrap();
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

fn bench_snapshot_overhead(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let tmp = TempDir::new().unwrap();
    let db = rt.block_on(MemFuse::open(tmp.path())).unwrap();

    rt.block_on(async {
        for i in 0..100 {
            db.insert(
                &format!("doc-{}", i),
                &vec![0.1; 1536],
                Some(serde_json::json!({"text": "The quick brown fox jumps over the lazy dog"})),
            )
            .await
            .unwrap();
        }
    });

    c.bench_function("snapshot_search_overhead", |b| {
        b.to_async(&rt).iter(|| async {
            let _ = db
                .search_with_filter(&vec![0.1; 1536], 5, None)
                .await
                .unwrap();
        })
    });
}

fn bench_staged_stats_commit(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let tmp = TempDir::new().unwrap();
    let db = rt.block_on(MemFuse::open(tmp.path())).unwrap();

    c.bench_function("staged_stats_commit_overhead", |b| {
        b.to_async(&rt).iter(|| async {
            db.insert(
                "bench-doc",
                &vec![0.5; 1536],
                Some(serde_json::json!({"text": "Test benchmark stats overhead"})),
            )
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
    bench_snapshot_overhead,
    bench_staged_stats_commit
);
criterion_main!(benches);
