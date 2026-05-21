use criterion::{black_box, criterion_group, criterion_main, Criterion};
use memfuse_core::{DocId, TxId, StorageEngine};
use memfuse_store::{LsmConfig, LsmStorage};
use memfuse_text::inverted::InvertedIndex;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tempfile::TempDir;
use tokio::runtime::Runtime;
use rand::Rng;

fn bench_inverted_index(c: &mut Criterion) {
    let rt = Runtime::new().expect("Failed to create Tokio runtime");
    let tmp = TempDir::new().expect("Failed to create temp dir");
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        ..Default::default()
    };
    let storage = rt.block_on(async { Arc::new(LsmStorage::new(config).await.expect("Failed to open storage")) });
    let index = InvertedIndex::new(storage.clone(), "bench");

    let mut group = c.benchmark_group("InvertedIndex");

    let words = vec!["rust", "fast", "search", "engine", "database", "performance", "benchmarking", "vector", "index", "storage"];
    let id_gen = Arc::new(AtomicU64::new(0));

    group.bench_function("upsert_document", |b| {
        b.to_async(&rt).iter(|| {
            let id_gen = id_gen.clone();
            let index = index.clone();
            let words = words.clone();
            async move {
                let id = id_gen.fetch_add(1, Ordering::SeqCst);
                let doc_id = DocId::new(id);
                let tx = TxId::new(id);

                // Create some semi-random text
                let mut text = String::new();
                let mut rng = rand::thread_rng();
                for _ in 0..10 {
                    let idx = rng.gen_range(0..words.len());
                    text.push_str(words[idx]);
                    text.push(' ');
                }

                index.upsert_document(tx, doc_id, black_box(&text)).await.expect("Upsert failed");
            }
        })
    });

    // Populate for search bench
    rt.block_on(async {
        for i in 0..1000 {
            let doc_id = DocId::new(i + 1_000_000);
            let tx = TxId::new(i + 1_000_000);
            let text = format!("document {} with some common keywords like rust and fast and some unique ones like unique_{}", i, i);
            index.upsert_document(tx, doc_id, &text).await.expect("Pre-population failed");
            storage.commit(tx).await.expect("Commit failed");
        }
    });

    group.bench_function("search_bm25_common", |b| {
        b.to_async(&rt).iter(|| async {
            let results = index.search_bm25(black_box("rust fast"), 10).await.expect("Search failed");
            black_box(results);
        })
    });

    group.bench_function("search_bm25_rare", |b| {
        b.to_async(&rt).iter(|| async {
            let results = index.search_bm25(black_box("unique_500"), 10).await.expect("Search failed");
            black_box(results);
        })
    });

    group.finish();
}

criterion_group!(benches, bench_inverted_index);
criterion_main!(benches);
