use bytes::Bytes;
use criterion::{criterion_group, criterion_main, Criterion};
use memfuse_store::memtable::MemTable;
use parking_lot::RwLock;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

const SHARD_COUNT: usize = 16;
type SequenceNumber = u64;
type TransactionId = u64;
type MemTableEntry = (SequenceNumber, Bytes, TransactionId);
type MemTableMap = BTreeMap<Bytes, Vec<MemTableEntry>>;

struct OldMemTableShard {
    entries: RwLock<MemTableMap>,
}

struct OldMemTable {
    shards: [OldMemTableShard; SHARD_COUNT],
    size: AtomicUsize,
    min_tx: AtomicU64,
    max_tx: AtomicU64,
}

impl OldMemTable {
    fn new() -> Self {
        Self {
            shards: std::array::from_fn(|_| OldMemTableShard {
                entries: RwLock::new(BTreeMap::new()),
            }),
            size: AtomicUsize::new(0),
            min_tx: AtomicU64::new(u64::MAX),
            max_tx: AtomicU64::new(0),
        }
    }

    #[inline]
    fn shard_for(key: &[u8]) -> usize {
        key.first().copied().unwrap_or(0) as usize % SHARD_COUNT
    }

    fn put(&self, key: Bytes, value: Bytes, seq_no: u64, tx_id: u64) {
        let additional_size = key.len() + value.len() + 16;
        let shard_idx = Self::shard_for(&key);
        let mut entries = self.shards[shard_idx].entries.write();

        let versions = entries.entry(key).or_default();
        versions.push((seq_no, value, tx_id));

        self.size.fetch_add(additional_size, Ordering::Relaxed);

        loop {
            let current_min = self.min_tx.load(Ordering::Acquire);
            if tx_id >= current_min
                || self
                    .min_tx
                    .compare_exchange(current_min, tx_id, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
            {
                break;
            }
        }
        loop {
            let current_max = self.max_tx.load(Ordering::Acquire);
            if tx_id <= current_max
                || self
                    .max_tx
                    .compare_exchange(current_max, tx_id, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
            {
                break;
            }
        }
    }
}

fn bench_concurrent_puts(c: &mut Criterion) {
    let mut group = c.benchmark_group("MemTable Concurrent Puts");
    let num_threads = 8;
    let puts_per_thread = 1000;

    group.bench_function("old_first_byte_sharding", |b| {
        b.iter(|| {
            let mt = Arc::new(OldMemTable::new());
            let handles: Vec<_> = (0..num_threads)
                .map(|t| {
                    let mt = Arc::clone(&mt);
                    thread::spawn(move || {
                        for i in 0..puts_per_thread {
                            let mut key = b"__col:hr:\x00".to_vec();
                            key.push(0u8);
                            key.extend_from_slice(format!("doc-{t}-{i}").as_bytes());
                            let val = Bytes::from("val");
                            let seq = (t * puts_per_thread + i + 1) as u64;
                            mt.put(Bytes::from(key), val, seq, (t + 1) as u64);
                        }
                    })
                })
                .collect();
            for h in handles {
                h.join().unwrap();
            }
        });
    });

    group.bench_function("new_full_key_blake3_sharding", |b| {
        b.iter(|| {
            let mt = Arc::new(MemTable::new());
            let handles: Vec<_> = (0..num_threads)
                .map(|t| {
                    let mt = Arc::clone(&mt);
                    thread::spawn(move || {
                        for i in 0..puts_per_thread {
                            let mut key = b"__col:hr:\x00".to_vec();
                            key.push(0u8);
                            key.extend_from_slice(format!("doc-{t}-{i}").as_bytes());
                            let val = Bytes::from("val");
                            let seq = (t * puts_per_thread + i + 1) as u64;
                            mt.put(Bytes::from(key), val, seq, (t + 1) as u64);
                        }
                    })
                })
                .collect();
            for h in handles {
                h.join().unwrap();
            }
        });
    });

    group.finish();
}

criterion_group!(benches, bench_concurrent_puts);
criterion_main!(benches);
