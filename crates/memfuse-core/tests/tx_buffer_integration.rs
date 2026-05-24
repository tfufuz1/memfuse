use memfuse_core::{TxBuffer, TxId, DocId, IndexOp};
use std::time::Duration;

#[tokio::test]
async fn test_tx_buffer_staging_and_drain() {
    let buffer: TxBuffer<String> = TxBuffer::new_with_config(4, Duration::from_secs(60));
    let tx1 = TxId::new(1);
    let doc1 = DocId::from_key("doc1").unwrap();
    let doc2 = DocId::from_key("doc2").unwrap();

    buffer.stage(tx1, IndexOp::Insert { doc_id: doc1, data: "data1".to_string() });
    buffer.stage(tx1, IndexOp::Insert { doc_id: doc2, data: "data2".to_string() });

    // Verify nothing leaked before drain
    assert_eq!(buffer.len(), 1);

    let ops = buffer.drain(tx1);
    assert_eq!(ops.len(), 2);

    match &ops[0] {
        IndexOp::Insert { doc_id, data } => {
            assert_eq!(*doc_id, doc1);
            assert_eq!(data, "data1");
        }
        _ => panic!("Expected Insert op"),
    }
}

#[tokio::test]
async fn test_doc_id_consistency() {
    let key = "some-complex-key-123";
    let id1 = DocId::from_key(key).unwrap();
    let id2 = DocId::from_key(key).unwrap();

    assert_eq!(id1, id2, "DocId must be deterministic for same key");
    assert_ne!(id1, DocId::from_key("other").unwrap());
}

#[tokio::test]
async fn test_tx_buffer_discard() {
    let buffer: TxBuffer<String> = TxBuffer::new();
    let tx1 = TxId::new(1);

    buffer.stage(tx1, IndexOp::Insert { doc_id: DocId::new(1), data: "lost".to_string() });
    buffer.discard(tx1);

    let ops = buffer.drain(tx1);
    assert!(ops.is_empty(), "Discarded tx should have no ops");
}
