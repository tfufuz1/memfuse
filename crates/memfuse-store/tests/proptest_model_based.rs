use memfuse_core::{StorageEngine, TxId};
use memfuse_store::lsm::{LsmConfig, LsmStorage};
use proptest::prelude::*;
use std::collections::HashMap;
use tempfile::TempDir;

#[tokio::test]
async fn test_failing_proptest_sequence() -> memfuse_core::Result<()> {
    let tmp = TempDir::new().unwrap();
    let config = LsmConfig {
        path: tmp.path().to_path_buf(),
        memtable_size_limit: 1024,
        ..Default::default()
    };

    let mut storage = LsmStorage::new(config.clone()).await?;

    // 1. Delete(0)
    let tx1 = TxId::new(1);
    storage.delete(tx1, b"prop_k_0").await?;
    storage.commit(tx1).await?;

    // 2. Put(1, 0)
    let tx2 = TxId::new(2);
    storage.put(tx2, b"prop_k_1", b"prop_v_0").await?;
    storage.commit(tx2).await?;

    // 3. Put(1, 0)
    let tx3 = TxId::new(3);
    storage.put(tx3, b"prop_k_1", b"prop_v_0").await?;
    storage.commit(tx3).await?;

    // 4. Flush
    storage.force_flush().await?;

    // 5. Restart
    drop(storage);
    let storage = LsmStorage::new(config.clone()).await?;

    let val_k1 = storage.get(b"prop_k_1").await?;
    assert_eq!(
        val_k1,
        Some(b"prop_v_0".to_vec()),
        "prop_k_1 must equal prop_v_0 after restart"
    );

    Ok(())
}

fn operation_strategy() -> impl Strategy<Value = Operation> {
    prop_oneof![
        (0..10u8, 0..100u8).prop_map(|(k, v)| Operation::Put(k, v)),
        (0..10u8).prop_map(Operation::Delete),
        Just(Operation::Flush),
        Just(Operation::Compact),
        Just(Operation::Restart),
    ]
}

#[derive(Debug, Clone)]
enum Operation {
    Put(u8, u8),
    Delete(u8),
    Flush,
    Compact,
    Restart,
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(20))]
    #[test]
    fn prop_model_based_lsm_simulation(ops in proptest::collection::vec(operation_strategy(), 1..60)) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let tmp = TempDir::new().unwrap();
            let path = tmp.path().to_path_buf();

            let config = LsmConfig {
                path: path.clone(),
                memtable_size_limit: 1024,
                ..Default::default()
            };

            let mut storage = LsmStorage::new(config.clone()).await.unwrap();
            let mut shadow = HashMap::<Vec<u8>, Vec<u8>>::new();
            let mut tx_counter = 1u64;

            for op in ops {
                match op {
                    Operation::Put(k_byte, v_byte) => {
                        let key = format!("prop_k_{}", k_byte).into_bytes();
                        let val = format!("prop_v_{}", v_byte).into_bytes();
                        let tx = TxId::new(tx_counter);
                        tx_counter += 1;

                        storage.put(tx, &key, &val).await.unwrap();
                        storage.commit(tx).await.unwrap();
                        shadow.insert(key, val);
                    }
                    Operation::Delete(k_byte) => {
                        let key = format!("prop_k_{}", k_byte).into_bytes();
                        let tx = TxId::new(tx_counter);
                        tx_counter += 1;

                        storage.delete(tx, &key).await.unwrap();
                        storage.commit(tx).await.unwrap();
                        shadow.remove(&key);
                    }
                    Operation::Flush => {
                        storage.force_flush().await.unwrap();
                    }
                    Operation::Compact => {
                        storage.maybe_compact().await.unwrap();
                    }
                    Operation::Restart => {
                        drop(storage);
                        storage = LsmStorage::new(config.clone()).await.unwrap();
                    }
                }
            }

            for (key, expected_val) in &shadow {
                let actual_val = storage.get(key).await.unwrap();
                assert_eq!(
                    actual_val.as_ref(),
                    Some(expected_val),
                    "Proptest mismatch for key {:?}",
                    String::from_utf8_lossy(key)
                );
            }

            for k_byte in 0..10u8 {
                let key = format!("prop_k_{}", k_byte).into_bytes();
                if !shadow.contains_key(&key) {
                    let actual_val = storage.get(&key).await.unwrap();
                    assert_eq!(
                        actual_val, None,
                        "Key {:?} expected deleted but found in LSM",
                        String::from_utf8_lossy(&key)
                    );
                }
            }
        });
    }
}
