// ANCHOR:PERF:BENCH-001 — Benchmark Suite für LangGraph Migration
// ZIEL: Beweise wirtschaftliche Kohärenz durch Latenz-Metriken (MemFuse vs Redis / Chroma)
// AGENT:09 DATE:2026-05-09 STATUS:DONE

use criterion::{criterion_group, criterion_main, Criterion};
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

    c.bench_function("checkpoint_latency", |b| {
        b.to_async(&rt).iter(|| async {
            // MemFuse checkpointing is memory-only pinning of the SnapshotRegistry
            // We use the collection's last_seq_no and pin it
            let col = db.collection("default").await.unwrap();
            let seq = col.last_seq_no();
            col.pin_checkpoint(seq).await.unwrap();
        })
    });
}

fn bench_rerun_cost(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().to_path_buf();

    // Initial open and one collection
    {
        let db = rt.block_on(MemFuse::open(&path)).unwrap();
        let _ = rt.block_on(db.collection("bench")).unwrap();
    }

    c.bench_function("rerun_cost", |b| {
        b.to_async(&rt).iter(|| async {
            // Measure cost of reopening the DB and listing collections
            let db = MemFuse::open(&path).await.unwrap();
            let _ = db.list_collections().await.unwrap();
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
