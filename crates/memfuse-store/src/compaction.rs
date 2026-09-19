// FILE-CONTEXT
// STAND: 2026-09-11T10:21:34Z (SESSION: 31ada253)
// ZWECK: Size-Tiered Compaction Strategy (STCS) für SSTables
// INVARIANTEN: Compaction löscht nur Tombstones, die von keinem gepinnten Snapshot mehr benötigt werden
// NICHT-OFFENSICHTLICH: Multi-Version-Merging behält die höchste Sequence Number
// SIEHE AUCH: lsm.rs, sstable.rs, DECISIONS.md

//! Background compaction engine for the LSM-Tree.
//!
//! Implements a Size-Tiered Compaction Strategy (STCS):
//! Groups SSTables by size class and merges groups that exceed a threshold.
//! Tombstones are garbage-collected during merge when no active snapshot
//! references them.

// INVARIANT: Background Compaction (STCS — Size-Tiered Compaction Strategy).
// ALGORITHMUS: Gruppiere SSTables nach Größenklasse → Merge wenn >= min_sstables_per_tier.
// TOMBSTONE-GC: Tombstones werden NUR gelöscht wenn seq < min_active_seqno (MVCC-SAFE).
// ATOMARER SWAP: Merge unter read-lock, SSTable-Liste swap unter write-lock.
// INVARIANTE-COMP-1: Merge-Iterator liest alle Kandidaten-SSTables vollständig. Kein Key-Value-Paar geht verloren.
// INVARIANTE-COMP-2: Tombstone-GC löscht Tombstones nur wenn seq < min_active_snapshot.
// INVARIANTE-COMP-3: Atomarer SSTable-Swap: Alte SSTables bleiben lesbar (über Arc) bis swap, neue Datei ist vollständig fsynced.
// LIFECYCLE: run_loop() -> maybe_compact() -> select_candidates() -> merge_sstables()
//!
//! Implements a Size-Tiered Compaction Strategy (STCS):
//! Groups SSTables by size class and merges groups that exceed a threshold.
//! Tombstones are garbage-collected during merge when no active snapshot
//! references them.

// FILE-CONTEXT
// STAND: 2026-08-30T21:49:55Z (SESSION: 283abf0f)
// ZWECK:       STCS-Compaction-Engine (Size-Tiered Compaction Strategy)
// INVARIANTEN: Compaction must not block concurrent reads, tombstone GC safe with active snapshot min_seqno, atomic SSTable swap
// HOTSPOTS:    compact_sstables(), merge_sorted_iters()
// SIEHE AUCH:  crates/memfuse-store/AGENTS.md

use crate::sstable::{BlockCache, SstableBuilder, SstableReader};
use memfuse_core::{Result, SnapshotRegistry, TOMBSTONE_BIT};
use memfuse_crypto::crypto::KeyManager;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing;

/// Configuration for the compaction engine.
#[derive(Debug, Clone)]
pub struct CompactionConfig {
    /// Minimum number of SSTables in a size tier to trigger compaction.
    pub min_sstables_per_tier: usize,
    /// Size ratio between adjacent tiers (e.g., 4.0 means each tier is ~4x the previous).
    pub size_ratio: f64,
    /// Interval between compaction checks.
    pub check_interval: Duration,
    /// Yield execution after this many entries during merge.
    pub yield_threshold: usize,
    /// Maximum memory (in bytes) to use for in-memory buffering during merge.
    pub max_memory_bytes: Option<u64>,
    /// I/O-Rate-Limit für Compaction-Merge-Writes (Token-Bucket, plattformneutral).
    /// `None` = unbegrenzt (Standard).
    /// Bei Aktivierung: Token-Bucket-Delay nach jedem Merge-Block AUSSERHALB
    /// aller MVCC-Write-Locks. Max. 100 ms Delay pro Iteration.
    /// Verhindert NVMe-Queue-Depth-Sättigung die P95-Lese-Latenz von hybrid_search() erhöht.
    pub max_io_bytes_per_second: Option<u64>,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            min_sstables_per_tier: 4,
            size_ratio: 4.0,
            check_interval: Duration::from_secs(30),
            yield_threshold: 1000,
            max_memory_bytes: Some(128 * 1024 * 1024), // 128MB budget by default
            max_io_bytes_per_second: None,
        }
    }
}

/// Shared mutable state that LsmStorage and CompactionEngine both access.
///
/// This is the same `LsmState` from `lsm.rs`, but we define the compaction
/// interface in terms of what we need: the SSTable list and the data path.
use std::sync::atomic::AtomicU64;

pub struct CompactionEngine {
    config: CompactionConfig,
    snapshot_registry: Arc<SnapshotRegistry>,
    block_cache: Arc<BlockCache>,
    key_manager: Option<Arc<KeyManager>>,
    budget: Arc<memfuse_core::ResourceTracker>,
    manifest: Option<Arc<crate::manifest::Manifest>>,
    compaction_counter: AtomicU64,
    pressure_rx: Option<tokio::sync::watch::Receiver<crate::system_pressure::SystemPressure>>,
}

impl CompactionEngine {
    /// Creates a new compaction engine.
    pub fn new(
        config: CompactionConfig,
        snapshot_registry: Arc<SnapshotRegistry>,
        block_cache: Arc<BlockCache>,
        key_manager: Option<Arc<KeyManager>>,
        budget: Arc<memfuse_core::ResourceTracker>,
        manifest: Option<Arc<crate::manifest::Manifest>>,
    ) -> Self {
        Self {
            config,
            snapshot_registry,
            block_cache,
            key_manager,
            budget,
            manifest,
            compaction_counter: AtomicU64::new(0),
            pressure_rx: None,
        }
    }

    /// Attaches a system pressure watch receiver to enable pressure-aware compaction backpressure.
    pub fn with_pressure_rx(
        mut self,
        rx: tokio::sync::watch::Receiver<crate::system_pressure::SystemPressure>,
    ) -> Self {
        self.pressure_rx = Some(rx);
        self
    }

    /// Evaluates whether compaction should run and performs it if needed.
    ///
    /// Takes a write-lock on the SSTable list to atomically swap old SSTables
    /// for the compacted result.
    // AI-TAG[SMELL][RESOLVED] audit-H-3: LsmStorage hält eine persistente CompactionEngine-Instanz (LsmStorage.compaction_engine), wodurch Compaction-State & Zähler erhalten bleiben.
    pub async fn maybe_compact(
        &self,
        sstables: &RwLock<Vec<Arc<SstableReader>>>,
        data_path: &std::path::Path,
    ) -> Result<bool> {
        self.maybe_compact_with_cancel(sstables, data_path, None)
            .await
    }

    /// Evaluates whether compaction should run and performs it with an optional cancellation token.
    pub async fn maybe_compact_with_cancel(
        &self,
        sstables: &RwLock<Vec<Arc<SstableReader>>>,
        data_path: &std::path::Path,
        cancel_token: Option<&tokio_util::sync::CancellationToken>,
    ) -> Result<bool> {
        // 1. Select candidates under a single read-lock window.
        // Both candidate decision, Arc cloning, and full-compaction determination
        // happen atomically under one lock acquisition to prevent TOCTOU race conditions.
        let (mut input_ssts, is_full_compaction) = {
            let ssts = sstables.read().await;
            if ssts.len() < self.config.min_sstables_per_tier {
                return Ok(false);
            }
            match self.select_compaction_candidates(&ssts) {
                Some(candidates) if candidates.len() >= 2 => {
                    let is_full = candidates.len() == ssts.len();
                    (candidates, is_full)
                }
                _ => return Ok(false),
            }
        };

        input_ssts.sort_by_key(|sst| sst.metadata().max_seq & !TOMBSTONE_BIT);

        tracing::info!(
            "Compaction triggered: merging {} SSTables",
            input_ssts.len()
        );

        // 2. Perform the merge (no lock held — this is the expensive part)
        let min_snapshot_seq = self.snapshot_registry.min_active_seqno();
        let output_path = self.generate_sst_path(data_path)?;
        self.merge_sstables_with_cancel(
            &input_ssts,
            &output_path,
            min_snapshot_seq,
            is_full_compaction,
            cancel_token,
        )
        .await?;

        // Explicit fsync of output file and parent directory
        crate::util::fsync_parent_dir(&output_path).await?;

        // 4. Check consistency under read-lock before writing MANIFEST
        let (all_present, insertion_point, old_paths) = {
            let ssts = sstables.read().await;

            let all_present = input_ssts
                .iter()
                .all(|inp| ssts.iter().any(|sst| Arc::ptr_eq(inp, sst)));

            if !all_present {
                (false, 0, Vec::new())
            } else {
                let insertion_point = ssts
                    .iter()
                    .position(|sst| input_ssts.iter().any(|inp| Arc::ptr_eq(inp, sst)))
                    .unwrap_or(ssts.len());

                let old_paths: Vec<PathBuf> = input_ssts
                    .iter()
                    .filter_map(|inp| {
                        ssts.iter()
                            .find(|sst| Arc::ptr_eq(inp, sst))
                            .map(|sst| sst.file_path().to_path_buf())
                    })
                    .collect();

                (true, insertion_point as u64, old_paths)
            }
        };

        if !all_present {
            // Concurrent modification detected — abort compaction, clean up output file without writing MANIFEST entry
            tracing::warn!(
                "Compaction aborted: input SSTables modified during merge \
                 (concurrent flush or rollback detected)"
            );
            if let Err(e) = tokio::fs::remove_file(&output_path).await {
                tracing::warn!(
                    "Failed to clean up aborted compaction output {:?}: {}",
                    output_path,
                    e
                );
            }
            return Ok(false);
        }

        // Open the new SSTable reader
        let new_reader = Arc::new(
            SstableReader::open_with_key_manager(
                &output_path,
                Arc::clone(&self.block_cache),
                self.key_manager.clone(),
            )
            .await?,
        );

        // 5. Write EXACTLY ONE atomic `Replace` entry to MANIFEST and fsync
        if let Some(ref manifest) = self.manifest {
            manifest
                .append(&crate::manifest::ManifestEntry::Replace {
                    removed: old_paths.clone(),
                    added: output_path.clone(),
                    added_max_tx: new_reader.metadata().max_tx_id,
                    rank: insertion_point,
                })
                .await?;
        }

        // 6. In-memory SSTable swap under write-lock — mirroring already-persisted MANIFEST state
        {
            let mut ssts = sstables.write().await;

            // Remove input SSTables by identity (Arc::ptr_eq), not by raw index
            ssts.retain(|sst| !input_ssts.iter().any(|inp| Arc::ptr_eq(inp, sst)));

            // Add new SSTable at insertion point
            let insert_idx = (insertion_point as usize).min(ssts.len());
            ssts.insert(insert_idx, new_reader);

            // Re-sort SSTable list by max_seq to guarantee shadowing/visibility order.
            // Non-input SSTables might lie between the oldest and newest input SSTables.
            ssts.sort_by_key(|sst| sst.metadata().max_seq & !TOMBSTONE_BIT);

            debug_assert!(
                ssts.windows(2)
                    .all(|w| (w[0].metadata().max_seq & !TOMBSTONE_BIT)
                        <= (w[1].metadata().max_seq & !TOMBSTONE_BIT)),
                "SSTable list must be sorted by max_seq in ascending order after compaction swap"
            );
        }

        // 7. Delete old SSTable files (best-effort cleanup outside lock)
        for path in &old_paths {
            if let Err(e) = tokio::fs::remove_file(path).await {
                tracing::warn!("Failed to delete compacted SSTable {:?}: {}", path, e);
            }
            let uuid_sidecar = PathBuf::from(format!("{}.uuid", path.display()));
            if matches!(tokio::fs::try_exists(&uuid_sidecar).await, Ok(true)) {
                if let Err(e) = tokio::fs::remove_file(&uuid_sidecar).await {
                    tracing::debug!(
                        "Could not remove SSTable UUID sidecar {:?}: {} (non-critical)",
                        uuid_sidecar,
                        e
                    );
                }
            }
        }

        tracing::info!(
            "Compaction complete: merged {} SSTables into {:?}",
            input_ssts.len(),
            output_path
        );

        if let Some(ref manifest) = self.manifest {
            let current_live_entries: Vec<crate::manifest::ManifestEntry> = {
                let ssts = sstables.read().await;
                ssts.iter()
                    .map(|sst| crate::manifest::ManifestEntry::Add {
                        path: sst.file_path().to_path_buf(),
                        max_tx: sst.metadata().max_tx_id,
                    })
                    .collect()
            };
            if let Err(e) = manifest
                .maybe_rollover(
                    &current_live_entries,
                    crate::manifest::DEFAULT_ROLLOVER_THRESHOLD_BYTES,
                )
                .await
            {
                tracing::warn!("Periodic MANIFEST rollover failed after compaction: {e}");
            }
        }

        Ok(true)
    }

    /// Selects SSTables to compact using Size-Tiered strategy.
    ///
    /// Groups by size class and returns the first group that meets the threshold.
    fn select_compaction_candidates(
        &self,
        ssts: &[Arc<SstableReader>],
    ) -> Option<Vec<Arc<SstableReader>>> {
        if ssts.len() < 2 {
            return None;
        }

        // Group SSTables by size tier
        let mut tiers: Vec<Vec<usize>> = Vec::new();

        for (i, sst) in ssts.iter().enumerate() {
            let size = sst.metadata().file_size;
            let mut placed = false;

            for tier in &mut tiers {
                if let Some(&neighbor_idx) = tier.last() {
                    if let Some(neighbor_sst) = ssts.get(neighbor_idx) {
                        let neighbor_size = neighbor_sst.metadata().file_size;
                        let ratio = if size > neighbor_size {
                            size as f64 / neighbor_size.max(1) as f64
                        } else {
                            neighbor_size as f64 / size.max(1) as f64
                        };

                        if ratio <= self.config.size_ratio {
                            tier.push(i);
                            placed = true;
                            break;
                        }
                    }
                }
            }

            if !placed {
                tiers.push(vec![i]);
            }
        }

        // Return the most-filled tier with enough candidates (FIND-STO-002)
        // Tie-breaker: prefer tiers with smaller files (likely closer to L0)
        tiers.sort_by(|a, b| {
            b.len().cmp(&a.len()).then_with(|| {
                let a_size = a
                    .first()
                    .and_then(|&i| ssts.get(i))
                    .map(|s| s.metadata().file_size)
                    .unwrap_or(0);
                let b_size = b
                    .first()
                    .and_then(|&i| ssts.get(i))
                    .map(|s| s.metadata().file_size)
                    .unwrap_or(0);
                a_size.cmp(&b_size)
            })
        });

        for tier in tiers {
            if tier.len() >= self.config.min_sstables_per_tier {
                let mut sorted_tier = tier;
                sorted_tier.sort_by_key(|&i| {
                    ssts.get(i)
                        .map(|s| s.metadata().max_seq & !TOMBSTONE_BIT)
                        .unwrap_or(0)
                });
                return Some(
                    sorted_tier
                        .into_iter()
                        .filter_map(|i| ssts.get(i).cloned())
                        .collect(),
                );
            }
        }

        // Fallback: if total SSTable count is very high, compact the smallest ones
        if ssts.len() >= self.config.min_sstables_per_tier * 2 {
            let mut by_size: Vec<(usize, u64)> = ssts
                .iter()
                .enumerate()
                .map(|(i, s)| (i, s.metadata().file_size))
                .collect();
            by_size.sort_by_key(|&(_, size)| size);
            let count = self.config.min_sstables_per_tier;
            let mut indices: Vec<usize> = by_size[..count].iter().map(|&(i, _)| i).collect();
            indices.sort_by_key(|&i| {
                ssts.get(i)
                    .map(|s| s.metadata().max_seq & !TOMBSTONE_BIT)
                    .unwrap_or(0)
            });
            return Some(
                indices
                    .into_iter()
                    .filter_map(|i| ssts.get(i).cloned())
                    .collect(),
            );
        }

        None
    }

    /// Performs a multi-way merge of input SSTables into a single output SSTable.
    ///
    /// During merge:
    /// - Duplicate keys: newest sequence number wins
    /// - Tombstones: removed if `seq_no < min_snapshot_seq` (no snapshot references them)
    pub async fn merge_sstables(
        &self,
        inputs: &[Arc<SstableReader>],
        output_path: &std::path::Path,
        min_snapshot_seq: u64,
        is_full_compaction: bool,
    ) -> Result<()> {
        self.merge_sstables_with_cancel(
            inputs,
            output_path,
            min_snapshot_seq,
            is_full_compaction,
            None,
        )
        .await
    }

    /// Performs a multi-way merge with an optional cancellation token.
    pub async fn merge_sstables_with_cancel(
        &self,
        inputs: &[Arc<SstableReader>],
        output_path: &std::path::Path,
        min_snapshot_seq: u64,
        is_full_compaction: bool,
        cancel_token: Option<&tokio_util::sync::CancellationToken>,
    ) -> Result<()> {
        let merge_res = self
            .merge_sstables_inner(
                inputs,
                output_path,
                min_snapshot_seq,
                is_full_compaction,
                cancel_token,
            )
            .await;

        if merge_res.is_err() {
            if let Err(e) = tokio::fs::remove_file(output_path).await {
                if e.kind() != std::io::ErrorKind::NotFound {
                    tracing::warn!(
                        "Failed to clean up partial compaction output file {:?}: {}",
                        output_path,
                        e
                    );
                }
            }
        }

        merge_res
    }

    async fn merge_sstables_inner(
        &self,
        inputs: &[Arc<SstableReader>],
        output_path: &std::path::Path,
        min_snapshot_seq: u64,
        is_full_compaction: bool,
        cancel_token: Option<&tokio_util::sync::CancellationToken>,
    ) -> Result<()> {
        struct HeapItem {
            key: bytes::Bytes,
            value: bytes::Bytes,
            seq: u64,
            tx: u64,
            source_idx: usize,
        }

        impl PartialEq for HeapItem {
            fn eq(&self, other: &Self) -> bool {
                self.key == other.key
                    && (self.seq & !TOMBSTONE_BIT) == (other.seq & !TOMBSTONE_BIT)
                    && self.source_idx == other.source_idx
            }
        }

        impl Eq for HeapItem {}

        impl PartialOrd for HeapItem {
            fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
                Some(self.cmp(other))
            }
        }

        impl Ord for HeapItem {
            fn cmp(&self, other: &Self) -> std::cmp::Ordering {
                // Reverse key comparison for min-heap (smallest key first)
                match other.key.cmp(&self.key) {
                    std::cmp::Ordering::Equal => {
                        // Max-heap for raw_seq (largest logical seq first)
                        let self_raw = self.seq & !TOMBSTONE_BIT;
                        let other_raw = other.seq & !TOMBSTONE_BIT;
                        self_raw
                            .cmp(&other_raw)
                            .then_with(|| self.source_idx.cmp(&other.source_idx))
                    }
                    ord => ord,
                }
            }
        }

        let mut streams = Vec::new();
        for sst in inputs {
            streams.push(sst.stream().await?);
        }

        let mut heap = std::collections::BinaryHeap::new();
        for (i, stream) in streams.iter_mut().enumerate() {
            if let Some((key, value, seq, tx)) = stream.next_entry().await? {
                heap.push(HeapItem {
                    key,
                    value,
                    seq,
                    tx,
                    source_idx: i,
                });
            }
        }

        let mut builder =
            SstableBuilder::create_with_key_manager(output_path, self.key_manager.clone()).await?;
        let mut last_key: Option<bytes::Bytes> = None;
        let mut floor_emitted = false;
        let mut processed_count = 0;

        // Token-Bucket-State für I/O-Rate-Limiting (§4.12 C-2)
        let mut io_token_bytes_written: u64 = 0;
        let mut io_token_last_reset = std::time::Instant::now();

        while let Some(item) = heap.pop() {
            if let Some(ct) = cancel_token {
                if ct.is_cancelled() {
                    return Err(memfuse_core::MemFuseError::Internal(
                        "Compaction cancelled during merge stream processing".into(),
                    ));
                }
            }

            processed_count += 1;
            if processed_count % self.config.yield_threshold == 0 {
                // PERF-3: System Pressure-Awareness
                // Check pressure_rx at batch boundaries between merge iterations.
                // Critical pressure level triggers a 50ms backpressure sleep delay to reduce NVMe / CPU contention.
                if let Some(ref pressure_rx) = self.pressure_rx {
                    let level = pressure_rx.borrow().pressure_level;
                    if level == crate::system_pressure::PressureLevel::Critical {
                        tracing::warn!("Compaction merge delayed due to Critical system pressure.");
                        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                    } else if level == crate::system_pressure::PressureLevel::Elevated {
                        tracing::debug!("Compaction merge active under Elevated system pressure.");
                    }
                }

                // FIND-STO-002: Budgeted Compaction
                // Apply memory backpressure to prevent Compaction from OOMing the system
                if !self.budget.has_memory_capacity() {
                    while !self.budget.has_memory_capacity() {
                        if let Some(ct) = cancel_token {
                            if ct.is_cancelled() {
                                return Err(memfuse_core::MemFuseError::Internal(
                                    "Compaction cancelled during memory budget wait".into(),
                                ));
                            }
                        }
                        tracing::warn!("Compaction engine paused due to memory budget exhaustion.");
                        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                    }
                    // Yield after blocking to give other tasks a fair chance
                    tokio::task::yield_now().await;
                }
                // When budget is fine: no yield, just continue — the budget check above
                // already provided cooperative scheduling opportunities via the sleep loop.
            }

            let is_tombstone = (item.seq & TOMBSTONE_BIT) != 0;
            let raw_seq = item.seq & !TOMBSTONE_BIT;

            if last_key.as_ref() != Some(&item.key) {
                floor_emitted = false;
            }

            // LSM Retention Rule:
            // Keep all versions with raw_seq >= min_snapshot_seq (visible to active or future snapshots)
            // PLUS the newest version with raw_seq < min_snapshot_seq (the "floor" version).
            // All further, older versions for the key below min_snapshot_seq are discarded.
            let keep = if raw_seq >= min_snapshot_seq {
                true
            } else if !floor_emitted {
                floor_emitted = true;
                true
            } else {
                false
            };

            if keep {
                // O(1): Bytes::clone is an Arc refcount increment
                last_key = Some(item.key.clone());

                // FIND-STO-001: Tombstone-Retention
                // Only GC tombstones during FULL compaction when no snapshot references them
                // and no older SSTables outside this compaction round can contain older values.
                // NOTE: If raw_seq < min_snapshot_seq and is_full_compaction is true, this tombstone
                // is the floor version below min_snapshot_seq. Being a tombstone below min_snapshot_seq
                // during full compaction, no active snapshot references a non-deleted version below it
                // and no older SSTables exist, so GC'ing it is safe.
                let should_gc_tombstone =
                    is_tombstone && is_full_compaction && raw_seq < min_snapshot_seq;
                if !should_gc_tombstone {
                    let entry_bytes = (item.key.len() + item.value.len() + 16) as u64; // +16 für Overhead
                    builder
                        .add(&item.key, &item.value, item.seq, item.tx)
                        .await?;

                    // Token-Bucket I/O Rate Limiting (plattformneutral, außerhalb aller Write-Locks)
                    if let Some(max_bps) = self.config.max_io_bytes_per_second {
                        if max_bps > 0 {
                            io_token_bytes_written += entry_bytes;
                            let elapsed = io_token_last_reset.elapsed();
                            let target = std::time::Duration::from_secs_f64(
                                io_token_bytes_written as f64 / max_bps as f64,
                            );
                            if target > elapsed {
                                let delay =
                                    (target - elapsed).min(std::time::Duration::from_millis(100));
                                // INVARIANTE: Kein MVCC-Write-Lock aktiv an dieser Stelle (merge läuft lock-frei).
                                // Verifiziert durch Lektüre von merge_sstables() — kein RwLock::write() im Merge-Loop.
                                tokio::time::sleep(delay).await;
                                // Deduct allowed bytes based on actual elapsed time instead of wiping to 0
                                let total_elapsed = io_token_last_reset.elapsed();
                                let allowed_bytes =
                                    (total_elapsed.as_secs_f64() * max_bps as f64) as u64;
                                io_token_bytes_written =
                                    io_token_bytes_written.saturating_sub(allowed_bytes);
                                io_token_last_reset = std::time::Instant::now();
                            }
                        }
                    }
                }
            }

            // Immediately fetch the next item from the source stream
            if let Some((key, value, seq, tx)) = streams[item.source_idx].next_entry().await? {
                heap.push(HeapItem {
                    key,
                    value,
                    seq,
                    tx,
                    source_idx: item.source_idx,
                });
            }
        }
        builder.finish().await?;
        Ok(())
    }

    /// Generates a unique SSTable file path using microsecond timestamp.
    fn generate_sst_path(&self, data_path: &std::path::Path) -> Result<PathBuf> {
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| memfuse_core::MemFuseError::Storage(format!("System clock error: {}", e)))?
            .as_micros();
        let count = self
            .compaction_counter
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(data_path.join(format!("sst-compact-{:020}-{:04}.sst", id, count % 10000)))
    }

    /// Runs the background compaction loop.
    ///
    /// Periodically checks if compaction is needed and performs it.
    /// Designed to be spawned via `tokio::spawn`.
    pub async fn run_loop(
        self: Arc<Self>,
        sstables: Arc<RwLock<Vec<Arc<SstableReader>>>>,
        data_path: PathBuf,
        shutdown: tokio_util::sync::CancellationToken,
    ) {
        loop {
            // Check for shutdown signal
            if shutdown.is_cancelled() {
                tracing::info!("Compaction engine received shutdown signal");
                break;
            }

            // Wait for interval OR shutdown signal
            tokio::select! {
                _ = tokio::time::sleep(self.config.check_interval) => {
                    match self.maybe_compact_with_cancel(&sstables, &data_path, Some(&shutdown)).await {
                        Ok(true) => {
                            tracing::debug!("Background compaction cycle completed successfully");
                        }
                        Ok(false) => {
                            tracing::trace!("No compaction needed");
                            if let Some(ref manifest) = self.manifest {
                                let current_live_entries: Vec<crate::manifest::ManifestEntry> = {
                                    let ssts = sstables.read().await;
                                    ssts.iter()
                                        .map(|sst| crate::manifest::ManifestEntry::Add {
                                            path: sst.file_path().to_path_buf(),
                                            max_tx: sst.metadata().max_tx_id,
                                        })
                                        .collect()
                                };
                                if let Err(e) = manifest
                                    .maybe_rollover(
                                        &current_live_entries,
                                        crate::manifest::DEFAULT_ROLLOVER_THRESHOLD_BYTES,
                                    )
                                    .await
                                {
                                    tracing::warn!("Periodic MANIFEST rollover check failed: {e}");
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!("Background compaction failed: {}", e);
                        }
                    }
                }
                _ = shutdown.cancelled() => {
                    tracing::info!("Compaction engine shutting down via signal");
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sstable::{create_block_cache, SstableBuilder};
    use memfuse_core::StorageEngine;
    use tempfile::TempDir;

    async fn create_test_sstable(
        dir: &std::path::Path,
        name: &str,
        entries: &[(&[u8], &[u8], u64)],
        bc: Arc<BlockCache>,
    ) -> Arc<SstableReader> {
        let path = dir.join(name);
        let mut builder = SstableBuilder::create(&path).await.expect("create sst"); // expect
        for (k, v, seq) in entries {
            builder.add(k, v, *seq, *seq).await.expect("add entry"); // expect
        }
        builder.finish().await.expect("finish sst"); // expect
        Arc::new(SstableReader::open(&path, bc).await.expect("open sst")) // expect
    }

    #[test]
    fn prop_compaction_tombstone_masking_latest_operation_wins() {
        use proptest::prelude::*;

        #[derive(Debug, Clone)]
        enum Op {
            Put(u8),
            Delete,
        }

        let op_seq_strategy = proptest::collection::vec(
            prop_oneof![any::<u8>().prop_map(Op::Put), Just(Op::Delete),],
            1..30,
        );

        proptest!(ProptestConfig::with_cases(30), |(ops in op_seq_strategy)| {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            rt.block_on(async {
                let tmp = TempDir::new().unwrap();
                let registry = Arc::new(SnapshotRegistry::new());
                let bc = create_block_cache(1);
                let engine = CompactionEngine::new(
                    CompactionConfig::default(),
                    registry,
                    Arc::clone(&bc),
                    None,
                    Arc::new(memfuse_core::ResourceTracker::new(
                        memfuse_core::ResourceBudget {
                            memory_limit: 1024 * 1024,
                        },
                    )),
                    None,
                );

                let mut input_ssts = Vec::new();
                for (idx, op) in ops.iter().enumerate() {
                    let seq = idx as u64 + 1;
                    let sst_name = format!("sst_{idx}.sst");
                    let sst = match op {
                        Op::Put(v) => {
                            create_test_sstable(
                                tmp.path(),
                                &sst_name,
                                &[(b"target_key", &[*v], seq)],
                                Arc::clone(&bc),
                            )
                            .await
                        }
                        Op::Delete => {
                            create_test_sstable(
                                tmp.path(),
                                &sst_name,
                                &[(b"target_key", &[], seq | TOMBSTONE_BIT)],
                                Arc::clone(&bc),
                            )
                            .await
                        }
                    };
                    input_ssts.push(sst);
                }

                let output_path = tmp.path().join("merged.sst");
                engine
                    .merge_sstables(&input_ssts, &output_path, u64::MAX, true)
                    .await
                    .unwrap();

                let reader = SstableReader::open(&output_path, Arc::clone(&bc))
                    .await
                    .unwrap();

                let last_op = ops.last().unwrap();
                let res = reader.get(b"target_key").await.unwrap();

                match last_op {
                    Op::Put(expected_v) => {
                        prop_assert!(
                            res.is_some(),
                            "Latest operation was PUT but key was not found after compaction"
                        );
                        let (val, seq, _) = res.unwrap();
                        prop_assert_eq!(
                            val.as_ref(),
                            &[*expected_v],
                            "Merged value must match latest PUT value"
                        );
                        prop_assert_eq!(
                            seq & TOMBSTONE_BIT,
                            0,
                            "TOMBSTONE_BIT must NOT be set for latest PUT"
                        );
                    }
                    Op::Delete => {
                        prop_assert!(
                            res.is_none(),
                            "Latest operation was DELETE, key must be GC'd after full compaction"
                        );
                    }
                }

                Ok(())
            }).unwrap();
        });
    }

    #[tokio::test]
    async fn test_compaction_candidate_selection_follows_chronological_order() {
        let tmp = TempDir::new().expect("temp dir"); // expect
        let registry = Arc::new(SnapshotRegistry::new());
        let bc = create_block_cache(1);
        let config = CompactionConfig {
            min_sstables_per_tier: 2,
            ..CompactionConfig::default()
        };
        let engine = CompactionEngine::new(
            config,
            registry,
            Arc::clone(&bc),
            None,
            Arc::new(memfuse_core::ResourceTracker::new(
                memfuse_core::ResourceBudget {
                    memory_limit: 1024 * 1024,
                },
            )),
            None,
        );

        // Create older SSTable with many entries (large file size) but small max_seq (seq=10)
        let mut large_entries = Vec::new();
        large_entries.push((b"key-1".as_ref(), b"old_val".as_ref(), 10u64));
        for _ in 0..100 {
            large_entries.push((b"pad", b"large_padding_data_to_increase_file_size", 10u64));
        }
        let sst_old_large = create_test_sstable(
            tmp.path(),
            "sst_old_large.sst",
            &large_entries,
            Arc::clone(&bc),
        )
        .await;

        // Create newer SSTable with few entries (small file size) but larger max_seq (seq=20)
        let sst_new_small = create_test_sstable(
            tmp.path(),
            "sst_new_small.sst",
            &[(b"key-1", b"new_val", 20)],
            Arc::clone(&bc),
        )
        .await;

        assert!(
            sst_old_large.metadata().file_size > sst_new_small.metadata().file_size,
            "Old SSTable must be larger in file size than new SSTable"
        );

        let sstables = vec![Arc::clone(&sst_old_large), Arc::clone(&sst_new_small)];

        // Select candidates
        let candidates = engine
            .select_compaction_candidates(&sstables)
            .expect("candidates selected");

        // Collect inputs in selected candidate order
        let candidate_ssts = candidates;

        // Perform merge
        let output = tmp.path().join("merged_chronological.sst");
        engine
            .merge_sstables(&candidate_ssts, &output, u64::MAX, true)
            .await
            .expect("merge");

        let reader = SstableReader::open(&output, Arc::clone(&bc))
            .await
            .expect("open merged");

        let (val, seq, _) = reader.get(b"key-1").await.expect("get").expect("exists");
        assert_eq!(
            val.as_ref(),
            b"new_val",
            "Merge must preserve newer version regardless of file size"
        );
        assert_eq!(seq, 20);
    }

    #[tokio::test]
    async fn test_mvcc_retention_floor_version_retained_for_snapshot() {
        let tmp = TempDir::new().expect("temp dir");
        let registry = Arc::new(SnapshotRegistry::new());
        let bc = create_block_cache(1);
        let engine = CompactionEngine::new(
            CompactionConfig::default(),
            registry,
            Arc::clone(&bc),
            None,
            Arc::new(memfuse_core::ResourceTracker::new(
                memfuse_core::ResourceBudget {
                    memory_limit: 1024 * 1024,
                },
            )),
            None,
        );

        // Two SSTables with versions 100 and 90 of key "k1"
        // Active snapshot is at min_snapshot_seq = 95
        let sst1 = create_test_sstable(
            tmp.path(),
            "sst1.sst",
            &[(b"k1", b"v100", 100)],
            Arc::clone(&bc),
        )
        .await;

        let sst2 = create_test_sstable(
            tmp.path(),
            "sst2.sst",
            &[(b"k1", b"v90", 90)],
            Arc::clone(&bc),
        )
        .await;

        let output = tmp.path().join("merged_mvcc2.sst");
        engine
            .merge_sstables(&[sst1, sst2], &output, 95, true)
            .await
            .expect("merge");

        let reader = SstableReader::open(&output, Arc::clone(&bc))
            .await
            .expect("open merged");
        let entries = reader.iter().await.expect("iter");

        // Both versions (100 and 90) must be retained:
        // seq 100 >= 95 (kept), seq 90 < 95 (kept as floor version)
        assert_eq!(
            entries.len(),
            2,
            "Both seq 100 and floor version 90 must be retained"
        );
        assert_eq!(entries[0].0.as_ref(), b"k1");
        assert_eq!(entries[0].2, 100);
        assert_eq!(entries[1].0.as_ref(), b"k1");
        assert_eq!(entries[1].2, 90);
    }

    #[tokio::test]
    async fn test_mvcc_retention_older_versions_below_floor_discarded() {
        let tmp = TempDir::new().expect("temp dir");
        let registry = Arc::new(SnapshotRegistry::new());
        let bc = create_block_cache(1);
        let engine = CompactionEngine::new(
            CompactionConfig::default(),
            registry,
            Arc::clone(&bc),
            None,
            Arc::new(memfuse_core::ResourceTracker::new(
                memfuse_core::ResourceBudget {
                    memory_limit: 1024 * 1024,
                },
            )),
            None,
        );

        // Three SSTables with versions 100, 90, 80 of key "k1"
        // Active snapshot is at min_snapshot_seq = 95
        let sst1 = create_test_sstable(
            tmp.path(),
            "sst1.sst",
            &[(b"k1", b"v100", 100)],
            Arc::clone(&bc),
        )
        .await;

        let sst2 = create_test_sstable(
            tmp.path(),
            "sst2.sst",
            &[(b"k1", b"v90", 90)],
            Arc::clone(&bc),
        )
        .await;

        let sst3 = create_test_sstable(
            tmp.path(),
            "sst3.sst",
            &[(b"k1", b"v80", 80)],
            Arc::clone(&bc),
        )
        .await;

        let output = tmp.path().join("merged_mvcc3.sst");
        engine
            .merge_sstables(&[sst1, sst2, sst3], &output, 95, true)
            .await
            .expect("merge");

        let reader = SstableReader::open(&output, Arc::clone(&bc))
            .await
            .expect("open merged");
        let entries = reader.iter().await.expect("iter");

        // Only versions 100 (>= 95) and 90 (floor version for < 95) must be retained.
        // Version 80 (< 95 and floor already emitted) must be discarded.
        assert_eq!(
            entries.len(),
            2,
            "Only seq 100 and floor version 90 must be retained, seq 80 discarded"
        );
        assert_eq!(entries[0].0.as_ref(), b"k1");
        assert_eq!(entries[0].2, 100);
        assert_eq!(entries[1].0.as_ref(), b"k1");
        assert_eq!(entries[1].2, 90);
    }

    #[tokio::test]
    async fn test_merge_deduplication() {
        let tmp = TempDir::new().expect("temp dir"); // expect
        let registry = Arc::new(SnapshotRegistry::new());
        let bc = create_block_cache(1);
        let engine = CompactionEngine::new(
            CompactionConfig::default(),
            registry,
            Arc::clone(&bc),
            None,
            Arc::new(memfuse_core::ResourceTracker::new(
                memfuse_core::ResourceBudget {
                    memory_limit: 1024 * 1024,
                },
            )),
            None,
        );

        // Two SSTables with overlapping keys
        let sst1 = create_test_sstable(
            tmp.path(),
            "sst1.sst",
            &[(b"key-a", b"val-1", 1), (b"key-b", b"val-2", 2)],
            Arc::clone(&bc),
        )
        .await;

        let sst2 = create_test_sstable(
            tmp.path(),
            "sst2.sst",
            &[(b"key-a", b"val-3", 3), (b"key-c", b"val-4", 4)],
            Arc::clone(&bc),
        )
        .await;

        let output = tmp.path().join("merged.sst");
        engine
            .merge_sstables(&[sst1, sst2], &output, u64::MAX, true)
            .await
            .expect("merge"); // expect

        let reader = SstableReader::open(&output, Arc::clone(&bc))
            .await
            .expect("open merged"); // expect
        let entries = reader.iter().await.expect("iter"); // expect

        // key-a should have the newer value (seq=3)
        assert_eq!(entries.len(), 3); // key-a, key-b, key-c
        assert_eq!(entries[0].0.as_ref(), b"key-a");
        assert_eq!(entries[0].1.as_ref(), b"val-3");
        assert_eq!(entries[0].2, 3);
    }

    #[tokio::test]
    async fn test_tombstone_gc_stream_advancement_preserves_subsequent_keys() {
        let tmp = TempDir::new().expect("temp dir"); // expect
        let registry = Arc::new(SnapshotRegistry::new());
        let bc = create_block_cache(1);
        let engine = CompactionEngine::new(
            CompactionConfig::default(),
            registry,
            Arc::clone(&bc),
            None,
            Arc::new(memfuse_core::ResourceTracker::new(
                memfuse_core::ResourceBudget {
                    memory_limit: 1024 * 1024,
                },
            )),
            None,
        );

        let tombstone_seq = 5 | TOMBSTONE_BIT;
        // Key A is tombstone, followed in same SSTable stream by B, C, D
        let sst1 = create_test_sstable(
            tmp.path(),
            "sst1.sst",
            &[
                (b"key-a", b"", tombstone_seq),
                (b"key-b", b"val-b", 10),
                (b"key-c", b"val-c", 11),
                (b"key-d", b"val-d", 12),
            ],
            Arc::clone(&bc),
        )
        .await;

        let output = tmp.path().join("compacted_stream.sst");

        // min_snapshot_seq=100 -> key-a tombstone is GC'd during full compaction
        engine
            .merge_sstables(&[sst1], &output, 100, true)
            .await
            .expect("merge"); // expect

        let reader = SstableReader::open(&output, Arc::clone(&bc))
            .await
            .expect("open"); // expect
        let entries = reader.iter().await.expect("iter"); // expect

        // key-a GC'd, key-b, key-c, key-d MUST be present and unchanged
        assert_eq!(
            entries.len(),
            3,
            "B, C, D must be preserved after tombstone GC"
        );
        assert_eq!(entries[0].0.as_ref(), b"key-b");
        assert_eq!(entries[0].1.as_ref(), b"val-b");
        assert_eq!(entries[1].0.as_ref(), b"key-c");
        assert_eq!(entries[1].1.as_ref(), b"val-c");
        assert_eq!(entries[2].0.as_ref(), b"key-d");
        assert_eq!(entries[2].1.as_ref(), b"val-d");
    }

    #[tokio::test]
    async fn test_tombstone_gc() {
        let tmp = TempDir::new().expect("temp dir"); // expect
        let registry = Arc::new(SnapshotRegistry::new());
        let bc = create_block_cache(1);
        let engine = CompactionEngine::new(
            CompactionConfig::default(),
            registry,
            Arc::clone(&bc),
            None,
            Arc::new(memfuse_core::ResourceTracker::new(
                memfuse_core::ResourceBudget {
                    memory_limit: 1024 * 1024,
                },
            )),
            None,
        );

        let tombstone_seq = 5 | TOMBSTONE_BIT;
        let sst1 = create_test_sstable(
            tmp.path(),
            "sst1.sst",
            &[(b"alive", b"val", 10), (b"dead", b"", tombstone_seq)],
            Arc::clone(&bc),
        )
        .await;

        let output = tmp.path().join("compacted.sst");

        // min_snapshot_seq=100 → tombstone at seq=5 is safe to GC
        engine
            .merge_sstables(&[sst1], &output, 100, true)
            .await
            .expect("merge"); // expect

        let reader = SstableReader::open(&output, Arc::clone(&bc))
            .await
            .expect("open"); // expect
        let entries = reader.iter().await.expect("iter"); // expect

        assert_eq!(entries.len(), 1); // Only "alive" remains
        assert_eq!(entries[0].0.as_ref(), b"alive");
    }

    #[tokio::test]
    async fn test_tombstone_preserved_with_active_snapshot() {
        let tmp = TempDir::new().expect("temp dir"); // expect
        let registry = Arc::new(SnapshotRegistry::new());
        let bc = create_block_cache(1);
        let engine = CompactionEngine::new(
            CompactionConfig::default(),
            registry,
            Arc::clone(&bc),
            None,
            Arc::new(memfuse_core::ResourceTracker::new(
                memfuse_core::ResourceBudget {
                    memory_limit: 1024 * 1024,
                },
            )),
            None,
        );

        let tombstone_seq = 5 | TOMBSTONE_BIT;
        let sst1 = create_test_sstable(
            tmp.path(),
            "sst1.sst",
            &[(b"alive", b"val", 10), (b"dead", b"", tombstone_seq)],
            Arc::clone(&bc),
        )
        .await;

        let output = tmp.path().join("compacted.sst");

        // min_snapshot_seq=2 → tombstone at seq=5 is NOT safe to GC
        engine
            .merge_sstables(&[sst1], &output, 2, true)
            .await
            .expect("merge"); // expect

        let reader = SstableReader::open(&output, Arc::clone(&bc))
            .await
            .expect("open"); // expect
        let entries = reader.iter().await.expect("iter"); // expect

        assert_eq!(entries.len(), 2); // Both preserved
    }

    #[tokio::test]
    async fn test_maybe_compact_full_cycle() {
        let tmp = TempDir::new().expect("temp dir"); // expect
        let registry = Arc::new(SnapshotRegistry::new());
        let bc = create_block_cache(1);
        let config = CompactionConfig {
            min_sstables_per_tier: 2, // Low threshold for testing
            ..Default::default()
        };
        let engine = CompactionEngine::new(
            config,
            registry,
            Arc::clone(&bc),
            None,
            Arc::new(memfuse_core::ResourceTracker::new(
                memfuse_core::ResourceBudget {
                    memory_limit: 1024 * 1024,
                },
            )),
            None,
        );

        // Create 3 small SSTables of similar size
        let sstables = Arc::new(RwLock::new(Vec::new()));
        for i in 0..3u8 {
            let sst = create_test_sstable(
                tmp.path(),
                &format!("sst-{}.sst", i),
                &[
                    (
                        format!("key-{}-a", i).as_bytes(),
                        b"val",
                        (i as u64) * 2 + 1,
                    ),
                    (
                        format!("key-{}-b", i).as_bytes(),
                        b"val",
                        (i as u64) * 2 + 2,
                    ),
                ],
                Arc::clone(&bc),
            )
            .await;
            sstables.write().await.push(sst);
        }

        assert_eq!(sstables.read().await.len(), 3);

        // Run compaction
        let compacted = engine
            .maybe_compact(&sstables, tmp.path())
            .await
            .expect("compact"); // expect

        assert!(compacted, "Compaction should have occurred");

        // After compaction: fewer SSTables, all data still accessible
        let ssts = sstables.read().await;
        assert!(
            ssts.len() < 3,
            "Should have fewer SSTables after compaction"
        );

        // Verify all data is present in the compacted result
        let last_sst = &ssts[ssts.len() - 1];
        let entries = last_sst.iter().await.expect("iter"); // expect
        assert_eq!(entries.len(), 6); // 3 SSTables × 2 entries each
    }

    #[tokio::test]
    async fn test_no_compaction_below_threshold() {
        let tmp = TempDir::new().expect("temp dir"); // expect
        let registry = Arc::new(SnapshotRegistry::new());
        let bc = create_block_cache(1);
        let config = CompactionConfig {
            min_sstables_per_tier: 4,
            ..Default::default()
        };
        let engine = CompactionEngine::new(
            config,
            registry,
            Arc::clone(&bc),
            None,
            Arc::new(memfuse_core::ResourceTracker::new(
                memfuse_core::ResourceBudget {
                    memory_limit: 1024 * 1024,
                },
            )),
            None,
        );

        let sstables = Arc::new(RwLock::new(Vec::new()));
        for i in 0..2u8 {
            let sst = create_test_sstable(
                tmp.path(),
                &format!("sst-{}.sst", i),
                &[(format!("key-{}", i).as_bytes(), b"val", i as u64 + 1)],
                Arc::clone(&bc),
            )
            .await;
            sstables.write().await.push(sst);
        }

        let compacted = engine
            .maybe_compact(&sstables, tmp.path())
            .await
            .expect("compact"); // expect

        assert!(!compacted, "Should not compact with only 2 SSTables");
        assert_eq!(sstables.read().await.len(), 2);
    }

    #[tokio::test]
    async fn test_compaction_aborts_on_concurrent_modification() {
        let tmp = TempDir::new().expect("temp dir"); // expect
        let registry = Arc::new(SnapshotRegistry::new());
        let bc = create_block_cache(1);
        let config = CompactionConfig {
            min_sstables_per_tier: 2,
            ..Default::default()
        };
        let engine = CompactionEngine::new(
            config,
            registry,
            Arc::clone(&bc),
            None,
            Arc::new(memfuse_core::ResourceTracker::new(
                memfuse_core::ResourceBudget {
                    memory_limit: 1024 * 1024,
                },
            )),
            None,
        );

        let sstables = Arc::new(RwLock::new(Vec::new()));
        for i in 0..2u8 {
            let sst = create_test_sstable(
                tmp.path(),
                &format!("sst-{}.sst", i),
                &[(format!("key-{}", i).as_bytes(), b"val", i as u64 + 1)],
                Arc::clone(&bc),
            )
            .await;
            sstables.write().await.push(sst);
        }

        // Simulate concurrent modification by clearing sstables right after selection or before swap
        // We test that retain/Arc::ptr_eq check properly detects missing input sstables
        let candidates = engine
            .select_compaction_candidates(&sstables.read().await)
            .unwrap(); // unwrap
        assert_eq!(candidates.len(), 2);

        // Remove one sstable concurrently
        sstables.write().await.pop();

        // Run maybe_compact, should return Ok(false) or abort cleanly without panic
        let result = engine.maybe_compact(&sstables, tmp.path()).await.unwrap(); // unwrap
        assert!(
            !result,
            "Compaction should be aborted when candidates are modified"
        );
    }

    #[tokio::test]
    async fn test_compaction_removes_uuid_sidecar_files() {
        let tmp = TempDir::new().expect("temp dir");
        let registry = Arc::new(SnapshotRegistry::new());
        let bc = create_block_cache(1);
        let config = CompactionConfig {
            min_sstables_per_tier: 2,
            ..Default::default()
        };
        let engine = CompactionEngine::new(
            config,
            registry,
            Arc::clone(&bc),
            None,
            Arc::new(memfuse_core::ResourceTracker::new(
                memfuse_core::ResourceBudget {
                    memory_limit: 1024 * 1024,
                },
            )),
            None,
        );

        let sstables = Arc::new(RwLock::new(Vec::new()));
        let mut old_sst_paths = Vec::new();
        let mut uuid_paths = Vec::new();

        for i in 0..2u8 {
            let name = format!("sst-{}.sst", i);
            let sst = create_test_sstable(
                tmp.path(),
                &name,
                &[(format!("key-{}", i).as_bytes(), b"val", i as u64 + 1)],
                Arc::clone(&bc),
            )
            .await;
            let sst_path = sst.file_path().to_path_buf();
            old_sst_paths.push(sst_path.clone());

            let uuid_path = PathBuf::from(format!("{}.uuid", sst_path.display()));
            tokio::fs::write(&uuid_path, b"dummy-uuid-bytes")
                .await
                .expect("write dummy uuid file");
            uuid_paths.push(uuid_path);

            sstables.write().await.push(sst);
        }

        for path in &old_sst_paths {
            assert!(
                path.exists(),
                "SSTable file {:?} must exist before compaction",
                path
            );
        }
        for uuid_path in &uuid_paths {
            assert!(
                uuid_path.exists(),
                "UUID sidecar file {:?} must exist before compaction",
                uuid_path
            );
        }

        let compacted = engine
            .maybe_compact(&sstables, tmp.path())
            .await
            .expect("maybe_compact should succeed");
        assert!(compacted, "Compaction should have occurred");

        for path in &old_sst_paths {
            assert!(
                !path.exists(),
                "Old SSTable file {:?} should be deleted after compaction",
                path
            );
        }
        for uuid_path in &uuid_paths {
            assert!(
                !uuid_path.exists(),
                "UUID sidecar file {:?} should be deleted after compaction",
                uuid_path
            );
        }
    }

    #[tokio::test]
    async fn test_compaction_swap_restores_shadowing_order_without_restart() {
        let tmp = TempDir::new().expect("temp dir");
        let registry = Arc::new(SnapshotRegistry::new());
        let bc = create_block_cache(1);
        let config = CompactionConfig {
            min_sstables_per_tier: 2,
            ..CompactionConfig::default()
        };
        let engine = CompactionEngine::new(
            config,
            registry,
            Arc::clone(&bc),
            None,
            Arc::new(memfuse_core::ResourceTracker::new(
                memfuse_core::ResourceBudget {
                    memory_limit: 1024 * 1024,
                },
            )),
            None,
        );

        // SSTable A (oldest candidate): key "k1" -> "v_old", seq 10
        let sst_a = create_test_sstable(
            tmp.path(),
            "sst_a.sst",
            &[(b"k1", b"v_old", 10)],
            Arc::clone(&bc),
        )
        .await;

        // SSTable C (non-input, intermediate seq, larger size so it is in a separate size tier): key "k1" -> "v_inter", seq 15
        let mut entries_c = vec![(b"k1".as_ref(), b"v_inter".as_ref(), 15u64)];
        for _ in 0..100 {
            entries_c.push((b"padding_key", b"padding_value_large_file", 15u64));
        }
        let sst_c = create_test_sstable(tmp.path(), "sst_c.sst", &entries_c, Arc::clone(&bc)).await;

        // SSTable B (newer candidate): key "k1" -> "v_new", seq 20
        let sst_b = create_test_sstable(
            tmp.path(),
            "sst_b.sst",
            &[(b"k1", b"v_new", 20)],
            Arc::clone(&bc),
        )
        .await;

        // sstables list initially sorted by max_seq: [A (seq 10), C (seq 15), B (seq 20)]
        let sstables = Arc::new(RwLock::new(vec![
            Arc::clone(&sst_a),
            Arc::clone(&sst_c),
            Arc::clone(&sst_b),
        ]));

        // Run production maybe_compact directly to trigger candidate selection, merge, and atomic swap
        let compacted = engine
            .maybe_compact(&sstables, tmp.path())
            .await
            .expect("maybe_compact");
        assert!(
            compacted,
            "Compaction should be triggered for tier {{A, B}}"
        );

        // Read in reverse order (simulating get_at_seq / scan)
        let ssts_read = sstables.read().await;
        let mut found_val = None;
        for sst in ssts_read.iter().rev() {
            if let Some((val, seq, tx)) = sst.get(b"k1").await.expect("get") {
                if seq & !TOMBSTONE_BIT <= 20 && tx <= 20 {
                    found_val = Some(val);
                    break;
                }
            }
        }

        assert_eq!(
            found_val.as_deref(),
            Some(&b"v_new"[..]),
            "Reader must see newer value from merged SSTable rather than stale value from non-input SSTable"
        );

        // Verify list is strictly sorted ascending by max_seq
        assert!(
            ssts_read
                .windows(2)
                .all(|w| w[0].metadata().max_seq <= w[1].metadata().max_seq),
            "SSTable list must be strictly sorted by max_seq"
        );
    }

    #[tokio::test]
    async fn test_compaction_swap_debug_assert_detects_unsorted_list() {
        let tmp = TempDir::new().expect("temp dir");
        let bc = create_block_cache(1);

        let sst_c = create_test_sstable(
            tmp.path(),
            "sst_c.sst",
            &[(b"k1", b"v_inter", 15)],
            Arc::clone(&bc),
        )
        .await;

        let sst_m = create_test_sstable(
            tmp.path(),
            "sst_m.sst",
            &[(b"k1", b"v_new", 20)],
            Arc::clone(&bc),
        )
        .await;

        // Intentionally create unsorted list: [M (seq 20), C (seq 15)]
        let unsorted_ssts = [sst_m, sst_c];

        let is_sorted = unsorted_ssts.windows(2).all(|w| {
            (w[0].metadata().max_seq & !TOMBSTONE_BIT) <= (w[1].metadata().max_seq & !TOMBSTONE_BIT)
        });

        assert!(
            !is_sorted,
            "Unsorted SSTable list must fail the max_seq order check"
        );
    }

    #[test]
    fn test_generate_sst_path_uniqueness() {
        let tmp = TempDir::new().expect("temp dir"); // expect
        let engine = CompactionEngine::new(
            CompactionConfig::default(),
            Arc::new(SnapshotRegistry::new()),
            create_block_cache(1),
            None,
            Arc::new(memfuse_core::ResourceTracker::new(
                memfuse_core::ResourceBudget {
                    memory_limit: 1024 * 1024,
                },
            )),
            None,
        );
        let path1 = engine.generate_sst_path(tmp.path()).expect("path 1"); // expect
        let path2 = engine.generate_sst_path(tmp.path()).expect("path 2"); // expect
        assert_ne!(
            path1, path2,
            "Rapid sequential calls must produce distinct SSTable paths"
        );
    }
    #[tokio::test(flavor = "multi_thread")]
    async fn test_compaction_stress_and_gc() {
        use crate::lsm::{LsmConfig, LsmStorage};
        use memfuse_core::TxId;
        use std::sync::atomic::{AtomicBool, Ordering};

        let tmp = TempDir::new().expect("temp dir"); // expect
        let config = LsmConfig {
            path: tmp.path().to_path_buf(),
            memtable_size_limit: 64 * 1024, // 64KB - very small to force flushes
            max_ram_mb: 64,
            tx_timeout: Duration::from_secs(60),
            compaction: CompactionConfig {
                min_sstables_per_tier: 3, // Small tier to trigger compaction often
                size_ratio: 2.0,
                check_interval: Duration::from_millis(100), // Fast check
                yield_threshold: 100,
                max_memory_bytes: Some(128 * 1024 * 1024),
                max_io_bytes_per_second: None,
            },
            encryption_passphrase: None,
            ..Default::default()
        };

        let storage = Arc::new(LsmStorage::new(config).await.expect("create storage")); // expect
        let running = Arc::new(AtomicBool::new(true));

        // 1. Parallel Reader Task [INV-C2]
        let storage_clone = Arc::clone(&storage);
        let running_clone = Arc::clone(&running);
        let reader_handle = tokio::spawn(async move {
            let mut rng = 0u64;
            while running_clone.load(Ordering::Relaxed) {
                rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
                let key_idx = rng % 10000;
                let key = format!("doc-{:04}", key_idx);

                // Randomly perform get or scan to test stability during swaps
                if rng.is_multiple_of(2) {
                    let _ = storage_clone.get(key.as_bytes()).await;
                } else {
                    let _ = storage_clone.scan_prefix(b"doc-").await;
                }
                tokio::task::yield_now().await;
            }
        });

        // 2. Initial Inserts
        for i in 0..1000 {
            let tx = TxId::new(i as u64);
            let key = format!("doc-{:04}", i);
            let val = vec![(i % 255) as u8; 100];
            storage.put(tx, key.as_bytes(), &val).await.expect("put"); // expect
            storage.commit(tx).await.expect("commit"); // expect
        }

        // 3. Register a Snapshot [INV-C1]
        let snapshot_seq = storage.last_seq_no().await.expect("last_seq_no"); // expect
        let _guard = storage.snapshot_registry.register(snapshot_seq);

        // 4. Heavy Load: 10,000 Inserts to trigger churn and background compaction
        for i in 0..10000 {
            let tx = TxId::new(1000 + i as u64);
            let key = format!("doc-{:04}", i);
            let val = vec![(i % 255) as u8; 100];
            storage
                .put(tx, key.as_bytes(), &val)
                .await
                .expect("put heavy"); // expect
            storage.commit(tx).await.expect("commit heavy"); // expect
        }

        // 5. Deletes
        for i in 0..5000 {
            let tx = TxId::new(20000 + i as u64);
            let key = format!("doc-{:04}", i);
            storage.delete(tx, key.as_bytes()).await.expect("delete"); // expect
            storage.commit(tx).await.expect("commit delete"); // expect
        }

        // 6. Wait for background compactions to stabilize
        let mut stabilized = false;
        for _ in 0..100 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let stats = storage.stats().await.expect("stats"); // expect
                                                               // If we have few segments, compaction is doing its job
            if stats.num_segments <= 5 {
                stabilized = true;
                break;
            }
        }

        // Stop reader and check for errors
        running.store(false, Ordering::SeqCst);
        reader_handle.await.expect("reader task panicked"); // expect

        // 7. Final Verification
        let stats = storage.stats().await.expect("final stats"); // expect
        println!(
            "Stress test finished. Final SSTable count: {}",
            stats.num_segments
        );

        // Non-deleted data MUST be present
        for i in 5000..10000 {
            let key = format!("doc-{:04}", i);
            // SAFETY: This panic is in a test-only context. A missing key after
            // compaction is a test logic error, not a production code path.
            let val = storage
                .get(key.as_bytes())
                .await
                .expect("get final") // expect
                .unwrap_or_else(|| panic!("missing key {}", key));
            assert_eq!(val[0], (i % 255) as u8);
        }

        // Deleted data MUST NOT be present in current view
        for i in 0..5000 {
            let key = format!("doc-{:04}", i);
            let val = storage.get(key.as_bytes()).await.expect("get deleted"); // expect
            assert!(val.is_none(), "Key {} should be deleted but found", key);
        }

        // SSTable count should be significantly reduced from the peak
        assert!(
            stats.num_segments <= 12,
            "Compaction failed to reduce segments: {}",
            stats.num_segments
        );
        assert!(
            stabilized,
            "Compaction didn't reach target segment count in time"
        );
    }

    #[tokio::test]
    async fn test_compaction_swap_maintains_shadowing_order_without_restart() {
        let tmp = TempDir::new().expect("temp dir");
        let registry = Arc::new(SnapshotRegistry::new());
        let bc = create_block_cache(1);
        let config = CompactionConfig {
            min_sstables_per_tier: 2,
            ..CompactionConfig::default()
        };
        let engine = CompactionEngine::new(
            config,
            registry,
            Arc::clone(&bc),
            None,
            Arc::new(memfuse_core::ResourceTracker::new(
                memfuse_core::ResourceBudget {
                    memory_limit: 1024 * 1024,
                },
            )),
            None,
        );

        // Setup 3 SSTables (A, C, B) in chronological sequence:
        // SSTable A (oldest, seq=10): [key = "val_A", key_common = "val_A_old"]
        // SSTable C (middle, seq=15): [key_common = "val_C_middle"]
        // SSTable B (newest, seq=20): [key = "val_B", key_common = "val_B_newest"]
        let sst_a = create_test_sstable(
            tmp.path(),
            "sst_a.sst",
            &[(b"key_a", b"val_A", 10), (b"key_common", b"val_A_old", 10)],
            Arc::clone(&bc),
        )
        .await;

        let sst_c = create_test_sstable(
            tmp.path(),
            "sst_c.sst",
            &[(b"key_common", b"val_C_middle", 15)],
            Arc::clone(&bc),
        )
        .await;

        let sst_b = create_test_sstable(
            tmp.path(),
            "sst_b.sst",
            &[
                (b"key_b", b"val_B", 20),
                (b"key_common", b"val_B_newest", 20),
            ],
            Arc::clone(&bc),
        )
        .await;

        // SSTable list in memory in max_seq ascending order: [A (10), C (15), B (20)]
        let sstables = Arc::new(RwLock::new(vec![
            Arc::clone(&sst_a),
            Arc::clone(&sst_c),
            Arc::clone(&sst_b),
        ]));

        // Select candidates to compact A and B (e.g. tier/fallback selection or explicit input list)
        // Here we compact input_ssts = [sst_a, sst_b] into M (max_seq = 20)
        let min_snapshot_seq = u64::MAX;
        let output_path = tmp.path().join("sst_merged_m.sst");
        engine
            .merge_sstables(
                &[sst_a.clone(), sst_b.clone()],
                &output_path,
                min_snapshot_seq,
                false,
            )
            .await
            .expect("merge A and B into M");

        let new_reader = Arc::new(
            SstableReader::open(&output_path, Arc::clone(&bc))
                .await
                .expect("open merged M"),
        );

        // Perform the swap manually inside a write guard (replicating swap logic in maybe_compact)
        let input_ssts = [sst_a, sst_b];
        {
            let mut ssts = sstables.write().await;
            let insertion_point = ssts
                .iter()
                .position(|sst| input_ssts.iter().any(|inp| Arc::ptr_eq(inp, sst)))
                .unwrap_or(ssts.len());

            ssts.retain(|sst| !input_ssts.iter().any(|inp| Arc::ptr_eq(inp, sst)));

            let insert_idx = insertion_point.min(ssts.len());
            ssts.insert(insert_idx, new_reader);

            // Re-sort SSTable list by max_seq to guarantee shadowing/visibility order.
            ssts.sort_by_key(|sst| sst.metadata().max_seq & !TOMBSTONE_BIT);

            debug_assert!(
                ssts.windows(2)
                    .all(|w| (w[0].metadata().max_seq & !TOMBSTONE_BIT)
                        <= (w[1].metadata().max_seq & !TOMBSTONE_BIT)),
                "SSTable list must be sorted by max_seq in ascending order after compaction swap"
            );
        }

        // Simulating LSM point lookup / scan (.iter().rev()):
        // Reading key_common from current sstables list in .iter().rev() order MUST find M first (seq=20),
        // returning "val_B_newest", NOT "val_C_middle" from C (seq=15).
        let ssts = sstables.read().await;
        let mut found_val = None;
        for sst in ssts.iter().rev() {
            if let Ok(Some((val, _seq, _tx))) = sst.get(b"key_common").await {
                found_val = Some(val);
                break;
            }
        }

        assert_eq!(
            found_val.expect("key_common found").as_ref(),
            b"val_B_newest",
            "Read path (.iter().rev()) must return newest value from merged SSTable M, not stale C"
        );
    }

    #[tokio::test]
    async fn test_phantom_data_after_partial_compaction() {
        let tmp = TempDir::new().expect("temp dir"); // expect
        let registry = Arc::new(SnapshotRegistry::new());
        let bc = create_block_cache(1);
        let engine = CompactionEngine::new(
            CompactionConfig {
                min_sstables_per_tier: 2,
                ..CompactionConfig::default()
            },
            registry,
            Arc::clone(&bc),
            None,
            Arc::new(memfuse_core::ResourceTracker::new(
                memfuse_core::ResourceBudget {
                    memory_limit: 1024 * 1024,
                },
            )),
            None,
        );

        // Scenario:
        // SST1 (Older): [key-1: value-1, seq 10]
        // SST2 (Newer): [key-1: tombstone, seq 20]
        // SST3 (Irrelevant): [key-x: val, seq 30]
        // Partial compaction of {SST1, SST2} must NOT GC the tombstone,
        // because we don't know if an even older version exists in some other SSTable not in the set.
        // Wait, even if we compact ALL SSTables that contain key-1, if it's not a FULL compaction
        // of the entire system, we must be conservative.

        let sst1 = create_test_sstable(
            tmp.path(),
            "sst1.sst",
            &[(b"key-1", b"value-1", 10)],
            Arc::clone(&bc),
        )
        .await;
        // seq 20 | TOMBSTONE_BIT
        let sst2 = create_test_sstable(
            tmp.path(),
            "sst2.sst",
            &[(b"key-1", &[], 20 | TOMBSTONE_BIT)],
            Arc::clone(&bc),
        )
        .await;
        let sst3 = create_test_sstable(
            tmp.path(),
            "sst3.sst",
            &[(b"key-x", b"val-x", 30)],
            Arc::clone(&bc),
        )
        .await;

        let _sstables = Arc::new(tokio::sync::RwLock::new(vec![
            Arc::clone(&sst1),
            Arc::clone(&sst2),
            Arc::clone(&sst3),
        ]));

        // min_snapshot_seq is 100 (high enough that 20 would normally be GC'd)
        let engine = Arc::new(engine);
        engine.snapshot_registry.pin(100);

        // Manually trigger merger for {sst1, sst2} -> partial compaction
        let output_path = tmp.path().join("merged.sst");
        engine
            .merge_sstables(&[sst1, sst2], &output_path, 100, false) // is_full = false
            .await
            .expect("merge"); // expect

        let reader = SstableReader::open(&output_path, Arc::clone(&bc))
            .await
            .expect("open"); // expect

        // Verify tombstone is RETAINED
        let res = reader.get(b"key-1").await.expect("get"); // expect
        assert!(
            res.is_some(),
            "Tombstone should be RETAINED in partial compaction"
        );
        let (_, seq, _) = res.unwrap(); // unwrap
        assert_eq!(seq & TOMBSTONE_BIT, TOMBSTONE_BIT);

        // Now test FULL compaction
        let output_path_full = tmp.path().join("full.sst");
        let sst1 = create_test_sstable(
            tmp.path(),
            "sst1_b.sst",
            &[(b"key-1", b"value-1", 10)],
            Arc::clone(&bc),
        )
        .await;
        let sst2 = create_test_sstable(
            tmp.path(),
            "sst2_b.sst",
            &[(b"key-1", &[], 20 | TOMBSTONE_BIT)],
            Arc::clone(&bc),
        )
        .await;

        engine
            .merge_sstables(&[sst1, sst2], &output_path_full, 100, true) // is_full = true
            .await
            .expect("merge"); // expect

        let reader_full = SstableReader::open(&output_path_full, Arc::clone(&bc))
            .await
            .expect("open"); // expect
        let res_full = reader_full.get(b"key-1").await.expect("get"); // expect
        assert!(
            res_full.is_none(),
            "Tombstone should be REMOVED in full compaction"
        );
    }

    #[tokio::test]
    async fn test_compaction_pressure_awareness() {
        use crate::system_pressure::{PressureLevel, SystemPressure};

        let tmp = TempDir::new().expect("temp dir");
        let registry = Arc::new(SnapshotRegistry::new());
        let bc = create_block_cache(1);
        let config = CompactionConfig {
            min_sstables_per_tier: 2,
            yield_threshold: 5, // Yield/check pressure every 5 entries
            ..Default::default()
        };

        let (pressure_tx, pressure_rx) = tokio::sync::watch::channel(SystemPressure {
            wal_queue_depth: 0,
            blocking_thread_utilization: 0.0,
            embedding_queue_depth: 0,
            pressure_level: PressureLevel::Normal,
        });

        let engine = CompactionEngine::new(
            config,
            registry,
            Arc::clone(&bc),
            None,
            Arc::new(memfuse_core::ResourceTracker::new(
                memfuse_core::ResourceBudget {
                    memory_limit: 1024 * 1024,
                },
            )),
            None,
        )
        .with_pressure_rx(pressure_rx);

        // Create 2 SSTables with 20 entries each
        let mut entries1 = Vec::new();
        let mut entries2 = Vec::new();
        for i in 0..20u8 {
            entries1.push((
                format!("key1-{:02}", i).into_bytes(),
                b"val1".to_vec(),
                i as u64 + 1,
            ));
            entries2.push((
                format!("key2-{:02}", i).into_bytes(),
                b"val2".to_vec(),
                i as u64 + 21,
            ));
        }

        let sst1 = create_test_sstable(
            tmp.path(),
            "sst1.sst",
            &entries1
                .iter()
                .map(|(k, v, s)| (k.as_slice(), v.as_slice(), *s))
                .collect::<Vec<_>>(),
            Arc::clone(&bc),
        )
        .await;

        let sst2 = create_test_sstable(
            tmp.path(),
            "sst2.sst",
            &entries2
                .iter()
                .map(|(k, v, s)| (k.as_slice(), v.as_slice(), *s))
                .collect::<Vec<_>>(),
            Arc::clone(&bc),
        )
        .await;

        // Measure merge duration with Normal pressure
        let normal_out = tmp.path().join("normal_merged.sst");
        let start_normal = std::time::Instant::now();
        engine
            .merge_sstables(
                &[Arc::clone(&sst1), Arc::clone(&sst2)],
                &normal_out,
                u64::MAX,
                true,
            )
            .await
            .expect("merge under normal pressure");
        let duration_normal = start_normal.elapsed();

        // Switch pressure level to Critical
        pressure_tx
            .send(SystemPressure {
                wal_queue_depth: 600,
                blocking_thread_utilization: 0.9,
                embedding_queue_depth: 0,
                pressure_level: PressureLevel::Critical,
            })
            .expect("send pressure");

        // Measure merge duration with Critical pressure (yielding 4 times across 40 items -> 4 * 50ms = ~200ms delay)
        let critical_out = tmp.path().join("critical_merged.sst");
        let start_critical = std::time::Instant::now();
        engine
            .merge_sstables(&[sst1, sst2], &critical_out, u64::MAX, true)
            .await
            .expect("merge under critical pressure");
        let duration_critical = start_critical.elapsed();

        assert!(
            duration_critical >= Duration::from_millis(150),
            "Compaction under Critical pressure should take at least ~150ms due to backpressure delays, took {:?}",
            duration_critical
        );
        assert!(
            duration_critical > duration_normal,
            "Compaction under Critical pressure ({:?}) should be measurably slower than under Normal pressure ({:?})",
            duration_critical,
            duration_normal
        );

        // Verify output file content correctness under critical pressure
        let reader = SstableReader::open(&critical_out, Arc::clone(&bc))
            .await
            .expect("open critical sst");
        let entries = reader.iter().await.expect("iter entries");
        assert_eq!(
            entries.len(),
            40,
            "All entries must be preserved after backpressure merge"
        );
    }

    #[tokio::test]
    async fn test_compaction_cancellation() {
        let registry = Arc::new(SnapshotRegistry::new());
        let bc = crate::sstable::create_block_cache(1024);
        let config = CompactionConfig {
            min_sstables_per_tier: 2,
            size_ratio: 2.0,
            check_interval: std::time::Duration::from_millis(10),
            yield_threshold: 100,
            max_memory_bytes: None,
            max_io_bytes_per_second: None,
        };
        let engine = Arc::new(CompactionEngine::new(
            config,
            registry,
            bc,
            None,
            Arc::new(memfuse_core::ResourceTracker::new(
                memfuse_core::ResourceBudget {
                    memory_limit: 1024 * 1024,
                },
            )),
            None,
        ));
        let sstables = Arc::new(tokio::sync::RwLock::new(Vec::new()));
        let tmp = tempfile::TempDir::new().unwrap(); // unwrap

        let cancel_token = tokio_util::sync::CancellationToken::new();

        // Spawn the loop
        let engine_clone = Arc::clone(&engine);
        let sstables_clone = Arc::clone(&sstables);
        let path = tmp.path().to_path_buf();
        let ct_clone = cancel_token.clone();
        let handle = tokio::spawn(async move {
            engine_clone.run_loop(sstables_clone, path, ct_clone).await;
        });

        // Let it run for a bit
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Cancel it
        cancel_token.cancel();

        // Wait for it to finish
        let result = tokio::time::timeout(std::time::Duration::from_secs(1), handle).await;
        assert!(
            result.is_ok(),
            "Compaction loop did not shut down gracefully"
        );
    }

    #[tokio::test]
    async fn concurrent_flush_and_compact_is_safe() {
        use crate::lsm::{LsmConfig, LsmStorage};
        use memfuse_core::traits::StorageEngine;

        for _iteration in 0..10 {
            let tmp = tempfile::TempDir::new().unwrap(); // unwrap
            let config = LsmConfig {
                path: tmp.path().to_path_buf(),
                memtable_size_limit: 1024,
                max_ram_mb: 64,
                tx_timeout: std::time::Duration::from_secs(60),
                compaction: CompactionConfig {
                    min_sstables_per_tier: 2,
                    size_ratio: 2.0,
                    check_interval: std::time::Duration::from_millis(10),
                    yield_threshold: 100,
                    max_memory_bytes: Some(1024 * 1024),
                    max_io_bytes_per_second: None,
                },
                encryption_passphrase: None,
                ..Default::default()
            };

            let storage = Arc::new(LsmStorage::new(config).await.expect("create storage")); // expect

            // Insert initial data and flush to create SSTables
            for i in 0..10u64 {
                let tx = memfuse_core::TxId::new(i + 1);
                let key = format!("key-{:04}", i);
                let val = format!("val-{:04}", i);
                storage
                    .put(tx, key.as_bytes(), val.as_bytes())
                    .await
                    .expect("put"); // expect
                storage.commit(tx).await.expect("commit"); // expect
            }
            storage.force_flush().await.expect("flush 1"); // expect

            for i in 10..20u64 {
                let tx = memfuse_core::TxId::new(i + 1);
                let key = format!("key-{:04}", i);
                let val = format!("val-{:04}", i);
                storage
                    .put(tx, key.as_bytes(), val.as_bytes())
                    .await
                    .expect("put"); // expect
                storage.commit(tx).await.expect("commit"); // expect
            }
            storage.force_flush().await.expect("flush 2"); // expect

            // Write un-flushed memtable data for concurrent flush task
            for i in 20..30u64 {
                let tx = memfuse_core::TxId::new(i + 1);
                let key = format!("key-{:04}", i);
                let val = format!("val-{:04}", i);
                storage
                    .put(tx, key.as_bytes(), val.as_bytes())
                    .await
                    .expect("put"); // expect
                storage.commit(tx).await.expect("commit"); // expect
            }

            let s1 = Arc::clone(&storage);
            let s2 = Arc::clone(&storage);

            let flush_handle = tokio::spawn(async move { s1.force_flush().await });

            let compact_handle = tokio::spawn(async move { s2.maybe_compact().await });

            let (flush_res, compact_res) = tokio::join!(flush_handle, compact_handle);
            flush_res
                .expect("flush task joined") // expect
                .expect("flush succeeded"); // expect
            compact_res
                .expect("compact task joined") // expect
                .expect("compact succeeded"); // expect

            // Verify data readability
            for i in 0..30u64 {
                let key = format!("key-{:04}", i);
                let expected_val = format!("val-{:04}", i);
                let val = storage
                    .get(key.as_bytes())
                    .await
                    .expect("get") // expect
                    .expect("key must exist"); // expect
                assert_eq!(val, expected_val.as_bytes());
            }
        }
    }

    #[tokio::test]
    async fn test_compaction_single_lock_candidate_selection_concurrency() {
        let tmp = TempDir::new().expect("temp dir");
        let registry = Arc::new(SnapshotRegistry::new());
        let bc = create_block_cache(1);
        let config = CompactionConfig {
            min_sstables_per_tier: 2,
            ..Default::default()
        };
        let engine = CompactionEngine::new(
            config,
            registry,
            Arc::clone(&bc),
            None,
            Arc::new(memfuse_core::ResourceTracker::new(
                memfuse_core::ResourceBudget {
                    memory_limit: 1024 * 1024,
                },
            )),
            None,
        );

        let sstables = Arc::new(RwLock::new(Vec::new()));
        for i in 0..3u8 {
            let sst = create_test_sstable(
                tmp.path(),
                &format!("sst-{}.sst", i),
                &[(format!("key-{}", i).as_bytes(), b"val", i as u64 + 1)],
                Arc::clone(&bc),
            )
            .await;
            sstables.write().await.push(sst);
        }

        // 1. Obtain selected candidate Arcs in a single read lock call
        let candidates = engine
            .select_compaction_candidates(&sstables.read().await)
            .expect("candidates selected");
        assert_eq!(candidates.len(), 3);

        // 2. Simulate concurrent modification (flush/rollback/pop/clear) on sstables
        let extra_sst = create_test_sstable(
            tmp.path(),
            "sst-concurrent-flush.sst",
            &[(b"concurrent-key", b"val", 99)],
            Arc::clone(&bc),
        )
        .await;
        {
            let mut guard = sstables.write().await;
            guard.remove(0); // remove item 0 (rollback/compaction modification)
            guard.push(extra_sst); // append new sstable (concurrent flush)
        }

        // 3. Verify candidates acquired in step 1 are completely decoupled from index changes
        // in sstables list and can be safely merged without out-of-bounds panics.
        let output = tmp.path().join("merged_decoupled.sst");
        let merge_res = engine
            .merge_sstables(&candidates, &output, u64::MAX, true)
            .await;
        assert!(
            merge_res.is_ok(),
            "Merge of Arc candidates must succeed regardless of list modifications"
        );

        let reader = SstableReader::open(&output, Arc::clone(&bc))
            .await
            .expect("open merged sst");
        let entries = reader.iter().await.expect("iter entries");
        assert_eq!(
            entries.len(),
            3,
            "All 3 original Arc candidates must be merged safely"
        );
    }

    #[tokio::test]
    async fn test_mvcc_floor_version_retained_for_active_snapshot() {
        let tmp = TempDir::new().expect("temp dir");
        let registry = Arc::new(SnapshotRegistry::new());
        let bc = create_block_cache(1);
        let engine = CompactionEngine::new(
            CompactionConfig::default(),
            registry,
            Arc::clone(&bc),
            None,
            Arc::new(memfuse_core::ResourceTracker::new(
                memfuse_core::ResourceBudget {
                    memory_limit: 1024 * 1024,
                },
            )),
            None,
        );

        // Key "k1" has 3 versions: seq 30 ("v30"), seq 20 ("v20"), seq 10 ("v10")
        let sst3 = create_test_sstable(
            tmp.path(),
            "sst3.sst",
            &[(b"k1", b"v30", 30)],
            Arc::clone(&bc),
        )
        .await;

        let sst2 = create_test_sstable(
            tmp.path(),
            "sst2.sst",
            &[(b"k1", b"v20", 20)],
            Arc::clone(&bc),
        )
        .await;

        let sst1 = create_test_sstable(
            tmp.path(),
            "sst1.sst",
            &[(b"k1", b"v10", 10)],
            Arc::clone(&bc),
        )
        .await;

        let output = tmp.path().join("merged_mvcc_floor.sst");

        // Active snapshot pinned at min_snapshot_seq = 15.
        // Versions > 15: seq 30, seq 20 (MUST both be retained).
        // Floor version (newest <= 15): seq 10 ("v10") (MUST be retained).
        engine
            .merge_sstables(&[sst1, sst2, sst3], &output, 15, true)
            .await
            .expect("merge");

        let reader = SstableReader::open(&output, Arc::clone(&bc))
            .await
            .expect("open merged");
        let entries = reader.iter().await.expect("iter");

        assert_eq!(
            entries.len(),
            3,
            "All 3 versions (seq 30, seq 20, seq 10) must be retained when min_snapshot_seq = 15"
        );
        assert_eq!(entries[0].2 & !TOMBSTONE_BIT, 30);
        assert_eq!(entries[1].2 & !TOMBSTONE_BIT, 20);
        assert_eq!(entries[2].2 & !TOMBSTONE_BIT, 10);
    }

    #[tokio::test]
    async fn test_chain_linkage_tier_grouping() {
        let tmp = TempDir::new().expect("temp dir");
        let registry = Arc::new(SnapshotRegistry::new());
        let bc = create_block_cache(1);
        let config = CompactionConfig {
            min_sstables_per_tier: 3,
            size_ratio: 2.0,
            ..CompactionConfig::default()
        };
        let engine = CompactionEngine::new(
            config,
            registry,
            Arc::clone(&bc),
            None,
            Arc::new(memfuse_core::ResourceTracker::new(
                memfuse_core::ResourceBudget {
                    memory_limit: 1024 * 1024,
                },
            )),
            None,
        );

        // SSTables with sizes 100, 180, 320
        // Under single-linkage (against 100):
        // 180 / 100 = 1.8 <= 2.0 (fits)
        // 320 / 100 = 3.2 > 2.0 (would NOT fit)
        // Under chain-linkage (against neighbor 180):
        // 180 / 100 = 1.8 <= 2.0
        // 320 / 180 = 1.77 <= 2.0 (FITS in tier under chain-linkage!)

        async fn create_padded_sst(
            dir: &std::path::Path,
            name: &str,
            size_target_kb: usize,
            seq: u64,
            bc: Arc<BlockCache>,
        ) -> Arc<SstableReader> {
            let path = dir.join(name);
            let mut builder = SstableBuilder::create(&path).await.expect("create sst");
            let pad = vec![0u8; 1024];
            for i in 0..size_target_kb {
                let k = format!("k-{:06}", i);
                builder
                    .add(k.as_bytes(), &pad, seq, seq)
                    .await
                    .expect("add entry");
            }
            builder.finish().await.expect("finish sst");
            Arc::new(SstableReader::open(&path, bc).await.expect("open sst"))
        }

        let sst1 = create_padded_sst(tmp.path(), "sst1.sst", 10, 1, Arc::clone(&bc)).await;
        let sst2 = create_padded_sst(tmp.path(), "sst2.sst", 18, 2, Arc::clone(&bc)).await;
        let sst3 = create_padded_sst(tmp.path(), "sst3.sst", 32, 3, Arc::clone(&bc)).await;

        let candidates = engine
            .select_compaction_candidates(&[sst1, sst2, sst3])
            .expect("chain linkage should select all 3 SSTables into a single tier");

        assert_eq!(
            candidates.len(),
            3,
            "Chain linkage must group [10, 18, 32] into a tier with size_ratio = 2.0"
        );
    }

    #[tokio::test]
    async fn test_compaction_concurrent_rollback_flush_no_panic() {
        let tmp = TempDir::new().expect("temp dir");
        let registry = Arc::new(SnapshotRegistry::new());
        let bc = create_block_cache(1);
        let config = CompactionConfig {
            min_sstables_per_tier: 2,
            yield_threshold: 1, // force frequent yields during merge
            ..CompactionConfig::default()
        };
        let engine = Arc::new(CompactionEngine::new(
            config,
            registry,
            Arc::clone(&bc),
            None,
            Arc::new(memfuse_core::ResourceTracker::new(
                memfuse_core::ResourceBudget {
                    memory_limit: 1024 * 1024,
                },
            )),
            None,
        ));

        let sstables = Arc::new(RwLock::new(Vec::new()));
        // Populate initial SSTables
        for i in 0..5u8 {
            let sst = create_test_sstable(
                tmp.path(),
                &format!("sst-init-{}.sst", i),
                &[(format!("key-{}", i).as_bytes(), b"val", i as u64 + 1)],
                Arc::clone(&bc),
            )
            .await;
            sstables.write().await.push(sst);
        }

        let engine_clone = Arc::clone(&engine);
        let sstables_clone = Arc::clone(&sstables);
        let tmp_path = tmp.path().to_path_buf();

        // Spawn concurrent task simulating rollback (shortening list) and flush (appending list)
        let mutator_cancel = tokio_util::sync::CancellationToken::new();
        let ct = mutator_cancel.clone();
        let sstables_mut = Arc::clone(&sstables);
        let bc_mut = Arc::clone(&bc);
        let tmp_path_mut = tmp.path().to_path_buf();

        let mutator_handle = tokio::spawn(async move {
            let mut counter = 100u8;
            while !ct.is_cancelled() {
                tokio::time::sleep(Duration::from_millis(1)).await;
                let mut guard = sstables_mut.write().await;
                if !guard.is_empty() && counter.is_multiple_of(2) {
                    // Simulate rollback / compaction cleanup: remove an entry
                    guard.pop();
                } else {
                    // Simulate concurrent flush: append a new SSTable
                    counter += 1;
                    let new_sst = create_test_sstable(
                        &tmp_path_mut,
                        &format!("sst-mut-{}.sst", counter),
                        &[(
                            format!("key-mut-{}", counter).as_bytes(),
                            b"val",
                            counter as u64,
                        )],
                        Arc::clone(&bc_mut),
                    )
                    .await;
                    guard.push(new_sst);
                }
            }
        });

        // Run maybe_compact multiple times during concurrent list mutations
        for _ in 0..10 {
            let res = engine_clone.maybe_compact(&sstables_clone, &tmp_path).await;
            // Compaction must return Ok(true) or Ok(false), never panic or error out bounds
            assert!(
                res.is_ok(),
                "maybe_compact must complete cleanly without panic or unexpected error under concurrent list mutation"
            );
        }

        mutator_cancel.cancel();
        let _ = mutator_handle.await;
    }

    #[tokio::test]
    async fn test_tombstone_retention_floor_with_active_snapshot() {
        use memfuse_core::TOMBSTONE_BIT;

        let tmp = TempDir::new().expect("temp dir");
        let bc = create_block_cache(1);
        let manifest = Arc::new(
            crate::manifest::Manifest::open(tmp.path().join("MANIFEST"))
                .await
                .expect("manifest"),
        );
        let registry = Arc::new(SnapshotRegistry::new());

        let mut config = CompactionConfig::default();
        config.min_sstables_per_tier = 2;

        let budget = Arc::new(memfuse_core::ResourceTracker::new(
            memfuse_core::ResourceBudget {
                memory_limit: 100 * 1024 * 1024,
            },
        ));
        let engine = CompactionEngine::new(
            config,
            registry.clone(),
            bc.clone(),
            None,
            budget,
            Some(manifest),
        );

        // Active snapshot pinned at seq 15
        registry.pin(15);

        // Input 1: Put at seq 20, Tombstone at seq 18, Put at seq 5.
        // Active snapshot is pinned at seq 15.
        let sst1 = create_test_sstable(
            tmp.path(),
            "sst-ts-1.sst",
            &[
                (b"key-1", b"new_val", 20),
                (b"key-1", b"", 18 | TOMBSTONE_BIT),
                (b"key-1", b"old_val", 5),
            ],
            Arc::clone(&bc),
        )
        .await;

        let output_path = tmp.path().join("sst-compacted.sst");
        engine
            .merge_sstables(&[sst1], &output_path, 15, true)
            .await
            .expect("compaction succeeds");

        let reader = SstableReader::open(&output_path, bc)
            .await
            .expect("open reader");
        let entries = reader.iter().await.expect("iter entries");

        // Under min_snapshot_seq = 15:
        // 1) seq 20 (>= 15): kept.
        // 2) seq 18 (>= 15, tombstone): kept because raw_seq >= min_snapshot_seq.
        // 3) seq 5 (< 15): floor version below min_snapshot_seq, kept.
        assert_eq!(
            entries.len(),
            3,
            "Expected 3 entries (seq 20, seq 18 tombstone, seq 5 floor)"
        );
        assert_eq!(entries[0].2 & !TOMBSTONE_BIT, 20);
        assert_eq!(entries[1].2 & !TOMBSTONE_BIT, 18);
        assert_ne!(
            entries[1].2 & TOMBSTONE_BIT,
            0,
            "seq 18 must be a tombstone"
        );
        assert_eq!(entries[2].2 & !TOMBSTONE_BIT, 5);
    }
}
