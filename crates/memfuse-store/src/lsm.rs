//! LSM-Tree (Log-Structured Merge-Tree) storage engine.
// FILE-CONTEXT
// STAND: 2026-08-30T21:49:55Z (SESSION: 283abf0f)
// ZWECK: LSM-Tree-Implementierung (MemTable + SSTable + Compaction)
// INVARIANTEN: Compaction darf keine Daten verlieren; WAL-Replay vor MemTable-Aufbau
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
//! `commit_mutex` serializes commits, ensuring that sequence number allocation, WAL logging,
//! and MemTable updates are strictly atomic and sequential, preventing snapshot inversion.
//!
//! ## Lock Hierarchy & Concurrency Control
//! To prevent deadlocks, locks across the LSM storage engine must be acquired in the following order:
//! 1. `commit_mutex` (`tokio::sync::Mutex<()>`) - Acquired during commit, rollback_to_tx, and state mutations.
//! 2. `state` write lock (`tokio::sync::RwLock<LsmState>`) - Protects active/immutable memtable pointers & WAL.
//! 3. `sstables` write lock (`tokio::sync::RwLock<Vec<Arc<SstableReader>>>`) - Protects SSTable set.
//!    Read locks on `state` and `sstables` may be acquired concurrently without holding `commit_mutex`.

use crate::compaction::{CompactionConfig, CompactionEngine};
use crate::memtable::MemTable;
use crate::sstable::{create_block_cache, BlockCache, SstableBuilder, SstableReader};
use crate::wal::{Wal, WalOp};
use bytes::Bytes;
use memfuse_core::{
    BoxFuture, DocId, IndexOp, MemFuseError, ResourceBudget, ResourceTracker, Result,
    SnapshotRegistry, StorageEngine, TxBuffer, TxId, TOMBSTONE_BIT,
};
use memfuse_crypto::crypto::KeyManager;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// Maximum key size allowed for LSM operations (1MB).
pub const MAX_KEY_SIZE: usize = 1_048_576;

/// Maximum value size allowed for LSM operations (128MB).
pub const MAX_VALUE_SIZE: usize = 134_217_728;

/// Maximum batch size for `delete_many` operations (10,000 items).
pub const MAX_BATCH_SIZE: usize = 10_000;

/// Atomically creates and writes the 32-byte SALT file using a temporary file pattern:
/// tmp file -> fsync -> rename -> parent dir fsync.
async fn write_salt_atomically(salt_path: &std::path::Path, buf: &[u8; 32]) -> Result<()> {
    let parent = salt_path
        .parent()
        .ok_or_else(|| MemFuseError::Storage("Invalid salt path parent".into()))?;

    let pid = std::process::id();
    let rand_val: u64 = rand::random();
    let tmp_path = parent.join(format!("SALT.tmp.{}.{}", pid, rand_val));

    let buf_copy = *buf;
    let write_res: Result<()> = async {
        let file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)
            .await
            .map_err(|e| {
                MemFuseError::Storage(format!("Failed to create temp SALT file: {}", e))
            })?;

        let mut std_file = file.into_std().await;

        tokio::task::spawn_blocking(move || -> Result<()> {
            use std::io::Write;
            std_file
                .write_all(&buf_copy)
                .map_err(|e| MemFuseError::Storage(format!("Failed to write SALT bytes: {}", e)))?;
            std_file.sync_all().map_err(|e| {
                MemFuseError::Storage(format!("Failed to sync temp SALT file: {}", e))
            })?;
            Ok(())
        })
        .await
        .map_err(|e| MemFuseError::Storage(format!("Join error during SALT write: {}", e)))??;

        tokio::fs::rename(&tmp_path, salt_path).await.map_err(|e| {
            MemFuseError::Storage(format!("Failed to rename temp SALT file: {}", e))
        })?;

        crate::util::fsync_parent_dir(salt_path).await?;
        Ok(())
    }
    .await;

    if write_res.is_err() {
        if let Err(e) = tokio::fs::remove_file(&tmp_path).await {
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!("Failed to remove temporary SALT file {:?}: {}", tmp_path, e);
            }
        }
    }

    write_res
}

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
    flush_counter: AtomicU64,
    segment_counter: AtomicU64,
}

impl LsmStorage {
    /// Creates a new LSM storage engine.
    pub async fn new(config: LsmConfig) -> Result<Self> {
        tokio::fs::create_dir_all(&config.path)
            .await
            .map_err(|e| MemFuseError::Storage(format!("Failed to create dir: {}", e)))?;

        // 🛡️ SICHERUNG: Directory FSync (FIND-STO-004)
        crate::util::fsync_parent_dir(&config.path).await?;

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
            write_salt_atomically(&salt_path, &buf).await?;
            buf.to_vec()
        };

        let key_manager = config
            .encryption_passphrase
            .as_ref()
            .map(|p| KeyManager::try_new(p, &salt).map(Arc::new))
            .transpose()?;

        // Discover and sort all WAL files for replay
        let mut max_wal_id: Option<u64> = None;
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
                    let id = ts as u64;
                    max_wal_id = Some(max_wal_id.map_or(id, |m| m.max(id)));
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
                let raw_lsn = *lsn & !TOMBSTONE_BIT;
                if raw_lsn > max_seq {
                    max_seq = raw_lsn;
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
            if let Err(e) = resource_tracker.consume_memory(replayed_size) {
                tracing::warn!(
                    replayed_bytes = replayed_size,
                    "Memory budget tracking nach WAL-Replay fehlgeschlagen: {e}. \
                     Budget-Accounting unpräzise bis zum nächsten Flush."
                );
            }
        }

        let tx_buffer = TxBuffer::new_with_config(16, config.tx_timeout);

        // Load existing SSTables and sort by filename (which includes seq_no)
        let mut sst_files = Vec::new();
        if let Ok(mut entries) = tokio::fs::read_dir(&config.path).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if file_name.ends_with(".tmp")
                    || path.extension().is_some_and(|ext| ext == "tmp")
                    || file_name.starts_with("SALT.tmp.")
                {
                    tracing::warn!("Removing leftover un-renamed temp file: {:?}", path);
                    if let Err(e) = tokio::fs::remove_file(&path).await {
                        tracing::warn!("Failed to remove leftover temp file {:?}: {}", path, e);
                    }
                } else if path.extension().is_some_and(|ext| ext == "sst") {
                    sst_files.push(path);
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
            let raw_sst_max_seq = reader.metadata().max_seq & !TOMBSTONE_BIT;
            if raw_sst_max_seq > max_seq {
                max_seq = raw_sst_max_seq;
            }
            if reader.metadata().max_tx_id > max_tx {
                // Ignore internal TxIds during recovery
                if reader.metadata().max_tx_id < TxId::INTERNAL_BASE {
                    max_tx = reader.metadata().max_tx_id;
                }
            }

            sstables.push(Arc::new(reader));
        }
        // Explicitly sort SSTables by metadata().max_seq for guaranteed read-path ordering
        sstables.sort_by_key(|sst| sst.metadata().max_seq & !TOMBSTONE_BIT);
        let sstables = Arc::new(RwLock::new(sstables));
        let snapshot_registry = Arc::new(SnapshotRegistry::new());

        // Spawn background compaction task
        // COMP-001 — Implementiere CompactionEngine::run_loop.
        // TEST: cargo test -p memfuse-store test_concurrent_reads_during_compaction
        // DONE: Triple-Test grün, keine Deadlocks in tokio::spawn.
        // Cleanup old replayed WAL files except the active WAL
        if wal_files.len() > 1 {
            let active_wal_path = wal.path();
            for (_ts, old_wal_path) in &wal_files[..wal_files.len() - 1] {
                if old_wal_path != active_wal_path {
                    if let Err(e) = tokio::fs::remove_file(old_wal_path).await {
                        tracing::warn!("Failed to remove old WAL file {:?}: {}", old_wal_path, e);
                    } else {
                        tracing::info!("Removed old replayed WAL file: {:?}", old_wal_path);
                    }
                }
            }
        }

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
            flush_counter: AtomicU64::new(max_wal_id.map_or(0, |m| m.saturating_add(1))),
            segment_counter: AtomicU64::new(0),
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

    /// Rolls back the entire storage state to a specific transaction ID.
    /// This is a destructive operation that removes all data after the target TX.
    pub async fn rollback_to_tx(&self, target_tx: TxId) -> Result<()> {
        let _commit_lock = self.commit_mutex.lock().await;
        self.rollback_to_tx_locked(target_tx).await
    }

    /// Internal rollback implementation.
    ///
    /// # Safety / Concurrency Invariant
    /// **MUST ONLY** be called while holding `commit_mutex`. Calling this function without
    /// holding `commit_mutex` violates lock ordering and leads to state corruption and race conditions.
    async fn rollback_to_tx_locked(&self, target_tx: TxId) -> Result<()> {
        let mut state = self.state.write().await;

        // 1. Truncate WAL to the position after target_tx
        let (target_offset, target_hmac) = state.wal.find_tx_offset(target_tx).await?;
        state.wal.truncate(target_offset, target_hmac).await?;

        // 2. Clear current memtable (it might have data > target_tx)
        state.memtable = Arc::new(MemTable::new());
        state.immutable_memtables.clear();

        // 3. Handle SSTables: Remove SSTables that are entirely newer than target_tx,
        // and recompact SSTables that span target_tx to physically delete post-rollback entries.
        let mut sstables_lock = self.sstables.write().await;
        let mut sst_to_remove = Vec::new();
        let mut spanning_sstables = Vec::new();

        sstables_lock.retain(|sst| {
            let meta = sst.metadata();
            if meta.min_tx_id > target_tx.inner() {
                sst_to_remove.push(sst.file_path().to_path_buf());
                false
            } else if meta.min_tx_id <= target_tx.inner() && meta.max_tx_id > target_tx.inner() {
                spanning_sstables.push(Arc::clone(sst));
                false
            } else {
                true
            }
        });

        for spanning in spanning_sstables {
            let mut surviving_entries = Vec::new();
            let mut stream = spanning.stream().await?;
            while let Some((k, v, seq, tx)) = stream.next_entry().await? {
                if tx <= target_tx.inner() {
                    surviving_entries.push((k, v, seq, tx));
                }
            }

            if !surviving_entries.is_empty() {
                let count = self.segment_counter.fetch_add(1, Ordering::Relaxed);
                let seq = self.next_seq_no.load(Ordering::Relaxed);
                let new_sst_path =
                    self.config
                        .path
                        .join(format!("sst-{:020}-{:06}.sst", seq, count % 1_000_000));

                let mut builder = SstableBuilder::create_with_key_manager(
                    &new_sst_path,
                    self.key_manager.clone(),
                )
                .await?;

                for (k, v, seq, tx) in surviving_entries {
                    builder.add(&k, &v, seq, tx).await?;
                }
                builder.finish().await?;

                let new_reader = SstableReader::open_with_key_manager(
                    &new_sst_path,
                    Arc::clone(&self.block_cache),
                    self.key_manager.clone(),
                )
                .await?;

                sstables_lock.push(Arc::new(new_reader));
            }

            sst_to_remove.push(spanning.file_path().to_path_buf());
        }

        sstables_lock.sort_by_key(|sst| sst.metadata().max_seq & !TOMBSTONE_BIT);

        // 4. Update next_seq_no and last_committed_tx
        // Find max_seq from kept SSTables to avoid regressing next_seq_no
        // TOMBSTONE_BIT-Disziplin (DECISIONS.md): Bit 63 darf niemals in next_seq_no einfließen.
        let mut max_seq = 0;
        for sst in sstables_lock.iter() {
            max_seq = max_seq.max(sst.metadata().max_seq & !TOMBSTONE_BIT);
        }
        drop(sstables_lock);

        for path in sst_to_remove {
            tracing::info!("Removing SSTable during rollback: {:?}", path);
            // Best-effort cleanup: do not abort rollback recovery if file removal fails.
            // The SSTable is superseded by restored WAL replay state, so its orphaned presence is safe but wastes disk space.
            if let Err(e) = tokio::fs::remove_file(&path).await {
                tracing::error!(
                    path = ?path,
                    "Orphaned SSTable konnte nicht entfernt werden: {e}. Manuelles Cleanup nötig."
                );
            }
        }

        // 5. Re-populate memtable from truncated WAL
        let entries = state.wal.replay().await?;
        // TOMBSTONE_BIT-Disziplin (DECISIONS.md): Bit 63 darf niemals in next_seq_no einfließen.
        for (seq, entry, _offset) in entries {
            if (seq & !TOMBSTONE_BIT) > max_seq {
                max_seq = seq & !TOMBSTONE_BIT;
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

    /// Evaluates detailed traversal metrics (evaluated_sstables, bloom_passes, range_passes, block_reads, found) for point lookups.
    pub async fn point_lookup_metrics(&self, key: &[u8]) -> (usize, usize, usize, usize, bool) {
        let sstables = self.sstables.read().await;
        let mut total_eval = 0usize;
        let mut total_bloom_pass = 0usize;
        let mut total_range_pass = 0usize;
        let mut total_block_read = 0usize;
        let mut found = false;

        for sst in sstables.iter().rev() {
            total_eval += 1;
            let (b_pass, r_pass, blk_read, k_found) = sst.lookup_metrics(key).await;
            if b_pass {
                total_bloom_pass += 1;
            }
            if r_pass {
                total_range_pass += 1;
            }
            if blk_read {
                total_block_read += 1;
            }
            if k_found {
                found = true;
                break;
            }
        }

        (
            total_eval,
            total_bloom_pass,
            total_range_pass,
            total_block_read,
            found,
        )
    }
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
    fn get<'a>(&'a self, key: &'a [u8]) -> BoxFuture<'a, Result<Option<Vec<u8>>>> {
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
    ) -> BoxFuture<'a, Result<Option<Vec<u8>>>> {
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
                return Ok(Some(val.to_vec()));
            }

            // 2. Immutable MemTables (newest first)
            for mt in state.immutable_memtables.iter().rev() {
                if let Some((val, seq, _tx)) = mt.get_at_seq(key, seq_no, snapshot_tx) {
                    if (seq & TOMBSTONE_BIT) != 0 {
                        return Ok(None);
                    }
                    return Ok(Some(val.to_vec()));
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
                        return Ok(Some(val.to_vec()));
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

            let _commit_lock = self.commit_mutex.lock().await;

            if let Some(is_insert) = self.tx_buffer.staged_status(key) {
                if is_insert {
                    return Ok(false);
                }
            }

            let current_max_seq = self.next_seq_no.load(Ordering::Acquire);
            if self.get_at_seq(key, current_max_seq).await?.is_some() {
                return Ok(false);
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
            )?;

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
                    let hash = blake3::hash(&key);
                    let mut bytes = [0u8; 8];
                    bytes.copy_from_slice(&hash.as_bytes()[..8]);
                    let doc_id = DocId::new(u64::from_le_bytes(bytes));
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
    fn commit<'a>(&'a self, tx_id: TxId) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            self.apply_backpressure().await;
            if !self.budget.has_memory_capacity() {
                return Err(MemFuseError::Storage("Memory budget exceeded (95%)".into()));
            }

            // ANCHOR[ALG-FIX:D6-001] STATUS:DONE (TS:2026-06-01T00:00:00Z) — Snapshot-Inversion bei parallel commit (INV-MVCC-1)
            // FIX: Commit-Mutex serialisiert fetch_add + memtable.put.
            // Ohne Mutex könnte seq=11 vor seq=10 fertig sein → Reader seq=11 sieht Lücke bei 10.
            let _commit_lock = self.commit_mutex.lock().await;

            let ops = self.tx_buffer.drain(tx_id);
            if ops.is_empty() {
                return Ok(());
            }
            let state = self.state.read().await;

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
                        return Err(MemFuseError::InvalidInput(
                            "Unsupported operation type staged in LSM commit".to_string(),
                        ));
                    }
                }
            }

            // --- PHASE 2: Group Commit to WAL ---
            let wal_entries = state.wal.prepare_batch(wal_ops).await?;
            if let Err(e) = state.wal.append_batch(&wal_entries).await {
                // FATAL I/O ERROR: Physical Rollback to last committed transaction state
                drop(state);
                let last_tx = TxId::new(self.last_committed_tx.load(Ordering::Acquire));
                if let Err(rollback_err) = self.rollback_to_tx_locked(last_tx).await {
                    tracing::error!(
                        "Failed to execute rollback_to_tx_locked after failed WAL append: {}",
                        rollback_err
                    );
                }
                return Err(MemFuseError::Storage(format!(
                    "Commit failed (at WAL append), WAL rollback executed: {}",
                    e
                )));
            }

            // INVARIANT (Task D - Commit Ordering):
            // 1. WAL append must succeed FIRST (done in PHASE 2).
            // 2. last_committed_tx.store() must happen AFTER successful WAL append (done here before/during MemTable write).
            // 3. MemTable write happens AFTER WAL append succeeds.
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

            // --- PHASE 3: Apply to MemTable ---
            for (key, value, seq) in mem_updates {
                let entry_size = key.len() + value.len() + 8;
                if let Err(e) = self.budget.consume_memory(entry_size as u64) {
                    tracing::warn!("Memory budget tracking warning during commit: {e}");
                }
                state
                    .memtable
                    .put(Bytes::from(key), Bytes::from(value), seq, tx_id.inner());
            }

            // Check if flush is needed
            if state.memtable.size() > self.config.memtable_size_limit {
                drop(state);
                self.flush().await?;
            }

            Ok(())
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
            self.tx_buffer.discard(tx_id);
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
            {
                let state = self.state.read().await;
                if state.memtable.is_empty() {
                    return Ok(());
                }
            } // read lock freigegeben

            // ── Phase 1: I/O außerhalb jedes Locks ──────────────────────────────
            let flush_id = self.flush_counter.fetch_add(1, Ordering::SeqCst);
            let wal_path = self.config.path.join(format!("wal-{}.log", flush_id));
            // WAL wird HIER erstellt — kein Lock gehalten
            let new_wal = Wal::open_with_key_manager(wal_path, self.key_manager.clone()).await?;

            // ── Phase 2: Atomarer In-Memory-Swap (Write-Lock, nur Pointer-Ops) ───
            let (old_memtable, old_wal_path) = {
                let mut state = self.state.write().await;
                if state.memtable.is_empty() {
                    // Zweiter Flush hat memtable bereits geleert — new_wal verwerfen
                    // WAL-Datei aufräumen (best-effort)
                    drop(state);
                    let _ = tokio::fs::remove_file(
                        &self.config.path.join(format!("wal-{}.log", flush_id)),
                    )
                    .await;
                    return Ok(());
                }
                let old_memtable =
                    std::mem::replace(&mut state.memtable, Arc::new(MemTable::new()));
                let old_wal = std::mem::replace(&mut state.wal, new_wal);
                state.immutable_memtables.push(old_memtable.clone());

                // ANCHOR[ALG-FIX:D1-011] STATUS:DONE (TS:2026-06-01T00:00:00Z) — Stale WAL-Dateien löschen nach Flush
                // Ohne Cleanup wächst die Disk-Usage unbegrenzt (eine WAL pro Flush).
                let old_wal_path = old_wal.path().to_path_buf();
                drop(old_wal);
                (old_memtable, old_wal_path)
            }; // write lock freigegeben

            let sst_path = {
                let count = self.segment_counter.fetch_add(1, Ordering::Relaxed);
                let seq = self.next_seq_no.load(Ordering::Relaxed);
                self.config
                    .path
                    .join(format!("sst-{:020}-{:06}.sst", seq, count % 1_000_000))
            };
            let mut builder =
                SstableBuilder::create_with_key_manager(&sst_path, self.key_manager.clone())
                    .await?;

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
            .map_err(|e| {
                MemFuseError::Storage(format!("SSTable open after flush failed: {}", e))
            })?;

            // Atomic transition: remove from immutable memtables and add to SSTables
            let mut state = self.state.write().await;
            let mut sstables = self.sstables.write().await;

            state
                .immutable_memtables
                .retain(|mt| !Arc::ptr_eq(mt, &old_memtable));

            // last_committed_tx MUSS vor sstables.push() aktualisiert werden — sonst Race-Fenster für parallele Reader, siehe DECISIONS.md ADR-043.
            let sst_max_tx = reader.metadata().max_tx_id;
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

            sstables.push(Arc::new(reader));
            sstables.sort_by_key(|sst| sst.metadata().max_seq & !TOMBSTONE_BIT);

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

    fn scan_prefix_bounded<'a>(
        &'a self,
        prefix: &'a [u8],
        limit: usize,
        cursor: Option<&'a [u8]>,
    ) -> BoxFuture<'a, Result<(Vec<(Vec<u8>, Vec<u8>)>, Option<Vec<u8>>)>> {
        Box::pin(async move {
            let last_tx = self.last_committed_tx.load(Ordering::Acquire);
            let mut map: std::collections::BTreeMap<Bytes, (Bytes, u64)> =
                std::collections::BTreeMap::new();
            let state = self.state.read().await;
            let sstables = self.sstables.read().await;

            // Collect from SSTables
            for sst in sstables.iter() {
                let first = sst.first_key();
                let last = sst.last_key();
                if !first.is_empty() && !last.is_empty() {
                    if prefix > last.as_ref() {
                        continue;
                    }
                    let mut prefix_end = prefix.to_vec();
                    if let Some(last_byte) = prefix_end.last_mut() {
                        if let Some(next_byte) = last_byte.checked_add(1) {
                            *last_byte = next_byte;
                            if first.as_ref() >= prefix_end.as_slice() {
                                continue;
                            }
                        }
                    }
                }

                let entries = sst.scan_prefix(prefix).await?;
                for (k, v, seq, tx) in entries {
                    if tx <= last_tx || tx >= TxId::INTERNAL_BASE {
                        let entry = map.entry(k).or_insert_with(|| (v.clone(), seq));
                        if (seq & !TOMBSTONE_BIT) > (entry.1 & !TOMBSTONE_BIT) {
                            *entry = (v, seq);
                        }
                    }
                }
            }

            // Collect from immutable memtables
            for mt in &state.immutable_memtables {
                for (k, v, seq, tx) in mt.iter() {
                    if k.starts_with(prefix) && (tx <= last_tx || tx >= TxId::INTERNAL_BASE) {
                        let entry = map.entry(k.clone()).or_insert_with(|| (v.clone(), seq));
                        if (seq & !TOMBSTONE_BIT) > (entry.1 & !TOMBSTONE_BIT) {
                            *entry = (v.clone(), seq);
                        }
                    }
                }
            }

            // Collect from active memtable
            for (k, v, seq, tx) in state.memtable.iter() {
                if k.starts_with(prefix) && (tx <= last_tx || tx >= TxId::INTERNAL_BASE) {
                    let entry = map.entry(k.clone()).or_insert_with(|| (v.clone(), seq));
                    if (seq & !TOMBSTONE_BIT) > (entry.1 & !TOMBSTONE_BIT) {
                        *entry = (v.clone(), seq);
                    }
                }
            }

            let range_bound = if let Some(cur) = cursor {
                std::ops::Bound::Excluded(Bytes::copy_from_slice(cur))
            } else {
                std::ops::Bound::Unbounded
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
                // Check if there are remaining valid entries in the iterator
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

    fn scan_prefix_at<'a>(
        &'a self,
        prefix: &'a [u8],
        seq_no: u64,
    ) -> BoxFuture<'a, Result<Vec<(Vec<u8>, Vec<u8>)>>> {
        Box::pin(async move {
            // INVARIANT (Task C - Single snapshot boundary):
            // last_committed_tx is loaded EXACTLY ONCE at start and passed through for snapshot isolation.
            let last_tx = self.last_committed_tx.load(Ordering::Acquire);
            let mut map: std::collections::BTreeMap<Bytes, (Bytes, u64)> =
                std::collections::BTreeMap::new();
            let state = self.state.read().await;
            let sstables = self.sstables.read().await;

            // Collect from SSTables
            for sst in sstables.iter() {
                let first = sst.first_key();
                let last = sst.last_key();
                if !first.is_empty() && !last.is_empty() {
                    if prefix > last.as_ref() {
                        continue;
                    }
                    let mut prefix_end = prefix.to_vec();
                    if let Some(last_byte) = prefix_end.last_mut() {
                        if let Some(next_byte) = last_byte.checked_add(1) {
                            *last_byte = next_byte;
                            if first.as_ref() >= prefix_end.as_slice() {
                                continue;
                            }
                        }
                    }
                }

                let entries = sst.scan_prefix(prefix).await?;
                for (k, v, seq, tx) in entries {
                    let raw_seq = seq & !TOMBSTONE_BIT;
                    if raw_seq <= seq_no && (tx <= last_tx || tx >= TxId::INTERNAL_BASE) {
                        let entry = map.entry(k).or_insert_with(|| (v.clone(), seq));
                        if (seq & !TOMBSTONE_BIT) > (entry.1 & !TOMBSTONE_BIT) {
                            *entry = (v, seq);
                        }
                    }
                }
            }

            // Collect from immutable memtables
            for mt in &state.immutable_memtables {
                for (k, v, seq, tx) in mt.iter() {
                    let raw_seq = seq & !TOMBSTONE_BIT;
                    if k.starts_with(prefix)
                        && raw_seq <= seq_no
                        && (tx <= last_tx || tx >= TxId::INTERNAL_BASE)
                    {
                        let entry = map.entry(k.clone()).or_insert_with(|| (v.clone(), seq));
                        if (seq & !TOMBSTONE_BIT) > (entry.1 & !TOMBSTONE_BIT) {
                            *entry = (v.clone(), seq);
                        }
                    }
                }
            }

            // Collect from active memtable
            for (k, v, seq, tx) in state.memtable.iter() {
                let raw_seq = seq & !TOMBSTONE_BIT;
                if k.starts_with(prefix)
                    && raw_seq <= seq_no
                    && (tx <= last_tx || tx >= TxId::INTERNAL_BASE)
                {
                    let entry = map.entry(k.clone()).or_insert_with(|| (v.clone(), seq));
                    if (seq & !TOMBSTONE_BIT) > (entry.1 & !TOMBSTONE_BIT) {
                        *entry = (v.clone(), seq);
                    }
                }
            }

            let mut results = Vec::with_capacity(map.len());
            for (k, (v, seq)) in map {
                if (seq & TOMBSTONE_BIT) == 0 {
                    results.push((k.to_vec(), v.to_vec()));
                }
            }

            Ok(results)
        })
    }

    fn scan<'a>(
        &'a self,
        start: std::ops::Bound<&'a [u8]>,
        end: std::ops::Bound<&'a [u8]>,
    ) -> BoxFuture<'a, Result<Vec<(Vec<u8>, Vec<u8>)>>> {
        Box::pin(async move {
            use std::ops::Bound;

            let last_tx = self.last_committed_tx.load(Ordering::Acquire);
            let mut map = std::collections::BTreeMap::<Vec<u8>, (Vec<u8>, u64)>::new();
            let state = self.state.read().await;
            let sstables = self.sstables.read().await;

            // 1. SSTables (filtered by visibility tx <= last_tx)
            for sst in sstables.iter() {
                let entries = sst.scan_range(start.map(|s| s), end.map(|e| e)).await?;
                for (k, v, seq, tx) in entries {
                    if tx <= last_tx || tx >= TxId::INTERNAL_BASE {
                        let entry = map.entry(k.to_vec()).or_insert((v.to_vec(), seq));
                        if (seq & !TOMBSTONE_BIT) > (entry.1 & !TOMBSTONE_BIT) {
                            *entry = (v.to_vec(), seq);
                        }
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

            // 3. Active memtable
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
                    let entry = map.entry(k.to_vec()).or_insert((v.to_vec(), seq));
                    if (seq & !TOMBSTONE_BIT) > (entry.1 & !TOMBSTONE_BIT) {
                        *entry = (v.to_vec(), seq);
                    }
                }
            }

            // 4. Filter tombstones
            let results = map
                .into_iter()
                .filter(|(_, (_, seq))| (seq & TOMBSTONE_BIT) == 0)
                .map(|(k, (v, _))| (k, v))
                .collect();

            Ok(results)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn test_storage() -> (LsmStorage, TempDir) {
        let tmp = TempDir::new().expect("temp dir"); // expect
        let config = LsmConfig {
            path: tmp.path().to_path_buf(),
            memtable_size_limit: 1024 * 1024,
            max_ram_mb: 64,
            tx_timeout: Duration::from_secs(60),
            compaction: CompactionConfig::default(),
            encryption_passphrase: None,
        };
        let storage = LsmStorage::new(config).await.expect("create storage"); // expect
        (storage, tmp)
    }

    #[tokio::test]
    async fn test_put_get_roundtrip() {
        let (storage, _tmp) = test_storage().await;
        let tx = TxId::new(1);

        storage.put(tx, b"hello", b"world").await.expect("put"); // expect
        storage.commit(tx).await.expect("commit"); // expect

        let val = storage.get(b"hello").await.expect("get"); // expect
        assert_eq!(val, Some(b"world".to_vec()));
    }

    #[tokio::test]
    async fn test_delete() {
        let (storage, _tmp) = test_storage().await;
        let tx1 = TxId::new(1);

        storage.put(tx1, b"key", b"val").await.expect("put"); // expect
        storage.commit(tx1).await.expect("commit"); // expect

        let tx2 = TxId::new(2);
        storage.delete(tx2, b"key").await.expect("delete"); // expect
        storage.commit(tx2).await.expect("commit"); // expect

        let val = storage.get(b"key").await.expect("get"); // expect
        assert_eq!(val, None);
    }

    #[tokio::test]
    async fn test_delete_prefix_removes_all_matching_keys() {
        let (storage, _tmp) = test_storage().await;
        let tx1 = TxId::new(1);

        // 1. Mehrere Keys mit gemeinsamem Prefix "test:" einfügen
        storage.put(tx1, b"test:1", b"val1").await.unwrap(); // unwrap
        storage.put(tx1, b"test:2", b"val2").await.unwrap(); // unwrap
        storage.put(tx1, b"test:3", b"val3").await.unwrap(); // unwrap
        storage.put(tx1, b"other:1", b"val4").await.unwrap(); // unwrap
        storage.commit(tx1).await.unwrap(); // unwrap

        // 2. delete_prefix("test:") aufrufen in tx2
        let tx2 = TxId::new(2);
        let deleted = storage.delete_prefix(tx2, b"test:").await.unwrap(); // unwrap
        assert_eq!(deleted, 3);
        storage.commit(tx2).await.unwrap(); // unwrap

        // 3. Prüfen: alle "test:*"-Keys sind weg, andere Keys bleiben unberührt
        assert_eq!(storage.get(b"test:1").await.unwrap(), None); // unwrap
        assert_eq!(storage.get(b"test:2").await.unwrap(), None); // unwrap
        assert_eq!(storage.get(b"test:3").await.unwrap(), None); // unwrap
        assert_eq!(
            storage.get(b"other:1").await.unwrap(), // unwrap
            Some(b"val4".to_vec())
        );
    }

    #[tokio::test]
    async fn test_delete_prefix_batch_single_tx_buffer_lock() {
        // Verify that delete_prefix stages all ops atomically:
        // after the call, exactly N ops must be in the tx_buffer for tx_id,
        // not scattered across N separate lock acquisitions.
        let dir = tempfile::tempdir().unwrap(); // unwrap
        let storage = LsmStorage::new(LsmConfig {
            path: dir.path().to_path_buf(),
            ..Default::default()
        })
        .await
        .unwrap(); // unwrap

        let tx1 = TxId::new(1);
        for i in 0..10u32 {
            storage
                .put(tx1, format!("pfx:key{}", i).as_bytes(), b"val")
                .await
                .unwrap(); // unwrap
        }
        storage.commit(tx1).await.unwrap(); // unwrap
        storage.flush().await.unwrap(); // unwrap

        let tx2 = TxId::new(2);
        let deleted = storage.delete_prefix(tx2, b"pfx:").await.unwrap(); // unwrap
        assert_eq!(deleted, 10);

        // Commit and verify all keys are gone
        storage.commit(tx2).await.unwrap(); // unwrap
        let remaining = storage.scan_prefix(b"pfx:").await.unwrap(); // unwrap
        assert!(remaining.is_empty(), "All prefixed keys must be deleted");
    }

    #[tokio::test]
    async fn test_lsm_storage_delete_many_uses_single_batch() {
        let (storage, _tmp) = test_storage().await;
        let tx1 = TxId::new(1);

        let keys_to_delete: Vec<Vec<u8>> = (0..50)
            .map(|i| format!("batch_key_{i}").into_bytes())
            .collect();

        for key in &keys_to_delete {
            storage.put(tx1, key, b"value").await.unwrap(); // unwrap
        }
        storage.commit(tx1).await.unwrap(); // unwrap

        let tx2 = TxId::new(2);
        let count = storage
            .delete_many(tx2, keys_to_delete.clone())
            .await
            .unwrap(); // unwrap
        assert_eq!(count, 50);

        // Verify that stage_many inserted all 50 delete operations into tx_buffer for tx2 atomically
        let staged_ops = storage.tx_buffer.get_ops(tx2).expect("ops staged"); // expect
        assert_eq!(staged_ops.len(), 50);

        storage.commit(tx2).await.unwrap(); // unwrap
        for key in &keys_to_delete {
            assert_eq!(storage.get(key).await.unwrap(), None); // unwrap
        }
    }

    #[tokio::test]
    async fn test_rollback() {
        let (storage, _tmp) = test_storage().await;
        let tx = TxId::new(1);

        storage.put(tx, b"key", b"val").await.expect("put"); // expect
        storage.rollback(tx).await.expect("rollback"); // expect

        let val = storage.get(b"key").await.expect("get"); // expect
        assert_eq!(val, None);
    }

    #[tokio::test]
    async fn test_get_nonexistent() {
        let (storage, _tmp) = test_storage().await;
        let val = storage.get(b"nonexistent").await.expect("get"); // expect
        assert_eq!(val, None);
    }

    #[tokio::test]
    async fn test_overwrite() {
        let (storage, _tmp) = test_storage().await;

        let tx1 = TxId::new(1);
        storage.put(tx1, b"key", b"val1").await.expect("put1"); // expect
        storage.commit(tx1).await.expect("commit1"); // expect

        let tx2 = TxId::new(2);
        storage.put(tx2, b"key", b"val2").await.expect("put2"); // expect
        storage.commit(tx2).await.expect("commit2"); // expect

        let val = storage.get(b"key").await.expect("get"); // expect
        assert_eq!(val, Some(b"val2".to_vec()));
    }

    #[tokio::test]
    async fn test_sstable_ordering_after_consecutive_flushes() {
        let (storage, _tmp) = test_storage().await;

        for i in 1..=10u64 {
            let tx = TxId::new(i);
            let val = format!("val-{}", i);
            storage
                .put(tx, b"seq_key", val.as_bytes())
                .await
                .expect("put"); // expect
            storage.commit(tx).await.expect("commit"); // expect
            storage.force_flush().await.expect("flush"); // expect

            let current_val = storage.get(b"seq_key").await.expect("get"); // expect
            assert_eq!(
                current_val,
                Some(val.into_bytes()),
                "After flush {}, get must return latest value",
                i
            );
        }

        let final_val = storage.get(b"seq_key").await.expect("final get"); // expect
        assert_eq!(final_val, Some(b"val-10".to_vec()));
    }

    #[tokio::test]
    async fn test_flush_creates_sstable() {
        let tmp = TempDir::new().expect("temp dir"); // expect
        let config = LsmConfig {
            path: tmp.path().to_path_buf(),
            memtable_size_limit: 64, // Tiny limit to trigger flush easily
            max_ram_mb: 64,
            tx_timeout: Duration::from_secs(60),
            compaction: CompactionConfig::default(),
            encryption_passphrase: None,
        };
        let storage = LsmStorage::new(config).await.expect("create storage"); // expect

        // Insert enough data to exceed the tiny memtable limit
        let tx = TxId::new(1);
        for i in 0..10u8 {
            let key = format!("key-{:03}", i);
            let val = format!("value-{:03}", i);
            storage
                .put(tx, key.as_bytes(), val.as_bytes())
                .await
                .expect("put"); // expect
        }
        storage.commit(tx).await.expect("commit"); // expect

        // Verify data is still readable (from SSTable after flush)
        for i in 0..10u8 {
            let key = format!("key-{:03}", i);
            let expected = format!("value-{:03}", i);
            let val = storage.get(key.as_bytes()).await.expect("get"); // expect
            assert_eq!(
                val,
                Some(expected.into_bytes()),
                "key {} missing after flush",
                key
            );
        }

        // Verify SSTable file(s) were created
        let stats = storage.stats().await.expect("stats"); // expect
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
            storage.put(tx, &key, &val).await.expect("put"); // expect
        }
        storage.commit(tx).await.expect("commit"); // expect

        // Scan [c, g] inclusive
        use std::ops::Bound;
        let results = storage
            .scan(Bound::Included(b"c"), Bound::Included(b"g"))
            .await
            .expect("scan"); // expect
        assert_eq!(results.len(), 5); // c, d, e, f, g
        assert_eq!(results[0].0, b"c");
        assert_eq!(results[4].0, b"g");

        // Scan (c, g) exclusive
        let results = storage
            .scan(Bound::Excluded(b"c"), Bound::Excluded(b"g"))
            .await
            .expect("scan"); // expect
        assert_eq!(results.len(), 3); // d, e, f

        // Scan unbounded start to d inclusive
        let results = storage
            .scan(Bound::Unbounded, Bound::Included(b"d"))
            .await
            .expect("scan"); // expect
        assert_eq!(results.len(), 4); // a, b, c, d

        // Scan with deleted key
        let tx2 = TxId::new(2);
        storage.delete(tx2, b"e").await.expect("delete"); // expect
        storage.commit(tx2).await.expect("commit"); // expect

        let results = storage
            .scan(Bound::Included(b"d"), Bound::Included(b"f"))
            .await
            .expect("scan"); // expect
        assert_eq!(results.len(), 2); // d, f (e deleted)
    }

    #[tokio::test]
    async fn test_lsm_rollback_persistence() {
        let tmp = TempDir::new().expect("temp dir"); // expect
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
                .expect("create storage"); // expect

            let tx1 = TxId::new(1);
            storage.put(tx1, b"k1", b"v1").await.unwrap(); // unwrap
            storage.commit(tx1).await.unwrap(); // unwrap

            let tx2 = TxId::new(2);
            storage.put(tx2, b"k2", b"v2").await.unwrap(); // unwrap
            storage.commit(tx2).await.unwrap(); // unwrap

            // Verify both exist
            assert_eq!(storage.get(b"k1").await.unwrap(), Some(b"v1".to_vec())); // unwrap
            assert_eq!(storage.get(b"k2").await.unwrap(), Some(b"v2".to_vec())); // unwrap

            // Rollback to Tx1
            storage.rollback_to_tx(tx1).await.expect("rollback"); // expect

            assert_eq!(storage.get(b"k1").await.unwrap(), Some(b"v1".to_vec())); // unwrap
            assert_eq!(storage.get(b"k2").await.unwrap(), None); // unwrap
        }

        // Restart storage
        {
            let storage = LsmStorage::new(config).await.expect("restart storage"); // expect
            assert_eq!(storage.get(b"k1").await.unwrap(), Some(b"v1".to_vec())); // unwrap
            assert_eq!(
                storage.get(b"k2").await.unwrap(), // unwrap
                None,
                "k2 should NOT be replayed after rollback"
            );

            // Verify we can still append new transactions after rollback
            let tx3 = TxId::new(3);
            storage.put(tx3, b"k3", b"v3").await.unwrap(); // unwrap
            storage.commit(tx3).await.unwrap(); // unwrap
            assert_eq!(storage.get(b"k3").await.unwrap(), Some(b"v3".to_vec()));
            // unwrap
            // unwrap
            // unwrap
        }
    }
    #[tokio::test]
    async fn test_rollback_with_sstables() {
        let tmp = TempDir::new().expect("temp dir"); // expect
        let config = LsmConfig {
            path: tmp.path().to_path_buf(),
            memtable_size_limit: 1024 * 1024,
            max_ram_mb: 64,
            tx_timeout: Duration::from_secs(60),
            compaction: CompactionConfig::default(),
            encryption_passphrase: None,
        };
        let storage = LsmStorage::new(config).await.expect("create storage"); // expect

        // 1. Insert data for TX 1, TX 2
        let tx1 = TxId::new(1);
        storage.put(tx1, b"k1", b"v1").await.unwrap(); // unwrap
        storage.commit(tx1).await.unwrap(); // unwrap

        let tx2 = TxId::new(2);
        storage.put(tx2, b"k2", b"v2").await.unwrap(); // unwrap
        storage.commit(tx2).await.unwrap(); // unwrap

        // 2. Flush (SSTable 1 contains TX 1, 2)
        storage.force_flush().await.unwrap(); // unwrap

        // 3. Insert data for TX 3, TX 4
        let tx3 = TxId::new(3);
        storage.put(tx3, b"k3", b"v3").await.unwrap(); // unwrap
        storage.commit(tx3).await.unwrap(); // unwrap

        let tx4 = TxId::new(4);
        storage.put(tx4, b"k4", b"v4").await.unwrap(); // unwrap
        storage.commit(tx4).await.unwrap(); // unwrap

        // 4. Flush (SSTable 2 contains TX 3, 4)
        storage.force_flush().await.unwrap(); // unwrap

        {
            let sstables = storage.sstables.read().await;
            assert_eq!(sstables.len(), 2);
        }

        // 5. Rollback to TX 2
        storage.rollback_to_tx(tx2).await.expect("rollback"); // expect

        // 6. Verify SSTable 2 is gone, 7. Verify SSTable 1 is still there.
        {
            let sstables = storage.sstables.read().await;
            assert_eq!(sstables.len(), 1, "SSTable 2 should be deleted");
            assert_eq!(sstables[0].metadata().max_tx_id, 2);
        }

        assert_eq!(storage.get(b"k1").await.unwrap(), Some(b"v1".to_vec())); // unwrap
        let val2 = storage.get(b"k2").await.unwrap(); // unwrap
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
        assert_eq!(storage.get(b"k3").await.unwrap(), None); // unwrap
        assert_eq!(storage.get(b"k4").await.unwrap(), None); // unwrap
    }

    #[tokio::test]
    async fn test_rollback_recompacts_spanning_sstable() {
        let tmp = TempDir::new().expect("temp dir"); // expect
        let config = LsmConfig {
            path: tmp.path().to_path_buf(),
            memtable_size_limit: 1024 * 1024,
            max_ram_mb: 64,
            tx_timeout: Duration::from_secs(60),
            compaction: CompactionConfig::default(),
            encryption_passphrase: None,
        };
        let storage = LsmStorage::new(config).await.expect("create storage"); // expect

        // 1. Write entries with tx_id 1..=10 and flush to an SSTable
        for i in 1..=10u64 {
            let tx = TxId::new(i);
            let key = format!("k{:02}", i);
            let val = format!("v{:02}", i);
            storage
                .put(tx, key.as_bytes(), val.as_bytes())
                .await
                .unwrap(); // unwrap
            storage.commit(tx).await.unwrap(); // unwrap
        }
        storage.force_flush().await.unwrap(); // unwrap

        // Ensure we have 1 SSTable spanning tx 1..10
        {
            let sstables = storage.sstables.read().await;
            assert_eq!(sstables.len(), 1);
            assert_eq!(sstables[0].metadata().min_tx_id, 1);
            assert_eq!(sstables[0].metadata().max_tx_id, 10);
        }

        // 2. Call rollback_to_tx(TxId::new(5))
        storage
            .rollback_to_tx(TxId::new(5))
            .await
            .expect("rollback"); // expect

        // 3. Inspect SSTable on disk: entry count should be 5 and max_tx_id <= 5
        {
            let sstables = storage.sstables.read().await;
            assert_eq!(
                sstables.len(),
                1,
                "Spanning SSTable should be recompacted into 1 new SSTable"
            );
            assert_eq!(sstables[0].metadata().max_tx_id, 5);

            let mut count = 0;
            let mut stream = sstables[0].stream().await.unwrap(); // unwrap
            while let Some((_k, _v, _seq, tx)) = stream.next_entry().await.unwrap() {
                // unwrap
                // unwrap
                assert!(
                    tx <= 5,
                    "SSTable on disk must not contain entries with tx_id > 5"
                );
                count += 1;
            }
            assert_eq!(
                count, 5,
                "Surviving on-disk entry count must equal exactly 5"
            );
        }

        // 4. Assert entries <= 5 are readable and > 5 are not
        for i in 1..=5u64 {
            let key = format!("k{:02}", i);
            let expected = format!("v{:02}", i);
            let val = storage.get(key.as_bytes()).await.unwrap(); // unwrap
            assert_eq!(val, Some(expected.into_bytes()));
        }

        for i in 6..=10u64 {
            let key = format!("k{:02}", i);
            let val = storage.get(key.as_bytes()).await.unwrap(); // unwrap
            assert_eq!(val, None);
        }
    }

    #[tokio::test]
    async fn test_rollback_drops_sstable_fully_stale_after_recompaction() {
        let tmp = TempDir::new().expect("temp dir"); // expect
        let config = LsmConfig {
            path: tmp.path().to_path_buf(),
            memtable_size_limit: 1024 * 1024,
            max_ram_mb: 64,
            tx_timeout: Duration::from_secs(60),
            compaction: CompactionConfig::default(),
            encryption_passphrase: None,
        };
        let storage = LsmStorage::new(config).await.expect("create storage"); // expect

        // 1. Write entries for tx 10..=15 and flush
        for i in 10..=15u64 {
            let tx = TxId::new(i);
            let key = format!("k{:02}", i);
            let val = format!("v{:02}", i);
            storage
                .put(tx, key.as_bytes(), val.as_bytes())
                .await
                .unwrap(); // unwrap
            storage.commit(tx).await.unwrap(); // unwrap
        }
        storage.force_flush().await.unwrap(); // unwrap

        // 2. Rollback to TX 5 (all entries in SSTable are > 5)
        storage
            .rollback_to_tx(TxId::new(5))
            .await
            .expect("rollback"); // expect

        // 3. Verify SSTable is completely dropped
        {
            let sstables = storage.sstables.read().await;
            assert!(sstables.is_empty(), "Fully stale SSTable must be dropped");
        }
    }

    #[tokio::test]
    async fn test_pin_unpin_checkpoint_prevents_gc() {
        let tmp = TempDir::new().expect("temp dir"); // expect
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
            .expect("create storage"); // expect

        // 1. Insert and commit data
        let tx1 = TxId::new(1);
        storage.put(tx1, b"key1", b"val1").await.unwrap(); // unwrap
        storage.commit(tx1).await.unwrap(); // unwrap
        let seq1 = storage.last_seq_no().await.unwrap(); // unwrap

        // 2. Pin seq1
        storage.pin_checkpoint(seq1).await.expect("pin"); // expect

        // 3. Delete key1 and commit
        let tx2 = TxId::new(2);
        storage.delete(tx2, b"key1").await.unwrap(); // unwrap
        storage.commit(tx2).await.unwrap(); // unwrap

        // 4. Force flush and compaction
        storage.force_flush().await.unwrap(); // unwrap

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
            .expect("compact"); // expect

        // 5. Verify min_active_seqno is correct
        assert_eq!(storage.snapshot_registry.min_active_seqno(), seq1);

        // 6. Unpin
        storage.unpin_checkpoint(seq1).await.expect("unpin"); // expect
        assert_eq!(storage.snapshot_registry.min_active_seqno(), u64::MAX);

        // 7. Compact again
        engine
            .maybe_compact(&storage.sstables, &storage.config.path)
            .await
            .unwrap(); // unwrap
    }

    #[tokio::test]
    async fn test_wal_survives_process_restart() {
        let tmp = TempDir::new().expect("temp dir"); // expect
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
                .expect("create storage"); // expect
            let tx = TxId::new(1);
            storage
                .put(tx, b"persistent_key", b"persistent_val")
                .await
                .expect("put"); // expect
            storage.commit(tx).await.expect("commit"); // expect
        } // drop storage instance

        {
            let storage = LsmStorage::new(config).await.expect("reopen storage"); // expect
            let val = storage.get(b"persistent_key").await.expect("get"); // expect
            assert_eq!(val, Some(b"persistent_val".to_vec()));
        }
    }

    #[tokio::test]
    async fn test_mvcc_snapshot_isolation() {
        let (storage, _tmp) = test_storage().await;

        let tx1 = TxId::new(1);
        storage.put(tx1, b"key", b"val_t1").await.expect("put t1"); // expect
        storage.commit(tx1).await.expect("commit t1"); // expect
        let seq_t1 = storage.last_seq_no().await.expect("seq t1"); // expect

        let tx2 = TxId::new(2);
        storage.put(tx2, b"key", b"val_t2").await.expect("put t2"); // expect
        storage.commit(tx2).await.expect("commit t2"); // expect

        // Read at seq_t1 should exclude T2's update
        let val_at_t1 = storage.get_at_seq(b"key", seq_t1).await.expect("get at t1"); // expect
        assert_eq!(val_at_t1, Some(b"val_t1".to_vec()));

        // Current get should return T2's value
        let val_current = storage.get(b"key").await.expect("get current"); // expect
        assert_eq!(val_current, Some(b"val_t2".to_vec()));
    }

    #[tokio::test]
    async fn test_flush_during_read_transaction_snapshot_isolation() {
        let (storage, _tmp) = test_storage().await;

        let tx1 = TxId::new(1);
        storage.put(tx1, b"key_flush", b"v1").await.expect("put t1"); // expect
        storage.commit(tx1).await.expect("commit t1"); // expect
        let seq1 = storage.last_seq_no().await.expect("seq1"); // expect

        // Read snapshot taken after tx1
        let snap_tx1 = storage.last_tx_id().await.expect("last tx1"); // expect

        // Flush tx1 to SSTable
        storage.flush().await.expect("flush tx1"); // expect

        // Commit tx2 and trigger flush while read transaction was established
        let tx2 = TxId::new(2);
        storage.put(tx2, b"key_flush", b"v2").await.expect("put t2"); // expect
        storage.commit(tx2).await.expect("commit t2"); // expect

        storage.flush().await.expect("flush tx2"); // expect

        // get_at_seq with seq1 must observe v1 and exclude v2 even after flushes
        let val = storage
            .get_at_seq(b"key_flush", seq1)
            .await
            .expect("get_at_seq"); // expect
        assert_eq!(val, Some(b"v1".to_vec()));
        assert_eq!(snap_tx1, TxId::new(1));
    }

    #[tokio::test]
    async fn test_concurrent_flush_and_get_at_seq_isolation() {
        let (storage, _tmp) = test_storage().await;
        let storage = Arc::new(storage);

        // Pre-populate with base transaction
        let tx_base = TxId::new(1);
        storage
            .put(tx_base, b"key_race", b"val_base")
            .await
            .expect("put base"); // expect
        storage.commit(tx_base).await.expect("commit base"); // expect

        let mut handles = Vec::new();

        // Writer / Flusher task
        let s_writer = Arc::clone(&storage);
        handles.push(tokio::spawn(async move {
            for i in 2..=1000u64 {
                let tx = TxId::new(i);
                let val = format!("val_{i}").into_bytes();
                s_writer.put(tx, b"key_race", &val).await.expect("put loop"); // expect
                s_writer.commit(tx).await.expect("commit loop"); // expect
                if i % 10 == 0 {
                    s_writer.flush().await.expect("flush loop"); // expect
                }
            }
        }));

        // Reader task: repeatedly calling get_at_seq and validating snapshot isolation invariant
        let s_reader = Arc::clone(&storage);
        handles.push(tokio::spawn(async move {
            for _ in 0..1000 {
                let last_tx = s_reader.last_tx_id().await.expect("last_tx").inner(); // expect
                let last_seq = s_reader.last_seq_no().await.expect("last_seq"); // expect

                let res = s_reader
                    .get_at_seq(b"key_race", last_seq)
                    .await
                    .expect("get_at_seq"); // expect

                if let Some(val_bytes) = res {
                    let val_str = String::from_utf8(val_bytes).expect("utf8"); // expect
                    if let Some(num_str) = val_str.strip_prefix("val_") {
                        if num_str != "base" {
                            let tx_num: u64 = num_str.parse().expect("parse tx num"); // expect
                            assert!(
                                tx_num <= last_tx,
                                "MVCC Invariant Violation: Read tx {} higher than snapshot_tx {}",
                                tx_num,
                                last_tx
                            );
                        }
                    }
                }
                tokio::task::yield_now().await;
            }
        }));

        for h in handles {
            h.await.expect("task join"); // expect
        }
    }

    #[tokio::test]
    async fn test_flush_during_active_snapshot_isolation_stress() {
        let (storage, _tmp) = test_storage().await;
        let storage = Arc::new(storage);

        // Pre-populate with base transaction
        let tx_base = TxId::new(1);
        storage.put(tx_base, b"snap:key", b"v_1").await.unwrap(); // unwrap allowed
        storage.commit(tx_base).await.unwrap(); // unwrap allowed

        let s_writer = Arc::clone(&storage);
        let writer_handle = tokio::spawn(async move {
            for i in 2..=1000u64 {
                let tx = TxId::new(i);
                let val = format!("v_{i}").into_bytes();
                s_writer.put(tx, b"snap:key", &val).await.unwrap(); // unwrap allowed
                s_writer.commit(tx).await.unwrap(); // unwrap allowed
                if i % 5 == 0 {
                    s_writer.flush().await.unwrap(); // unwrap allowed
                }
            }
        });

        let s_reader = Arc::clone(&storage);
        let reader_handle = tokio::spawn(async move {
            for _ in 0..1000 {
                let snapshot_tx = s_reader.last_tx_id().await.unwrap().inner(); // unwrap allowed
                let snapshot_seq = s_reader.last_seq_no().await.unwrap(); // unwrap allowed

                let get_val = s_reader
                    .get_at_seq(b"snap:key", snapshot_seq)
                    .await
                    .unwrap(); // unwrap allowed
                if let Some(bytes) = get_val {
                    let val_str = String::from_utf8(bytes).unwrap(); // unwrap allowed
                    let tx_num: u64 = val_str.strip_prefix("v_").unwrap().parse().unwrap(); // unwrap allowed
                    assert!(
                        tx_num <= snapshot_tx,
                        "MVCC Invariant Violation during flush: read tx {} exceeds snapshot_tx {}",
                        tx_num,
                        snapshot_tx
                    );
                }

                let scan_res = s_reader
                    .scan_prefix_at(b"snap:", snapshot_seq)
                    .await
                    .unwrap(); // unwrap allowed
                assert!(!scan_res.is_empty());
                let val_str = String::from_utf8(scan_res[0].1.clone()).unwrap(); // unwrap allowed
                let tx_num: u64 = val_str.strip_prefix("v_").unwrap().parse().unwrap(); // unwrap allowed
                assert!(
                    tx_num <= snapshot_tx,
                    "MVCC Invariant Violation in scan_prefix_at during flush: read tx {} exceeds snapshot_tx {}",
                    tx_num,
                    snapshot_tx
                );

                tokio::task::yield_now().await;
            }
        });

        writer_handle.await.unwrap(); // unwrap allowed
        reader_handle.await.unwrap(); // unwrap allowed
    }

    #[tokio::test]
    async fn test_compaction_roundtrip() {
        let tmp = TempDir::new().expect("temp dir"); // expect
        let config = LsmConfig {
            path: tmp.path().to_path_buf(),
            memtable_size_limit: 1024,
            max_ram_mb: 64,
            tx_timeout: Duration::from_secs(60),
            compaction: CompactionConfig {
                min_sstables_per_tier: 2,
                size_ratio: 2.0,
                check_interval: Duration::from_secs(3600),
                yield_threshold: 100,
                max_memory_bytes: Some(1024 * 1024),
            },
            encryption_passphrase: None,
        };
        let storage = LsmStorage::new(config.clone())
            .await
            .expect("create storage"); // expect

        let tx1 = TxId::new(1);
        storage.put(tx1, b"key1", b"val1").await.unwrap(); // unwrap
        storage.commit(tx1).await.unwrap(); // unwrap
        storage.force_flush().await.unwrap(); // unwrap

        let tx2 = TxId::new(2);
        storage.put(tx2, b"key2", b"val2").await.unwrap(); // unwrap
        storage.commit(tx2).await.unwrap(); // unwrap
        storage.force_flush().await.unwrap(); // unwrap

        let compact_res = storage.maybe_compact().await.expect("compact"); // expect
        assert!(compact_res, "Compaction should occur");

        assert_eq!(storage.get(b"key1").await.unwrap(), Some(b"val1".to_vec())); // unwrap
        assert_eq!(storage.get(b"key2").await.unwrap(), Some(b"val2".to_vec()));
        // unwrap
        // unwrap
    }

    #[tokio::test]
    async fn test_sequence_numbers_strictly_monotonic_across_concurrent_commits() {
        let storage = Arc::new(test_storage().await.0);
        let mut handles = Vec::new();

        for i in 1..=10u64 {
            let st = Arc::clone(&storage);
            handles.push(tokio::spawn(async move {
                let tx = TxId::new(i);
                st.put(tx, format!("concurrent_key_{i}").as_bytes(), b"val")
                    .await
                    .unwrap(); // unwrap
                st.commit(tx).await.unwrap(); // unwrap
            }));
        }

        for h in handles {
            h.await.unwrap(); // unwrap
        }

        let last_seq = storage.last_seq_no().await.unwrap(); // unwrap
        assert_eq!(
            last_seq, 10,
            "10 commits must generate sequence numbers 1..10 monotonically"
        );
    }

    #[tokio::test]
    async fn test_scan_prefix_at_uncommitted_isolation() {
        let tmp = TempDir::new().expect("temp dir"); // expect
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
        let storage = LsmStorage::new(config).await.expect("create storage"); // expect

        // 1. Insert and commit doc1 under tx1
        let tx1 = TxId::new(1);
        storage.put(tx1, b"prefix:doc1", b"val1").await.unwrap(); // unwrap
        storage.commit(tx1).await.unwrap(); // unwrap

        // 2. Stage uncommitted doc2 under tx2
        let tx2 = TxId::new(2);
        storage.put(tx2, b"prefix:doc2", b"val2").await.unwrap(); // unwrap
                                                                  // tx2 NOT committed

        // 3. Scan prefix at current committed snapshot seq
        let seq = storage.last_seq_no().await.unwrap(); // unwrap
        let scanned = storage.scan_prefix_at(b"prefix:", seq).await.unwrap(); // unwrap

        // Uncommitted doc2 must NOT be visible in scan_prefix_at!
        assert_eq!(scanned.len(), 1);
        assert_eq!(scanned[0].0, b"prefix:doc1");
    }

    #[tokio::test]
    async fn test_get_at_seq_mvcc_sequence_correctness() {
        let (storage, _tmp) = test_storage().await;
        let key = b"mvcc_key";

        // Seq 1: insert val "a"
        let tx1 = TxId::new(1);
        storage.put(tx1, key, b"a").await.unwrap(); // unwrap #[cfg(test)]
        storage.commit(tx1).await.unwrap(); // unwrap #[cfg(test)]
        let seq1 = storage.last_seq_no().await.unwrap(); // unwrap #[cfg(test)]
        assert_eq!(seq1, 1);

        // Seq 2: delete key
        let tx2 = TxId::new(2);
        storage.delete(tx2, key).await.unwrap(); // unwrap #[cfg(test)]
        storage.commit(tx2).await.unwrap(); // unwrap #[cfg(test)]
        let seq2 = storage.last_seq_no().await.unwrap(); // unwrap #[cfg(test)]
        assert_eq!(seq2, 2);

        // Seq 3: insert val "b"
        let tx3 = TxId::new(3);
        storage.put(tx3, key, b"b").await.unwrap(); // unwrap #[cfg(test)]
        storage.commit(tx3).await.unwrap(); // unwrap #[cfg(test)]
        let seq3 = storage.last_seq_no().await.unwrap(); // unwrap #[cfg(test)]
        assert_eq!(seq3, 3);

        // get_at_seq(key, 0) -> None
        let val_seq0 = storage.get_at_seq(key, 0).await.unwrap(); // unwrap #[cfg(test)]
        assert_eq!(val_seq0, None, "seq 0 should be before any write");

        // get_at_seq(key, 1) -> Some("a")
        let val_seq1 = storage.get_at_seq(key, 1).await.unwrap(); // unwrap #[cfg(test)]
        assert_eq!(val_seq1, Some(b"a".to_vec()));

        // get_at_seq(key, 2) -> None (tombstoned)
        let val_seq2 = storage.get_at_seq(key, 2).await.unwrap(); // unwrap #[cfg(test)]
        assert_eq!(val_seq2, None, "seq 2 should return None for tombstone");

        // get_at_seq(key, 3) -> Some("b")
        let val_seq3 = storage.get_at_seq(key, 3).await.unwrap(); // unwrap #[cfg(test)]
        assert_eq!(val_seq3, Some(b"b".to_vec()));
    }

    #[tokio::test]
    async fn test_scan_prefix_bounded_pagination() {
        let (storage, _tmp) = test_storage().await;
        let tx = TxId::new(1);

        // Populate 25 items: pfx:00..pfx:24
        for i in 0..25 {
            let key = format!("pfx:{:02}", i);
            let val = format!("val:{:02}", i);
            storage
                .put(tx, key.as_bytes(), val.as_bytes())
                .await
                .unwrap();
        }
        storage.commit(tx).await.unwrap();

        // 1st call: limit 10, cursor None -> 10 items + Some(cursor)
        let (p1, cur1) = storage
            .scan_prefix_bounded(b"pfx:", 10, None)
            .await
            .unwrap();
        assert_eq!(p1.len(), 10);
        assert_eq!(p1[0].0, b"pfx:00");
        assert_eq!(p1[9].0, b"pfx:09");
        assert!(cur1.is_some());
        let cur1_val = cur1.unwrap();
        assert_eq!(cur1_val, b"pfx:09");

        // 2nd call: limit 10, cursor cur1 -> next 10 items + Some(cursor)
        let (p2, cur2) = storage
            .scan_prefix_bounded(b"pfx:", 10, Some(&cur1_val))
            .await
            .unwrap();
        assert_eq!(p2.len(), 10);
        assert_eq!(p2[0].0, b"pfx:10");
        assert_eq!(p2[9].0, b"pfx:19");
        assert!(cur2.is_some());
        let cur2_val = cur2.unwrap();
        assert_eq!(cur2_val, b"pfx:19");

        // 3rd call: limit 10, cursor cur2 -> remaining 5 items + None
        let (p3, cur3) = storage
            .scan_prefix_bounded(b"pfx:", 10, Some(&cur2_val))
            .await
            .unwrap();
        assert_eq!(p3.len(), 5);
        assert_eq!(p3[0].0, b"pfx:20");
        assert_eq!(p3[4].0, b"pfx:24");
        assert!(cur3.is_none());
    }

    #[tokio::test]
    async fn test_scan_prefix_memtable_shadows_sstable() {
        let (storage, _tmp) = test_storage().await;

        // 1. Put key "pfx:a" = "old" and flush to SSTable
        let tx1 = TxId::new(1);
        storage.put(tx1, b"pfx:a", b"old").await.unwrap(); // unwrap #[cfg(test)]
        storage.commit(tx1).await.unwrap(); // unwrap #[cfg(test)]
        storage.force_flush().await.unwrap(); // unwrap #[cfg(test)]

        // Verify it is in SSTable
        let stats = storage.stats().await.unwrap(); // unwrap #[cfg(test)]
        assert!(stats.num_segments > 0, "SSTable segment must exist");

        // 2. Put key "pfx:a" = "new" in active MemTable (unflushed)
        let tx2 = TxId::new(2);
        storage.put(tx2, b"pfx:a", b"new").await.unwrap(); // unwrap #[cfg(test)]
        storage.commit(tx2).await.unwrap(); // unwrap #[cfg(test)]

        // 3. Scan prefix "pfx:" and verify "new" is returned
        let results = storage.scan_prefix(b"pfx:").await.unwrap(); // unwrap #[cfg(test)]
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, b"pfx:a");
        assert_eq!(results[0].1, b"new");
    }

    #[tokio::test]
    async fn test_close_durability() {
        let tmp = TempDir::new().unwrap(); // unwrap #[cfg(test)]
        let config = LsmConfig {
            path: tmp.path().to_path_buf(),
            memtable_size_limit: 1024 * 1024,
            max_ram_mb: 64,
            tx_timeout: Duration::from_secs(60),
            compaction: CompactionConfig::default(),
            encryption_passphrase: None,
        };

        // 1. Open storage, write, commit WITHOUT explicit force_flush(), call close()
        {
            let storage = LsmStorage::new(config.clone()).await.unwrap(); // unwrap #[cfg(test)]
            let tx = TxId::new(1);
            storage.put(tx, b"close_key", b"close_val").await.unwrap(); // unwrap #[cfg(test)]
            storage.commit(tx).await.unwrap(); // unwrap #[cfg(test)]
            storage.close().await.unwrap(); // unwrap #[cfg(test)]
        }

        // 2. Reopen storage and read key — written data must be present
        {
            let storage = LsmStorage::new(config).await.unwrap(); // unwrap #[cfg(test)]
            let val = storage.get(b"close_key").await.unwrap(); // unwrap #[cfg(test)]
            assert_eq!(val, Some(b"close_val".to_vec()));
        }
    }

    #[tokio::test]
    async fn test_scan_prefix_at_snapshot_isolation() {
        let (storage, _tmp) = test_storage().await;
        let tx1 = TxId::new(1);
        storage.put(tx1, b"col:doc1", b"v1").await.unwrap(); // unwrap
        storage.commit(tx1).await.unwrap(); // unwrap
        let seq_after_tx1 = storage.last_seq_no().await.unwrap(); // unwrap

        let tx2 = TxId::new(2);
        storage.put(tx2, b"col:doc2", b"v2").await.unwrap(); // unwrap
        storage.commit(tx2).await.unwrap(); // unwrap

        // Scan at seq_after_tx1: must ONLY see doc1, NOT doc2
        let results = storage
            .scan_prefix_at(b"col:", seq_after_tx1)
            .await
            .unwrap(); // unwrap
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, b"col:doc1");
    }

    #[tokio::test]
    async fn test_scan_prefix_at_mvcc_sequence_filtering() {
        let (storage, _tmp) = test_storage().await;

        // seq 1: put key1 = v1, key2 = v2
        let tx1 = TxId::new(1);
        storage.put(tx1, b"pfx:1", b"v1").await.unwrap(); // unwrap
        storage.put(tx1, b"pfx:2", b"v2").await.unwrap(); // unwrap
        storage.commit(tx1).await.unwrap(); // unwrap
        let seq1 = storage.last_seq_no().await.unwrap(); // unwrap

        // seq 2: update key1 = v1_new, delete key2
        let tx2 = TxId::new(2);
        storage.put(tx2, b"pfx:1", b"v1_new").await.unwrap(); // unwrap
        storage.delete(tx2, b"pfx:2").await.unwrap(); // unwrap
        storage.commit(tx2).await.unwrap(); // unwrap
        let seq2 = storage.last_seq_no().await.unwrap(); // unwrap

        // seq 3: put key3 = v3
        let tx3 = TxId::new(3);
        storage.put(tx3, b"pfx:3", b"v3").await.unwrap(); // unwrap
        storage.commit(tx3).await.unwrap(); // unwrap

        // scan_prefix_at at seq1: must see key1=v1, key2=v2, no key3
        let res_seq1 = storage.scan_prefix_at(b"pfx:", seq1).await.unwrap(); // unwrap
        assert_eq!(res_seq1.len(), 2);
        let map1: std::collections::HashMap<_, _> = res_seq1.into_iter().collect();
        assert_eq!(map1.get(&b"pfx:1"[..]), Some(&b"v1"[..].to_vec()));
        assert_eq!(map1.get(&b"pfx:2"[..]), Some(&b"v2"[..].to_vec()));

        // scan_prefix_at at seq2: must see key1=v1_new, key2 deleted, no key3
        let res_seq2 = storage.scan_prefix_at(b"pfx:", seq2).await.unwrap(); // unwrap
        assert_eq!(res_seq2.len(), 1);
        assert_eq!(res_seq2[0].0, b"pfx:1");
        assert_eq!(res_seq2[0].1, b"v1_new");
    }

    #[tokio::test]
    async fn test_scan_prefix_at_tombstone_isolation() {
        let (storage, _tmp) = test_storage().await;

        // tx1: put pfx:a
        let tx1 = TxId::new(1);
        storage.put(tx1, b"pfx:a", b"val_a").await.unwrap(); // unwrap
        storage.commit(tx1).await.unwrap(); // unwrap
        let seq1 = storage.last_seq_no().await.unwrap(); // unwrap

        // Flush to SSTable so pfx:a is in SSTable
        storage.force_flush().await.unwrap(); // unwrap

        // tx2: delete pfx:a (tombstone in active memtable)
        let tx2 = TxId::new(2);
        storage.delete(tx2, b"pfx:a").await.unwrap(); // unwrap
        storage.commit(tx2).await.unwrap(); // unwrap
        let seq2 = storage.last_seq_no().await.unwrap(); // unwrap

        // scan_prefix_at at seq1: must return pfx:a despite tombstone added at seq2
        let res_seq1 = storage.scan_prefix_at(b"pfx:", seq1).await.unwrap(); // unwrap
        assert_eq!(res_seq1.len(), 1);
        assert_eq!(res_seq1[0].0, b"pfx:a");
        assert_eq!(res_seq1[0].1, b"val_a");

        // scan_prefix_at at seq2: tombstone applies, returns empty
        let res_seq2 = storage.scan_prefix_at(b"pfx:", seq2).await.unwrap(); // unwrap
        assert!(res_seq2.is_empty());
    }

    #[test]
    fn prop_lsm_scan_prefix_at_consistency() {
        use proptest::prelude::*;

        #[derive(Debug, Clone)]
        enum Op {
            Put(u8, Vec<u8>),
            Delete(u8),
        }

        let op_strategy = proptest::collection::vec(
            prop_oneof![
                (1u8..10, proptest::collection::vec(any::<u8>(), 1..10))
                    .prop_map(|(k, v)| Op::Put(k, v)),
                (1u8..10).prop_map(Op::Delete),
            ],
            10..60,
        );

        proptest!(ProptestConfig::with_cases(20), |(ops in op_strategy)| {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap(); // unwrap

            rt.block_on(async {
                let tmp = tempfile::TempDir::new().unwrap(); // unwrap
                let config = LsmConfig {
                    path: tmp.path().to_path_buf(),
                    memtable_size_limit: 1024 * 1024,
                    max_ram_mb: 64,
                    tx_timeout: Duration::from_secs(60),
                    compaction: CompactionConfig::default(),
                    encryption_passphrase: None,
                };
                let storage = LsmStorage::new(config).await.unwrap(); // unwrap

                let mut current_tx = 1u64;
                let mut tx_checkpoints = Vec::new();

                for op in ops {
                    let tx = TxId::new(current_tx);
                    match op {
                        Op::Put(key_id, val) => {
                            let key = format!("pfx:{}", key_id);
                            let _ = storage.put(tx, key.as_bytes(), &val).await;
                        }
                        Op::Delete(key_id) => {
                            let key = format!("pfx:{}", key_id);
                            let _ = storage.delete(tx, key.as_bytes()).await;
                        }
                    }
                    if storage.commit(tx).await.is_ok() {
                        let seq = storage.last_seq_no().await.unwrap(); // unwrap
                        tx_checkpoints.push((current_tx, seq));
                        current_tx += 1;
                    }
                }

                // Verify scan_prefix_at at each target sequence against reference replay model
                for &(_tx_num, target_seq) in &tx_checkpoints {
                    let scanned = storage.scan_prefix_at(b"pfx:", target_seq).await.unwrap(); // unwrap
                    let actual_map: std::collections::BTreeMap<_, _> = scanned.into_iter().collect();

                    // Replay all committed ops up to target_seq to build expected state
                    let mut ref_map = std::collections::BTreeMap::new();
                    let state = storage.state.read().await;

                    // Collect all entries from MemTable + SSTables with seq <= target_seq
                    let mut all_entries = Vec::new();
                    for (k, v, seq, _tx) in state.memtable.iter() {
                        all_entries.push((k.to_vec(), v.to_vec(), seq));
                    }
                    for mt in &state.immutable_memtables {
                        for (k, v, seq, _tx) in mt.iter() {
                            all_entries.push((k.to_vec(), v.to_vec(), seq));
                        }
                    }
                    drop(state);

                    let sstables = storage.sstables.read().await;
                    for sst in sstables.iter() {
                        let sst_entries = sst.scan_prefix(b"pfx:").await.unwrap(); // unwrap
                        for (k, v, seq, _tx) in sst_entries {
                            all_entries.push((k.to_vec(), v.to_vec(), seq));
                        }
                    }
                    drop(sstables);

                    all_entries.sort_by_key(|e| e.2 & !TOMBSTONE_BIT);

                    for (k, v, seq) in all_entries {
                        let raw_seq = seq & !TOMBSTONE_BIT;
                        if raw_seq <= target_seq && k.starts_with(b"pfx:") {
                            if (seq & TOMBSTONE_BIT) != 0 {
                                ref_map.remove(&k);
                            } else {
                                ref_map.insert(k, v);
                            }
                        }
                    }

                    prop_assert_eq!(actual_map, ref_map, "scan_prefix_at at seq {} must match reference model", target_seq);
                }
                Ok(())
            }).unwrap(); // unwrap
        });
    }

    #[tokio::test]
    async fn test_input_boundary_guards() {
        let tmp = TempDir::new().expect("temp dir"); // expect
        let config = LsmConfig {
            path: tmp.path().to_path_buf(),
            ..Default::default()
        };
        let storage = LsmStorage::new(config).await.expect("create storage"); // expect
        let tx = TxId::new(1);

        // 1. Empty key check
        assert!(matches!(
            storage.put(tx, b"", b"val").await,
            Err(MemFuseError::InvalidInput(_))
        ));
        assert!(matches!(
            storage.delete(tx, b"").await,
            Err(MemFuseError::InvalidInput(_))
        ));
        assert!(matches!(
            storage.get(b"").await,
            Err(MemFuseError::InvalidInput(_))
        ));
        assert!(matches!(
            storage.get_at_seq(b"", 10).await,
            Err(MemFuseError::InvalidInput(_))
        ));

        // 2. Oversized key check (> 1MB)
        let huge_key = vec![b'a'; MAX_KEY_SIZE + 1];
        assert!(matches!(
            storage.put(tx, &huge_key, b"val").await,
            Err(MemFuseError::InvalidInput(_))
        ));
        assert!(matches!(
            storage.delete(tx, &huge_key).await,
            Err(MemFuseError::InvalidInput(_))
        ));
        assert!(matches!(
            storage.get(&huge_key).await,
            Err(MemFuseError::InvalidInput(_))
        ));

        // 3. Oversized delete_many batch (> 10,000 items)
        let too_many_keys = vec![b"key".to_vec(); MAX_BATCH_SIZE + 1];
        assert!(matches!(
            storage.delete_many(tx, too_many_keys).await,
            Err(MemFuseError::InvalidInput(_))
        ));

        // 4. Oversized value check (> 128MB)
        let huge_val = vec![b'v'; MAX_VALUE_SIZE + 1];
        assert!(matches!(
            storage.put(tx, b"valid_key", &huge_val).await,
            Err(MemFuseError::InvalidInput(_))
        ));
    }

    #[tokio::test]
    async fn test_rollback_to_tx_edge_cases() {
        let (storage, _tmp) = test_storage().await;

        // Rollback on empty storage with non-existent TxId (e.g. TxId::new(999))
        let res = storage.rollback_to_tx(TxId::new(999)).await;
        // Rolling back on empty WAL safely returns Ok((0, [0; 32]))
        assert!(res.is_ok());

        // Put and commit a transaction
        let tx1 = TxId::new(1);
        storage.put(tx1, b"key1", b"val1").await.expect("put"); // expect
        storage.commit(tx1).await.expect("commit"); // expect

        // Rollback to TxId::new(0) -> should wipe key1
        storage
            .rollback_to_tx(TxId::new(0))
            .await
            .expect("rollback to 0"); // expect
        assert_eq!(storage.get(b"key1").await.expect("get"), None); // expect
    }

    #[tokio::test]
    async fn test_rollback_tombstone_sstable() {
        let (storage, _tmp) = test_storage().await;

        // a. Inserts committen
        let tx1 = TxId::new(1);
        storage.put(tx1, b"key1", b"val1").await.unwrap(); // unwrap
        storage.commit(tx1).await.unwrap(); // unwrap

        let tx2 = TxId::new(2);
        storage.put(tx2, b"key2", b"val2").await.unwrap(); // unwrap
        storage.commit(tx2).await.unwrap(); // unwrap

        // b. Delete (Tombstone) als letzte Op vor Target committen und flushen
        let tx3 = TxId::new(3);
        storage.delete(tx3, b"key2").await.unwrap(); // unwrap
        storage.commit(tx3).await.unwrap(); // unwrap
        storage.force_flush().await.unwrap(); // unwrap

        // c. Rollback auf target_tx (tx3)
        storage.rollback_to_tx(tx3).await.unwrap(); // unwrap

        // d. Neuen Insert mit neuem Key committen
        let tx4 = TxId::new(4);
        storage.put(tx4, b"key3", b"val3").await.unwrap(); // unwrap
        storage.commit(tx4).await.unwrap(); // unwrap

        // e. Assert: Der neue Key ist lesbar und TOMBSTONE_BIT ist NICHT gesetzt
        let current_max_seq = storage.next_seq_no.load(Ordering::Acquire);
        let val = storage.get_at_seq(b"key3", current_max_seq).await.unwrap(); // unwrap
        assert_eq!(val, Some(b"val3".to_vec()));

        let last_seq = storage.last_seq_no().await.unwrap(); // unwrap
        assert_eq!(
            last_seq & TOMBSTONE_BIT,
            0,
            "Sequence number of new insert must not have TOMBSTONE_BIT set"
        );
    }

    #[tokio::test]
    async fn test_rollback_tombstone_wal() {
        let (storage, _tmp) = test_storage().await;

        // a. Inserts committen
        let tx1 = TxId::new(1);
        storage.put(tx1, b"k1", b"v1").await.unwrap(); // unwrap
        storage.commit(tx1).await.unwrap(); // unwrap

        let tx2 = TxId::new(2);
        storage.put(tx2, b"k2", b"v2").await.unwrap(); // unwrap
        storage.commit(tx2).await.unwrap(); // unwrap

        // b. Delete (Tombstone) im WAL als letzte Op vor Target committen (unflushed)
        let tx3 = TxId::new(3);
        storage.delete(tx3, b"k2").await.unwrap(); // unwrap
        storage.commit(tx3).await.unwrap(); // unwrap

        // c. Rollback auf target_tx (tx3)
        storage.rollback_to_tx(tx3).await.unwrap(); // unwrap

        // d. Neuen Insert mit neuem Key committen
        let tx4 = TxId::new(4);
        storage.put(tx4, b"k3", b"v3").await.unwrap(); // unwrap
        storage.commit(tx4).await.unwrap(); // unwrap

        // e. Assert: Der neue Key ist lesbar und TOMBSTONE_BIT ist NICHT gesetzt
        let current_max_seq = storage.next_seq_no.load(Ordering::Acquire);
        let val = storage.get_at_seq(b"k3", current_max_seq).await.unwrap(); // unwrap
        assert_eq!(val, Some(b"v3".to_vec()));

        let last_seq = storage.last_seq_no().await.unwrap(); // unwrap
        assert_eq!(
            last_seq & TOMBSTONE_BIT,
            0,
            "Sequence number of new insert must not have TOMBSTONE_BIT set"
        );
    }

    #[tokio::test]
    async fn test_rollback_tombstone_subsequent_ops() {
        let (storage, _tmp) = test_storage().await;

        let tx1 = TxId::new(1);
        storage.put(tx1, b"key1", b"val1").await.unwrap(); // unwrap
        storage.commit(tx1).await.unwrap(); // unwrap

        let tx2 = TxId::new(2);
        storage.delete(tx2, b"key1").await.unwrap(); // unwrap
        storage.commit(tx2).await.unwrap(); // unwrap

        // Rollback auf tx2
        storage.rollback_to_tx(tx2).await.unwrap(); // unwrap

        // Abfolge von weiteren Inserts und Deletes in Folge
        let tx3 = TxId::new(3);
        storage.put(tx3, b"key2", b"val2").await.unwrap(); // unwrap
        storage.commit(tx3).await.unwrap(); // unwrap

        let tx4 = TxId::new(4);
        storage.delete(tx4, b"key2").await.unwrap(); // unwrap
        storage.commit(tx4).await.unwrap(); // unwrap

        let tx5 = TxId::new(5);
        storage.put(tx5, b"key3", b"val3").await.unwrap(); // unwrap
        storage.commit(tx5).await.unwrap(); // unwrap

        // Prüfe direkt im MemTable, dass der neueste Zustand jedes Keys das korrekte TOMBSTONE_BIT trägt
        let state = storage.state.read().await;
        for (k, _v, seq, _tx) in state.memtable.iter_latest() {
            if k.as_ref() == b"key1" || k.as_ref() == b"key2" {
                assert_ne!(
                    seq & TOMBSTONE_BIT,
                    0,
                    "Latest entry for deleted key {:?} must have TOMBSTONE_BIT set",
                    String::from_utf8_lossy(&k)
                );
            } else if k.as_ref() == b"key3" {
                assert_eq!(
                    seq & TOMBSTONE_BIT,
                    0,
                    "Latest entry for inserted key {:?} must NOT have TOMBSTONE_BIT set",
                    String::from_utf8_lossy(&k)
                );
            }
        }
        drop(state);

        // Verify final state via read path
        assert_eq!(storage.get(b"key1").await.unwrap(), None); // unwrap
        assert_eq!(storage.get(b"key2").await.unwrap(), None); // unwrap
        assert_eq!(storage.get(b"key3").await.unwrap(), Some(b"val3".to_vec()));
        // unwrap
    }

    #[tokio::test]
    async fn test_concurrent_get_and_flush_latency() {
        let (storage, _tmp) = test_storage().await;
        let storage = Arc::new(storage);

        // Seed storage with data in memtable
        for i in 0..100 {
            let tx = TxId::new(i + 1);
            let k = format!("key-{}", i);
            let v = format!("val-{}", i);
            storage.put(tx, k.as_bytes(), v.as_bytes()).await.unwrap();
            storage.commit(tx).await.unwrap();
        }

        // Spawn 4 concurrent read tasks that call get() repeatedly
        let mut handles = Vec::new();
        for task_idx in 0..4 {
            let storage_clone = Arc::clone(&storage);
            let handle = tokio::spawn(async move {
                let start = std::time::Instant::now();
                let key = format!("key-{}", task_idx * 10);
                for _ in 0..50 {
                    let req_start = std::time::Instant::now();
                    let val = storage_clone.get(key.as_bytes()).await.unwrap();
                    let elapsed = req_start.elapsed();
                    assert!(val.is_some());
                    assert!(
                        elapsed < std::time::Duration::from_millis(5),
                        "get() took {:?}, exceeding 5 ms latency threshold under concurrent flush",
                        elapsed
                    );
                    tokio::time::sleep(std::time::Duration::from_micros(100)).await;
                }
                start.elapsed()
            });
            handles.push(handle);
        }

        // Trigger flush concurrently
        storage.flush().await.unwrap();

        for handle in handles {
            handle.await.unwrap();
        }
    }

    #[tokio::test]
    async fn test_recovery_scan_ignores_and_removes_tmp_files() {
        let tmp = tempfile::TempDir::new().unwrap(); // unwrap #[cfg(test)]
        let corrupt_tmp_path = tmp.path().join("sst-compact-corrupt.sst.tmp");
        tokio::fs::write(&corrupt_tmp_path, b"invalid sst data from crash")
            .await
            .unwrap(); // unwrap #[cfg(test)]

        let config = LsmConfig {
            path: tmp.path().to_path_buf(),
            memtable_size_limit: 1024 * 1024,
            max_ram_mb: 64,
            tx_timeout: std::time::Duration::from_secs(60),
            compaction: CompactionConfig::default(),
            encryption_passphrase: None,
        };

        let storage = LsmStorage::new(config)
            .await
            .expect("LsmStorage startup must succeed"); // expect
        assert_eq!(storage.sstables.read().await.len(), 0);

        // Assert corrupt .tmp file was removed from data directory during recovery
        assert!(
            !corrupt_tmp_path.exists(),
            "Leftover .tmp file must be removed during startup recovery scan"
        );
    }

    #[tokio::test]
    async fn test_two_instances_independent_flush_counters() {
        let tmp1 = TempDir::new().unwrap();
        let tmp2 = TempDir::new().unwrap();

        let storage1 = LsmStorage::new(LsmConfig {
            path: tmp1.path().to_path_buf(),
            ..Default::default()
        })
        .await
        .unwrap();

        let storage2 = LsmStorage::new(LsmConfig {
            path: tmp2.path().to_path_buf(),
            ..Default::default()
        })
        .await
        .unwrap();

        let tx1 = TxId::new(1);
        storage1.put(tx1, b"key1", b"val1").await.unwrap();
        storage1.commit(tx1).await.unwrap();
        storage1.force_flush().await.unwrap();

        let tx2 = TxId::new(1);
        storage2.put(tx2, b"key2", b"val2").await.unwrap();
        storage2.commit(tx2).await.unwrap();
        storage2.force_flush().await.unwrap();

        // Check instance flush counters
        assert_eq!(storage1.flush_counter.load(Ordering::Relaxed), 1);
        assert_eq!(storage2.flush_counter.load(Ordering::Relaxed), 1);

        // Confirm both generated wal-0.log in their separate directories without cross-contamination
        assert!(tmp1.path().join("wal-0.log").exists());
        assert!(tmp2.path().join("wal-0.log").exists());
    }

    #[tokio::test]
    async fn test_parallel_flush_counter_no_race() {
        let tmp1 = TempDir::new().unwrap();
        let tmp2 = TempDir::new().unwrap();

        let storage1 = Arc::new(
            LsmStorage::new(LsmConfig {
                path: tmp1.path().to_path_buf(),
                ..Default::default()
            })
            .await
            .unwrap(),
        );

        let storage2 = Arc::new(
            LsmStorage::new(LsmConfig {
                path: tmp2.path().to_path_buf(),
                ..Default::default()
            })
            .await
            .unwrap(),
        );

        let tx1 = TxId::new(1);
        storage1.put(tx1, b"key1", b"val1").await.unwrap();
        storage1.commit(tx1).await.unwrap();

        let tx2 = TxId::new(1);
        storage2.put(tx2, b"key2", b"val2").await.unwrap();
        storage2.commit(tx2).await.unwrap();

        let s1 = Arc::clone(&storage1);
        let s2 = Arc::clone(&storage2);

        let (res1, res2) = tokio::join!(s1.force_flush(), s2.force_flush());
        res1.unwrap();
        res2.unwrap();

        assert_eq!(storage1.flush_counter.load(Ordering::Relaxed), 1);
        assert_eq!(storage2.flush_counter.load(Ordering::Relaxed), 1);
        assert!(tmp1.path().join("wal-0.log").exists());
        assert!(tmp2.path().join("wal-0.log").exists());
    }

    #[tokio::test]
    async fn test_lsm_put_if_absent_parallel_two_tasks() {
        let (storage, _tmp) = test_storage().await;
        let storage = Arc::new(storage);
        let key = b"cas_key_2tasks";

        let s1 = Arc::clone(&storage);
        let h1 = tokio::spawn(async move {
            let tx = TxId::new(1);
            let res = s1.put_if_absent(tx, key, b"val1").await;
            if res.as_ref().copied().unwrap_or(false) {
                let _ = s1.commit(tx).await;
            }
            res
        });

        let s2 = Arc::clone(&storage);
        let h2 = tokio::spawn(async move {
            let tx = TxId::new(2);
            let res = s2.put_if_absent(tx, key, b"val2").await;
            if res.as_ref().copied().unwrap_or(false) {
                let _ = s2.commit(tx).await;
            }
            res
        });

        let r1 = h1.await.unwrap().unwrap();
        let r2 = h2.await.unwrap().unwrap();

        assert_ne!(
            r1, r2,
            "Exactly one task must succeed (true) and the other fail (false)"
        );

        let stored_val = storage.get(key).await.unwrap().expect("value must exist");
        if r1 {
            assert_eq!(stored_val, b"val1");
        } else {
            assert_eq!(stored_val, b"val2");
        }
    }

    #[tokio::test]
    async fn test_lsm_put_if_absent_stress_200_tasks() {
        let (storage, _tmp) = test_storage().await;
        let storage = Arc::new(storage);
        let key = b"cas_key_stress_200";

        let mut set = tokio::task::JoinSet::new();

        for i in 0..200u64 {
            let s = Arc::clone(&storage);
            let val = format!("val_{i}").into_bytes();
            set.spawn(async move {
                let tx = TxId::new(i + 1);
                let res = s.put_if_absent(tx, key, &val).await;
                if res.as_ref().copied().unwrap_or(false) {
                    let _ = s.commit(tx).await;
                }
                (i, res)
            });
        }

        let mut true_count = 0;
        let mut false_count = 0;
        let mut winning_task_id = None;

        while let Some(res) = set.join_next().await {
            let (task_id, result) = res.unwrap();
            match result {
                Ok(true) => {
                    true_count += 1;
                    winning_task_id = Some(task_id);
                }
                Ok(false) => {
                    false_count += 1;
                }
                Err(e) => panic!("Unexpected error in task {task_id}: {e:?}"),
            }
        }

        assert_eq!(true_count, 1, "Exactly 1 task must return Ok(true)");
        assert_eq!(false_count, 199, "199 tasks must return Ok(false)");

        let winner = winning_task_id.expect("winning task id");
        let expected_val = format!("val_{winner}").into_bytes();
        let stored_val = storage.get(key).await.unwrap().expect("value must exist");
        assert_eq!(
            stored_val, expected_val,
            "Stored value must match winning task's value"
        );
    }
}
