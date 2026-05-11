//! SnapshotRegistry for MVCC-safe reads.
//!
//! Manages active read snapshots and computes the minimum active
//! sequence number to prevent premature tombstone GC.

// ANCHOR:ARCH:MVCC-001 — Snapshot-Registry schützt Reads vor Compaction-GC.
// WP:WP-0.0 PRIO:1 NEEDS:NONE
// AGENT:01 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-05 DEADLINE:NONE
// INVARIANTE: Solange SnapshotGuard lebt → keine Tombstone-GC für seq >= guard.seq_no.
// RAII-PATTERN: Drop deregistriert automatisch. unwrap_or(u64::MAX) ist KORREKT.

use parking_lot::Mutex;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Registry for active read snapshots.
///
/// ### Locking Strategy
/// Uses a single `parking_lot::Mutex` to protect the map of active snapshots.
/// Updates to `min_active_seqno` use atomic operations with Release/Acquire
/// semantics to ensure visibility across threads without holding the lock
/// during reads.
#[derive(Debug)]
pub struct SnapshotRegistry {
    active: Mutex<BTreeMap<u64, usize>>,
    min_active_seqno: AtomicU64,
}

impl Default for SnapshotRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SnapshotRegistry {
    /// Creates a new empty SnapshotRegistry.
    pub fn new() -> Self {
        Self {
            active: Mutex::new(BTreeMap::new()),
            min_active_seqno: AtomicU64::new(u64::MAX),
        }
    }

    /// Registers a read snapshot. Returns an RAII guard that
    /// automatically deregisters on drop.
    pub fn register(self: &Arc<Self>, seq_no: u64) -> SnapshotGuard {
        let mut active = self.active.lock();
        *active.entry(seq_no).or_default() += 1;
        self.update_min(&active);
        SnapshotGuard {
            registry: self.clone(),
            seq_no,
        }
    }

    /// Returns the minimum active sequence number (u64::MAX if none).
    #[inline]
    pub fn min_active_seqno(&self) -> u64 {
        self.min_active_seqno.load(Ordering::Acquire)
    }

    /// Persistent pin of a sequence number to prevent GC (SAOS Checkpoint).
    pub fn pin(&self, seq_no: u64) {
        let mut active = self.active.lock();
        *active.entry(seq_no).or_default() += 1;
        self.update_min(&active);
    }

    /// Removes a persistent pin.
    pub fn unpin(&self, seq_no: u64) {
        self.release(seq_no);
    }

    pub(crate) fn release(&self, seq_no: u64) {
        let mut active = self.active.lock();
        if let Some(count) = active.get_mut(&seq_no) {
            *count -= 1;
            if *count == 0 {
                active.remove(&seq_no);
            }
        }
        self.update_min(&active);
    }

    fn update_min(&self, active: &BTreeMap<u64, usize>) {
        // SAFETY: u64::MAX is the correct default when no snapshots are active.
        // It allows the LSM compaction to garbage collect ALL tombstones, as
        // all existing records will have seq_no < u64::MAX.
        let min = active.keys().next().copied().unwrap_or(u64::MAX);
        self.min_active_seqno.store(min, Ordering::Release);
    }
}

/// RAII Guard for an active snapshot.
pub struct SnapshotGuard {
    registry: Arc<SnapshotRegistry>,
    seq_no: u64,
}

impl SnapshotGuard {
    /// Returns the sequence number of this snapshot.
    pub fn seq_no(&self) -> u64 {
        self.seq_no
    }
}

impl Drop for SnapshotGuard {
    fn drop(&mut self) {
        self.registry.release(self.seq_no);
    }
}
