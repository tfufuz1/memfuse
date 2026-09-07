use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use memfuse_checkpoint::PersistentCheckpointStore;
use memfuse_core::{BoxFuture, Result, StorageEngine, StorageStats, TxId};
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

impl StorageEngine for BenchStorage {
    fn get<'a>(&'a self, key: &'a [u8]) -> BoxFuture<'a, Result<Option<Vec<u8>>>> {
        Box::pin(async move { Ok(self.data.lock().get(key).cloned()) })
    }
    fn put<'a>(
        &'a self,
        _tx_id: TxId,
        key: &'a [u8],
        value: &'a [u8],
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            self.data.lock().insert(key.to_vec(), value.to_vec());
            Ok(())
        })
    }
    fn delete<'a>(&'a self, _tx_id: TxId, key: &'a [u8]) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            self.data.lock().remove(key);
            Ok(())
        })
    }
    fn commit<'a>(&'a self, _tx_id: TxId) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move { Ok(()) })
    }
    fn rollback<'a>(&'a self, _tx_id: TxId) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move { Ok(()) })
    }
    fn rollback_to_tx<'a>(&'a self, _tx_id: TxId) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move { Ok(()) })
    }
    fn flush<'a>(&'a self) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move { Ok(()) })
    }
    fn stats<'a>(&'a self) -> BoxFuture<'a, Result<StorageStats>> {
        Box::pin(async move {
            Ok(StorageStats {
                num_segments: 0,
                total_size_bytes: 0,
                memtable_size_bytes: 0,
            })
        })
    }
    fn pin_checkpoint<'a>(&'a self, seq_no: u64) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            self.pinned.lock().insert(seq_no);
            Ok(())
        })
    }
    fn unpin_checkpoint<'a>(&'a self, seq_no: u64) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            self.pinned.lock().remove(&seq_no);
            Ok(())
        })
    }
    fn get_at_seq<'a>(
        &'a self,
        key: &'a [u8],
        _seq: u64,
    ) -> BoxFuture<'a, Result<Option<Vec<u8>>>> {
        Box::pin(async move { self.get(key).await })
    }
    fn last_seq_no<'a>(&'a self) -> BoxFuture<'a, Result<u64>> {
        Box::pin(async move { Ok(0) })
    }
    fn last_tx_id<'a>(&'a self) -> BoxFuture<'a, Result<TxId>> {
        Box::pin(async move { Ok(TxId::new(0)) })
    }
    fn scan<'a>(
        &'a self,
        _start: std::ops::Bound<&'a [u8]>,
        _end: std::ops::Bound<&'a [u8]>,
    ) -> BoxFuture<'a, Result<Vec<(Vec<u8>, Vec<u8>)>>> {
        Box::pin(async move { Ok(Vec::new()) })
    }
    fn scan_prefix<'a>(
        &'a self,
        prefix: &'a [u8],
    ) -> BoxFuture<'a, Result<Vec<(Vec<u8>, Vec<u8>)>>> {
        Box::pin(async move {
            let data = self.data.lock();
            Ok(data
                .iter()
                .filter(|(k, _)| k.starts_with(prefix))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect())
        })
    }
    fn scan_prefix_at<'a>(
        &'a self,
        prefix: &'a [u8],
        _seq_no: u64,
    ) -> BoxFuture<'a, Result<Vec<(Vec<u8>, Vec<u8>)>>> {
        Box::pin(async move { self.scan_prefix(prefix).await })
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
                let store = PersistentCheckpointStore::new(storage, "bench").unwrap();
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
    let store_hit = PersistentCheckpointStore::new(storage_hit, "bench_cache").unwrap();
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
    let store_rb = PersistentCheckpointStore::new(storage_rb, "bench_rb").unwrap();
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
