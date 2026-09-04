use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use memfuse_checkpoint::PersistentCheckpointStore;
use memfuse_core::{Result, StorageEngine, StorageStats, TxId};
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio::task::JoinSet;

struct BenchStorage {
    data: Mutex<HashMap<Vec<u8>, Vec<u8>>>,
    pinned: Mutex<HashSet<u64>>,
}

impl BenchStorage {
    fn new() -> Self {
        Self {
            data: Mutex::new(HashMap::new()),
            pinned: Mutex::new(HashSet::new()),
        }
    }
}

#[async_trait::async_trait]
impl StorageEngine for BenchStorage {
    async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        Ok(self.data.lock().get(key).cloned())
    }
    async fn put(&self, _tx_id: TxId, key: &[u8], value: &[u8]) -> Result<()> {
        self.data.lock().insert(key.to_vec(), value.to_vec());
        Ok(())
    }
    async fn delete(&self, _tx_id: TxId, key: &[u8]) -> Result<()> {
        self.data.lock().remove(key);
        Ok(())
    }
    async fn commit(&self, _tx_id: TxId) -> Result<()> {
        Ok(())
    }
    async fn rollback(&self, _tx_id: TxId) -> Result<()> {
        Ok(())
    }
    async fn rollback_to_tx(&self, _tx_id: TxId) -> Result<()> {
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
    async fn pin_checkpoint(&self, seq_no: u64) -> Result<()> {
        self.pinned.lock().insert(seq_no);
        Ok(())
    }
    async fn unpin_checkpoint(&self, seq_no: u64) -> Result<()> {
        self.pinned.lock().remove(&seq_no);
        Ok(())
    }
    async fn get_at_seq(&self, key: &[u8], _seq: u64) -> Result<Option<Vec<u8>>> {
        self.get(key).await
    }
    async fn last_seq_no(&self) -> Result<u64> {
        Ok(0)
    }
    async fn last_tx_id(&self) -> Result<TxId> {
        Ok(TxId::new(0))
    }
    async fn scan(
        &self,
        _start: std::ops::Bound<&[u8]>,
        _end: std::ops::Bound<&[u8]>,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        Ok(Vec::new())
    }
    async fn scan_prefix(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let data = self.data.lock();
        Ok(data
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect())
    }
    async fn scan_prefix_at(&self, prefix: &[u8], _seq_no: u64) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        self.scan_prefix(prefix).await
    }
}

fn bench_checkpoint_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("checkpoint_creation_latency");
    for size_kb in [1, 10, 100] {
        let payload = "x".repeat(size_kb * 1024);
        let metadata = serde_json::json!({ "payload": payload });

        group.bench_with_input(
            BenchmarkId::new("create_checkpoint_size_kb", size_kb),
            &size_kb,
            |b, _| {
                let storage = Arc::new(BenchStorage::new());
                let store = PersistentCheckpointStore::new(storage, "bench");
                let mut counter = 0u64;

                b.to_async(&rt).iter(|| {
                    counter += 1;
                    let store = &store;
                    let metadata = metadata.clone();
                    async move {
                        store
                            .create_checkpoint(
                                &format!("cp_{counter}"),
                                "col_bench",
                                counter,
                                TxId::new(counter),
                                metadata,
                            )
                            .await
                            .unwrap();
                    }
                });
            },
        );
    }
    group.finish();

    let mut group_cache = c.benchmark_group("checkpoint_cache_latency");

    let storage_hit = Arc::new(BenchStorage::new());
    let store_hit = PersistentCheckpointStore::new(storage_hit, "bench_cache");
    rt.block_on(async {
        store_hit
            .create_checkpoint(
                "cp_hit_target",
                "col",
                1,
                TxId::new(1),
                serde_json::json!({}),
            )
            .await
            .unwrap();
    });

    group_cache.bench_function("cache_hit_read", |b| {
        b.to_async(&rt).iter(|| async {
            let _ = store_hit.get_checkpoint("cp_hit_target").await.unwrap();
        });
    });

    let storage_miss = Arc::new(BenchStorage::new());
    let store_populator =
        PersistentCheckpointStore::new(storage_miss.clone(), "bench_cache").unwrap();
    rt.block_on(async {
        store_populator
            .create_checkpoint(
                "cp_miss_target",
                "col",
                1,
                TxId::new(1),
                serde_json::json!({}),
            )
            .await
            .unwrap();
    });

    group_cache.bench_function("cache_miss_read", |b| {
        b.to_async(&rt).iter(|| async {
            let fresh_store =
                PersistentCheckpointStore::new(storage_miss.clone(), "bench_cache").unwrap();
            let _ = fresh_store.get_checkpoint("cp_miss_target").await.unwrap();
        });
    });
    group_cache.finish();

    let mut group_rollback = c.benchmark_group("checkpoint_rollback");
    let storage_rb = Arc::new(BenchStorage::new());
    let store_rb = PersistentCheckpointStore::new(storage_rb, "bench_rb");
    rt.block_on(async {
        store_rb
            .create_checkpoint(
                "cp_rb_target",
                "col",
                1,
                TxId::new(1),
                serde_json::json!({}),
            )
            .await
            .unwrap();
    });

    group_rollback.bench_function("restore_checkpoint", |b| {
        b.to_async(&rt).iter(|| async {
            let _ = store_rb.restore_checkpoint("cp_rb_target").await.unwrap();
        });
    });
    group_rollback.finish();

    let mut group_throughput = c.benchmark_group("concurrent_checkpoint_throughput");
    for tasks_count in [1, 10, 100] {
        group_throughput.throughput(Throughput::Elements(tasks_count as u64));
        group_throughput.bench_with_input(
            BenchmarkId::new("concurrent_writers", tasks_count),
            &tasks_count,
            |b, &tasks| {
                let storage = Arc::new(BenchStorage::new());
                let store =
                    Arc::new(PersistentCheckpointStore::new(storage, "bench_conc").unwrap());
                let mut batch = 0u64;

                b.to_async(&rt).iter(|| {
                    batch += 1;
                    let store = Arc::clone(&store);
                    async move {
                        let mut set: JoinSet<Result<()>> = JoinSet::new();
                        for i in 0..tasks {
                            let store = Arc::clone(&store);
                            let cp_id = batch * 1000 + i as u64;
                            set.spawn(async move {
                                store
                                    .create_checkpoint(
                                        &format!("conc_cp_{cp_id}"),
                                        "col",
                                        cp_id,
                                        TxId::new(cp_id),
                                        serde_json::json!({}),
                                    )
                                    .await?;
                                Ok(())
                            });
                        }
                        while let Some(res) = set.join_next().await {
                            res.unwrap().unwrap();
                        }
                    }
                });
            },
        );
    }
    group_throughput.finish();
}

criterion_group!(benches, bench_checkpoint_operations);
criterion_main!(benches);
