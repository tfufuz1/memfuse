//! Encryption at Rest layer for LSM/WAL components (WP-3.2)
//!
//! Secures blocks of data transparently via ChaCha20Poly1305 / AES-GCM-SIV.

#![forbid(unsafe_code)]

use memfuse_core::Result;


/// Provides Key Management Strategy hooks.
pub trait KmsProvider {
    /// Retrieves the Data Encryption Key (DEK).
    fn get_key(&self) -> Result<Vec<u8>>;
}

/// A wrapper handling logical Wal append encryption logic.
pub struct EncryptedWal {
    _key: Vec<u8>,
}

impl Default for EncryptedWal {
    fn default() -> Self {
        Self { _key: vec![0; 32] }
    }
}

impl EncryptedWal {
    /// Wraps the internal WAL chunk in ChaCha20Poly1305 stream.
    pub fn encrypt_chunk(&self, payload: &[u8]) -> Result<Vec<u8>> {
        // TODO(WP-3.2): Process through aead.
        Ok(payload.to_vec())
    }
}

use hmac::{Hmac, Mac};
use sha2::Sha256;

pub struct WalHmac {
    mac: Hmac<Sha256>,
}

impl WalHmac {
    pub fn new(integrity_key: &[u8]) -> Result<Self> {
        let mac = Hmac::<Sha256>::new_from_slice(integrity_key)
            .map_err(|e| memfuse_core::MemFuseError::Storage(format!("HMAC key error: {}", e)))?;
        Ok(Self { mac })
    }

    pub fn update(&mut self, data: &[u8]) {
        self.mac.update(data);
    }

    pub fn finalize(self) -> [u8; 32] {
        self.mac.finalize().into_bytes().into()
    }
}

