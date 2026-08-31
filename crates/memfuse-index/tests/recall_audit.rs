// FILE-CONTEXT
// ZWECK: Audit-Testsuite für HNSW & DiskANN Graph-Korrektheit & Brute-Force Recall
// STAND: TS:2026-08-31T00:00:00Z

use memfuse_core::traits::VectorIndex;
use memfuse_core::types::{DistanceMetric, DocId, TxId};
use memfuse_index::diskann::{DiskAnnConfig, DiskAnnIndex};
use memfuse_index::hnsw::{HnswConfig, HnswIndex};
use rand::seq::SliceRandom;
use rand::Rng;
use std::collections::HashSet;
use std::sync::Arc;

/// Independent Brute-Force Linear Scan kNN Reference Implementation
fn brute_force_knn(
    query: &[f32],
    vectors: &[Vec<f32>],
    ids: &[DocId],
    k: usize,
    metric: DistanceMetric,
) -> Vec<DocId> {
    let mut scored: Vec<(DocId, f64)> = vectors
        .iter()
        .zip(ids.iter())
        .map(|(v, &id)| {
            let dist = match metric {
                DistanceMetric::Cosine => {
                    let mut dot = 0.0f64;
                    let mut na = 0.0f64;
                    let mut nb = 0.0f64;
                    for (&x, &y) in query.iter().zip(v.iter()) {
                        let x64 = x as f64;
                        let y64 = y as f64;
                        dot += x64 * y64;
                        na += x64 * x64;
                        nb += y64 * y64;
                    }
                    if na == 0.0 || nb == 0.0 {
                        1.0
                    } else {
                        1.0 - (dot / (na.sqrt() * nb.sqrt()))
                    }
                }
                DistanceMetric::Euclidean => query
                    .iter()
                    .zip(v.iter())
                    .map(|(&x, &y)| {
                        let diff = (x as f64) - (y as f64);
                        diff * diff
                    })
                    .sum::<f64>()
                    .sqrt(),
                DistanceMetric::DotProduct => -query
                    .iter()
                    .zip(v.iter())
                    .map(|(&x, &y)| (x as f64) * (y as f64))
                    .sum::<f64>(),
                _ => unreachable!(),
            };
            (id, dist)
        })
        .collect();

    scored.sort_by(|a, b| a.1.total_cmp(&b.1));
    scored.into_iter().take(k).map(|(id, _)| id).collect()
}

fn calculate_recall(retrieved: &[DocId], ground_truth: &[DocId]) -> f64 {
    if ground_truth.is_empty() {
        return 1.0;
    }
    let truth_set: HashSet<DocId> = ground_truth.iter().copied().collect();
    let hits = retrieved.iter().filter(|id| truth_set.contains(id)).count();
    hits as f64 / ground_truth.len() as f64
}

#[tokio::test]
async fn test_hnsw_recall_matrix() {
    let dataset_sizes = [100, 1000, 10000];
    let dimensions = [64, 128, 384, 768, 1536];
    let k_values = [1, 5, 10, 50];

    let mut rng = rand::thread_rng();

    println!("\n=== HNSW RECALL AUDIT MATRIX ===");
    println!("N\tDim\tk=1\tk=5\tk=10\tk=50");

    for &n in &dataset_sizes {
        for &dim in &dimensions {
            if n == 10000 && dim > 384 {
                // Skip 10k x 1536 in fast unit test run to keep runtime reasonable
                continue;
            }

            let mut vectors = Vec::with_capacity(n);
            let mut ids = Vec::with_capacity(n);
            for i in 0..n {
                let v: Vec<f32> = (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect();
                vectors.push(v);
                ids.push(DocId::new((i + 1) as u64));
            }

            let config = HnswConfig {
                dimension: dim,
                m: 16,
                ef_construction: 200,
                ef_search: 64,
                distance_metric: DistanceMetric::Cosine,
                ..Default::default()
            };
            let index = HnswIndex::try_new(config).unwrap();
            let tx = TxId::new(1);
            for (id, vec) in ids.iter().zip(vectors.iter()) {
                index.insert(tx, *id, vec).await.unwrap();
            }
            index.commit(tx).await.unwrap();

            // Perform 20 query evaluations
            let mut recalls = vec![0.0f64; k_values.len()];
            let num_queries = 20;

            for _ in 0..num_queries {
                let q_idx = rng.gen_range(0..n);
                let query = &vectors[q_idx];

                for (ki, &k) in k_values.iter().enumerate() {
                    if k > n {
                        continue;
                    }
                    let gt = brute_force_knn(query, &vectors, &ids, k, DistanceMetric::Cosine);
                    let res = index.search(query, k).await.unwrap();
                    let ret_ids: Vec<DocId> = res.into_iter().map(|s| s.doc_id).collect();
                    recalls[ki] += calculate_recall(&ret_ids, &gt);
                }
            }

            let avg_recalls: Vec<f64> = recalls.iter().map(|r| r / num_queries as f64).collect();

            println!(
                "{}\t{}\t{:.4}\t{:.4}\t{:.4}\t{:.4}",
                n, dim, avg_recalls[0], avg_recalls[1], avg_recalls[2], avg_recalls[3]
            );

            // Assert Recall@10 is at least 0.85 for standard parameters
            assert!(
                avg_recalls[2] >= 0.85,
                "Recall@10 ({:.4}) below 0.85 threshold for N={n}, dim={dim}",
                avg_recalls[2]
            );
        }
    }
}

#[tokio::test]
async fn test_diskann_recall_matrix() {
    let dataset_sizes = [100, 1000];
    let dimensions = [64, 128, 384];

    let mut rng = rand::thread_rng();

    println!("\n=== DISKANN RECALL AUDIT MATRIX ===");
    println!("N\tDim\tRecall@10");

    for &n in &dataset_sizes {
        for &dim in &dimensions {
            let temp_dir = tempfile::tempdir().unwrap();
            let index_path = temp_dir.path().join(format!("diskann_recall_{n}_{dim}.idx"));

            let mut vectors = Vec::with_capacity(n);
            let mut ids = Vec::with_capacity(n);
            for i in 0..n {
                let v: Vec<f32> = (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect();
                vectors.push(v);
                ids.push(DocId::new((i + 1) as u64));
            }

            let config = DiskAnnConfig {
                index_path: index_path.clone(),
                dimension: dim,
                max_degree: 32,
                beam_width: 32,
                distance_metric: DistanceMetric::Cosine,
                ..Default::default()
            };

            let index = DiskAnnIndex::try_new(config).unwrap();
            index.build(&vectors, &ids).await.unwrap();

            let mut recall_sum = 0.0f64;
            let num_queries = 20;

            for _ in 0..num_queries {
                let q_idx = rng.gen_range(0..n);
                let query = &vectors[q_idx];
                let gt = brute_force_knn(query, &vectors, &ids, 10, DistanceMetric::Cosine);
                let res = index.search(query, 10).await.unwrap();
                let ret_ids: Vec<DocId> = res.into_iter().map(|s| s.doc_id).collect();
                recall_sum += calculate_recall(&ret_ids, &gt);
            }

            let avg_recall = recall_sum / num_queries as f64;
            println!("{}\t{}\t{:.4}", n, dim, avg_recall);
            assert!(
                avg_recall >= 0.85,
                "DiskANN Recall@10 ({avg_recall:.4}) below threshold for N={n}, dim={dim}"
            );
        }
    }
}

#[tokio::test]
async fn test_insert_order_insensitivity() {
    let dim = 128;
    let n = 300;
    let mut rng = rand::thread_rng();

    let mut vectors = Vec::with_capacity(n);
    let mut ids = Vec::with_capacity(n);
    for i in 0..n {
        let v: Vec<f32> = (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect();
        vectors.push(v);
        ids.push(DocId::new((i + 1) as u64));
    }

    // Index 1: Sequential insertion
    let index1 = HnswIndex::try_new(HnswConfig {
        dimension: dim,
        ..Default::default()
    })
    .unwrap();
    let tx1 = TxId::new(1);
    for (id, v) in ids.iter().zip(vectors.iter()) {
        index1.insert(tx1, *id, v).await.unwrap();
    }
    index1.commit(tx1).await.unwrap();

    // Index 2: Shuffled insertion order
    let mut shuffled_indices: Vec<usize> = (0..n).collect();
    shuffled_indices.shuffle(&mut rng);

    let index2 = HnswIndex::try_new(HnswConfig {
        dimension: dim,
        ..Default::default()
    })
    .unwrap();
    let tx2 = TxId::new(1);
    for &idx in &shuffled_indices {
        index2.insert(tx2, ids[idx], &vectors[idx]).await.unwrap();
    }
    index2.commit(tx2).await.unwrap();

    // Compare search quality across 20 query vectors
    let mut recall1_sum = 0.0;
    let mut recall2_sum = 0.0;

    for i in 0..20 {
        let query = &vectors[i * 10];
        let gt = brute_force_knn(query, &vectors, &ids, 10, DistanceMetric::Cosine);

        let res1: Vec<DocId> = index1
            .search(query, 10)
            .await
            .unwrap()
            .into_iter()
            .map(|s| s.doc_id)
            .collect();
        let res2: Vec<DocId> = index2
            .search(query, 10)
            .await
            .unwrap()
            .into_iter()
            .map(|s| s.doc_id)
            .collect();

        recall1_sum += calculate_recall(&res1, &gt);
        recall2_sum += calculate_recall(&res2, &gt);
    }

    let avg1 = recall1_sum / 20.0;
    let avg2 = recall2_sum / 20.0;
    println!("Sequential Insert Recall@10: {avg1:.4}, Shuffled Insert Recall@10: {avg2:.4}");

    assert!(
        (avg1 - avg2).abs() < 0.08,
        "Insert order caused significant recall divergence: {avg1} vs {avg2}"
    );
}

#[tokio::test]
async fn test_edge_cases_and_tombstones() {
    let dim = 16;
    let config = HnswConfig {
        dimension: dim,
        ..Default::default()
    };

    // 1. Empty Index Search
    let empty_index = HnswIndex::try_new(config.clone()).unwrap();
    let empty_res = empty_index.search(&vec![1.0; dim], 10).await.unwrap();
    assert!(
        empty_res.is_empty(),
        "Search on empty index must return empty Vec"
    );

    // 2. Exactly 1 Element Index
    let single_index = HnswIndex::try_new(config.clone()).unwrap();
    let tx1 = TxId::new(1);
    single_index
        .insert(tx1, DocId::new(42), &vec![2.0; dim])
        .await
        .unwrap();
    single_index.commit(tx1).await.unwrap();

    let single_res = single_index.search(&vec![2.0; dim], 5).await.unwrap();
    assert_eq!(single_res.len(), 1);
    assert_eq!(single_res[0].doc_id, DocId::new(42));

    // 3. Search k > Total elements (k=50, N=5)
    let small_index = HnswIndex::try_new(config.clone()).unwrap();
    let tx2 = TxId::new(2);
    for i in 1..=5 {
        small_index
            .insert(tx2, DocId::new(i), &vec![i as f32; dim])
            .await
            .unwrap();
    }
    small_index.commit(tx2).await.unwrap();

    let small_res = small_index.search(&vec![1.0; dim], 50).await.unwrap();
    assert_eq!(
        small_res.len(),
        5,
        "Search k > N must return exactly N elements"
    );

    // 4. Duplicate Vectors (Identical coordinates inserted multiple times)
    let dup_index = HnswIndex::try_new(config.clone()).unwrap();
    let tx3 = TxId::new(3);
    for i in 1..=5 {
        dup_index
            .insert(tx3, DocId::new(i), &vec![0.5f32; dim])
            .await
            .unwrap();
    }
    dup_index.commit(tx3).await.unwrap();

    let dup_res = dup_index.search(&vec![0.5f32; dim], 5).await.unwrap();
    assert_eq!(dup_res.len(), 5, "Duplicate vectors search must return 5 results");

    // 5. Tombstone Deletes Safety Check
    let del_index = HnswIndex::try_new(config.clone()).unwrap();
    let tx4 = TxId::new(4);
    for i in 1..=10 {
        del_index
            .insert(tx4, DocId::new(i), &vec![i as f32; dim])
            .await
            .unwrap();
    }
    del_index.commit(tx4).await.unwrap();

    // Delete DocId 3 and 7
    let tx5 = TxId::new(5);
    del_index.delete(tx5, DocId::new(3)).await.unwrap();
    del_index.delete(tx5, DocId::new(7)).await.unwrap();
    del_index.commit(tx5).await.unwrap();

    let del_res = del_index.search(&vec![3.0; dim], 10).await.unwrap();
    let returned_ids: HashSet<DocId> = del_res.into_iter().map(|s| s.doc_id).collect();

    assert!(
        !returned_ids.contains(&DocId::new(3)),
        "Tombstoned DocId 3 must not appear in search results"
    );
    assert!(
        !returned_ids.contains(&DocId::new(7)),
        "Tombstoned DocId 7 must not appear in search results"
    );
    assert_eq!(returned_ids.len(), 8, "Expected 8 active documents after deleting 2");
}

#[tokio::test]
async fn test_concurrency_stress_inserts_and_searches() {
    let dim = 32;
    let config = HnswConfig {
        dimension: dim,
        m: 16,
        ef_construction: 64,
        ef_search: 32,
        ..Default::default()
    };
    let index = Arc::new(HnswIndex::try_new(config).unwrap());

    // Pre-populate index with 50 vectors
    let tx_init = TxId::new(1);
    for i in 1..=50 {
        index
            .insert(tx_init, DocId::new(i), &vec![i as f32 * 0.1; dim])
            .await
            .unwrap();
    }
    index.commit(tx_init).await.unwrap();

    let mut handles = Vec::new();

    // 5 Worker tasks performing concurrent inserts
    for worker_id in 0..5 {
        let idx = Arc::clone(&index);
        let handle = tokio::spawn(async move {
            for i in 0..20 {
                let doc_raw = 100 + (worker_id * 20) + i;
                let tx = TxId::new(200 + doc_raw);
                let vec = vec![(doc_raw as f32) * 0.05; dim];
                idx.insert(tx, DocId::new(doc_raw), &vec).await.unwrap();
                idx.commit(tx).await.unwrap();
            }
        });
        handles.push(handle);
    }

    // 5 Worker tasks performing concurrent searches
    for worker_id in 0..5 {
        let idx = Arc::clone(&index);
        let handle = tokio::spawn(async move {
            for i in 0..50 {
                let q_val = ((worker_id * 50 + i) as f32) * 0.1;
                let query = vec![q_val; dim];
                let res = idx.search(&query, 10).await.unwrap();
                assert!(!res.is_empty(), "Concurrent search should return results");
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    // Final integrity checks
    assert_eq!(index.len().await, 150, "Expected 150 active documents post stress test");
    assert!(index.check_connectivity().is_ok(), "Graph connectivity must remain healthy");
}
