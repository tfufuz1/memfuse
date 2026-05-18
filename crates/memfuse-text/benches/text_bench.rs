use criterion::{black_box, criterion_group, criterion_main, Criterion};
use memfuse_text::tokenizer::{DefaultTokenizer, Tokenizer};
use memfuse_text::inverted::InvertedIndex;
use memfuse_core::{DocId, TxId};
use memfuse_store::{LsmConfig, LsmStorage};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::runtime::Runtime;

fn bench_tokenizer(c: &mut Criterion) {
    let tokenizer = DefaultTokenizer;
    let text = "The quick brown fox jumps over the lazy dog. Rust is a fast programming language for systems. Bundesverfassungsgericht.";

    c.bench_function("tokenizer_tokenize", |b| {
        b.iter(|| tokenizer.tokenize(black_box(text)))
    });
}

fn bench_inverted_index_upsert(c: &mut Criterion) {
    let rt = Runtime::new().unwrap(); // unwrap
    let tmp = TempDir::new().unwrap(); // unwrap
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        ..Default::default()
    };

    let storage = rt.block_on(async {
        Arc::new(LsmStorage::new(config).await.unwrap()) // unwrap
    });

    let index = InvertedIndex::new(storage, "bench");
    let text = "Rust is a fast programming language for systems.";
    let mut doc_id_counter = 0;

    c.bench_function("inverted_index_upsert", |b| {
        b.to_async(&rt).iter(|| {
            doc_id_counter += 1;
            let tx = TxId::new(doc_id_counter);
            let doc_id = DocId::new(doc_id_counter);
            index.upsert_document(tx, doc_id, black_box(text))
        })
    });
}

criterion_group!(benches, bench_tokenizer, bench_inverted_index_upsert);
criterion_main!(benches);
