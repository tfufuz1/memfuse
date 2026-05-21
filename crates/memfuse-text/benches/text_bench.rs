use criterion::{black_box, criterion_group, criterion_main, Criterion};
use memfuse_text::inverted::InvertedIndex;
use memfuse_core::{DocId, TxId};
use memfuse_store::{LsmConfig, LsmStorage};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::runtime::Runtime;

fn bench_text_engine(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let tmp = TempDir::new().unwrap();
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        ..Default::default()
    };

    let storage = rt.block_on(async {
        Arc::new(LsmStorage::new(config).await.unwrap())
    });
    let index = InvertedIndex::new(storage.clone(), "bench");

    let mut group = c.benchmark_group("TextEngine");

    // Ingestion benchmark
    group.bench_function("upsert_document", |b| {
        let mut id_counter = 0;
        b.to_async(&rt).iter(|| {
            id_counter += 1;
            let doc_id = DocId::new(id_counter);
            let tx = TxId::new(id_counter);
            index.upsert_document(tx, doc_id, black_box("The quick brown fox jumps over the lazy dog. Rust is fast and safe."))
        })
    });

    // Search benchmark
    rt.block_on(async {
        for i in 1..=100 {
            let doc_id = DocId::new(i);
            let tx = TxId::new(i);
            index.upsert_document(tx, doc_id, "The quick brown fox jumps over the lazy dog. Rust is fast and safe.").await.unwrap();
        }
    });

    group.bench_function("search_bm25", |b| {
        b.to_async(&rt).iter(|| {
            index.search_bm25(black_box("quick brown rust"), 10)
        })
    });

    group.finish();
}

criterion_group!(benches, bench_text_engine);
criterion_main!(benches);
