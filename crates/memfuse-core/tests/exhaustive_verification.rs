use memfuse_core::{
    error::MemFuseError,
    ipc::{jsonrpc::*, root_as_search_response},
    tx_buffer::{IndexOp, TxBuffer},
    types::*,
};
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn test_txid_boundary_and_range_isolation() {
    // ADR-028: System vs Collection transaction ranges
    let col_tx = TxId::new(TxId::MAX_COLLECTION_SEQUENCE);
    let invalid_gap_tx = TxId::new(TxId::MAX_COLLECTION_SEQUENCE + 1);
    let system_tx = TxId::new(TxId::INTERNAL_BASE);

    assert!(col_tx.is_valid_origin());
    assert!(!invalid_gap_tx.is_valid_origin());
    assert!(system_tx.is_valid_origin());

    // u64::MAX boundary
    let max_tx = TxId::new(u64::MAX);
    assert!(max_tx.is_valid_origin());
    assert_eq!(max_tx.inner(), u64::MAX);
}

#[tokio::test]
async fn test_blake3_doc_id_collision_math() {
    // Independent theoretical collision probability calculation for 64-bit truncated BLAKE3 hash
    // Birthday problem: P(collision) ≈ 1 - exp(-n^2 / (2 * 2^64))
    // For n = 1,000,000 items: n^2 / (2 * 2^64) = 1e12 / (2 * 1.844e19) ≈ 2.71e-8
    let n: f64 = 1_000_000.0;
    let two_pow_64: f64 = 18_446_744_073_709_551_615.0;
    let prob_collision = 1.0 - (- (n * n) / (2.0 * two_pow_64)).exp();

    assert!(prob_collision < 1e-7, "1M items collision probability must be < 0.00001%");

    // Practical verification
    let id1 = DocId::from_key("document_key_alpha").unwrap();
    let id2 = DocId::from_key("document_key_beta").unwrap();
    assert_ne!(id1, id2);

    // Empty key error check
    assert!(DocId::from_key("").is_err());
}

#[tokio::test]
async fn test_memfuse_error_variant_construction_coverage() {
    let errors: Vec<MemFuseError> = vec![
        MemFuseError::NotFound("item".into()),
        MemFuseError::InvalidInput("input".into()),
        MemFuseError::wal_corruption(100, "bad wal"),
        MemFuseError::checksum_mismatch("file.sst", 1),
        MemFuseError::MemoryBudgetExceeded { used_mb: 100, limit_mb: 50 },
        MemFuseError::TransactionTimeout { tx_id: 42, elapsed_ms: 1000 },
        MemFuseError::HnswConnectivityDegraded { deleted_ratio: 0.5 },
        MemFuseError::PolicyViolation("policy error".into()),
        MemFuseError::Internal("internal error".into()),
    ];

    for err in errors {
        let msg = err.to_string();
        assert!(!msg.is_empty());
    }
}

#[tokio::test]
async fn test_ipc_jsonrpc_roundtrip_and_corruption() {
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        method: "search".to_string(),
        params: serde_json::json!({"query": "rust", "k": 10}),
        id: Some(serde_json::json!(1)),
    };

    let serialized = serde_json::to_string(&req).unwrap();
    let deserialized: JsonRpcRequest = serde_json::from_str(&serialized).unwrap();
    assert_eq!(req.jsonrpc, deserialized.jsonrpc);
    assert_eq!(req.method, deserialized.method);
    assert_eq!(req.id, deserialized.id);

    // Corrupted payload
    let corrupted = "{ \"jsonrpc\": \"2.0\", \"method\": ";
    assert!(serde_json::from_str::<JsonRpcRequest>(corrupted).is_err());
}

#[tokio::test]
async fn test_ipc_flatbuffers_corruption_handling() {
    // Truncated FlatBuffers payload
    let garbage = vec![0x12, 0x34, 0x56, 0x78];
    assert!(root_as_search_response(&garbage).is_err());
}

#[tokio::test]
async fn test_concurrent_tx_buffer_stress() {
    let buffer = Arc::new(TxBuffer::<String>::new_with_config(16, Duration::from_secs(1)));
    let mut tasks = vec![];

    for i in 0..50 {
        let buf = buffer.clone();
        tasks.push(tokio::spawn(async move {
            let tx = TxId::new(100 + i);
            buf.begin(tx);
            buf.stage(tx, IndexOp::Insert { doc_id: DocId::new(i), data: format!("data_{}", i) }).unwrap();

            if i % 2 == 0 {
                let ops = buf.drain(tx);
                assert_eq!(ops.len(), 1);
            } else {
                buf.discard(tx);
            }
        }));
    }

    for t in tasks {
        t.await.unwrap();
    }
}
