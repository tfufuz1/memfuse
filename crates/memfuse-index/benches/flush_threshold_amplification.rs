// FILE-CONTEXT: Benchmark für Write-Amplification und p95-Latenz in DiskANN
// ZWECK: Quantifizierung von Schreibvolumen und Latenz-Trade-offs für PENDING_FLUSH_THRESHOLD (50 / 200 / 1.000)
// STAND: TS:2026-09-07T00:00:00Z

use std::fs;
use std::time::Instant;

#[cfg(feature = "experimental-diskann")]
use memfuse_core::traits::VectorIndex;
#[cfg(feature = "experimental-diskann")]
use memfuse_core::types::{DistanceMetric, DocId, TxId};
#[cfg(feature = "experimental-diskann")]
use memfuse_index::diskann::{DiskAnnConfig, DiskAnnIndex};
#[cfg(feature = "experimental-diskann")]
use rand::Rng;

fn percentile(mut latencies: Vec<f64>, p: f64) -> f64 {
    if latencies.is_empty() {
        return 0.0;
    }
    latencies.sort_by(|a, b| a.total_cmp(b));
    let idx = ((latencies.len() as f64 - 1.0) * p).round() as usize;
    latencies[idx]
}

#[cfg(feature = "experimental-diskann")]
struct BenchResult {
    collection_size: usize,
    threshold: u64,
    raw_payload_bytes: u64,
    total_disk_bytes: u64,
    write_amplification: f64,
    p95_latency_ms: f64,
    num_flushes: usize,
}

#[cfg(feature = "experimental-diskann")]
async fn run_benchmark_for_config(
    collection_size: usize,
    threshold: u64,
    num_inserts: usize,
    dim: usize,
) -> BenchResult {
    let temp_dir = tempfile::tempdir().expect("tempdir creation failed");
    let index_path = temp_dir.path().join("bench_diskann.idx");

    let config = DiskAnnConfig {
        index_path: index_path.clone(),
        dimension: dim,
        max_degree: 16,
        beam_width: 8,
        distance_metric: DistanceMetric::Euclidean,
        quantize: false,
        pending_flush_threshold: Some(threshold),
        ..Default::default()
    };

    let index = DiskAnnIndex::try_new(config).expect("DiskAnnIndex creation failed");

    let mut rng = rand::thread_rng();

    // 1. Base Collection generate and build
    let base_vectors: Vec<Vec<f32>> = (0..collection_size)
        .map(|_| (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect())
        .collect();
    let base_ids: Vec<DocId> = (1..=collection_size as u64).map(DocId::from).collect();

    index
        .build(&base_vectors, &base_ids)
        .await
        .expect("Initial build failed");

    // 2. Prepare insert workload
    let new_vectors: Vec<Vec<f32>> = (0..num_inserts)
        .map(|_| (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect())
        .collect();
    let new_ids: Vec<DocId> = ((collection_size + 1) as u64
        ..=(collection_size + num_inserts) as u64)
        .map(DocId::from)
        .collect();

    let mut total_bytes_written = 0u64;
    let mut latencies_ms = Vec::with_capacity(num_inserts);
    let mut num_flushes = 0;

    let wal_entry_bytes = (8 + 4 + dim * 4) as u64;
    let raw_payload_bytes = (num_inserts * dim * 4) as u64;

    let mut pending_count = 0u64;

    for i in 0..num_inserts {
        let doc_id = new_ids[i];
        let vec = &new_vectors[i];
        let tx = TxId::new((i + 1) as u64);

        let t0 = Instant::now();

        index.insert(tx, doc_id, vec).await.expect("Insert failed");
        total_bytes_written += wal_entry_bytes;
        pending_count += 1;

        if pending_count >= threshold || i == num_inserts - 1 {
            num_flushes += 1;
            index.persist_delta().await.expect("persist_delta failed");
            let flushed_file_size = fs::metadata(&index_path).map(|m| m.len()).unwrap_or(0);
            total_bytes_written += flushed_file_size;
            pending_count = 0;
        }

        let elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
        latencies_ms.push(elapsed_ms);
    }

    let p95_latency_ms = percentile(latencies_ms, 0.95);
    let write_amplification = total_bytes_written as f64 / raw_payload_bytes as f64;

    BenchResult {
        collection_size,
        threshold,
        raw_payload_bytes,
        total_disk_bytes: total_bytes_written,
        write_amplification,
        p95_latency_ms,
        num_flushes,
    }
}

#[tokio::main]
async fn main() {
    run_benchmark_suite().await;
}

async fn run_benchmark_suite() {
    #[cfg(not(feature = "experimental-diskann"))]
    {
        println!("Bitte '--features experimental-diskann' angeben, um den Benchmark auszuführen.");
        println!("Befehl: cargo bench -p memfuse-index --bench flush_threshold_amplification --features experimental-diskann");
        return;
    }

    #[cfg(feature = "experimental-diskann")]
    {
        println!("=========================================================================================");
        println!("         DISKANN PENDING_FLUSH_THRESHOLD WRITE AMPLIFICATION & LATENCY STUDY            ");
        println!("=========================================================================================");
        println!();

        let collection_sizes = [100, 1_000, 10_000, 100_000];
        let thresholds = [50u64, 200u64, 1_000u64];
        let dim = 64;

        let mut results = Vec::new();

        for &size in &collection_sizes {
            let num_inserts = match size {
                100 => 200,
                1_000 => 200,
                10_000 => 200,
                100_000 => 100,
                _ => 100,
            };

            println!(
                "--- Running Collection Size N = {} (Inserts = {}) ---",
                size, num_inserts
            );

            for &t in &thresholds {
                print!("  Testing PENDING_FLUSH_THRESHOLD = {:>4} ... ", t);
                let res = run_benchmark_for_config(size, t, num_inserts, dim).await;
                println!(
                    "Flushes: {:>2}, Disk: {:>8.2} MB, WA: {:>6.2}x, p95 Latency: {:>7.2} ms",
                    res.num_flushes,
                    res.total_disk_bytes as f64 / (1024.0 * 1024.0),
                    res.write_amplification,
                    res.p95_latency_ms
                );
                results.push(res);
            }
            println!();
        }

        println!("=========================================================================================");
        println!("                                    FINAL SUMMARY TABLE                                  ");
        println!("=========================================================================================");
        println!(
            "{:<12} | {:<10} | {:<12} | {:<12} | {:<8} | {:<14}",
            "Size (N)", "Threshold", "Disk Write", "Raw Payload", "WA Factor", "p95 Latency (ms)"
        );
        println!("-----------------------------------------------------------------------------------------");

        for r in &results {
            println!(
                "{:<12} | {:<10} | {:>9.2} MB | {:>9.2} MB | {:>7.2}x | {:>14.2}",
                r.collection_size,
                r.threshold,
                r.total_disk_bytes as f64 / (1024.0 * 1024.0),
                r.raw_payload_bytes as f64 / (1024.0 * 1024.0),
                r.write_amplification,
                r.p95_latency_ms
            );
        }
        println!("=========================================================================================");
    }
}
