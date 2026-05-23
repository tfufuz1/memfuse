//! Background compaction engine for the LSM-Tree.
//!
//! Implements a Size-Tiered Compaction Strategy (STCS):
//! Groups SSTables by size class and merges groups that exceed a threshold.
//! Tombstones are garbage-collected during merge when no active snapshot
//! references them.

// ANCHOR:ARCH:COMPACT-001 — Background Compaction (STCS — Size-Tiered Compaction Strategy).
// WP:WP-0.0 PRIO:1 NEEDS:NONE
// AGENT:01 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-05 DEADLINE:NONE
// ALGORITHMUS: Gruppiere SSTables nach Größenklasse → Merge wenn >= min_sstables_per_tier.
// TOMBSTONE-GC: Tombstones werden NUR gelöscht wenn seq < min_active_seqno (MVCC-SAFE).
// ATOMARER SWAP: Merge unter read-lock, SSTable-Liste swap unter write-lock.
// INVARIANTE: Während Compaction sind alte SSTables noch lesbar (readers halten Arc).
// LIFECYCLE: run_loop() -> maybe_compact() -> select_candidates() -> merge_sstables()
//!
//! Implements a Size-Tiered Compaction Strategy (STCS):
//! Groups SSTables by size class and merges groups that exceed a threshold.
//! Tombstones are garbage-collected during merge when no active snapshot
//! references them.

use crate::sstable::{BlockCache, SstableBuilder, SstableReader};
use memfuse_core::{Result, SnapshotRegistry, TOMBSTONE_BIT};
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
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            min_sstables_per_tier: 4,
            size_ratio: 4.0,
            check_interval: Duration::from_secs(30),
        }
    }
}

/// Shared mutable state that LsmStorage and CompactionEngine both access.
///
/// This is the same `LsmState` from `lsm.rs`, but we define the compaction
/// interface in terms of what we need: the SSTable list and the data path.
pub struct CompactionEngine {
    config: CompactionConfig,
    snapshot_registry: Arc<SnapshotRegistry>,
    block_cache: Arc<BlockCache>,
}

impl CompactionEngine {
    /// Creates a new compaction engine.
    pub fn new(
        config: CompactionConfig,
        snapshot_registry: Arc<SnapshotRegistry>,
        block_cache: Arc<BlockCache>,
    ) -> Self {
        Self {
            config,
            snapshot_registry,
            block_cache,
        }
    }

    /// Evaluates whether compaction should run and performs it if needed.
    ///
    /// Takes a write-lock on the SSTable list to atomically swap old SSTables
    /// for the compacted result.
    pub async fn maybe_compact(
        &self,
        sstables: &RwLock<Vec<Arc<SstableReader>>>,
        data_path: &std::path::Path,
    ) -> Result<bool> {
        // 1. Read current SSTables under read-lock
        let candidates = {
            let ssts = sstables.read().await;
            if ssts.len() < self.config.min_sstables_per_tier {
                return Ok(false);
            }
            self.select_compaction_candidates(&ssts)
        };

        let indices = match candidates {
            Some(indices) if indices.len() >= 2 => indices,
            _ => return Ok(false),
        };

        tracing::info!("Compaction triggered: merging {} SSTables", indices.len());

        // 2. Collect input SSTables (under read-lock, just clone Arcs)
        let input_ssts: Vec<Arc<SstableReader>> = {
            let ssts = sstables.read().await;
            indices.iter().map(|&i| Arc::clone(&ssts[i])).collect()
        };

        // 3. Perform the merge (no lock held — this is the expensive part)
        let min_snapshot_seq = self.snapshot_registry.min_active_seqno();
        let output_path = Self::generate_sst_path(data_path);
        self.merge_sstables(&input_ssts, &output_path, min_snapshot_seq)
            .await?;

        // 4. Open the new SSTable
        let new_reader =
            Arc::new(SstableReader::open(&output_path, Arc::clone(&self.block_cache)).await?);

        // 5. Atomic swap under write-lock
        let old_paths: Vec<PathBuf> = {
            let mut ssts = sstables.write().await;

            // Collect paths of old SSTables before removing them
            let old_paths: Vec<PathBuf> = indices
                .iter()
                .map(|&i| ssts[i].file_path().to_path_buf())
                .collect();

            // Remove old SSTables (reverse order to preserve indices)
            let mut sorted_indices = indices.clone();
            sorted_indices.sort_unstable_by(|a, b| b.cmp(a));
            let insertion_point = sorted_indices[sorted_indices.len() - 1]; // Position of the oldest input

            for idx in sorted_indices {
                ssts.remove(idx);
            }

            // Add new SSTable at the correct position to maintain shadowing order
            let insert_idx = insertion_point.min(ssts.len());
            ssts.insert(insert_idx, new_reader);

            old_paths
        };

        // 6. Delete old SSTable files (best-effort, outside lock)
        for path in &old_paths {
            if let Err(e) = tokio::fs::remove_file(path).await {
                tracing::warn!("Failed to delete compacted SSTable {:?}: {}", path, e);
            }
        }

        tracing::info!(
            "Compaction complete: merged {} SSTables into {:?}",
            indices.len(),
            output_path
        );

        Ok(true)
    }

    /// Selects SSTables to compact using Size-Tiered strategy.
    ///
    /// Groups by size class and returns the first group that meets the threshold.
    fn select_compaction_candidates(&self, ssts: &[Arc<SstableReader>]) -> Option<Vec<usize>> {
        if ssts.len() < 2 {
            return None;
        }

        // Group SSTables by size tier
        let mut tiers: Vec<Vec<usize>> = Vec::new();

        for (i, sst) in ssts.iter().enumerate() {
            let size = sst.metadata().file_size;
            let mut placed = false;

            for tier in &mut tiers {
                let tier_size = ssts[tier[0]].metadata().file_size;
                let ratio = if size > tier_size {
                    size as f64 / tier_size.max(1) as f64
                } else {
                    tier_size as f64 / size.max(1) as f64
                };

                if ratio <= self.config.size_ratio {
                    tier.push(i);
                    placed = true;
                    break;
                }
            }

            if !placed {
                tiers.push(vec![i]);
            }
        }

        // Return the first tier with enough candidates
        for tier in tiers {
            if tier.len() >= self.config.min_sstables_per_tier {
                return Some(tier);
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
            return Some(by_size[..count].iter().map(|&(i, _)| i).collect());
        }

        None
    }

    /// Performs a multi-way merge of input SSTables into a single output SSTable.
    ///
    /// During merge:
    /// - Duplicate keys: newest sequence number wins
    /// - Tombstones: removed if `seq_no < min_snapshot_seq` (no snapshot references them)
    async fn merge_sstables(
        &self,
        inputs: &[Arc<SstableReader>],
        output_path: &std::path::Path,
        min_snapshot_seq: u64,
    ) -> Result<()> {
        // 1. Collect all entries from all input SSTables
        let mut all_entries: Vec<(Vec<u8>, Vec<u8>, u64)> = Vec::new();
        for sst in inputs {
            let entries = sst.iter().await?;
            for (k, v, seq) in entries {
                all_entries.push((k.to_vec(), v.to_vec(), seq));
            }
        }

        // 2. Sort by key, then by sequence number descending (newest first)
        all_entries.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| b.2.cmp(&a.2)));

        // 3. Deduplicate: for each key, keep only the entry with the highest seq_no
        let mut deduped: Vec<(Vec<u8>, Vec<u8>, u64)> = Vec::new();
        let mut last_key: Option<&[u8]> = None;

        for entry in &all_entries {
            if last_key == Some(&entry.0) {
                continue; // Skip: already have a newer version of this key
            }
            last_key = Some(&entry.0);

            let is_tombstone = (entry.2 & TOMBSTONE_BIT) != 0;
            let raw_seq = entry.2 & !TOMBSTONE_BIT;

            // GC tombstones that are older than all active snapshots
            if is_tombstone && raw_seq < min_snapshot_seq {
                continue; // Tombstone is safe to garbage-collect
            }

            deduped.push(entry.clone());
        }

        // 4. Write to output SSTable
        let mut builder = SstableBuilder::create(output_path).await?;
        for (key, value, seq) in &deduped {
            builder.add(key, value, *seq).await?;
        }
        builder.finish().await?;

        Ok(())
    }

    /// Generates a unique SSTable file path using microsecond timestamp.
    fn generate_sst_path(data_path: &std::path::Path) -> PathBuf {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros();
        let count = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        data_path.join(format!("sst-compact-{:020}-{:04}.sst", id, count % 10000))
    }

    /// Runs the background compaction loop.
    ///
    /// Periodically checks if compaction is needed and performs it.
    /// Designed to be spawned via `tokio::spawn`.
    pub async fn run_loop(
        self: Arc<Self>,
        sstables: Arc<RwLock<Vec<Arc<SstableReader>>>>,
        data_path: PathBuf,
    ) {
        loop {
            tokio::time::sleep(self.config.check_interval).await;
            match self.maybe_compact(&sstables, &data_path).await {
                Ok(true) => {
                    tracing::debug!("Background compaction cycle completed successfully");
                }
                Ok(false) => {
                    tracing::trace!("No compaction needed");
                }
                Err(e) => {
                    tracing::error!("Background compaction failed: {}", e);
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
        let mut builder = SstableBuilder::create(&path).await.expect("create sst");
        for (k, v, seq) in entries {
            builder.add(k, v, *seq).await.expect("add entry");
        }
        builder.finish().await.expect("finish sst");
        Arc::new(SstableReader::open(&path, bc).await.expect("open sst"))
    }

    #[tokio::test]
    async fn test_merge_deduplication() {
        let tmp = TempDir::new().expect("temp dir");
        let registry = Arc::new(SnapshotRegistry::new());
        let bc = create_block_cache(1);
        let engine = CompactionEngine::new(CompactionConfig::default(), registry, Arc::clone(&bc));

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
            .merge_sstables(&[sst1, sst2], &output, 0)
            .await
            .expect("merge");

        let reader = SstableReader::open(&output, Arc::clone(&bc))
            .await
            .expect("open merged");
        let entries = reader.iter().await.expect("iter");

        // key-a should have the newer value (seq=3)
        assert_eq!(entries.len(), 3); // key-a, key-b, key-c
        assert_eq!(entries[0].0.as_ref(), b"key-a");
        assert_eq!(entries[0].1.as_ref(), b"val-3");
        assert_eq!(entries[0].2, 3);
    }

    #[tokio::test]
    async fn test_tombstone_gc() {
        let tmp = TempDir::new().expect("temp dir");
        let registry = Arc::new(SnapshotRegistry::new());
        let bc = create_block_cache(1);
        let engine = CompactionEngine::new(CompactionConfig::default(), registry, Arc::clone(&bc));

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
            .merge_sstables(&[sst1], &output, 100)
            .await
            .expect("merge");

        let reader = SstableReader::open(&output, Arc::clone(&bc))
            .await
            .expect("open");
        let entries = reader.iter().await.expect("iter");

        assert_eq!(entries.len(), 1); // Only "alive" remains
        assert_eq!(entries[0].0.as_ref(), b"alive");
    }

    #[tokio::test]
    async fn test_tombstone_preserved_with_active_snapshot() {
        let tmp = TempDir::new().expect("temp dir");
        let registry = Arc::new(SnapshotRegistry::new());
        let bc = create_block_cache(1);
        let engine = CompactionEngine::new(CompactionConfig::default(), registry, Arc::clone(&bc));

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
            .merge_sstables(&[sst1], &output, 2)
            .await
            .expect("merge");

        let reader = SstableReader::open(&output, Arc::clone(&bc))
            .await
            .expect("open");
        let entries = reader.iter().await.expect("iter");

        assert_eq!(entries.len(), 2); // Both preserved
    }

    #[tokio::test]
    async fn test_maybe_compact_full_cycle() {
        let tmp = TempDir::new().expect("temp dir");
        let registry = Arc::new(SnapshotRegistry::new());
        let bc = create_block_cache(1);
        let config = CompactionConfig {
            min_sstables_per_tier: 2, // Low threshold for testing
            size_ratio: 4.0,
            check_interval: Duration::from_secs(30),
        };
        let engine = CompactionEngine::new(config, registry, Arc::clone(&bc));

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
            .expect("compact");

        assert!(compacted, "Compaction should have occurred");

        // After compaction: fewer SSTables, all data still accessible
        let ssts = sstables.read().await;
        assert!(
            ssts.len() < 3,
            "Should have fewer SSTables after compaction"
        );

        // Verify all data is present in the compacted result
        let last_sst = &ssts[ssts.len() - 1];
        let entries = last_sst.iter().await.expect("iter");
        assert_eq!(entries.len(), 6); // 3 SSTables × 2 entries each
    }

    #[tokio::test]
    async fn test_no_compaction_below_threshold() {
        let tmp = TempDir::new().expect("temp dir");
        let registry = Arc::new(SnapshotRegistry::new());
        let bc = create_block_cache(1);
        let config = CompactionConfig {
            min_sstables_per_tier: 4,
            size_ratio: 4.0,
            check_interval: Duration::from_secs(30),
        };
        let engine = CompactionEngine::new(config, registry, Arc::clone(&bc));

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
            .expect("compact");

        assert!(!compacted, "Should not compact with only 2 SSTables");
        assert_eq!(sstables.read().await.len(), 2);
    }
    #[tokio::test(flavor = "multi_thread")]
    async fn test_compaction_stress_and_gc() {
        use crate::lsm::{LsmConfig, LsmStorage};
        use memfuse_core::TxId;
        use std::sync::atomic::{AtomicBool, Ordering};

        let tmp = TempDir::new().expect("temp dir");
        let config = LsmConfig {
            path: tmp.path().to_path_buf(),
            memtable_size_limit: 64 * 1024, // 64KB - very small to force flushes
            max_ram_mb: 64,
            tx_timeout: Duration::from_secs(60),
            compaction: CompactionConfig {
                min_sstables_per_tier: 3, // Small tier to trigger compaction often
                size_ratio: 2.0,
                check_interval: Duration::from_millis(100), // Fast check
            },
            encryption_passphrase: None,
        };

        let storage = Arc::new(LsmStorage::new(config).await.expect("create storage"));
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
            storage.put(tx, key.as_bytes(), &val).await.expect("put");
            storage.commit(tx).await.expect("commit");
        }

        // 3. Register a Snapshot [INV-C1]
        let snapshot_seq = storage.last_seq_no().await.expect("last_seq_no");
        let _guard = storage.snapshot_registry.register(snapshot_seq);

        // 4. Heavy Load: 10,000 Inserts to trigger churn and background compaction
        for i in 0..10000 {
            let tx = TxId::new(1000 + i as u64);
            let key = format!("doc-{:04}", i);
            let val = vec![(i % 255) as u8; 100];
            storage
                .put(tx, key.as_bytes(), &val)
                .await
                .expect("put heavy");
            storage.commit(tx).await.expect("commit heavy");
        }

        // 5. Deletes
        for i in 0..5000 {
            let tx = TxId::new(20000 + i as u64);
            let key = format!("doc-{:04}", i);
            storage.delete(tx, key.as_bytes()).await.expect("delete");
            storage.commit(tx).await.expect("commit delete");
        }

        // 6. Wait for background compactions to stabilize
        let mut stabilized = false;
        for _ in 0..100 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let stats = storage.stats().await.expect("stats");
            // If we have few segments, compaction is doing its job
            if stats.num_segments <= 5 {
                stabilized = true;
                break;
            }
        }

        // Stop reader and check for errors
        running.store(false, Ordering::SeqCst);
        reader_handle.await.expect("reader task panicked");

        // 7. Final Verification
        let stats = storage.stats().await.expect("final stats");
        println!(
            "Stress test finished. Final SSTable count: {}",
            stats.num_segments
        );

        // Non-deleted data MUST be present
        for i in 5000..10000 {
            let key = format!("doc-{:04}", i);
            let val = storage
                .get(key.as_bytes())
                .await
                .expect("get final")
                .unwrap_or_else(|| panic!("missing key {}", key));
            assert_eq!(val[0], (i % 255) as u8);
        }

        // Deleted data MUST NOT be present in current view
        for i in 0..5000 {
            let key = format!("doc-{:04}", i);
            let val = storage.get(key.as_bytes()).await.expect("get deleted");
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
}
