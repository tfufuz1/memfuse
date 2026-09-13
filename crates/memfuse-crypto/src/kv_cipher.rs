// FILE-CONTEXT
// ZWECK: Dedicated AEAD encryption and key isolation for KV-cache segments (memfuse-kv-bridge).
// INVARIANTEN: Key derivation per (tenant_id, model_fingerprint) tuple via KeyManager HKDF-Expand.
// NICHT-OFFENSICHTLICH: OsRng generates fresh 12-byte nonces per encrypt call. AES-256-GCM-SIV provides misuse-resistance.
// STAND: TS:2026-09-08T00:00:00Z

//! KV-Cache Segment Encryption Module (memfuse-crypto).
//!
//! # Migration Strategy (Zwei-Schritt-Migrationsstrategie)
//! - **Schritt 1 (Dieses Modul):** Erstellung des eigenständigen Krypto-Moduls `KvSegmentCipher`
//!   mit kryptographischer Tenant-Isolation und Modell-Versionierungs-Trennung in `memfuse-crypto`.
//! - **Schritt 2 (Folge-Task / Prompt 5):** Verdrahtung von `KvSegmentCipher` und `EncryptedKvLayer`
//!   in `crates/memfuse-kv-bridge` zur Erweiterung der `KvSegment`-Struktur.
//!
//! # Nonce-Sicherheit & Nonce-Misuse-Resistance (RFC 8452)
//! Das Modul verwendet AES-256-GCM-SIV mit per-call `OsRng` generierten 12-Byte-Nonces.
//! Die Nonce-Misuse-Resistance von AES-256-GCM-SIV schützt vor Authentifizierungs-Schlüssel-Leaks
//! bei versehentlicher Nonce-Wiederverwendung. Sie dient als kryptographisches Sicherheitsnetz,
//! ersetzt jedoch **nicht** die Notwendigkeit, für jede Verschlüsselungsoperation frische,
//! kryptographisch zufällige Nonces über `OsRng` zu erzeugen.

#![forbid(unsafe_code)]

use crate::crypto::KeyManager;
use crate::error::{CryptoError, Result};
use memfuse_core::TenantId;
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

pub use memfuse_core::ModelFingerprint;

/// Current persisted format version for KV-cache segment layers.
pub const CURRENT_KV_FORMAT_VERSION: u8 = 2;

/// Container for an encrypted KV-cache segment layer.
///
/// Plaintext data and nonce are zeroized on drop. Public metadata (`tenant_id`, `model_fingerprint`, `format_version`)
/// is explicitly skipped during zeroization (`#[zeroize(skip)]`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Zeroize)]
#[zeroize(drop)]
pub struct EncryptedKvLayer {
    /// Format version number of this encrypted KV layer.
    #[zeroize(skip)]
    pub format_version: u8,
    /// AES-256-GCM-SIV ciphertext containing encrypted KV tensor payload and 16-byte auth tag.
    pub ciphertext: Vec<u8>,
    /// 12-byte initialization vector (nonce) generated via `OsRng`.
    pub nonce: [u8; 12],
    /// Tenant identifier bound to this layer.
    #[zeroize(skip)]
    pub tenant_id: TenantId,
    /// Model fingerprint bound to this layer.
    #[zeroize(skip)]
    pub model_fingerprint: ModelFingerprint,
}

/// High-level cipher engine for KV-cache segment encryption and decryption.
pub struct KvSegmentCipher {
    key_manager: KeyManager,
}

impl KvSegmentCipher {
    /// Creates a new `KvSegmentCipher` wrapping the workspace master `KeyManager`.
    pub fn new(key_manager: KeyManager) -> Self {
        Self { key_manager }
    }

    /// Encrypts KV-cache plaintext payload for a given `(tenant_id, model_fingerprint)` pair.
    ///
    /// Derives an isolated sub-key via HKDF-SHA256 through `KeyManager::derive_kv_key()`
    /// to cryptographically enforce tenant isolation and model quantization boundaries.
    pub fn encrypt(
        &self,
        tenant_id: TenantId,
        model_fingerprint: ModelFingerprint,
        plaintext: &[u8],
    ) -> Result<EncryptedKvLayer> {
        let sub_km = self
            .key_manager
            .derive_kv_key(tenant_id, &model_fingerprint)?;
        let (ciphertext, nonce) = sub_km.encrypt_auto_nonce(plaintext)?;

        Ok(EncryptedKvLayer {
            format_version: CURRENT_KV_FORMAT_VERSION,
            ciphertext,
            nonce,
            tenant_id,
            model_fingerprint,
        })
    }

    /// Decrypts an `EncryptedKvLayer` back to its original plaintext.
    ///
    /// Checks that `encrypted.format_version == CURRENT_KV_FORMAT_VERSION`.
    /// Derives the exact sub-key for `(encrypted.tenant_id, encrypted.model_fingerprint)`.
    /// Fails with a format version error or authentication error if the layer or key
    /// was tampered with.
    pub fn decrypt(&self, encrypted: &EncryptedKvLayer) -> Result<Vec<u8>> {
        if encrypted.format_version != CURRENT_KV_FORMAT_VERSION {
            return Err(CryptoError::KvFormatVersionMismatch {
                expected: CURRENT_KV_FORMAT_VERSION,
                found: encrypted.format_version,
            });
        }

        let sub_km = self
            .key_manager
            .derive_kv_key(encrypted.tenant_id, &encrypted.model_fingerprint)?;
        sub_km.decrypt_auto_nonce(&encrypted.ciphertext, &encrypted.nonce)
    }

    /// Derives a sub-key for a specific segment given tenant_id, segment_id, and key_derivation_version.
    pub fn derive_key_for_segment(
        &self,
        tenant_id: TenantId,
        segment_id: u64,
        version: u8,
    ) -> Result<KeyManager> {
        let info = if version == 0 {
            format!("memfuse-kv-segment-{}-{}", tenant_id, segment_id)
        } else {
            format!("memfuse-kv-v{}-segment-{}-{}", version, tenant_id, segment_id)
        };
        self.key_manager.derive_segment_key(&info)
    }

    /// Encrypts KV-cache plaintext payload using versioned segment HKDF key derivation.
    pub fn encrypt_with_version(
        &self,
        tenant_id: TenantId,
        segment_id: u64,
        version: u8,
        model_fingerprint: ModelFingerprint,
        plaintext: &[u8],
    ) -> Result<EncryptedKvLayer> {
        let sub_km = self.derive_key_for_segment(tenant_id, segment_id, version)?;
        let (ciphertext, nonce) = sub_km.encrypt_auto_nonce(plaintext)?;

        Ok(EncryptedKvLayer {
            format_version: CURRENT_KV_FORMAT_VERSION,
            ciphertext,
            nonce,
            tenant_id,
            model_fingerprint,
        })
    }

    /// Decrypts an `EncryptedKvLayer` using versioned segment HKDF key derivation.
    pub fn decrypt_with_version(
        &self,
        encrypted: &EncryptedKvLayer,
        segment_id: u64,
        version: u8,
    ) -> Result<Vec<u8>> {
        if encrypted.format_version != CURRENT_KV_FORMAT_VERSION {
            return Err(CryptoError::KvFormatVersionMismatch {
                expected: CURRENT_KV_FORMAT_VERSION,
                found: encrypted.format_version,
            });
        }

        let sub_km = self.derive_key_for_segment(encrypted.tenant_id, segment_id, version)?;
        sub_km.decrypt_auto_nonce(&encrypted.ciphertext, &encrypted.nonce)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_fingerprint(id: &str) -> ModelFingerprint {
        ModelFingerprint::new([0x42u8; 32], id, "Q4_K_M")
    }

    #[test]
    fn test_kv_segment_cipher_encrypt_decrypt_roundtrip() {
        let master_km = KeyManager::try_new("master-passphrase", b"master-salt").unwrap();
        let cipher = KvSegmentCipher::new(master_km);

        let tenant_id = TenantId::try_new(101).unwrap();
        let fp = dummy_fingerprint("model-v1");
        let plaintext = b"KV tensor keys and values payload data";

        let encrypted = cipher.encrypt(tenant_id, fp.clone(), plaintext).unwrap();
        assert_eq!(encrypted.tenant_id, tenant_id);
        assert_eq!(encrypted.model_fingerprint, fp);

        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_wrong_tenant_id_fails_decryption() {
        let master_km = KeyManager::try_new("master-passphrase", b"master-salt").unwrap();
        let cipher = KvSegmentCipher::new(master_km);

        let tenant_a = TenantId::try_new(101).unwrap();
        let tenant_b = TenantId::try_new(202).unwrap();
        let fp = dummy_fingerprint("model-v1");
        let plaintext = b"tenant A confidential payload";

        let mut encrypted = cipher.encrypt(tenant_a, fp, plaintext).unwrap();

        // Attempt decryption with tampered tenant_id in layer header
        encrypted.tenant_id = tenant_b;
        let res = cipher.decrypt(&encrypted);
        assert!(
            res.is_err(),
            "Decryption with mismatching tenant_id MUST fail authentication"
        );
    }

    #[test]
    fn test_wrong_model_fingerprint_fails_decryption() {
        let master_km = KeyManager::try_new("master-passphrase", b"master-salt").unwrap();
        let cipher = KvSegmentCipher::new(master_km);

        let tenant = TenantId::try_new(101).unwrap();
        let fp_q4 = ModelFingerprint::new([0x11u8; 32], "llama-3", "Q4_K_M");
        let fp_q8 = ModelFingerprint::new([0x11u8; 32], "llama-3", "Q8_0");
        let plaintext = b"Q4 model KV cache data";

        let mut encrypted = cipher.encrypt(tenant, fp_q4, plaintext).unwrap();

        // Attempt decryption with different quantization tier
        encrypted.model_fingerprint = fp_q8;
        let res = cipher.decrypt(&encrypted);
        assert!(
            res.is_err(),
            "Decryption with mismatching model_fingerprint MUST fail authentication"
        );
    }

    #[test]
    fn test_nonce_freshness_identical_plaintext() {
        let master_km = KeyManager::try_new("master-passphrase", b"master-salt").unwrap();
        let cipher = KvSegmentCipher::new(master_km);

        let tenant = TenantId::try_new(101).unwrap();
        let fp = dummy_fingerprint("model-v1");
        let plaintext = b"identical plaintext payload";

        let enc1 = cipher.encrypt(tenant, fp.clone(), plaintext).unwrap();
        let enc2 = cipher.encrypt(tenant, fp, plaintext).unwrap();

        assert_ne!(
            enc1.nonce, enc2.nonce,
            "Two encrypt calls for identical plaintext MUST produce distinct nonces"
        );
        assert_ne!(
            enc1.ciphertext, enc2.ciphertext,
            "Two encrypt calls for identical plaintext MUST produce distinct ciphertexts"
        );
    }

    #[test]
    fn test_invalid_format_version_returns_version_mismatch_error() {
        let master_km = KeyManager::try_new("master-passphrase", b"master-salt").unwrap();
        let cipher = KvSegmentCipher::new(master_km);

        let tenant_id = TenantId::try_new(101).unwrap();
        let fp = dummy_fingerprint("model-v1");
        let plaintext = b"KV tensor payload data";

        let mut encrypted = cipher.encrypt(tenant_id, fp, plaintext).unwrap();
        assert_eq!(encrypted.format_version, CURRENT_KV_FORMAT_VERSION);

        // Tamper with format_version (e.g., set to old version 1 or future version 99)
        encrypted.format_version = 1;
        let res = cipher.decrypt(&encrypted);

        match res {
            Err(CryptoError::KvFormatVersionMismatch { expected, found }) => {
                assert_eq!(expected, CURRENT_KV_FORMAT_VERSION);
                assert_eq!(found, 1);
            }
            res => panic!("Expected KvFormatVersionMismatch error, got: {:?}", res),
        }
    }
}
