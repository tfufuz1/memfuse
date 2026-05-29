use memfuse_crypto::crypto::KeyManager;

#[test]
fn test_nonce_reuse_vulnerability_demonstration() {
    let passphrase = "test-passphrase";
    let km1 = KeyManager::try_new(passphrase, None).expect("km1"); // expect #[cfg(test)] // expect #[cfg(test)]
    let km2 = KeyManager::try_new(passphrase, None).expect("km2"); // expect #[cfg(test)] // expect #[cfg(test)]

    let data = b"sensitive-data";
    let offset = 0;

    let enc1 = km1.encrypt(data, offset).expect("enc1"); // expect #[cfg(test)] // expect #[cfg(test)]
    let enc2 = km2.encrypt(data, offset).expect("enc2"); // expect #[cfg(test)] // expect #[cfg(test)]

    // This is the vulnerability: identical keys + identical nonces = identical ciphertexts
    assert_eq!(
        enc1, enc2,
        "VULNERABILITY: Identical ciphertexts for same key/nonce"
    );
}

#[test]
fn test_sub_key_derivation_prevents_reuse() {
    let passphrase = "test-passphrase";
    let master_km = KeyManager::try_new(passphrase, None).expect("master_km"); // expect #[cfg(test)] // expect #[cfg(test)]

    let data = b"sensitive-data";
    let offset = 0;

    // Simulate two different files using derived sub-keys
    let km_file1 = master_km.derive_file_key(b"file1.log").expect("km1"); // expect #[cfg(test)] // expect #[cfg(test)]
    let km_file2 = master_km.derive_file_key(b"file2.log").expect("km2"); // expect #[cfg(test)] // expect #[cfg(test)]

    let enc1 = km_file1.encrypt(data, offset).expect("enc1"); // expect #[cfg(test)] // expect #[cfg(test)]
    let enc2 = km_file2.encrypt(data, offset).expect("enc2"); // expect #[cfg(test)] // expect #[cfg(test)]

    // Different sub-keys ensure different ciphertexts even if offset (nonce) is same
    assert_ne!(
        enc1, enc2,
        "SUCCESS: Different ciphertexts for different files"
    );
}
