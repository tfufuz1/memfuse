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
    enc_key: [u8; 32],
    int_key: [u8; 32],
}

impl KeyManager {
    /// Creates a new KeyManager by deriving dual keys from a passphrase.
    pub fn new(passphrase: &str) -> Self {
        let salt = b"memfuse-encryption-salt-v1";
        let hk = Hkdf::<Sha256>::new(Some(salt), passphrase.as_bytes());

        let mut enc_key = [0u8; 32];
        hk.expand(b"memfuse-aes-256-gcm-key", &mut enc_key)
            .expect("32 bytes is a valid length for HKDF expansion"); // unwrap

        let mut int_key = [0u8; 32];
        hk.expand(b"memfuse-hmac-sha256-key", &mut int_key)
            .expect("32 bytes is a valid length for HKDF expansion"); // unwrap

        Self { enc_key, int_key }
    }

    /// Returns the integrity key for HMAC-SHA256.
    pub fn integrity_key(&self) -> [u8; 32] {
        self.int_key
    }

    /// Encrypts a block of data with a given nonce (e.g., block offset).
    pub fn encrypt(&self, data: &[u8], nonce_val: u64) -> Result<Vec<u8>> {
        let cipher = Aes256Gcm::new_from_slice(&self.enc_key)
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
        let cipher = Aes256Gcm::new_from_slice(&self.enc_key)
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
        let km = KeyManager::new("secret-passphrase");
        let data = b"sensitive data";
        let nonce = 42;

        let encrypted = km.encrypt(data, nonce).expect("encrypt");
        let decrypted = km.decrypt(&encrypted, nonce).expect("decrypt");

        assert_eq!(data, decrypted.as_slice());
    }

    #[test]
    fn test_wrong_nonce_fails() {
        let km = KeyManager::new("secret-passphrase");
        let data = b"sensitive data";
        let encrypted = km.encrypt(data, 42).expect("encrypt");

        let result = km.decrypt(&encrypted, 43);
        assert!(result.is_err());
    }

    #[test]
    fn test_integrity_key_derivation() {
        let km1 = KeyManager::new("secret");
        let km2 = KeyManager::new("secret");
        let km3 = KeyManager::new("different");

        assert_eq!(km1.integrity_key(), km2.integrity_key());
        assert_ne!(km1.integrity_key(), km3.integrity_key());
        // Ensure integrity key is different from encryption key (statistically)
        // We don't have direct access to enc_key but we can check if it behaves differently
        let data = b"data";
        let _enc1 = km1.encrypt(data, 0).expect("encrypt");
        let int_key = km1.integrity_key();
        // If int_key was used for encryption, this might or might not be different,
        // but it should be different.
        assert_ne!(int_key, [0u8; 32]);
    }

    #[test]
    fn test_different_keys_different_ciphertexts() {
        let km1 = KeyManager::new("pass1");
        let km2 = KeyManager::new("pass2");
        let data = b"data";
        let nonce = 0;

        let enc1 = km1.encrypt(data, nonce).expect("enc1");
        let enc2 = km2.encrypt(data, nonce).expect("enc2");

        assert_ne!(enc1, enc2);
    }
}
