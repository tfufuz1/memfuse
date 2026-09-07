// FILE-CONTEXT
// ZWECK: KvSegment mit Zeroize-Garantie (ZeroizeOnDrop, nie unverschlüsselt auf Disk).
// STAND: TS:2026-09-07T12:00:00Z (SESSION: a413a598)

use memfuse_core::TenantId;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Ein KV-Cache-Segment. P9-Pflicht: Zeroize-on-Drop, nie unverschlüsselt
/// auf persistentem/auslagerbarem Speicher.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct KvSegment {
    #[zeroize(skip)] // Metadaten, keine sensiblen Tensor-Daten
    pub tenant_id: TenantId,
    #[zeroize(skip)]
    pub segment_id: u64,
    /// Rohe Tensor-Bytes. WIRD gezeroized beim Drop.
    data: Vec<u8>,
}

impl KvSegment {
    /// Erstellt ein neues KV-Cache-Segment.
    pub fn new(tenant_id: TenantId, segment_id: u64, data: Vec<u8>) -> Self {
        Self {
            tenant_id,
            segment_id,
            data,
        }
    }

    /// Read-Only-Zugriff. Kein Klartext-Export nach außen ohne expliziten Call.
    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }

    /// Länge der Tensor-Bytes in Bytes.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Prüft ob das Segment leer ist.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

impl std::fmt::Debug for KvSegment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KvSegment")
            .field("tenant_id", &self.tenant_id)
            .field("segment_id", &self.segment_id)
            .field("data_len", &self.data.len())
            .field("data", &"*** REDACTED ***")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::ManuallyDrop;

    #[test]
    fn test_kv_segment_zeroize_on_drop() {
        let tenant = TenantId::try_new(1).unwrap();
        let data = vec![0xAAu8; 1024];
        let mut segment = ManuallyDrop::new(KvSegment::new(tenant, 1, data));
        let ptr = segment.as_bytes().as_ptr();
        let len = segment.len();

        // Precondition: check that memory contains original non-zero tensor bytes
        // SAFETY: `segment` is alive in ManuallyDrop wrapper and `ptr` points directly to its buffer.
        unsafe {
            let slice = std::slice::from_raw_parts(ptr, len);
            assert_eq!(slice, &[0xAAu8; 1024]);
        }

        // Action: Explicitly invoke zeroize without deallocating/dropping stack frame memory
        Zeroize::zeroize(&mut *segment);

        // Postcondition: Check that memory was zeroed in place without UAF
        // SAFETY: `segment` memory buffer is still allocated within ManuallyDrop wrapper in this frame.
        unsafe {
            let cleared_slice = std::slice::from_raw_parts(ptr, len);
            assert_eq!(
                cleared_slice,
                &[0x00u8; 1024],
                "KvSegment data MUST be zeroed after zeroize"
            );
        }
    }
}
