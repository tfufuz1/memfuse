//! Encryption utilities for MemFuse.
//!
//! Implements AES-256-GCM-SIV encryption and HKDF-SHA256 key derivation.
//!
//! # Nonce Design
//!
//! The **only** public encryption entry-point is [`KeyManager::encrypt_auto_nonce`],
//! which generates a collision-resistant 12-byte nonce composed of:
//! - 4 bytes: random `nonce_prefix` generated once per `KeyManager` instance.
//! - 8 bytes: monotonically increasing `AtomicU64` counter (starts at 1).
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
use sha2::Sha256;

use std::sync::atomic::{AtomicU64, Ordering};

/// Manager for encryption keys and block encryption.
pub struct KeyManager {
    key: VolatileEncryptionKey,
    nonce_counter: AtomicU64,
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
        let hk = Hkdf::<Sha256>::new(Some(salt), passphrase.as_bytes());
        let mut key_raw = [0u8; 32];
        hk.expand(b"memfuse-aes-256-gcm-key", &mut key_raw)
            .map_err(|e| MemFuseError::Storage(format!("HKDF expansion failed: {}", e)))?;

        use rand::RngCore;
        let mut nonce_prefix = [0u8; 4];
        rand::thread_rng().fill_bytes(&mut nonce_prefix);

        Ok(Self {
            key: VolatileEncryptionKey::new(key_raw),
            nonce_counter: AtomicU64::new(1),
            nonce_prefix,
        })
    }

    /// Creates a new KeyManager with a cryptographically secure random salt.
    pub fn try_new_random_salt(passphrase: &str) -> Result<(Self, [u8; 32])> {
        use rand::{thread_rng, RngCore};
        let mut salt = [0u8; 32];
        thread_rng().fill_bytes(&mut salt);
        let km = Self::try_new(passphrase, &salt)?;
        Ok((km, salt))
    }

    /// Derives a sub-key for a specific file or logical stream.
    /// This prevents nonce-reuse when multiple files use the same master key.
    pub fn derive_file_key(&self, file_id: &[u8]) -> Result<Self> {
        // Since self.key is already derived via HKDF in try_new, it is a high-entropy PRK.
        // We use HKDF-Expand with a domain-separating prefix to derive a per-file key.
        let hk = Hkdf::<Sha256>::from_prk(self.key.as_bytes())
            .map_err(|_| MemFuseError::Storage("Invalid PRK length".to_string()))?;

        let mut sub_key = [0u8; 32];
        let mut info = Vec::with_capacity(b"memfuse-file-key:".len() + file_id.len());
        info.extend_from_slice(b"memfuse-file-key:");
        info.extend_from_slice(file_id);

        hk.expand(&info, &mut sub_key)
            .map_err(|e| MemFuseError::Storage(format!("HKDF sub-key expansion failed: {}", e)))?;

        use rand::RngCore;
        let mut nonce_prefix = [0u8; 4];
        rand::thread_rng().fill_bytes(&mut nonce_prefix);

        Ok(Self {
            key: VolatileEncryptionKey::new(sub_key),
            nonce_counter: AtomicU64::new(1),
            nonce_prefix,
        })
    }

    /// Derives an integrity key for HMAC-SHA256.
    pub fn integrity_key(&self) -> Result<[u8; 32]> {
        let hk = Hkdf::<Sha256>::from_prk(self.key.as_bytes())
            .map_err(|_| MemFuseError::Storage("Invalid PRK length".to_string()))?;
        let mut key = [0u8; 32];
        hk.expand(b"memfuse-hmac-sha256-key", &mut key)
            .map_err(|e| {
                MemFuseError::Storage(format!("HKDF integrity expansion failed: {}", e))
            })?;
        Ok(key)
    }

    /// Encrypts a block of data with an automatically generated monotonic nonce.
    /// Returns the ciphertext and the full 12-byte nonce used.
    pub fn encrypt_auto_nonce(&self, data: &[u8]) -> Result<(Vec<u8>, [u8; 12])> {
        let nonce_val = self.nonce_counter.fetch_add(1, Ordering::SeqCst);
        let mut nonce_bytes = [0u8; 12];
        nonce_bytes[0..4].copy_from_slice(&self.nonce_prefix);
        nonce_bytes[4..12].copy_from_slice(&nonce_val.to_le_bytes());

        let cipher = Aes256GcmSiv::new_from_slice(self.key.as_bytes())
            .map_err(|e| MemFuseError::Storage(format!("Crypto error: {}", e)))?;
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(nonce, data)
            .map_err(|e| MemFuseError::Storage(format!("Encryption failed: {}", e)))?;

        Ok((ciphertext, nonce_bytes))
    }

    /// Decrypts a block of data using a full 12-byte nonce.
    pub fn decrypt_auto_nonce(&self, ciphertext: &[u8], nonce_bytes: &[u8; 12]) -> Result<Vec<u8>> {
        let cipher = Aes256GcmSiv::new_from_slice(self.key.as_bytes())
            .map_err(|e| MemFuseError::Storage(format!("Crypto error: {}", e)))?;
        let nonce = Nonce::from_slice(nonce_bytes);
        cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| MemFuseError::Storage(format!("Decryption failed: {}", e)))
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
        let km = KeyManager::try_new("secret-passphrase", b"salt1").expect("try_new");
        let data = b"sensitive data";

        let (encrypted, nonce) = km.encrypt_auto_nonce(data).expect("encrypt");
        let decrypted = km.decrypt_auto_nonce(&encrypted, &nonce).expect("decrypt");

        assert_eq!(data, decrypted.as_slice());
    }

    #[test]
    fn test_wrong_nonce_fails() {
        let km = KeyManager::try_new("secret-passphrase", b"salt1").expect("try_new");
        let data = b"sensitive data";
        let (encrypted, mut nonce) = km.encrypt_auto_nonce(data).expect("encrypt");
        nonce[0] ^= 1; // alter the nonce

        let result = km.decrypt_auto_nonce(&encrypted, &nonce);
        assert!(result.is_err());
    }

    #[test]
    fn test_different_keys_different_ciphertexts() {
        let km1 = KeyManager::try_new("pass1", b"salt1").expect("try_new");
        let km2 = KeyManager::try_new("pass2", b"salt1").expect("try_new");
        let data = b"data";

        let (enc1, _) = km1.encrypt_auto_nonce(data).expect("enc1");
        let (enc2, _) = km2.encrypt_auto_nonce(data).expect("enc2");

        assert_ne!(enc1, enc2);
    }

    #[test]
    fn test_different_salts_different_keys() {
        let pass = "secret";
        let km1 = KeyManager::try_new(pass, b"salt1").expect("km1");
        let km2 = KeyManager::try_new(pass, b"salt2").expect("km2");

        assert_ne!(km1.key, km2.key);
    }

    #[test]
    fn test_sub_key_derivation_prevents_nonce_reuse() {
        let master_km = KeyManager::try_new("master-secret", b"salt1").expect("try_new");
        let data = b"identical-data";

        let km_file1 = master_km.derive_file_key(b"file1").expect("derive1");
        let km_file2 = master_km.derive_file_key(b"file2").expect("derive2");

        let (enc1, n1) = km_file1.encrypt_auto_nonce(data).expect("enc1");
        let (enc2, _) = km_file2.encrypt_auto_nonce(data).expect("enc2");

        assert_ne!(enc1, enc2);

        // Decryption must still work with the correct sub-key
        let dec1 = km_file1.decrypt_auto_nonce(&enc1, &n1).expect("dec1");
        assert_eq!(data, dec1.as_slice());
    }

    #[test]
    fn test_random_salt_generates_unique_keys() {
        let (km1, salt1) = KeyManager::try_new_random_salt("password").unwrap();
        let (km2, salt2) = KeyManager::try_new_random_salt("password").unwrap();

        // Salts should be different
        assert_ne!(salt1, salt2);

        // Derived keys for same password should be different
        assert_ne!(km1.key, km2.key);
    }

    #[test]
    fn test_key_manager_emergency_wipe() {
        let mut km = KeyManager::try_new("password", b"salt").unwrap();

        // Ensure key is not zero initially
        assert_ne!(km.inspect_key_bytes_for_test(), &[0u8; 32]);

        km.emergency_wipe();

        // Ensure key is zero after wipe
        assert_eq!(km.inspect_key_bytes_for_test(), &[0u8; 32]);
    }
}
