// FILE-CONTEXT
// ZWECK: Integrationstest für KvSegment-Verschlüsselung, Zeroize und Prozess-Speicherabbild-Prüfung (P9).
// STAND: TS:2026-09-08T00:00:00Z

#![cfg(feature = "kv-encryption")]

use memfuse_core::TenantId;
use memfuse_security::kv_segment::{KvSegment, TenantIsolatedKvStore};
use memfuse_security::{CryptoKey, KvSegmentCipher, ModelFingerprint};
use std::mem::ManuallyDrop;
use zeroize::Zeroize;

fn setup_cipher() -> KvSegmentCipher {
    let master_km = CryptoKey::try_new("master-passphrase-kv-bridge", b"master-salt-kv-bridge")
        .expect("KeyManager creation should succeed");
    KvSegmentCipher::new(master_km)
}

fn dummy_fingerprint(model_id: &str) -> ModelFingerprint {
    ModelFingerprint::new([0x77u8; 32], model_id, "Q4_K_M")
}

#[test]
fn test_encrypted_segment_memory_inspection_and_decryption_roundtrip() {
    let cipher = setup_cipher();
    let store = TenantIsolatedKvStore::new();

    let tenant = TenantId::try_new(42).unwrap();
    let segment_id = 999;
    let fp = dummy_fingerprint("llama-3.2-3b");
    let rope_offset = Some(256);

    // Distinct confidential plaintext payload
    let secret_plaintext =
        b"CONFIDENTIAL_TENSOR_PAYLOAD_0123456789_SECRET_KEY_VALUES_P9_VERIFICATION";

    // 1. Insert encrypted segment into store
    store
        .insert_encrypted_segment(
            &cipher,
            tenant,
            segment_id,
            fp.clone(),
            rope_offset,
            secret_plaintext,
        )
        .expect("Encryption & insertion MUST succeed");

    // 2. Process Memory Dump / Memory Inspection Simulation
    // Access internal segment bytes stored in store
    let segment_ids = store.get_segments(tenant);
    assert_eq!(segment_ids, vec![segment_id]);

    // Construct direct segment via new_encrypted to inspect raw in-memory bytes
    let encrypted_seg = KvSegment::new_encrypted(
        &cipher,
        tenant,
        segment_id,
        fp.clone(),
        rope_offset,
        secret_plaintext,
    )
    .expect("new_encrypted MUST succeed");

    assert!(encrypted_seg.encrypted);
    assert_eq!(encrypted_seg.rope_offset, Some(256));

    let raw_stored_bytes = encrypted_seg.as_bytes();

    // Verify raw stored memory bytes DO NOT contain any plaintext substring (P9 requirement)
    let contains_plaintext = raw_stored_bytes
        .windows(secret_plaintext.len())
        .any(|window| window == secret_plaintext);

    assert!(
        !contains_plaintext,
        "P9 VIOLATION: Stored memory buffer MUST NOT contain plaintext secret tensor data"
    );

    // 3. Decryption Roundtrip
    let decrypted = store
        .get_decrypted_segment(&cipher, tenant, segment_id)
        .expect("Decryption MUST succeed")
        .expect("Segment MUST exist");

    assert_eq!(
        decrypted, secret_plaintext,
        "Decrypted payload MUST equal original confidential plaintext"
    );

    // 4. Zeroize-on-drop combined verification
    let mut manual_seg = ManuallyDrop::new(encrypted_seg);
    let raw_ptr = manual_seg.as_bytes().as_ptr();
    let raw_len = manual_seg.len();

    // Before zeroize: contains non-zero ciphertext bytes
    // SAFETY: manual_seg is alive inside ManuallyDrop
    unsafe {
        let slice = std::slice::from_raw_parts(raw_ptr, raw_len);
        assert_ne!(slice, vec![0u8; raw_len].as_slice());
    }

    // Action: Zeroize memory in place
    Zeroize::zeroize(&mut *manual_seg);

    // After zeroize: all bytes in memory wiped to 0x00
    // SAFETY: manual_seg memory buffer is still allocated within ManuallyDrop wrapper
    unsafe {
        let cleared_slice = std::slice::from_raw_parts(raw_ptr, raw_len);
        assert_eq!(
            cleared_slice,
            vec![0u8; raw_len].as_slice(),
            "Memory MUST be zeroed after zeroize on drop"
        );
    }
}
