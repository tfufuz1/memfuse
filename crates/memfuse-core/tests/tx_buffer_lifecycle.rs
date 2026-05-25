// AGENT:12 DATE:2026-05-25 STATUS:READY
// ANCHOR:INTEGRATION:CORE-TXBUF-001 — TxBuffer lifecycle and orphan reaper.

use memfuse_core::{DocId, IndexOp, TxBuffer, TxId};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn test_tx_buffer_lifecycle_full() {
    let buffer = Arc::new(TxBuffer::<String>::new_with_config(
        4, // Small number of shards for testing
        Duration::from_millis(100),
    ));

    let tx1 = TxId::new(1);
    let tx2 = TxId::new(2);

    // 1. Begin and Stage
    buffer.begin(tx1);
    buffer.stage(tx1, IndexOp::Insert { doc_id: DocId::new(1), data: "data1".to_string() });

    buffer.begin(tx2);
    buffer.stage(tx2, IndexOp::Insert { doc_id: DocId::new(2), data: "data2".to_string() });

    assert!(buffer.has_tx(tx1));
    assert!(buffer.has_tx(tx2));
    assert_eq!(buffer.len(), 2);

    // 2. Drain tx1
    let ops1 = buffer.drain(tx1);
    assert_eq!(ops1.len(), 1);
    assert!(!buffer.has_tx(tx1));
    assert_eq!(buffer.len(), 1);

    // 3. Discard tx2
    buffer.discard(tx2);
    assert!(!buffer.has_tx(tx2));
    assert!(buffer.is_empty());
}

#[tokio::test]
async fn test_tx_buffer_orphan_reaper_integration() {
    let buffer = Arc::new(TxBuffer::<String>::new_with_config(
        4,
        Duration::from_millis(50),
    ));

    // Spawn orphan reaper
    let _reaper = memfuse_core::tx_buffer::start_orphan_reaper(buffer.clone(), Duration::from_millis(10));

    let tx = TxId::new(42);
    buffer.begin(tx);
    buffer.stage(tx, IndexOp::Insert { doc_id: DocId::new(1), data: "orphan".to_string() });

    assert!(buffer.has_tx(tx));

    // Wait for reaper to kick in
    sleep(Duration::from_millis(150)).await;

    assert!(!buffer.has_tx(tx), "Orphan reaper should have removed expired transaction");
    assert!(buffer.is_empty());
}

#[tokio::test]
async fn test_tx_buffer_concurrent_sharding() {
    let buffer = Arc::new(TxBuffer::<usize>::new_with_config(
        16,
        Duration::from_secs(60),
    ));

    let num_tx = 50;
    let ops_per_tx = 20;
    let mut handles = Vec::new();

    for i in 0..num_tx {
        let b = buffer.clone();
        handles.push(tokio::spawn(async move {
            let tx = TxId::new(i as u64);
            b.begin(tx);
            for j in 0..ops_per_tx {
                b.stage(tx, IndexOp::Insert { doc_id: DocId::new(j as u64), data: j });
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    assert_eq!(buffer.len(), num_tx);

    for i in 0..num_tx {
        let ops = buffer.drain(TxId::new(i as u64));
        assert_eq!(ops.len(), ops_per_tx);
    }

    assert!(buffer.is_empty());
}
