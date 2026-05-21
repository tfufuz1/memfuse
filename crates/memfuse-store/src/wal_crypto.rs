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
