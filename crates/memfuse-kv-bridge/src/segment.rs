// FILE-CONTEXT
// ZWECK: KvSegment mit Zeroize-Garantie (ZeroizeOnDrop, nie unverschlüsselt auf Disk).
// STAND: TS:2026-09-09T12:43:43Z (SESSION: 76e16dcf)

use memfuse_core::TenantId;
use std::sync::atomic::{AtomicU64, Ordering};
use zeroize::{Zeroize, ZeroizeOnDrop};

#[cfg(feature = "kv-encryption")]
use memfuse_crypto::{EncryptedKvLayer, KvSegmentCipher, ModelFingerprint};

/// Monotoner Logical-Clock-Zähler für Recency-Ordering (P3: Keine SystemTime als Kausalitätsgarant).
static GLOBAL_KV_ACCESS_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Encrypted layer representation stored inside a KvSegment when encryption is active.
#[cfg(feature = "kv-encryption")]
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct EncryptedSegmentPayload {
    /// Encrypted KV layer container from memfuse-crypto. Zeroized on drop.
    pub layer: EncryptedKvLayer,
}

/// Ein KV-Cache-Segment. P9-Pflicht: Zeroize-on-Drop, nie unverschlüsselt
/// auf persistentem/auslagerbarem Speicher.
///
/// Gemäß Gesamtspezifikation v7.0 §7.3 enthält ein Segment:
/// - `tenant_id`: Mandanten-Identifikator
/// - `segment_id`: Segment-Identifikator
/// - `model_fingerprint`: Optionaler Modell-Fingerprint zur Schlüsselableitung
/// - `rope_offset`: Optionaler RoPE-Positions-Offset (bereits vorbereitet; falls memfuse-mcp
///   diesen Wert noch nicht bereitstellt, ist `None` als offener Folgepunkt dokumentiert)
/// - `encrypted`: Kennzeichen, ob die Tensor-Bytes verschlüsselt vorliegen
/// - `data`: Rohe Tensor-Bytes oder Ciphertext-Bytes. WIRD gezeroized beim Drop.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct KvSegment {
    #[zeroize(skip)] // Metadaten, keine sensiblen Tensor-Daten
    pub tenant_id: TenantId,
    #[zeroize(skip)]
    pub segment_id: u64,
    #[zeroize(skip)]
    pub encrypted: bool,
    #[zeroize(skip)]
    #[cfg(feature = "kv-encryption")]
    pub model_fingerprint: Option<ModelFingerprint>,
    #[zeroize(skip)]
    pub rope_offset: Option<usize>,
    #[zeroize(skip)] // AtomicU64 enthält keine sensiblen Tensor-Daten
    last_accessed: AtomicU64,
    /// Rohe Tensor-Bytes (Klartext oder Ciphertext). WIRD gezeroized beim Drop.
    data: Vec<u8>,
    #[cfg(feature = "kv-encryption")]
    encrypted_payload: Option<EncryptedSegmentPayload>,
}

impl KvSegment {
    /// Erstellt ein neues Klartext-KV-Cache-Segment (Default / Zero-Config, P12-konform).
    pub fn new(tenant_id: TenantId, segment_id: u64, data: Vec<u8>) -> Self {
        let initial_clock = GLOBAL_KV_ACCESS_COUNTER.fetch_add(1, Ordering::Relaxed);
        Self {
            tenant_id,
            segment_id,
            encrypted: false,
            #[cfg(feature = "kv-encryption")]
            model_fingerprint: None,
            rope_offset: None,
            last_accessed: AtomicU64::new(initial_clock),
            data,
            #[cfg(feature = "kv-encryption")]
            encrypted_payload: None,
        }
    }

    /// Erstellt ein neues Klartext-KV-Cache-Segment mit optionalen Metadaten.
    pub fn new_with_metadata(
        tenant_id: TenantId,
        segment_id: u64,
        data: Vec<u8>,
        #[cfg(feature = "kv-encryption")] model_fingerprint: Option<ModelFingerprint>,
        rope_offset: Option<usize>,
    ) -> Self {
        let initial_clock = GLOBAL_KV_ACCESS_COUNTER.fetch_add(1, Ordering::Relaxed);
        Self {
            tenant_id,
            segment_id,
            encrypted: false,
            #[cfg(feature = "kv-encryption")]
            model_fingerprint,
            rope_offset,
            last_accessed: AtomicU64::new(initial_clock),
            data,
            #[cfg(feature = "kv-encryption")]
            encrypted_payload: None,
        }
    }

    /// Erstellt ein verschlüsseltes KV-Cache-Segment aus einem Klartext-Tensor.
    #[cfg(feature = "kv-encryption")]
    pub fn new_encrypted(
        cipher: &KvSegmentCipher,
        tenant_id: TenantId,
        segment_id: u64,
        model_fingerprint: ModelFingerprint,
        rope_offset: Option<usize>,
        plaintext: &[u8],
    ) -> Result<Self, memfuse_crypto::CryptoError> {
        let initial_clock = GLOBAL_KV_ACCESS_COUNTER.fetch_add(1, Ordering::Relaxed);
        let encrypted_layer = cipher.encrypt(tenant_id, model_fingerprint.clone(), plaintext)?;
        let ciphertext_copy = encrypted_layer.ciphertext.clone();

        Ok(Self {
            tenant_id,
            segment_id,
            encrypted: true,
            model_fingerprint: Some(model_fingerprint),
            rope_offset,
            last_accessed: AtomicU64::new(initial_clock),
            data: ciphertext_copy,
            encrypted_payload: Some(EncryptedSegmentPayload {
                layer: encrypted_layer,
            }),
        })
    }

    /// Entschlüsselt die Daten des Segments, falls es verschlüsselt ist.
    /// Gibt bei Klartext-Segmenten direkt einen Klon der `data`-Bytes zurück.
    #[cfg(feature = "kv-encryption")]
    pub fn decrypt_data(
        &self,
        cipher: &KvSegmentCipher,
    ) -> Result<Vec<u8>, memfuse_crypto::CryptoError> {
        if !self.encrypted {
            return Ok(self.data.clone());
        }

        if let Some(payload) = &self.encrypted_payload {
            cipher.decrypt(&payload.layer)
        } else if let Some(model_fp) = &self.model_fingerprint {
            // AI-TAG[CRYPTO][MAJOR] Dummy zero nonce fallback masks missing payload state (ID: AGT-KV-BRIDGE-fae9dd56) (TS: 2026-09-09T12:43:43Z) (SESSION: 76e16dcf)
            // BEFUND: Falls `encrypted_payload` `None` ist, wird ein Layer mit `nonce: [0u8; 12]` rekonstruiert.
            // RISIKO: AES-GCM-SIV Entschlüsselung schlägt mit nichtssagendem Auth-Tag-Fehler fehl, statt einen klaren InvalidState-Fehler anzuzeigen.
            // EMPFEHLUNG: Rückgabe eines expliziten CryptoError::InvalidInput / MissingPayload statt Blind-Fallback auf Null-Nonce.
            // Reconstruct layer if payload reference was split
            let layer = EncryptedKvLayer {
                ciphertext: self.data.clone(),
                nonce: [0u8; 12], // Dummy or missing nonce guard
                tenant_id: self.tenant_id,
                model_fingerprint: model_fp.clone(),
            };
            cipher.decrypt(&layer)
        } else {
            Err(memfuse_crypto::CryptoError::Crypto(
                "Missing model fingerprint for encrypted segment decryption".into(),
            ))
        }
    }

    /// Aktualisiert den atomaren Zugriffs-Zeitstempel (Logical Clock) für LRU-Eviction-Heuristiken.
    pub fn touch(&self) {
        let now = GLOBAL_KV_ACCESS_COUNTER.fetch_add(1, Ordering::Relaxed);
        self.last_accessed.store(now, Ordering::Relaxed);
    }

    /// Gibt den aktuellen atomaren Logical-Clock-Wert des letzten Zugriffs zurück.
    pub fn last_accessed(&self) -> u64 {
        self.last_accessed.load(Ordering::Relaxed)
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
                cleared_slice, &[0x00u8; 1024],
                "KvSegment data MUST be zeroed after zeroize"
            );
        }
    }
}
