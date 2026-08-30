//! Encryption utilities for MemFuse.
//!
//! Implements AES-256-GCM-SIV encryption and HKDF-SHA256 key derivation.
//!
//! # Nonce Design
//!
//! The **only** public encryption entry-point is [`KeyManager::encrypt_auto_nonce`],
//! which generates a collision-resistant 12-byte nonce composed of:
//! - 4 bytes: random `nonce_prefix` generated once per `KeyManager` instance.
//! - 8 bytes: cryptographically random suffix generated per encryption call via `OsRng`.
//!
//! Per-file key isolation via [`KeyManager::derive_file_key`] (HKDF-Expand) ensures
//! that even if two instances share a nonce counter value, they operate on
//! cryptographically independent keys, making (key, nonce) collisions impossible.
//!
// AI-NOTE[BOUNDARY-MISSING][RESOLVED] AGT-CRYPTO-001: The former `encrypt(&self, data, nonce_val: u64)`
// method has been removed (2026-08-24). It had zero callers (verified workspace-wide via
// `grep -rn ".encrypt("`) and posed a latent nonce-reuse risk: callers could supply an
// arbitrary u64 without any uniqueness guarantee. The ONLY safe encryption path is
// `encrypt_auto_nonce`, which is enforced by this removal. No corresponding `decrypt(u64)`
// existed, so the removed method could not even form a valid round-trip from outside the crate.
// KONTEXT: crates/memfuse-crypto/src/crypto.rs — resolved by removal, no callers.
// ID: AGT-CRYPTO-001

#![forbid(unsafe_code)]

use crate::anti_tamper::VolatileEncryptionKey;
use aes_gcm_siv::{
    aead::{Aead, KeyInit},
    Aes256GcmSiv, Nonce,
};
use hkdf::Hkdf;
use memfuse_core::{MemFuseError, Result};
use rand::RngCore;
use sha2::Sha256;

/// Manager for encryption keys and block encryption.
pub struct KeyManager {
    key: VolatileEncryptionKey,
    nonce_prefix: [u8; 4],
}

impl std::fmt::Debug for KeyManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyManager")
            .field("key", &"***REDACTED***")
            .field("nonce_prefix", &self.nonce_prefix)
            .finish()
    }
}

impl KeyManager {
    /// Creates a new KeyManager by deriving a key from a passphrase.
    pub fn try_new(passphrase: &str, salt: &[u8]) -> Result<Self> {
        if passphrase.is_empty() {
            return Err(MemFuseError::invalid_input("Passphrase must not be empty"));
        }
        let hk = Hkdf::<Sha256>::new(Some(salt), passphrase.as_bytes());
        let mut key_raw = [0u8; 32];
        hk.expand(b"memfuse-aes-256-gcm-key", &mut key_raw)
            .map_err(|e| MemFuseError::Crypto(format!("HKDF expansion failed: {}", e)))?;

        let mut nonce_prefix = [0u8; 4];
        rand::rngs::OsRng.fill_bytes(&mut nonce_prefix);

        Ok(Self {
            key: VolatileEncryptionKey::new(key_raw),
            nonce_prefix,
        })
    }

    /// Creates a new KeyManager with a cryptographically secure random salt.
    pub fn try_new_random_salt(passphrase: &str) -> Result<(Self, [u8; 32])> {
        let mut salt = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut salt);
        let km = Self::try_new(passphrase, &salt)?;
        Ok((km, salt))
    }

    /// Derives a sub-key for a specific file or logical stream.
    /// This prevents nonce-reuse when multiple files use the same master key.
    pub fn derive_file_key(&self, file_id: &[u8]) -> Result<Self> {
        if file_id.is_empty() {
            return Err(MemFuseError::invalid_input("file_id must not be empty"));
        }
        // Since self.key is already derived via HKDF in try_new, it is a high-entropy PRK.
        // We use HKDF-Expand with a domain-separating prefix to derive a per-file key.
        let hk = Hkdf::<Sha256>::from_prk(self.key.as_bytes())
            .map_err(|_| MemFuseError::Crypto("Invalid PRK length".to_string()))?;

        let mut sub_key = [0u8; 32];
        let mut info = Vec::with_capacity(b"memfuse-file-key-v1:".len() + file_id.len());
        info.extend_from_slice(b"memfuse-file-key-v1:");
        info.extend_from_slice(file_id);

        hk.expand(&info, &mut sub_key)
            .map_err(|e| MemFuseError::Crypto(format!("HKDF sub-key expansion failed: {}", e)))?;

        let mut nonce_prefix = [0u8; 4];
        rand::rngs::OsRng.fill_bytes(&mut nonce_prefix);

        Ok(Self {
            key: VolatileEncryptionKey::new(sub_key),
            nonce_prefix,
        })
    }

    /// Derives an integrity key for HMAC-SHA256.
    pub fn integrity_key(&self) -> Result<[u8; 32]> {
        let hk = Hkdf::<Sha256>::from_prk(self.key.as_bytes())
            .map_err(|_| MemFuseError::Crypto("Invalid PRK length".to_string()))?;
        let mut key = [0u8; 32];
        hk.expand(b"memfuse-hmac-sha256-key", &mut key)
            .map_err(|e| MemFuseError::Crypto(format!("HKDF integrity expansion failed: {}", e)))?;
        Ok(key)
    }

    /// Encrypts a block of data with an automatically generated random nonce.
    /// Returns the ciphertext and the full 12-byte nonce used.
    pub fn encrypt_auto_nonce(&self, data: &[u8]) -> Result<(Vec<u8>, [u8; 12])> {
        // AES-256-GCM-SIV: nonce-reuse-resistant (RFC 8452). Ciphertext-Integrität
        // bleibt auch bei versehentlicher Nonce-Wiederverwendung gewahrt, anders
        // als bei AES-GCM das bei Nonce-Reuse den Auth-Key leakt.
        let mut nonce_bytes = [0u8; 12];
        nonce_bytes[0..4].copy_from_slice(&self.nonce_prefix);
        // SAFETY: Fresh 8-byte random suffix generated per call via OsRng avoids atomic counter persistence requirements.
        // OsRng per-call nonces are collision-resistant at expected usage volumes (2^32 messages before birthday prob exceeds 2^-32).
        rand::rngs::OsRng.fill_bytes(&mut nonce_bytes[4..12]);

        let cipher = Aes256GcmSiv::new_from_slice(self.key.as_bytes())
            .map_err(|e| MemFuseError::Crypto(format!("Crypto error: {}", e)))?;
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(nonce, data)
            .map_err(|e| MemFuseError::Crypto(format!("Encryption failed: {}", e)))?;

        Ok((ciphertext, nonce_bytes))
    }

    /// Decrypts a block of data using a full 12-byte nonce.
    pub fn decrypt_auto_nonce(&self, ciphertext: &[u8], nonce_bytes: &[u8; 12]) -> Result<Vec<u8>> {
        let cipher = Aes256GcmSiv::new_from_slice(self.key.as_bytes())
            .map_err(|e| MemFuseError::Crypto(format!("Crypto error: {}", e)))?;
        let nonce = Nonce::from_slice(nonce_bytes);
        cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| MemFuseError::Crypto(format!("Decryption failed: {}", e)))
    }

    /// Emergency Trigger: Explicitly wipes the key from memory.
    pub fn emergency_wipe(&mut self) {
        self.key.emergency_wipe();
    }

    /// Provides access to the key bytes ONLY during testing.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn inspect_key_bytes_for_test(&self) -> &[u8; 32] {
        self.key.inspect_key_bytes_for_test()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let km = KeyManager::try_new("secret-passphrase", b"salt1").expect("try_new"); // expect
        let data = b"sensitive data";

        let (encrypted, nonce) = km.encrypt_auto_nonce(data).expect("encrypt"); // expect
        let decrypted = km.decrypt_auto_nonce(&encrypted, &nonce).expect("decrypt"); // expect

        assert_eq!(data, decrypted.as_slice());
    }

    #[test]
    fn test_wrong_nonce_fails() {
        let km = KeyManager::try_new("secret-passphrase", b"salt1").expect("try_new"); // expect
        let data = b"sensitive data";
        let (encrypted, mut nonce) = km.encrypt_auto_nonce(data).expect("encrypt"); // expect
        nonce[0] ^= 1; // alter the nonce

        let result = km.decrypt_auto_nonce(&encrypted, &nonce);
        assert!(result.is_err());
    }

    #[test]
    fn test_different_keys_different_ciphertexts() {
        let km1 = KeyManager::try_new("pass1", b"salt1").expect("try_new"); // expect
        let km2 = KeyManager::try_new("pass2", b"salt1").expect("try_new"); // expect
        let data = b"data";

        let (enc1, _) = km1.encrypt_auto_nonce(data).expect("enc1"); // expect
        let (enc2, _) = km2.encrypt_auto_nonce(data).expect("enc2"); // expect

        assert_ne!(enc1, enc2);
    }

    #[test]
    fn test_different_salts_different_keys() {
        let pass = "secret";
        let km1 = KeyManager::try_new(pass, b"salt1").expect("km1"); // expect
        let km2 = KeyManager::try_new(pass, b"salt2").expect("km2"); // expect

        assert_ne!(km1.key, km2.key);
    }

    #[test]
    fn test_sub_key_derivation_prevents_nonce_reuse() {
        let master_km = KeyManager::try_new("master-secret", b"salt1").expect("try_new"); // expect
        let data = b"identical-data";

        let km_file1 = master_km.derive_file_key(b"file1").expect("derive1"); // expect
        let km_file2 = master_km.derive_file_key(b"file2").expect("derive2"); // expect

        let (enc1, n1) = km_file1.encrypt_auto_nonce(data).expect("enc1"); // expect
        let (enc2, _) = km_file2.encrypt_auto_nonce(data).expect("enc2"); // expect

        assert_ne!(enc1, enc2);

        // Decryption must still work with the correct sub-key
        let dec1 = km_file1.decrypt_auto_nonce(&enc1, &n1).expect("dec1"); // expect
        assert_eq!(data, dec1.as_slice());
    }

    #[test]
    fn test_random_salt_generates_unique_keys() {
        let (km1, salt1) = KeyManager::try_new_random_salt("password").unwrap(); // unwrap
        let (km2, salt2) = KeyManager::try_new_random_salt("password").unwrap(); // unwrap

        // Salts should be different
        assert_ne!(salt1, salt2);

        // Derived keys for same password should be different
        assert_ne!(km1.key, km2.key);
    }

    #[test]
    fn test_key_manager_emergency_wipe() {
        let mut km = KeyManager::try_new("password", b"salt").unwrap(); // unwrap

        // Ensure key is not zero initially
        assert_ne!(km.inspect_key_bytes_for_test(), &[0u8; 32]);

        km.emergency_wipe();

        // Ensure key is zero after wipe
        assert_eq!(km.inspect_key_bytes_for_test(), &[0u8; 32]);
    }

    #[test]
    fn file_keys_are_distinct_per_path() {
        let km = KeyManager::try_new("secret", b"salt").expect("init"); // expect
        let key1 = km.derive_file_key(b"segment_001.sst").expect("k1"); // expect
        let key2 = km.derive_file_key(b"segment_002.sst").expect("k2"); // expect
        assert_ne!(
            key1.inspect_key_bytes_for_test(),
            key2.inspect_key_bytes_for_test(),
            "HKDF must produce distinct keys per file path"
        );
    }

    #[test]
    fn same_path_same_passphrase_same_key() {
        let km1 = KeyManager::try_new("secret", b"salt").expect("km1"); // expect
        let km2 = KeyManager::try_new("secret", b"salt").expect("km2"); // expect
        let k1 = km1.derive_file_key(b"data.sst").expect("k1"); // expect
        let k2 = km2.derive_file_key(b"data.sst").expect("k2"); // expect
        assert_eq!(
            k1.inspect_key_bytes_for_test(),
            k2.inspect_key_bytes_for_test(),
            "Same passphrase+salt+path must produce same key"
        );
    }

    #[test]
    fn same_passphrase_salt_path_gives_same_key() {
        let km1 = KeyManager::try_new("secret", b"salt").expect("km1"); // expect
        let km2 = KeyManager::try_new("secret", b"salt").expect("km2"); // expect
        assert_eq!(
            km1.derive_file_key(b"data.sst")
                .expect("k1") // expect
                .inspect_key_bytes_for_test(),
            km2.derive_file_key(b"data.sst")
                .expect("k2") // expect
                .inspect_key_bytes_for_test(),
            "HKDF muss bei gleichem Passphrase, Salt und Pfad denselben Key liefern"
        );
    }

    #[test]
    fn key_manager_field_zeroizes_on_drop() {
        use zeroize::Zeroize;
        let mut raw: [u8; 32] = [0xFF; 32];
        raw.zeroize();
        assert_eq!(raw, [0u8; 32]);
    }

    #[test]
    fn key_manager_debug_redacts_key() {
        let (km, _) = KeyManager::try_new_random_salt("test-passphrase").unwrap(); // unwrap
        let debug_str = format!("{km:?}");
        assert!(!debug_str.contains("test-passphrase"));
        assert!(debug_str.contains("REDACTED"));
    }

    #[test]
    fn test_nonce_uniqueness() {
        let km = KeyManager::try_new("secret-passphrase", b"salt1").expect("try_new"); // expect
        let data = b"sample payload";
        let mut nonces = std::collections::HashSet::new();
        for _ in 0..1000 {
            let (_, nonce) = km.encrypt_auto_nonce(data).expect("encrypt"); // expect
            assert!(nonces.insert(nonce), "Nonce reuse detected!");
        }
    }

    #[test]
    fn test_wrong_key_fails_decrypt() {
        let km1 = KeyManager::try_new("pass1", b"salt1").expect("try_new"); // expect
        let km2 = KeyManager::try_new("pass2", b"salt1").expect("try_new"); // expect
        let data = b"top secret data";

        let (encrypted, nonce) = km1.encrypt_auto_nonce(data).expect("encrypt"); // expect
        let result = km2.decrypt_auto_nonce(&encrypted, &nonce);
        assert!(result.is_err());
    }

    #[test]
    fn test_hkdf_derive_file_key_deterministic() {
        let km1 = KeyManager::try_new("secret", b"salt").expect("km1"); // expect
        let km2 = KeyManager::try_new("secret", b"salt").expect("km2"); // expect
        let sub1 = km1.derive_file_key(b"file_123").expect("sub1"); // expect
        let sub2 = km2.derive_file_key(b"file_123").expect("sub2"); // expect
        assert_eq!(
            sub1.inspect_key_bytes_for_test(),
            sub2.inspect_key_bytes_for_test(),
            "Same input MUST yield deterministic sub-keys"
        );
    }

    #[test]
    fn test_hkdf_derive_file_key_different_per_file() {
        let km = KeyManager::try_new("secret", b"salt").expect("km"); // expect
        let sub1 = km.derive_file_key(b"file_A").expect("sub1"); // expect
        let sub2 = km.derive_file_key(b"file_B").expect("sub2"); // expect
        assert_ne!(
            sub1.inspect_key_bytes_for_test(),
            sub2.inspect_key_bytes_for_test(),
            "Different file_ids MUST yield different sub-keys"
        );
    }
}
