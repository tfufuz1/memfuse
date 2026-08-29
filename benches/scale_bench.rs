// ANCHOR[PERF:BENCH-002] STATUS:DONE (TS:2026-08-29T00:00:00Z) — Realistic-Scale Throughput & Latency Benchmark
// ZIEL: Parameterisierte Messung (10K, 100K, 1M Chunks) von Batch-Insert-Durchsatz, Hybrid-Search-Latenz und Peak-RSS
// AGENT:09 DATE:2026-08-29 STATUS:DONE

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use memfuse_db::MemFuse;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::SystemTime;
use tempfile::TempDir;
use tokio::runtime::Runtime;

const EMBEDDING_DIM: usize = 768;

/// Helper function to sample VmRSS (Peak / Current Resident Set Size) from /proc/self/status on Linux
fn get_vm_rss_kb() -> Option<u64> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if line.starts_with("VmRSS:") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                return parts[1].parse::<u64>().ok();
            }
        }
    }
    None
}

/// Helper function to record RSS measurements to benches/results/scale_rss.csv
fn log_rss_measurement(stage: &str, num_chunks: usize, rss_kb: u64) {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let workspace_root = Path::new(&manifest_dir)
        .parent()
        .and_then(|p| p.parent())
        .unwrap_or_else(|| Path::new("."));

    let dir = if workspace_root.join("benches").exists() {
        workspace_root.join("benches").join("results")
    } else {
        Path::new("benches").join("results")
    };

    let _ = fs::create_dir_all(&dir);
    let csv_path = dir.join("scale_rss.csv");

    let needs_header = !csv_path.exists();
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&csv_path) {
        if needs_header {
            let _ = writeln!(file, "timestamp_secs,stage,num_chunks,vm_rss_kb,vm_rss_mb");
        }
        let timestamp_secs = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let rss_mb = (rss_kb as f64) / 1024.0;
        let _ = writeln!(
            file,
            "{},{},{},{},{:.2}",
            timestamp_secs, stage, num_chunks, rss_kb, rss_mb
        );
    }
}

/// Deterministically generates synthetic 768-dim embeddings with realistic variation
fn generate_embedding(doc_idx: usize) -> Vec<f32> {
    let mut vec = vec![0.0f32; EMBEDDING_DIM];
    let cluster = doc_idx % 20;
    let base_val = (cluster as f32) * 0.05;
    for (i, elem) in vec.iter_mut().enumerate() {
        let noise = ((doc_idx * 31 + i * 17) % 1000) as f32 / 10000.0;
        *elem = base_val + noise;
    }
    vec
}

/// Deterministically generates realistic multi-topic document content
fn generate_content(doc_idx: usize) -> String {
    let topics = [
        "quantum computing breakthroughs and superconducting qubits",
        "distributed consensus algorithms in memory-dense database clusters",
        "morphological tokenization and inverted index optimization techniques",
        "vector database indexing with HNSW and DiskANN graph navigation",
        "neural retrieval augmented generation with hybrid dense sparse fusion",
        "high throughput asynchronous IO and zero copy serialization in Rust",
        "checkpoint recovery protocols for persistent agent memory stores",
        "approximate nearest neighbor search evaluation metrics recall and precision",
    ];
    let topic = topics[doc_idx % topics.len()];
    format!(
        "Document chunk {:07}: Detailed technical report regarding {}. Additional context payload index {}.",
        doc_idx, topic, doc_idx
    )
}

fn bench_scale_inserts_and_search(c: &mut Criterion) {
    let rt = Runtime::new().unwrap(); // unwrap allowed

    // Scale tiers: 10_000, 100_000, 1_000_000 (1M) by default.
    // Can be overridden via MEMFUSE_SCALE_TIERS env var (e.g. "100,1000,5000" for fast test runs).
    let scale_levels: Vec<usize> = match std::env::var("MEMFUSE_SCALE_TIERS") {
        Ok(val) => val
            .split(',')
            .filter_map(|s| s.trim().parse::<usize>().ok())
            .collect(),
        Err(_) => vec![10_000, 100_000, 1_000_000],
    };

    let mut group = c.benchmark_group("scale_performance");
    group.sample_size(10);

    for &num_chunks in &scale_levels {
        group.throughput(Throughput::Elements(num_chunks as u64));

        let tmp = TempDir::new().unwrap(); // unwrap allowed
        let db = rt.block_on(MemFuse::open(tmp.path())).unwrap(); // unwrap allowed

        if let Some(rss_kb) = get_vm_rss_kb() {
            log_rss_measurement("before_insert", num_chunks, rss_kb);
        }

        // Measure batch population
        let start_time = std::time::Instant::now();
        rt.block_on(async {
            for i in 0..num_chunks {
                let doc_id = format!("chunk-{:07}", i);
                let vector = generate_embedding(i);
                let text = generate_content(i);
                db.insert(
                    &doc_id,
                    &vector,
                    Some(serde_json::json!({ "text": text, "chunk_index": i })),
                )
                .await
                .unwrap(); // unwrap allowed
            }
        });
        let duration = start_time.elapsed();
        let docs_per_sec = (num_chunks as f64) / duration.as_secs_f64();
        println!(
            "\n[SCALE] Populated {} chunks in {:.2}s ({:.1} docs/sec)",
            num_chunks,
            duration.as_secs_f64(),
            docs_per_sec
        );

        if let Some(rss_kb) = get_vm_rss_kb() {
            log_rss_measurement("after_insert", num_chunks, rss_kb);
            println!(
                "[SCALE] VmRSS after inserting {} chunks: {:.2} MB",
                num_chunks,
                (rss_kb as f64) / 1024.0
            );
        }

        // Benchmark hybrid_search latency on populated corpus
        let query_vec = generate_embedding(42);
        let query_text = "vector database indexing HNSW search";

        group.bench_with_input(
            BenchmarkId::new("hybrid_search", num_chunks),
            &num_chunks,
            |b, _| {
                b.to_async(&rt).iter(|| async {
                    let _res = db
                        .hybrid_search(query_text, &query_vec, 10, None)
                        .await
                        .unwrap(); // unwrap allowed
                });
            },
        );

        if let Some(rss_kb) = get_vm_rss_kb() {
            log_rss_measurement("after_search", num_chunks, rss_kb);
        }
    }

    group.finish();
}

criterion_group!(benches, bench_scale_inserts_and_search);
criterion_main!(benches);
