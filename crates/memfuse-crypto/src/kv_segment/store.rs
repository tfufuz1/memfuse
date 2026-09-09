// FILE-CONTEXT
// ZWECK: Tenant-isolierter KV-Segment-Store (INV-TENANT Isolation).
// STAND: TS:2026-09-09T13:20:00Z (SESSION: 5665b844)

use std::sync::atomic::{AtomicUsize, Ordering};

use ahash::AHashMap;
use memfuse_core::TenantId;
use parking_lot::RwLock;

use super::segment::KvSegment;

/// Tenant-isolierter KV-Segment-Store.
///
/// INV-TENANT-Analogon für KV-Bridge: Ein Tenant kann niemals Segmente
/// eines anderen Tenants lesen. Strukturell erzwungen durch getrennte Maps.
pub struct TenantIsolatedKvStore {
    segments: RwLock<AHashMap<TenantId, Vec<KvSegment>>>,
    eviction_round_offset: AtomicUsize,
}

impl TenantIsolatedKvStore {
    /// Erstellt einen neuen tenant-isolierten KV-Store.
    pub fn new() -> Self {
        Self {
            segments: RwLock::new(AHashMap::new()),
            eviction_round_offset: AtomicUsize::new(0),
        }
    }

    /// Fügt ein Segment für einen bestimmten Tenant ein.
    // AI-TAG[API][MINOR][RESOLVED] Overwrite or update existing segment_id on duplicate insert (ID: AGT-CRYPTO-6016eb9a) (TS: 2026-09-09T13:17:00Z) (SESSION: a413a598)
    pub fn insert_segment(&self, tenant: TenantId, segment: KvSegment) {
        let mut map = self.segments.write();
        let list = map.entry(tenant).or_default();
        if let Some(pos) = list.iter().position(|s| s.segment_id == segment.segment_id) {
            list[pos] = segment;
        } else {
            list.push(segment);
        }
    }

    /// Liefert unverschlüsselte Segment-Bytes für einen Tenant (Klartext-Modus).
    // AI-TAG[API][MAJOR][RESOLVED] Add unencrypted segment retrieval API for plaintext mode (ID: AGT-CRYPTO-7e1f286d) (TS: 2026-09-09T13:17:00Z) (SESSION: a413a598)
    pub fn get_segment_bytes(&self, tenant: TenantId, segment_id: u64) -> Option<Vec<u8>> {
        self.segments.read().get(&tenant).and_then(|segs| {
            segs.iter().find(|s| s.segment_id == segment_id).map(|s| {
                s.touch();
                s.as_bytes().to_vec()
            })
        })
    }

    /// Verschlüsselt einen Klartext-Tensor und fügt ein verschlüsseltes Segment ein.
    #[cfg(feature = "kv-encryption")]
    pub fn insert_encrypted_segment(
        &self,
        cipher: &crate::KvSegmentCipher,
        tenant: TenantId,
        segment_id: u64,
        model_fingerprint: crate::ModelFingerprint,
        rope_offset: Option<usize>,
        plaintext: &[u8],
    ) -> Result<(), crate::CryptoError> {
        let segment = KvSegment::new_encrypted(
            cipher,
            tenant,
            segment_id,
            model_fingerprint,
            rope_offset,
            plaintext,
        )?;
        self.insert_segment(tenant, segment);
        Ok(())
    }

    /// INV-TENANT-Analogon für KV-Bridge: Ein Tenant kann niemals Segmente
    /// eines anderen Tenants lesen. Strukturell erzwungen durch getrennte Maps.
    pub fn get_segments(&self, tenant: TenantId) -> Vec<u64> {
        self.segments
            .read()
            .get(&tenant)
            .map(|v| {
                v.iter()
                    .map(|s| {
                        s.touch();
                        s.segment_id
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Liest ein bestimmtes Segment eines Mandanten und entschlüsselt es falls nötig.
    #[cfg(feature = "kv-encryption")]
    pub fn get_decrypted_segment(
        &self,
        cipher: &crate::KvSegmentCipher,
        tenant: TenantId,
        segment_id: u64,
    ) -> Result<Option<Vec<u8>>, crate::CryptoError> {
        let map = self.segments.read();
        if let Some(list) = map.get(&tenant) {
            if let Some(seg) = list.iter().find(|s| s.segment_id == segment_id) {
                seg.touch();
                let decrypted = seg.decrypt_data(cipher)?;
                return Ok(Some(decrypted));
            }
        }
        Ok(None)
    }

    /// Gibt die Anzahl der gespeicherten Segmente für einen bestimmten Tenant zurück.
    pub fn get_tenant_segment_len(&self, tenant: TenantId) -> usize {
        self.segments
            .read()
            .get(&tenant)
            .map(|v| v.len())
            .unwrap_or(0)
    }

    /// Dies ist GLOBALES LRU ohne Tenant-Fairness. Für faire Multi-Tenant-Eviction siehe `evict_lru_fair()`.
    #[allow(dead_code)]
    pub(crate) fn evict_lru_global(&self, target_free_bytes: usize) -> usize {
        let mut map = self.segments.write();
        let mut freed = 0;

        while freed < target_free_bytes && !map.is_empty() {
            let mut lru_tenant = None;
            let mut lru_idx = 0;
            // KV segment timestamp handling
            let mut oldest_time = None;

            for (tenant, segs) in map.iter() {
                for (idx, seg) in segs.iter().enumerate() {
                    let acc = seg.last_accessed();
                    if oldest_time.is_none_or(|t| acc < t) {
                        lru_tenant = Some(*tenant);
                        lru_idx = idx;
                        oldest_time = Some(acc);
                    }
                }
            }

            if let Some(tenant) = lru_tenant {
                if let Some(segs) = map.get_mut(&tenant) {
                    let evicted = segs.remove(lru_idx);
                    freed += evicted.len();
                    tracing::debug!(
                        tenant_id = tenant.inner(),
                        segment_id = evicted.segment_id,
                        freed_bytes = evicted.len(),
                        "KV eviction worker: evicted segment"
                    );
                    if segs.is_empty() {
                        // Avoid holding the mutable reference while removing
                    }
                }
                if map.get(&tenant).is_some_and(|s| s.is_empty()) {
                    map.remove(&tenant);
                }
            } else {
                break;
            }
        }

        freed
    }

    /// Maximale Anzahl an Eviction-Runden, die unter einem einzigen Lock-Erwerb
    /// ausgeführt werden, bevor der Schreiblock kurzzeitig freigegeben wird, um
    /// wartenden Lesezugriffen (get_segments/get_decrypted_segment) eine Chance zu geben.
    const MAX_ROUNDS_PER_LOCK_ACQUISITION: usize = 4;

    /// Evictiert KV-Segmente unter Erhaltung von Tenant-Fairness via Round-Robin über alle aktiven Tenants.
    ///
    /// Im Gegensatz zu `evict_lru_global()` verhindert diese Methode, dass sehr aktive
    /// Tenants inaktive Tenants vollständig verdrängen (Prevent Cross-Tenant Starvation).
    // AI-TAG[SECURITY][MAJOR][RESOLVED] Add tenant-fair LRU eviction to prevent cross-tenant starvation (ID: AGT-CRYPTO-c1a93b22) (TS: 2026-09-10T10:00:00Z)
    pub fn evict_lru_fair(&self, target_free_bytes: usize) -> usize {
        let mut freed = 0;

        'outer: while freed < target_free_bytes {
            let mut map = self.segments.write();
            if map.is_empty() {
                break;
            }

            for _round in 0..Self::MAX_ROUNDS_PER_LOCK_ACQUISITION {
                if freed >= target_free_bytes {
                    break 'outer;
                }

                let mut tenants: Vec<TenantId> = map.keys().copied().collect();
                tenants.sort();

                let mut evicted_in_round = false;

                for tenant in tenants {
                    if freed >= target_free_bytes {
                        break;
                    }

                    if let Some(segs) = map.get_mut(&tenant) {
                        if segs.is_empty() {
                            map.remove(&tenant);
                            continue;
                        }

                        let mut lru_idx = 0;
                        let mut oldest_time = None;

                        for (idx, seg) in segs.iter().enumerate() {
                            let acc = seg.last_accessed();
                            if oldest_time.is_none_or(|t| acc < t) {
                                lru_idx = idx;
                                oldest_time = Some(acc);
                            }
                        }

                        let evicted = segs.remove(lru_idx);
                        freed += evicted.len();
                        evicted_in_round = true;

                        tracing::debug!(
                            tenant_id = tenant.inner(),
                            segment_id = evicted.segment_id,
                            freed_bytes = evicted.len(),
                            "KV eviction worker: evicted segment"
                        );

                        if segs.is_empty() {
                            map.remove(&tenant);
                        }
                    }
                }

                if !evicted_in_round {
                    break 'outer;
                }
            }
        }

        freed
    }

    pub fn clear_all(&self) {
        let mut map = self.segments.write();
        map.clear();
        tracing::warn!("KV emergency_wipe: all segments zeroized synchronously");
    }
}

impl Default for TenantIsolatedKvStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_tenant_isolation_no_cross_read() {
        let store = TenantIsolatedKvStore::new();

        let tenant_a = TenantId::try_new(1).unwrap();
        let tenant_b = TenantId::try_new(2).unwrap();

        let seg_a = KvSegment::new(tenant_a, 101, vec![0x11; 128]);
        let seg_b = KvSegment::new(tenant_b, 202, vec![0x22; 256]);

        store.insert_segment(tenant_a, seg_a);
        store.insert_segment(tenant_b, seg_b);

        // Tenant A sees only its own segment
        let segs_a = store.get_segments(tenant_a);
        assert_eq!(segs_a, vec![101]);
        assert_eq!(store.get_tenant_segment_len(tenant_a), 1);

        // Tenant B sees only its own segment
        let segs_b = store.get_segments(tenant_b);
        assert_eq!(segs_b, vec![202]);
        assert_eq!(store.get_tenant_segment_len(tenant_b), 1);

        // Non-existent tenant C sees nothing
        let tenant_c = TenantId::try_new(3).unwrap();
        assert!(store.get_segments(tenant_c).is_empty());
        assert_eq!(store.get_tenant_segment_len(tenant_c), 0);
    }

    #[test]
    fn test_evict_lru_fair_does_not_starve_inactive_tenant() {
        let store = TenantIsolatedKvStore::new();

        let tenant_a = TenantId::try_new(1).unwrap();
        let tenant_b = TenantId::try_new(2).unwrap();

        // Tenant B has 1 segment with the oldest timestamp (inserted first, untouched)
        let seg_b = KvSegment::new(tenant_b, 201, vec![0x22; 256]);
        store.insert_segment(tenant_b, seg_b);

        std::thread::sleep(std::time::Duration::from_millis(5));

        // Tenant A has 10 segments that are active/touched
        for i in 1..=10 {
            let seg_a = KvSegment::new(tenant_a, i, vec![0x11; 256]);
            store.insert_segment(tenant_a, seg_a);
        }

        // Touch Tenant A's segments so their last_accessed timestamps are fresh
        for i in 1..=10 {
            let _ = store.get_segment_bytes(tenant_a, i);
        }

        assert_eq!(store.get_tenant_segment_len(tenant_a), 10);
        assert_eq!(store.get_tenant_segment_len(tenant_b), 1);

        // Evict 256 bytes using fair eviction
        let freed = store.evict_lru_fair(256);
        assert!(freed >= 256);

        // Tenant A should lose 1 segment (leaving 9)
        assert_eq!(store.get_tenant_segment_len(tenant_a), 9);

        // Tenant B's segment MUST NOT be evicted despite being globally oldest
        assert_eq!(
            store.get_tenant_segment_len(tenant_b),
            1,
            "Tenant B (inactive) must not be starved by Tenant A"
        );
    }

    #[test]
    fn test_evict_lru_fair_respects_target_free_bytes() {
        let store = TenantIsolatedKvStore::new();

        let tenant_a = TenantId::try_new(1).unwrap();
        let tenant_b = TenantId::try_new(2).unwrap();

        for i in 1..=3 {
            store.insert_segment(tenant_a, KvSegment::new(tenant_a, i, vec![0x11; 512]));
            store.insert_segment(tenant_b, KvSegment::new(tenant_b, i + 10, vec![0x22; 512]));
        }

        assert_eq!(store.get_tenant_segment_len(tenant_a), 3);
        assert_eq!(store.get_tenant_segment_len(tenant_b), 3);

        // Target 1000 bytes: requires freeing 2 segments (1024 bytes) across tenants
        let freed = store.evict_lru_fair(1000);
        assert!(
            freed >= 1000,
            "freed bytes ({freed}) must be >= target_free_bytes (1000)"
        );

        // Fair round-robin evicts 1 segment from Tenant A and 1 segment from Tenant B
        assert_eq!(store.get_tenant_segment_len(tenant_a), 2);
        assert_eq!(store.get_tenant_segment_len(tenant_b), 2);
    }

    #[test]
    fn test_evict_lru_fair_releases_lock_between_batches() {
        let store = Arc::new(TenantIsolatedKvStore::new());

        let tenant_a = TenantId::try_new(1).unwrap();
        let tenant_b = TenantId::try_new(2).unwrap();
        let tenant_c = TenantId::try_new(3).unwrap();
        let tenant_unaffected = TenantId::try_new(99).unwrap();

        // Populate store with 30 segments for Tenant A, 30 for Tenant B, and 50 for Tenant C.
        // Each round evicts 1 segment per tenant = 3 segments = 768 bytes per round.
        // 12 rounds will evict 12 segments per tenant (36 segments total = 9,216 bytes).
        // 12 rounds requires 3 lock acquisition cycles (since MAX_ROUNDS_PER_LOCK_ACQUISITION = 4).
        for i in 1..=30 {
            store.insert_segment(tenant_a, KvSegment::new(tenant_a, i, vec![0x11; 256]));
            store.insert_segment(tenant_b, KvSegment::new(tenant_b, i + 100, vec![0x22; 256]));
        }
        for i in 1..=50 {
            store.insert_segment(tenant_c, KvSegment::new(tenant_c, i + 200, vec![0x33; 256]));
        }

        let is_evicting = Arc::new(AtomicBool::new(false));
        let eviction_done = Arc::new(AtomicBool::new(false));
        let reads_during_eviction = Arc::new(AtomicUsize::new(0));

        let store_clone = Arc::clone(&store);
        let is_evicting_clone = Arc::clone(&is_evicting);
        let eviction_done_clone = Arc::clone(&eviction_done);
        let reads_during_eviction_clone = Arc::clone(&reads_during_eviction);

        // Spawn reader thread continuously attempting to read for Tenant C and unaffected tenant.
        let reader_handle = thread::spawn(move || {
            while !eviction_done_clone.load(Ordering::SeqCst) {
                let len = store_clone.get_tenant_segment_len(tenant_c);
                assert!(
                    len >= 38,
                    "Tenant C should have at least 38 segments remaining (got {len})"
                );
                let segs = store_clone.get_segments(tenant_unaffected);
                assert!(segs.is_empty(), "Unaffected tenant 99 must have 0 segments");
                if is_evicting_clone.load(Ordering::SeqCst) {
                    reads_during_eviction_clone.fetch_add(1, Ordering::SeqCst);
                }
                thread::yield_now();
            }
        });

        // Give reader thread time to start running
        thread::sleep(Duration::from_millis(10));

        // Start eviction requiring 9,216 bytes (36 segments across tenants)
        is_evicting.store(true, Ordering::SeqCst);
        let freed = store.evict_lru_fair(9_216);
        is_evicting.store(false, Ordering::SeqCst);
        eviction_done.store(true, Ordering::SeqCst);

        reader_handle.join().expect("reader thread panicked");

        assert!(freed >= 9_216, "must free at least 9,216 bytes");
        assert_eq!(store.get_tenant_segment_len(tenant_a), 18);
        assert_eq!(store.get_tenant_segment_len(tenant_b), 18);
        assert_eq!(store.get_tenant_segment_len(tenant_c), 38);

        let successful_reads = reads_during_eviction.load(Ordering::SeqCst);
        assert!(
            successful_reads > 0,
            "Reader thread must execute at least one successful read while eviction is in progress (got {successful_reads} reads)"
        );
    }
}
