use memfuse_core::{Entity, EntityId, GraphIndex};
use memfuse_graph::csr::{CsrGraph, CsrGraphConfig};
use std::sync::Arc;
use std::time::Instant;

#[tokio::test]
async fn bench_single_compaction_scaling() {
    println!("\n=== BENCHMARK 1: Single Compaction Latency vs. Graph Size (Fixed 1,000 Pending Edges) ===");
    let sizes = vec![1_000, 10_000, 100_000, 1_000_000];
    let pending_to_add = 1_000;

    for &num_committed in &sizes {
        // Create graph with very large rebuild_threshold so auto-compaction is not triggered during prep
        let graph = Arc::new(CsrGraph::with_config(CsrGraphConfig {
            rebuild_threshold: usize::MAX,
        }));

        // Populate committed entities and edges
        let num_nodes = num_committed.max(2);
        for i in 1..=num_nodes {
            let _ = graph.insert_entity_direct(Entity::new(
                EntityId::new(i as u64),
                format!("Node_{i}"),
                "Entity",
            ));
        }

        // Add committed edges directly
        for i in 1..num_committed {
            let src = (i % num_nodes) + 1;
            let dst = ((i + 1) % num_nodes) + 1;
            graph
                .insert_edge_direct(EntityId::new(src as u64), EntityId::new(dst as u64), 1.0)
                .await
                .unwrap();
        }

        // Force compact to establish main CSR arrays with `num_committed` edges
        graph.compact();

        // Verify CSR targets size
        let initial_csr_edges = graph.stats().await.unwrap().num_edges;

        // Stage fixed pending edges into delta buffer
        for j in 0..pending_to_add {
            let src = (j % num_nodes) + 1;
            let dst = ((j + 7) % num_nodes) + 1;
            graph
                .insert_edge_direct(EntityId::new(src as u64), EntityId::new(dst as u64), 0.5)
                .await
                .unwrap();
        }

        // Measure single `compact()` latency over multiple warm runs / samples
        let start = Instant::now();
        graph.compact();
        let elapsed = start.elapsed();

        let final_edges = graph.stats().await.unwrap().num_edges;

        println!(
            "Graph Size (Committed Edges): {:>9} | CSR Initial: {:>9} | Pending: {:>5} | Final Edges: {:>9} | Compaction Latency: {:>10.3?} ({:>8.3} ms)",
            num_committed,
            initial_csr_edges,
            pending_to_add,
            final_edges,
            elapsed,
            elapsed.as_secs_f64() * 1000.0
        );
    }
}

#[tokio::test]
async fn bench_amortized_1m_edge_inserts() {
    println!("\n=== BENCHMARK 2: Amortized 1 Million Sequential Edge Inserts (rebuild_threshold = 1000) ===");
    let total_inserts: usize = 1_000_000;
    let rebuild_threshold = 1000;

    let graph = Arc::new(CsrGraph::with_config(CsrGraphConfig { rebuild_threshold }));

    let num_nodes: u64 = 100_000; // 100k distinct nodes
    for i in 1..=num_nodes {
        let _ = graph.insert_entity_direct(Entity::new(
            EntityId::new(i),
            format!("Node_{i}"),
            "Entity",
        ));
    }

    let mut latencies_nanos: Vec<u64> = Vec::with_capacity(total_inserts);
    let mut compaction_latencies_ms: Vec<f64> = Vec::new();

    let benchmark_start = Instant::now();

    for i in 0..total_inserts {
        let src = EntityId::new((i as u64 % num_nodes) + 1);
        let dst = EntityId::new(((i as u64 * 3 + 7) % num_nodes) + 1);

        let t0 = Instant::now();
        graph.insert_edge_direct(src, dst, 1.0).await.unwrap();
        let elapsed_nanos = t0.elapsed().as_nanos() as u64;
        latencies_nanos.push(elapsed_nanos);

        // Track compaction spikes (when latency exceeds 100 microseconds / 100,000 ns)
        if elapsed_nanos > 100_000 {
            compaction_latencies_ms.push(elapsed_nanos as f64 / 1_000_000.0);
        }
    }

    let total_elapsed = benchmark_start.elapsed();
    let throughput = total_inserts as f64 / total_elapsed.as_secs_f64();
    let avg_latency_ns = latencies_nanos.iter().sum::<u64>() as f64 / total_inserts as f64;

    // Sort for percentiles
    latencies_nanos.sort_unstable();

    let p50 = latencies_nanos[total_inserts * 50 / 100];
    let p95 = latencies_nanos[total_inserts * 95 / 100];
    let p99 = latencies_nanos[total_inserts * 99 / 100];
    let p99_9 = latencies_nanos[total_inserts * 999 / 1000];
    let p99_99 = latencies_nanos[total_inserts * 9999 / 10000];
    let max = *latencies_nanos.last().unwrap();

    println!("Total Edges Inserted: {}", total_inserts);
    println!("Total Elapsed Time:   {:.3?}", total_elapsed);
    println!("Average Throughput:   {:.2} ops/sec", throughput);
    println!(
        "Average Latency:      {:.3} µs ({:.1} ns)",
        avg_latency_ns / 1000.0,
        avg_latency_ns
    );
    println!("\nPercentile Distribution:");
    println!("  p50:    {:>8.3} µs ({:>7} ns)", p50 as f64 / 1000.0, p50);
    println!("  p95:    {:>8.3} µs ({:>7} ns)", p95 as f64 / 1000.0, p95);
    println!("  p99:    {:>8.3} µs ({:>7} ns)", p99 as f64 / 1000.0, p99);
    println!(
        "  p99.9:  {:>8.3} µs ({:>7} ns)",
        p99_9 as f64 / 1000.0,
        p99_9
    );
    println!(
        "  p99.99: {:>8.3} µs ({:>7} ns)",
        p99_99 as f64 / 1000.0,
        p99_99
    );
    println!(
        "  max:    {:>8.3} ms ({:>7} µs)",
        max as f64 / 1_000_000.0,
        max as f64 / 1000.0
    );

    println!("\nCompaction Spike Analysis:");
    println!(
        "  Total Compaction Spikes Triggered: {}",
        compaction_latencies_ms.len()
    );
    if !compaction_latencies_ms.is_empty() {
        let avg_spike =
            compaction_latencies_ms.iter().sum::<f64>() / compaction_latencies_ms.len() as f64;
        let max_spike = compaction_latencies_ms.iter().cloned().fold(0.0, f64::max);
        println!("  Average Spike Duration: {:.3} ms", avg_spike);
        println!("  Maximum Spike Duration: {:.3} ms", max_spike);
    }
}
