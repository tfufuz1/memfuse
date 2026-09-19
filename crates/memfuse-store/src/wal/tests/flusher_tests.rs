use super::*;
use crate::wal::{PreparedBatch, WalConfig, WalEntry, WalOp};
use memfuse_core::TxId;
use tempfile::tempdir;

#[tokio::test]
async fn test_wal_flusher_actor_coalescing() -> Result<()> {
    let dir = tempdir()?;
    let wal_path = dir.path().join("test_flusher.wal");

    let wal = Arc::new(Wal::open(&wal_path).await?);

    let num_tasks = 10;
    let mut handles = Vec::new();

    for i in 0..num_tasks {
        let wal_clone = Arc::clone(&wal);
        handles.push(tokio::spawn(async move {
            let op = WalOp::Put {
                tx_id: TxId::new(i + 1),
                key: format!("flusher_k_{i}").into_bytes(),
                value: format!("flusher_v_{i}").into_bytes(),
            };
            let (batch, _) = wal_clone.prepare_batch(vec![(op, i + 1)]).await?;
            wal_clone.append_batch(batch).await
        }));
    }

    for h in handles {
        h.await
            .map_err(|e| MemFuseError::Storage(e.to_string()))??;
    }

    let replayed = wal.replay().await?;
    assert_eq!(replayed.len(), num_tasks as usize);

    for (i, (_seq, entry, _pos)) in replayed.iter().enumerate() {
        assert_eq!(entry.seq_no, (i + 1) as u64);
    }

    Ok(())
}

#[tokio::test]
async fn test_flusher_batch_window_coalesces_writes() -> Result<()> {
    let dir = tempdir()?;
    let wal_path = dir.path().join("batch_window_test.wal");

    let wal = Arc::new(
        Wal::open_with_config(
            &wal_path,
            WalConfig {
                flusher_config: WalFlusherConfig {
                    batch_window_micros: 50,
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await?,
    );

    let num_tasks = 5;
    let mut handles = Vec::new();

    for i in 0u64..num_tasks {
        let wal_clone = Arc::clone(&wal);
        handles.push(tokio::spawn(async move {
            let op = WalOp::Put {
                tx_id: TxId::new(i + 1),
                key: format!("key-{i}").into_bytes(),
                value: b"val".to_vec(),
            };
            let (batch, _) = wal_clone.prepare_batch(vec![(op, i + 1)]).await?;
            wal_clone.append_batch(batch).await
        }));
    }

    for h in handles {
        h.await
            .map_err(|e| MemFuseError::Storage(e.to_string()))??;
    }

    let replayed = wal.replay().await?;
    assert_eq!(
        replayed.len(),
        num_tasks as usize,
        "Alle 5 Batches müssen sicher im WAL landen"
    );

    for (i, (_seq, entry, _pos)) in replayed.iter().enumerate() {
        assert_eq!(entry.seq_no, (i + 1) as u64);
    }

    Ok(())
}

#[tokio::test]
async fn test_wal_flusher_actor_no_write_to_sealed() {
    let dir = tempfile::tempdir().expect("tempdir");
    let wal_path = dir.path().join("test_flusher_sealed.wal");

    let wal = Wal::open(&wal_path).await.expect("open WAL");
    let op1 = WalOp::Put {
        tx_id: TxId::new(1),
        key: b"k1".to_vec(),
        value: b"v1".to_vec(),
    };
    let (batch1, _) = wal
        .prepare_batch(vec![(op1, 1)])
        .await
        .expect("prepare batch 1");
    wal.append_batch(batch1).await.expect("append batch 1");

    assert!(
        !wal.is_sealed(),
        "WAL must not be sealed before rotate_and_seal"
    );

    let sealed_path = wal.rotate_and_seal().await.expect("rotate_and_seal");
    assert!(wal.is_sealed(), "WAL must be sealed after rotate_and_seal");

    // Attempting to prepare or append to the sealed WAL segment must return Err
    let op2 = WalOp::Put {
        tx_id: TxId::new(2),
        key: b"k2".to_vec(),
        value: b"v2".to_vec(),
    };
    let prep_res = wal.prepare_batch(vec![(op2, 2)]).await;
    assert!(
        prep_res.is_err(),
        "prepare_batch on sealed WAL must return Err, no panic or silent write"
    );

    let dummy_entry = WalEntry::try_new(
        WalOp::Put {
            tx_id: TxId::new(3),
            key: b"k3".to_vec(),
            value: b"v3".to_vec(),
        },
        3,
        &[0u8; 32],
        [0u8; 32],
    )
    .expect("dummy entry");
    let manual_batch = PreparedBatch(vec![dummy_entry]);
    let append_res = wal.append_batch(manual_batch).await;
    assert!(
        append_res.is_err(),
        "append_batch on sealed WAL must return Err, no panic or silent write"
    );

    assert!(sealed_path.exists(), "Sealed path must exist");
}

#[tokio::test]
async fn test_wal_queue_capacity_zero_rejected() {
    let dir = tempdir().expect("tempdir");
    let wal_path = dir.path().join("invalid_cap.wal");

    let res = Wal::open_with_config(
        &wal_path,
        WalConfig {
            flusher_config: WalFlusherConfig {
                queue_capacity: 0,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .await;

    assert!(res.is_err(), "queue_capacity == 0 must return an error");
    if let Err(err) = res {
        assert!(
            err.to_string().contains("queue_capacity"),
            "Error message must mention queue_capacity, got: {err}"
        );
    }
}

#[tokio::test]
async fn test_wal_try_append_backpressure() -> Result<()> {
    let dir = tempdir()?;
    let wal_path = dir.path().join("try_append_backpressure.wal");

    // Configure capacity of 1 with batch_window_micros = 100_000 (100ms window)
    let wal = Arc::new(
        Wal::open_with_config(
            &wal_path,
            WalConfig {
                flusher_config: WalFlusherConfig {
                    queue_capacity: 1,
                    batch_window_micros: 100_000,
                },
                ..Default::default()
            },
        )
        .await?,
    );

    // Get flusher_tx directly to fill channel capacity without holding truncate_lock
    let flusher_tx = {
        let guard = wal.flusher_tx.read().unwrap();
        guard.clone().unwrap()
    };

    let (ack1_tx, _ack1_rx) = tokio::sync::oneshot::channel();
    let (ack2_tx, _ack2_rx) = tokio::sync::oneshot::channel();

    // 1st command is popped by flusher loop and enters batch window wait
    flusher_tx
        .send(WalCommand::Append {
            payload: vec![1, 2, 3],
            last_hmac_val: [0u8; 32],
            ack: ack1_tx,
        })
        .await
        .unwrap();

    // 2nd command fills the bounded channel (capacity 1)
    flusher_tx
        .send(WalCommand::Append {
            payload: vec![4, 5, 6],
            last_hmac_val: [0u8; 32],
            ack: ack2_tx,
        })
        .await
        .unwrap();

    // Now try_append_batch when channel queue is full
    let op = WalOp::Put {
        tx_id: TxId::new(10),
        key: b"overflow_key".to_vec(),
        value: b"overflow_val".to_vec(),
    };
    let (overflow_batch, _) = wal.prepare_batch(vec![(op, 10)]).await?;

    let try_res = wal.try_append_batch(overflow_batch).await;
    assert!(
        try_res.is_err(),
        "try_append_batch must return error when queue is full"
    );
    if let Err(err) = try_res {
        assert!(
            err.to_string().contains("backpressure"),
            "Error must mention backpressure, got: {err}"
        );
    }

    Ok(())
}
