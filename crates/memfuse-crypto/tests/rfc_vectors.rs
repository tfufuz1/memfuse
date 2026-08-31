// FILE-CONTEXT
// ZWECK: Verification of cryptographic primitives against official RFC and reference test vectors.
// INVARIANTEN: AES-256-GCM-SIV against RFC 8452, HKDF against RFC 5869, HMAC-SHA256 against RFC 4231.
// NICHT-OFFENSICHTLICH: All expected ciphertext/hash values are independently taken from RFC standards, not auto-generated.
// STAND: TS:2026-08-30T19:25:00Z (SESSION: 20260830)

use aes_gcm_siv::{
    aead::{Aead, KeyInit as AeadKeyInit},
    Aes256GcmSiv, Nonce,
};
use hkdf::Hkdf;
use hmac::{digest::KeyInit, Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

fn hex_decode(hex: &str) -> Vec<u8> {
    let clean = hex.replace(" ", "");
    (0..clean.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&clean[i..i + 2], 16).expect("valid hex byte"))
        .collect()
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

#[test]
fn test_rfc_8452_aes_256_gcm_siv_vector_1() {
    // RFC 8452 Appendix C.2: AEAD_AES_256_GCM_SIV Vector 1 (Empty plaintext, Empty AAD)
    let key_bytes = hex_decode("0100000000000000000000000000000000000000000000000000000000000000");
    let nonce_bytes = hex_decode("030000000000000000000000");
    let plaintext = b"";
    let expected_ct_tag = hex_decode("07f5f4169bbf55a8400cd47ea6fd400f");

    let cipher = Aes256GcmSiv::new_from_slice(&key_bytes).expect("key init");
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher.encrypt(nonce, plaintext.as_ref()).expect("encrypt");

    assert_eq!(
        hex_encode(&ciphertext),
        hex_encode(&expected_ct_tag),
        "AES-256-GCM-SIV RFC 8452 Vector 1 mismatch!"
    );

    let decrypted = cipher.decrypt(nonce, ciphertext.as_ref()).expect("decrypt");
    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_rfc_8452_aes_256_gcm_siv_vector_2() {
    // RFC 8452 Appendix C.2: AEAD_AES_256_GCM_SIV Vector 2 (8 bytes plaintext)
    let key_bytes = hex_decode("0100000000000000000000000000000000000000000000000000000000000000");
    let nonce_bytes = hex_decode("030000000000000000000000");
    let plaintext = hex_decode("0100000000000000");
    let expected_ct_tag = hex_decode("c2ef328e5c71c83b843122130f7364b761e0b97427e3df28");

    let cipher = Aes256GcmSiv::new_from_slice(&key_bytes).expect("key init");
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher.encrypt(nonce, plaintext.as_ref()).expect("encrypt");

    assert_eq!(
        hex_encode(&ciphertext),
        hex_encode(&expected_ct_tag),
        "AES-256-GCM-SIV RFC 8452 Vector 2 mismatch!"
    );

    let decrypted = cipher.decrypt(nonce, ciphertext.as_ref()).expect("decrypt");
    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_rfc_5869_hkdf_sha256_case_1() {
    // RFC 5869 Test Case 1
    let ikm = hex_decode("0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b");
    let salt = hex_decode("000102030405060708090a0b0c");
    let info = hex_decode("f0f1f2f3f4f5f6f7f8f9");
    let okm_len = 42;
    let expected_okm = hex_decode(
        "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865",
    );

    let hk = Hkdf::<Sha256>::new(Some(&salt), &ikm);
    let mut okm = vec![0u8; okm_len];
    hk.expand(&info, &mut okm).expect("hkdf expand");

    assert_eq!(
        hex_encode(&okm),
        hex_encode(&expected_okm),
        "HKDF-SHA256 RFC 5869 Test Case 1 mismatch!"
    );
}

#[test]
fn test_rfc_5869_hkdf_sha256_case_2() {
    // RFC 5869 Test Case 2
    let ikm = hex_decode("000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f404142434445464748494a4b4c4d4e4f");
    let salt = hex_decode("606162636465666768696a6b6c6d6e6f707172737475767778797a7b7c7d7e7f808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9fa0a1a2a3a4a5a6a7a8a9aaabacadaeaf");
    let info = hex_decode("b0b1b2b3b4b5b6b7b8b9babbbcbdbebfc0c1c2c3c4c5c6c7c8c9cacbcccdcecfd0d1d2d3d4d5d6d7d8d9dadbdcdddedfe0e1e2e3e4e5e6e7e8e9eaebecedeeeff0f1f2f3f4f5f6f7f8f9fafbfcfdfeff");
    let okm_len = 82;
    let expected_okm = hex_decode("b11e398dc80327a1c8e7f78c596a49344f012eda2d4efad8a050cc4c19afa97c59045a99cac7827271cb41c65e590e09da3275600c2f09b8367793a9aca3db71cc30c58179ec3e87c14c01d5c1f3434f1d87");

    let hk = Hkdf::<Sha256>::new(Some(&salt), &ikm);
    let mut okm = vec![0u8; okm_len];
    hk.expand(&info, &mut okm).expect("hkdf expand");

    assert_eq!(
        hex_encode(&okm),
        hex_encode(&expected_okm),
        "HKDF-SHA256 RFC 5869 Test Case 2 mismatch!"
    );
}

#[test]
fn test_rfc_4231_hmac_sha256_case_1() {
    // RFC 4231 Test Case 1
    let key = hex_decode("0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b");
    let data = b"Hi There";
    let expected_digest =
        hex_decode("b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7");

    let mut mac = <HmacSha256 as KeyInit>::new_from_slice(&key).expect("hmac key init");
    Mac::update(&mut mac, data);
    let result = Mac::finalize(mac).into_bytes();

    assert_eq!(
        hex_encode(&result),
        hex_encode(&expected_digest),
        "HMAC-SHA256 RFC 4231 Case 1 mismatch!"
    );
}

#[test]
fn test_rfc_4231_hmac_sha256_case_2() {
    // RFC 4231 Test Case 2 ("Jefe")
    let key = b"Jefe";
    let data = b"what do ya want for nothing?";
    let expected_digest =
        hex_decode("5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843");

    let mut mac = <HmacSha256 as KeyInit>::new_from_slice(key).expect("hmac key init");
    Mac::update(&mut mac, data);
    let result = Mac::finalize(mac).into_bytes();

    assert_eq!(
        hex_encode(&result),
        hex_encode(&expected_digest),
        "HMAC-SHA256 RFC 4231 Case 2 mismatch!"
    );
}
