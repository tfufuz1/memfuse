// FILE-CONTEXT
// ZWECK: Property-based testing suite for memfuse-crypto using proptest.
// INVARIANTEN: Roundtrip invariant: decrypt(encrypt(pt)) == pt. Authenticity invariant: 1-bit ciphertext flip must fail decryption.
// STAND: TS:2026-08-31T21:13:05Z (SESSION: 8427f167)

use memfuse_crypto::wal_crypto::{EncryptedWal, IntegrityVerifier, WalEntrySnapshot, WalHmac};
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

    #[test]
    fn prop_encrypted_wal_roundtrip(
        payload in proptest::collection::vec(any::<u8>(), 0..5_000),
        file_id in "[a-zA-Z0-9_-]{1,64}",
    ) {
        let km = CryptoKey::try_new("wal-proptest-passphrase", b"wal-proptest-salt").unwrap();
        let wal = EncryptedWal::new(km, file_id.as_bytes()).unwrap();
        let encrypted = wal.encrypt_chunk(&payload).unwrap();
        let decrypted = wal.decrypt_chunk(&encrypted).unwrap();
        prop_assert_eq!(decrypted, payload);
    }

    #[test]
    fn prop_integrity_verifier_v3_valid_and_tampered(
        seq_no in 1u64..100_000u64,
        key_bytes in proptest::collection::vec(any::<u8>(), 1..200),
        val_bytes in proptest::collection::vec(any::<u8>(), 0..500),
    ) {
        let integrity_key = b"proptest-integrity-key-32-bytes";
        let tx_id = seq_no;

        let mut hmac = WalHmac::new(integrity_key).unwrap();
        hmac.update(&[0u8; 32]);
        hmac.update(&seq_no.to_le_bytes());
        hmac.update(&tx_id.to_le_bytes());
        hmac.update(&[0u8]); // Put
        hmac.update(&(key_bytes.len() as u32).to_le_bytes());
        hmac.update(&key_bytes);
        hmac.update(&(val_bytes.len() as u32).to_le_bytes());
        hmac.update(&val_bytes);
        let checksum = hmac.finalize();

        let valid_entry = WalEntrySnapshot {
            tx_id,
            seq_no,
            op_type: 0,
            key: key_bytes.clone(),
            value: val_bytes.clone(),
            checksum,
            prev_hmac: [0u8; 32],
        };

        let mut verifier = IntegrityVerifier::new(integrity_key);
        prop_assert!(verifier.verify_and_update(&valid_entry, 0).is_ok());

        // Tamper key
        let mut tampered_entry = valid_entry;
        tampered_entry.key[0] ^= 0xFF;
        let mut verifier2 = IntegrityVerifier::new(integrity_key);
        prop_assert!(verifier2.verify_and_update(&tampered_entry, 0).is_err());
    }
}
