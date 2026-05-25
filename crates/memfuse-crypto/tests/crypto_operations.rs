// AGENT:12 DATE:2026-05-25 STATUS:READY
// ANCHOR:INTEGRATION:CRYPTO-001 — Key derivation, encryption, and integrity.

use memfuse_crypto::crypto::KeyManager;
use memfuse_crypto::wal_crypto::{IntegrityVerifier, WalEntrySnapshot};

#[test]
fn test_key_manager_integration() {
    let passphrase = "integration-test-password";
    let km = KeyManager::try_new(passphrase).expect("Should derive key");

    let original_data = b"This is a secret message for integration testing.";
    let nonce = 123456789;

    // Encrypt
    let ciphertext = km.encrypt(original_data, nonce).expect("Encryption failed");
    assert_ne!(original_data.to_vec(), ciphertext);

    // Decrypt
    let decrypted = km.decrypt(&ciphertext, nonce).expect("Decryption failed");
    assert_eq!(original_data.to_vec(), decrypted);

    // Integrity key derivation
    let i_key = km.integrity_key().expect("Should derive integrity key");
    assert_ne!(i_key, [0u8; 32]);
}

#[test]
fn test_wal_integrity_chain_integration() {
    let km = KeyManager::try_new("chain-password").unwrap();
    let integrity_key = km.integrity_key().unwrap();

    let mut verifier = IntegrityVerifier::new(&integrity_key);

    // Create a chain of 3 entries
    let mut last_hmac = [0u8; 32];
    let mut entries = Vec::new();

    for i in 0..3 {
        let seq: u64 = 1000 + i;
        let key = format!("key-{}", i).into_bytes();
        let val = format!("val-{}", i).into_bytes();

        let mut hmac_builder = memfuse_crypto::wal_crypto::WalHmac::new(&integrity_key).unwrap();
        hmac_builder.update(&last_hmac);
        hmac_builder.update(&seq.to_le_bytes());
        hmac_builder.update(&[0u8]); // Put
        hmac_builder.update(&key);
        hmac_builder.update(&val);
        let checksum = hmac_builder.finalize();

        entries.push(WalEntrySnapshot {
            seq_no: seq,
            op_type: 0,
            key,
            value: val,
            checksum,
            prev_hmac: last_hmac,
        });

        last_hmac = checksum;
    }

    // Verify the whole chain
    for entry in &entries {
        verifier.verify_and_update(entry).expect("Chain should be valid");
    }

    // Tamper with an entry
    let mut tampered = entries[1].clone();
    tampered.value = b"corrupted".to_vec();

    let mut verifier2 = IntegrityVerifier::new(&integrity_key);
    verifier2.verify_and_update(&entries[0]).unwrap();
    let result = verifier2.verify_and_update(&tampered);
    assert!(result.is_err(), "Integrity verification should fail for tampered data");
}
