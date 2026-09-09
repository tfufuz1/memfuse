// FILE-CONTEXT
// ZWECK: Tenant-isolierter KV-Segment-Store (INV-TENANT Isolation).
// STAND: TS:2026-09-09T12:43:43Z (SESSION: 76e16dcf)

use ahash::AHashMap;
use memfuse_core::TenantId;
use parking_lot::RwLock;

use crate::segment::KvSegment;

/// Tenant-isolierter KV-Segment-Store.
///
/// INV-TENANT-Analogon für KV-Bridge: Ein Tenant kann niemals Segmente
/// eines anderen Tenants lesen. Strukturell erzwungen durch getrennte Maps.
// AI-TAG[API][MAJOR] Missing unencrypted segment retrieval API on TenantIsolatedKvStore (ID: AGT-KV-BRIDGE-7e1f286d) (TS: 2026-09-09T12:43:43Z) (SESSION: 76e16dcf)
// BEFUND: `TenantIsolatedKvStore` bietet `get_segments` (IDs) und `get_decrypted_segment` (nur bei feature `kv-encryption`), aber keine Methode um unverschlüsselte Segmentdaten im Klartext-Modus zu lesen.
// RISIKO: Aufrufer können ohne `kv-encryption` Feature die in den `KvSegment`s gespeicherten Tensor-Bytes nicht aus dem Store auslesen.
// EMPFEHLUNG: Öffentliche Methode `get_segment_bytes(&self, tenant: TenantId, segment_id: u64) -> Option<Vec<u8>>` ergänzen.
pub struct TenantIsolatedKvStore {
    segments: RwLock<AHashMap<TenantId, Vec<KvSegment>>>,
}

impl TenantIsolatedKvStore {
    /// Erstellt einen neuen tenant-isolierten KV-Store.
    pub fn new() -> Self {
        Self {
            segments: RwLock::new(AHashMap::new()),
        }
    }

    // AI-TAG[API][MINOR] Duplicate segment_id allowed on insertion without overwrite or validation (ID: AGT-KV-BRIDGE-6016eb9a) (TS: 2026-09-09T12:43:43Z) (SESSION: 76e16dcf)
    // BEFUND: `insert_segment` fügt Segmente via `.push()` ohne Überprüfung auf bereits existierende `segment_id` ein.
    // RISIKO: Mehrfache Einfügungen derselben `segment_id` führen zu Redundanz, verfälschen `get_tenant_segment_len` und verbrauchen unnötig Speicher.
    // EMPFEHLUNG: Existierende Segmente mit gleicher `segment_id` ersetzen oder `Result<(), MemFuseError>` bei Kollision zurückgeben.
    /// Fügt ein Segment für einen bestimmten Tenant ein.
    pub fn insert_segment(&self, tenant: TenantId, segment: KvSegment) {
        let mut map = self.segments.write();
        map.entry(tenant).or_default().push(segment);
    }

    /// Verschlüsselt einen Klartext-Tensor und fügt ein verschlüsseltes Segment ein.
    #[cfg(feature = "kv-encryption")]
    pub fn insert_encrypted_segment(
        &self,
        cipher: &memfuse_crypto::KvSegmentCipher,
        tenant: TenantId,
        segment_id: u64,
        model_fingerprint: memfuse_crypto::ModelFingerprint,
        rope_offset: Option<usize>,
        plaintext: &[u8],
    ) -> Result<(), memfuse_crypto::CryptoError> {
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
        cipher: &memfuse_crypto::KvSegmentCipher,
        tenant: TenantId,
        segment_id: u64,
    ) -> Result<Option<Vec<u8>>, memfuse_crypto::CryptoError> {
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
}
