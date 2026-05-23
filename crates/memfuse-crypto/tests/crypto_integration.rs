use memfuse_crypto::crypto::KeyManager;

// ANCHOR:INTEGRATION:CRYPTO-001 STATUS:DONE AGENT:12 DATE:2026-05-23
// Verifies that KeyManager provides consistent encryption and cross-instance compatibility.
#[test]
fn test_crypto_consistency_integration() {
    let passphrase = "integration-test-secret";
    let km1 = KeyManager::try_new(passphrase).expect("km1");
    let km2 = KeyManager::try_new(passphrase).expect("km2");

    let data = b"This is a secret message for integration testing.";
    let nonce = 123456789;

    // km1 encrypts
    let encrypted = km1.encrypt(data, nonce).expect("encrypt");

    // km2 decrypts (simulating persistence/restart with same passphrase)
    let decrypted = km2.decrypt(&encrypted, nonce).expect("decrypt");

    assert_eq!(data, decrypted.as_slice());
}

#[test]
fn test_integrity_key_derivation() {
    let passphrase = "another-secret";
    let km = KeyManager::try_new(passphrase).expect("km");

    let k1 = km.integrity_key().expect("k1");
    let k2 = km.integrity_key().expect("k2");

    // Key should be consistent
    assert_eq!(k1, k2);

    // Key should be 32 bytes (256-bit)
    assert_eq!(k1.len(), 32);

    // Different passphrases should yield different integrity keys
    let km_other = KeyManager::try_new("different").expect("km_other");
    let k_other = km_other.integrity_key().expect("k_other");
    assert_ne!(k1, k_other);
}

#[test]
fn test_block_encryption_different_nonces() {
    let km = KeyManager::try_new("nonce-test").expect("km");
    let data = b"same data content";

    let enc1 = km.encrypt(data, 1).expect("enc1");
    let enc2 = km.encrypt(data, 2).expect("enc2");

    // Ciphertext must be different for different nonces even with same key and data
    assert_ne!(enc1, enc2);

    // Both must decrypt correctly with their respective nonces
    assert_eq!(km.decrypt(&enc1, 1).expect("dec1"), data);
    assert_eq!(km.decrypt(&enc2, 2).expect("dec2"), data);
}
