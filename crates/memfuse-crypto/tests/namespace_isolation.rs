//! SD-09-CRYPTO-002 — Namespace Isolation Tests.
//!
//! Verifies that `derive_file_key` produces unique sub-keys even when
//! filenames are identical across different namespaces/shards. Two WAL files
//! named `wal-100.log` in different shards MUST NOT share encryption keys.

use memfuse_crypto::crypto::KeyManager;

/// When file_id includes the full namespace path (e.g. "shard-a/wal-100.log"
/// vs "shard-b/wal-100.log"), the derived sub-keys MUST be different.
/// This is the recommended mitigation for SD-09-CRYPTO-002.
#[test]
fn test_same_filename_different_namespace_yields_different_keys() {
    let master = KeyManager::try_new("master-secret", b"salt").expect("master");

    let km_a = master
        .derive_file_key(b"shard-a/wal-100.log")
        .expect("derive shard-a");
    let km_b = master
        .derive_file_key(b"shard-b/wal-100.log")
        .expect("derive shard-b");

    let data = b"identical-payload";
    let (enc_a, nonce_a) = km_a.encrypt_auto_nonce(data).expect("enc_a");
    let (enc_b, _nonce_b) = km_b.encrypt_auto_nonce(data).expect("enc_b");

    // Ciphertexts MUST differ — different sub-keys
    assert_ne!(
        enc_a, enc_b,
        "Different namespaces must produce different ciphertexts"
    );

    // Cross-key decryption must FAIL
    let cross_decrypt = km_b.decrypt_auto_nonce(&enc_a, &nonce_a);
    assert!(
        cross_decrypt.is_err(),
        "Decrypting shard-a data with shard-b key must fail"
    );
}

/// Demonstrates that using ONLY the filename (without namespace prefix) as
/// file_id produces identical sub-keys — the exact vulnerability described
/// in SD-09-CRYPTO-002. This test documents the attack vector.
#[test]
fn test_filename_only_file_id_is_collision_prone() {
    let master = KeyManager::try_new("master-secret", b"salt").expect("master");

    // Both use the same file_id — simulates the vulnerability
    let km1 = master.derive_file_key(b"wal-100.log").expect("derive 1");
    let km2 = master.derive_file_key(b"wal-100.log").expect("derive 2");

    let data = b"sensitive-data";

    // Same sub-key → km2 can decrypt km1's ciphertext (with correct nonce)
    let (enc1, nonce1) = km1.encrypt_auto_nonce(data).expect("enc1");
    let dec2 = km2.decrypt_auto_nonce(&enc1, &nonce1);
    assert!(
        dec2.is_ok(),
        "Same file_id MUST produce same sub-key (vulnerability demonstration)"
    );
    assert_eq!(dec2.unwrap(), data);
}

/// The sub-key derivation must be fully deterministic for the same inputs.
/// This is critical for WAL reopen: the same file_id must produce the
/// same key so existing ciphertext can be decrypted.
#[test]
fn test_key_derivation_deterministic_for_same_input() {
    let master = KeyManager::try_new("master-secret", b"salt").expect("master");

    let km1 = master
        .derive_file_key(b"namespace/wal-42.log")
        .expect("derive 1");
    let km2 = master
        .derive_file_key(b"namespace/wal-42.log")
        .expect("derive 2");

    let data = b"test-payload";
    let (encrypted, nonce) = km1.encrypt_auto_nonce(data).expect("encrypt");

    // km2 must be able to decrypt km1's output (same derived key)
    let decrypted = km2.decrypt_auto_nonce(&encrypted, &nonce).expect("decrypt");
    assert_eq!(
        data.as_slice(),
        decrypted.as_slice(),
        "Same file_id must always derive the same sub-key"
    );
}

/// Edge case: empty file_id must be rejected with defined InvalidInput error.
#[test]
fn test_empty_file_id_fails_validation() {
    let master = KeyManager::try_new("master-secret", b"salt").expect("master");
    let res = master.derive_file_key(b"");
    assert!(res.is_err());
    if let Err(err) = res {
        assert!(matches!(
            err,
            memfuse_core::MemFuseError::InvalidInput { .. }
        ));
    }
}

/// Different master keys with the same file_id must produce different
/// sub-keys (defense in depth).
#[test]
fn test_different_masters_same_file_id_different_subkeys() {
    let master1 = KeyManager::try_new("secret-1", b"salt").expect("master1");
    let master2 = KeyManager::try_new("secret-2", b"salt").expect("master2");

    let file_id = b"shared-wal.log";
    let km1 = master1.derive_file_key(file_id).expect("derive 1");
    let km2 = master2.derive_file_key(file_id).expect("derive 2");

    let data = b"payload";
    let (enc1, nonce1) = km1.encrypt_auto_nonce(data).expect("enc1");

    // Cross-master decryption must fail
    let cross = km2.decrypt_auto_nonce(&enc1, &nonce1);
    assert!(
        cross.is_err(),
        "Different master keys must produce incompatible sub-keys"
    );
}
