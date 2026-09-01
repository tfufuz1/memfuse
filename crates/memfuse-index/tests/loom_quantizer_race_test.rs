//! Loom-basierter Nebenläufigkeitsbeweis für AGT-INDEX-b2c3d4e5.
//! Ausführung: RUSTFLAGS="--cfg loom" cargo test -p memfuse-index -- loom
//!
//! Testziel: Kein Datenverlust und keine Panik bei 4 parallelen insert()-Aufrufen
//! auf demselben HnswIndex mit aktiviertem SQ8-Quantizer.

#[cfg(loom)]
mod loom_tests {
    use loom::sync::Arc;
    use loom::thread;
    use memfuse_core::{DocId, TxId, VectorIndex};
    use memfuse_index::{HnswConfig, HnswIndex};

    #[test]
    fn test_sq8_quantizer_write_lock_no_race() {
        loom::model(|| {
            let config = HnswConfig {
                dimension: 4,
                quantize: true,
                m: 8,
                ef_construction: 16,
                ef_search: 16,
                ..Default::default()
            };
            let index = Arc::new(HnswIndex::try_new(config).expect("valid index"));

            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            rt.block_on(async {
                let tx0 = TxId::new(100);
                let vec0 = vec![1.0, 2.0, 3.0, 4.0];
                index
                    .insert(tx0, DocId::new(100), &vec0)
                    .await
                    .expect("initial insert");
                index.commit(tx0).await.expect("initial commit");
            });

            let mut handles = Vec::new();

            for i in 1..=4 {
                let index_clone = Arc::clone(&index);
                let handle = thread::spawn(move || {
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .unwrap();
                    rt.block_on(async {
                        let tx = TxId::new(i);
                        let doc_id = DocId::new(i);
                        let vec = vec![i as f32 * 10.0, i as f32 * -5.0, 1.0, 2.0];
                        index_clone
                            .insert(tx, doc_id, &vec)
                            .await
                            .expect("parallel insert");
                        index_clone.commit(tx).await.expect("parallel commit");
                    });
                });
                handles.push(handle);
            }

            for handle in handles {
                handle.join().expect("thread finished without panic");
            }

            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            rt.block_on(async {
                assert_eq!(index.len().await, 5);
            });
        });
    }
}

// Normaler (nicht-loom) Smoke-Test der immer in CI läuft:
#[cfg(not(loom))]
#[cfg(test)]
mod normal_tests {
    use memfuse_core::{DocId, TxId, VectorIndex};
    use memfuse_index::{HnswConfig, HnswIndex};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_sq8_parallel_insert_no_corruption() {
        let config = HnswConfig {
            dimension: 4,
            quantize: true,
            m: 8,
            ef_construction: 16,
            ef_search: 16,
            ..Default::default()
        };
        let index = Arc::new(HnswIndex::try_new(config).expect("valid index"));

        let tx0 = TxId::new(100);
        let vec0 = vec![1.0, 2.0, 3.0, 4.0];
        index
            .insert(tx0, DocId::new(100), &vec0)
            .await
            .expect("initial insert");
        index.commit(tx0).await.expect("initial commit");

        let mut tasks = Vec::new();
        for i in 1..=4u64 {
            let index_clone = Arc::clone(&index);
            tasks.push(tokio::spawn(async move {
                let tx = TxId::new(i);
                let doc_id = DocId::new(i);
                let vec = vec![i as f32 * 10.0, i as f32 * -5.0, 1.0, 2.0];
                index_clone.insert(tx, doc_id, &vec).await?;
                index_clone.commit(tx).await?;
                Ok::<(), memfuse_core::MemFuseError>(())
            }));
        }

        for task in tasks {
            let res = task.await.expect("task join succeeded");
            assert!(res.is_ok(), "Insert and commit succeeded without error");
        }

        assert_eq!(index.len().await, 5);
    }
}
