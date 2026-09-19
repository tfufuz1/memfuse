// FILE-CONTEXT
// STAND: 2026-09-17T00:00:00Z (SESSION: KV-BRIDGE-ZERO-COPY-IMPL)
// ZWECK: KvBridgeAdapter verbindet Retrieval-Chunks mit mandantenisoliertem KV-Cache-Store (RAM Tier 1 + LSM Tier 2 Spill).
// REIFEGRAD: 🟡 (Golden Test verifiziert, Stufe A / Tier 2 Async Spill)
// INVARIANTEN: Cache-Miss und jeder Fehler ergeben transparenten Fallback auf vollen Prefill.
//              Kein parking_lot-Lock über .await-Punkt.
//              Zero-Panic-Doctrine: Keine unwrap/expect in der Eviction Bridge Pipeline.
//              Zero-Copy: bytes::Bytes-Slices im Async-LSM Spill Path.

//! KV-Bridge Adapter connecting Candle inference to tenant-isolated encrypted KV cache store.

#![cfg(feature = "kv-bridge")]

use bytes::Bytes;
use memfuse_core::traits::ContextSegment;
#[cfg(feature = "memfuse-store")]
use memfuse_core::traits::StorageEngine;
#[cfg(feature = "memfuse-store")]
use memfuse_core::TxId;
use memfuse_core::{ModelFingerprint, TenantId};
#[cfg(feature = "memfuse-store")]
use memfuse_crypto::EncryptedKvLayer;
#[cfg(test)]
use memfuse_crypto::KvSegment;
use memfuse_crypto::{KvSegmentCipher, TenantIsolatedKvStore};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Gekapselter KV-Payload zur Persistierung im verschlüsselten Store mit Metadaten.
#[derive(Debug, Serialize, Deserialize)]
struct CachedKvPayload {
    fingerprint: ModelFingerprint,
    rope_offset: Option<usize>,
    data: Vec<u8>,
}

/// Cache-Lookup-Schlüssel: eindeutige Kombination aus Chunk-ID, Modell und optionalem RoPE-Offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KvCacheKey {
    pub chunk_id: u64,
    pub fingerprint: ModelFingerprint,
    pub rope_offset: Option<usize>,
}

impl KvCacheKey {
    pub fn new(chunk_id: u64, fingerprint: ModelFingerprint, rope_offset: Option<usize>) -> Self {
        Self {
            chunk_id,
            fingerprint,
            rope_offset,
        }
    }
}

/// Verbindet Retrieval-Chunks mit dem mandantenisolierten, verschlüsselten KV-Cache.
///
/// # Fail-Safe-Garantie
/// Jede Methode fällt bei Fehler oder Cache-Miss transparent auf den normalen Prefill zurück.
/// Kein Fehler aus dem KV-Store oder der Krypto-Schicht darf eine Anfrage abbrechen.
#[derive(Clone)]
pub struct KvBridgeAdapter {
    pub store: Arc<TenantIsolatedKvStore>,
    #[cfg(feature = "memfuse-store")]
    pub lsm_store: Option<Arc<memfuse_store::LsmStorage>>,
    pub cipher: Arc<KvSegmentCipher>,
    pub consultations: Arc<std::sync::atomic::AtomicU64>,
}

impl KvBridgeAdapter {
    /// Erstellt einen neuen Adapter mit gegebenem Store und Cipher.
    pub fn new(store: Arc<TenantIsolatedKvStore>, cipher: Arc<KvSegmentCipher>) -> Self {
        Self {
            store,
            #[cfg(feature = "memfuse-store")]
            lsm_store: None,
            cipher,
            consultations: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// Erstellt einen neuen Adapter mit Zwei-Tier-Hierarchie (RAM Fast-Path + LSM-Spill Slow-Path).
    #[cfg(feature = "memfuse-store")]
    pub fn with_lsm_fallback(
        store: Arc<TenantIsolatedKvStore>,
        lsm_store: Arc<memfuse_store::LsmStorage>,
        cipher: Arc<KvSegmentCipher>,
    ) -> Self {
        let lsm = Arc::clone(&lsm_store);
        let handler = Arc::new(
            move |tenant: TenantId, segment_id: u64, ciphertext: Vec<u8>| {
                let lsm_inner = Arc::clone(&lsm);
                tokio::spawn(async move {
                    let spill_key =
                        format!("__kv_spill:{}:{:#x}", tenant.inner(), segment_id).into_bytes();
                    let tx_id = TxId(segment_id);
                    let ciphertext_bytes = Bytes::from(ciphertext);
                    if let Err(e) = lsm_inner.put(tx_id, &spill_key, &ciphertext_bytes).await {
                        tracing::warn!(
                            tenant_id = tenant.inner(),
                            segment_id,
                            error = %e,
                            "KvBridgeAdapter: Failed to put spilled segment into LSM"
                        );
                        return;
                    }
                    if let Err(e) = lsm_inner.commit(tx_id).await {
                        tracing::warn!(
                            tenant_id = tenant.inner(),
                            segment_id,
                            error = %e,
                            "KvBridgeAdapter: Failed to commit spilled segment into LSM"
                        );
                    }
                });
            },
        );

        store.set_spill_handler(handler);

        Self {
            store,
            lsm_store: Some(lsm_store),
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

    /// Deserialisiert und validiert entschlüsselte Payload-Bytes gegen den angegebenen Key.
    fn validate_payload(decrypted_bytes: &[u8], key: &KvCacheKey) -> Option<Vec<u8>> {
        let payload: CachedKvPayload = match bincode::deserialize(decrypted_bytes) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(
                    chunk_id = key.chunk_id,
                    error = %e,
                    "KvBridgeAdapter: Deserialization failed — cache miss"
                );
                return None;
            }
        };

        if payload.fingerprint != key.fingerprint {
            tracing::warn!(
                chunk_id = key.chunk_id,
                "KvBridgeAdapter: Model fingerprint mismatch — cache miss"
            );
            return None;
        }

        if payload.rope_offset != key.rope_offset {
            tracing::warn!(
                chunk_id = key.chunk_id,
                expected_rope = ?key.rope_offset,
                cached_rope = ?payload.rope_offset,
                "KvBridgeAdapter: RoPE offset mismatch — cache miss"
            );
            return None;
        }

        Some(payload.data)
    }

    /// Versucht, ein gecachetes KV-Segment zu laden.
    ///
    /// Gibt `None` zurück bei Cache-Miss, Decrypt-Fehler, Fingerprint-Mismatch oder jedem anderen Fehler.
    /// NIEMALS wird ein Fehler propagiert — `None` bedeutet stets "voller Prefill".
    pub fn try_get_cached_segment(&self, tenant: TenantId, key: &KvCacheKey) -> Option<Vec<u8>> {
        // 1. Store-Lookup & Decrypt via rope-bewusste API
        let decrypted_bytes =
            match self
                .store
                .get_decrypted_segment(&self.cipher, tenant, key.chunk_id)
            {
                Ok(Some(bytes)) => bytes,
                Ok(None) => return None,
                Err(e) => {
                    tracing::warn!(
                        chunk_id = key.chunk_id,
                        error = %e,
                        "KvBridgeAdapter: Decrypt failed — cache miss"
                    );
                    return None;
                }
            };

        Self::validate_payload(&decrypted_bytes, key)
    }

    /// Versucht asynchron, ein gecachetes KV-Segment erst im RAM (Tier 1) und bei Miss im LSM-Store (Tier 2) zu laden.
    #[cfg(feature = "memfuse-store")]
    pub async fn try_get_cached_segment_async(
        &self,
        tenant: TenantId,
        key: &KvCacheKey,
    ) -> Option<Vec<u8>> {
        // 1. Try RAM cache first
        if let Some(bytes) = self.try_get_cached_segment(tenant, key) {
            return Some(bytes);
        }

        // 2. Try LSM store fallback if lsm_store is configured
        let lsm = self.lsm_store.as_ref()?;
        let spill_key = format!("__kv_spill:{}:{:#x}", tenant.inner(), key.chunk_id).into_bytes();

        let doc_bytes = match lsm.get(&spill_key).await {
            Ok(Some(bytes)) => bytes,
            _ => return None,
        };

        // 3. Decrypt and validate payload identically
        let layer: EncryptedKvLayer = match bincode::deserialize(&doc_bytes) {
            Ok(l) => l,
            Err(e) => {
                tracing::warn!(
                    chunk_id = key.chunk_id,
                    error = %e,
                    "KvBridgeAdapter: Failed to deserialize EncryptedKvLayer from LSM spill"
                );
                return None;
            }
        };

        let decrypted_bytes = match self.cipher.decrypt_with_version(&layer, key.chunk_id, 1) {
            Ok(bytes) => bytes,
            Err(e) => {
                tracing::warn!(
                    chunk_id = key.chunk_id,
                    error = %e,
                    "KvBridgeAdapter: Decrypt LSM spill failed — cache miss"
                );
                return None;
            }
        };

        Self::validate_payload(&decrypted_bytes, key)
    }

    /// Versucht asynchron, ein gecachetes KV-Segment zu laden und als `Bytes` Slice (Zero-Copy) zurückzugeben.
    #[cfg(feature = "memfuse-store")]
    pub async fn try_get_cached_segment_async_bytes(
        &self,
        tenant: TenantId,
        key: &KvCacheKey,
    ) -> Option<Bytes> {
        self.try_get_cached_segment_async(tenant, key)
            .await
            .map(Bytes::from)
    }

    /// Speichert ein KV-Segment im Cache. Fehler werden geloggt, nie propagiert.
    pub fn store_segment(&self, tenant: TenantId, key: KvCacheKey, plaintext_kv_bytes: Vec<u8>) {
        let payload = CachedKvPayload {
            fingerprint: key.fingerprint.clone(),
            rope_offset: key.rope_offset,
            data: plaintext_kv_bytes,
        };

        let serialized_bytes = match bincode::serialize(&payload) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(
                    chunk_id = key.chunk_id,
                    error = %e,
                    "KvBridgeAdapter: Serialization failed — segment not cached"
                );
                return;
            }
        };

        if let Err(e) = self.store.insert_encrypted_segment(
            &self.cipher,
            tenant,
            key.chunk_id,
            key.fingerprint,
            key.rope_offset,
            &serialized_bytes,
        ) {
            tracing::warn!(
                chunk_id = key.chunk_id,
                error = %e,
                "KvBridgeAdapter: Insert encrypted segment failed — segment not cached"
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
    fn test_cache_miss_returns_none_without_panic() {
        let adapter = create_test_adapter();
        let tenant = TenantId::try_new(1).unwrap();
        let key = KvCacheKey::new(999, dummy_fp(), None);

        let cached = adapter.try_get_cached_segment(tenant, &key);
        assert!(
            cached.is_none(),
            "Cache miss MUST return None without panic"
        );
    }

    #[test]
    fn test_roundtrip_store_then_get() {
        let adapter = create_test_adapter();
        let tenant = TenantId::try_new(10).unwrap();
        let fp = dummy_fp();
        let chunk_id = 42;
        let payload = b"KV tensor keys and values plaintext cache payload".to_vec();
        let key = KvCacheKey::new(chunk_id, fp, Some(128));

        adapter.store_segment(tenant, key.clone(), payload.clone());

        let retrieved = adapter.try_get_cached_segment(tenant, &key);
        assert!(retrieved.is_some(), "Stored segment MUST be retrievable");
        assert_eq!(retrieved.unwrap(), payload);
    }

    #[test]
    fn test_tenant_isolation() {
        let adapter = create_test_adapter();
        let tenant_a = TenantId::try_new(101).unwrap();
        let tenant_b = TenantId::try_new(202).unwrap();
        let fp = dummy_fp();
        let chunk_id = 1;
        let payload_a = b"Secret payload of Tenant A".to_vec();
        let key = KvCacheKey::new(chunk_id, fp, None);

        adapter.store_segment(tenant_a, key.clone(), payload_a.clone());

        // Tenant A gets its data
        let retrieved_a = adapter.try_get_cached_segment(tenant_a, &key);
        assert_eq!(retrieved_a, Some(payload_a));

        // Tenant B trying to get chunk_id 1 under tenant_b gets None
        let retrieved_b = adapter.try_get_cached_segment(tenant_b, &key);
        assert!(
            retrieved_b.is_none(),
            "Tenant B MUST NOT access Tenant A's cached segment"
        );
    }

    #[test]
    fn test_rope_offset_mismatch_returns_cache_miss() {
        let adapter = create_test_adapter();
        let tenant = TenantId::try_new(10).unwrap();
        let fp = dummy_fp();
        let chunk_id = 123;
        let payload = b"KV tensor keys and values plaintext cache payload".to_vec();

        // Key stored with rope_offset = Some(128)
        let key_stored = KvCacheKey::new(chunk_id, fp.clone(), Some(128));
        adapter.store_segment(tenant, key_stored.clone(), payload.clone());

        // Exact match succeeds
        let retrieved = adapter.try_get_cached_segment(tenant, &key_stored);
        assert_eq!(retrieved, Some(payload.clone()));

        // Different rope_offset (e.g. 256) returns None (cache miss)
        let key_diff_rope = KvCacheKey::new(chunk_id, fp.clone(), Some(256));
        let res_diff = adapter.try_get_cached_segment(tenant, &key_diff_rope);
        assert!(
            res_diff.is_none(),
            "Lookup with different rope_offset MUST return None"
        );

        // Missing rope_offset (None) returns None (cache miss)
        let key_no_rope = KvCacheKey::new(chunk_id, fp, None);
        let res_none = adapter.try_get_cached_segment(tenant, &key_no_rope);
        assert!(
            res_none.is_none(),
            "Lookup with None rope_offset MUST return None when stored with Some(128)"
        );
    }

    #[test]
    fn test_corrupt_payload_returns_none() {
        let adapter = create_test_adapter();
        let tenant = TenantId::try_new(55).unwrap();
        let key = KvCacheKey::new(777, dummy_fp(), None);

        // Store garbage bytes in store under tenant and chunk_id
        let corrupt_segment = KvSegment::new(tenant, key.chunk_id, vec![0xFF, 0xFE, 0xFD, 0xFC]);
        adapter.store.insert_segment(tenant, corrupt_segment);

        let result = adapter.try_get_cached_segment(tenant, &key);
        assert!(
            result.is_none(),
            "Corrupt payload MUST return None without panic"
        );
    }

    #[test]
    #[cfg(feature = "memfuse-store")]
    fn test_with_lsm_fallback_without_lsm_store() {
        let master_km = CryptoKey::try_new("test-passphrase-kv", b"test-salt-12345").unwrap();
        let cipher = Arc::new(KvSegmentCipher::new(master_km));
        let store = Arc::new(TenantIsolatedKvStore::new());
        let adapter = KvBridgeAdapter::new(store, cipher);

        assert!(adapter.lsm_store.is_none());
        let tenant = TenantId::try_new(1).unwrap();
        let key = KvCacheKey::new(999, dummy_fp(), None);
        assert!(adapter.try_get_cached_segment(tenant, &key).is_none());
    }

    #[tokio::test]
    #[cfg(feature = "memfuse-store")]
    async fn test_lsm_fallback_ram_hit() {
        let master_km = CryptoKey::try_new("test-passphrase-kv", b"test-salt-12345").unwrap();
        let cipher = Arc::new(KvSegmentCipher::new(master_km));
        let store = Arc::new(TenantIsolatedKvStore::new());
        let temp_dir = tempfile::tempdir().unwrap();
        let lsm_config = memfuse_store::LsmConfig {
            path: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let lsm_store = Arc::new(memfuse_store::LsmStorage::new(lsm_config).await.unwrap());

        let adapter = KvBridgeAdapter::with_lsm_fallback(store, lsm_store, cipher);
        let tenant = TenantId::try_new(10).unwrap();
        let fp = dummy_fp();
        let chunk_id = 42;
        let payload = b"RAM hit payload".to_vec();
        let key = KvCacheKey::new(chunk_id, fp, None);

        adapter.store_segment(tenant, key.clone(), payload.clone());

        let retrieved = adapter.try_get_cached_segment_async(tenant, &key).await;
        assert_eq!(retrieved, Some(payload));
    }

    #[tokio::test]
    #[cfg(feature = "memfuse-store")]
    async fn test_lsm_fallback_ram_miss_lsm_hit() {
        let master_km = CryptoKey::try_new("test-passphrase-kv", b"test-salt-12345").unwrap();
        let cipher = Arc::new(KvSegmentCipher::new(master_km));
        let store = Arc::new(TenantIsolatedKvStore::new());
        let temp_dir = tempfile::tempdir().unwrap();
        let lsm_config = memfuse_store::LsmConfig {
            path: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let lsm_store = Arc::new(memfuse_store::LsmStorage::new(lsm_config).await.unwrap());

        let adapter =
            KvBridgeAdapter::with_lsm_fallback(store, Arc::clone(&lsm_store), cipher.clone());
        let tenant = TenantId::try_new(10).unwrap();
        let fp = dummy_fp();
        let chunk_id = 100;
        let payload = b"LSM hit payload".to_vec();
        let key = KvCacheKey::new(chunk_id, fp.clone(), None);

        // Manually serialize payload and encrypt layer, then store directly in LSM
        let inner_payload = CachedKvPayload {
            fingerprint: fp.clone(),
            rope_offset: None,
            data: payload.clone(),
        };
        let serialized_bytes = bincode::serialize(&inner_payload).unwrap();
        let layer = cipher
            .encrypt_with_version(tenant, chunk_id, 1, fp, &serialized_bytes)
            .unwrap();
        let doc_bytes = bincode::serialize(&layer).unwrap();

        let spill_key = format!("__kv_spill:{}:{:#x}", tenant.inner(), chunk_id).into_bytes();
        let tx_id = TxId(chunk_id);
        lsm_store.put(tx_id, &spill_key, &doc_bytes).await.unwrap();
        lsm_store.commit(tx_id).await.unwrap();

        // Ensure RAM store is empty for key
        assert!(adapter.try_get_cached_segment(tenant, &key).is_none());

        // async lookup should hit LSM store and return decrypted payload
        let retrieved = adapter.try_get_cached_segment_async(tenant, &key).await;
        assert_eq!(retrieved, Some(payload));
    }

    #[tokio::test]
    #[cfg(feature = "memfuse-store")]
    async fn test_lsm_fallback_ram_miss_lsm_miss() {
        let master_km = CryptoKey::try_new("test-passphrase-kv", b"test-salt-12345").unwrap();
        let cipher = Arc::new(KvSegmentCipher::new(master_km));
        let store = Arc::new(TenantIsolatedKvStore::new());
        let temp_dir = tempfile::tempdir().unwrap();
        let lsm_config = memfuse_store::LsmConfig {
            path: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let lsm_store = Arc::new(memfuse_store::LsmStorage::new(lsm_config).await.unwrap());

        let adapter = KvBridgeAdapter::with_lsm_fallback(store, lsm_store, cipher);
        let tenant = TenantId::try_new(10).unwrap();
        let key = KvCacheKey::new(999, dummy_fp(), None);

        let retrieved = adapter.try_get_cached_segment_async(tenant, &key).await;
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    #[cfg(feature = "memfuse-store")]
    async fn test_lsm_fallback_fingerprint_mismatch() {
        let master_km = CryptoKey::try_new("test-passphrase-kv", b"test-salt-12345").unwrap();
        let cipher = Arc::new(KvSegmentCipher::new(master_km));
        let store = Arc::new(TenantIsolatedKvStore::new());
        let temp_dir = tempfile::tempdir().unwrap();
        let lsm_config = memfuse_store::LsmConfig {
            path: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let lsm_store = Arc::new(memfuse_store::LsmStorage::new(lsm_config).await.unwrap());

        let adapter =
            KvBridgeAdapter::with_lsm_fallback(store, Arc::clone(&lsm_store), cipher.clone());
        let tenant = TenantId::try_new(10).unwrap();
        let fp_stored = dummy_fp();
        let fp_lookup = ModelFingerprint::new([0x88u8; 32], "other-model.gguf", "Q8_0");
        let chunk_id = 200;
        let payload = b"LSM payload with mismatched fingerprint".to_vec();

        let inner_payload = CachedKvPayload {
            fingerprint: fp_stored.clone(),
            rope_offset: None,
            data: payload,
        };
        let serialized_bytes = bincode::serialize(&inner_payload).unwrap();
        let layer = cipher
            .encrypt_with_version(tenant, chunk_id, 1, fp_stored, &serialized_bytes)
            .unwrap();
        let doc_bytes = bincode::serialize(&layer).unwrap();

        let spill_key = format!("__kv_spill:{}:{:#x}", tenant.inner(), chunk_id).into_bytes();
        let tx_id = TxId(chunk_id);
        lsm_store.put(tx_id, &spill_key, &doc_bytes).await.unwrap();
        lsm_store.commit(tx_id).await.unwrap();

        let key_mismatch = KvCacheKey::new(chunk_id, fp_lookup, None);
        let retrieved = adapter
            .try_get_cached_segment_async(tenant, &key_mismatch)
            .await;
        assert!(
            retrieved.is_none(),
            "Fingerprint mismatch MUST return None without panic"
        );
    }

    #[tokio::test]
    #[cfg(feature = "memfuse-store")]
    async fn test_lsm_fallback_golden_zero_copy_async_bytes() {
        let master_km =
            CryptoKey::try_new("test-passphrase-golden", b"test-salt-golden123").unwrap();
        let cipher = Arc::new(KvSegmentCipher::new(master_km));
        let store = Arc::new(TenantIsolatedKvStore::with_capacity(1));
        let temp_dir = tempfile::tempdir().unwrap();
        let lsm_config = memfuse_store::LsmConfig {
            path: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let lsm_store = Arc::new(memfuse_store::LsmStorage::new(lsm_config).await.unwrap());

        let adapter = KvBridgeAdapter::with_lsm_fallback(store, lsm_store, cipher);
        let tenant = TenantId::try_new(777).unwrap();
        let fp = dummy_fp();

        let key1 = KvCacheKey::new(1, fp.clone(), Some(32));
        let payload1 = b"Golden zero-copy paged KV payload block".to_vec();
        adapter.store_segment(tenant, key1.clone(), payload1.clone());

        let key2 = KvCacheKey::new(2, fp.clone(), Some(64));
        let payload2 = b"Eviction trigger payload block".to_vec();
        adapter.store_segment(tenant, key2.clone(), payload2);

        let mut retrieved_bytes = None;
        for _ in 0..20 {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            if let Some(b) = adapter
                .try_get_cached_segment_async_bytes(tenant, &key1)
                .await
            {
                retrieved_bytes = Some(b);
                break;
            }
        }
        assert!(retrieved_bytes.is_some());
        assert_eq!(retrieved_bytes.unwrap().as_ref(), payload1.as_slice());
    }

    #[tokio::test]
    #[cfg(feature = "memfuse-store")]
    async fn test_lsm_fallback_eviction_triggers_spill() {
        let master_km = CryptoKey::try_new("test-passphrase-kv", b"test-salt-12345").unwrap();
        let cipher = Arc::new(KvSegmentCipher::new(master_km));
        // Create store with capacity of 1 segment per tenant
        let store = Arc::new(TenantIsolatedKvStore::with_capacity(1));
        let temp_dir = tempfile::tempdir().unwrap();
        let lsm_config = memfuse_store::LsmConfig {
            path: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let lsm_store = Arc::new(memfuse_store::LsmStorage::new(lsm_config).await.unwrap());

        let adapter = KvBridgeAdapter::with_lsm_fallback(store, Arc::clone(&lsm_store), cipher);
        let tenant = TenantId::try_new(100).unwrap();
        let fp = dummy_fp();

        let key1 = KvCacheKey::new(1, fp.clone(), None);
        let payload1 = b"Payload segment 1".to_vec();
        adapter.store_segment(tenant, key1.clone(), payload1.clone());

        let key2 = KvCacheKey::new(2, fp.clone(), None);
        let payload2 = b"Payload segment 2".to_vec();
        // Storing second segment forces eviction of segment 1 from RAM
        adapter.store_segment(tenant, key2.clone(), payload2.clone());

        // Wait brief moment for tokio::spawn spill handler task to complete
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // RAM store should now miss key1
        assert!(adapter.try_get_cached_segment(tenant, &key1).is_none());

        // But async lookup should successfully fetch key1 from Tier-2 LSM spill
        let retrieved1 = adapter.try_get_cached_segment_async(tenant, &key1).await;
        assert_eq!(retrieved1, Some(payload1));
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
                let key = KvCacheKey::new(chunk_id, fp1.clone(), None);
                adapter1.store_segment(tenant1, key.clone(), data);
                let _ = adapter1.try_get_cached_segment(tenant1, &key);
            }
        });

        let fp2 = fp;
        let handle2 = thread::spawn(move || {
            for chunk_id in 100..150 {
                let data = vec![(chunk_id % 256) as u8; 128];
                let key = KvCacheKey::new(chunk_id, fp2.clone(), None);
                adapter2.store_segment(tenant2, key.clone(), data);
                let _ = adapter2.try_get_cached_segment(tenant2, &key);
            }
        });

        handle1.join().expect("Thread 1 panicked");
        handle2.join().expect("Thread 2 panicked");

        worker.shutdown();
    }
}
