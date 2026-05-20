//! LSM-Tree (Log-Structured Merge-Tree) storage engine.
// ANCHOR:DOC:DOC-LSM-001 — Missing module documentation
// WP:WP-0.0 PRIO:3 NEEDS:NONE
// AGENT:02 DATE:2026-05-16 STATUS:REVIEW
// CREATED:2026-05-09 DEADLINE:NONE
// ANCHOR:ARCH:LSM-001 — Zentraler Storage-Engine-Orchestrator des Triebwerks.
// WP:WP-0.0 PRIO:1 NEEDS:NONE
// AGENT:01 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-05 DEADLINE:NONE
// IMPLEMENTS: StorageEngine Trait (memfuse-core/src/traits.rs)
// READ-PATH:  get() → Active MemTable → Immutable MemTables → SSTables (newest first)
// WRITE-PATH: put()/delete() → TxBuffer → commit() → WAL + MemTable
// FLUSH:      MemTable > size_limit → rotate → SSTable schreiben → cleanup
// BACKGROUND: CompactionEngine läuft als tokio::spawn loop
// INVARIANTE: WAL Replay bei Neustart stellt MemTable deterministisch wieder her.
//!
//! The `LsmStorage` engine provides a high-performance, persistent key-value store
//! implementing the `StorageEngine` trait.
//!
//! ## Architecture
//! - **MemTable**: An in-memory sorted buffer (`BTreeMap`) that absorbs all writes.
//!   Once it reaches a size threshold, it is frozen (becoming an immutable MemTable)
//!   and eventually flushed to disk as an SSTable.
//! - **WAL (Write-Ahead Log)**: Ensures durability by logging all operations before
//!   they are applied to the MemTable.
//! - **SSTables (Sorted String Tables)**: Persistent, immutable files on disk.
//!   They are organized into tiers by the Compaction Engine.
//! - **Compaction**: A background process that merges multiple SSTables into one,
//!   deduplicating keys and garbage-collecting tombstones.
//! - **MVCC (Multi-Version Concurrency Control)**: Supports snapshots and transactional
//!   isolation via sequence numbers and the `SnapshotRegistry`.
//!
//! ## Read Path
//! 1. Check the active MemTable.
//! 2. Check immutable MemTables (from newest to oldest).
//! 3. Check SSTables (from newest to oldest).
//!
//! ## Write Path
//! 1. Operations are staged in the `TxBuffer`.
//! 2. On `commit()`, operations are assigned sequence numbers, written to the WAL,
//!    and then applied to the active MemTable.

use crate::compaction::{CompactionConfig, CompactionEngine};
use crate::crypto::KeyManager;
use crate::memtable::MemTable;
use crate::sstable::{create_block_cache, BlockCache, SstableBuilder, SstableReader};
use crate::wal::{Wal, WalEntry, WalOp};
use async_trait::async_trait;
use bytes::Bytes;
use memfuse_core::{
    DocId, IndexOp, MemFuseError, ResourceBudget, ResourceTracker, Result, SnapshotRegistry,
    StorageEngine, TxBuffer, TxId, TOMBSTONE_BIT,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// LSM storage configuration.
// ANCHOR:TODO:SEC-001 — Erweitere LsmConfig um `encryption_passphrase` und AES-256.
// WP:WP-3.2 PRIO:1 NEEDS:COL-001
// AGENT:@JULES-10 DATE:2026-05-09 STATUS:REVIEW
// TEST: cargo test -p memfuse-store test_encrypted_db_unreadable_without_key
// DONE: LsmConfig akzeptiert Passphrase, AES-256 wird für Disk-I/O verwendet.
// SUCCESSOR: @JULES-13 — "Encryption ist impl. Bitte Specs finalisieren."
#[derive(Clone, Debug)]
/// Configuration for the LSM storage engine.
pub struct LsmConfig {
    /// Path to the data directory.
    pub path: PathBuf,
    /// Maximum size of the memtable before flushing to disk.
    pub memtable_size_limit: usize,
    /// Maximum RAM usage for the storage engine in MB.
    pub max_ram_mb: u64,
    /// Timeout for transactions in the buffer.
    pub tx_timeout: Duration,
    /// Configuration for background compaction.
    pub compaction: CompactionConfig,
    pub encryption_passphrase: Option<String>,
}

impl Default for LsmConfig {
    fn default() -> Self {
        Self {
            path: PathBuf::from("memfuse_data"),
            memtable_size_limit: 64 * 1024 * 1024,
            max_ram_mb: 2048,
            tx_timeout: Duration::from_secs(60),
            compaction: CompactionConfig::default(),
            encryption_passphrase: None,
        }
    }
}

struct LsmState {
    memtable: Arc<MemTable>,
    immutable_memtables: Vec<Arc<MemTable>>,
    wal: Wal,
}

/// LSM-Tree based storage engine.
pub struct LsmStorage {
    config: LsmConfig,
    key_manager: Option<Arc<KeyManager>>,
    state: RwLock<LsmState>,
    /// SSTables stored separately for shared access with compaction engine.
    sstables: Arc<RwLock<Vec<Arc<SstableReader>>>>,
    tx_buffer: TxBuffer<(Vec<u8>, Vec<u8>)>,
    budget: Arc<ResourceTracker>,
    block_cache: Arc<BlockCache>,
    pub snapshot_registry: Arc<SnapshotRegistry>,
    next_seq_no: AtomicU64,
    /// Mutex to serialize commits and prevent snapshot inversion (parallel seq_no holes).
    commit_mutex: tokio::sync::Mutex<()>,
}

impl LsmStorage {
    /// Creates a new LSM storage engine.
    pub async fn new(config: LsmConfig) -> Result<Self> {
        tokio::fs::create_dir_all(&config.path)
            .await
            .map_err(|e| MemFuseError::Storage(format!("Failed to create dir: {}", e)))?;

        let key_manager = config
            .encryption_passphrase
            .as_ref()
            .map(|p| KeyManager::try_new(p).map(Arc::new))
            .transpose()?;

        let wal =
            Wal::open_with_key_manager(config.path.join("wal.log"), key_manager.clone()).await?;
        let memtable = MemTable::new();

        // Replay WAL
        let wal_entries = wal.replay().await?;
        let mut max_seq = 0u64;
        let mut replayed_size = 0u64;

        for (lsn, entry) in &wal_entries {
            if *lsn > max_seq {
                max_seq = *lsn;
            }
            match &entry.op {
                WalOp::Put { key, value, .. } => {
                    replayed_size += (key.len() + value.len()) as u64;
                    memtable.put(Bytes::from(key.clone()), Bytes::from(value.clone()), *lsn);
                }
                WalOp::Delete { key, .. } => {
                    replayed_size += key.len() as u64;
                    memtable.put(Bytes::from(key.clone()), Bytes::new(), lsn | TOMBSTONE_BIT);
                }
            }
        }

        let budget_config = ResourceBudget {
            memory_limit: config.max_ram_mb * 1024 * 1024,
        };
        let resource_tracker = Arc::new(ResourceTracker::new(budget_config));
        if replayed_size > 0 {
            let _ = resource_tracker.consume_memory(replayed_size);
        }

        let tx_buffer = TxBuffer::new_with_config(16, config.tx_timeout);

        // Load existing SSTables and sort by filename (which includes seq_no)
        let mut sst_files = Vec::new();
        if let Ok(mut entries) = tokio::fs::read_dir(&config.path).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                if entry.path().extension().is_some_and(|ext| ext == "sst") {
                    sst_files.push(entry.path());
                }
            }
        }
        sst_files.sort();

        let block_cache = create_block_cache(64); // 64MB block cache for SSTables

        let mut sstables = Vec::new();
        for path in sst_files {
            let reader = SstableReader::open_with_key_manager(
                path,
                Arc::clone(&block_cache),
                key_manager.clone(),
            )
            .await?;
            sstables.push(Arc::new(reader));
        }
        let sstables = Arc::new(RwLock::new(sstables));
        let snapshot_registry = Arc::new(SnapshotRegistry::new());

        // Spawn background compaction task
        // ANCHOR:TODO:COMP-001 — Implementiere CompactionEngine::run_loop.
        // WP:WP-1.1 PRIO:1 NEEDS:NONE
        // AGENT:@JULES-02 DATE:2026-05-12 STATUS:REVIEW
        // TEST: cargo test -p memfuse-store test_concurrent_reads_during_compaction
        // DONE: Triple-Test grün, keine Deadlocks in tokio::spawn.
        // SUCCESSOR: @JULES-04 — "Background compaction ist stabil. Collections können aufbauen."
        let compaction_engine = Arc::new(CompactionEngine::new(
            config.compaction.clone(),
            Arc::clone(&snapshot_registry),
            Arc::clone(&block_cache),
        ));
        let compaction_sstables = Arc::clone(&sstables);
        let compaction_path = config.path.clone();
        tokio::spawn(async move {
            compaction_engine
                .run_loop(compaction_sstables, compaction_path)
                .await;
        });

        Ok(Self {
            config,
            key_manager,
            state: RwLock::new(LsmState {
                memtable: Arc::new(memtable),
                immutable_memtables: Vec::new(),
                wal,
            }),
            sstables,
            tx_buffer,
            budget: resource_tracker,
            block_cache,
            snapshot_registry,
            next_seq_no: AtomicU64::new(max_seq + 1),
            commit_mutex: tokio::sync::Mutex::new(()),
        })
    }

    /// Returns the last committed sequence number.
    pub fn last_seq_no(&self) -> u64 {
        self.next_seq_no.load(Ordering::Acquire).saturating_sub(1)
    }

    /// Pins a sequence number to prevent premature GC (SAOS Checkpoint).
    pub async fn pin_checkpoint(&self, seq_no: u64) -> Result<()> {
        self.snapshot_registry.pin(seq_no);
        Ok(())
    }

    /// Unpins a sequence number.
    pub async fn unpin_checkpoint(&self, seq_no: u64) -> Result<()> {
        self.snapshot_registry.unpin(seq_no);
        Ok(())
    }

    /// Forces a flush (to be used by CheckpointManager or tests).
    pub async fn force_flush(&self) -> Result<()> {
        self.flush().await
    }
}

#[async_trait]
impl StorageEngine for LsmStorage {
    async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let state = self.state.read().await;

        if let Some((val, seq)) = state.memtable.get(key) {
            if (seq & TOMBSTONE_BIT) != 0 {
                return Ok(None);
            }
            return Ok(Some(val.to_vec()));
        }

        for mt in state.immutable_memtables.iter().rev() {
            if let Some((val, seq)) = mt.get(key) {
                if (seq & TOMBSTONE_BIT) != 0 {
                    return Ok(None);
                }
                return Ok(Some(val.to_vec()));
            }
        }

        let sstables = self.sstables.read().await;
        for sst in sstables.iter().rev() {
            if let Some((val, seq)) = sst.get(key).await? {
                if (seq & TOMBSTONE_BIT) != 0 {
                    return Ok(None);
                }
                return Ok(Some(val.to_vec()));
            }
        }

        Ok(None)
    }

    async fn put(&self, tx_id: TxId, key: &[u8], value: &[u8]) -> Result<()> {
        self.budget.apply_backpressure().await;
        if !self.budget.has_memory_capacity() {
            return Err(MemFuseError::Storage("Memory budget exceeded (95%)".into()));
        }
        let doc_id = {
            let hash = blake3::hash(key);
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(&hash.as_bytes()[..8]);
            DocId::new(u64::from_le_bytes(bytes))
        };

        self.tx_buffer.stage(
            tx_id,
            IndexOp::Insert {
                doc_id,
                data: (key.to_vec(), value.to_vec()),
            },
        );
        Ok(())
    }

    async fn delete(&self, tx_id: TxId, key: &[u8]) -> Result<()> {
        let doc_id = {
            let hash = blake3::hash(key);
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(&hash.as_bytes()[..8]);
            DocId::new(u64::from_le_bytes(bytes))
        };

        self.tx_buffer.stage(
            tx_id,
            IndexOp::Delete {
                doc_id,
                data: Some((key.to_vec(), Vec::new())),
            },
        );
        Ok(())
    }

    async fn commit(&self, tx_id: TxId) -> Result<()> {
        self.budget.apply_backpressure().await;
        if !self.budget.has_memory_capacity() {
            return Err(MemFuseError::Storage("Memory budget exceeded (95%)".into()));
        }

        // ANCHOR:ALG-FIX:D6-001 — Snapshot-Inversion bei parallel commit (INV-MVCC-1)
        // WP:WP-0.0 PRIO:1 NEEDS:NONE
        // AGENT:13 DATE:2026-05-08 STATUS:DONE
        // CREATED:2026-05-08 DEADLINE:NONE
        // FIX: Commit-Mutex serialisiert fetch_add + memtable.put.
        // Ohne Mutex könnte seq=11 vor seq=10 fertig sein → Reader seq=11 sieht Lücke bei 10.
        let _commit_lock = self.commit_mutex.lock().await;

        let ops = self.tx_buffer.drain(tx_id);
        let state = self.state.read().await;

        let integrity_key = if let Some(km) = &self.key_manager {
            km.integrity_key()?
        } else {
            *b"memfuse-integrity-key-v1\0\0\0\0\0\0\0\0"
        };

        for op in ops {
            let seq_no = self.next_seq_no.fetch_add(1, Ordering::SeqCst);

            match op {
                IndexOp::Insert { data, .. } => {
                    let (key, value) = data;
                    let entry = WalEntry::try_new(
                        WalOp::Put {
                            tx_id,
                            key: key.clone(),
                            value: value.clone(),
                        },
                        seq_no,
                        &integrity_key,
                    )?;
                    let entry_size = key.len() + value.len() + 8;
                    let _ = self.budget.consume_memory(entry_size as u64);
                    state.wal.append(&entry).await?;
                    state
                        .memtable
                        .put(Bytes::from(key), Bytes::from(value), seq_no);
                }
                IndexOp::Delete { data, .. } => {
                    if let Some((key, _)) = data {
                        let entry = WalEntry::try_new(
                            WalOp::Delete {
                                tx_id,
                                key: key.clone(),
                            },
                            seq_no,
                            &integrity_key,
                        )?;
                        let _ = self.budget.consume_memory(key.len() as u64 + 8);
                        state.wal.append(&entry).await?;
                        state
                            .memtable
                            .put(Bytes::from(key), Bytes::new(), seq_no | TOMBSTONE_BIT);
                    }
                }
            }
        }

        // Check if flush is needed
        if state.memtable.size() > self.config.memtable_size_limit {
            drop(state);
            self.flush().await?;
        }

        Ok(())
    }

    async fn rollback(&self, tx_id: TxId) -> Result<()> {
        self.tx_buffer.discard(tx_id);
        Ok(())
    }

    async fn flush(&self) -> Result<()> {
        let mut state = self.state.write().await;
        if state.memtable.is_empty() {
            return Ok(());
        }

        let flush_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| MemFuseError::Storage(format!("Time error: {}", e)))?
            .as_micros();
        let wal_path = self.config.path.join(format!("wal-{}.log", flush_id));
        let new_wal = Wal::open_with_key_manager(wal_path, self.key_manager.clone()).await?;

        let old_memtable = std::mem::replace(&mut state.memtable, Arc::new(MemTable::new()));
        let old_wal = std::mem::replace(&mut state.wal, new_wal);
        state.immutable_memtables.push(old_memtable.clone());

        // ANCHOR:ALG-FIX:D1-011 — Stale WAL-Dateien löschen nach Flush
        // WP:WP-0.0 PRIO:1 NEEDS:NONE
        // AGENT:13 DATE:2026-05-08 STATUS:DONE
        // CREATED:2026-05-08 DEADLINE:NONE
        // Ohne Cleanup wächst die Disk-Usage unbegrenzt (eine WAL pro Flush).
        let old_wal_path = old_wal.path().to_path_buf();
        drop(old_wal);
        drop(state);

        // Best-effort delete of old WAL (non-critical if it fails)
        if let Err(e) = tokio::fs::remove_file(&old_wal_path).await {
            tracing::debug!("Could not delete old WAL {:?}: {}", old_wal_path, e);
        }

        let sst_path = {
            static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let count = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            self.config
                .path
                .join(format!("sst-{:020}-{:04}.sst", flush_id, count % 10000))
        };
        let mut builder =
            SstableBuilder::create_with_key_manager(&sst_path, self.key_manager.clone()).await?;

        for (k, v, seq) in old_memtable.iter() {
            builder.add(&k, &v, seq).await?;
        }
        builder.finish().await?;

        let reader = SstableReader::open_with_key_manager(
            &sst_path,
            Arc::clone(&self.block_cache),
            self.key_manager.clone(),
        )
        .await?;

        // Atomic transition: remove from immutable memtables and add to SSTables
        let mut state = self.state.write().await;
        let mut sstables = self.sstables.write().await;

        state
            .immutable_memtables
            .retain(|mt| !Arc::ptr_eq(mt, &old_memtable));
        sstables.push(Arc::new(reader));

        drop(sstables);
        drop(state);

        let bytes_freed = old_memtable.size() as u64;
        self.budget.release_memory(bytes_freed);

        tracing::info!("Flushed memtable to SSTable: {} bytes", bytes_freed);
        Ok(())
    }

    async fn stats(&self) -> Result<memfuse_core::StorageStats> {
        let state = self.state.read().await;
        let sstables = self.sstables.read().await;
        let num_segments = sstables.len();
        let mut total_size_bytes = 0;
        for sst in sstables.iter() {
            total_size_bytes += sst.metadata().file_size;
        }

        let mut memtable_size_bytes = state.memtable.size() as u64;
        for m in &state.immutable_memtables {
            memtable_size_bytes += m.size() as u64;
        }

        Ok(memfuse_core::StorageStats {
            num_segments,
            total_size_bytes,
            memtable_size_bytes,
        })
    }

    async fn scan_prefix(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let mut map = std::collections::BTreeMap::new();
        let state = self.state.read().await;
        let sstables = self.sstables.read().await;

        for sst in sstables.iter() {
            let entries = sst.scan_prefix(prefix).await?;
            // ANCHOR:ALG-FIX:D1-002 — seq_no-Vergleich bei scan_prefix (INV-LSM-2)
            // WP:WP-0.0 PRIO:1 NEEDS:NONE
            // AGENT:13 DATE:2026-05-08 STATUS:DONE
            // CREATED:2026-05-08 DEADLINE:NONE
            // Ohne seq_no-Vergleich kann eine ältere SSTable einen neueren Wert
            // überschreiben wenn die SSTable-Reihenfolge nicht strikt chronologisch ist.
            for (k, v, seq) in entries {
                let entry = map.entry(k.to_vec()).or_insert((v.to_vec(), seq));
                if (seq & !TOMBSTONE_BIT) > (entry.1 & !TOMBSTONE_BIT) {
                    *entry = (v.to_vec(), seq);
                }
            }
        }

        // ANCHOR:ALG-FIX:D1-008 — seq_no-Vergleich für MemTable-Entries in scan_prefix
        // WP:WP-0.0 PRIO:1 NEEDS:NONE
        // AGENT:13 DATE:2026-05-08 STATUS:DONE
        // CREATED:2026-05-08 DEADLINE:NONE
        // Immutable MemTables können ältere Versionen eines Keys enthalten
        // als SSTables falls Flush-Reihenfolge abweicht.
        for mt in &state.immutable_memtables {
            for (k, v, seq) in mt.iter() {
                if k.starts_with(prefix) {
                    let entry = map.entry(k.to_vec()).or_insert((v.to_vec(), seq));
                    if (seq & !TOMBSTONE_BIT) > (entry.1 & !TOMBSTONE_BIT) {
                        *entry = (v.to_vec(), seq);
                    }
                }
            }
        }

        // Active memtable always has the newest data (commit_mutex serializes writes)
        for (k, v, seq) in state.memtable.iter() {
            if k.starts_with(prefix) {
                map.insert(k.to_vec(), (v.to_vec(), seq));
            }
        }

        let mut results = Vec::new();
        for (k, (v, seq)) in map {
            if (seq & TOMBSTONE_BIT) == 0 {
                results.push((k, v));
            }
        }

        Ok(results)
    }

    async fn scan(
        &self,
        start: std::ops::Bound<&[u8]>,
        end: std::ops::Bound<&[u8]>,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        use std::ops::Bound;

        let mut map = std::collections::BTreeMap::<Vec<u8>, (Vec<u8>, u64)>::new();
        let state = self.state.read().await;
        let sstables = self.sstables.read().await;

        // 1. SSTables (oldest first → newer entries overwrite)
        for sst in sstables.iter() {
            let entries = sst.scan_range(start.map(|s| s), end.map(|e| e)).await?;
            for (k, v, seq) in entries {
                let entry = map.entry(k.to_vec()).or_insert((v.to_vec(), seq));
                if (seq & !TOMBSTONE_BIT) > (entry.1 & !TOMBSTONE_BIT) {
                    *entry = (v.to_vec(), seq);
                }
            }
        }

        // 2. Immutable memtables (older → newer)
        for mt in &state.immutable_memtables {
            for (k, v, seq) in mt.iter() {
                let in_range = match start {
                    Bound::Included(s) => k.as_ref() >= s,
                    Bound::Excluded(s) => k.as_ref() > s,
                    Bound::Unbounded => true,
                } && match end {
                    Bound::Included(e) => k.as_ref() <= e,
                    Bound::Excluded(e) => k.as_ref() < e,
                    Bound::Unbounded => true,
                };
                if in_range {
                    let entry = map.entry(k.to_vec()).or_insert((v.to_vec(), seq));
                    if (seq & !TOMBSTONE_BIT) > (entry.1 & !TOMBSTONE_BIT) {
                        *entry = (v.to_vec(), seq);
                    }
                }
            }
        }

        // 3. Active memtable (newest, always wins on tie)
        for (k, v, seq) in state.memtable.iter() {
            let in_range = match start {
                Bound::Included(s) => k.as_ref() >= s,
                Bound::Excluded(s) => k.as_ref() > s,
                Bound::Unbounded => true,
            } && match end {
                Bound::Included(e) => k.as_ref() <= e,
                Bound::Excluded(e) => k.as_ref() < e,
                Bound::Unbounded => true,
            };
            if in_range {
                map.insert(k.to_vec(), (v.to_vec(), seq));
            }
        }

        // 4. Filter tombstones
        let results = map
            .into_iter()
            .filter(|(_, (_, seq))| (seq & TOMBSTONE_BIT) == 0)
            .map(|(k, (v, _))| (k, v))
            .collect();

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn test_storage() -> (LsmStorage, TempDir) {
        let tmp = TempDir::new().expect("temp dir");
        let config = LsmConfig {
            path: tmp.path().to_path_buf(),
            memtable_size_limit: 1024 * 1024,
            max_ram_mb: 64,
            tx_timeout: Duration::from_secs(60),
            compaction: CompactionConfig::default(),
            encryption_passphrase: None,
        };
        let storage = LsmStorage::new(config).await.expect("create storage");
        (storage, tmp)
    }

    #[tokio::test]
    async fn test_put_get_roundtrip() {
        let (storage, _tmp) = test_storage().await;
        let tx = TxId::new(1);

        storage.put(tx, b"hello", b"world").await.expect("put");
        storage.commit(tx).await.expect("commit");

        let val = storage.get(b"hello").await.expect("get");
        assert_eq!(val, Some(b"world".to_vec()));
    }

    #[tokio::test]
    async fn test_delete() {
        let (storage, _tmp) = test_storage().await;
        let tx1 = TxId::new(1);

        storage.put(tx1, b"key", b"val").await.expect("put");
        storage.commit(tx1).await.expect("commit");

        let tx2 = TxId::new(2);
        storage.delete(tx2, b"key").await.expect("delete");
        storage.commit(tx2).await.expect("commit");

        let val = storage.get(b"key").await.expect("get");
        assert_eq!(val, None);
    }

    #[tokio::test]
    async fn test_rollback() {
        let (storage, _tmp) = test_storage().await;
        let tx = TxId::new(1);

        storage.put(tx, b"key", b"val").await.expect("put");
        storage.rollback(tx).await.expect("rollback");

        let val = storage.get(b"key").await.expect("get");
        assert_eq!(val, None);
    }

    #[tokio::test]
    async fn test_get_nonexistent() {
        let (storage, _tmp) = test_storage().await;
        let val = storage.get(b"nonexistent").await.expect("get");
        assert_eq!(val, None);
    }

    #[tokio::test]
    async fn test_overwrite() {
        let (storage, _tmp) = test_storage().await;

        let tx1 = TxId::new(1);
        storage.put(tx1, b"key", b"val1").await.expect("put1");
        storage.commit(tx1).await.expect("commit1");

        let tx2 = TxId::new(2);
        storage.put(tx2, b"key", b"val2").await.expect("put2");
        storage.commit(tx2).await.expect("commit2");

        let val = storage.get(b"key").await.expect("get");
        assert_eq!(val, Some(b"val2".to_vec()));
    }

    #[tokio::test]
    async fn test_flush_creates_sstable() {
        let tmp = TempDir::new().expect("temp dir");
        let config = LsmConfig {
            path: tmp.path().to_path_buf(),
            memtable_size_limit: 64, // Tiny limit to trigger flush easily
            max_ram_mb: 64,
            tx_timeout: Duration::from_secs(60),
            compaction: CompactionConfig::default(),
            encryption_passphrase: None,
        };
        let storage = LsmStorage::new(config).await.expect("create storage");

        // Insert enough data to exceed the tiny memtable limit
        let tx = TxId::new(1);
        for i in 0..10u8 {
            let key = format!("key-{:03}", i);
            let val = format!("value-{:03}", i);
            storage
                .put(tx, key.as_bytes(), val.as_bytes())
                .await
                .expect("put");
        }
        storage.commit(tx).await.expect("commit");

        // Verify data is still readable (from SSTable after flush)
        for i in 0..10u8 {
            let key = format!("key-{:03}", i);
            let expected = format!("value-{:03}", i);
            let val = storage.get(key.as_bytes()).await.expect("get");
            assert_eq!(
                val,
                Some(expected.into_bytes()),
                "key {} missing after flush",
                key
            );
        }

        // Verify SSTable file(s) were created
        let stats = storage.stats().await.expect("stats");
        assert!(
            stats.num_segments > 0,
            "Expected at least one SSTable segment after flush"
        );
    }

    #[tokio::test]
    async fn test_scan_range() {
        let (storage, _tmp) = test_storage().await;
        let tx = TxId::new(1);

        // Insert ordered keys
        for c in b'a'..=b'z' {
            let key = [c];
            let val = [c, c];
            storage.put(tx, &key, &val).await.expect("put");
        }
        storage.commit(tx).await.expect("commit");

        // Scan [c, g] inclusive
        use std::ops::Bound;
        let results = storage
            .scan(Bound::Included(b"c"), Bound::Included(b"g"))
            .await
            .expect("scan");
        assert_eq!(results.len(), 5); // c, d, e, f, g
        assert_eq!(results[0].0, b"c");
        assert_eq!(results[4].0, b"g");

        // Scan (c, g) exclusive
        let results = storage
            .scan(Bound::Excluded(b"c"), Bound::Excluded(b"g"))
            .await
            .expect("scan");
        assert_eq!(results.len(), 3); // d, e, f

        // Scan unbounded start to d inclusive
        let results = storage
            .scan(Bound::Unbounded, Bound::Included(b"d"))
            .await
            .expect("scan");
        assert_eq!(results.len(), 4); // a, b, c, d

        // Scan with deleted key
        let tx2 = TxId::new(2);
        storage.delete(tx2, b"e").await.expect("delete");
        storage.commit(tx2).await.expect("commit");

        let results = storage
            .scan(Bound::Included(b"d"), Bound::Included(b"f"))
            .await
            .expect("scan");
        assert_eq!(results.len(), 2); // d, f (e deleted)
    }
}
