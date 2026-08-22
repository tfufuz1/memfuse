//! LSM-Tree (Log-Structured Merge-Tree) storage engine.
// TODO: Missing module documentation
// INVARIANT: Zentraler Storage-Engine-Orchestrator des Triebwerks.
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
use crate::memtable::MemTable;
use crate::sstable::{create_block_cache, BlockCache, SstableBuilder, SstableReader};
use crate::wal::{Wal, WalOp};
use bytes::Bytes;
use memfuse_core::{
    DocId, IndexOp, MemFuseError, ResourceBudget, ResourceTracker, Result, SnapshotRegistry,
    StorageEngine, TxBuffer, TxId, TOMBSTONE_BIT,
};
use memfuse_crypto::crypto::KeyManager;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// LSM storage configuration.
// SEC-001 — Erweitere LsmConfig um `encryption_passphrase` und AES-256.
// TEST: cargo test -p memfuse-store test_encrypted_db_unreadable_without_key
// DONE: LsmConfig akzeptiert Passphrase, AES-256 wird für Disk-I/O verwendet.
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
    last_committed_tx: AtomicU64,
    /// Mutex to serialize commits and prevent snapshot inversion (parallel seq_no holes).
    commit_mutex: tokio::sync::Mutex<()>,
    cancel_token: tokio_util::sync::CancellationToken,
    task_tracker: tokio_util::task::TaskTracker,
}

impl LsmStorage {
    /// Creates a new LSM storage engine.
    pub async fn new(config: LsmConfig) -> Result<Self> {
        tokio::fs::create_dir_all(&config.path)
            .await
            .map_err(|e| MemFuseError::Storage(format!("Failed to create dir: {}", e)))?;

        // 🛡️ SICHERUNG: Directory FSync (FIND-STO-004)
        if let Some(parent) = config.path.parent() {
            if let Ok(dir) = tokio::fs::File::open(parent).await {
                let _ = dir.sync_all().await;
            }
        }

        // Persistent Salt Management (FIND-CRY-001)
        let salt_path = config.path.join("SALT");
        let salt = if let Ok(buf) = tokio::fs::read(&salt_path).await {
            if buf.len() != 32 {
                return Err(MemFuseError::Storage(format!(
                    "Invalid SALT length: expected 32, got {}",
                    buf.len()
                )));
            }
            buf
        } else {
            let mut buf = [0u8; 32];
            use rand::Rng;
            rand::thread_rng().fill(&mut buf);
            tokio::fs::write(&salt_path, &buf)
                .await
                .map_err(|e| MemFuseError::Storage(format!("Failed to write SALT: {}", e)))?;
            buf.to_vec()
        };

        let key_manager = config
            .encryption_passphrase
            .as_ref()
            .map(|p| KeyManager::try_new(p, &salt).map(Arc::new))
            .transpose()?;

        // Discover and sort all WAL files for replay
        let mut wal_files = Vec::new();
        let mut entries = tokio::fs::read_dir(&config.path)
            .await
            .map_err(|e| MemFuseError::Storage(format!("Failed to read data dir: {}", e)))?;

        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with("wal-") && name_str.ends_with(".log") {
                if let Ok(ts) = name_str[4..name_str.len() - 4].parse::<u128>() {
                    wal_files.push((ts, entry.path()));
                }
            } else if name_str == "wal.log" {
                // Legacy WAL
                wal_files.push((0, entry.path()));
            }
        }
        wal_files.sort_by_key(|(ts, _)| *ts);

        let memtable = MemTable::new();
        let mut max_seq = 0u64;
        let mut max_tx = 0u64;
        let mut replayed_size = 0u64;
        let mut last_wal = None;

        for (_ts, wal_path) in &wal_files {
            let wal = Wal::open_with_key_manager(wal_path, key_manager.clone()).await?;
            let wal_entries = wal.replay().await?;

            for (lsn, entry, _offset) in &wal_entries {
                if *lsn > max_seq {
                    max_seq = *lsn;
                }
                if entry.tx_id().inner() > max_tx && entry.tx_id().inner() < TxId::INTERNAL_BASE {
                    max_tx = entry.tx_id().inner();
                }
                match &entry.op {
                    WalOp::Put { key, value, tx_id } => {
                        replayed_size += (key.len() + value.len()) as u64;
                        memtable.put(
                            Bytes::from(key.clone()),
                            Bytes::from(value.clone()),
                            *lsn,
                            tx_id.inner(),
                        );
                    }
                    WalOp::Delete { key, tx_id } => {
                        replayed_size += key.len() as u64;
                        memtable.put(
                            Bytes::from(key.clone()),
                            Bytes::new(),
                            *lsn | TOMBSTONE_BIT,
                            tx_id.inner(),
                        );
                    }
                }
            }
            last_wal = Some(wal);
        }

        let wal = if let Some(w) = last_wal {
            w
        } else {
            // No WAL found, create a new one
            Wal::open_with_key_manager(config.path.join("wal.log"), key_manager.clone()).await?
        };

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

            // Recover max_seq and max_tx from SSTables
            if reader.metadata().max_seq > max_seq {
                max_seq = reader.metadata().max_seq;
            }
            if reader.metadata().max_tx_id > max_tx {
                // Ignore internal TxIds during recovery
                if reader.metadata().max_tx_id < TxId::INTERNAL_BASE {
                    max_tx = reader.metadata().max_tx_id;
                }
            }

            sstables.push(Arc::new(reader));
        }
        let sstables = Arc::new(RwLock::new(sstables));
        let snapshot_registry = Arc::new(SnapshotRegistry::new());

        // Spawn background compaction task
        // COMP-001 — Implementiere CompactionEngine::run_loop.
        // TEST: cargo test -p memfuse-store test_concurrent_reads_during_compaction
        // DONE: Triple-Test grün, keine Deadlocks in tokio::spawn.
        let compaction_engine = Arc::new(CompactionEngine::new(
            config.compaction.clone(),
            Arc::clone(&snapshot_registry),
            Arc::clone(&block_cache),
            key_manager.clone(),
            Arc::clone(&resource_tracker),
        ));
        let compaction_sstables = Arc::clone(&sstables);
        let compaction_path = config.path.clone();
        let cancel_token = tokio_util::sync::CancellationToken::new();
        let task_tracker = tokio_util::task::TaskTracker::new();

        let ct_clone = cancel_token.clone();
        task_tracker.spawn(async move {
            compaction_engine
                .run_loop(compaction_sstables, compaction_path, ct_clone)
                .await;
        });
        task_tracker.close();

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
            next_seq_no: AtomicU64::new(max_seq.saturating_add(1)),
            last_committed_tx: AtomicU64::new(max_tx),
            commit_mutex: tokio::sync::Mutex::new(()),
            cancel_token,
            task_tracker,
        })
    }

    /// Forces a flush (to be used by PersistentCheckpointStore or tests).
    pub async fn force_flush(&self) -> Result<()> {
        self.flush().await
    }

    /// Evaluates whether compaction should run and performs it if needed.
    #[doc(hidden)]
    pub async fn maybe_compact(&self) -> Result<bool> {
        let compaction_engine = CompactionEngine::new(
            self.config.compaction.clone(),
            Arc::clone(&self.snapshot_registry),
            Arc::clone(&self.block_cache),
            self.key_manager.clone(),
            Arc::clone(&self.budget),
        );
        compaction_engine
            .maybe_compact(&self.sstables, &self.config.path)
            .await
    }

    /// Signals the background compaction engine to stop.
    pub fn shutdown(&self) {
        self.cancel_token.cancel();
    }

    /// Waits for all spawned tasks to shut down fully.
    pub async fn wait_shutdown(&self) {
        self.shutdown();
        self.task_tracker.wait().await;
    }

    /// Spawns a background task tracked by this storage instance.
    pub fn spawn_tracked<F>(&self, future: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        self.task_tracker.spawn(future);
    }

    /// Rolls back the entire storage state to a specific transaction ID.
    /// This is a destructive operation that removes all data after the target TX.
    pub async fn rollback_to_tx(&self, target_tx: TxId) -> Result<()> {
        let _commit_lock = self.commit_mutex.lock().await;
        let mut state = self.state.write().await;

        // 1. Truncate WAL to the position after target_tx
        let (target_offset, target_hmac) = state.wal.find_tx_offset(target_tx).await?;
        state.wal.truncate(target_offset, target_hmac).await?;

        // 2. Clear current memtable (it might have data > target_tx)
        state.memtable = Arc::new(MemTable::new());
        state.immutable_memtables.clear();

        // 3. Handle SSTables: Remove SSTables that are entirely newer than target_tx
        let mut sstables_lock = self.sstables.write().await;
        let mut sst_to_remove = Vec::new();
        sstables_lock.retain(|sst| {
            if sst.metadata().min_tx_id > target_tx.inner() {
                sst_to_remove.push(sst.file_path().to_path_buf());
                false
            } else {
                true
            }
        });

        // 4. Update next_seq_no and last_committed_tx
        // Find max_seq from kept SSTables to avoid regressing next_seq_no
        let mut max_seq = 0;
        for sst in sstables_lock.iter() {
            max_seq = max_seq.max(sst.metadata().max_seq);
        }
        drop(sstables_lock);

        for path in sst_to_remove {
            tracing::info!("Removing SSTable during rollback: {:?}", path);
            let _ = tokio::fs::remove_file(path).await;
        }

        // 5. Re-populate memtable from truncated WAL
        let entries = state.wal.replay().await?;
        for (seq, entry, _offset) in entries {
            if seq > max_seq {
                max_seq = seq;
            }
            match entry.op {
                WalOp::Put { key, value, tx_id } => {
                    state.memtable.put(
                        bytes::Bytes::from(key),
                        bytes::Bytes::from(value),
                        seq,
                        tx_id.inner(),
                    );
                }
                WalOp::Delete { key, tx_id } => {
                    state.memtable.put(
                        bytes::Bytes::from(key),
                        bytes::Bytes::new(),
                        seq | TOMBSTONE_BIT,
                        tx_id.inner(),
                    );
                }
            }
        }

        self.next_seq_no.store(max_seq + 1, Ordering::SeqCst);
        self.last_committed_tx
            .store(target_tx.inner(), Ordering::SeqCst);

        tracing::info!(
            "Rollback to TX {} successful. Max seq: {}, WAL offset: {}",
            target_tx.inner(),
            max_seq,
            target_offset
        );

        Ok(())
    }

    /// Suspends execution briefly if memory usage exceeds 80% to apply backpressure.
    async fn apply_backpressure(&self) {
        if self.budget.memory_used()
            >= (self.config.max_ram_mb as f64 * 1024.0 * 1024.0 * 0.80) as u64
        {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    }
}

#[async_trait::async_trait]
impl StorageEngine for LsmStorage {
    async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let current_max_seq = self.next_seq_no.load(Ordering::Acquire);
        let res = self.get_at_seq(key, current_max_seq).await?;
        tracing::debug!(
            "LsmStorage::get key={:?} seq={} found={}",
            String::from_utf8_lossy(key),
            current_max_seq,
            res.is_some()
        );
        Ok(res)
    }

    async fn get_at_seq(&self, key: &[u8], seq_no: u64) -> Result<Option<Vec<u8>>> {
        let state = self.state.read().await;
        let last_tx = self.last_committed_tx.load(Ordering::Acquire);
        tracing::debug!(
            "LsmStorage::get_at_seq key={:?} seq={} last_tx={}",
            String::from_utf8_lossy(key),
            seq_no,
            last_tx
        );

        // 1. MemTable (only if seq_no in entry <= target seq_no AND tx_id <= last_tx)
        if let Some((val, seq, tx)) = state.memtable.get_at_seq(key, seq_no) {
            if tx <= last_tx || tx >= TxId::INTERNAL_BASE {
                if (seq & TOMBSTONE_BIT) != 0 {
                    return Ok(None);
                }
                return Ok(Some(val.to_vec()));
            }
        }

        // 2. Immutable MemTables (newest first)
        for mt in state.immutable_memtables.iter().rev() {
            if let Some((val, seq, tx)) = mt.get_at_seq(key, seq_no) {
                if tx <= last_tx {
                    if (seq & TOMBSTONE_BIT) != 0 {
                        return Ok(None);
                    }
                    return Ok(Some(val.to_vec()));
                }
            }
        }

        // 3. SSTables (newest first, filtered by seq_no and last_tx)
        let sstables = self.sstables.read().await;
        let last_tx = self.last_committed_tx.load(Ordering::Acquire);
        for sst in sstables.iter().rev() {
            // SSTables already only contain entries up to their last_key.
            // But we still need to check the entry's seq_no and tx_id.
            if let Some((val, seq, tx)) = sst.get(key).await? {
                tracing::debug!(
                    "LsmStorage::get_at_seq SSTable check: seq={} target_seq={} tx={} last_tx={}",
                    seq & !TOMBSTONE_BIT,
                    seq_no,
                    tx,
                    last_tx
                );
                if (seq & !TOMBSTONE_BIT) <= seq_no && tx <= last_tx {
                    if (seq & TOMBSTONE_BIT) != 0 {
                        tracing::debug!("LsmStorage::get_at_seq FOUND TOMBSTONE");
                        return Ok(None);
                    }
                    tracing::debug!("LsmStorage::get_at_seq MATCH found in SSTable");
                    return Ok(Some(val.to_vec()));
                } else {
                    tracing::debug!("LsmStorage::get_at_seq SKIPPED entry due to seq/tx filter");
                }
            }
        }

        Ok(None)
    }

    async fn put(&self, tx_id: TxId, key: &[u8], value: &[u8]) -> Result<()> {
        self.apply_backpressure().await;
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

    async fn delete_prefix(&self, tx_id: TxId, prefix: &[u8]) -> Result<u64> {
        let matching_keys = self.scan_prefix(prefix).await?;
        let mut deleted = 0u64;
        for (key, _) in matching_keys {
            self.delete(tx_id, &key).await?;
            deleted += 1;
        }
        Ok(deleted)
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
        self.apply_backpressure().await;
        if !self.budget.has_memory_capacity() {
            return Err(MemFuseError::Storage("Memory budget exceeded (95%)".into()));
        }

        // ANCHOR:ALG-FIX:D6-001 — Snapshot-Inversion bei parallel commit (INV-MVCC-1)
        // ANCHOR:ALG-FIX:D6-001 — Snapshot-Inversion bei parallel commit (INV-MVCC-1)
        // FIX: Commit-Mutex serialisiert fetch_add + memtable.put.
        // Ohne Mutex könnte seq=11 vor seq=10 fertig sein → Reader seq=11 sieht Lücke bei 10.
        let _commit_lock = self.commit_mutex.lock().await;

        let ops = self.tx_buffer.drain(tx_id);
        if ops.is_empty() {
            return Ok(());
        }
        let state = self.state.read().await;

        // --- PHASE 1: WAL Snapshot for atomic rollback ---
        let pre_tx_offset = state.wal.size();
        let pre_tx_hmac = state.wal.last_hmac_snapshot().await;

        let mut wal_ops = Vec::with_capacity(ops.len());
        let mut mem_updates = Vec::with_capacity(ops.len());

        for op in &ops {
            let seq_no = self.next_seq_no.fetch_add(1, Ordering::SeqCst);
            match op {
                IndexOp::Insert { doc_id: _, data } => {
                    let (key, value) = data;
                    wal_ops.push((
                        WalOp::Put {
                            tx_id,
                            key: key.clone(),
                            value: value.clone(),
                        },
                        seq_no,
                    ));
                    mem_updates.push((key.clone(), value.clone(), seq_no));
                }
                IndexOp::Delete { doc_id: _, data } => {
                    if let Some((key, _)) = data {
                        wal_ops.push((
                            WalOp::Delete {
                                tx_id,
                                key: key.clone(),
                            },
                            seq_no,
                        ));
                        mem_updates.push((key.clone(), Vec::new(), seq_no | TOMBSTONE_BIT));
                    }
                }
            }
        }

        // --- PHASE 2: Group Commit to WAL ---
        let wal_entries = state.wal.prepare_batch(wal_ops).await?;
        if let Err(e) = state.wal.append_batch(&wal_entries).await {
            // FATAL I/O ERROR: Physical Rollback of the WAL to pre-tx state
            state.wal.truncate(pre_tx_offset, pre_tx_hmac).await?;
            return Err(MemFuseError::Storage(format!(
                "Commit failed (at WAL append), WAL rollback executed: {}",
                e
            )));
        }

        // --- PHASE 3: Apply to MemTable ---
        for (key, value, seq) in mem_updates {
            let entry_size = key.len() + value.len() + 8;
            let _ = self.budget.consume_memory(entry_size as u64);
            state
                .memtable
                .put(Bytes::from(key), Bytes::from(value), seq, tx_id.inner());
        }

        // Update last committed transaction ID if it is not a system transaction
        if tx_id.inner() < TxId::INTERNAL_BASE {
            let mut current = self.last_committed_tx.load(Ordering::Acquire);
            while tx_id.inner() > current {
                match self.last_committed_tx.compare_exchange_weak(
                    current,
                    tx_id.inner(),
                    Ordering::SeqCst,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => {
                        break;
                    }
                    Err(actual) => current = actual,
                }
            }
            if tx_id.inner() <= current && tx_id.inner() != 0 {
                // Already superseded
            } else if tx_id.inner() == 0 {
                tracing::warn!("LsmStorage::commit tx=0 called — ignoring visibility update to prevent blackout");
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

    async fn rollback_to_tx(&self, tx_id: TxId) -> Result<()> {
        Self::rollback_to_tx(self, tx_id).await
    }

    async fn pin_checkpoint(&self, seq_no: u64) -> Result<()> {
        self.snapshot_registry.pin(seq_no);
        Ok(())
    }

    async fn unpin_checkpoint(&self, seq_no: u64) -> Result<()> {
        self.snapshot_registry.unpin(seq_no);
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
        // ANCHOR:ALG-FIX:D1-011 — Stale WAL-Dateien löschen nach Flush
        // Ohne Cleanup wächst die Disk-Usage unbegrenzt (eine WAL pro Flush).
        let old_wal_path = old_wal.path().to_path_buf();
        drop(old_wal);
        drop(state);

        let sst_path = {
            static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let count = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            self.config
                .path
                .join(format!("sst-{:020}-{:04}.sst", flush_id, count % 10000))
        };
        let mut builder =
            SstableBuilder::create_with_key_manager(&sst_path, self.key_manager.clone()).await?;

        for (k, v, seq, tx) in old_memtable.iter_latest() {
            builder.add(&k, &v, seq, tx).await?;
        }
        builder
            .finish()
            .await
            .map_err(|e| MemFuseError::Storage(format!("SSTable finish failed: {}", e)))?;

        let reader = SstableReader::open_with_key_manager(
            &sst_path,
            Arc::clone(&self.block_cache),
            self.key_manager.clone(),
        )
        .await
        .map_err(|e| MemFuseError::Storage(format!("SSTable open after flush failed: {}", e)))?;

        // Atomic transition: remove from immutable memtables and add to SSTables
        let mut state = self.state.write().await;
        let mut sstables = self.sstables.write().await;

        state
            .immutable_memtables
            .retain(|mt| !Arc::ptr_eq(mt, &old_memtable));

        // Update last_committed_tx if the SSTable contains newer committed transactions
        // FIX: Extract max_tx_id before move to satisfy borrow checker.
        let sst_max_tx = reader.metadata().max_tx_id;
        sstables.push(Arc::new(reader));

        if sst_max_tx < TxId::INTERNAL_BASE {
            let mut current = self.last_committed_tx.load(Ordering::Acquire);
            while sst_max_tx > current {
                match self.last_committed_tx.compare_exchange_weak(
                    current,
                    sst_max_tx,
                    Ordering::SeqCst,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => break,
                    Err(actual) => current = actual,
                }
            }
        }

        drop(sstables);
        drop(state);

        // Best-effort delete of old WAL (non-critical if it fails, as it will be replayed safely)
        if let Err(e) = tokio::fs::remove_file(&old_wal_path).await {
            tracing::debug!("Could not delete old WAL {:?}: {}", old_wal_path, e);
        }

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

    async fn last_seq_no(&self) -> Result<u64> {
        Ok(self.next_seq_no.load(Ordering::SeqCst).saturating_sub(1))
    }

    async fn last_tx_id(&self) -> Result<TxId> {
        Ok(TxId::new(self.last_committed_tx.load(Ordering::SeqCst)))
    }

    async fn scan_prefix(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        self.scan_prefix_at(prefix, u64::MAX).await
    }

    async fn scan_prefix_at(&self, prefix: &[u8], seq_no: u64) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let mut map = std::collections::BTreeMap::new();
        let state = self.state.read().await;
        let sstables = self.sstables.read().await;

        // Collect from SSTables
        for sst in sstables.iter() {
            let entries = sst.scan_prefix(prefix).await?;
            for (k, v, seq, _tx) in entries {
                let raw_seq = seq & !TOMBSTONE_BIT;
                if raw_seq <= seq_no {
                    let entry = map.entry(k.to_vec()).or_insert((v.to_vec(), seq));
                    if (seq & !TOMBSTONE_BIT) > (entry.1 & !TOMBSTONE_BIT) {
                        *entry = (v.to_vec(), seq);
                    }
                }
            }
        }

        // Collect from immutable memtables
        for mt in &state.immutable_memtables {
            for (k, v, seq, _tx) in mt.iter() {
                let raw_seq = seq & !TOMBSTONE_BIT;
                if k.starts_with(prefix) && raw_seq <= seq_no {
                    let entry = map.entry(k.to_vec()).or_insert((v.to_vec(), seq));
                    if (seq & !TOMBSTONE_BIT) > (entry.1 & !TOMBSTONE_BIT) {
                        *entry = (v.to_vec(), seq);
                    }
                }
            }
        }

        // Collect from active memtable
        for (k, v, seq, _tx) in state.memtable.iter() {
            let raw_seq = seq & !TOMBSTONE_BIT;
            if k.starts_with(prefix) && raw_seq <= seq_no {
                let entry = map.entry(k.to_vec()).or_insert((v.to_vec(), seq));
                if (seq & !TOMBSTONE_BIT) > (entry.1 & !TOMBSTONE_BIT) {
                    *entry = (v.to_vec(), seq);
                }
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
        let last_tx = self.last_committed_tx.load(Ordering::Acquire);

        // 1. SSTables (oldest first → newer entries overwrite)
        for sst in sstables.iter() {
            let entries = sst.scan_range(start.map(|s| s), end.map(|e| e)).await?;
            for (k, v, seq, _tx) in entries {
                let entry = map.entry(k.to_vec()).or_insert((v.to_vec(), seq));
                if (seq & !TOMBSTONE_BIT) > (entry.1 & !TOMBSTONE_BIT) {
                    *entry = (v.to_vec(), seq);
                }
            }
        }

        // 2. Immutable memtables (older → newer)
        for mt in &state.immutable_memtables {
            for (k, v, seq, tx) in mt.iter() {
                if tx > last_tx && tx < TxId::INTERNAL_BASE {
                    continue;
                }
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
        for (k, v, seq, tx) in state.memtable.iter() {
            if tx > last_tx && tx < TxId::INTERNAL_BASE {
                continue;
            }
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
    async fn test_delete_prefix_removes_all_matching_keys() {
        let (storage, _tmp) = test_storage().await;
        let tx1 = TxId::new(1);

        // 1. Mehrere Keys mit gemeinsamem Prefix "test:" einfügen
        storage.put(tx1, b"test:1", b"val1").await.unwrap();
        storage.put(tx1, b"test:2", b"val2").await.unwrap();
        storage.put(tx1, b"test:3", b"val3").await.unwrap();
        storage.put(tx1, b"other:1", b"val4").await.unwrap();
        storage.commit(tx1).await.unwrap();

        // 2. delete_prefix("test:") aufrufen in tx2
        let tx2 = TxId::new(2);
        let deleted = storage.delete_prefix(tx2, b"test:").await.unwrap();
        assert_eq!(deleted, 3);
        storage.commit(tx2).await.unwrap();

        // 3. Prüfen: alle "test:*"-Keys sind weg, andere Keys bleiben unberührt
        assert_eq!(storage.get(b"test:1").await.unwrap(), None);
        assert_eq!(storage.get(b"test:2").await.unwrap(), None);
        assert_eq!(storage.get(b"test:3").await.unwrap(), None);
        assert_eq!(
            storage.get(b"other:1").await.unwrap(),
            Some(b"val4".to_vec())
        );
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

    #[tokio::test]
    async fn test_lsm_rollback_persistence() {
        let tmp = TempDir::new().expect("temp dir");
        let config = LsmConfig {
            path: tmp.path().to_path_buf(),
            memtable_size_limit: 1024 * 1024,
            max_ram_mb: 64,
            tx_timeout: Duration::from_secs(60),
            compaction: CompactionConfig::default(),
            encryption_passphrase: None,
        };

        {
            let storage = LsmStorage::new(config.clone())
                .await
                .expect("create storage");

            let tx1 = TxId::new(1);
            storage.put(tx1, b"k1", b"v1").await.unwrap();
            storage.commit(tx1).await.unwrap();

            let tx2 = TxId::new(2);
            storage.put(tx2, b"k2", b"v2").await.unwrap();
            storage.commit(tx2).await.unwrap();

            // Verify both exist
            assert_eq!(storage.get(b"k1").await.unwrap(), Some(b"v1".to_vec()));
            assert_eq!(storage.get(b"k2").await.unwrap(), Some(b"v2".to_vec()));

            // Rollback to Tx1
            storage.rollback_to_tx(tx1).await.expect("rollback");

            assert_eq!(storage.get(b"k1").await.unwrap(), Some(b"v1".to_vec()));
            assert_eq!(storage.get(b"k2").await.unwrap(), None);
        }

        // Restart storage
        {
            let storage = LsmStorage::new(config).await.expect("restart storage");
            assert_eq!(storage.get(b"k1").await.unwrap(), Some(b"v1".to_vec()));
            assert_eq!(
                storage.get(b"k2").await.unwrap(),
                None,
                "k2 should NOT be replayed after rollback"
            );

            // Verify we can still append new transactions after rollback
            let tx3 = TxId::new(3);
            storage.put(tx3, b"k3", b"v3").await.unwrap();
            storage.commit(tx3).await.unwrap();
            assert_eq!(storage.get(b"k3").await.unwrap(), Some(b"v3".to_vec()));
        }
    }
    #[tokio::test]
    async fn test_rollback_with_sstables() {
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

        // 1. Insert data for TX 1, TX 2
        let tx1 = TxId::new(1);
        storage.put(tx1, b"k1", b"v1").await.unwrap();
        storage.commit(tx1).await.unwrap();

        let tx2 = TxId::new(2);
        storage.put(tx2, b"k2", b"v2").await.unwrap();
        storage.commit(tx2).await.unwrap();

        // 2. Flush (SSTable 1 contains TX 1, 2)
        storage.force_flush().await.unwrap();

        // 3. Insert data for TX 3, TX 4
        let tx3 = TxId::new(3);
        storage.put(tx3, b"k3", b"v3").await.unwrap();
        storage.commit(tx3).await.unwrap();

        let tx4 = TxId::new(4);
        storage.put(tx4, b"k4", b"v4").await.unwrap();
        storage.commit(tx4).await.unwrap();

        // 4. Flush (SSTable 2 contains TX 3, 4)
        storage.force_flush().await.unwrap();

        {
            let sstables = storage.sstables.read().await;
            assert_eq!(sstables.len(), 2);
        }

        // 5. Rollback to TX 2
        storage.rollback_to_tx(tx2).await.expect("rollback");

        // 6. Verify SSTable 2 is gone, 7. Verify SSTable 1 is still there.
        {
            let sstables = storage.sstables.read().await;
            assert_eq!(sstables.len(), 1, "SSTable 2 should be deleted");
            assert_eq!(sstables[0].metadata().max_tx_id, 2);
        }

        assert_eq!(storage.get(b"k1").await.unwrap(), Some(b"v1".to_vec()));
        let val2 = storage.get(b"k2").await.unwrap();
        let ssts = storage.sstables.read().await;
        let sst_meta = if !ssts.is_empty() {
            format!(
                "min_tx: {}, max_tx: {}, range: [{:?}, {:?}]",
                ssts[0].metadata().min_tx_id,
                ssts[0].metadata().max_tx_id,
                ssts[0].metadata().first_key,
                ssts[0].metadata().last_key
            )
        } else {
            "NO SSTABLES".into()
        };
        assert_eq!(
            val2,
            Some(b"v2".to_vec()),
            "k2 should be found. SST 0 meta: {}",
            sst_meta
        );
        assert_eq!(storage.get(b"k3").await.unwrap(), None);
        assert_eq!(storage.get(b"k4").await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_pin_unpin_checkpoint_prevents_gc() {
        let tmp = TempDir::new().expect("temp dir");
        let config = LsmConfig {
            path: tmp.path().to_path_buf(),
            memtable_size_limit: 1024,
            max_ram_mb: 64,
            tx_timeout: Duration::from_secs(60),
            compaction: CompactionConfig {
                min_sstables_per_tier: 2,
                size_ratio: 4.0,
                check_interval: Duration::from_secs(30),
                yield_threshold: 1000,
                max_memory_bytes: Some(1024 * 1024),
            },
            encryption_passphrase: None,
        };
        let storage = LsmStorage::new(config.clone())
            .await
            .expect("create storage");

        // 1. Insert and commit data
        let tx1 = TxId::new(1);
        storage.put(tx1, b"key1", b"val1").await.unwrap();
        storage.commit(tx1).await.unwrap();
        let seq1 = storage.last_seq_no().await.unwrap();

        // 2. Pin seq1
        storage.pin_checkpoint(seq1).await.expect("pin");

        // 3. Delete key1 and commit
        let tx2 = TxId::new(2);
        storage.delete(tx2, b"key1").await.unwrap();
        storage.commit(tx2).await.unwrap();

        // 4. Force flush and compaction
        storage.force_flush().await.unwrap();

        let engine = CompactionEngine::new(
            config.compaction.clone(),
            storage.snapshot_registry.clone(),
            storage.block_cache.clone(),
            storage.key_manager.clone(),
            Arc::clone(&storage.budget),
        );

        engine
            .maybe_compact(&storage.sstables, &storage.config.path)
            .await
            .expect("compact");

        // 5. Verify min_active_seqno is correct
        assert_eq!(storage.snapshot_registry.min_active_seqno(), seq1);

        // 6. Unpin
        storage.unpin_checkpoint(seq1).await.expect("unpin");
        assert_eq!(storage.snapshot_registry.min_active_seqno(), u64::MAX);

        // 7. Compact again
        engine
            .maybe_compact(&storage.sstables, &storage.config.path)
            .await
            .unwrap();
    }
}
