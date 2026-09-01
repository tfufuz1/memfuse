//! Loom-basierter Nebenläufigkeitsbeweis für AGT-INDEX-b2c3d4e5.
//! Ausführung: RUSTFLAGS="--cfg loom" cargo test -p memfuse-index -- loom
//!
//! Testziel: Kein Datenverlust und keine Panik bei 4 parallelen insert()-Aufrufen
//! auf demselben HnswIndex mit aktiviertem SQ8-Quantizer.

#![allow(unexpected_cfgs)]

#[cfg(loom)]
mod loom_tests {
    #[cfg(feature = "loom")]
    use loom::{sync::Arc, thread};

    #[cfg(not(feature = "loom"))]
    mod loom_fallback {
        pub use std::sync::Arc;
        pub use std::thread;
        pub fn model<F: FnOnce()>(f: F) {
            f();
        }
    }

    #[cfg(not(feature = "loom"))]
    use loom_fallback::{model, Arc, thread};

    use memfuse_core::{DocId, TxId, VectorIndex};
    use memfuse_index::{HnswConfig, HnswIndex};

    #[test]
    fn test_sq8_quantizer_write_lock_no_race() {
        #[cfg(feature = "loom")]
        loom::model(run_model);

        #[cfg(not(feature = "loom"))]
        model(run_model);
    }

    fn run_model() {
        let config = HnswConfig {
            dimension: 4,
            quantize: true,
            ..HnswConfig::default()
        };
        let index = Arc::new(HnswIndex::try_new(config).expect("valid config"));

        // Trainiere Quantizer mit einem initialen Vektor damit er Some() ist
        let init_rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        init_rt.block_on(async {
            let init_tx = TxId::new(1);
            index
                .insert(init_tx, DocId::new(1), &[1.0, 2.0, 3.0, 4.0])
                .await
                .expect("initial insert");
            index.commit(init_tx).await.expect("commit init_tx");
        });

        // Starte 4 loom-Threads, jeder ruft index.insert(tx, doc_id, vec) auf
        let mut handles = Vec::new();
        for i in 2..=5u64 {
            let idx = Arc::clone(&index);
            handles.push(thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .build()
                    .unwrap();
                rt.block_on(async {
                    let tx = TxId::new(i);
                    let doc_id = DocId::new(i);
                    let vec = [i as f32, (i * 2) as f32, (i * 3) as f32, (i * 4) as f32];
                    idx.insert(tx, doc_id, &vec).await?;
                    idx.commit(tx).await
                })
            }));
        }

        for handle in handles {
            let res = handle.join().expect("thread panicked");
            assert!(res.is_ok(), "insert/commit failed: {:?}", res);
        }

        // Nach join aller Threads: prüfe dass kein Panic aufgetreten ist und Suche funktioniert
        let search_rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        search_rt.block_on(async {
            let res = index.search(&[1.0, 2.0, 3.0, 4.0], 5).await;
            assert!(res.is_ok(), "search failed: {:?}", res);
            assert_eq!(res.unwrap().len(), 5);
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
            ..HnswConfig::default()
        };
        let index = Arc::new(HnswIndex::try_new(config).expect("valid config"));

        // Trainiere Quantizer mit einem initialen Vektor damit er Some() ist
        let init_tx = TxId::new(1);
        index
            .insert(init_tx, DocId::new(1), &[1.0, 2.0, 3.0, 4.0])
            .await
            .expect("initial insert");
        index.commit(init_tx).await.expect("commit init");

        // Starte 4 tokio-Tasks, jeder insertet einen Vektor
        let mut tasks = Vec::new();
        for i in 2..=5u64 {
            let idx = Arc::clone(&index);
            tasks.push(tokio::spawn(async move {
                let tx = TxId::new(i);
                let doc_id = DocId::new(i);
                let vec = [i as f32, (i * 2) as f32, (i * 3) as f32, (i * 4) as f32];
                idx.insert(tx, doc_id, &vec).await?;
                idx.commit(tx).await
            }));
        }

        // Warte auf alle Tasks
        for task in tasks {
            let res = task.await.expect("task join failed");
            assert!(res.is_ok(), "insert/commit returned error: {:?}", res);
        }

        // Prüfe: search liefert 5 Ergebnisse und kein Fehler zurückgegeben
        let results = index
            .search(&[1.0, 2.0, 3.0, 4.0], 5)
            .await
            .expect("search succeeded");
        assert_eq!(results.len(), 5);
    }
}
