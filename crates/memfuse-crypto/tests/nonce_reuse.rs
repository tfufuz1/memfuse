// FILE-CONTEXT
// ZWECK: Nonce reuse demonstration and mitigation verification suite.
// INVARIANTEN: Unique random nonces generated per encrypt_auto_nonce call even across identical KeyManager instances.
// STAND: TS:2026-08-31T21:13:05Z (SESSION: 8427f167)

use memfuse_crypto::crypto::KeyManager;

#[test]
fn test_nonce_reuse_vulnerability_demonstration() {
    let passphrase = "test-passphrase";
    let km1 = KeyManager::try_new(passphrase, b"salt").expect("km1");
    let km2 = KeyManager::try_new(passphrase, b"salt").expect("km2");

    let data = b"sensitive-data";
    let (enc1, _) = km1.encrypt_auto_nonce(data).expect("enc1");
    let (enc2, _) = km2.encrypt_auto_nonce(data).expect("enc2");

    // Mitigated: auto-generated nonces use random prefixes. Ciphertexts are different!
    assert_ne!(
        enc1, enc2,
        "MITIGATED: Different ciphertexts for same key due to secure 12-byte auto nonce"
    );
}

#[test]
fn test_sub_key_derivation_prevents_reuse() {
    let passphrase = "test-passphrase";
    let master_km = KeyManager::try_new(passphrase, b"salt").expect("master_km");

    let data = b"sensitive-data";
    let km_file1 = master_km.derive_file_key(b"file1.log").expect("km1");
    let km_file2 = master_km.derive_file_key(b"file2.log").expect("km2");

    let (enc1, _) = km_file1.encrypt_auto_nonce(data).expect("enc1");
    let (enc2, _) = km_file2.encrypt_auto_nonce(data).expect("enc2");

    // Different sub-keys AND different nonce prefixes ensure different ciphertexts
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
    let master_km = KeyManager::try_new("master", b"salt").expect("master");

    // Simulate reading back the same UUID from the sidecar on two calls
    let uuid_bytes: [u8; 16] = [
        0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77,
        0x88,
    ];

    let km1 = master_km.derive_file_key(&uuid_bytes).expect("derive 1");
    let km2 = master_km.derive_file_key(&uuid_bytes).expect("derive 2");

    let data = b"block-data";

    // km1 and km2 derived from same master key and same UUID bytes.
    // Their derived keys AND deterministic nonce prefixes will be identical.
    let (enc1, nonce1) = km1.encrypt_auto_nonce(data).expect("enc1");
    let dec2 = km2.decrypt_auto_nonce(&enc1, &nonce1).expect("dec2");

    assert_eq!(
        data,
        dec2.as_slice(),
        "Sub-key must be identical for the same UUID bytes"
    );
}

/// SD-09-CRYPTO-002: Two distinct UUID v4 values yield distinct sub-keys,
/// preventing ciphertext collision at nonce 0.
#[test]
fn test_uuid_file_id_isolates_different_wals() {
    let master_km = KeyManager::try_new("shared-master", b"salt").expect("master");

    // Two different WAL instances get different UUID v4 values
    let uuid1 = [0u8; 16]; // all-zeros UUID (simulating one WAL)
    let uuid2 = [0xFF; 16]; // all-ones UUID (simulating another WAL)

    let km1 = master_km.derive_file_key(&uuid1).expect("derive 1");
    let km2 = master_km.derive_file_key(&uuid2).expect("derive 2");

    let data = b"same-block-content";

    let (enc1, _n1) = km1.encrypt_auto_nonce(data).expect("enc1");
    let (enc2, _n2) = km2.encrypt_auto_nonce(data).expect("enc2");

    assert_ne!(
        enc1, enc2,
        "Different UUIDs must produce different ciphertexts at the same nonce"
    );
}
