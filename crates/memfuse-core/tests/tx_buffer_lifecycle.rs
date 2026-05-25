use memfuse_core::{DocId, IndexOp, TxBuffer, TxId};
use std::sync::Arc;
use std::time::Duration;
#[tokio::test]
async fn test_tx_buffer_lifecycle() {
    let buffer = Arc::new(TxBuffer::<String>::new_with_config(
        4,
        Duration::from_secs(1),
    ));
    let tx = TxId::new(1);
    buffer.begin(tx);
    buffer.stage(
        tx,
        IndexOp::Insert {
            doc_id: DocId::new(1),
            data: "data".to_string(),
        },
    );
    assert!(buffer.has_tx(tx));
    let _ = buffer.drain(tx);
    assert!(!buffer.has_tx(tx));
}
