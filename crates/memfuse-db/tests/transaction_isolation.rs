use memfuse_db::Collection;
use memfuse_core::{DocId, Result, DistanceMetric};
use tempfile::tempdir;
use std::sync::Arc;
use tokio::sync::Barrier;

#[tokio::test]
async fn test_transaction_atomicity_under_load() -> Result<()> {
    let dir = tempdir().unwrap();
    // Wir nutzen eine reale Collection-Instanz (LSM + HNSW)
    let collection = Arc::new(Collection::open_temp(dir.path(), 128, DistanceMetric::Cosine).await?);
    
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
                let tx = col.begin_transaction().await.unwrap();
                for j in 0..batch_size {
                    let id = (w * 1000 + i * 10 + j) as u64;
                    let vec = vec![i as f32; 128];
                    col.insert_with_tx(&tx, DocId::new(id), &vec, format!("data_{}_{}_{}", w, i, j).as_bytes())
                        .await
                        .unwrap();
                }
                tx.commit().await.unwrap();
            }
        }));
    }

    // READERS: Prüfen auf Atomarität
    // Invariante: Da wir immer batch_size (5) Dokumente pro Tx schreiben, 
    // und die TxId-Bereiche disjunkt sind, muss die Gesamtzahl der Dokumente 
    // in der Collection immer ein Vielfaches von batch_size sein (oder wir finden gar nichts).
    let atomicity_errors = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    for _ in 0..num_readers {
        let col = collection.clone();
        let b = barrier.clone();
        let errs = atomicity_errors.clone();
        handles.push(tokio::spawn(async move {
            b.wait().await;
            for _ in 0..(iterations * 2) {
                // Wir zählen alle Dokumente im Index
                // Hinweis: search mit hohem k, um alle zu finden
                let results = col.search(&vec![0.0; 128], 1000).await.unwrap();
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
    assert_eq!(final_errors, 0, "Atomicity violation detected! Readers saw partial transaction states.");
    
    Ok(())
}
