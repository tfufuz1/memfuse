// FILE-CONTEXT
// ZWECK: Key separation and cryptographic edge cases test suite for memfuse-crypto.
// INVARIANTEN: HMAC integrity key and encryption key derived from same passphrase/salt must be distinct.
// NICHT-OFFENSICHTLICH: Handles extreme passphrase lengths, unicode, truncated ciphertexts, and 100MB payloads without panics.
// STAND: TS:2026-08-31T21:13:05Z (SESSION: 8427f167)

use memfuse_crypto::wal_crypto::EncryptedWal;
use memfuse_crypto::CryptoKey;

#[test]
fn test_key_separation_encryption_vs_hmac() {
    let passphrase = "master-passphrase-for-key-separation";
    let salt = b"salt-domain-sep-12345678";
    let km = CryptoKey::try_new(passphrase, salt).expect("CryptoKey try_new");

    let enc_key_bytes = km.inspect_key_bytes_for_test();
    let hmac_key_bytes = km.integrity_key().expect("integrity_key derivation");

    assert_ne!(
        enc_key_bytes, &hmac_key_bytes,
        "CRITICAL Cryptographic Failure: Encryption Key and HMAC Integrity Key MUST be distinct!"
    );
}

#[test]
fn test_passphrase_length_variations() {
    let salt = b"standard-salt";

    // Single character passphrase
    let km_short = CryptoKey::try_new("a", salt).expect("short passphrase");
    let (ct_short, nonce_short) = km_short
        .encrypt_auto_nonce(b"hello")
        .expect("encrypt short");
    let dec_short = km_short
        .decrypt_auto_nonce(&ct_short, &nonce_short)
        .expect("decrypt short");
    assert_eq!(dec_short, b"hello");

    // Very long passphrase (> 1000 chars)
    let long_passphrase = "a".repeat(2000);
    let km_long = CryptoKey::try_new(&long_passphrase, salt).expect("long passphrase");
    let (ct_long, nonce_long) = km_long.encrypt_auto_nonce(b"hello").expect("encrypt long");
    let dec_long = km_long
        .decrypt_auto_nonce(&ct_long, &nonce_long)
        .expect("decrypt long");
    assert_eq!(dec_long, b"hello");

    // Unicode passphrase
    let unicode_pass = "🔒SicherheitsSchlüssel🔑-Passphrase-üöä-😀";
    let km_uni = CryptoKey::try_new(unicode_pass, salt).expect("unicode passphrase");
    let (ct_uni, nonce_uni) = km_uni
        .encrypt_auto_nonce(b"hello unicode")
        .expect("encrypt unicode");
    let dec_uni = km_uni
        .decrypt_auto_nonce(&ct_uni, &nonce_uni)
        .expect("decrypt unicode");
    assert_eq!(dec_uni, b"hello unicode");
}

#[test]
fn test_decryption_wrong_key_fails() {
    let km1 = CryptoKey::try_new("correct-passphrase", b"salt").expect("km1");
    let km2 = CryptoKey::try_new("wrong-passphrase", b"salt").expect("km2");

    let (ct, nonce) = km1.encrypt_auto_nonce(b"sensitive data").expect("encrypt");
    let res = km2.decrypt_auto_nonce(&ct, &nonce);
    assert!(
        res.is_err(),
        "Decryption with wrong key MUST fail gracefully without panic"
    );
}

#[test]
fn test_decryption_truncated_ciphertext_fails() {
    let km = CryptoKey::try_new("passphrase", b"salt").expect("km");
    let (ct, nonce) = km
        .encrypt_auto_nonce(b"sensitive data payload")
        .expect("encrypt");

    // Truncate tag (last 16 bytes of AES-GCM-SIV output)
    let truncated_ct = &ct[..ct.len() - 8];
    let res = km.decrypt_auto_nonce(truncated_ct, &nonce);
    assert!(res.is_err(), "Decryption of truncated ciphertext MUST fail");
}

#[test]
fn test_zero_byte_plaintext() {
    let km = CryptoKey::try_new("passphrase", b"salt").expect("km");
    let empty_payload = b"";
    let (ct, nonce) = km.encrypt_auto_nonce(empty_payload).expect("encrypt empty");
    assert_eq!(
        ct.len(),
        16,
        "AES-GCM-SIV tag size for 0-byte plaintext is 16 bytes"
    );

    let decrypted = km.decrypt_auto_nonce(&ct, &nonce).expect("decrypt empty");
    assert_eq!(decrypted, empty_payload);
}

#[test]
fn test_large_100mb_payload_roundtrip() {
    let km = CryptoKey::try_new("passphrase-100mb", b"salt-100mb-123456").expect("km");
    let payload_size = 100 * 1024 * 1024; // 100 MB
    let mut large_payload = vec![0u8; payload_size];
    for (i, byte) in large_payload.iter_mut().enumerate() {
        *byte = (i % 251) as u8;
    }

    let (ct, nonce) = km
        .encrypt_auto_nonce(&large_payload)
        .expect("encrypt 100MB");
    assert_eq!(ct.len(), payload_size + 16);

    let decrypted = km.decrypt_auto_nonce(&ct, &nonce).expect("decrypt 100MB");
    assert_eq!(decrypted.len(), payload_size);
    assert_eq!(decrypted, large_payload);
}

#[test]
fn test_passphrase_reuse_behavior() {
    let pass = "reused-password";
    let salt = b"same-salt";

    let km1 = CryptoKey::try_new(pass, salt).expect("km1");
    let km2 = CryptoKey::try_new(pass, salt).expect("km2");

    // Key bytes are deterministic for same passphrase and salt
    assert_eq!(
        km1.inspect_key_bytes_for_test(),
        km2.inspect_key_bytes_for_test()
    );

    // However, nonces generated are non-deterministic (OsRng)
    let (ct1, nonce1) = km1.encrypt_auto_nonce(b"data").expect("ct1");
    let (ct2, nonce2) = km2.encrypt_auto_nonce(b"data").expect("ct2");

    assert_ne!(
        nonce1, nonce2,
        "Nonces MUST be non-deterministic across calls/instances"
    );
    assert_ne!(ct1, ct2, "Ciphertexts MUST differ due to random nonces");
}

#[test]
fn test_unicode_file_id_derivation() {
    let km = CryptoKey::try_new("passphrase", b"salt").expect("km");
    let unicode_file_id = "pfad/zur/datenbank_🔒_001.sst";
    let sub_km = km
        .derive_file_key(unicode_file_id.as_bytes())
        .expect("derive unicode file_id");

    let (ct, nonce) = sub_km.encrypt_auto_nonce(b"payload").expect("encrypt");
    let decrypted = sub_km.decrypt_auto_nonce(&ct, &nonce).expect("decrypt");
    assert_eq!(decrypted, b"payload");
}

#[test]
fn test_extremely_large_passphrase_100k_chars() {
    let large_passphrase = "x".repeat(100_000);
    let salt = b"salt-for-100k-passphrase";
    let km = CryptoKey::try_new(&large_passphrase, salt).expect("100k passphrase try_new");

    let (ct, nonce) = km
        .encrypt_auto_nonce(b"100k pass payload")
        .expect("encrypt");
    let decrypted = km.decrypt_auto_nonce(&ct, &nonce).expect("decrypt");
    assert_eq!(decrypted, b"100k pass payload");
}

#[test]
fn test_encrypted_wal_zero_byte_chunk() {
    let km = CryptoKey::try_new("wal-pass", b"wal-salt").expect("km");
    let wal = EncryptedWal::new(km, b"wal_0.log").expect("wal");

    let encrypted = wal.encrypt_chunk(b"").expect("encrypt 0-byte chunk");
    assert_eq!(encrypted.len(), 12 + 16);

    let decrypted = wal.decrypt_chunk(&encrypted).expect("decrypt 0-byte chunk");
    assert_eq!(decrypted, b"");
}
