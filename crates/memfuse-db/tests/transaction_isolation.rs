use memfuse_core::{DistanceMetric, Result};
use memfuse_db::{MemFuse, MemFuseConfig};
use std::sync::Arc;
use tempfile::tempdir;
use tokio::sync::Barrier;

#[tokio::test]
async fn test_transaction_atomicity_under_load() -> Result<()> {
    let dir = tempdir().unwrap();
    let config = MemFuseConfig {
        dimension: 128,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };
    let db = Arc::new(MemFuse::open_with_config(dir.path(), config).await?);
    let collection = Arc::new(db.collection("default").await?);

    let num_writers = 4;
    let num_readers = 10;
    let iterations = 20;
    let batch_size = 5; // Jede Transaktion schreibt 5 Dokumente

    let barrier = Arc::new(Barrier::new(num_writers + num_readers));
    let mut handles = Vec::new();

    // WRITERS: Schreiben atomare Batches
    for w in 0..num_writers {
        let col = collection.clone();
        let b = barrier.clone();
        handles.push(tokio::spawn(async move {
            b.wait().await;
            for i in 0..iterations {
                let tx = col.begin_transaction().unwrap(); // unwrap allowed
                for j in 0..batch_size {
                    let id = format!("doc_{}_{}_{}", w, i, j);
                    let vec = vec![i as f32; 128];
                    col.insert_op(
                        &tx,
                        &id,
                        &vec,
                        Some(serde_json::json!({"text": format!("data_{}_{}_{}", w, i, j)})),
                    )
                    .await
                    .unwrap(); // unwrap allowed
                }
                tx.commit().await.unwrap(); // unwrap allowed
            }
        }));
    }

    // READERS: Prüfen auf Atomarität
    let atomicity_errors = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    for _ in 0..num_readers {
        let col = collection.clone();
        let b = barrier.clone();
        let errs = atomicity_errors.clone();
        handles.push(tokio::spawn(async move {
            b.wait().await;
            for _ in 0..(iterations * 2) {
                let results = col.scan_prefix("doc_").await.unwrap();
                let count = results.len();

                if count % batch_size != 0 {
                    errs.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                }
                tokio::task::yield_now().await;
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    let final_errors = atomicity_errors.load(std::sync::atomic::Ordering::SeqCst);
    assert_eq!(
        final_errors, 0,
        "Atomicity violation detected! Readers saw partial transaction states."
    );

    Ok(())
}
