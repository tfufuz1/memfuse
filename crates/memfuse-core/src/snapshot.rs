//! SnapshotRegistry for MVCC-safe reads.
//!
//! Manages active read snapshots and computes the minimum active
//! sequence number to prevent premature tombstone GC.

// INVARIANT: Snapshot-Registry schützt Reads vor Compaction-GC.
// INVARIANTE: Solange SnapshotGuard lebt → keine Tombstone-GC für seq >= guard.seq_no.
// RAII-PATTERN: Drop deregistriert automatisch. unwrap_or(u64::MAX) ist KORREKT.

use crate::types::TOMBSTONE_BIT;
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
        let seq_no = seq_no & !TOMBSTONE_BIT;
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
        let seq_no = seq_no & !TOMBSTONE_BIT;
        let mut active = self.active.lock();
        *active.entry(seq_no).or_default() += 1;
        self.update_min(&active);
    }

    /// Removes a persistent pin.
    pub fn unpin(&self, seq_no: u64) {
        self.release(seq_no);
    }

    pub(crate) fn release(&self, seq_no: u64) {
        let seq_no = seq_no & !TOMBSTONE_BIT;
        let mut active = self.active.lock();
        if let Some(count) = active.get_mut(&seq_no) {
            *count -= 1;
            if *count == 0 {
                active.remove(&seq_no);
            }
        } else {
            debug_assert!(
                false,
                "SnapshotRegistry::release called for unknown seq_no: {}",
                seq_no
            );
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
    pub fn seq_no(&self) -> u64 {
        self.seq_no
    }
}

impl Drop for SnapshotGuard {
    fn drop(&mut self) {
        self.registry.release(self.seq_no);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prop_assert_eq;

    // INTENT: Snapshot-Registry Lifecycle verified by 5 unit tests.
    #[test]
    fn test_snapshot_registry_basic() {
        let registry = Arc::new(SnapshotRegistry::new());
        assert_eq!(registry.min_active_seqno(), u64::MAX);

        let guard = registry.register(100);
        assert_eq!(guard.seq_no(), 100);
        assert_eq!(registry.min_active_seqno(), 100);

        drop(guard);
        assert_eq!(registry.min_active_seqno(), u64::MAX);
    }

    #[test]
    fn test_multiple_snapshots_min_calc() {
        let registry = Arc::new(SnapshotRegistry::new());
        let _g1 = registry.register(200);
        let g2 = registry.register(100);
        let _g3 = registry.register(300);

        assert_eq!(registry.min_active_seqno(), 100);

        drop(g2);
        assert_eq!(registry.min_active_seqno(), 200);
    }

    #[test]
    fn test_pin_unpin() {
        let registry = Arc::new(SnapshotRegistry::new());
        registry.pin(50);
        assert_eq!(registry.min_active_seqno(), 50);

        let g = registry.register(100);
        assert_eq!(registry.min_active_seqno(), 50);

        registry.unpin(50);
        assert_eq!(registry.min_active_seqno(), 100);

        drop(g);
        assert_eq!(registry.min_active_seqno(), u64::MAX);
    }

    #[test]
    fn test_seq_no_tombstone_masking() {
        let registry = Arc::new(SnapshotRegistry::new());
        // seq_no with tombstone bit set
        let seq = 100 | crate::types::TOMBSTONE_BIT;
        let guard = registry.register(seq);

        assert_eq!(guard.seq_no(), 100);
        assert_eq!(registry.min_active_seqno(), 100);
    }

    #[test]
    fn test_ref_counting() {
        let registry = Arc::new(SnapshotRegistry::new());
        let g1 = registry.register(100);
        let g2 = registry.register(100);

        assert_eq!(registry.min_active_seqno(), 100);

        drop(g1);
        assert_eq!(registry.min_active_seqno(), 100);

        drop(g2);
        assert_eq!(registry.min_active_seqno(), u64::MAX);
    }

    proptest::proptest! {
        #[test]
        fn prop_snapshot_registry_min_active(
            seqs in proptest::collection::vec(0..1000u64, 1..50)
        ) {
            let registry = Arc::new(SnapshotRegistry::new());
            let mut guards = Vec::new();

            for &seq in &seqs {
                guards.push(registry.register(seq));
            }

            let min_expected = *seqs.iter().min().unwrap();
            prop_assert_eq!(registry.min_active_seqno(), min_expected);

            guards.pop(); // Drop last element

            // If all elements dropped, min_active is MAX, else it's min of remaining
            if guards.is_empty() {
                prop_assert_eq!(registry.min_active_seqno(), u64::MAX);
            } else {
                let remaining_min = guards.iter().map(|g| g.seq_no()).min().unwrap();
                prop_assert_eq!(registry.min_active_seqno(), remaining_min);
            }
        }
    }
}
