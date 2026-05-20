use criterion::{black_box, criterion_group, criterion_main, Criterion};
use memfuse_core::{DocId, StorageEngine, TxId};
use memfuse_store::{LsmConfig, LsmStorage};
use memfuse_text::inverted::InvertedIndex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::runtime::Runtime;

fn bench_inverted_index(c: &mut Criterion) {
    let rt = Runtime::new().expect("bench: runtime");
    let tmp = TempDir::new().expect("bench: tempdir");
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        ..Default::default()
    };

    let storage =
        rt.block_on(async { Arc::new(LsmStorage::new(config).await.expect("bench: lsm")) });

    let index = InvertedIndex::new(storage.clone(), "default");

    // Prepare some data
    rt.block_on(async {
        let tx = TxId::new(1);
        for i in 0..100 {
            let doc_id = DocId::new(i as u64);
            let text = format!(
                "This is document number {}. It contains some common words like rust and search.",
                i
            );
            index
                .upsert_document(tx, doc_id, &text)
                .await
                .expect("bench: upsert");
        }
        storage.commit(tx).await.expect("bench: commit");
    });

    let mut group = c.benchmark_group("InvertedIndex");

    group.bench_function("search_bm25", |b| {
        b.to_async(&rt).iter(|| async {
            black_box(index.search_bm25("rust search", 10).await).expect("bench: search");
        });
    });

    let counter = AtomicU64::new(1000);
    group.bench_function("upsert_document", |b| {
        b.to_async(&rt).iter(|| async {
            let i = counter.fetch_add(1, Ordering::SeqCst);
            let doc_id = DocId::new(i);
            let tx = TxId::new(i);
            black_box(
                index
                    .upsert_document(
                        tx,
                        doc_id,
                        "New document with some text for benchmarking purposes.",
                    )
                    .await,
            )
            .expect("bench: upsert");
        });
    });

    group.finish();
}

criterion_group!(benches, bench_inverted_index);
criterion_main!(benches);
