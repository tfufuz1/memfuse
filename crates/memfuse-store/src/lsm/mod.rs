//! LSM-Tree (Log-Structured Merge-Tree) storage engine.
// FILE-CONTEXT
// STAND: 2026-08-30T21:49:55Z (SESSION: 283abf0f)
// ZWECK: LSM-Tree-Implementierung (MemTable + SSTable + Compaction)
// INVARIANTEN: Compaction darf keine Daten verlieren; WAL-Replay vor MemTable-Aufbau; LOCK-REIHENFOLGE: commit_mutex → state.write/read → MemTable-RwLock
//              Single-Commit: state.write (Flush-Schutz). Group-Commit-Leader: state.read (commit_mutex hält Isolation).
// NICHT-OFFENSICHTLICH: Compaction-Lock muss VOR MemTable-Lock genommen werden (Deadlock-Gefahr)
// SIEHE AUCH: wal.rs, sstable.rs, DECISIONS.md ADR-003

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
//!    Newer sequence numbers shadow older ones for the same key.
//!
//! ## Write Path
//! 1. Operations are staged in the `TxBuffer`.
//! 2. On `commit()`, operations acquire `commit_mutex` to serialize sequence assignment,
//!    are written to the WAL (with fsync durability), and applied to the active MemTable.
//! 3. When the MemTable exceeds `memtable_size_limit`, it rotates to an immutable MemTable
//!    and is flushed asynchronously to a new SSTable file on disk.
//!
//! ## Compaction
//! Compaction runs as a background task. When the number of SSTables in a tier exceeds
//! configured thresholds, compaction merges multiple SSTables into a single new SSTable,
//! deduplicating key versions and garbage-collecting tombstones not pinned by active snapshots.
//!
//! ## `commit_mutex` Role
//! `commit_mutex` serializes sequence allocation and WAL batch preparation during commits, preventing
//! snapshot inversion. In the group commit leader path, `commit_mutex` is released prior to executing physical
//! disk I/O (`wal.append_batch`) and re-acquired afterwards for MemTable updates / visibility advancement (and on error for WAL rollback).
//!
//! ## Lock Hierarchy & Concurrency Control
//! To prevent deadlocks, locks across the LSM storage engine must be acquired in the following order:
//! 1. `commit_mutex` (`tokio::sync::Mutex<()>`) - Acquired during sequence/batch preparation, rollback_to_tx, and state mutations. Released before disk I/O in group commit leader happy path, and re-acquired for MemTable update and visibility advancement.
//! 2. `state` write lock (`tokio::sync::RwLock<LsmState>`) - Protects active/immutable memtable pointers & WAL.
//! 3. `sstables` write lock (`tokio::sync::RwLock<Vec<Arc<SstableReader>>>`) - Protects SSTable set.
//!    Read locks on `state` and `sstables` may be acquired concurrently without holding `commit_mutex`.

use crate::compaction::{CompactionConfig, CompactionEngine};
use crate::memtable::MemTable;
use crate::sstable::{BlockCache, SstableBuilder, SstableReader};
use crate::wal::{Wal, WalOp};
use bytes::Bytes;
use memfuse_core::{
    BoxFuture, DocId, IndexOp, MemFuseError, ResourceTracker, Result, SnapshotRegistry,
    StorageEngine, TxBuffer, TxId, TOMBSTONE_BIT,
};
use memfuse_crypto::crypto::KeyManager;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

pub mod commit;
pub mod flush;
pub mod group_commit;
pub mod recovery;
pub mod scan;

#[cfg(test)]
mod tests;

use group_commit::{GroupCommitRequest, PendingCommitQueue, WalQueueGuard};
use scan::{check_in_range, SstableScanMode};

/// Maximum key size allowed for LSM operations (65,535 bytes).
pub const MAX_KEY_SIZE: usize = 65_535;

/// Maximum value size allowed for LSM operations (128MB).
pub const MAX_VALUE_SIZE: usize = 134_217_728;

/// Maximum batch size for `delete_many` operations (10,000 items).
pub const MAX_BATCH_SIZE: usize = 10_000;

/// Maximum factor for internal merge set size relative to limit in bounded scans.
pub const MAX_INTERNAL_MERGE_ENTRIES_FACTOR: usize = 8;

/// Maximum batch size for group commits (1,000 transactions).
pub const MAX_GROUP_COMMIT_BATCH_SIZE: usize = 1_000;

/// Minimum surviving entry threshold required to rebuild a new SSTable during rollback.
/// Below this threshold (1..7 entries), surviving entries from a spanning SSTable are inserted
/// directly into the MemTable instead of allocating a full new SSTable/manifest pipeline.
pub const MIN_ENTRIES_FOR_SSTABLE_REBUILD: usize = 8;

fn validate_key(key: &[u8]) -> Result<()> {
    if key.is_empty() {
        return Err(MemFuseError::InvalidInput("Key cannot be empty".into()));
    }
    if key.len() > MAX_KEY_SIZE {
        return Err(MemFuseError::InvalidInput(format!(
            "Key length ({} bytes) exceeds limit of {} bytes",
            key.len(),
            MAX_KEY_SIZE
        )));
    }
    Ok(())
}

#[cfg(not(feature = "docid-128"))]
fn derive_doc_id(key: &[u8]) -> DocId {
    let hash = blake3::hash(key);
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&hash.as_bytes()[..8]);
    DocId::new(u64::from_le_bytes(bytes))
}

#[cfg(feature = "docid-128")]
fn derive_doc_id(key: &[u8]) -> DocId {
    let hash = blake3::hash(key);
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&hash.as_bytes()[..16]);
    DocId::new(u128::from_le_bytes(bytes))
}

fn validate_value(value: &[u8]) -> Result<()> {
    if value.len() > MAX_VALUE_SIZE {
        return Err(MemFuseError::InvalidInput(format!(
            "Value length ({} bytes) exceeds limit of {} bytes",
            value.len(),
            MAX_VALUE_SIZE
        )));
    }
    Ok(())
}

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
    /// Time window in microseconds to batch concurrent WAL commits before issuing fsync.
    /// Set to 0 to disable group commit batching (immediate single commit).
    pub group_commit_window_micros: u64,
    /// Number of shards for the block cache.
    /// Default is 64 (increased from 16 to reduce lock contention during concurrent BM25 range scans).
    pub block_cache_shards: usize,
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
            group_commit_window_micros: 500,
            block_cache_shards: 64,
        }
    }
}

/// Proof that `commit_mutex` is currently held by the calling task.
/// Can only be constructed while holding the mutex guard.
pub(super) struct CommitGuard<'a> {
    _lock: &'a tokio::sync::MutexGuard<'a, ()>,
}

pub(super) struct LsmState {
    memtable: Arc<MemTable>,
    immutable_memtables: Vec<Arc<MemTable>>,
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
    wal: RwLock<Arc<Wal>>,
    pub snapshot_registry: Arc<SnapshotRegistry>,
    /// Persistent CompactionEngine instance — retains counter across maybe_compact() calls.
    /// Prevents SSTable name collisions from fresh-counter ad-hoc instantiation (audit H-3).
    compaction_engine: Arc<CompactionEngine>,
    manifest: Arc<crate::manifest::Manifest>,
    next_seq_no: AtomicU64,
    last_committed_tx: AtomicU64,
    /// Mutex to serialize commits and prevent snapshot inversion (parallel seq_no holes).
    commit_mutex: tokio::sync::Mutex<()>,
    cancel_token: tokio_util::sync::CancellationToken,
    task_tracker: tokio_util::task::TaskTracker,
    flush_counter: AtomicU64,
    segment_counter: AtomicU64,
    budget_tracking_drift_bytes: std::sync::atomic::AtomicU64,
    pending_commit_queue: tokio::sync::Mutex<Option<PendingCommitQueue>>,
    wal_queue_depth: Arc<std::sync::atomic::AtomicUsize>,
    pressure_rx: tokio::sync::watch::Receiver<crate::system_pressure::SystemPressure>,
    intent_locks: std::sync::Mutex<std::collections::HashMap<Vec<u8>, TxId>>,
}

impl LsmStorage {
    /// Returns a watch receiver for monitoring system pressure levels.
    pub fn pressure_receiver(
        &self,
    ) -> tokio::sync::watch::Receiver<crate::system_pressure::SystemPressure> {
        self.pressure_rx.clone()
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

    /// Gracefully closes the storage engine, stopping background tasks and flushing active memtable to disk.
    pub async fn close(&self) -> Result<()> {
        self.wait_shutdown().await;
        self.flush().await?;
        Ok(())
    }

    /// Spawns a background task tracked by this storage instance.
    pub fn spawn_tracked<F>(&self, future: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        self.task_tracker.spawn(future);
    }

    #[doc(hidden)]
    pub async fn simulate_wal_append_failure_for_test(&self) {
        #[cfg(feature = "fault-injection")]
        crate::wal::FAIL_APPEND_FOR_TX.store(u64::MAX, std::sync::atomic::Ordering::SeqCst);
    }

    #[doc(hidden)]
    pub async fn restore_wal_file_handle_for_test(&self) {
        #[cfg(feature = "fault-injection")]
        crate::wal::FAIL_APPEND_FOR_TX.store(0, std::sync::atomic::Ordering::SeqCst);
    }

    /// Returns the accumulated total memory budget tracking drift in bytes caused by
    /// unbudgeted memtable puts during commit when memory limit was exceeded.
    pub fn budget_tracking_drift_bytes(&self) -> u64 {
        self.budget_tracking_drift_bytes
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Maximum threshold for surviving entries during rollback below which entries are retained in memtable
    /// instead of creating a new SSTable (M-4 optimization).
    pub const ROLLBACK_INLINE_THRESHOLD_ENTRIES: usize = 1;
}

impl StorageEngine for LsmStorage {
    /// # ACID-Garantie
    /// Bietet Snapshot-Isolations-Point-Reads des aktuellsten committed Zustands.
    ///
    /// # Fehler
    /// Gibt `Err` zurück, wenn I/O auf SSTables fehlschlägt oder Block-Dekodierung scheitert.
    ///
    /// # Panics
    /// Panikt nicht in Produktionscode.
    fn get<'a>(&'a self, key: &'a [u8]) -> BoxFuture<'a, Result<Option<Bytes>>> {
        Box::pin(async move {
            validate_key(key)?;
            let current_max_seq = self.next_seq_no.load(Ordering::Acquire);
            let res = self.get_at_seq(key, current_max_seq).await?;
            tracing::debug!(
                "LsmStorage::get key={:?} seq={} found={}",
                String::from_utf8_lossy(key),
                current_max_seq,
                res.is_some()
            );
            Ok(res)
        })
    }

    /// # ACID-Garantie
    /// Garantierte Snapshot-Isolation zum angegebenen Sequenz-Zeitpunkt ohne Phantom-Reads.
    ///
    /// # Fehler
    /// Gibt `Err` zurück bei I/O- oder Dekodierungsfehlern.
    ///
    /// # Panics
    /// Panikt nicht in Produktionscode.
    fn get_at_seq<'a>(
        &'a self,
        key: &'a [u8],
        seq_no: u64,
    ) -> BoxFuture<'a, Result<Option<Bytes>>> {
        Box::pin(async move {
            validate_key(key)?;
            // Genau EINMAL laden — Snapshot-Konsistenz über die gesamte Methode (INVARIANT-2)
            let snapshot_tx = self.last_committed_tx.load(Ordering::Acquire);
            let state = self.state.read().await;
            tracing::debug!(
                "LsmStorage::get_at_seq key={:?} seq={} snapshot_tx={}",
                String::from_utf8_lossy(key),
                seq_no,
                snapshot_tx
            );

            // 1. MemTable (only if seq_no in entry <= target seq_no AND tx_id <= snapshot_tx)
            if let Some((val, seq, _tx)) = state.memtable.get_at_seq(key, seq_no, snapshot_tx) {
                if (seq & TOMBSTONE_BIT) != 0 {
                    return Ok(None);
                }
                return Ok(Some(val));
            }

            // 2. Immutable MemTables (newest first)
            for mt in state.immutable_memtables.iter().rev() {
                if let Some((val, seq, _tx)) = mt.get_at_seq(key, seq_no, snapshot_tx) {
                    if (seq & TOMBSTONE_BIT) != 0 {
                        return Ok(None);
                    }
                    return Ok(Some(val));
                }
            }

            // 3. SSTables (newest first, filtered by seq_no and snapshot_tx)
            let sstables = self.sstables.read().await;
            for sst in sstables.iter().rev() {
                // SSTables already only contain entries up to their last_key.
                // But we still need to check the entry's seq_no and tx_id.
                if let Some((val, seq, tx)) = sst.get(key).await? {
                    tracing::debug!(
                    "LsmStorage::get_at_seq SSTable check: seq={} target_seq={} tx={} snapshot_tx={}",
                    seq & !TOMBSTONE_BIT,
                    seq_no,
                    tx,
                    snapshot_tx
                );
                    if (seq & !TOMBSTONE_BIT) <= seq_no && tx <= snapshot_tx {
                        if (seq & TOMBSTONE_BIT) != 0 {
                            tracing::debug!("LsmStorage::get_at_seq FOUND TOMBSTONE");
                            return Ok(None);
                        }
                        tracing::debug!("LsmStorage::get_at_seq MATCH found in SSTable");
                        return Ok(Some(val));
                    }
                    tracing::debug!("LsmStorage::get_at_seq SKIPPED entry due to seq/tx filter");
                }
            }

            Ok(None)
        })
    }

    /// # ACID-Garantie
    /// Staged die Insertion im In-Memory TxBuffer. Wird erst nach `commit()` dauerhaft.
    ///
    /// # Fehler
    /// Gibt `Err` zurück, wenn das Speicherbudget (95%) überschritten ist.
    ///
    /// # Panics
    /// Panikt nicht in Produktionscode.
    fn put<'a>(&'a self, tx_id: TxId, key: &'a [u8], value: &'a [u8]) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            validate_key(key)?;
            validate_value(value)?;
            self.apply_backpressure().await;
            if !self.budget.has_memory_capacity() {
                return Err(MemFuseError::Storage("Memory budget exceeded (95%)".into()));
            }
            let doc_id = derive_doc_id(key);

            self.tx_buffer.stage_kv(
                tx_id,
                IndexOp::Insert {
                    doc_id,
                    data: (key.to_vec(), value.to_vec()),
                },
            )?;
            Ok(())
        })
    }

    fn put_if_absent<'a>(
        &'a self,
        tx_id: TxId,
        key: &'a [u8],
        value: &'a [u8],
    ) -> BoxFuture<'a, Result<bool>> {
        Box::pin(async move {
            validate_key(key)?;
            validate_value(value)?;
            self.apply_backpressure().await;
            if !self.budget.has_memory_capacity() {
                return Err(MemFuseError::Storage("Memory budget exceeded (95%)".into()));
            }

            // 1. Check if key is staged in tx_buffer (by any transaction)
            if matches!(self.tx_buffer.staged_status(key), Some(true)) {
                return Ok(false);
            }

            // 2. Intent Lock check and registration
            {
                let mut locks = self.intent_locks.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(&existing_tx) = locks.get(key) {
                    if existing_tx != tx_id {
                        return Ok(false);
                    }
                } else {
                    locks.insert(key.to_vec(), tx_id);
                }
            }

            // 3. Query committed state
            let current_max_seq = self.next_seq_no.load(Ordering::Acquire);
            let is_present = match self.get_at_seq(key, current_max_seq).await {
                Ok(opt) => opt.is_some(),
                Err(e) => {
                    let mut locks = self.intent_locks.lock().unwrap_or_else(|e| e.into_inner());
                    if locks.get(key) == Some(&tx_id) {
                        locks.remove(key);
                    }
                    return Err(e);
                }
            };

            if is_present {
                let mut locks = self.intent_locks.lock().unwrap_or_else(|e| e.into_inner());
                if locks.get(key) == Some(&tx_id) {
                    locks.remove(key);
                }
                return Ok(false);
            }

            // 4. Stage operation in tx_buffer
            let doc_id = derive_doc_id(key);

            if let Err(e) = self.tx_buffer.stage_kv(
                tx_id,
                IndexOp::Insert {
                    doc_id,
                    data: (key.to_vec(), value.to_vec()),
                },
            ) {
                let mut locks = self.intent_locks.lock().unwrap_or_else(|e| e.into_inner());
                if locks.get(key) == Some(&tx_id) {
                    locks.remove(key);
                }
                return Err(e);
            }

            Ok(true)
        })
    }

    fn delete_many<'a>(&'a self, tx_id: TxId, keys: Vec<Vec<u8>>) -> BoxFuture<'a, Result<u64>> {
        Box::pin(async move {
            if keys.len() > MAX_BATCH_SIZE {
                return Err(MemFuseError::InvalidInput(format!(
                    "Batch size ({} items) exceeds limit of {} items",
                    keys.len(),
                    MAX_BATCH_SIZE
                )));
            }
            for key in &keys {
                validate_key(key)?;
            }
            let count = keys.len() as u64;
            if count == 0 {
                return Ok(0);
            }

            let ops: Vec<IndexOp<(Vec<u8>, Vec<u8>)>> = keys
                .into_iter()
                .map(|key| {
                    let doc_id = derive_doc_id(&key);
                    IndexOp::Delete {
                        doc_id,
                        data: Some((key, Vec::new())),
                    }
                })
                .collect();

            self.tx_buffer.stage_many(tx_id, ops)?;
            Ok(count)
        })
    }

    fn delete_prefix<'a>(&'a self, tx_id: TxId, prefix: &'a [u8]) -> BoxFuture<'a, Result<u64>> {
        Box::pin(async move {
            let matching_keys: Vec<Vec<u8>> = self
                .scan_prefix(prefix)
                .await?
                .into_iter()
                .map(|(key, _)| key)
                .collect();
            self.delete_many(tx_id, matching_keys).await
        })
    }

    /// # ACID-Garantie
    /// Staged einen Tombstone im TxBuffer. Erst nach `commit()` wirksam.
    ///
    /// # Fehler
    /// Gibt `Err` zurück, wenn das Speicherbudget überschritten ist.
    ///
    /// # Panics
    /// Panikt nicht in Produktionscode.
    fn delete<'a>(&'a self, tx_id: TxId, key: &'a [u8]) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            validate_key(key)?;
            let doc_id = derive_doc_id(key);

            self.tx_buffer.stage_kv(
                tx_id,
                IndexOp::Delete {
                    doc_id,
                    data: Some((key.to_vec(), Vec::new())),
                },
            )?;
            Ok(())
        })
    }

    /// # ACID-Garantie
    /// Serialisiertes Group-Commit in das WAL inkl. fsync. Atomares Rollback bei I/O-Fehler.
    /// Nach erfolgreichem Return ist die Transaktion absturzsicher auf Disk (INVARIANT-1).
    ///
    /// # Fehler
    /// Gibt `Err` bei Disk-/I/O-Fehlern zurück und stellt den vorherigen WAL-Zustand wieder her.
    ///
    /// # Panics
    /// Panikt nicht in Produktionscode.
    // AI-TAG[SMELL][RESOLVED] audit-M-6: Drift-Counter budget_tracking_drift_bytes wird in LsmStorage::flush nach erfolgreichem Flush auf 0 zurückgesetzt.
    fn commit<'a>(&'a self, tx_id: TxId) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            self.apply_backpressure().await;
            if !self.budget.has_memory_capacity() {
                return Err(MemFuseError::Storage("Memory budget exceeded (95%)".into()));
            }

            struct IntentLockGuard<'a>(&'a LsmStorage, TxId);
            impl<'a> Drop for IntentLockGuard<'a> {
                fn drop(&mut self) {
                    self.0.clear_intent_locks_for_tx(self.1);
                }
            }
            let _intent_guard = IntentLockGuard(self, tx_id);

            // ANCHOR[ALG-FIX:D6-001] STATUS:DONE (TS:2026-06-01T00:00:00Z) — Snapshot-Inversion bei parallel commit (INV-MVCC-1)
            // FIX: Commit-Mutex serialisiert fetch_add + wal.prepare_batch.
            let _commit_lock = self.commit_mutex.lock().await;

            let ops = self.tx_buffer.drain_kv(tx_id);
            if ops.is_empty() {
                self.cleanup_intent_locks_for_tx(tx_id);
                return Ok(());
            }

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
                    _ => {
                        self.cleanup_intent_locks_for_tx(tx_id);
                        return Err(MemFuseError::InvalidInput(
                            "Unsupported operation type staged in LSM commit".to_string(),
                        ));
                    }
                }
            }

            // --- PHASE 2: Prepare WAL entries under commit_mutex ---
            // commit_mutex ist gehalten; WAL I/O erfolgt außerhalb des state-Locks.
            let wal = self.wal.read().await.clone();
            let (wal_entries, prev_hmac_snapshot) = wal.prepare_batch(wal_ops).await?;

            // If group commit window is disabled (0 micros), perform immediate single commit
            if self.config.group_commit_window_micros == 0 {
                if let Err(e) = wal.append_batch(wal_entries).await {
                    let _ = wal.restore_last_hmac(prev_hmac_snapshot).await;
                    // FATAL I/O ERROR: Physical Rollback to last committed transaction state
                    let last_tx = TxId::new(self.last_committed_tx.load(Ordering::Acquire));
                    let commit_guard = CommitGuard {
                        _lock: &_commit_lock,
                    };
                    if let Err(rollback_err) =
                        self.rollback_to_tx_locked(last_tx, &commit_guard).await
                    {
                        tracing::error!(
                            "Failed to execute rollback_to_tx_locked after failed WAL append: {}",
                            rollback_err
                        );
                    }
                    self.cleanup_intent_locks_for_tx(tx_id);
                    return Err(MemFuseError::Storage(format!(
                        "Commit failed (at WAL append), WAL rollback executed: {}",
                        e
                    )));
                }

                // PHASE 4: MemTable-Update — kurzer Write-Lock nur für In-Memory-Schreibvorgang
                let state = self.state.write().await;
                self.advance_visibility(tx_id);
                self.apply_mem_updates(&state.memtable, &mem_updates, tx_id);

                let should_flush = state.memtable.size() > self.config.memtable_size_limit;
                drop(state);
                if should_flush {
                    self.flush().await?;
                }

                self.cleanup_intent_locks_for_tx(tx_id);
                return Ok(());
            }

            // --- PHASE 2b: Group Commit Coordination ---
            let mut queue_guard = self.pending_commit_queue.lock().await;

            if let Some(ref mut queue) = *queue_guard {
                // RAII guard increments wal_queue_depth while follower is waiting in group commit queue
                let _wal_queue_guard = WalQueueGuard::new(Arc::clone(&self.wal_queue_depth));

                // Follower task: enqueue request with oneshot channel and await leader's notification
                let (tx, rx) = tokio::sync::oneshot::channel();
                let req = GroupCommitRequest {
                    tx_id,
                    wal_entries,
                    mem_updates,
                    sender: tx,
                };
                queue.requests.push(req);
                let is_full = queue.requests.len() >= MAX_GROUP_COMMIT_BATCH_SIZE;
                let notify_full = if is_full {
                    Some(queue.notify_full.clone())
                } else {
                    None
                };
                drop(queue_guard);
                drop(_commit_lock);

                if let Some(notify) = notify_full {
                    notify.notify_one();
                }

                let res = match tokio::time::timeout(self.config.tx_timeout, rx).await {
                    Ok(Ok(res)) => res,
                    Ok(Err(_)) => Err(MemFuseError::Internal(
                        "Group commit leader dropped without sending result".to_string(),
                    )),
                    Err(_) => {
                        let mut queue_guard = self.pending_commit_queue.lock().await;
                        *queue_guard = None;
                        drop(queue_guard);
                        Err(MemFuseError::CommitTimeout {
                            tx_id: tx_id.inner(),
                        })
                    }
                };
                self.cleanup_intent_locks_for_tx(tx_id);
                res
            } else {
                // Batch Leader task: initialize batch for followers without pushing leader's own channel.
                // Leader retains its own tx_id, wal_entries, mem_updates locally.
                let leader_tx_id = tx_id;
                let leader_wal_entries = wal_entries;
                let leader_mem_updates = mem_updates;

                let notify_full = Arc::new(tokio::sync::Notify::new());
                *queue_guard = Some(PendingCommitQueue {
                    requests: Vec::new(),
                    first_prev_hmac: prev_hmac_snapshot,
                    notify_full: notify_full.clone(),
                });
                drop(queue_guard);
                drop(_commit_lock);

                // Zero-Wait-Heuristik for low-load single-writer latency:
                // Yield briefly to allow any concurrent tasks waiting to enqueue to do so,
                // then check if followers arrived.
                tokio::task::yield_now().await;
                let has_followers = {
                    let q = self.pending_commit_queue.lock().await;
                    q.as_ref().is_some_and(|q| !q.requests.is_empty())
                };

                if has_followers {
                    // Wait for group commit window or until MAX_GROUP_COMMIT_BATCH_SIZE is reached
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_micros(self.config.group_commit_window_micros)) => {},
                        _ = notify_full.notified() => {},
                    }
                }

                // Acquire commit_mutex to perform bundled disk write and state updates
                let _commit_lock = self.commit_mutex.lock().await;
                let mut queue_guard = self.pending_commit_queue.lock().await;
                // INV-LSM-1: No panic in group-commit leader
                let pending_queue = match queue_guard.take() {
                    Some(q) => q,
                    None => {
                        tracing::error!(
                            "Group commit leader: pending_commit_queue unexpectedly missing. \
                             This is a bug — notifying all followers."
                        );
                        return Err(MemFuseError::Internal(
                            "Group commit queue invariant violated".into(),
                        ));
                    }
                };
                drop(queue_guard);

                // INVARIANT-LOCK-3 (Group-Commit Read-Lock ist korrekt und beabsichtigt):
                // Im Happy Path gibt der Group-Commit-Leader `commit_mutex` vor dem physischen
                // Disk-I/O (`wal.append_batch`) frei. `state.read()` schützt die MemTable-Sichtbarkeit.
                // Read-Lock reicht, weil MemTable intern per parking_lot::RwLock granular
                // synchronisiert (memtable.rs) — concurrent puts() sind threadsicher.
                // ⚠ WARNUNG: Diesen Lock NICHT auf state.write() ändern — das würde zu einem
                // verschachtelten Write-Lock führen, der mit flush() deadlocken kann.
                let wal = self.wal.read().await.clone();

                // Combine leader's WAL entries with all follower WAL entries
                let mut all_wal_entries = leader_wal_entries;
                for r in pending_queue.requests.iter() {
                    all_wal_entries.extend(r.wal_entries.clone());
                }

                // LOCK-HANDOFF (P0-A Fix): Acquire truncate_lock BEFORE releasing commit_mutex.
                // This guarantees physical WAL write order matches HMAC sequence allocation order,
                // while dropping commit_mutex during disk I/O to avoid serializing WAL preparation.
                let truncate_guard = wal.truncate_lock.lock().await;
                drop(_commit_lock);

                let append_res = wal
                    .append_batch_locked(all_wal_entries, &truncate_guard)
                    .await;
                drop(truncate_guard);

                if let Err(e) = append_res {
                    // Re-acquire commit_mutex on error path to restore HMAC chain and execute rollback_to_tx_locked
                    let _commit_lock = self.commit_mutex.lock().await;

                    let _ = wal.restore_last_hmac(pending_queue.first_prev_hmac).await;

                    let last_tx = TxId::new(self.last_committed_tx.load(Ordering::Acquire));
                    let commit_guard = CommitGuard {
                        _lock: &_commit_lock,
                    };

                    let rollback_res = self.rollback_to_tx_locked(last_tx, &commit_guard).await;

                    let err_msg = if let Err(ref rollback_err) = rollback_res {
                        tracing::error!(
                            "Failed to execute rollback_to_tx_locked after failed group WAL append: {}",
                            rollback_err
                        );
                        format!(
                            "Fatal double-fault: WAL append failed ({e}) and subsequent rollback failed: {rollback_err}"
                        )
                    } else {
                        format!("Commit failed (at WAL append), WAL rollback executed: {e}")
                    };

                    let mut batch_txs = vec![leader_tx_id];
                    batch_txs.extend(pending_queue.requests.iter().map(|r| r.tx_id));
                    self.cleanup_intent_locks_for_txs(&batch_txs);

                    // Invariant: Every follower sender MUST be notified exactly once, even in double-fault (commit+rollback fail) paths.
                    for r in pending_queue.requests {
                        if r.sender
                            .send(Err(MemFuseError::Storage(err_msg.clone())))
                            .is_err()
                        {
                            tracing::warn!(
                                follower_tx = ?r.tx_id,
                                "Follower dropped receiver during group commit failure notification"
                            );
                        }
                    }

                    return Err(MemFuseError::Storage(err_msg));
                }

                type MemUpdateBatch<'a> = (TxId, &'a [(Vec<u8>, Vec<u8>, u64)]);
                // Group append succeeded: update last_committed_tx and memtable for leader + followers
                let mut all_updates: Vec<MemUpdateBatch> =
                    Vec::with_capacity(1 + pending_queue.requests.len());
                all_updates.push((leader_tx_id, &leader_mem_updates));
                for r in &pending_queue.requests {
                    all_updates.push((r.tx_id, &r.mem_updates));
                }

                // Re-acquire commit_mutex for MemTable update and visibility advancement
                let _commit_lock = self.commit_mutex.lock().await;

                let state = self.state.read().await;
                for (req_tx_id, mem_updates) in all_updates {
                    self.advance_visibility(req_tx_id);
                    self.apply_mem_updates(&state.memtable, mem_updates, req_tx_id);
                }

                let needs_flush = state.memtable.size() > self.config.memtable_size_limit;
                drop(state);
                if needs_flush {
                    if let Err(flush_err) = self.flush().await {
                        tracing::error!("Flush failed after group commit: {}", flush_err);
                    }
                }

                let mut batch_txs = vec![leader_tx_id];
                batch_txs.extend(pending_queue.requests.iter().map(|r| r.tx_id));
                self.cleanup_intent_locks_for_txs(&batch_txs);

                // Invariant: Every follower sender MUST be notified exactly once, even in double-fault (commit+rollback fail) paths.
                for r in pending_queue.requests {
                    if r.sender.send(Ok(())).is_err() {
                        tracing::warn!(
                            follower_tx = ?r.tx_id,
                            "Follower dropped receiver before group commit notification"
                        );
                    }
                }

                Ok(())
            }
        })
    }

    /// # ACID-Garantie
    /// Verwirft uncommitted Operationen einer spezifischen Transaktion aus dem In-Memory TxBuffer.
    ///
    /// **WICHTIGER HINWEIS (ADR-023)**:
    /// Diese Methode hat NUR Wirkung auf Operationen, die noch NICHT via `commit()` physisch
    /// in den WAL geschrieben wurden. Nach einem erfolgreichen `commit()` ist `tx_buffer.drain()`
    /// ausgeführt und der Eintrag im Buffer geleert. Ein Aufruf von `rollback()` NACH `commit()` ist
    /// ein wirkungsloser No-Op. Ein Rückgängigmachen bereits committeter Daten erfordert eine
    /// kompensierende Transaktion (Delete/Tombstone-Eintrag unter neuer TxId) oder `rollback_to_tx()`.
    ///
    /// # Fehler
    /// Gibt `Err` bei internen Puffer-Fehlern zurück.
    ///
    /// # Panics
    /// Panikt nicht in Produktionscode.
    fn rollback<'a>(&'a self, tx_id: TxId) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            self.tx_buffer.discard_kv(tx_id);
            self.cleanup_intent_locks_for_tx(tx_id);
            Ok(())
        })
    }

    /// # ACID-Garantie
    /// Physikalische Truncation des WAL und Zurücksetzen aller Statedaten auf target_tx.
    ///
    /// # Fehler
    /// Gibt `Err` bei I/O- oder Truncate-Fehlern zurück.
    ///
    /// # Panics
    /// Panikt nicht in Produktionscode.
    fn rollback_to_tx<'a>(&'a self, tx_id: TxId) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move { Self::rollback_to_tx(self, tx_id).await })
    }

    fn pin_checkpoint<'a>(&'a self, seq_no: u64) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            self.snapshot_registry.pin(seq_no);
            Ok(())
        })
    }

    fn unpin_checkpoint<'a>(&'a self, seq_no: u64) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            self.snapshot_registry.unpin(seq_no);
            Ok(())
        })
    }

    /// # ACID-Garantie
    /// Atomarer Flush der aktiven MemTable in eine unveränderliche SSTable auf Disk.
    ///
    /// # Fehler
    /// Gibt `Err` zurück, wenn SSTable-Erstellung oder fsync fehlschlägt.
    ///
    /// # Panics
    /// Panikt nicht in Produktionscode.
    fn flush<'a>(&'a self) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            // ── Phase 0: Schnellcheck (Read-Lock, kein I/O) ──────────────────────
            let has_active_memtable = {
                let state = self.state.read().await;
                if state.memtable.is_empty() && state.immutable_memtables.is_empty() {
                    return Ok(());
                }
                !state.memtable.is_empty()
            }; // read lock freigegeben

            // ── Phase 1: Pre-Allokierung der neuen WAL VOR dem Write-Lock ────────
            // WAL-Erstellung (Datei erstellen, Header schreiben) erfolgt außerhalb des State-Locks,
            // um Tokio-Worker-Thread Blockaden bei Disk-Latenz-Spikes zu verhindern (Fix E).
            let new_wal_opt = if has_active_memtable {
                let flush_id = self.flush_counter.fetch_add(1, Ordering::SeqCst);
                let wal_path = self.config.path.join(format!("wal-{:020}.log", flush_id));
                let new_wal =
                    Wal::open_with_key_manager(wal_path, self.key_manager.clone()).await?;
                Some(new_wal)
            } else {
                None
            };

            // ── Phase 2: Atomarer Swap unter Write-Lock ──────────────────────────
            let (to_flush, old_wal_path) = {
                // INVARIANT-LOCK-2 (Atomarer Memtable-Swap):
                // Write-Lock serialisiert diesen Swap atomar gegen alle parallelen commit()-Aufrufe im
                // Single-Commit-Pfad (die ebenfalls state.write() halten). Nach erfolgreichem Swap zeigt
                // `state.memtable` auf einen frischen, leeren Memtable; der alte wird als immutable weitergeführt.
                // Group-Commit-Leader-Pfade halten hier bereits commit_mutex, sodass kein Commit simultan
                // in den neu-swapped Memtable schreibt, bevor dieser korrekt initialisiert ist.
                let mut state = self.state.write().await;
                if state.memtable.is_empty() && state.immutable_memtables.is_empty() {
                    return Ok(());
                }

                let old_wal_path = if !state.memtable.is_empty() {
                    let new_wal = match new_wal_opt {
                        Some(w) => w,
                        None => {
                            let flush_id = self.flush_counter.fetch_add(1, Ordering::SeqCst);
                            let wal_path =
                                self.config.path.join(format!("wal-{:020}.log", flush_id));
                            Wal::open_with_key_manager(wal_path, self.key_manager.clone()).await?
                        }
                    };

                    let old_memtable =
                        std::mem::replace(&mut state.memtable, Arc::new(MemTable::new()));
                    let old_wal = {
                        let mut wal_guard = self.wal.write().await;
                        std::mem::replace(&mut *wal_guard, Arc::new(new_wal))
                    };
                    state.immutable_memtables.push(old_memtable);
                    let path = old_wal.path().to_path_buf();
                    drop(old_wal);
                    Some(path)
                } else {
                    None
                };

                let to_flush = state.immutable_memtables.clone();
                (to_flush, old_wal_path)
            }; // write lock freigegeben

            let count = self.segment_counter.fetch_add(1, Ordering::Relaxed);
            let seq = self.next_seq_no.load(Ordering::Relaxed);
            let sst_path =
                self.config
                    .path
                    .join(format!("sst-{:020}-{:06}.sst", seq, count % 1_000_000));

            // ── Phase 3: Expensive I/O & Atomic Transition ──────────────────────────
            let phase3_res: Result<()> = async {
                let mut builder =
                    SstableBuilder::create_with_key_manager(&sst_path, self.key_manager.clone())
                        .await?;

                let mut map = std::collections::BTreeMap::new();
                for mt in &to_flush {
                    for (k, v, seq, tx) in mt.iter_latest() {
                        map.insert(k, (v, seq, tx));
                    }
                }
                for (k, (v, seq, tx)) in map {
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
                .map_err(|e| {
                    MemFuseError::Storage(format!("SSTable open after flush failed: {}", e))
                })?;

                // === SSTABLE MANIFEST INTEGRATION START ===
                self.manifest
                    .append(&crate::manifest::ManifestEntry::Add {
                        path: sst_path.clone(),
                        max_tx: reader.metadata().max_tx_id,
                    })
                    .await?;
                // === SSTABLE MANIFEST INTEGRATION END ===

                // Atomic transition: remove successfully flushed memtables from immutable memtables and add to SSTables
                let mut state = self.state.write().await;
                let mut sstables = self.sstables.write().await;

                state
                    .immutable_memtables
                    .retain(|mt| !to_flush.iter().any(|tf| Arc::ptr_eq(mt, tf)));

                // last_committed_tx MUSS vor sstables.push() aktualisiert werden — sonst Race-Fenster für parallele Reader, siehe DECISIONS.md ADR-043.
                let sst_max_tx = reader.metadata().max_tx_id;
                self.advance_visibility(TxId::new(sst_max_tx));

                sstables.push(Arc::new(reader));
                sstables.sort_by_key(|sst| sst.metadata().max_seq & !TOMBSTONE_BIT);

                debug_assert!(
                    sstables
                        .windows(2)
                        .all(|w| (w[0].metadata().max_seq & !TOMBSTONE_BIT)
                            <= (w[1].metadata().max_seq & !TOMBSTONE_BIT)),
                    "SSTable list must be sorted by max_seq in ascending order after flush"
                );

                drop(sstables);
                drop(state);

                // Best-effort delete of old WAL (non-critical if it fails, as it will be replayed safely)
                if let Some(ref path) = old_wal_path {
                    if let Err(e) = tokio::fs::remove_file(path).await {
                        tracing::debug!("Could not delete old WAL {:?}: {}", path, e);
                    }
                }

                let bytes_freed: u64 = to_flush.iter().map(|mt| mt.size() as u64).sum();
                self.budget.release_memory(bytes_freed);

                // M-6 FIX: Reset drift counter after successful flush.
                // After a flush, the budget is accurately reflected via release_memory().
                // Drift accumulated during this memtable's lifetime is now irrelevant.
                self.budget_tracking_drift_bytes
                    .store(0, std::sync::atomic::Ordering::Relaxed);

                tracing::info!("Flushed memtable to SSTable: {} bytes", bytes_freed);
                Ok(())
            }
            .await;

            if let Err(ref e) = phase3_res {
                // Cleanup on Phase 3 failure:
                // Retain old memtables in state.immutable_memtables for continued read availability.
                // self.budget.release_memory() is NOT called because memory is still in use.
                if sst_path.exists() {
                    if let Err(rm_err) = tokio::fs::remove_file(&sst_path).await {
                        tracing::warn!(
                            path = ?sst_path,
                            "Failed to remove partial SSTable after flush failure: {rm_err}"
                        );
                    }
                }

                tracing::warn!(
                    "Flush Phase 3 failed; old memtable retained in immutable list for continued read availability. WAL on disk is intact. Error: {e}"
                );
            }

            phase3_res
        })
    }

    fn stats<'a>(&'a self) -> BoxFuture<'a, Result<memfuse_core::StorageStats>> {
        Box::pin(async move {
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
        })
    }

    fn last_seq_no<'a>(&'a self) -> BoxFuture<'a, Result<u64>> {
        Box::pin(async move { Ok(self.next_seq_no.load(Ordering::SeqCst).saturating_sub(1)) })
    }

    fn last_tx_id<'a>(&'a self) -> BoxFuture<'a, Result<TxId>> {
        Box::pin(async move { Ok(TxId::new(self.last_committed_tx.load(Ordering::SeqCst))) })
    }

    fn scan_prefix<'a>(
        &'a self,
        prefix: &'a [u8],
    ) -> BoxFuture<'a, Result<Vec<(Vec<u8>, Vec<u8>)>>> {
        Box::pin(async move { self.scan_prefix_at(prefix, u64::MAX).await })
    }

    /// Scans a prefix with a limit and pagination cursor.
    ///
    /// # Complexity & Bounding Strategy
    /// Reads entries matching `prefix` starting after `cursor` from active MemTable, immutable
    /// MemTables, and SSTables. To avoid materializing all matching entries unbounded into RAM,
    /// each source is scanned only up to a candidate capacity of `limit + 1` entries above `cursor`.
    /// The candidates across sources are then merged in a `BTreeMap`.
    /// - Memory Complexity: O(N * limit) where N is the number of storage sources (SSTables + MemTables).
    /// - Time Complexity: O(N * limit * log(limit)) instead of O(M^2) across paginated calls over M entries.
    // AI-TAG[SMELL][RESOLVED] audit-NC-1/M-5: range_bound ist in scan_prefix_bounded korrekt im Scope deklariert und steuert die Paginierung.
    fn scan_prefix_bounded<'a>(
        &'a self,
        prefix: &'a [u8],
        limit: usize,
        cursor: Option<&'a [u8]>,
    ) -> BoxFuture<'a, Result<(Vec<(Vec<u8>, Vec<u8>)>, Option<Vec<u8>>)>> {
        Box::pin(async move {
            let cur_bytes = cursor.map(Bytes::copy_from_slice);

            let map = self
                .collect_visible_entries(
                    SstableScanMode::Prefix(prefix),
                    |k, _raw_seq, _tx| {
                        if let Some(cb) = cursor {
                            if k <= cb {
                                return false;
                            }
                        }
                        k.starts_with(prefix)
                    },
                    true,
                    None,
                    "scan_prefix_bounded()",
                )
                .await?;

            // Cursor-Bound ableiten für den Paginierungs-Iterator
            let range_bound = match &cur_bytes {
                Some(cb) => std::ops::Bound::Excluded(cb.clone()),
                None => std::ops::Bound::Unbounded,
            };

            let mut results = Vec::new();
            let mut iter = map.range((range_bound, std::ops::Bound::Unbounded));

            for (k, (v, seq)) in iter.by_ref() {
                if (seq & TOMBSTONE_BIT) == 0 {
                    results.push((k.to_vec(), v.to_vec()));
                    if results.len() == limit {
                        break;
                    }
                }
            }

            let next_cursor = if results.len() == limit {
                let mut has_more = false;
                for (_k, (_v, seq)) in iter {
                    if (seq & TOMBSTONE_BIT) == 0 {
                        has_more = true;
                        break;
                    }
                }
                if has_more {
                    results.last().map(|(k, _)| k.clone())
                } else {
                    None
                }
            } else {
                None
            };

            Ok((results, next_cursor))
        })
    }

    // AI-TAG[SMELL][RESOLVED] audit-H-7: last_tx wird in scan_prefix_at nach dem Erwerb von self.state.read() geladen.
    fn scan_prefix_at<'a>(
        &'a self,
        prefix: &'a [u8],
        seq_no: u64,
    ) -> BoxFuture<'a, Result<Vec<(Vec<u8>, Vec<u8>)>>> {
        Box::pin(async move {
            let map = self
                .collect_visible_entries(
                    SstableScanMode::Prefix(prefix),
                    |k, raw_seq, _tx| raw_seq <= seq_no && k.starts_with(prefix),
                    false,
                    None,
                    "scan_prefix_at()",
                )
                .await?;

            let mut results = Vec::with_capacity(map.len());
            for (k, (v, seq)) in map {
                if (seq & TOMBSTONE_BIT) == 0 {
                    results.push((k.to_vec(), v.to_vec()));
                }
            }

            Ok(results)
        })
    }

    fn scan_bounded<'a>(
        &'a self,
        start: std::ops::Bound<&'a [u8]>,
        end: std::ops::Bound<&'a [u8]>,
        limit: usize,
        cursor: Option<&'a [u8]>,
    ) -> BoxFuture<'a, Result<(Vec<(Vec<u8>, Vec<u8>)>, Option<Vec<u8>>)>> {
        Box::pin(async move {
            use std::ops::Bound;

            let effective_start = match cursor {
                Some(c) => Bound::Excluded(c),
                None => start,
            };

            let map = self
                .collect_visible_entries(
                    SstableScanMode::Range(effective_start, end),
                    |k, _raw_seq, _tx| check_in_range(k, effective_start, end),
                    true,
                    None,
                    "scan_bounded()",
                )
                .await?;

            // 4. Apply limit (cursor is already handled via effective_start)
            let mut results = Vec::new();
            let mut iter = map.into_iter();

            for (k, (v, seq)) in iter.by_ref() {
                if (seq & TOMBSTONE_BIT) == 0 {
                    results.push((k.to_vec(), v.to_vec()));
                    if results.len() == limit {
                        break;
                    }
                }
            }

            let next_cursor = if results.len() == limit {
                let mut has_more = false;
                for (_k, (_v, seq)) in iter {
                    if (seq & TOMBSTONE_BIT) == 0 {
                        has_more = true;
                        break;
                    }
                }
                if has_more {
                    results.last().map(|(k, _)| k.clone())
                } else {
                    None
                }
            } else {
                None
            };

            Ok((results, next_cursor))
        })
    }

    fn scan<'a>(
        &'a self,
        start: std::ops::Bound<&'a [u8]>,
        end: std::ops::Bound<&'a [u8]>,
        limit: Option<usize>,
    ) -> BoxFuture<'a, Result<Vec<(Vec<u8>, Vec<u8>)>>> {
        Box::pin(async move {
            let map = self
                .collect_visible_entries(
                    SstableScanMode::Range(start, end),
                    |k, _raw_seq, _tx| check_in_range(k, start, end),
                    false,
                    limit,
                    "scan()",
                )
                .await?;

            // 4. Filter tombstones and apply limit
            let mut results = Vec::new();
            for (k, (v, seq)) in map {
                if (seq & TOMBSTONE_BIT) == 0 {
                    results.push((k.to_vec(), v.to_vec()));
                    if let Some(lim) = limit {
                        if results.len() == lim {
                            break;
                        }
                    }
                }
            }

            Ok(results)
        })
    }
}
