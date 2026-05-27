// ANCHOR:PERF:BENCH-001 — Benchmark Suite für LangGraph Migration
// ZIEL: Beweise wirtschaftliche Kohärenz durch Latenz-Metriken (MemFuse vs Redis / Chroma)
// AGENT:09 DATE:2026-05-20 STATUS:DONE
// OPT: DocId::from_key fix and HNSW search ep optimization

use criterion::{criterion_group, criterion_main, Criterion};
use memfuse_checkpoint::CheckpointManager;
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
            let _ = manager
                .create_checkpoint("test-cp", "default", 0, serde_json::json!({}))
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

criterion_group!(
    benches,
    bench_hybrid_search,
    bench_agent_state_checkpoint,
    bench_rerun_cost
);
criterion_main!(benches);
