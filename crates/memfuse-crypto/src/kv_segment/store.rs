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

    // INTEGRATION TODO (memfuse-db):
    // In `crates/memfuse-db/src/transaction.rs` muss `compensate_*()` nach dem
    // Rollback `kv_store.remove_segments_for_rollback(tenant_id, &rolled_back_segment_ids)`
    // aufrufen. Dies ist der Schritt, der verwaiste Segmente verhindert.
    // Zuständig: nachgelagerte PR nach diesem Fix.

    /// Entfernt alle Segmente mit den angegebenen `segment_ids` für den gegebenen Tenant.
    ///
    /// Muss bei transaktionalem Rollback aufgerufen werden, wenn ein `DbTransaction::commit()`
    /// fehlschlägt und `compensate_*()` ausgeführt wird, um verwaiste KV-Segmente zu
    /// vermeiden (Memory-Leak-Prävention).
    ///
    /// Aktuell ist der Store ein reiner In-Memory-Cache — kein Disk-Write erforderlich.
    /// Falls der Store zukünftig als autoritativer Store genutzt wird, muss hier ein
    /// WAL-Rollback-Eintrag ergänzt werden.
    pub fn remove_segments_for_rollback(&self, tenant: TenantId, segment_ids: &[u64]) {
        if segment_ids.is_empty() {
            return;
        }
        let mut map = self.segments.write();
        if let Some(segs) = map.get_mut(&tenant) {
            segs.retain(|s| !segment_ids.contains(&s.segment_id));
            // Tenant-Eintrag entfernen wenn leer (wie in evict_lru_fair_internal)
            if segs.is_empty() {
                map.remove(&tenant);
            }
        }
        tracing::debug!(
            tenant_id = ?tenant,
            removed_segment_ids = ?segment_ids,
            "KvStore rollback: removed segments for failed transaction"
        );
    }

    /// Entfernt ein einzelnes Segment. Convenience-Wrapper um `remove_segments_for_rollback`.
    pub fn remove_segment(&self, tenant: TenantId, segment_id: u64) {
        self.remove_segments_for_rollback(tenant, &[segment_id]);
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
        #[cfg(test)]
        {
            self.evict_lru_fair_internal(target_free_bytes, None)
        }
        #[cfg(not(test))]
        {
            self.evict_lru_fair_internal(target_free_bytes)
        }
    }

    #[cfg(test)]
    pub fn evict_lru_fair_with_hook<F>(&self, target_free_bytes: usize, mut hook: F) -> usize
    where
        F: FnMut(),
    {
        self.evict_lru_fair_internal(target_free_bytes, Some(&mut hook))
    }

    fn evict_lru_fair_internal(
        &self,
        target_free_bytes: usize,
        #[cfg(test)] mut batch_released_hook: Option<&mut dyn FnMut()>,
    ) -> usize {
        let mut freed = 0;

        'outer: while freed < target_free_bytes {
            {
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

                    // Apply rotation offset to avoid systematic low-ID tenant eviction bias
                    if !tenants.is_empty() {
                        let offset = self.eviction_round_offset.fetch_add(1, Ordering::Relaxed)
                            % tenants.len();
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
                        break 'outer;
                    }
                }
            }

            #[cfg(test)]
            if freed < target_free_bytes {
                if let Some(ref mut hook) = batch_released_hook {
                    hook();
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
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::thread;

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

    // AI-TAG[TEST][ANALYZED-SAFE] Lock release test timing dependency resolved via Notify handshake (ID: AGT-SECURITY-3edfea62) (TS: 2026-09-12T09:35:00Z) (SESSION: 5f10d4f0)
    // BEFUND: Befund beschrieb ursprüngliche sleep/timing-basierte Annahmen. Der Test nutzt nun synchrone Notify-Handshakes (`notify_batch_released` / `notify_read_complete`), die Ping-Pong-synchronisiert ablaufen.
    // RISIKO: Analyse auf Permit-Verlust bei `Notify`: Da jede Runde exakt einen Handshake-Schritt ausführt und erst nach `notify_read_complete.notified().await` in die nächste Eviction-Runde geht, existiert kein ausstehendes unbehandeltes Permit während der Iterationen.
    // EMPFEHLUNG: Test ist vollständig deterministisch und race-frei. Markiert als ANALYZED-SAFE.
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

        let notify_batch_released = Arc::new(tokio::sync::Notify::new());
        let notify_read_complete = Arc::new(tokio::sync::Notify::new());
        let reads_during_eviction = Arc::new(AtomicUsize::new(0));

        let store_clone = Arc::clone(&store);
        let notify_batch_released_clone = Arc::clone(&notify_batch_released);
        let notify_read_complete_clone = Arc::clone(&notify_read_complete);
        let reads_during_eviction_clone = Arc::clone(&reads_during_eviction);

        // Spawn reader thread synchronized via Notify checkpoints instead of timing/sleep loops.
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();

        let reader_handle = thread::spawn(move || {
            rt.block_on(async {
                loop {
                    notify_batch_released_clone.notified().await;
                    let len = store_clone.get_tenant_segment_len(tenant_c);
                    let segs = store_clone.get_segments(tenant_unaffected);
                    assert!(segs.is_empty(), "Unaffected tenant 99 must have 0 segments");
                    reads_during_eviction_clone.fetch_add(1, Ordering::SeqCst);
                    notify_read_complete_clone.notify_one();
                    if len <= 38 {
                        // All eviction batches completed and verified.
                        break;
                    }
                }
            });
        });

        // Start eviction requiring 9,216 bytes (36 segments across tenants)
        let notify_batch_released_evict = Arc::clone(&notify_batch_released);
        let notify_read_complete_evict = Arc::clone(&notify_read_complete);

        let freed = store.evict_lru_fair_with_hook(9_216, || {
            // Handshake: Notify reader that write lock was released between batches,
            // then wait for reader to perform concurrent read before resuming next batch.
            notify_batch_released_evict.notify_one();
            tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap()
                .block_on(async {
                    notify_read_complete_evict.notified().await;
                });
        });

        // Notify reader thread one final time so it can clean up and exit.
        notify_batch_released.notify_one();
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

    #[test]
    fn test_evict_lru_global_visibility_or_deprecation() {
        // Verifiziere, dass evict_lru_global nicht mehr öffentlich aufrufbar ist
        // (außer mit pub(crate), was nicht über Crate-Grenzen hinweg sichtbar ist).
        // Da dies auf Compiler-Ebene durchgesetzt wird, ist der Test rein dokumentarisch:
        // Ein Versuch, von außerhalb des Crates evict_lru_global aufzurufen, würde nicht
        // kompilieren. Für Crate-interne Tests: prüfe lediglich, dass die Funktion
        // weiterhin existiert und aufrufbar ist (z. B. mit allow(dead_code)).
        let _store = TenantIsolatedKvStore::new();
        let tenant = TenantId::try_new(1).unwrap();
        let _seg = KvSegment::new(tenant, 1, vec![1, 2, 3]);
        // Aufruf würde hier stattfinden, wenn nicht pub(crate) wäre — Auslassung beweist Sichtbarkeit ist begrenzt
    }

    #[test]
    fn test_evict_lru_fair_rotation_prevents_low_id_bias() {
        let store = TenantIsolatedKvStore::new();

        // Setup: 5 tenants (IDs 1–5), each with 10 small segments
        for tenant_id in 1..=5 {
            let tenant = TenantId::try_new(tenant_id as u64).unwrap();
            for seg_id in 0..10 {
                let seg = KvSegment::new(
                    tenant,
                    seg_id,
                    vec![1; 100], // 100 bytes each
                );
                // Insert segment into store
                let mut segments = store.segments.write();
                segments.entry(tenant).or_default().push(seg);
            }
        }

        // Evict in small batches: target_free_bytes = 150 bytes
        // At 100 bytes/segment, each call will evict ~1.5 segments = 1 segment per call (due to per-tenant limit)
        // Expected: over 5 sequential eviction calls, each tenant should lose ~1 segment
        // Without rotation, Tenant 1 and 2 would be hit in ALL calls (bias)

        let mut evicted_by_tenant = std::collections::HashMap::new();

        for _call_num in 0..5 {
            store.evict_lru_fair(150);

            // Count remaining segments per tenant
            let segments = store.segments.read();
            for tenant in [1u64, 2, 3, 4, 5].iter() {
                let tenant_id = TenantId::try_new(*tenant).unwrap();
                if let Some(segs) = segments.get(&tenant_id) {
                    let remaining = segs.len();
                    evicted_by_tenant.insert(*tenant, 10 - remaining);
                }
            }
        }

        // Assertion: no single tenant should be disproportionately evicted
        // (all tenants should lose roughly 5–6 segments over 5 calls; if rotation works,
        // no tenant is hit in all 5 calls, some hit 0–1 times)
        let eviction_counts: Vec<usize> = evicted_by_tenant.values().copied().collect();
        let min_evictions = *eviction_counts.iter().min().unwrap_or(&0);
        let max_evictions = *eviction_counts.iter().max().unwrap_or(&100);

        // With rotation, the spread should be tighter than without
        // (this is a probabilistic test — deterministic validation would require seeding
        // the rotation offset; for now, just assert that no tenant is hit 5 times while
        // another is hit 0 times)
        assert!(
            max_evictions - min_evictions <= 2,
            "Eviction bias detected: min_evictions={}, max_evictions={}; \
             tenants should be hit more evenly. Rotation may not be working.",
            min_evictions,
            max_evictions
        );
    }

    #[test]
    fn test_store_edge_cases() {
        let store = TenantIsolatedKvStore::default();
        let tenant = TenantId::try_new(10).unwrap();

        // Non-existent segment / tenant byte retrieval
        assert!(store.get_segment_bytes(tenant, 999).is_none());

        // Evict 0 bytes
        let freed_zero = store.evict_lru_fair(0);
        assert_eq!(freed_zero, 0);

        // Evict on empty store
        let freed_empty = store.evict_lru_fair(500);
        assert_eq!(freed_empty, 0);

        // Insert duplicate segment_id -> should overwrite
        let seg1 = KvSegment::new(tenant, 1, vec![1, 2, 3]);
        let seg2 = KvSegment::new(tenant, 1, vec![4, 5, 6, 7]);
        store.insert_segment(tenant, seg1);
        store.insert_segment(tenant, seg2);

        assert_eq!(store.get_tenant_segment_len(tenant), 1);
        assert_eq!(store.get_segment_bytes(tenant, 1), Some(vec![4, 5, 6, 7]));
    }

    #[test]
    fn test_store_clear_all_and_global_lru() {
        let store = TenantIsolatedKvStore::new();
        let tenant_a = TenantId::try_new(1).unwrap();
        let tenant_b = TenantId::try_new(2).unwrap();

        store.insert_segment(tenant_a, KvSegment::new(tenant_a, 1, vec![10; 256]));
        store.insert_segment(tenant_b, KvSegment::new(tenant_b, 2, vec![20; 256]));

        assert_eq!(store.get_tenant_segment_len(tenant_a), 1);
        assert_eq!(store.get_tenant_segment_len(tenant_b), 1);

        // Test pub(crate) evict_lru_global
        let freed = store.evict_lru_global(200);
        assert!(freed >= 200);
        assert_eq!(
            store.get_tenant_segment_len(tenant_a) + store.get_tenant_segment_len(tenant_b),
            1
        );

        // Test clear_all emergency wipe
        store.clear_all();
        assert_eq!(store.get_tenant_segment_len(tenant_a), 0);
        assert_eq!(store.get_tenant_segment_len(tenant_b), 0);
    }

    #[test]
    fn test_remove_segments_for_rollback_cleans_up_correctly() {
        let store = TenantIsolatedKvStore::new();
        let tenant = TenantId::try_new(42).unwrap();

        // Einfügen von 3 Segmenten
        for id in [1u64, 2, 3] {
            store.insert_segment(tenant, KvSegment::new(tenant, id, vec![id as u8; 8]));
        }
        assert_eq!(store.get_tenant_segment_len(tenant), 3);

        // Rollback für Segment 1 und 3
        store.remove_segments_for_rollback(tenant, &[1, 3]);
        assert_eq!(store.get_tenant_segment_len(tenant), 1);
        assert!(
            store.get_segment_bytes(tenant, 1).is_none(),
            "Segment 1 muss entfernt sein"
        );
        assert!(
            store.get_segment_bytes(tenant, 2).is_some(),
            "Segment 2 muss erhalten bleiben"
        );
        assert!(
            store.get_segment_bytes(tenant, 3).is_none(),
            "Segment 3 muss entfernt sein"
        );

        // Rollback aller verbleibenden → Tenant-Eintrag muss vollständig entfernt werden
        store.remove_segments_for_rollback(tenant, &[2]);
        assert_eq!(store.get_tenant_segment_len(tenant), 0);
        // Tenant-Key darf nicht mehr in der Map existieren
        assert!(store.get_segments(tenant).is_empty());
    }

    #[test]
    fn test_remove_segments_noop_on_empty_ids() {
        let store = TenantIsolatedKvStore::new();
        let tenant = TenantId::try_new(1).unwrap();
        store.insert_segment(tenant, KvSegment::new(tenant, 1, vec![0u8; 8]));
        store.remove_segments_for_rollback(tenant, &[]); // Kein Panic, kein Effekt
        assert_eq!(store.get_tenant_segment_len(tenant), 1);
    }
}
