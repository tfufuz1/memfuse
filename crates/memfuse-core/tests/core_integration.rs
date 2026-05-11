use memfuse_core::{DocId, TxId};

// ANCHOR:INTEGRATION:CORE-001 — Core Types Integration Test
// AGENT:12 DATE:2026-05-09 STATUS:DONE
#[test]
fn test_core_types_serialization() {
    let doc_id = DocId::new(42);
    assert_eq!(doc_id.inner(), 42);

    let tx_id = TxId::new(100);
    assert_eq!(tx_id.inner(), 100);
}
