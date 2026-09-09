// FILE-CONTEXT: Criterion benchmark suite for F-02 Partial HNSW Rebuild recall regression curve across growing tombstone ratios.

use criterion::{black_box, criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion};
use memfuse_core::{DistanceMetric, DocId, TxId, VectorIndex};
use memfuse_index::hnsw::{HnswConfig, HnswIndex};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::collections::HashSet;

fn generate_vectors(count: usize, dim: usize, seed: u64) -> Vec<Vec<f32>> {
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

fn brute_force_knn(
    vectors: &[Vec<f32>],
    deleted_set: &HashSet<u64>,
    queries: &[Vec<f32>],
    k: usize,
) -> Vec<Vec<DocId>> {
    let mut all_gt = Vec::with_capacity(queries.len());
    for query in queries {
        let mut scored: Vec<(DocId, f32)> = vectors
            .iter()
            .enumerate()
            .filter(|(idx, _)| !deleted_set.contains(&(*idx as u64)))
            .map(|(idx, v)| {
                let dist = 1.0 - dot_product(query, v);
                (DocId::new(idx as u64), dist)
            })
            .collect();

        scored.sort_by(|a, b| a.1.total_cmp(&b.1));
        let top_k: Vec<DocId> = scored.into_iter().take(k).map(|(id, _)| id).collect();
        all_gt.push(top_k);
    }
    all_gt
}

fn bench_nucleation_recall_curve(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Tokio runtime required for Nucleation benchmarks");

    let num_vectors = 1_000;
    let dim = 128;
    let num_queries = 50;

    let vectors = generate_vectors(num_vectors, dim, 42);
    let queries = generate_vectors(num_queries, dim, 1337);

    let tombstone_ratios = [0.10, 0.20, 0.30, 0.40];

    let mut group = c.benchmark_group("Nucleation_Recall_Regression_Curve");

    for &ratio in &tombstone_ratios {
        let ratio_percent = (ratio * 100.0) as usize;
        let tombstone_count = (num_vectors as f64 * ratio) as usize;

        let tombstone_ids: Vec<u64> = (0..tombstone_count as u64).collect();
        let tombstone_set: HashSet<u64> = tombstone_ids.iter().copied().collect();

        let ground_truth = brute_force_knn(&vectors, &tombstone_set, &queries, 10);

        group.bench_function(
            BenchmarkId::new("rebuild_region_recall_k10", format!("{}pct_deleted", ratio_percent)),
            |b| {
                b.iter_batched(
                    || {
                        rt.block_on(async {
                            let config = HnswConfig {
                                dimension: dim,
                                max_elements: num_vectors + 100,
                                m: 16,
                                ef_construction: 100,
                                ef_search: 64,
                                distance_metric: DistanceMetric::Cosine,
                                rebuild_threshold: 0.0,
                                quantize: false,
                                ..Default::default()
                            };

                            let index = HnswIndex::try_new(config).expect("Index creation failed");
                            let tx = TxId::new(1);
                            for (i, v) in vectors.iter().enumerate() {
                                index
                                    .insert(tx, DocId::new(i as u64), v)
                                    .await
                                    .expect("insert failed");
                            }
                            index.commit(tx).await.expect("commit failed");

                            let tx_del = TxId::new(2);
                            for &id in &tombstone_ids {
                                index
                                    .delete(tx_del, DocId::new(id))
                                    .await
                                    .expect("delete failed");
                            }
                            index.commit(tx_del).await.expect("commit delete failed");

                            (index, tombstone_ids.clone())
                        })
                    },
                    |(index, ts_ids)| {
                        rt.block_on(async {
                            index
                                .rebuild_region(black_box(ts_ids))
                                .await
                                .expect("rebuild_region failed");

                            let mut total_hits = 0;
                            for (i, query) in queries.iter().enumerate() {
                                let results = index.search(query, 10).await.expect("search failed");
                                let gt_set: HashSet<DocId> = ground_truth[i].iter().copied().collect();
                                total_hits += results
                                    .iter()
                                    .filter(|res| gt_set.contains(&res.doc_id))
                                    .count();
                            }
                            let recall = total_hits as f64 / (num_queries * 10) as f64;
                            black_box(recall);
                        });
                    },
                    BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_nucleation_recall_curve);
criterion_main!(benches);
