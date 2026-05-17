use criterion::{black_box, criterion_group, criterion_main, Criterion};
use memfuse_core::{DocId, TxId, StorageEngine};
use memfuse_store::{LsmConfig, LsmStorage};
use memfuse_text::inverted::InvertedIndex;
use std::sync::Arc;
use tokio::runtime::Runtime;

fn bench_text_search(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        ..Default::default()
    };

    let storage = rt.block_on(async {
        Arc::new(LsmStorage::new(config).await.unwrap())
    });

    let index = InvertedIndex::new(storage.clone(), "default");

    // Seed with some data
    rt.block_on(async {
        for i in 0..100 {
            let tx = TxId::new(i as u64);
            let doc_id = DocId::new(i as u64);
            let text = format!("Rust is a fast and safe programming language. Episode {}. Benchmarking performance with Criterion.", i);
            index.upsert_document(tx, doc_id, &text).await.unwrap();
            storage.commit(tx).await.unwrap();
        }
    });

    let mut group = c.benchmark_group("TextSearch");

    group.bench_function("search_bm25", |b| {
        b.iter(|| {
            rt.block_on(async {
                black_box(index.search_bm25("rust programming performance", 10).await.unwrap())
            })
        })
    });

    group.finish();
}

criterion_group!(benches, bench_text_search);
criterion_main!(benches);
