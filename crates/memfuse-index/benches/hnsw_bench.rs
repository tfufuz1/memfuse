// FILE-CONTEXT: v3 format, HOTSPOTS: [benches/hnsw_bench.rs]
// Hot Path HNSW Insertion, Search, and Quantized Search Benchmarks for ADR-031 Regressions.

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use memfuse_core::traits::VectorIndex;
use memfuse_core::types::{DocId, TxId};
use memfuse_index::hnsw::{HnswConfig, HnswIndex};
use rand::Rng;

fn bench_hnsw_hot_paths(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Tokio runtime required for HNSW benchmarks");

    let dim = 128;
    let num_docs = 100;
    let mut rng = rand::thread_rng();

    // Generate test vectors
    let vectors: Vec<Vec<f32>> = (0..num_docs)
        .map(|_| (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect())
        .collect();

    // 1. Insertion Throughput Benchmark
    let mut group = c.benchmark_group("HNSW_Insertion");
    group.throughput(Throughput::Elements(num_docs as u64));

    group.bench_function("insert_100_docs", |b| {
        b.iter(|| {
            rt.block_on(async {
                let config = HnswConfig {
                    dimension: dim,
                    m: 16,
                    ef_construction: 64,
                    quantize: false,
                    rebuild_threshold: 1.0,
                    ..Default::default()
                };
                let index = HnswIndex::try_new(config).expect("Index creation failed");

                for (idx, vec) in vectors.iter().enumerate() {
                    let tx = TxId::new(1);
                    let doc_id = DocId::new((idx + 1) as u64);
                    index.insert(tx, doc_id, vec).await.expect("Insert failed");
                    index.commit(tx).await.expect("Commit failed");
                }
            });
        });
    });
    group.finish();

    // Setup populated index for search benchmarks
    let populated_index = rt.block_on(async {
        let config = HnswConfig {
            dimension: dim,
            m: 16,
            ef_construction: 64,
            quantize: false,
            rebuild_threshold: 1.0,
            ..Default::default()
        };
        let index = HnswIndex::try_new(config).expect("Index creation failed");

        for (idx, vec) in vectors.iter().enumerate() {
            let tx = TxId::new(1);
            let doc_id = DocId::new((idx + 1) as u64);
            index.insert(tx, doc_id, vec).await.expect("Insert failed");
            index.commit(tx).await.expect("Commit failed");
        }
        index
    });

    let query: Vec<f32> = (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect();

    // 2. Query Latency Benchmark
    let mut search_group = c.benchmark_group("HNSW_Search");
    search_group.bench_function("search_k10", |b| {
        b.iter(|| {
            rt.block_on(async {
                let res = populated_index
                    .search(black_box(&query), black_box(10))
                    .await;
                black_box(res).expect("Search failed");
            });
        });
    });

    search_group.bench_function("search_k50", |b| {
        b.iter(|| {
            rt.block_on(async {
                let res = populated_index
                    .search(black_box(&query), black_box(50))
                    .await;
                black_box(res).expect("Search failed");
            });
        });
    });

    search_group.finish();
}

criterion_group!(benches, bench_hnsw_hot_paths);
criterion_main!(benches);
