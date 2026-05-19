use criterion::{black_box, criterion_group, criterion_main, Criterion};
use memfuse_core::{DocId, TxId};
use memfuse_store::{LsmConfig, LsmStorage};
use memfuse_text::InvertedIndex;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::runtime::Runtime;

fn bench_text_engine(c: &mut Criterion) {
    let rt = Runtime::new().unwrap(); // unwrap
    let tmp = TempDir::new().unwrap(); // unwrap
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        ..Default::default()
    };

    let storage = rt.block_on(async { Arc::new(LsmStorage::new(config).await.unwrap()) }); // unwrap
    let index = InvertedIndex::new(storage.clone(), "bench");

    let mut group = c.benchmark_group("text_engine");

    // Benchmark Ingestion
    group.bench_function("ingest_document", |b| {
        let mut i = 0u64;
        b.to_async(&rt).iter(|| {
            let doc_id = DocId::new(i);
            let text = format!("This is a benchmark document number {}. It contains some words to be indexed by the inverted index.", i);
            i += 1;
            let index = index.clone();
            async move {
                let tx = TxId::new(i);
                index.upsert_document(tx, doc_id, &text).await.unwrap(); // unwrap
            }
        });
    });

    // Setup for search benchmark
    rt.block_on(async {
        for i in 0..100 {
            let doc_id = DocId::new(i);
            let text = format!("document keyword{} common_word", i % 10);
            index
                .upsert_document(TxId::new(i + 1000), doc_id, &text)
                .await
                .unwrap(); // unwrap
        }
    });

    // Benchmark Search
    group.bench_function("search_bm25", |b| {
        b.to_async(&rt).iter(|| {
            let index = index.clone();
            async move {
                black_box(index.search_bm25("keyword5 common_word", 10).await.unwrap()); // unwrap
            }
        });
    });

    group.finish();
}

criterion_group!(benches, bench_text_engine);
criterion_main!(benches);
