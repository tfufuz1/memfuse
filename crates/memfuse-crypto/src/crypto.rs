//! Encryption utilities for MemFuse.
//!
//! Implements AES-256-GCM encryption and HKDF-SHA256 key derivation.

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use hkdf::Hkdf;
use memfuse_core::{MemFuseError, Result};
use sha2::Sha256;

/// Manager for encryption keys and block encryption.
pub struct KeyManager {
    key: [u8; 32],
}

impl std::fmt::Debug for KeyManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyManager")
            .field("key", &"***REDACTED***")
            .finish()
    }
}

impl KeyManager {
    /// Creates a new KeyManager by deriving a key from a passphrase.
    pub fn try_new(passphrase: &str, salt: Option<&[u8]>) -> Result<Self> {
        // FIND-CRY-001: Support dynamic salt.
        // Legacy salt for backward compatibility if None is provided.
        let salt = salt.unwrap_or(b"memfuse-encryption-salt-v1");
        let hk = Hkdf::<Sha256>::new(Some(salt), passphrase.as_bytes());
        let mut key = [0u8; 32];
        hk.expand(b"memfuse-aes-256-gcm-key", &mut key)
            .map_err(|e| MemFuseError::Storage(format!("HKDF expansion failed: {}", e)))?;

        Ok(Self { key })
    }

    /// Derives a sub-key for a specific file or logical stream.
    /// This prevents nonce-reuse when multiple files use the same master key.
    pub fn derive_file_key(&self, file_id: &[u8]) -> Result<Self> {
        let hk = Hkdf::<Sha256>::new(None, &self.key);
        let mut sub_key = [0u8; 32];
        hk.expand(file_id, &mut sub_key)
            .map_err(|e| MemFuseError::Storage(format!("HKDF sub-key expansion failed: {}", e)))?;
        Ok(Self { key: sub_key })
    }

    /// Derives an integrity key for HMAC-SHA256.
    pub fn integrity_key(&self) -> Result<[u8; 32]> {
        // Use the already-salted key for derivation to prevent rainbow table attacks.
        let hk = Hkdf::<Sha256>::new(None, &self.key);
        let mut key = [0u8; 32];
        hk.expand(b"memfuse-hmac-sha256-key", &mut key)
            .map_err(|e| {
                MemFuseError::Storage(format!("HKDF integrity expansion failed: {}", e))
            })?;
        Ok(key)
    }

    /// Encrypts a block of data with a given nonce (e.g., block offset).
    pub fn encrypt(&self, data: &[u8], nonce_val: u64) -> Result<Vec<u8>> {
        let cipher = Aes256Gcm::new_from_slice(&self.key)
            .map_err(|e| MemFuseError::Storage(format!("Crypto error: {}", e)))?;

        let mut nonce_bytes = [0u8; 12];
        nonce_bytes[4..12].copy_from_slice(&nonce_val.to_le_bytes());
        let nonce = Nonce::from_slice(&nonce_bytes);

        cipher
            .encrypt(nonce, data)
            .map_err(|e| MemFuseError::Storage(format!("Encryption failed: {}", e)))
    }

    /// Decrypts a block of data.
    pub fn decrypt(&self, ciphertext: &[u8], nonce_val: u64) -> Result<Vec<u8>> {
        let cipher = Aes256Gcm::new_from_slice(&self.key)
            .map_err(|e| MemFuseError::Storage(format!("Crypto error: {}", e)))?;

        let mut nonce_bytes = [0u8; 12];
        nonce_bytes[4..12].copy_from_slice(&nonce_val.to_le_bytes());
        let nonce = Nonce::from_slice(&nonce_bytes);

        cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| MemFuseError::Storage(format!("Decryption failed: {}", e)))
    }
}

#[cfg(test)]
mod tests {
    pub use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let km = KeyManager::try_new("secret-passphrase", None).expect("try_new");
        let data = b"sensitive data";
        let nonce = 42;

        let encrypted = km.encrypt(data, nonce).expect("encrypt");
        let decrypted = km.decrypt(&encrypted, nonce).expect("decrypt");

        assert_eq!(data, decrypted.as_slice());
    }

    #[test]
    fn test_wrong_nonce_fails() {
        let km = KeyManager::try_new("secret-passphrase", None).expect("try_new");
        let data = b"sensitive data";
        let encrypted = km.encrypt(data, 42).expect("encrypt");

        let result = km.decrypt(&encrypted, 43);
        assert!(result.is_err());
    }

    #[test]
    fn test_different_keys_different_ciphertexts() {
        let km1 = KeyManager::try_new("pass1", None).expect("try_new");
        let km2 = KeyManager::try_new("pass2", None).expect("try_new");
        let data = b"data";
        let nonce = 0;

        let enc1 = km1.encrypt(data, nonce).expect("enc1");
        let enc2 = km2.encrypt(data, nonce).expect("enc2");

        assert_ne!(enc1, enc2);
    }

    #[test]
    fn test_different_salts_different_keys() {
        let pass = "secret";
        let km1 = KeyManager::try_new(pass, Some(b"salt1")).expect("km1");
        let km2 = KeyManager::try_new(pass, Some(b"salt2")).expect("km2");

        assert_ne!(km1.key, km2.key);
    }

    #[test]
    fn test_legacy_salt_compatibility() {
        let pass = "secret";
        let km_legacy = KeyManager::try_new(pass, None).expect("legacy");
        let km_explicit =
            KeyManager::try_new(pass, Some(b"memfuse-encryption-salt-v1")).expect("explicit");

        assert_eq!(km_legacy.key, km_explicit.key);
    }

    #[test]
    fn test_sub_key_derivation_prevents_nonce_reuse() {
        let master_km = KeyManager::try_new("master-secret", None).expect("try_new");
        let data = b"identical-data";
        let offset = 0;

        // Currently, we don't have derive_file_key, so we can't write this yet.
        // But we want to ensure that if we had it, ciphertexts would differ.

        let km_file1 = master_km.derive_file_key(b"file1").expect("derive1");
        let km_file2 = master_km.derive_file_key(b"file2").expect("derive2");

        let enc1 = km_file1.encrypt(data, offset).expect("enc1");
        let enc2 = km_file2.encrypt(data, offset).expect("enc2");

        assert_ne!(
            enc1, enc2,
            "Ciphertexts must differ even if offset is identical"
        );

        // Decryption must still work with the correct sub-key
        let dec1 = km_file1.decrypt(&enc1, offset).expect("dec1");
        assert_eq!(data, dec1.as_slice());
    }
}
