use criterion::{black_box, criterion_group, criterion_main, Criterion};
use memfuse_core::{DocId, Result, StorageEngine, StorageStats, TxId};
use memfuse_text::inverted::InvertedIndex;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::runtime::Runtime;

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

#[async_trait::async_trait]
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
    async fn stats(&self) -> Result<StorageStats> {
        Ok(StorageStats {
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
    let rt = Runtime::new().expect("bench unwrap"); // unwrap
    let storage = Arc::new(MockStorage::new());
    let index = InvertedIndex::new(storage.clone(), "default");

    // Ingestion benchmark
    c.bench_function("upsert_document", |b| {
        let mut id_counter = 0;
        let index = index.clone();
        b.to_async(&rt).iter(|| {
            id_counter += 1;
            let doc_id = DocId::new(id_counter);
            let text = "Rust is a fast programming language for systems.";
            let tx = TxId::new(id_counter);
            let index = index.clone();
            async move {
                index
                    .upsert_document(tx, doc_id, black_box(text))
                    .await
                    .expect("bench unwrap"); // unwrap
            }
        });
    });

    // Search benchmark (pre-populated)
    rt.block_on(async {
        for i in 1..=100 {
            index
                .upsert_document(
                    TxId::new(i),
                    DocId::new(i),
                    "The quick brown fox jumps over the lazy dog",
                )
                .await
                .expect("bench unwrap"); // unwrap
        }
    });

    c.bench_function("search_bm25", |b| {
        let index = index.clone();
        b.to_async(&rt).iter(|| {
            let index = index.clone();
            async move {
                let _ = index
                    .search_bm25(black_box("quick fox"), 10)
                    .await
                    .expect("bench unwrap"); // unwrap
            }
        });
    });
}

criterion_group!(benches, bench_text_engine);
criterion_main!(benches);
