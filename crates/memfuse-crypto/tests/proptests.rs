// FILE-CONTEXT
// ZWECK: Property-based testing suite for memfuse-crypto using proptest.
// INVARIANTEN: Roundtrip invariant: decrypt(encrypt(pt)) == pt. Authenticity invariant: 1-bit ciphertext flip must fail decryption.
// STAND: TS:2026-08-30T19:50:00Z (SESSION: 20260830)

use memfuse_crypto::CryptoKey;
use proptest::prelude::*;

proptest! {
    #[test]
    fn prop_encrypt_decrypt_roundtrip(plaintext in proptest::collection::vec(any::<u8>(), 0..10_000)) {
        let km = CryptoKey::try_new("proptest-passphrase", b"proptest-salt-123456").unwrap();
        let (ciphertext, nonce) = km.encrypt_auto_nonce(&plaintext).unwrap();
        let decrypted = km.decrypt_auto_nonce(&ciphertext, &nonce).unwrap();
        prop_assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn prop_ciphertext_bit_flip_authenticity_failure(
        plaintext in proptest::collection::vec(any::<u8>(), 0..2_000),
        byte_offset in 0..2_000usize,
        bit_offset in 0..8u8,
    ) {
        let km = CryptoKey::try_new("proptest-passphrase", b"proptest-salt-123456").unwrap();
        let (mut ciphertext, nonce) = km.encrypt_auto_nonce(&plaintext).unwrap();

        if !ciphertext.is_empty() {
            let actual_idx = byte_offset % ciphertext.len();
            ciphertext[actual_idx] ^= 1 << (bit_offset % 8);

            let res = km.decrypt_auto_nonce(&ciphertext, &nonce);
            prop_assert!(res.is_err(), "Decryption of corrupted ciphertext with 1-bit flip MUST fail");
        }
    }
}
