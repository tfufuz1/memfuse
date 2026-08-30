//! Encryption at Rest layer for LSM/WAL components (WP-3.2)
//!
//! Secures blocks of data transparently via ChaCha20Poly1305 / AES-GCM-SIV.

#![forbid(unsafe_code)]

use memfuse_core::{Result, TxId};

use crate::crypto::KeyManager;

/// Provides Key Management Strategy hooks.
pub trait KmsProvider {
    /// Retrieves the Data Encryption Key (DEK).
    fn get_key(&self) -> Result<Vec<u8>>;
}

/// Encrypted WAL chunk provider that handles transparent encryption/decryption of WAL payloads.
///
/// # Invariants
/// - Uses per-file sub-key derivation via `KeyManager::derive_file_key()` to prevent nonce-reuse
///   across independent WAL streams sharing a master key.
/// - Prepends a unique 12-byte random nonce to every encrypted chunk output.
///
/// # Usage
/// Use when writing or reading encrypted WAL logs at rest. Call `encrypt_chunk` to wrap
/// plain payload bytes into an encrypted chunk with prepended nonce, and `decrypt_chunk`
/// to recover the original payload.
///
/// # Errors
/// Emits `MemFuseError::Crypto` if encryption/decryption fails, key derivation fails,
/// or if chunk length is less than 12 bytes during decryption.
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
    /// Prepends the 12-byte nonce to the encrypted ciphertext.
    pub fn encrypt_chunk(&self, payload: &[u8]) -> Result<Vec<u8>> {
        let (ciphertext, nonce) = self.key_manager.encrypt_auto_nonce(payload)?;
        let mut out = Vec::with_capacity(12 + ciphertext.len());
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&ciphertext);
        Ok(out)
    }

    /// Decrypts the WAL chunk by extracting the prepended 12-byte nonce from the data.
    pub fn decrypt_chunk(&self, data: &[u8]) -> Result<Vec<u8>> {
        if data.len() < 12 {
            return Err(memfuse_core::MemFuseError::Crypto(
                "Encrypted WAL chunk too short for 12-byte nonce".into(),
            ));
        }
        let mut nonce = [0u8; 12];
        nonce.copy_from_slice(&data[0..12]);
        let ciphertext = &data[12..];
        self.key_manager.decrypt_auto_nonce(ciphertext, &nonce)
    }
}

use hmac::{Hmac, Mac};
use sha2::Sha256;
use zeroize::{Zeroize, Zeroizing};

/// Stateful wrapper around HMAC-SHA256 initialized with WAL domain separation.
///
/// # Invariants
/// - Initialized with `b"memfuse-wal-v1"` domain separation string as the first updated block,
///   preventing cross-context HMAC collisions if the integrity key is reused elsewhere.
///
/// # Usage
/// Use to compute deterministic 32-byte HMAC-SHA256 checksums over WAL entry fields and
/// previous chain links during WAL appends or verification.
///
/// # Errors
/// Emits `MemFuseError::Crypto` if key initialization fails.
pub struct WalHmac {
    mac: Hmac<Sha256>,
}

impl WalHmac {
    pub fn new(integrity_key: &[u8]) -> Result<Self> {
        if integrity_key.is_empty() {
            return Err(memfuse_core::MemFuseError::invalid_input(
                "Integrity key must not be empty",
            ));
        }
        let mut mac = Hmac::<Sha256>::new_from_slice(integrity_key)
            .map_err(|e| memfuse_core::MemFuseError::Crypto(format!("HMAC key error: {}", e)))?;
        mac.update(b"memfuse-wal-v1");
        Ok(Self { mac })
    }

    pub fn update(&mut self, data: &[u8]) {
        self.mac.update(data);
    }

    pub fn finalize(self) -> [u8; 32] {
        self.mac.finalize().into_bytes().into()
    }
}

/// Immutable snapshot of a WAL entry used for cryptographic integrity verification.
///
/// # Invariants
/// - Captures entry fields (`seq_no`, `op_type`, `key`, `value`, `checksum`, `prev_hmac`)
///   required to recompute the entry's HMAC-SHA256 checksum and verify hash-chain continuity.
///
/// # Usage
/// Passed to `IntegrityVerifier::verify_and_update()` during WAL replay or recovery.
#[derive(Debug, Clone)]
pub struct WalEntrySnapshot {
    pub tx_id: TxId,
    pub seq_no: u64,
    pub op_type: u8, // 0: Put, 1: Delete
    pub key: Vec<u8>,
    pub value: Vec<u8>,
    pub checksum: [u8; 32],
    pub prev_hmac: [u8; 32],
}

/// Stateful verifier for WAL HMAC-SHA256 hash chains.
///
/// # Invariants
/// - Maintains running `last_hmac` state across entries to enforce sequential hash-chain continuity.
/// - Performs constant-time comparison (`subtle::ConstantTimeEq`) of checksums and `prev_hmac`
///   to prevent timing side-channel attacks.
/// - Zeroizes key material on drop to prevent secret key leakage from memory.
/// - Any HMAC discrepancy or broken chain link immediately halts verification.
///
/// # Usage
/// Instantiate with the WAL's integrity key at the start of replay and call `verify_and_update()`
/// sequentially for each deserialized WAL entry.
///
/// # Errors
/// Emits `MemFuseError::WalCorruption` immediately if `checksum` or `prev_hmac` does not match,
/// or `MemFuseError::Crypto` if HMAC initialization fails.
#[derive(Zeroize)]
#[zeroize(drop)]
pub struct IntegrityVerifier {
    last_hmac: [u8; 32],
    integrity_key: Zeroizing<Vec<u8>>,
}

impl IntegrityVerifier {
    pub fn new(integrity_key: &[u8]) -> Self {
        Self {
            last_hmac: [0u8; 32],
            integrity_key: Zeroizing::new(integrity_key.to_vec()),
        }
    }

    /// Verifies a V3 entry (with tx_id and length prefixes) and updates the chain state.
    pub fn verify_and_update_v3(&mut self, entry: &WalEntrySnapshot, offset: u64) -> Result<()> {
        let mut mac = WalHmac::new(&self.integrity_key)?;
        mac.update(&self.last_hmac);
        mac.update(&entry.seq_no.to_le_bytes());

        let tx_id_bytes = entry.tx_id.inner().to_le_bytes();
        mac.update(&tx_id_bytes);

        if entry.op_type == 0 {
            // Put
            mac.update(&[0u8]);
            mac.update(&(entry.key.len() as u32).to_le_bytes());
            mac.update(&entry.key);
            mac.update(&(entry.value.len() as u32).to_le_bytes());
            mac.update(&entry.value);
        } else {
            // Delete
            mac.update(&[1u8]);
            mac.update(&(entry.key.len() as u32).to_le_bytes());
            mac.update(&entry.key);
        }

        use subtle::ConstantTimeEq;
        let computed = mac.finalize();
        if computed.ct_eq(&entry.checksum).unwrap_u8() == 0
            || entry.prev_hmac.ct_eq(&self.last_hmac).unwrap_u8() == 0
        {
            return Err(memfuse_core::MemFuseError::wal_corruption(
                offset,
                format!("HMAC mismatch for seq {}", entry.seq_no),
            ));
        }

        self.last_hmac = computed;
        Ok(())
    }

    /// Verifies a V2 entry (legacy without tx_id and without length prefixes) and updates the chain state.
    pub fn verify_and_update_v2(&mut self, entry: &WalEntrySnapshot, offset: u64) -> Result<()> {
        let mut mac = WalHmac::new(&self.integrity_key)?;
        mac.update(&self.last_hmac);
        mac.update(&entry.seq_no.to_le_bytes());

        mac.update(&[entry.op_type]);
        mac.update(&entry.key);
        if entry.op_type == 0 {
            // Put
            mac.update(&entry.value);
        }

        use subtle::ConstantTimeEq;
        let computed = mac.finalize();
        if computed.ct_eq(&entry.checksum).unwrap_u8() == 0
            || entry.prev_hmac.ct_eq(&self.last_hmac).unwrap_u8() == 0
        {
            return Err(memfuse_core::MemFuseError::wal_corruption(
                offset,
                format!("HMAC mismatch for seq {}", entry.seq_no),
            ));
        }

        self.last_hmac = computed;
        Ok(())
    }

    /// Updates the chain state for legacy V1 entries without HMAC verification.
    pub fn skip_hmac_verify_legacy(&mut self, entry: &WalEntrySnapshot) {
        self.last_hmac = entry.checksum;
    }

    /// Default verification delegating to V3 verification.
    pub fn verify_and_update(&mut self, entry: &WalEntrySnapshot, offset: u64) -> Result<()> {
        self.verify_and_update_v3(entry, offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // INTENT: IntegrityVerifier lifecycle and HMAC chain validation verified.
    #[test]
    fn test_wal_hmac_basic() {
        let key = b"test-key-32-bytes-long-----------";
        let mut hmac = WalHmac::new(key).unwrap(); // unwrap
        hmac.update(b"data");
        let result = hmac.finalize();
        assert_ne!(result, [0u8; 32]);
    }

    #[test]
    fn test_wal_hmac_empty_key_fails() {
        let res = WalHmac::new(b"");
        assert!(res.is_err());
        if let Err(err) = res {
            assert!(matches!(
                err,
                memfuse_core::MemFuseError::InvalidInput { .. }
            ));
        }
    }

    #[test]
    fn test_integrity_verifier_chain() {
        let key = b"test-key-32-bytes-long-----------";
        let mut verifier = IntegrityVerifier::new(key);

        // entry 1
        let e1 = create_entry(key, [0u8; 32], 100, 0, b"k1", b"v1");
        let checksum1 = e1.checksum;

        verifier.verify_and_update(&e1, 100).expect("e1 valid"); // expect

        // entry 2
        let e2 = create_entry(key, checksum1, 101, 1, b"k1", b"");
        let checksum2 = e2.checksum;

        verifier.verify_and_update(&e2, 200).expect("e2 valid"); // expect

        // entry 3 (corrupt)
        let e3 = WalEntrySnapshot {
            tx_id: TxId::new(3),
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
        let encrypted = wal.encrypt_chunk(data)?;
        assert_ne!(encrypted.as_slice(), data);

        let decrypted = wal.decrypt_chunk(&encrypted)?;
        assert_eq!(decrypted.as_slice(), data);

        Ok(())
    }

    #[tokio::test]
    async fn test_encrypted_wal_1mb_roundtrip() -> Result<()> {
        let km = crate::crypto::KeyManager::try_new("test-pass-1mb", b"salt1234")?;
        let wal = EncryptedWal::new(km, b"test-wal-1mb.log")?;

        // 1MB payload
        let size = 1024 * 1024;
        let mut payload = vec![0u8; size];
        for (i, byte) in payload.iter_mut().enumerate() {
            *byte = (i % 251) as u8;
        }

        // Encrypt (prepends nonce)
        let encrypted = wal.encrypt_chunk(&payload)?;
        assert!(encrypted.len() >= 12 + size);
        assert_ne!(&encrypted[12..], &payload[..]);

        // Simulate serialization/deserialization over bytes
        let serialized_bytes = encrypted.clone();
        let deserialized_bytes = serialized_bytes.as_slice();

        // Decrypt
        let decrypted = wal.decrypt_chunk(deserialized_bytes)?;
        assert_eq!(decrypted.len(), payload.len());
        assert_eq!(decrypted, payload);

        Ok(())
    }

    fn create_entry(
        key: &[u8],
        prev_hmac: [u8; 32],
        seq_no: u64,
        op_type: u8,
        k: &[u8],
        v: &[u8],
    ) -> WalEntrySnapshot {
        let tx_id = TxId::new(seq_no);
        let mut hmac = WalHmac::new(key).expect("hmac init"); // expect
        hmac.update(&prev_hmac);
        hmac.update(&seq_no.to_le_bytes());
        hmac.update(&tx_id.inner().to_le_bytes());
        if op_type == 0 {
            hmac.update(&[0u8]);
            hmac.update(&(k.len() as u32).to_le_bytes());
            hmac.update(k);
            hmac.update(&(v.len() as u32).to_le_bytes());
            hmac.update(v);
        } else {
            hmac.update(&[1u8]);
            hmac.update(&(k.len() as u32).to_le_bytes());
            hmac.update(k);
        }
        let checksum = hmac.finalize();
        WalEntrySnapshot {
            tx_id,
            seq_no,
            op_type,
            key: k.to_vec(),
            value: v.to_vec(),
            checksum,
            prev_hmac,
        }
    }

    #[test]
    fn test_hmac_tamper_detection() {
        let key = b"integrity-key-32-bytes-long-----";
        let e1 = create_entry(key, [0u8; 32], 1, 0, b"key1", b"val1");
        let mut tampered_e1 = e1.clone();
        tampered_e1.value = b"tampered".to_vec();

        let mut verifier = IntegrityVerifier::new(key);
        assert!(verifier.verify_and_update(&tampered_e1, 10).is_err());
    }

    #[test]
    fn test_single_bit_checksum_corruption() {
        let key = b"integrity-key-32-bytes-long-----";
        let entry = create_entry(key, [0u8; 32], 1, 0, b"key1", b"val1");
        let mut corrupted_entry = entry.clone();
        corrupted_entry.checksum[0] ^= 0x01; // Flip a single bit in checksum

        let mut verifier = IntegrityVerifier::new(key);
        let err = verifier
            .verify_and_update(&corrupted_entry, 100)
            .unwrap_err();
        if let memfuse_core::MemFuseError::WalCorruption { offset, reason, .. } = err {
            assert_eq!(offset, 100);
            assert!(reason.contains("HMAC mismatch"));
        } else {
            panic!("Expected WalCorruption error");
        }
    }

    #[test]
    fn hmac_chain_detects_tampered_entry() {
        let key = b"integrity-key-32-bytes-long-----";
        let e1 = create_entry(key, [0u8; 32], 1, 0, b"key1", b"val1");
        let e2 = create_entry(key, e1.checksum, 2, 0, b"key2", b"val2");

        // Tamper with entry 2's payload value
        let mut tampered_e2 = e2.clone();
        tampered_e2.value = b"tampered_val2".to_vec();

        let e3 = create_entry(key, e2.checksum, 3, 0, b"key3", b"val3");

        let mut verifier = IntegrityVerifier::new(key);
        verifier.verify_and_update(&e1, 10).expect("e1 valid"); // expect

        // Verification of tampered entry 2 must fail
        assert!(verifier.verify_and_update(&tampered_e2, 20).is_err());

        // Verification of entry 3 with original e2's prev_hmac must fail because verifier chain was not updated with tampered e2
        assert!(verifier.verify_and_update(&e3, 30).is_err());
    }

    #[test]
    fn hmac_chain_detects_deleted_entry() {
        let key = b"integrity-key-32-bytes-long-----";
        let e1 = create_entry(key, [0u8; 32], 1, 0, b"key1", b"val1");
        let e2 = create_entry(key, e1.checksum, 2, 0, b"key2", b"val2");
        let e3 = create_entry(key, e2.checksum, 3, 0, b"key3", b"val3");

        let mut verifier = IntegrityVerifier::new(key);
        verifier.verify_and_update(&e1, 10).expect("e1 valid"); // expect

        // Skip entry 2 (removed entry) and attempt to verify entry 3 directly
        let err = verifier.verify_and_update(&e3, 30).unwrap_err();
        if let memfuse_core::MemFuseError::WalCorruption { offset, reason, .. } = err {
            assert_eq!(offset, 30);
            assert!(reason.contains("HMAC mismatch"));
        } else {
            panic!("Expected WalCorruption error");
        }
    }

    #[test]
    fn test_encrypted_wal_empty_file_id_fails() {
        let km = crate::crypto::KeyManager::try_new("test-pass", b"salt1").expect("km"); // expect
        let res = EncryptedWal::new(km, b"");
        assert!(res.is_err());
        if let Err(err) = res {
            assert!(matches!(
                err,
                memfuse_core::MemFuseError::InvalidInput { .. }
            ));
        } else {
            panic!("Expected InvalidInput error");
        }
    }

    #[test]
    fn test_encrypted_wal_decrypt_chunk_too_short_fails() {
        let km = crate::crypto::KeyManager::try_new("test-pass", b"salt1").expect("km"); // expect
        let wal = EncryptedWal::new(km, b"test.wal").expect("wal"); // expect

        let short_data = [0u8; 11]; // Less than 12 bytes
        let res = wal.decrypt_chunk(&short_data);
        assert!(res.is_err());
        let err = res.unwrap_err();
        if let memfuse_core::MemFuseError::Crypto(msg) = err {
            assert!(msg.contains("too short"));
        } else {
            panic!("Expected MemFuseError::Crypto error");
        }
    }

    #[test]
    fn test_encrypted_wal_empty_payload_roundtrip() {
        let km = crate::crypto::KeyManager::try_new("test-pass", b"salt1").expect("km"); // expect
        let wal = EncryptedWal::new(km, b"test-empty.wal").expect("wal"); // expect

        let encrypted = wal.encrypt_chunk(b"").expect("encrypt empty"); // expect
        assert_eq!(encrypted.len(), 12 + 16); // 12-byte nonce + 16-byte AES-GCM-SIV tag

        let decrypted = wal.decrypt_chunk(&encrypted).expect("decrypt empty"); // expect
        assert_eq!(decrypted.as_slice(), b"");
    }

    #[test]
    fn test_wal_hmac_independent_reference_verification() {
        let key = b"integrity-key-32-bytes-long-----";
        let mut wal_hmac = WalHmac::new(key).expect("wal_hmac init"); // expect
        wal_hmac.update(b"test-payload-data");
        let result = wal_hmac.finalize();

        // Anti-mirroring: Independently compute reference HMAC using raw Hmac<Sha256>
        let mut raw_mac = Hmac::<Sha256>::new_from_slice(key).expect("raw mac init"); // expect
        raw_mac.update(b"memfuse-wal-v1");
        raw_mac.update(b"test-payload-data");
        let expected_bytes: [u8; 32] = raw_mac.finalize().into_bytes().into();

        assert_eq!(
            result, expected_bytes,
            "WalHmac result must match independent raw HMAC calculation"
        );
    }

    #[test]
    fn test_integrity_verifier_v2_and_legacy_skip() {
        let key = b"integrity-key-32-bytes-long-----";
        let mut verifier = IntegrityVerifier::new(key);

        // V2 Entry creation (Put operation, op_type = 0)
        let seq_no = 50u64;
        let op_type = 0u8;
        let k = b"key_v2";
        let v = b"val_v2";

        let mut mac = WalHmac::new(key).expect("mac init"); // expect
        mac.update(&[0u8; 32]); // prev_hmac
        mac.update(&seq_no.to_le_bytes());
        mac.update(&[op_type]);
        mac.update(k);
        mac.update(v);
        let checksum_v2 = mac.finalize();

        let v2_entry = WalEntrySnapshot {
            tx_id: TxId::new(0),
            seq_no,
            op_type,
            key: k.to_vec(),
            value: v.to_vec(),
            checksum: checksum_v2,
            prev_hmac: [0u8; 32],
        };

        // Verify V2 entry
        verifier
            .verify_and_update_v2(&v2_entry, 500)
            .expect("v2 valid"); // expect

        // Test skip_hmac_verify_legacy
        let legacy_entry = WalEntrySnapshot {
            tx_id: TxId::new(0),
            seq_no: 51,
            op_type: 0,
            key: b"legacy_k".to_vec(),
            value: b"legacy_v".to_vec(),
            checksum: [0xAA; 32],
            prev_hmac: checksum_v2,
        };

        verifier.skip_hmac_verify_legacy(&legacy_entry);

        // V2 entry with tampered value should fail
        let mut tampered_v2 = v2_entry.clone();
        tampered_v2.value = b"tampered_v2".to_vec();
        tampered_v2.prev_hmac = [0xAA; 32];

        let err = verifier
            .verify_and_update_v2(&tampered_v2, 600)
            .unwrap_err();
        if let memfuse_core::MemFuseError::WalCorruption { offset, reason, .. } = err {
            assert_eq!(offset, 600);
            assert!(reason.contains("HMAC mismatch"));
        } else {
            panic!("Expected WalCorruption error");
        }
    }

    #[test]
    fn test_integrity_verifier_empty_key_fails_on_verify() {
        let mut verifier = IntegrityVerifier::new(b"");
        let entry = create_entry(b"dummy", [0u8; 32], 1, 0, b"k", b"v");

        let res = verifier.verify_and_update(&entry, 100);
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(matches!(
            err,
            memfuse_core::MemFuseError::InvalidInput { .. }
        ));
    }
}
