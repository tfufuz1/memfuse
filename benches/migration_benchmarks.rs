// ANCHOR[PERF:BENCH-001] STATUS:PARTIAL (TS:2026-08-29T00:00:00Z) — Benchmark Suite für LangGraph Migration
// ZIEL: Latenz-Baseline für MemFuse-interne Operationen — KEIN Cross-System-Vergleich
// AGENT:09 DATE:2026-05-15 STATUS:PARTIAL

use criterion::{criterion_group, criterion_main, Criterion};
use memfuse_checkpoint::PersistentCheckpointStore;
use memfuse_core::TxId;
use memfuse_db::{MemFuse, MemFuseConfig};
use tempfile::TempDir;
use tokio::runtime::Runtime;

fn bench_hybrid_search(c: &mut Criterion) {
    let rt = Runtime::new().unwrap(); // unwrap allowed
    let tmp = TempDir::new().unwrap(); // unwrap allowed
    let config = MemFuseConfig {
        dimension: 768,
        ..Default::default()
    };
    let db = rt
        .block_on(MemFuse::open_with_config(tmp.path(), config))
        .unwrap(); // unwrap allowed

    // Prepare data
    rt.block_on(async {
        db.insert(
            "doc-1",
            &vec![0.1; 768],
            Some(serde_json::json!({"text": "The quick brown fox jumps over the lazy dog"})),
        )
        .await
        .unwrap(); // unwrap allowed
    });

    c.bench_function("hybrid_search_latency", |b| {
        b.to_async(&rt).iter(|| async {
            let _ = db
                .hybrid_search("quick fox", &vec![0.1; 768], 5, None)
                .await
                .unwrap(); // unwrap allowed
        })
    });
}

fn bench_agent_state_checkpoint(c: &mut Criterion) {
    let rt = Runtime::new().unwrap(); // unwrap allowed
    let tmp = TempDir::new().unwrap(); // unwrap allowed
    let db = rt.block_on(MemFuse::open(tmp.path())).unwrap(); // unwrap allowed
    let storage = db.inner_storage();
    let manager = PersistentCheckpointStore::new(storage, "test").unwrap();

    c.bench_function("checkpoint_latency", |b| {
        b.to_async(&rt).iter(|| async {
            let _ = manager
                .create_checkpoint("test-cp", "default", 0, TxId::new(0), serde_json::json!({}))
                .await
                .unwrap(); // unwrap allowed
        })
    });
}

fn bench_rerun_cost(c: &mut Criterion) {
    let rt = Runtime::new().unwrap(); // unwrap allowed
    let tmp = TempDir::new().unwrap(); // unwrap allowed
    let config = MemFuseConfig {
        dimension: 768,
        ..Default::default()
    };
    let db = rt
        .block_on(MemFuse::open_with_config(tmp.path(), config))
        .unwrap(); // unwrap allowed

    // Prepare data
    rt.block_on(async {
        db.insert(
            "doc-1",
            &vec![0.1; 768],
            Some(serde_json::json!({"text": "The quick brown fox jumps over the lazy dog"})),
        )
        .await
        .unwrap(); // unwrap allowed
    });

    c.bench_function("rerun_cost_get_latency", |b| {
        b.to_async(&rt).iter(|| async {
            let _ = db.get("doc-1").await.unwrap(); // unwrap allowed
        })
    });
}

fn bench_snapshot_overhead(c: &mut Criterion) {
    let rt = Runtime::new().unwrap(); // unwrap allowed
    let tmp = TempDir::new().unwrap(); // unwrap allowed
    let config = MemFuseConfig {
        dimension: 768,
        ..Default::default()
    };
    let db = rt
        .block_on(MemFuse::open_with_config(tmp.path(), config))
        .unwrap(); // unwrap allowed

    rt.block_on(async {
        for i in 0..100 {
            db.insert(
                &format!("doc-{}", i),
                &vec![0.1; 768],
                Some(serde_json::json!({"text": "The quick brown fox jumps over the lazy dog"})),
            )
            .await
            .unwrap(); // unwrap allowed
        }
    });

    c.bench_function("snapshot_search_overhead", |b| {
        b.to_async(&rt).iter(|| async {
            let _ = db
                .search_with_filter_expr(&vec![0.1; 768], 5, None)
                .await
                .unwrap(); // unwrap allowed
        })
    });
}

fn bench_staged_stats_commit(c: &mut Criterion) {
    let rt = Runtime::new().unwrap(); // unwrap allowed
    let tmp = TempDir::new().unwrap(); // unwrap allowed
    let config = MemFuseConfig {
        dimension: 768,
        ..Default::default()
    };
    let db = rt
        .block_on(MemFuse::open_with_config(tmp.path(), config))
        .unwrap(); // unwrap allowed

    c.bench_function("staged_stats_commit_overhead", |b| {
        b.to_async(&rt).iter(|| async {
            db.insert(
                "bench-doc",
                &vec![0.5; 768],
                Some(serde_json::json!({"text": "Test benchmark stats overhead"})),
            )
            .await
            .unwrap(); // unwrap allowed
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
