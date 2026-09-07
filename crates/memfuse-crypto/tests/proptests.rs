// FILE-CONTEXT
// ZWECK: Property-based testing suite for memfuse-crypto using proptest.
// INVARIANTEN: Roundtrip invariant: decrypt(encrypt(pt)) == pt. Authenticity invariant: 1-bit ciphertext flip must fail decryption.
// STAND: TS:2026-08-31T21:13:05Z (SESSION: 8427f167)

use memfuse_core::TenantId;
use memfuse_crypto::wal_crypto::{EncryptedWal, IntegrityVerifier, WalEntrySnapshot, WalHmac};
use memfuse_crypto::{CryptoKey, KvSegmentCipher, ModelFingerprint};
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
    fn prop_kv_segment_cipher_roundtrip(
        tenant_val in 1u64..10_000u64,
        model_id in "[a-zA-Z0-9_-]{1,32}",
        quant in "(Q4_K_M|Q8_0|F16|BF16)",
        plaintext in proptest::collection::vec(any::<u8>(), 0..5_000),
    ) {
        let km = CryptoKey::try_new("kv-proptest-passphrase", b"kv-proptest-salt").unwrap();
        let cipher = KvSegmentCipher::new(km);

        let tenant_id = TenantId::try_new(tenant_val).unwrap();
        let fp = ModelFingerprint::new([0x33u8; 32], model_id, quant);

        let encrypted = cipher.encrypt(tenant_id, fp.clone(), &plaintext).unwrap();
        prop_assert_eq!(encrypted.tenant_id, tenant_id);
        prop_assert_eq!(&encrypted.model_fingerprint, &fp);

        let decrypted = cipher.decrypt(&encrypted).unwrap();
        prop_assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn prop_kv_segment_cipher_mismatch_fails_decrypt(
        tenant_a_val in 1u64..5_000u64,
        tenant_b_offset in 1u64..5_000u64,
        plaintext in proptest::collection::vec(any::<u8>(), 0..1_000),
    ) {
        let km = CryptoKey::try_new("kv-mismatch-passphrase", b"kv-mismatch-salt").unwrap();
        let cipher = KvSegmentCipher::new(km);

        let tenant_a = TenantId::try_new(tenant_a_val).unwrap();
        let tenant_b = TenantId::try_new(tenant_a_val + tenant_b_offset).unwrap();
        let fp_a = ModelFingerprint::new([0x11u8; 32], "llama-3.2", "Q4_K_M");

        let mut encrypted = cipher.encrypt(tenant_a, fp_a, &plaintext).unwrap();

        // Tampering tenant_id MUST cause decryption authentication failure (not silent corrupt data)
        encrypted.tenant_id = tenant_b;
        prop_assert!(cipher.decrypt(&encrypted).is_err());
    }

    #[test]
    fn prop_kv_segment_cipher_freshness_nonce_and_ciphertext(
        tenant_val in 1u64..10_000u64,
        plaintext in proptest::collection::vec(any::<u8>(), 0..1_000),
    ) {
        let km = CryptoKey::try_new("kv-freshness-passphrase", b"kv-freshness-salt").unwrap();
        let cipher = KvSegmentCipher::new(km);

        let tenant_id = TenantId::try_new(tenant_val).unwrap();
        let fp = ModelFingerprint::new([0x77u8; 32], "model-freshness", "Q8_0");

        let enc1 = cipher.encrypt(tenant_id, fp.clone(), &plaintext).unwrap();
        let enc2 = cipher.encrypt(tenant_id, fp, &plaintext).unwrap();

        prop_assert_ne!(enc1.nonce, enc2.nonce, "Two encryptions of same plaintext MUST have distinct nonces");
        prop_assert_ne!(&enc1.ciphertext, &enc2.ciphertext, "Two encryptions of same plaintext MUST have distinct ciphertexts");
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
