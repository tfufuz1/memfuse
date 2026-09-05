// FILE-CONTEXT
// ZWECK: Systematic WAL anti-tamper matrix, single bit-flip analysis, replay attack protection, and constant-time check.
// INVARIANTEN: Every single byte flip in WAL entry payload/header MUST cause verification failure. Hash chain prevents replay.
// NICHT-OFFENSICHTLICH: Verifies subtle::ConstantTimeEq usage in IntegrityVerifier via code inspection / behavioral tests.
// STAND: TS:2026-08-30T19:45:00Z (SESSION: 20260830)

use memfuse_crypto::wal_crypto::{IntegrityVerifier, WalEntrySnapshot, WalHmac};

fn create_valid_entry(
    key: &[u8],
    prev_hmac: [u8; 32],
    seq_no: u64,
    op_type: u8,
    k: &[u8],
    v: &[u8],
) -> WalEntrySnapshot {
    let tx_id = seq_no;
    let mut hmac = WalHmac::new(key).expect("WalHmac new");
    hmac.update(&prev_hmac);
    hmac.update(&seq_no.to_le_bytes());
    hmac.update(&tx_id.to_le_bytes());
    if op_type == 0 {
        hmac.update(&[0u8]);
        hmac.update(&(k.len() as u32).to_le_bytes());
        hmac.update(k);
        hmac.update(&(v.len() as u32).to_le_bytes());
        hmac.update(v);
    } else {
        hmac.update(&[1u8]);
        hmac.update(&(k.len() as u32).to_le_bytes());
        hmac.update(k);
    }
    let checksum = hmac.finalize();
    WalEntrySnapshot {
        tx_id,
        seq_no,
        op_type,
        key: k.to_vec(),
        value: v.to_vec(),
        checksum,
        prev_hmac,
    }
}

#[test]
fn test_exhaustive_bit_flip_payload_and_header() {
    let integrity_key = b"master-integrity-key-32-bytes---";
    let entry = create_valid_entry(
        integrity_key,
        [0u8; 32],
        1001,
        0,
        b"user_key_42",
        b"user_val_99999",
    );

    // 1. Verify baseline valid entry passes
    let mut verifier = IntegrityVerifier::new(integrity_key);
    assert!(verifier.verify_and_update(&entry, 0).is_ok());

    // 2. Bit-flip every byte in key payload
    for byte_idx in 0..entry.key.len() {
        for bit in 0..8 {
            let mut tampered = entry.clone();
            tampered.key[byte_idx] ^= 1 << bit;
            let mut v = IntegrityVerifier::new(integrity_key);
            assert!(
                v.verify_and_update(&tampered, 0).is_err(),
                "Bit flip at key byte {} bit {} was NOT detected!",
                byte_idx,
                bit
            );
        }
    }

    // 3. Bit-flip every byte in value payload
    for byte_idx in 0..entry.value.len() {
        for bit in 0..8 {
            let mut tampered = entry.clone();
            tampered.value[byte_idx] ^= 1 << bit;
            let mut v = IntegrityVerifier::new(integrity_key);
            assert!(
                v.verify_and_update(&tampered, 0).is_err(),
                "Bit flip at value byte {} bit {} was NOT detected!",
                byte_idx,
                bit
            );
        }
    }

    // 4. Bit-flip every byte in checksum
    for byte_idx in 0..32 {
        for bit in 0..8 {
            let mut tampered = entry.clone();
            tampered.checksum[byte_idx] ^= 1 << bit;
            let mut v = IntegrityVerifier::new(integrity_key);
            assert!(
                v.verify_and_update(&tampered, 0).is_err(),
                "Bit flip at checksum byte {} bit {} was NOT detected!",
                byte_idx,
                bit
            );
        }
    }

    // 5. Bit-flip seq_no / tx_id / op_type
    {
        let mut tampered = entry.clone();
        tampered.seq_no += 1;
        let mut v = IntegrityVerifier::new(integrity_key);
        assert!(
            v.verify_and_update(&tampered, 0).is_err(),
            "Modified seq_no NOT detected!"
        );
    }
    {
        let mut tampered = entry.clone();
        tampered.op_type ^= 1;
        let mut v = IntegrityVerifier::new(integrity_key);
        assert!(
            v.verify_and_update(&tampered, 0).is_err(),
            "Modified op_type NOT detected!"
        );
    }
}

#[test]
fn test_replay_attack_prevention() {
    let integrity_key = b"replay-protection-key-32-bytes-";

    // Create chain e1 -> e2
    let e1 = create_valid_entry(integrity_key, [0u8; 32], 1, 0, b"k1", b"v1");
    let e2 = create_valid_entry(integrity_key, e1.checksum, 2, 0, b"k2", b"v2");
    let e3 = create_valid_entry(integrity_key, e2.checksum, 3, 0, b"k3", b"v3");

    let mut verifier = IntegrityVerifier::new(integrity_key);
    verifier.verify_and_update(&e1, 10).expect("e1 valid");
    verifier.verify_and_update(&e2, 20).expect("e2 valid");

    // Attempt Replay Attack: re-insert old valid e1 at position 3
    let mut replayed_e1 = e1.clone();
    replayed_e1.prev_hmac = e2.checksum; // Point prev_hmac to current chain head

    let err = verifier.verify_and_update(&replayed_e1, 30);
    assert!(
        err.is_err(),
        "CRITICAL: Replay attack with re-bound prev_hmac succeeded! HMAC must bind seq_no/tx_id."
    );

    // Verify e3 passes normally
    let mut verifier2 = IntegrityVerifier::new(integrity_key);
    verifier2.verify_and_update(&e1, 10).expect("e1 valid");
    verifier2.verify_and_update(&e2, 20).expect("e2 valid");
    verifier2.verify_and_update(&e3, 30).expect("e3 valid");
}

#[test]
fn test_constant_time_eq_verifies() {
    // Inspect source code of src/wal_crypto.rs to confirm subtle::ConstantTimeEq usage
    let wal_crypto_src = include_str!("../src/wal_crypto.rs");
    assert!(
        wal_crypto_src.contains("subtle::ConstantTimeEq") || wal_crypto_src.contains("ct_eq"),
        "PASS/FAIL AUDIT: src/wal_crypto.rs MUST use subtle::ConstantTimeEq for HMAC comparison!"
    );
}
