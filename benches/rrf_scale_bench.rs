use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use memfuse_db::fusion::weighted_reciprocal_rank_fusion;
use memfuse_db::SearchResult;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime};

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

/// Helper function to record RSS measurements to benches/results/rrf_scale_rss.csv
fn log_rss_measurement(
    stage: &str,
    hits_per_signal: usize,
    total_hits: usize,
    rss_kb: u64,
    latency_micros: f64,
) {
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
    let csv_path = dir.join("rrf_scale_rss.csv");

    let needs_header = !csv_path.exists();
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&csv_path) {
        if needs_header {
            let _ = writeln!(file, "timestamp_secs,stage,hits_per_signal,total_hits,vm_rss_kb,vm_rss_mb,latency_micros");
        }
        let timestamp_secs = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let rss_mb = (rss_kb as f64) / 1024.0;
        let _ = writeln!(
            file,
            "{},{},{},{},{},{:.2},{:.2}",
            timestamp_secs, stage, hits_per_signal, total_hits, rss_kb, rss_mb, latency_micros
        );
    }
}

fn generate_signal_results(
    signal_name: &str,
    count: usize,
    overlap_factor: usize,
) -> Vec<SearchResult> {
    let mut results = Vec::with_capacity(count);
    for i in 0..count {
        // overlap_factor controls how many IDs collide across signals vs unique
        let doc_id = if i % overlap_factor == 0 {
            format!("shared_doc_{:07}", i)
        } else {
            format!("{}_doc_{:07}", signal_name, i)
        };
        results.push(SearchResult {
            id: doc_id,
            score: 1.0 / ((i + 1) as f32),
            metadata: Some(serde_json::json!({
                "signal": signal_name,
                "rank": i,
                "payload": "Sample metadata string payload to simulate JSON object overhead in real applications."
            })),
            matched_signals: vec![],
        });
    }
    results
}

fn bench_rrf_scaling(c: &mut Criterion) {
    let hit_counts: Vec<usize> = match std::env::var("MEMFUSE_RRF_TIERS") {
        Ok(val) => val
            .split(',')
            .filter_map(|s| s.trim().parse::<usize>().ok())
            .collect(),
        Err(_) => vec![1_000, 10_000, 100_000, 500_000],
    };

    let signals = ["vector", "text", "graph", "hybrid"];
    let max_results = 10;

    let mut group = c.benchmark_group("rrf_fusion_scaling");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(5));

    for &count in &hit_counts {
        let total_input_hits = count * signals.len();
        group.throughput(Throughput::Elements(total_input_hits as u64));

        // Generate synthetic signal result sets
        let result_sets: Vec<(String, Vec<SearchResult>, f32)> = signals
            .iter()
            .map(|&sig| (sig.to_string(), generate_signal_results(sig, count, 5), 1.0))
            .collect();

        // Measure RSS before fusion call
        let rss_before = get_vm_rss_kb().unwrap_or(0);

        // Standalone latency measurement (warmup + multiple runs)
        let mut dur_samples = Vec::new();
        for _ in 0..5 {
            let sets_clone = result_sets.clone();
            let start = Instant::now();
            let fused = weighted_reciprocal_rank_fusion(sets_clone, max_results);
            let elapsed = start.elapsed();
            assert_eq!(fused.len(), max_results);
            dur_samples.push(elapsed);
        }

        let rss_after = get_vm_rss_kb().unwrap_or(0);
        let peak_rss_kb = rss_before.max(rss_after);
        let avg_latency_micros = dur_samples
            .iter()
            .map(|d| d.as_secs_f64() * 1_000_000.0)
            .sum::<f64>()
            / dur_samples.len() as f64;

        log_rss_measurement(
            "rrf_fusion",
            count,
            total_input_hits,
            peak_rss_kb,
            avg_latency_micros,
        );

        println!(
            "\n[RRF-BENCH] Hits/Signal: {:7} | Total Hits: {:7} | Latency: {:10.2} µs ({:7.2} ms) | Peak RSS: {:.2} MB",
            count,
            total_input_hits,
            avg_latency_micros,
            avg_latency_micros / 1000.0,
            (peak_rss_kb as f64) / 1024.0
        );

        group.bench_with_input(BenchmarkId::new("weighted_rrf", count), &count, |b, _| {
            b.iter_with_setup(
                || result_sets.clone(),
                |sets| weighted_reciprocal_rank_fusion(sets, max_results),
            );
        });
    }

    group.finish();
}

criterion_group!(benches, bench_rrf_scaling);
criterion_main!(benches);
