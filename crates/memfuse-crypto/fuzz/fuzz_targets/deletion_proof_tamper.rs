#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use memfuse_core::{DocId, TenantId, TxId};
use memfuse_security::deletion_proof::{
    DeletionLayer, DeletionProof, DeletionScope, ExcludedScope, LayerCleanupProof,
};

#[derive(Arbitrary, Debug)]
pub struct TamperMutation {
    pub offset: usize,
    pub bit_mask: u8,
}

#[derive(Arbitrary, Debug)]
pub struct DeletionProofTamperInput {
    pub proof_key: Vec<u8>,
    pub mutations: Vec<TamperMutation>,
    pub raw_bytes: Vec<u8>,
}

fuzz_target!(|input: DeletionProofTamperInput| {
    let proof_key = if input.proof_key.is_empty() {
        b"fuzz_default_proof_key_32bytes!!".to_vec()
    } else {
        input.proof_key
    };

    // 1. Fuzz arbitrary raw bytes deserialization directly
    if let Ok(proof) = bincode::deserialize::<DeletionProof>(&input.raw_bytes) {
        let _ = proof.verify(&proof_key);
        let _ = proof.export_for_audit();
    }

    // 2. Construct valid proof, apply mutations, and verify
    let tenant_id = match TenantId::try_new(1) {
        Ok(t) => t,
        Err(_) => return,
    };
    let scope = DeletionScope::Document {
        doc_id: DocId::new(500),
        tenant_id,
    };
    let layer_proof = match LayerCleanupProof::new_after_verified_empty(DeletionLayer::LsmMemtable, 0) {
        Ok(p) => p,
        Err(_) => return,
    };

    let valid_proof = match DeletionProof::create(
        scope,
        vec![b"key_a".to_vec(), b"key_b".to_vec()],
        TxId::new(42),
        vec![layer_proof],
        vec![ExcludedScope::LlmParameterMemory],
        &proof_key,
    ) {
        Ok(p) => p,
        Err(_) => return,
    };

    if let Ok(mut serialized) = bincode::serialize(&valid_proof) {
        if !serialized.is_empty() {
            for m in input.mutations.iter().take(10) {
                let idx = m.offset % serialized.len();
                serialized[idx] ^= m.bit_mask;
            }

            match bincode::deserialize::<DeletionProof>(&serialized) {
                Ok(tampered_proof) => {
                    // Tampered proof must never panic during verify
                    let is_valid = tampered_proof.verify(&proof_key).unwrap_or(false);
                    let _ = is_valid;
                }
                Err(_) => {
                    // Deserialization failure is expected for corrupted bytes
                }
            }
        }
    }
});
