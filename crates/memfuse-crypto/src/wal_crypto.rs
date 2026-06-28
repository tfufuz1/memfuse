//! Encryption at Rest layer for LSM/WAL components (WP-3.2)
//!
//! Secures blocks of data transparently via ChaCha20Poly1305 / AES-GCM-SIV.

#![forbid(unsafe_code)]

use memfuse_core::Result;

use crate::crypto::KeyManager;

/// Provides Key Management Strategy hooks.
pub trait KmsProvider {
    /// Retrieves the Data Encryption Key (DEK).
    fn get_key(&self) -> Result<Vec<u8>>;
}

/// A wrapper handling logical Wal append encryption logic.
pub struct EncryptedWal {
    key_manager: KeyManager,
}

impl EncryptedWal {
    /// Creates a new EncryptedWal with per-file key derivation to prevent nonce-reuse.
    /// `file_id` (e.g., filename) is used to derive a unique sub-key for this stream.
    pub fn new(key_manager: KeyManager, file_id: &[u8]) -> Result<Self> {
        let sub_km = key_manager.derive_file_key(file_id)?;
        Ok(Self {
            key_manager: sub_km,
        })
    }

    /// Wraps the internal WAL chunk in AES-256-GCM stream.
    pub fn encrypt_chunk(&self, payload: &[u8]) -> Result<(Vec<u8>, [u8; 12])> {
        self.key_manager.encrypt_auto_nonce(payload)
    }

    /// Decrypts the WAL chunk from the AES-256-GCM stream.
    pub fn decrypt_chunk(&self, ciphertext: &[u8], nonce: &[u8; 12]) -> Result<Vec<u8>> {
        self.key_manager.decrypt_auto_nonce(ciphertext, nonce)
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

/// A snapshot of a WAL entry for cryptographic verification.
#[derive(Debug, Clone)]
pub struct WalEntrySnapshot {
    pub seq_no: u64,
    pub op_type: u8, // 0: Put, 1: Delete
    pub key: Vec<u8>,
    pub value: Vec<u8>,
    pub checksum: [u8; 32],
    pub prev_hmac: [u8; 32],
}

/// Helper for stateful verification of a WAL HMAC chain.
pub struct IntegrityVerifier {
    last_hmac: [u8; 32],
    integrity_key: Vec<u8>,
}

impl IntegrityVerifier {
    pub fn new(integrity_key: &[u8]) -> Self {
        Self {
            last_hmac: [0u8; 32],
            integrity_key: integrity_key.to_vec(),
        }
    }

    /// Verifies an entry and updates the chain state.
    pub fn verify_and_update(&mut self, entry: &WalEntrySnapshot, offset: u64) -> Result<()> {
        let mut mac = WalHmac::new(&self.integrity_key)?;
        mac.update(&self.last_hmac);
        mac.update(&entry.seq_no.to_le_bytes());

        mac.update(&[entry.op_type]);
        mac.update(&entry.key);
        if entry.op_type == 0 {
            // Put
            mac.update(&entry.value);
        }

        let computed = mac.finalize();
        if computed != entry.checksum || entry.prev_hmac != self.last_hmac {
            return Err(memfuse_core::MemFuseError::WalCorruption {
                offset,
                reason: format!("HMAC mismatch for seq {}", entry.seq_no),
            });
        }

        self.last_hmac = computed;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // INTENT: IntegrityVerifier lifecycle and HMAC chain validation verified.
    #[test]
    fn test_wal_hmac_basic() {
        let key = b"test-key-32-bytes-long-----------";
        let mut hmac = WalHmac::new(key).unwrap();
        hmac.update(b"data");
        let result = hmac.finalize();
        assert_ne!(result, [0u8; 32]);
    }

    #[test]
    fn test_integrity_verifier_chain() {
        let key = b"test-key-32-bytes-long-----------";
        let mut verifier = IntegrityVerifier::new(key);

        // entry 1
        let mut hmac1 = WalHmac::new(key).unwrap();
        hmac1.update(&[0u8; 32]); // prev_hmac
        hmac1.update(&100u64.to_le_bytes()); // seq
        hmac1.update(&[0u8]); // op_type Put
        hmac1.update(b"k1");
        hmac1.update(b"v1");
        let checksum1 = hmac1.finalize();

        let e1 = WalEntrySnapshot {
            seq_no: 100,
            op_type: 0,
            key: b"k1".to_vec(),
            value: b"v1".to_vec(),
            checksum: checksum1,
            prev_hmac: [0u8; 32],
        };

        verifier.verify_and_update(&e1, 100).expect("e1 valid");

        // entry 2
        let mut hmac2 = WalHmac::new(key).unwrap();
        hmac2.update(&checksum1); // prev_hmac is checksum1
        hmac2.update(&101u64.to_le_bytes());
        hmac2.update(&[1u8]); // op_type Delete
        hmac2.update(b"k1");
        let checksum2 = hmac2.finalize();

        let e2 = WalEntrySnapshot {
            seq_no: 101,
            op_type: 1,
            key: b"k1".to_vec(),
            value: Vec::new(),
            checksum: checksum2,
            prev_hmac: checksum1,
        };

        verifier.verify_and_update(&e2, 200).expect("e2 valid");

        // entry 3 (corrupt)
        let e3 = WalEntrySnapshot {
            seq_no: 102,
            op_type: 1,
            key: b"k1".to_vec(),
            value: Vec::new(),
            checksum: [0u8; 32],
            prev_hmac: checksum2,
        };
        let err = verifier.verify_and_update(&e3, 300).unwrap_err();
        if let memfuse_core::MemFuseError::WalCorruption { offset, .. } = err {
            assert_eq!(offset, 300);
        } else {
            panic!("Expected WalCorruption with offset");
        }
    }

    #[tokio::test]
    async fn test_encrypted_wal_roundtrip() -> Result<()> {
        let km = crate::crypto::KeyManager::try_new("test-pass", b"salt1")?;
        let wal = EncryptedWal::new(km, b"test-wal.log")?;
        let data = b"wal-entry-data-to-encrypt";
        let (encrypted, nonce) = wal.encrypt_chunk(data)?;
        assert_ne!(encrypted.as_slice(), data);

        let decrypted = wal.decrypt_chunk(&encrypted, &nonce)?;
        assert_eq!(decrypted.as_slice(), data);

        Ok(())
    }
}
