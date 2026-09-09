#![allow(deprecated)]

//! Integration tests for GDPR Art. 17 DeletionProof generation during collection drop.

use memfuse_core::{CollectionId, DistanceMetric, StorageEngine, TenantId, TxId};
use memfuse_crypto::deletion_proof::{DeletionLayer, DeletionProof, DeletionScope};
use memfuse_db::{MemFuse, MemFuseConfig};
use serde_json::json;
use tempfile::TempDir;

async fn setup_db(dim: usize) -> (MemFuse, TempDir) {
    let tmp = TempDir::new().expect("Failed to create temp dir");
    let config = MemFuseConfig {
        dimension: dim,
        max_elements: 1000,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(tmp.path(), config)
        .await
        .expect("Failed to open DB");
    (db, tmp)
}

/// Test 4a: Population, drop, and verification of returned DeletionProof with proof_key.
#[tokio::test]
async fn test_drop_collection_generates_verifiable_deletion_proof() {
    let (db, _tmp) = setup_db(3).await;
    let tenant_id = TenantId::try_new(42).unwrap();
    let proof_key = b"secret-tenant-deletion-proof-key";

    // 1. Create collection and populate with known documents
    let col = db.collection("sensitive_col").await.expect("create col");
    col.insert(
        "doc1",
        &[1.0, 0.0, 0.0],
        Some(json!({"data": "user PII data 1", "text": "secret record 1"})),
    )
    .await
    .expect("insert doc1");

    col.insert(
        "doc2",
        &[0.0, 1.0, 0.0],
        Some(json!({"data": "user PII data 2", "text": "secret record 2"})),
    )
    .await
    .expect("insert doc2");

    col.insert(
        "doc3",
        &[0.0, 0.0, 1.0],
        Some(json!({"data": "user PII data 3", "text": "secret record 3"})),
    )
    .await
    .expect("insert doc3");

    assert_eq!(col.len().await, 3, "Collection should contain 3 documents");

    // 2. Perform physical collection drop with proof key
    let proof = db
        .drop_collection("sensitive_col", tenant_id, proof_key)
        .await
        .expect("drop_collection should succeed and return DeletionProof");

    // 3. Verify DeletionProof properties
    assert_eq!(proof.tenant_id(), tenant_id);
    assert!(
        proof.covered_layers.contains(&DeletionLayer::LsmMemtable),
        "Proof must cover LsmMemtable"
    );
    assert!(
        proof
            .covered_layers
            .contains(&DeletionLayer::SsTableAllLevels),
        "Proof must cover SsTableAllLevels"
    );

    // 4. Verify signature with correct proof key
    let is_valid = proof
        .verify(proof_key)
        .expect("verify() should execute without error");
    assert!(
        is_valid,
        "DeletionProof signature must verify successfully with correct proof_key"
    );

    // 5. Verify signature fails with wrong proof key
    let wrong_key = b"invalid-wrong-deletion-proof-key";
    let is_valid_wrong = proof
        .verify(wrong_key)
        .expect("verify() should execute without error");
    assert!(
        !is_valid_wrong,
        "DeletionProof signature must fail verification with wrong key"
    );
}

/// Test 4b: Negative reconstruction test (GESAMTSPEZIFIKATION_v10.1 §9 DoD):
/// Access prefix via scan_prefix after DeletionProof is issued and verify NO raw data is discoverable.
#[tokio::test]
async fn test_negative_reconstruction_no_data_discoverable_after_drop() {
    let (db, _tmp) = setup_db(3).await;
    let tenant_id = TenantId::try_new(99).unwrap();
    let proof_key = b"negative-reconstruction-proof-key";
    let col_name = "gdpr_purge_col";

    // 1. Create collection with sensitive data
    let col = db.collection(col_name).await.expect("create col");
    col.insert(
        "user_pii_1001",
        &[0.5, 0.5, 0.0],
        Some(json!({"email": "john.doe@example.com", "text": "john's personal history"})),
    )
    .await
    .expect("insert pii 1");

    col.insert(
        "user_pii_1002",
        &[0.2, 0.8, 0.0],
        Some(json!({"email": "jane.smith@example.com", "text": "jane's medical notes"})),
    )
    .await
    .expect("insert pii 2");

    // Confirm keys exist in underlying storage before drop
    let col_data_prefix = format!("__col:{col_name}:");
    let txt_data_prefix = format!("__txt:{col_name}:");

    let col_keys_before = db
        .inner_storage()
        .scan_prefix(col_data_prefix.as_bytes())
        .await
        .expect("scan_prefix before drop");
    assert!(
        !col_keys_before.is_empty(),
        "Raw storage keys must exist prior to deletion"
    );

    // 2. Issue DeletionProof via drop_collection
    let proof = db
        .drop_collection(col_name, tenant_id, proof_key)
        .await
        .expect("drop_collection");

    assert!(
        proof.verify(proof_key).expect("verify"),
        "Proof must be cryptographically valid"
    );

    // 3. NEGATIVE RECONSTRUCTION CHECK: Scan storage prefixes after drop
    let col_keys_after = db
        .inner_storage()
        .scan_prefix(col_data_prefix.as_bytes())
        .await
        .expect("scan_prefix after drop");

    assert!(
        col_keys_after.is_empty(),
        "NEGATIVE RECONSTRUCTION VERIFICATION FAILED: Found {} residual keys under collection data prefix!",
        col_keys_after.len()
    );

    let txt_keys_after = db
        .inner_storage()
        .scan_prefix(txt_data_prefix.as_bytes())
        .await
        .expect("scan_prefix text after drop");

    assert!(
        txt_keys_after.is_empty(),
        "NEGATIVE RECONSTRUCTION VERIFICATION FAILED: Found {} residual keys under text index prefix!",
        txt_keys_after.len()
    );

    // 4. Verify index key is gone
    let col_idx_key = [b"__col_idx:\x00", col_name.as_bytes()].concat();
    let idx_key_val = db
        .inner_storage()
        .get(&col_idx_key)
        .await
        .expect("get col_idx_key");
    assert!(
        idx_key_val.is_none(),
        "Collection index key must be purged from storage"
    );

    // 5. Re-opening or querying collection returns no documents
    let col_reopen = db.collection(col_name).await.expect("reopen col");
    assert_eq!(col_reopen.len().await, 0);
    assert!(
        col_reopen
            .get("user_pii_1001")
            .await
            .expect("get")
            .is_none(),
        "user_pii_1001 must not be retrievable"
    );
}

/// Test 4c: Deterministic key hashing test:
/// Confirms that deleted_keys_hash in DeletionProof is calculated deterministically
/// from the actually deleted keys regardless of initial key ordering.
#[tokio::test]
async fn test_deleted_keys_hash_is_deterministic() {
    let tenant_id = TenantId::try_new(7).unwrap();
    let proof_key = b"deterministic-test-key";

    let col_id = CollectionId::try_new(100).unwrap();
    let scope = DeletionScope::Collection {
        collection_id: col_id,
        tenant_id,
    };

    // Construct raw key lists in different orders
    let keys_order_1 = vec![
        b"__col:test_col:\x00doc_c".to_vec(),
        b"__col:test_col:\x00doc_a".to_vec(),
        b"__col:test_col:\x00doc_b".to_vec(),
        b"__txt:test_col:\x00doc_c".to_vec(),
    ];

    let keys_order_2 = vec![
        b"__txt:test_col:\x00doc_c".to_vec(),
        b"__col:test_col:\x00doc_b".to_vec(),
        b"__col:test_col:\x00doc_a".to_vec(),
        b"__col:test_col:\x00doc_c".to_vec(),
    ];

    let proof_1 = DeletionProof::create(
        scope.clone(),
        keys_order_1,
        TxId::new(50),
        vec![DeletionLayer::LsmMemtable, DeletionLayer::SsTableAllLevels],
        vec![],
        proof_key,
    )
    .expect("create proof 1");

    let proof_2 = DeletionProof::create(
        scope,
        keys_order_2,
        TxId::new(50),
        vec![DeletionLayer::LsmMemtable, DeletionLayer::SsTableAllLevels],
        vec![],
        proof_key,
    )
    .expect("create proof 2");

    assert_eq!(
        proof_1.deleted_keys_hash, proof_2.deleted_keys_hash,
        "deleted_keys_hash must be identical regardless of input key order"
    );
    assert_eq!(
        proof_1.signature, proof_2.signature,
        "HMAC signatures must match when key hashes and parameters are identical"
    );

    assert!(proof_1.verify(proof_key).unwrap());
    assert!(proof_2.verify(proof_key).unwrap());
}
