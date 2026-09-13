// FILE-CONTEXT
// STAND: 2026-09-12T00:00:00Z (SESSION: KV-BRIDGE-ADAPTER-IMPL)
// ZWECK: Adapter connecting retrieval segments to Candle inference engine using encrypted tenant-isolated KV cache store.
// INVARIANTEN: Zero-panic policy on cache miss or crypto error (fallback to full prefill).
// Synchronous parking_lot locks are never held across async await points.

//! KV-Bridge Adapter connecting Candle inference to tenant-isolated encrypted KV cache store.

#![cfg(feature = "kv-bridge")]

use memfuse_core::traits::ContextSegment;
use memfuse_core::{ModelFingerprint, TenantId};
use memfuse_crypto::{KvSegmentCipher, TenantIsolatedKvStore};
use std::sync::Arc;

/// Adapter bridge managing encrypted KV-cache segment lookup and storage for Candle inference sessions.
#[derive(Clone)]
pub struct KvBridgeAdapter {
    /// Tenant-isolated KV segment storage engine.
    pub store: Arc<TenantIsolatedKvStore>,
    /// High-level cipher engine for segment encryption and decryption.
    pub cipher: Arc<KvSegmentCipher>,
    /// Number of segment consultations performed.
    pub consultations: Arc<std::sync::atomic::AtomicU64>,
}

impl KvBridgeAdapter {
    /// Creates a new `KvBridgeAdapter` wrapping the given store and cipher handles.
    pub fn new(store: Arc<TenantIsolatedKvStore>, cipher: Arc<KvSegmentCipher>) -> Self {
        Self {
            store,
            cipher,
            consultations: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// Consults segment metadata and KV cache bridge state for a context segment.
    pub fn consult_segment<'a>(&self, segment: &ContextSegment<'a>) {
        self.consultations
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let _ = (
            segment.chunk_id,
            segment.text,
            segment.model_fingerprint,
            segment.rope_offset,
        );
    }

    /// Returns the number of segment consultations recorded.
    pub fn consultation_count(&self) -> u64 {
        self.consultations.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Attempts to retrieve and decrypt a cached KV segment for a tenant and chunk ID.
    ///
    /// Returns `None` on cache miss, format mismatch, or decryption failure,
    /// triggering a transparent fallback to full prefill in the inference pipeline (APM-HARD-FAIL-ON-CACHE-MISS).
    // AI-TAG[SECURITY][MAJOR] try_get_cached_segment ignores caller requested fingerprint (ID: AGT-CANDLE-d0dacdd8) (TS: 2026-09-13T01:40:00Z) (SESSION: 50c8c755)
    // BEFUND: try_get_cached_segment ignores the _fingerprint argument and returns decrypted KV segments from TenantIsolatedKvStore regardless of requested model quantization or fingerprint.
    // RISIKO: When switching models or quantization levels, cached KV segments from a different model variant could be re-used, causing corrupted tensor attention states or inference crashes.
    // EMPFEHLUNG: Verify caller's fingerprint against stored segment metadata in TenantIsolatedKvStore or KvBridgeAdapter before returning cached KV bytes.
    pub fn try_get_cached_segment(
        &self,
        tenant: TenantId,
        chunk_id: u64,
        _fingerprint: &ModelFingerprint,
        _rope_offset: Option<usize>,
    ) -> Option<Vec<u8>> {
        match self
            .store
            .get_decrypted_segment(&self.cipher, tenant, chunk_id)
        {
            Ok(Some(bytes)) => Some(bytes),
            Ok(None) => None,
            Err(err) => {
                tracing::debug!(
                    tenant_id = tenant.inner(),
                    chunk_id = chunk_id,
                    error = %err,
                    "KV-Bridge cache miss due to decryption/format error, falling back to prefill"
                );
                None
            }
        }
    }

    /// Encrypts and stores a KV-cache segment for a given tenant and chunk ID.
    ///
    /// Failures during store/encryption are logged as warnings and non-fatal (never propagated).
    pub fn store_segment(
        &self,
        tenant: TenantId,
        chunk_id: u64,
        fingerprint: ModelFingerprint,
        rope_offset: Option<usize>,
        plaintext_kv_bytes: &[u8],
    ) {
        if let Err(err) = self.store.insert_encrypted_segment(
            &self.cipher,
            tenant,
            chunk_id,
            fingerprint,
            rope_offset,
            plaintext_kv_bytes,
        ) {
            tracing::warn!(
                tenant_id = tenant.inner(),
                chunk_id = chunk_id,
                error = %err,
                "Failed to insert encrypted segment into KV-Bridge store"
            );
        }
    }
}

impl std::fmt::Debug for KvBridgeAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KvBridgeAdapter")
            .field("store", &"<TenantIsolatedKvStore>")
            .field("cipher", &"<KvSegmentCipher>")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use memfuse_crypto::{CryptoKey, EvictionWorker};
    use std::sync::Arc;
    use std::thread;

    fn create_test_adapter() -> KvBridgeAdapter {
        let master_km = CryptoKey::try_new("test-passphrase-kv", b"test-salt-12345").unwrap();
        let cipher = Arc::new(KvSegmentCipher::new(master_km));
        let store = Arc::new(TenantIsolatedKvStore::new());
        KvBridgeAdapter::new(store, cipher)
    }

    fn dummy_fp() -> ModelFingerprint {
        ModelFingerprint::new([0x77u8; 32], "llama-3.2-1b.gguf", "Q4_K_M")
    }

    #[test]
    fn test_cache_miss_returns_none() {
        let adapter = create_test_adapter();
        let tenant = TenantId::try_new(1).unwrap();
        let fp = dummy_fp();

        let cached = adapter.try_get_cached_segment(tenant, 999, &fp, None);
        assert!(
            cached.is_none(),
            "Cache miss MUST return None without panic"
        );
    }

    #[test]
    fn test_store_and_get_roundtrip() {
        let adapter = create_test_adapter();
        let tenant = TenantId::try_new(10).unwrap();
        let fp = dummy_fp();
        let chunk_id = 42;
        let payload = b"KV tensor keys and values plaintext cache payload";

        adapter.store_segment(tenant, chunk_id, fp.clone(), Some(128), payload);

        let retrieved = adapter.try_get_cached_segment(tenant, chunk_id, &fp, Some(128));
        assert!(retrieved.is_some(), "Stored segment MUST be retrievable");
        assert_eq!(retrieved.unwrap(), payload);
    }

    #[test]
    fn test_tenant_isolation_strictness() {
        let adapter = create_test_adapter();
        let tenant_a = TenantId::try_new(101).unwrap();
        let tenant_b = TenantId::try_new(202).unwrap();
        let fp = dummy_fp();
        let chunk_id = 1;
        let payload_a = b"Secret payload of Tenant A";

        adapter.store_segment(tenant_a, chunk_id, fp.clone(), None, payload_a);

        // Tenant A gets its data
        let retrieved_a = adapter.try_get_cached_segment(tenant_a, chunk_id, &fp, None);
        assert_eq!(retrieved_a, Some(payload_a.to_vec()));

        // Tenant B trying to get chunk_id 1 under tenant_b gets None
        let retrieved_b = adapter.try_get_cached_segment(tenant_b, chunk_id, &fp, None);
        assert!(
            retrieved_b.is_none(),
            "Tenant B MUST NOT access Tenant A's cached segment"
        );
    }

    #[test]
    fn test_concurrency_parallel_requests_and_eviction() {
        let master_km = CryptoKey::try_new("concurrency-passphrase", b"salt-987654321").unwrap();
        let cipher = Arc::new(KvSegmentCipher::new(master_km));
        let store = Arc::new(TenantIsolatedKvStore::new());
        let adapter = KvBridgeAdapter::new(Arc::clone(&store), cipher);

        // Spawn EvictionWorker
        let worker = EvictionWorker::spawn(Arc::clone(&store));

        let adapter1 = adapter.clone();
        let adapter2 = adapter.clone();

        let tenant1 = TenantId::try_new(1).unwrap();
        let tenant2 = TenantId::try_new(2).unwrap();
        let fp = dummy_fp();

        let fp1 = fp.clone();
        let handle1 = thread::spawn(move || {
            for chunk_id in 0..50 {
                let data = vec![(chunk_id % 256) as u8; 128];
                adapter1.store_segment(tenant1, chunk_id, fp1.clone(), None, &data);
                let _ = adapter1.try_get_cached_segment(tenant1, chunk_id, &fp1, None);
            }
        });

        let fp2 = fp;
        let handle2 = thread::spawn(move || {
            for chunk_id in 100..150 {
                let data = vec![(chunk_id % 256) as u8; 128];
                adapter2.store_segment(tenant2, chunk_id, fp2.clone(), None, &data);
                let _ = adapter2.try_get_cached_segment(tenant2, chunk_id, &fp2, None);
            }
        });

        handle1.join().expect("Thread 1 panicked");
        handle2.join().expect("Thread 2 panicked");

        worker.shutdown();
    }
}
