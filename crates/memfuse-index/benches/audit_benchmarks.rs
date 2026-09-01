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

    // 5. HNSW Concurrent Write Throughput Scaling (50,000 base vectors, 1,000 parallel inserts)
    println!(
        "\n--- 5. HNSW WRITE THROUGHPUT SCALING (50,000 Base Vectors, 1,000 Total Inserts) ---"
    );
    let base_n = 50_000;
    let extra_n = 1_000;
    let dim_s = 128;

    println!("Building base index with {} vectors...", base_n);
    let base_config = HnswConfig {
        dimension: dim_s,
        m: 16,
        ef_construction: 64,
        ef_search: 64,
        ..Default::default()
    };
    let base_index = HnswIndex::try_new(base_config.clone()).unwrap();

    let base_vectors: Vec<Vec<f32>> = (0..base_n)
        .map(|_| (0..dim_s).map(|_| rng.gen_range(-1.0..1.0)).collect())
        .collect();

    let t_base_start = Instant::now();
    let batch_size = 5000;
    for batch_idx in 0..(base_n / batch_size) {
        let tx = TxId::new(batch_idx as u64 + 1);
        for i in 0..batch_size {
            let idx = batch_idx * batch_size + i;
            let doc_id = DocId::new((idx + 1) as u64);
            base_index
                .insert(tx, doc_id, &base_vectors[idx])
                .await
                .unwrap();
        }
        base_index.commit(tx).await.unwrap();
    }
    println!(
        "Base index of {} vectors built in {:.2}s",
        base_n,
        t_base_start.elapsed().as_secs_f64()
    );

    let temp_dir = tempfile::tempdir().unwrap();
    let base_file = temp_dir.path().join("base_50k.hnsw");
    base_index.save(&base_file).await.unwrap();

    let extra_vectors: Vec<Vec<f32>> = (0..extra_n)
        .map(|_| (0..dim_s).map(|_| rng.gen_range(-1.0..1.0)).collect())
        .collect();

    let thread_counts = [1, 2, 4, 8, 16];
    let mut baseline_throughput = 0.0;

    println!("Threads\tTotal Inserts\tElapsed (ms)\tThroughput (vec/s)\tScaling Factor");

    for &num_threads in &thread_counts {
        let index = HnswIndex::try_new(base_config.clone()).unwrap();
        index.load_mmap(&base_file).await.unwrap();

        let extra_vectors_arc = std::sync::Arc::new(extra_vectors.clone());
        let index_arc = std::sync::Arc::new(index);

        let t0 = Instant::now();
        let items_per_thread = extra_n / num_threads;

        let mut handles = Vec::with_capacity(num_threads);
        for t_idx in 0..num_threads {
            let vecs = std::sync::Arc::clone(&extra_vectors_arc);
            let idx_ref = std::sync::Arc::clone(&index_arc);
            let start = t_idx * items_per_thread;
            let end = if t_idx == num_threads - 1 {
                extra_n
            } else {
                start + items_per_thread
            };

            handles.push(tokio::spawn(async move {
                for i in start..end {
                    let tx = TxId::new(((t_idx * 100_000) + i + 100_000) as u64);
                    let doc_id = DocId::new((50_000 + i + 1) as u64);
                    idx_ref.insert(tx, doc_id, &vecs[i]).await.unwrap();
                    idx_ref.commit(tx).await.unwrap();
                }
            }));
        }

        for handle in handles {
            handle.await.unwrap();
        }

        let elapsed = t0.elapsed();
        let elapsed_secs = elapsed.as_secs_f64();
        let throughput = extra_n as f64 / elapsed_secs;

        if num_threads == 1 {
            baseline_throughput = throughput;
        }
        let scaling_factor = throughput / baseline_throughput;

        println!(
            "{}\t{}\t\t{:.2}\t\t{:.1}\t\t{:.2}x",
            num_threads,
            extra_n,
            elapsed_secs * 1000.0,
            throughput,
            scaling_factor
        );
    }

    // 6. HNSW Mixed Workload Benchmark (Concurrent Search under Write Load)
    println!("\n--- 6. HNSW MIXED WORKLOAD BENCHMARK (Concurrent Reads + High Write Load) ---");
    let mixed_index = HnswIndex::try_new(base_config.clone()).unwrap();
    mixed_index.load_mmap(&base_file).await.unwrap();
    let mixed_index_arc = std::sync::Arc::new(mixed_index);

    // Baseline: Pure Search Latency (Zero Write Load)
    let num_searches = 500;
    let query_vectors: Vec<Vec<f32>> = (0..num_searches)
        .map(|_| (0..dim_s).map(|_| rng.gen_range(-1.0..1.0)).collect())
        .collect();

    let mut baseline_search_latencies = Vec::with_capacity(num_searches);
    for q in &query_vectors {
        let t0 = Instant::now();
        let _ = mixed_index_arc.search(q, 10).await.unwrap();
        baseline_search_latencies.push(t0.elapsed().as_secs_f64() * 1_000_000.0);
    }

    let b_p50 = percentile(baseline_search_latencies.clone(), 0.50);
    let b_p95 = percentile(baseline_search_latencies.clone(), 0.95);
    let b_p99 = percentile(baseline_search_latencies.clone(), 0.99);
    let b_mean = baseline_search_latencies.iter().sum::<f64>() / num_searches as f64;

    println!("Baseline Search Latency (Idle Index, 50k vectors):");
    println!(
        "  Mean: {:.1} µs, P50: {:.1} µs, P95: {:.1} µs, P99: {:.1} µs",
        b_mean, b_p50, b_p95, b_p99
    );

    // Mixed Workload: 16 Writer Tasks inserting 1,000 vectors while Reader Tasks perform 500 searches
    let extra_vectors_arc = std::sync::Arc::new(extra_vectors.clone());
    let idx_writer = std::sync::Arc::clone(&mixed_index_arc);
    let vecs_writer = std::sync::Arc::clone(&extra_vectors_arc);

    let num_writers = 16;
    let items_per_writer = extra_n / num_writers;
    let mut writer_handles = Vec::with_capacity(num_writers);

    let idx_reader = std::sync::Arc::clone(&mixed_index_arc);
    let queries_reader = query_vectors.clone();

    let reader_handle = tokio::spawn(async move {
        let mut mixed_search_latencies = Vec::with_capacity(num_searches);
        for q in &queries_reader {
            let t0 = Instant::now();
            let _ = idx_reader.search(q, 10).await.unwrap();
            mixed_search_latencies.push(t0.elapsed().as_secs_f64() * 1_000_000.0);
        }
        mixed_search_latencies
    });

    for w_idx in 0..num_writers {
        let idx_ref = std::sync::Arc::clone(&idx_writer);
        let vecs = std::sync::Arc::clone(&vecs_writer);
        let start = w_idx * items_per_writer;
        let end = if w_idx == num_writers - 1 {
            extra_n
        } else {
            start + items_per_writer
        };

        writer_handles.push(tokio::spawn(async move {
            for i in start..end {
                let tx = TxId::new(((w_idx * 100_000) + i + 200_000) as u64);
                let doc_id = DocId::new((60_000 + i + 1) as u64);
                idx_ref.insert(tx, doc_id, &vecs[i]).await.unwrap();
                idx_ref.commit(tx).await.unwrap();
            }
        }));
    }

    for handle in writer_handles {
        handle.await.unwrap();
    }

    let mixed_search_latencies = reader_handle.await.unwrap();

    let m_p50 = percentile(mixed_search_latencies.clone(), 0.50);
    let m_p95 = percentile(mixed_search_latencies.clone(), 0.95);
    let m_p99 = percentile(mixed_search_latencies.clone(), 0.99);
    let m_mean = mixed_search_latencies.iter().sum::<f64>() / num_searches as f64;

    let degradation_factor = m_mean / b_mean;

    println!("\nMixed Workload Search Latency (Under 16 Parallel Insert Tasks):");
    println!(
        "  Mean: {:.1} µs, P50: {:.1} µs, P95: {:.1} µs, P99: {:.1} µs",
        m_mean, m_p50, m_p95, m_p99
    );
    println!(
        "  Search Latency Degradation Factor: {:.2}x (under concurrent write load)",
        degradation_factor
    );

    println!("\n===============================================================================");
    println!("                          AUDIT BENCHMARKS COMPLETE                            ");
    println!("===============================================================================");
}
