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

    /// Evictiert KV-Segmente unter Erhaltung von Tenant-Fairness via Round-Robin über alle
    /// aktive Tenants.
    ///
    /// FAIRNESS-DEFINITION: Pro Runde wird höchstens EIN Segment pro aktivem Tenant
    /// evictiert, unabhängig von dessen Byte-Größe. Dies ist Count-Fairness pro Runde,
    /// NICHT Byte-Fairness — bei stark heterogenen Segmentgrößen zwischen Tenants kann ein
    /// Tenant mit großen Segmenten pro Runde deutlich mehr Bytes verlieren als einer mit
    /// kleinen Segmenten, obwohl beide "fair" (je 1 Segment/Runde) behandelt werden.
    ///
    /// Im Gegensatz zu `evict_lru_global()` verhindert diese Methode, dass sehr aktive
    /// Tenants inaktive Tenants vollständig verdrängen (Prevent Cross-Tenant Starvation).
    // AI-TAG[SECURITY][MAJOR][RESOLVED] Add tenant-fair LRU eviction to prevent cross-tenant starvation (ID: AGT-CRYPTO-c1a93b22) (TS: 2026-09-10T10:00:00Z)
    pub fn evict_lru_fair(&self, target_free_bytes: usize) -> usize {
        let mut map = self.segments.write();
        let mut freed = 0;

        while freed < target_free_bytes && !map.is_empty() {
            let mut tenants: Vec<TenantId> = map.keys().copied().collect();
            tenants.sort();
            if !tenants.is_empty() {
                let offset =
                    self.eviction_round_offset.fetch_add(1, Ordering::Relaxed) % tenants.len();
                tenants.rotate_left(offset);
            }

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
                break;
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
    fn test_evict_lru_global_visibility_or_deprecation() {
        let store = TenantIsolatedKvStore::new();
        let tenant = TenantId::try_new(1).unwrap();
        store.insert_segment(tenant, KvSegment::new(tenant, 1, vec![0x11; 100]));

        // Verify that evict_lru_global is callable within crate (pub(crate))
        let freed = store.evict_lru_global(100);
        assert_eq!(freed, 100);
        assert_eq!(store.get_tenant_segment_len(tenant), 0);
    }

    #[test]
    fn test_evict_lru_fair_rotation_prevents_low_id_bias() {
        let store = TenantIsolatedKvStore::new();
        let tenants: Vec<TenantId> = (1..=5).map(|id| TenantId::try_new(id).unwrap()).collect();

        // Populate 5 tenants with 5 segments each (100 bytes each)
        for tenant in &tenants {
            for seg_id in 1..=5 {
                store.insert_segment(*tenant, KvSegment::new(*tenant, seg_id, vec![0xAA; 100]));
            }
        }

        // Call 1: target_free_bytes = 200 (evicts 2 segments total across 2 tenants)
        // Offset starts at 0 -> Tenants 1 & 2 evicted
        store.evict_lru_fair(200);
        assert_eq!(store.get_tenant_segment_len(tenants[0]), 4); // Tenant 1
        assert_eq!(store.get_tenant_segment_len(tenants[1]), 4); // Tenant 2
        assert_eq!(store.get_tenant_segment_len(tenants[2]), 5); // Tenant 3

        // Call 2: target_free_bytes = 200 (evicts 2 segments total)
        // Offset becomes 1 -> rotated order [2, 3, 4, 5, 1] -> Tenants 2 & 3 evicted
        store.evict_lru_fair(200);

        // Without rotation, Call 2 would evict from Tenants 1 & 2 again (leaving Tenant 1 with 3 and Tenant 3 with 5).
        // With rotation, Call 2 evicts from Tenants 2 & 3:
        // Tenant 1 stays at 4, Tenant 2 drops to 3, Tenant 3 drops to 4.
        assert_eq!(
            store.get_tenant_segment_len(tenants[2]),
            4,
            "Tenant 3 must be evicted on Call 2 due to round-robin rotation offset"
        );
        assert_eq!(
            store.get_tenant_segment_len(tenants[0]),
            4,
            "Tenant 1 must be skipped on Call 2 due to round-robin rotation offset"
        );

        // Call 3: target_free_bytes = 200 (evicts 2 segments total)
        // Offset becomes 2 -> rotated order [3, 4, 5, 1, 2] -> Tenants 3 & 4 evicted
        store.evict_lru_fair(200);
        assert_eq!(store.get_tenant_segment_len(tenants[3]), 4); // Tenant 4 evicted!
    }
}
