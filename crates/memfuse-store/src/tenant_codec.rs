//! TenantKeyCodec — LSM-Key-Isolation via Präfix-Encoding.
//!
//! INVARIANTE INV-TENANT-2: scan_prefix(codec.scan_prefix()) liefert
//! AUSSCHLIESSLICH Keys dieses Tenants. Cross-Tenant-Leak strukturell ausgeschlossen.
//!
//! ENCODING: `t:{tenant_id}:{collection_id}:{doc_type}:{doc_id}`
//! Festes `t:`-Präfix verhindert Kollision mit Legacy-Keys.

use memfuse_core::{CollectionId, DocId, TenantId};

/// Encodes LSM storage keys with tenant isolation prefixes.
pub struct TenantKeyCodec {
    tenant_id: TenantId,
    /// Vorberechnetes Scan-Präfix ohne Allokation im Hot-Path.
    scan_prefix_cache: Vec<u8>,
}

impl TenantKeyCodec {
    /// Creates a new `TenantKeyCodec` for the given `TenantId`.
    pub fn new(tenant_id: TenantId) -> Self {
        let scan_prefix_cache = format!("t:{}:", tenant_id.inner()).into_bytes();
        Self {
            tenant_id,
            scan_prefix_cache,
        }
    }

    /// Encodes a document chunk key: `t:{tenant}:{collection}:chunk:{doc}`.
    #[inline]
    pub fn encode_chunk_key(&self, collection: &CollectionId, doc_id: DocId) -> Vec<u8> {
        format!(
            "t:{}:{}:chunk:{}",
            self.tenant_id.inner(),
            collection.0,
            doc_id.0
        )
        .into_bytes()
    }

    /// Encodes a graph entity key: `t:{tenant}:{collection}:graph:{entity}`.
    #[inline]
    pub fn encode_graph_key(&self, collection: &CollectionId, entity_id: u64) -> Vec<u8> {
        format!(
            "t:{}:{}:graph:{}",
            self.tenant_id.inner(),
            collection.0,
            entity_id
        )
        .into_bytes()
    }

    /// Globales Scan-Präfix für diesen Tenant (alle Collections): `t:{tenant}:`
    /// Cached — keine Allokation im Hot-Path.
    #[inline]
    pub fn scan_prefix(&self) -> &[u8] {
        &self.scan_prefix_cache
    }

    /// Collection-spezifisches Scan-Präfix: `t:{tenant}:{col}:`
    pub fn collection_prefix(&self, collection: &CollectionId) -> Vec<u8> {
        format!("t:{}:{}:", self.tenant_id.inner(), collection.0).into_bytes()
    }

    /// Dekodiert TenantId aus encoded Key. None bei ungültigem Format.
    /// Kein Unwrap — robuste Nutzung im Recovery-Pfad.
    pub fn decode_tenant_id(key: &[u8]) -> Option<TenantId> {
        let s = std::str::from_utf8(key).ok()?;
        let rest = s.strip_prefix("t:")?;
        let end = rest.find(':')?;
        let id: u64 = rest[..end].parse().ok()?;
        if id == 0 {
            None
        } else {
            Some(TenantId(id))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_roundtrip() {
        let tenant = TenantId::try_new(7).unwrap();
        let codec = TenantKeyCodec::new(tenant);
        let col = CollectionId(42);
        let doc = DocId(99);
        let key = codec.encode_chunk_key(&col, doc);
        assert_eq!(TenantKeyCodec::decode_tenant_id(&key), Some(tenant));
    }

    #[test]
    fn test_cross_tenant_isolation() {
        let codec_a = TenantKeyCodec::new(TenantId::try_new(1).unwrap());
        let codec_b = TenantKeyCodec::new(TenantId::try_new(2).unwrap());
        let col = CollectionId(1);
        let doc = DocId(1);
        let key_a = codec_a.encode_chunk_key(&col, doc);
        // Key von Tenant A darf NICHT mit Präfix von Tenant B matchen
        assert!(!key_a.starts_with(codec_b.scan_prefix()));
    }

    #[test]
    fn test_scan_prefix_cached_no_alloc() {
        let codec = TenantKeyCodec::new(TenantId::try_new(42).unwrap());
        assert!(codec.scan_prefix().starts_with(b"t:42:"));
    }

    #[test]
    fn test_decode_invalid_key_returns_none() {
        assert!(TenantKeyCodec::decode_tenant_id(b"invalid").is_none());
        assert!(TenantKeyCodec::decode_tenant_id(b"t:0:col:chunk:1").is_none()); // SYSTEM
    }
}
