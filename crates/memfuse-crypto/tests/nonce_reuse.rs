use memfuse_crypto::crypto::KeyManager;

#[test]
fn test_nonce_reuse_vulnerability_demonstration() {
    let passphrase = "test-passphrase";
    let km1 = KeyManager::try_new(passphrase, None).expect("km1");
    let km2 = KeyManager::try_new(passphrase, None).expect("km2");

    let data = b"sensitive-data";
    let offset = 0;

    let enc1 = km1.encrypt(data, offset).expect("enc1");
    let enc2 = km2.encrypt(data, offset).expect("enc2");

    // This is the vulnerability: identical keys + identical nonces = identical ciphertexts
    assert_eq!(
        enc1, enc2,
        "VULNERABILITY: Identical ciphertexts for same key/nonce"
    );
}

#[test]
fn test_sub_key_derivation_prevents_reuse() {
    let passphrase = "test-passphrase";
    let master_km = KeyManager::try_new(passphrase, None).expect("master_km");

    let data = b"sensitive-data";
    let offset = 0;

    // Simulate two different files using derived sub-keys
    let km_file1 = master_km.derive_file_key(b"file1.log").expect("km1");
    let km_file2 = master_km.derive_file_key(b"file2.log").expect("km2");

    let enc1 = km_file1.encrypt(data, offset).expect("enc1");
    let enc2 = km_file2.encrypt(data, offset).expect("enc2");

    // Different sub-keys ensure different ciphertexts even if offset (nonce) is same
    assert_ne!(
        enc1, enc2,
        "SUCCESS: Different ciphertexts for different files"
    );
}

/// SD-09-CRYPTO-002: UUID-based file_id — same UUID bytes → same sub-key → same ciphertext.
///
/// When the same UUID is used as file_id (as would happen across WAL reopens
/// when the sidecar is read back), the sub-key must be deterministically
/// identical so that existing ciphertext can be decrypted.
#[test]
fn test_uuid_file_id_is_stable_for_same_uuid() {
    let master_km = KeyManager::try_new("master", None).expect("master");

    // Simulate reading back the same UUID from the sidecar on two calls
    let uuid_bytes: [u8; 16] = [
        0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66,
        0x77, 0x88,
    ];

    let km1 = master_km.derive_file_key(&uuid_bytes).expect("derive 1");
    let km2 = master_km.derive_file_key(&uuid_bytes).expect("derive 2");

    let data = b"block-data";
    let nonce = 42;

    let enc1 = km1.encrypt(data, nonce).expect("enc1");
    let dec2 = km2.decrypt(&enc1, nonce).expect("dec2");

    assert_eq!(
        data, dec2.as_slice(),
        "Sub-key must be identical for the same UUID bytes"
    );
}

/// SD-09-CRYPTO-002: Two distinct UUID v4 values yield distinct sub-keys,
/// preventing ciphertext collision at nonce 0.
#[test]
fn test_uuid_file_id_isolates_different_wals() {
    let master_km = KeyManager::try_new("shared-master", None).expect("master");

    // Two different WAL instances get different UUID v4 values
    let uuid1 = [0u8; 16]; // all-zeros UUID (simulating one WAL)
    let uuid2 = [0xFF; 16]; // all-ones UUID (simulating another WAL)

    let km1 = master_km.derive_file_key(&uuid1).expect("derive 1");
    let km2 = master_km.derive_file_key(&uuid2).expect("derive 2");

    let data = b"same-block-content";
    let nonce = 0; // identical block offset

    let enc1 = km1.encrypt(data, nonce).expect("enc1");
    let enc2 = km2.encrypt(data, nonce).expect("enc2");

    assert_ne!(
        enc1, enc2,
        "Different UUIDs must produce different ciphertexts at the same nonce"
    );
}
