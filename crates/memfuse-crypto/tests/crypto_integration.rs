// ANCHOR:INTEGRATION:CRYPTO-001 STATUS:READY AGENT:12
use memfuse_crypto::crypto::KeyManager;
use memfuse_crypto::wal_crypto::{IntegrityVerifier, WalEntrySnapshot};

#[test]
fn test_crypto_key_derivation_and_encryption() {
    let passphrase = "integration-test-pass";
    let km = KeyManager::try_new(passphrase).expect("failed to create key manager");

    let data = b"very secret information";
    let nonce = 12345;

    let encrypted = km.encrypt(data, nonce).expect("encryption failed");
    let decrypted = km.decrypt(&encrypted, nonce).expect("decryption failed");

    assert_eq!(data, decrypted.as_slice());

    // Verify different nonces produce different ciphertexts
    let encrypted2 = km.encrypt(data, nonce + 1).expect("encryption failed");
    assert_ne!(encrypted, encrypted2);
}

#[test]
fn test_crypto_wal_integrity_verification_logic() {
    let km = KeyManager::try_new("pass").unwrap();
    let integrity_key = km.integrity_key().unwrap();
    let mut verifier = IntegrityVerifier::new(&integrity_key);

    // In a real integration, we'd use the WAL writer to produce entries.
    // Here we just verify the verifier is accessible and constructible.
    assert!(verifier.verify_and_update(&WalEntrySnapshot {
        seq_no: 1,
        op_type: 0,
        key: b"k1".to_vec(),
        value: b"v1".to_vec(),
        checksum: [0u8; 32],
        prev_hmac: [0u8; 32],
    }).is_err(), "Should fail with zeroed checksum");
}
