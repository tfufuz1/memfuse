use criterion::{black_box, criterion_group, criterion_main, Criterion};
use memfuse_core::{DocId, TxId};
use memfuse_store::{LsmConfig, LsmStorage};
use memfuse_text::inverted::InvertedIndex;
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

    let text = "Rust is a systems programming language that runs blazingly fast, prevents segfaults, and guarantees thread safety.";
    let mut doc_counter = 0;

    c.bench_function("upsert_document", |b| {
        b.to_async(&rt).iter(|| {
            doc_counter += 1;
            index.upsert_document(TxId::new(1), DocId::new(doc_counter), black_box(text))
        })
    });

    // Prepare for search bench
    rt.block_on(async {
        for i in 1..100 {
            index
                .upsert_document(TxId::new(1), DocId::new(i), text)
                .await
                .unwrap(); // unwrap
        }
    });

    c.bench_function("search_bm25", |b| {
        b.to_async(&rt)
            .iter(|| index.search_bm25(black_box("rust fast safety"), 10))
    });
}

criterion_group!(benches, bench_text_engine);
criterion_main!(benches);
