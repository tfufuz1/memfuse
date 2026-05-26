// ANCHOR:INTEGRATION:CRYPTO-001 STATUS:DONE AGENT:12 DATE:2026-06-21
//! Integration tests for crypto operations.

use memfuse_crypto::crypto::KeyManager;

#[test]
fn test_key_manager_full_flow() {
    let passphrase = "secure-password-123";
    let km = KeyManager::try_new(passphrase).expect("Should derive key");

    let original_data = b"MemFuse secret document data";
    let nonce = 1337u64;

    // Encrypt
    let ciphertext = km.encrypt(original_data, nonce).expect("Encryption failed");
    assert_ne!(
        original_data.to_vec(),
        ciphertext,
        "Ciphertext must differ from plaintext"
    );

    // Decrypt
    let decrypted = km.decrypt(&ciphertext, nonce).expect("Decryption failed");
    assert_eq!(
        original_data.to_vec(),
        decrypted,
        "Decrypted data must match original"
    );
}

#[test]
fn test_crypto_integrity_check() {
    let km = KeyManager::try_new("pass").expect("ok");
    let data = b"some data";
    let nonce = 42;

    let mut ciphertext = km.encrypt(data, nonce).expect("ok");

    // Tamper with ciphertext
    if let Some(byte) = ciphertext.get_mut(5) {
        *byte ^= 0xFF;
    }

    // Decryption should fail due to AEAD tag mismatch
    let result = km.decrypt(&ciphertext, nonce);
    assert!(result.is_err(), "Decryption should fail for tampered data");
}

#[test]
fn test_nonce_isolation() {
    let km = KeyManager::try_new("pass").expect("ok");
    let data = b"data";

    let c1 = km.encrypt(data, 1).expect("ok");
    let c2 = km.encrypt(data, 2).expect("ok");

    assert_ne!(
        c1, c2,
        "Same data with different nonces must result in different ciphertexts"
    );

    // Decrypting with wrong nonce should fail
    assert!(km.decrypt(&c1, 2).is_err());
}

#[test]
fn test_consistent_key_derivation() {
    let pass = "consistent";
    let km1 = KeyManager::try_new(pass).expect("ok");
    let km2 = KeyManager::try_new(pass).expect("ok");

    let data = b"hello";
    let nonce = 0;

    let c1 = km1.encrypt(data, nonce).expect("ok");
    let c2 = km2.encrypt(data, nonce).expect("ok");

    assert_eq!(
        c1, c2,
        "Same passphrase must result in same key and ciphertext"
    );
}
