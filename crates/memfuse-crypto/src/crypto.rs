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
    pub fn try_new(passphrase: &str) -> Result<Self> {
        let salt = b"memfuse-encryption-salt-v1";
        let hk = Hkdf::<Sha256>::new(Some(salt), passphrase.as_bytes());
        let mut key = [0u8; 32];
        hk.expand(b"memfuse-aes-256-gcm-key", &mut key)
            .map_err(|e| MemFuseError::Storage(format!("HKDF expansion failed: {}", e)))?;

        Ok(Self { key })
    }

    /// Derives an integrity key for HMAC-SHA256.
    pub fn integrity_key(&self) -> Result<[u8; 32]> {
        let salt = b"memfuse-encryption-salt-v1";
        let hk = Hkdf::<Sha256>::new(Some(salt), &self.key);
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
        let km = KeyManager::try_new("secret-passphrase").expect("try_new");
        let data = b"sensitive data";
        let nonce = 42;

        let encrypted = km.encrypt(data, nonce).expect("encrypt");
        let decrypted = km.decrypt(&encrypted, nonce).expect("decrypt");

        assert_eq!(data, decrypted.as_slice());
    }

    #[test]
    fn test_wrong_nonce_fails() {
        let km = KeyManager::try_new("secret-passphrase").expect("try_new");
        let data = b"sensitive data";
        let encrypted = km.encrypt(data, 42).expect("encrypt");

        let result = km.decrypt(&encrypted, 43);
        assert!(result.is_err());
    }

    #[test]
    fn test_different_keys_different_ciphertexts() {
        let km1 = KeyManager::try_new("pass1").expect("try_new");
        let km2 = KeyManager::try_new("pass2").expect("try_new");
        let data = b"data";
        let nonce = 0;

        let enc1 = km1.encrypt(data, nonce).expect("enc1");
        let enc2 = km2.encrypt(data, nonce).expect("enc2");

        assert_ne!(enc1, enc2);
    }
}
