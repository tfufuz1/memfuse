// ANCHOR:INTEGRATION:CORE-001 STATUS:DONE AGENT:12 DATE:2026-06-21
//! Integration tests for TxBuffer lifecycle.

use memfuse_core::{DocId, IndexOp, TxBuffer, TxId};
use memfuse_core::tx_buffer::start_orphan_reaper;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn test_tx_buffer_full_lifecycle() {
    let buffer = TxBuffer::<String>::new_with_config(4, Duration::from_secs(30));
    let tx = TxId::new(100);

    // 1. Begin
    buffer.begin(tx);
    assert!(buffer.has_tx(tx));

    // 2. Stage operations
    buffer.stage(tx, IndexOp::Insert { doc_id: DocId::new(1), data: "A".to_string() });
    buffer.stage(tx, IndexOp::Insert { doc_id: DocId::new(2), data: "B".to_string() });
    buffer.stage(tx, IndexOp::Delete { doc_id: DocId::new(1), data: None });

    assert_eq!(buffer.len(), 1);
    buffer.validate_pending_ops(tx).expect("Should have pending ops");

    // 3. Drain
    let ops = buffer.drain(tx);
    assert_eq!(ops.len(), 3);
    assert!(!buffer.has_tx(tx));
    assert!(buffer.is_empty());
}

#[tokio::test]
async fn test_tx_buffer_discard() {
    let buffer = TxBuffer::<String>::new();
    let tx = TxId::new(200);

    buffer.stage(tx, IndexOp::Insert { doc_id: DocId::new(1), data: "Trash".to_string() });
    assert!(buffer.has_tx(tx));

    buffer.discard(tx);
    assert!(!buffer.has_tx(tx));
    assert!(buffer.is_empty());
}

#[tokio::test]
async fn test_orphan_reaper_lifecycle() {
    let timeout = Duration::from_millis(50);
    let buffer = Arc::new(TxBuffer::<String>::new_with_config(4, timeout));
    let tx = TxId::new(300);

    buffer.begin(tx);
    buffer.stage(tx, IndexOp::Insert { doc_id: DocId::new(1), data: "Stale".to_string() });

    let _reaper = start_orphan_reaper(buffer.clone(), Duration::from_millis(10));

    assert!(buffer.has_tx(tx));

    // Wait for timeout
    sleep(timeout * 2).await;

    assert!(!buffer.has_tx(tx), "Orphan reaper should have removed expired transaction");
}

#[tokio::test]
async fn test_concurrent_tx_staging() {
    let buffer = Arc::new(TxBuffer::<usize>::new_with_config(16, Duration::from_secs(60)));
    let num_tasks = 20;
    let ops_per_task = 50;

    let mut handles = Vec::new();
    for i in 0..num_tasks {
        let b = buffer.clone();
        handles.push(tokio::spawn(async move {
            let tx = TxId::new(i as u64);
            for j in 0..ops_per_task {
                b.stage(tx, IndexOp::Insert { doc_id: DocId::new(j as u64), data: j });
            }
        }));
    }

    for h in handles {
        h.await.expect("Task failed");
    }

    assert_eq!(buffer.len(), num_tasks);

    for i in 0..num_tasks {
        let tx = TxId::new(i as u64);
        let ops = buffer.drain(tx);
        assert_eq!(ops.len(), ops_per_task);
    }

    assert!(buffer.is_empty());
}
