//! Integration test for DiskANN Vamana graph build and Recall@10 accuracy (BEFUND 1).
//! Verifies that DiskANN achieves Recall@10 >= 0.95 on synthetic dataset.
//! Ground-truth top-10 neighbors are computed via an independent brute-force reference implementation.

#![cfg(feature = "experimental-diskann")]

use memfuse_core::{DistanceMetric, DocId, VectorIndex};
use memfuse_index::diskann::{DiskAnnConfig, DiskAnnIndex};
use rand::Rng;
use std::collections::HashSet;

/// Independent brute-force Cosine distance calculation (anti-mirroring requirement).
/// This implementation is strictly separate from `memfuse_index::distance::compute_distance`.
fn independent_brute_force_cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0.0f32;
    let mut norm_a_sq = 0.0f32;
    let mut norm_b_sq = 0.0f32;
    for (&x, &y) in a.iter().zip(b.iter()) {
        dot += x * y;
        norm_a_sq += x * x;
        norm_b_sq += y * y;
    }
    if norm_a_sq <= 0.0 || norm_b_sq <= 0.0 {
        1.0
    } else {
        let sim = dot / (norm_a_sq.sqrt() * norm_b_sq.sqrt());
        (1.0 - sim).max(0.0)
    }
}

/// Computes independent top-k ground truth doc_ids using brute-force exact search.
fn independent_brute_force_knn(
    query: &[f32],
    vectors: &[Vec<f32>],
    ids: &[DocId],
    k: usize,
) -> HashSet<DocId> {
    let mut scored: Vec<(DocId, f32)> = vectors
        .iter()
        .zip(ids.iter())
        .map(|(v, &id)| {
            let dist = independent_brute_force_cosine_distance(query, v);
            (id, dist)
        })
        .collect();

    scored.sort_by(|a, b| a.1.total_cmp(&b.1));
    scored.into_iter().take(k).map(|(id, _)| id).collect()
}

#[tokio::test]
async fn test_diskann_recall_at_10_above_95() {
    let dim = 32;
    let num_vectors = 10_000;
    let num_queries = 50;
    let k = 10;

    let temp_dir = tempfile::tempdir().expect("tempdir");
    let index_path = temp_dir.path().join("diskann_recall.idx");

    let config = DiskAnnConfig {
        index_path,
        dimension: dim,
        max_degree: 32,
        beam_width: 64,
        sector_size: 4096,
        distance_metric: DistanceMetric::Cosine,
        quantize: false,
        ..DiskAnnConfig::default()
    };

    let index = DiskAnnIndex::try_new(config).expect("valid DiskAnnConfig");

    let mut rng = rand::thread_rng();

    // 1. Generate 10,000 synthetic random vectors
    let mut vectors = Vec::with_capacity(num_vectors);
    let mut ids = Vec::with_capacity(num_vectors);
    for i in 0..num_vectors {
        let mut v: Vec<f32> = (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect();
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in v.iter_mut() {
                *x /= norm;
            }
        }
        vectors.push(v);
        ids.push(DocId::from(i as u64 + 1));
    }

    // 2. Generate 50 ground-truth query vectors
    let mut queries = Vec::with_capacity(num_queries);
    for _ in 0..num_queries {
        let mut q: Vec<f32> = (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect();
        let norm: f32 = q.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in q.iter_mut() {
                *x /= norm;
            }
        }
        queries.push(q);
    }

    // 3. Compute independent ground-truth kNN for each query
    let ground_truths: Vec<HashSet<DocId>> = queries
        .iter()
        .map(|q| independent_brute_force_knn(q, &vectors, &ids, k))
        .collect();

    // 4. Build the DiskANN index (uses in-memory graph passes)
    index
        .build(&vectors, &ids)
        .await
        .expect("DiskANN build failed");

    // 5. Execute 50 queries and compute Recall@10
    let mut total_recall = 0.0;
    for (q, gt) in queries.iter().zip(ground_truths.iter()) {
        let search_results = index.search(q, k).await.expect("Search failed");
        let hits = search_results
            .iter()
            .filter(|r| gt.contains(&r.doc_id))
            .count();
        total_recall += hits as f64 / k as f64;
    }

    let avg_recall = total_recall / num_queries as f64;
    println!("DiskANN Recall@10: {:.4}", avg_recall);

    // AC: Recall@10 >= 0.95
    assert!(
        avg_recall >= 0.95,
        "DiskANN Recall@10 too low: {:.4} (expected >= 0.95)",
        avg_recall
    );
}

#[tokio::test]
async fn test_diskann_incremental_vs_full_rebuild_recall_parity() {
    let dim = 32;
    let base_vectors = 1_000;
    let incremental_vectors = 50; // 50 / 1050 ≈ 0.047 <= 0.10 threshold for incremental insert
    let num_queries = 30;
    let k = 10;

    let temp_dir = tempfile::tempdir().expect("tempdir");
    let full_path = temp_dir.path().join("full_rebuild.idx");
    let inc_path = temp_dir.path().join("incremental.idx");

    let mut rng = rand::thread_rng();

    // 1. Generate base + incremental vectors
    let mut all_vectors = Vec::with_capacity(base_vectors + incremental_vectors);
    let mut all_ids = Vec::with_capacity(base_vectors + incremental_vectors);
    for i in 0..(base_vectors + incremental_vectors) {
        let mut v: Vec<f32> = (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect();
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in v.iter_mut() {
                *x /= norm;
            }
        }
        all_vectors.push(v);
        all_ids.push(DocId::from(i as u64 + 1));
    }

    // 2. Generate ground-truth query vectors
    let mut queries = Vec::with_capacity(num_queries);
    for _ in 0..num_queries {
        let mut q: Vec<f32> = (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect();
        let norm: f32 = q.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in q.iter_mut() {
                *x /= norm;
            }
        }
        queries.push(q);
    }

    let ground_truths: Vec<HashSet<DocId>> = queries
        .iter()
        .map(|q| independent_brute_force_knn(q, &all_vectors, &all_ids, k))
        .collect();

    // 3. Build Full Rebuild Index
    let full_config = DiskAnnConfig {
        index_path: full_path,
        dimension: dim,
        max_degree: 32,
        beam_width: 64,
        distance_metric: DistanceMetric::Cosine,
        ..DiskAnnConfig::default()
    };
    let full_index = DiskAnnIndex::try_new(full_config).unwrap();
    full_index.build(&all_vectors, &all_ids).await.unwrap();

    let mut full_recall = 0.0;
    for (q, gt) in queries.iter().zip(ground_truths.iter()) {
        let results = full_index.search(q, k).await.unwrap();
        let hits = results.iter().filter(|r| gt.contains(&r.doc_id)).count();
        full_recall += hits as f64 / k as f64;
    }
    let full_avg_recall = full_recall / num_queries as f64;

    // 4. Build Incremental Index (base via build(), then 50 vectors via insert() + persist_delta())
    let inc_config = DiskAnnConfig {
        index_path: inc_path,
        dimension: dim,
        max_degree: 32,
        beam_width: 64,
        distance_metric: DistanceMetric::Cosine,
        ..DiskAnnConfig::default()
    };
    let inc_index = DiskAnnIndex::try_new(inc_config).unwrap();

    let (base_vecs, base_doc_ids) = (&all_vectors[..base_vectors], &all_ids[..base_vectors]);
    inc_index.build(base_vecs, base_doc_ids).await.unwrap();

    for i in base_vectors..(base_vectors + incremental_vectors) {
        inc_index
            .insert(memfuse_core::TxId(1), all_ids[i], &all_vectors[i])
            .await
            .unwrap();
    }
    inc_index.persist_delta().await.unwrap();

    let mut inc_recall = 0.0;
    for (q, gt) in queries.iter().zip(ground_truths.iter()) {
        let results = inc_index.search(q, k).await.unwrap();
        let hits = results.iter().filter(|r| gt.contains(&r.doc_id)).count();
        inc_recall += hits as f64 / k as f64;
    }
    let inc_avg_recall = inc_recall / num_queries as f64;

    println!(
        "Full Rebuild Recall@10: {:.4}, Incremental Recall@10: {:.4}",
        full_avg_recall, inc_avg_recall
    );

    // Recall drop should be <= 0.05 (5 percentage points)
    let recall_drop = full_avg_recall - inc_avg_recall;
    assert!(
        recall_drop <= 0.05,
        "Incremental insert Recall@10 ({:.4}) dropped too much compared to full rebuild ({:.4}): drop = {:.4} (allowed <= 0.05)",
        inc_avg_recall,
        full_avg_recall,
        recall_drop
    );
}
