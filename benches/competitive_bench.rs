// ANCHOR[PERF:BENCH-003] STATUS:DONE (TS:2026-09-03T00:00:00Z) — MemFuse Competitive Benchmark Suite
// ZIEL: Criterion-basierte Messung von Write-Durchsatz, Hybrid-Search-Latenz und Context-Compaction-Durchsatz

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use memfuse_core::{ContextChunk, DocId, TokenBudget};
use memfuse_db::context_compaction::{CompactionStrategy, ContextCompactor};
use memfuse_db::MemFuse;
use tempfile::TempDir;
use tokio::runtime::Runtime;

const EMBEDDING_DIM: usize = 768;

fn generate_embedding(doc_idx: usize) -> Vec<f32> {
    let mut vec = vec![0.0f32; EMBEDDING_DIM];
    let cluster = doc_idx % 20;
    let base_val = (cluster as f32) * 0.05;
    for (i, elem) in vec.iter_mut().enumerate() {
        let noise = ((doc_idx * 31 + i * 17) % 1000) as f32 / 10000.0;
        *elem = base_val + noise;
    }
    vec
}

fn generate_content(doc_idx: usize) -> String {
    format!(
        "Document chunk {:07}: Detailed technical report regarding quantum consensus and hybrid vector indexing. Context index {}.",
        doc_idx, doc_idx
    )
}

fn bench_write_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let write_sizes = [100, 1_000, 10_000];

    let mut group = c.benchmark_group("competitive_write_throughput");
    group.sample_size(10);

    for &n in &write_sizes {
        group.throughput(Throughput::Elements(n as u64));

        let docs: Vec<(String, Vec<f32>, Option<serde_json::Value>)> = (0..n)
            .map(|i| {
                (
                    format!("doc-{:07}", i),
                    generate_embedding(i),
                    Some(serde_json::json!({
                        "text": generate_content(i),
                        "chunk_index": i,
                    })),
                )
            })
            .collect();

        group.bench_with_input(BenchmarkId::new("insert_batch", n), &n, |b, _| {
            b.to_async(&rt).iter(|| async {
                let tmp = TempDir::new().unwrap();
                let db = MemFuse::open(tmp.path()).await.unwrap();

                // Insert in batches of 100 to stay safely within max_ops_per_tx capacity
                let batch_size = 100;
                for chunk in docs.chunks(batch_size) {
                    db.insert_many(chunk).await.unwrap();
                }
            });
        });
    }

    group.finish();
}

fn bench_hybrid_search_latency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let num_docs = 1_000;

    let tmp = TempDir::new().unwrap();
    let db = rt.block_on(MemFuse::open(tmp.path())).unwrap();

    let batch_size = 100;
    let mut current_batch = Vec::with_capacity(batch_size);
    for i in 0..num_docs {
        current_batch.push((
            format!("doc-{:07}", i),
            generate_embedding(i),
            Some(serde_json::json!({
                "text": generate_content(i),
                "chunk_index": i,
            })),
        ));
        if current_batch.len() == batch_size || i + 1 == num_docs {
            rt.block_on(db.insert_many(&current_batch)).unwrap();
            current_batch.clear();
        }
    }

    let query_vec = generate_embedding(42);
    let query_text = "quantum consensus hybrid vector";

    let mut group = c.benchmark_group("competitive_hybrid_search_latency");
    group.sample_size(50);

    group.bench_function("hybrid_search_p50_p95_p99", |b| {
        b.to_async(&rt).iter(|| async {
            let res = db
                .hybrid_search(query_text, &query_vec, 10, None)
                .await
                .unwrap();
            assert!(!res.is_empty());
        });
    });

    group.finish();
}

fn bench_compaction_throughput(c: &mut Criterion) {
    let segment_counts = [10, 50, 200];

    let mut group = c.benchmark_group("competitive_compaction_throughput");

    for &m in &segment_counts {
        group.throughput(Throughput::Elements(m as u64));

        let chunks: Vec<ContextChunk> = (0..m)
            .map(|i| ContextChunk {
                doc_id: DocId::new(i as u64),
                content: format!(
                    "Memory segment {:03}: Tool execution output and context history log payload.",
                    i
                ),
                relevance: 1.0 / ((i + 1) as f32),
                token_count: 50,
                metadata: if i % 3 == 0 {
                    Some(serde_json::json!({"tool_output": true}))
                } else {
                    None
                },
                contextual_prefix: None,
                links: Vec::new(),
            })
            .collect();

        let target_budget = (m * 50) / 2; // Compact approximately half the chunks
        let budget = TokenBudget::new(target_budget, 0);
        let compactor = ContextCompactor::new(budget, CompactionStrategy::StatusToken);

        group.bench_with_input(BenchmarkId::new("compact_segments", m), &m, |b, _| {
            b.iter(|| {
                let compacted = compactor.compact(chunks.clone());
                assert!(compacted.tokens_used <= target_budget);
            });
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_write_throughput,
    bench_hybrid_search_latency,
    bench_compaction_throughput
);
criterion_main!(benches);
