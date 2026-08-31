// FILE-CONTEXT
// ZWECK: Audit Benchmark Suite zur Ermittlung von Durchsatz, Speedup, Latenz-Perzentilen, Pareto-Front und RAM-Footprint
// STAND: TS:2026-08-31T00:00:00Z

use memfuse_core::traits::VectorIndex;
use memfuse_core::types::{DocId, TxId};
use memfuse_index::distance::*;
use memfuse_index::hnsw::{HnswConfig, HnswIndex};
use rand::Rng;
use std::collections::HashSet;
use std::time::Instant;

fn brute_force_knn(query: &[f32], vectors: &[Vec<f32>], ids: &[DocId], k: usize) -> Vec<DocId> {
    let mut scored: Vec<(DocId, f32)> = vectors
        .iter()
        .zip(ids.iter())
        .map(|(v, &id)| {
            let dist = cosine_distance_scalar(query, v);
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

fn percentile(mut latencies: Vec<f64>, p: f64) -> f64 {
    if latencies.is_empty() {
        return 0.0;
    }
    latencies.sort_by(|a, b| a.total_cmp(b));
    let idx = ((latencies.len() as f64 - 1.0) * p).round() as usize;
    latencies[idx]
}

#[tokio::main]
async fn main() {
    let mut rng = rand::thread_rng();

    println!("===============================================================================");
    println!("                   MEMFUSE-INDEX EMPIRICAL AUDIT BENCHMARKS                   ");
    println!("===============================================================================");

    // 1. Distance Metric Throughput & Speedup (SIMD vs Scalar)
    println!("\n--- 1. DISTANCE METRIC THROUGHPUT & SIMD SPEEDUP (1536-dim, 100,000 ops) ---");
    let dim = 1536;
    let num_ops = 100_000;
    let a: Vec<f32> = (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect();
    let b: Vec<f32> = (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect();

    // Cosine
    let t0 = Instant::now();
    for _ in 0..num_ops {
        let _ = std::hint::black_box(cosine_distance_scalar(&a, &b));
    }
    let dur_cos_scalar = t0.elapsed();

    let t0 = Instant::now();
    for _ in 0..num_ops {
        let _ = std::hint::black_box(cosine_distance(&a, &b).unwrap());
    }
    let dur_cos_simd = t0.elapsed();

    let speedup_cos = dur_cos_scalar.as_secs_f64() / dur_cos_simd.as_secs_f64();
    let ops_sec_cos_simd = num_ops as f64 / dur_cos_simd.as_secs_f64();

    // Euclidean
    let t0 = Instant::now();
    for _ in 0..num_ops {
        let _ = std::hint::black_box(euclidean_distance_scalar(&a, &b));
    }
    let dur_euc_scalar = t0.elapsed();

    let t0 = Instant::now();
    for _ in 0..num_ops {
        let _ = std::hint::black_box(euclidean_distance(&a, &b).unwrap());
    }
    let dur_euc_simd = t0.elapsed();

    let speedup_euc = dur_euc_scalar.as_secs_f64() / dur_euc_simd.as_secs_f64();
    let ops_sec_euc_simd = num_ops as f64 / dur_euc_simd.as_secs_f64();

    // Dot Product
    let t0 = Instant::now();
    for _ in 0..num_ops {
        let _ = std::hint::black_box(dot_product_scalar(&a, &b));
    }
    let dur_dot_scalar = t0.elapsed();

    let t0 = Instant::now();
    for _ in 0..num_ops {
        let _ = std::hint::black_box(dot_product_distance(&a, &b).unwrap());
    }
    let dur_dot_simd = t0.elapsed();

    let speedup_dot = dur_dot_scalar.as_secs_f64() / dur_dot_simd.as_secs_f64();
    let ops_sec_dot_simd = num_ops as f64 / dur_dot_simd.as_secs_f64();

    println!("Metric       \tScalar (ms)\tSIMD (ms)\tThroughput (ops/s)\tSpeedup");
    println!(
        "Cosine       \t{:.2}\t\t{:.2}\t\t{:.2e}\t\t{:.2}x",
        dur_cos_scalar.as_secs_f64() * 1000.0,
        dur_cos_simd.as_secs_f64() * 1000.0,
        ops_sec_cos_simd,
        speedup_cos
    );
    println!(
        "Euclidean    \t{:.2}\t\t{:.2}\t\t{:.2e}\t\t{:.2}x",
        dur_euc_scalar.as_secs_f64() * 1000.0,
        dur_euc_simd.as_secs_f64() * 1000.0,
        ops_sec_euc_simd,
        speedup_euc
    );
    println!(
        "DotProduct   \t{:.2}\t\t{:.2}\t\t{:.2e}\t\t{:.2}x",
        dur_dot_scalar.as_secs_f64() * 1000.0,
        dur_dot_simd.as_secs_f64() * 1000.0,
        ops_sec_dot_simd,
        speedup_dot
    );

    // 2. HNSW Build Time vs Dataset Size
    println!("\n--- 2. HNSW BUILD TIME VS DATASET SIZE (128-dim, M=16, ef_construction=200) ---");
    println!("N       \tBuild Time (ms)\tThroughput (vec/sec)");
    for &n in &[100, 1000, 5000] {
        let vectors: Vec<Vec<f32>> = (0..n)
            .map(|_| (0..128).map(|_| rng.gen_range(-1.0..1.0)).collect())
            .collect();
        let ids: Vec<DocId> = (1..=n).map(|i| DocId::new(i as u64)).collect();

        let config = HnswConfig {
            dimension: 128,
            m: 16,
            ef_construction: 200,
            ..Default::default()
        };
        let index = HnswIndex::try_new(config).unwrap();

        let t0 = Instant::now();
        let tx = TxId::new(1);
        for (&id, v) in ids.iter().zip(vectors.iter()) {
            index.insert(tx, id, v).await.unwrap();
        }
        index.commit(tx).await.unwrap();
        let build_time = t0.elapsed();

        let vec_per_sec = n as f64 / build_time.as_secs_f64();
        println!(
            "{}\t\t{:.2}\t\t{:.1}",
            n,
            build_time.as_secs_f64() * 1000.0,
            vec_per_sec
        );
    }

    // 3. HNSW Search Latency & Recall vs ef_search (Pareto Front)
    println!("\n--- 3. RECALL VS SEARCH LATENCY PARETO FRONT (N=1,000, 128-dim) ---");
    let n = 1000;
    let dim_p = 128;
    let vectors: Vec<Vec<f32>> = (0..n)
        .map(|_| (0..dim_p).map(|_| rng.gen_range(-1.0..1.0)).collect())
        .collect();
    let ids: Vec<DocId> = (1..=n).map(|i| DocId::new(i as u64)).collect();

    let ef_search_values = [8, 16, 32, 64, 128, 256];
    println!("ef_search\tp50 (µs)\tp95 (µs)\tp99 (µs)\tRecall@10");

    for &ef in &ef_search_values {
        let custom_config = HnswConfig {
            dimension: dim_p,
            m: 16,
            ef_construction: 200,
            ef_search: ef,
            ..Default::default()
        };
        let index_ef = HnswIndex::try_new(custom_config).unwrap();
        let tx_ef = TxId::new(1);
        for (&id, v) in ids.iter().zip(vectors.iter()) {
            index_ef.insert(tx_ef, id, v).await.unwrap();
        }
        index_ef.commit(tx_ef).await.unwrap();

        let mut latencies_us = Vec::with_capacity(50);
        let mut recall_sum = 0.0f64;

        for _ in 0..50 {
            let q_idx = rng.gen_range(0..n);
            let query = &vectors[q_idx];

            let gt = brute_force_knn(query, &vectors, &ids, 10);

            let t0 = Instant::now();
            let res = index_ef.search(query, 10).await.unwrap();
            let dur_us = t0.elapsed().as_secs_f64() * 1_000_000.0;
            latencies_us.push(dur_us);

            let ret_ids: Vec<DocId> = res.into_iter().map(|s| s.doc_id).collect();
            recall_sum += calculate_recall(&ret_ids, &gt);
        }

        let p50 = percentile(latencies_us.clone(), 0.50);
        let p95 = percentile(latencies_us.clone(), 0.95);
        let p99 = percentile(latencies_us, 0.99);
        let avg_recall = recall_sum / 50.0;

        println!(
            "{}\t\t{:.1}\t\t{:.1}\t\t{:.1}\t\t{:.4}",
            ef, p50, p95, p99, avg_recall
        );
    }

    // 4. Memory Footprint & SQ8 Reduction Factor
    println!("\n--- 4. MEMORY FOOTPRINT & SQ8 REDUCTION FACTOR (2,000 vectors) ---");
    println!("Dimension\tUnquantized (MB)\tSQ8 Quantized (MB)\tReduction Factor\tRecall@10 Loss");

    for &dim_m in &[128, 384, 768] {
        let n_m = 2000;
        let vecs: Vec<Vec<f32>> = (0..n_m)
            .map(|_| (0..dim_m).map(|_| rng.gen_range(-1.0..1.0)).collect())
            .collect();
        let ids_m: Vec<DocId> = (1..=n_m).map(|i| DocId::new(i as u64)).collect();

        // Unquantized HNSW
        let idx_f32 = HnswIndex::try_new(HnswConfig {
            dimension: dim_m,
            quantize: false,
            ..Default::default()
        })
        .unwrap();
        let tx_m = TxId::new(1);
        for (&id, v) in ids_m.iter().zip(vecs.iter()) {
            idx_f32.insert(tx_m, id, v).await.unwrap();
        }
        idx_f32.commit(tx_m).await.unwrap();
        let stats_f32 = idx_f32.stats().await.unwrap();
        let mem_f32_mb = stats_f32.memory_usage_bytes as f64 / (1024.0 * 1024.0);

        // SQ8 Quantized HNSW
        let idx_sq8 = HnswIndex::try_new(HnswConfig {
            dimension: dim_m,
            quantize: true,
            ..Default::default()
        })
        .unwrap();
        let tx_sq = TxId::new(1);
        for (&id, v) in ids_m.iter().zip(vecs.iter()) {
            idx_sq8.insert(tx_sq, id, v).await.unwrap();
        }
        idx_sq8.commit(tx_sq).await.unwrap();
        let stats_sq8 = idx_sq8.stats().await.unwrap();
        let mem_sq8_mb = stats_sq8.memory_usage_bytes as f64 / (1024.0 * 1024.0);

        let reduction = mem_f32_mb / mem_sq8_mb;

        // Measure Recall Loss
        let mut r_f32_sum = 0.0;
        let mut r_sq8_sum = 0.0;
        for _ in 0..20 {
            let q_idx = rng.gen_range(0..n_m);
            let query = &vecs[q_idx];
            let gt = brute_force_knn(query, &vecs, &ids_m, 10);

            let res_f32: Vec<DocId> = idx_f32
                .search(query, 10)
                .await
                .unwrap()
                .into_iter()
                .map(|s| s.doc_id)
                .collect();
            let res_sq8: Vec<DocId> = idx_sq8
                .search(query, 10)
                .await
                .unwrap()
                .into_iter()
                .map(|s| s.doc_id)
                .collect();

            r_f32_sum += calculate_recall(&res_f32, &gt);
            r_sq8_sum += calculate_recall(&res_sq8, &gt);
        }

        let loss = (r_f32_sum / 20.0) - (r_sq8_sum / 20.0);

        println!(
            "{}\t\t{:.2}\t\t\t{:.2}\t\t\t{:.2}x\t\t\t{:.4}",
            dim_m, mem_f32_mb, mem_sq8_mb, reduction, loss
        );
    }

    println!("\n===============================================================================");
    println!("                          AUDIT BENCHMARKS COMPLETE                            ");
    println!("===============================================================================");
}
