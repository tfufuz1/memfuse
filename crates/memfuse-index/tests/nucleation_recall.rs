use memfuse_core::{DistanceMetric, DocId, TxId, VectorIndex};
use memfuse_index::hnsw::{HnswConfig, HnswIndex};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::collections::HashSet;

fn generate_random_vectors(count: usize, dim: usize, seed: u64) -> Vec<Vec<f32>> {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut vectors = Vec::with_capacity(count);
    for _ in 0..count {
        let mut v: Vec<f32> = (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect();
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in v.iter_mut() {
                *x /= norm;
            }
        }
        vectors.push(v);
    }
    vectors
}

fn dot_product(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

fn brute_force_top_k(vectors: &[Vec<f32>], queries: &[Vec<f32>], k: usize) -> Vec<Vec<DocId>> {
    let mut all_top_k = Vec::with_capacity(queries.len());
    for query in queries {
        let mut scored: Vec<(DocId, f32)> = vectors
            .iter()
            .enumerate()
            .map(|(idx, v)| {
                // For unit vectors, Cosine distance = 1 - dot_product
                let dist = 1.0 - dot_product(query, v);
                (DocId::new(idx as u64), dist)
            })
            .collect();

        scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        let top_k: Vec<DocId> = scored.into_iter().take(k).map(|(id, _)| id).collect();
        all_top_k.push(top_k);
    }
    all_top_k
}

async fn build_hnsw_index(vectors: &[Vec<f32>]) -> HnswIndex {
    let dim = vectors[0].len();
    let config = HnswConfig {
        dimension: dim,
        max_elements: vectors.len() + 500,
        m: 16,
        ef_construction: 200,
        ef_search: 64,
        distance_metric: DistanceMetric::Cosine,
        rebuild_threshold: 0.0, // Disable automatic global rebuilds
        quantize: false,
        ..Default::default()
    };

    let index = HnswIndex::try_new(config).unwrap();
    let tx = TxId::new(1);
    for (i, v) in vectors.iter().enumerate() {
        index.insert(tx, DocId::new(i as u64), v).await.unwrap();
    }
    index.commit(tx).await.unwrap();
    index
}

async fn measure_recall_at_k(
    index: &HnswIndex,
    queries: &[Vec<f32>],
    ground_truth: &[Vec<DocId>],
    k: usize,
) -> f64 {
    let mut total_recall = 0.0;
    for (i, query) in queries.iter().enumerate() {
        let results = index.search(query, k).await.unwrap();
        let gt_set: HashSet<DocId> = ground_truth[i].iter().copied().collect();
        let hits = results
            .iter()
            .filter(|res| gt_set.contains(&res.doc_id))
            .count();
        total_recall += hits as f64 / k as f64;
    }
    total_recall / queries.len() as f64
}

fn select_local_cluster_region(vectors: &[Vec<f32>], fraction: f64) -> Vec<u64> {
    let count = (vectors.len() as f64 * fraction) as usize;
    let seed_vector = &vectors[0];
    let mut distances: Vec<(u64, f32)> = vectors
        .iter()
        .enumerate()
        .map(|(idx, v)| {
            let dist = 1.0 - dot_product(seed_vector, v);
            (idx as u64, dist)
        })
        .collect();

    distances.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    distances
        .into_iter()
        .take(count)
        .map(|(id, _)| id)
        .collect()
}

#[tokio::test]
async fn test_nucleation_recall_regression() {
    // 1. Baue Index mit 2000 zufälligen 128-dim Vektoren
    let vectors = generate_random_vectors(2000, 128, 42);
    let index = build_hnsw_index(&vectors).await;

    // 2. Ground truth via Brute-Force für 50 Queries
    let queries = generate_random_vectors(50, 128, 99);
    let ground_truth = brute_force_top_k(&vectors, &queries, 10);

    // 3. Baseline-Recall messen (voller Index)
    let recall_before = measure_recall_at_k(&index, &queries, &ground_truth, 10).await;

    // 4. Lokal konzentrierte Löschung: 15% in einer Nachbarschaftsregion
    let region_ids = select_local_cluster_region(&vectors, 0.15);
    let tx_del = TxId::new(2);
    for id in &region_ids {
        index.delete(tx_del, DocId::new(*id)).await.unwrap();
    }
    index.commit(tx_del).await.unwrap();

    // 5a. Recall OHNE rebuild_region() (nur Tombstones markiert)
    let recall_tombstoned_only = measure_recall_at_k(&index, &queries, &ground_truth, 10).await;

    // 5b. rebuild_region() aufrufen
    index.rebuild_region(region_ids.clone()).await.unwrap();
    let recall_after_rebuild = measure_recall_at_k(&index, &queries, &ground_truth, 10).await;

    println!("Recall vor Löschung: {recall_before:.4}");
    println!("Recall nach Tombstone (kein Rebuild): {recall_tombstoned_only:.4}");
    println!("Recall nach rebuild_region(): {recall_after_rebuild:.4}");

    // KRITISCHE ASSERTION: rebuild_region() darf Recall nicht signifikant
    // schlechter machen als reines Tombstoning
    assert!(
        recall_after_rebuild >= recall_tombstoned_only - 0.05,
        "rebuild_region() verschlechtert Recall@10 um mehr als 5pp: {} -> {}",
        recall_tombstoned_only,
        recall_after_rebuild
    );

    // Und: Absoluter Recall-Verlust gegenüber Baseline darf nicht dramatisch sein
    assert!(
        recall_after_rebuild >= recall_before - 0.15,
        "rebuild_region() Gesamt-Recall-Verlust > 15pp gegenüber Baseline: {} -> {}",
        recall_before,
        recall_after_rebuild
    );
}
