use criterion::{criterion_group, criterion_main, Criterion};
use memfuse_text::inverted::InvertedIndex;
use memfuse_core::{StorageEngine, Result, DocId, TxId};
use std::sync::Arc;
use tokio::runtime::Runtime;

struct MockStorage;
#[async_trait::async_trait]
impl StorageEngine for MockStorage {
    async fn get(&self, _key: &[u8]) -> Result<Option<Vec<u8>>> { Ok(None) }
    async fn put(&self, _tx: TxId, _key: &[u8], _value: &[u8]) -> Result<()> { Ok(()) }
    async fn delete(&self, _tx: TxId, _key: &[u8]) -> Result<()> { Ok(()) }
    async fn commit(&self, _tx: TxId) -> Result<()> { Ok(()) }
    async fn rollback(&self, _tx: TxId) -> Result<()> { Ok(()) }
    async fn flush(&self) -> Result<()> { Ok(()) }
    async fn stats(&self) -> Result<memfuse_core::StorageStats> {
        Ok(memfuse_core::StorageStats {
            num_segments: 0,
            total_size_bytes: 0,
            memtable_size_bytes: 0,
        })
    }
    async fn scan_prefix(&self, _prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> { Ok(vec![]) }
}

fn bench_upsert(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let index = InvertedIndex::new(Arc::new(MockStorage), "default");
    let text = "The quick brown fox jumps over the lazy dog. Programming in Rust is fun and efficient.";

    c.bench_function("upsert_document", |b| {
        b.to_async(&rt).iter(|| async {
            index.upsert_document(TxId::new(1), DocId::new(1), text).await.unwrap(); // unwrap
        })
    });
}

fn bench_search(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let index = InvertedIndex::new(Arc::new(MockStorage), "default");

    c.bench_function("search_bm25_empty", |b| {
        b.to_async(&rt).iter(|| async {
            index.search_bm25("test query", 10).await.unwrap(); // unwrap
        })
    });
}

criterion_group!(benches, bench_upsert, bench_search);
criterion_main!(benches);
