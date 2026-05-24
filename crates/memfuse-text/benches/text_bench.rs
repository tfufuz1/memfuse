use async_trait::async_trait;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use memfuse_core::{DocId, Result, StorageEngine, TxId};
use memfuse_text::inverted::InvertedIndex;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

struct MockStorage {
    store: RwLock<HashMap<Vec<u8>, Vec<u8>>>,
}

impl MockStorage {
    fn new() -> Self {
        Self {
            store: RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl StorageEngine for MockStorage {
    async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        Ok(self.store.read().get(key).cloned())
    }
    async fn put(&self, _tx_id: TxId, key: &[u8], value: &[u8]) -> Result<()> {
        self.store.write().insert(key.to_vec(), value.to_vec());
        Ok(())
    }
    async fn delete(&self, _tx_id: TxId, key: &[u8]) -> Result<()> {
        self.store.write().remove(key);
        Ok(())
    }
    async fn commit(&self, _tx_id: TxId) -> Result<()> {
        Ok(())
    }
    async fn rollback(&self, _tx_id: TxId) -> Result<()> {
        Ok(())
    }
    async fn flush(&self) -> Result<()> {
        Ok(())
    }
    async fn stats(&self) -> Result<memfuse_core::StorageStats> {
        Ok(memfuse_core::StorageStats {
            num_segments: 0,
            total_size_bytes: 0,
            memtable_size_bytes: 0,
        })
    }
    async fn scan_prefix(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let store = self.store.read();
        Ok(store
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect())
    }
}

fn bench_text_engine(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap(); // unwrap
    let storage = Arc::new(MockStorage::new());
    let index = InvertedIndex::new(storage.clone(), "default");

    let text = "Rust is a fast programming language for systems. I like rust programming and rust ownership rules.";
    let doc_id = DocId::new(1);
    let tx = TxId::new(1);

    c.bench_function("upsert_document", |b| {
        b.to_async(&runtime)
            .iter(|| index.upsert_document(black_box(tx), black_box(doc_id), black_box(text)))
    });

    // Ingest some data for search bench
    runtime.block_on(async {
        for i in 1..=100 {
            index
                .upsert_document(TxId::new(i), DocId::new(i), text)
                .await
                .unwrap(); // unwrap
        }
    });

    c.bench_function("search_bm25", |b| {
        b.to_async(&runtime)
            .iter(|| index.search_bm25(black_box("rust programming"), black_box(10)))
    });
}

criterion_group!(benches, bench_text_engine);
criterion_main!(benches);
