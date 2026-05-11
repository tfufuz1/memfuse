use memfuse_core::{DocId, TxId};

#[test]
fn test_doc_id_from_key_consistency() {
    let key = "test_key";
    let id1 = DocId::from_key(key);
    let id2 = DocId::from_key(key);
    assert_eq!(id1, id2);
}

#[test]
fn test_tx_id_new() {
    let tx = TxId::new(42);
    assert_eq!(tx.inner(), 42);
}
