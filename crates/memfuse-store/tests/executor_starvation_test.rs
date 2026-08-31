//! Empirical verification test for executor starvation under heavy I/O load.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use memfuse_store::sstable::{create_block_cache, SstableBuilder, SstableReader};

async fn run_starvation_benchmark(test_name: &str, num_entries: usize, payload_size: usize) {
    println!("\n=======================================================");
    println!("RUNNING TEST: {}", test_name);
    println!("Configuration: {} entries, {} bytes payload (~{} MB total)",
        num_entries, payload_size, (num_entries * payload_size) / (1024 * 1024));
    println!("=======================================================");

    let temp_dir = TempDir::new().expect("temp dir");
    let sst_path = temp_dir.path().join("heavy_io.sst");

    // 1. Monitor task setup
    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();

    let max_latency_ns = Arc::new(AtomicU64::new(0));
    let max_latency_ns_clone = max_latency_ns.clone();

    let total_samples = Arc::new(AtomicU64::new(0));
    let total_samples_clone = total_samples.clone();

    let sample_history = Arc::new(parking_lot::Mutex::new(Vec::new()));
    let sample_history_clone = sample_history.clone();

    let monitor_handle = tokio::spawn(async move {
        let expected_duration = Duration::from_millis(1);
        while running_clone.load(Ordering::Relaxed) {
            let start = Instant::now();
            tokio::time::sleep(expected_duration).await;
            let elapsed = start.elapsed();

            let elapsed_ns = elapsed.as_nanos() as u64;
            total_samples_clone.fetch_add(1, Ordering::Relaxed);

            // Track max latency
            let mut current_max = max_latency_ns_clone.load(Ordering::Relaxed);
            while elapsed_ns > current_max {
                match max_latency_ns_clone.compare_exchange_weak(
                    current_max,
                    elapsed_ns,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => break,
                    Err(actual) => current_max = actual,
                }
            }

            if sample_history_clone.lock().len() < 5000 {
                sample_history_clone.lock().push((start, elapsed));
            }
        }
    });

    // Let monitor spin up and collect baseline idle samples
    tokio::time::sleep(Duration::from_millis(100)).await;

    let payload = vec![0x5A; payload_size];
    let io_start = Instant::now();

    // 2. Heavy Write I/O Phase
    println!("[{}] Starting Heavy Write Phase...", test_name);
    let mut builder = SstableBuilder::create(&sst_path).await.expect("builder create");
    for i in 0..num_entries {
        let key = format!("key-{:08}", i);
        builder.add(key.as_bytes(), &payload, i as u64, 1).await.expect("builder add");
    }
    let metadata = builder.finish().await.expect("builder finish");
    let write_duration = io_start.elapsed();
    println!("[{}] Write Phase Finished in {:.2?}. File size: {} bytes", test_name, write_duration, metadata.file_size);

    // 3. Heavy Read I/O Phase (Random block access without cache)
    println!("[{}] Starting Heavy Read Phase (Cache Bypassed)...", test_name);
    let read_start = Instant::now();
    let bc = create_block_cache(1); // Small 1MB cache to force disk I/O
    let reader = SstableReader::open(&sst_path, bc).await.expect("reader open");

    for i in (0..num_entries).step_by(5) {
        let key = format!("key-{:08}", i);
        let res = reader.get(key.as_bytes()).await.expect("reader get");
        assert!(res.is_some());
    }
    let read_duration = read_start.elapsed();
    println!("[{}] Read Phase Finished in {:.2?}", test_name, read_duration);

    // Stop monitor task
    running.store(false, Ordering::Relaxed);
    let _ = monitor_handle.await;

    let total_s = total_samples.load(Ordering::Relaxed);
    let max_lat = Duration::from_nanos(max_latency_ns.load(Ordering::Relaxed));

    println!("\n--- [{}] RESULTS ---", test_name);
    println!("Total Monitor Samples: {}", total_s);
    println!("Max Light Task Latency: {:.3?} (Expected: ~1.0ms)", max_lat);

    // Print latency percentiles / sample tail
    let history = sample_history.lock();
    let mut latencies: Vec<Duration> = history.iter().map(|(_, dur)| *dur).collect();
    latencies.sort();

    if !latencies.is_empty() {
        let p50 = latencies[latencies.len() * 50 / 100];
        let p95 = latencies[latencies.len() * 95 / 100];
        let p99 = latencies[latencies.len() * 99 / 100];
        let p999 = latencies[latencies.len() * 999 / 1000];
        let max = latencies[latencies.len() - 1];

        println!("p50: {:.3?}", p50);
        println!("p95: {:.3?}", p95);
        println!("p99: {:.3?}", p99);
        println!("p99.9: {:.3?}", p999);
        println!("Max: {:.3?}", max);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_executor_starvation_multi_thread() {
    run_starvation_benchmark("Multi-Worker Runtime (4 Threads)", 20_000, 8_192).await;
}

#[tokio::test(flavor = "current_thread")]
async fn test_executor_starvation_single_thread() {
    run_starvation_benchmark("Single-Worker Runtime (current_thread)", 20_000, 8_192).await;
}
