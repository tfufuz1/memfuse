use criterion::{black_box, criterion_group, criterion_main, Criterion};
use memfuse_core::{DocId, StorageEngine, TxId};
use memfuse_store::{LsmConfig, LsmStorage};
use memfuse_text::inverted::InvertedIndex;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::runtime::Runtime;

fn bench_text_search(c: &mut Criterion) {
    let rt = Runtime::new().unwrap(); // unwrap
    let tmp = TempDir::new().unwrap(); // unwrap
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        ..Default::default()
    };
    let storage = rt.block_on(async { Arc::new(LsmStorage::new(config).await.unwrap()) }); // unwrap
    let index = InvertedIndex::new(storage.clone(), "bench");

    // Populate index
    rt.block_on(async {
        let tx = TxId::new(1);
        for i in 0..100 {
            let doc_id = DocId::new(i as u64);
            let text = format!("This is a document number {} containing some rust programming keywords and other text to make it longer and more realistic for a search benchmark.", i);
            index.upsert_document(tx, doc_id, &text).await.unwrap(); // unwrap
        }
        storage.commit(tx).await.unwrap(); // unwrap
    });

    let mut group = c.benchmark_group("InvertedIndex");

    group.bench_function("search_bm25_one_term", |b| {
        b.to_async(&rt).iter(|| async {
            let res = index.search_bm25(black_box("rust"), 10).await.unwrap(); // unwrap
            black_box(res);
        })
    });

    group.bench_function("search_bm25_multi_term", |b| {
        b.to_async(&rt).iter(|| async {
            let res = index
                .search_bm25(black_box("rust programming search"), 10)
                .await
                .unwrap(); // unwrap
            black_box(res);
        })
    });

    group.bench_function("upsert_document", |b| {
        b.to_async(&rt).iter(|| async {
            let tx = TxId::new(2);
            let doc_id = DocId::new(1000); // Fixed ID for benchmark
            let text = "New document for upsert benchmark testing performance improvements.";
            index
                .upsert_document(tx, doc_id, black_box(text))
                .await
                .unwrap(); // unwrap
            // We don't commit to avoid disk growth in the benchmark loop
        })
    });

    group.finish();
}

criterion_group!(benches, bench_text_search);
criterion_main!(benches);
