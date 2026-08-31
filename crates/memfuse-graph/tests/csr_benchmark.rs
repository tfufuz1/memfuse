//! Comprehensive Benchmark Suite for `memfuse-graph`.
//!
//! Measures:
//! 1. BFS Traversal Latency vs Scale (1K, 10K, 100K nodes)
//! 2. PPR Convergence Runtime vs Scale (1K, 10K, 100K nodes)
//! 3. Community Detection Runtime vs Scale (1K, 10K, 100K nodes)
//! 4. Edge Insertion Throughput (Delta Buffer vs Forced CSR Rebuild)
//! 5. Session-DAG Branch Operation Latency vs Depth/Breadth

use memfuse_core::{Edge, Entity, EntityId, GraphIndex, PprConfig, TxId};
use memfuse_graph::{detect_communities, CommunityDetectionConfig, CsrGraph, SessionBranchTree};
use memfuse_graph::csr::CsrGraphConfig;
use std::sync::Arc;
use std::time::Instant;

#[tokio::test]
async fn test_csr_delta_buffer_incremental_benchmark() {
    let num_existing_nodes = 10_000;
    let num_commits = 100;

    // --- Benchmark 1: WITH Delta Buffer (rebuild_threshold = 1000) ---
    let graph_delta = CsrGraph::with_config(CsrGraphConfig {
        rebuild_threshold: 1000,
    });

    let setup_tx = TxId::new(1);
    for i in 1..=num_existing_nodes {
        graph_delta
            .add_entity(
                setup_tx,
                Entity::new(EntityId::new(i), format!("Node_{i}"), "Entity"),
            )
            .await
            .unwrap();
    }
    for i in 1..num_existing_nodes {
        graph_delta
            .add_edge(
                setup_tx,
                Edge::new(EntityId::new(i), EntityId::new(i + 1), "link").with_weight(0.9),
            )
            .await
            .unwrap();
    }
    graph_delta.commit(setup_tx).await.unwrap();
    graph_delta.compact();

    let delta_start = Instant::now();
    for commit_idx in 1..=num_commits {
        let tx = TxId::new(100 + commit_idx as u64);
        let from_node = commit_idx as u64;
        let to_node = (commit_idx + 1000) as u64;
        graph_delta
            .add_edge(
                tx,
                Edge::new(EntityId::new(from_node), EntityId::new(to_node), "new_link")
                    .with_weight(0.8),
            )
            .await
            .unwrap();
        graph_delta.commit(tx).await.unwrap();
    }
    let delta_duration = delta_start.elapsed();

    // Verify traversal correctness with delta buffer
    let traverse_results = graph_delta.traverse(EntityId::new(1), 2).await.unwrap();
    assert!(!traverse_results.is_empty(), "Traversal must return nodes");

    // --- Benchmark 2: WITHOUT Delta Buffer (rebuild_threshold = 0, full rebuild on every commit) ---
    let graph_rebuild = CsrGraph::with_config(CsrGraphConfig {
        rebuild_threshold: 0,
    });

    let setup_tx2 = TxId::new(1);
    for i in 1..=num_existing_nodes {
        graph_rebuild
            .add_entity(
                setup_tx2,
                Entity::new(EntityId::new(i), format!("Node_{i}"), "Entity"),
            )
            .await
            .unwrap();
    }
    for i in 1..num_existing_nodes {
        graph_rebuild
            .add_edge(
                setup_tx2,
                Edge::new(EntityId::new(i), EntityId::new(i + 1), "link").with_weight(0.9),
            )
            .await
            .unwrap();
    }
    graph_rebuild.commit(setup_tx2).await.unwrap();
    graph_rebuild.compact();

    let rebuild_start = Instant::now();
    for commit_idx in 1..=num_commits {
        let tx = TxId::new(100 + commit_idx as u64);
        let from_node = commit_idx as u64;
        let to_node = (commit_idx + 1000) as u64;
        graph_rebuild
            .add_edge(
                tx,
                Edge::new(EntityId::new(from_node), EntityId::new(to_node), "new_link")
                    .with_weight(0.8),
            )
            .await
            .unwrap();
        graph_rebuild.commit(tx).await.unwrap();
    }
    let rebuild_duration = rebuild_start.elapsed();

    println!(
        "\n[BENCHMARK] Edge Insert Throughput (10,000 nodes + 100 sequential 1-edge commits):\n\
         - With Delta Buffer:    {:?}\n\
         - With Forced Rebuild:  {:?}\n\
         - Speedup factor:       {:.2}x",
        delta_duration,
        rebuild_duration,
        rebuild_duration.as_secs_f64() / delta_duration.as_secs_f64().max(0.000001)
    );

    assert!(
        delta_duration < rebuild_duration,
        "Incremental delta commits ({:?}) should be faster than full rebuild commits ({:?})",
        delta_duration,
        rebuild_duration
    );
}

#[tokio::test]
async fn bench_bfs_traversal_latency_scale() {
    println!("\n[BENCHMARK] BFS Traversal Latency vs Graph Scale");
    println!("Node Count | Edges Count | Hops | Traversal Time | Results Count");
    println!("------------------------------------------------------------------");

    for &num_nodes in &[1_000usize, 10_000usize, 100_000usize] {
        let graph = CsrGraph::new();
        let tx = TxId::new(1);

        for i in 1..=num_nodes {
            graph
                .add_entity(
                    tx,
                    Entity::new(EntityId::new(i as u64), format!("N{i}"), "Type"),
                )
                .await
                .unwrap();
        }

        // Add 3 outgoing edges per node (sparse graph)
        for i in 1..=num_nodes {
            let u = i as u64;
            for neighbor_offset in 1..=3 {
                let v = ((i + neighbor_offset - 1) % num_nodes + 1) as u64;
                graph
                    .add_edge(
                        tx,
                        Edge::new(EntityId::new(u), EntityId::new(v), "link").with_weight(0.8),
                    )
                    .await
                    .unwrap();
            }
        }
        graph.commit(tx).await.unwrap();
        graph.compact();

        let edge_count = graph.edge_count();

        for hops in 1..=3 {
            let start_time = Instant::now();
            let results = graph.traverse(EntityId::new(1), hops).await.unwrap();
            let elapsed = start_time.elapsed();

            println!(
                "{:10} | {:11} | {:4} | {:14?} | {:13}",
                num_nodes,
                edge_count,
                hops,
                elapsed,
                results.len()
            );
        }
    }
}

#[tokio::test]
async fn bench_ppr_convergence_runtime_scale() {
    println!("\n[BENCHMARK] Personalized PageRank Convergence Runtime vs Scale");
    println!("Node Count | Edges Count | Damping | Max Iters | Runtime   | Total Rank Mass");
    println!("------------------------------------------------------------------------------");

    for &num_nodes in &[1_000usize, 10_000usize, 50_000usize] {
        let graph = CsrGraph::new();
        let tx = TxId::new(1);

        for i in 1..=num_nodes {
            graph
                .add_entity(
                    tx,
                    Entity::new(EntityId::new(i as u64), format!("N{i}"), "Type"),
                )
                .await
                .unwrap();
        }

        // Ring topology + random cross links
        for i in 1..=num_nodes {
            let u = i as u64;
            let v_next = (i % num_nodes + 1) as u64;
            graph
                .add_edge(
                    tx,
                    Edge::new(EntityId::new(u), EntityId::new(v_next), "link").with_weight(1.0),
                )
                .await
                .unwrap();

            if i % 10 == 0 {
                let v_cross = ((i + 500) % num_nodes + 1) as u64;
                graph
                    .add_edge(
                        tx,
                        Edge::new(EntityId::new(u), EntityId::new(v_cross), "cross").with_weight(0.5),
                    )
                    .await
                    .unwrap();
            }
        }
        graph.commit(tx).await.unwrap();
        graph.compact();

        let edge_count = graph.edge_count();
        let config = PprConfig {
            damping_factor: 0.85,
            max_iterations: 100,
            convergence_epsilon: 1e-6,
            warn_on_non_convergence: true,
        };

        let start_time = Instant::now();
        let results = graph
            .personalized_page_rank(&[EntityId::new(1)], &config)
            .await
            .unwrap();
        let elapsed = start_time.elapsed();

        let total_mass: f32 = results.iter().map(|(_, r)| r).sum();

        println!(
            "{:10} | {:11} | {:7.2} | {:9} | {:9?} | {:15.6}",
            num_nodes, edge_count, config.damping_factor, config.max_iterations, elapsed, total_mass
        );
    }
}

#[tokio::test]
async fn bench_community_detection_runtime_scale() {
    println!("\n[BENCHMARK] Community Detection Runtime vs Scale");
    println!("Node Count | Edges Count | Max Iters | Runtime   | Detected Communities");
    println!("-------------------------------------------------------------------------");

    for &num_nodes in &[1_000usize, 5_000usize, 20_000usize] {
        let graph = CsrGraph::new();
        let tx = TxId::new(1);

        for i in 1..=num_nodes {
            graph
                .add_entity(
                    tx,
                    Entity::new(EntityId::new(i as u64), format!("N{i}"), "Type"),
                )
                .await
                .unwrap();
        }

        // Create 10 dense clusters
        let cluster_size = num_nodes / 10;
        for c in 0..10 {
            let start = c * cluster_size + 1;
            let end = (c + 1) * cluster_size;
            for i in start..end {
                let u = i as u64;
                let v = if i == end { start as u64 } else { (i + 1) as u64 };
                graph
                    .add_edge(
                        tx,
                        Edge::new(EntityId::new(u), EntityId::new(v), "link"),
                    )
                    .await
                    .unwrap();
            }
        }
        graph.commit(tx).await.unwrap();
        graph.compact();

        let config = CommunityDetectionConfig {
            max_iterations: 30,
            seed: 42,
        };

        let start_time = Instant::now();
        let assignments = detect_communities(&graph, &config).await.unwrap();
        let elapsed = start_time.elapsed();

        let unique_communities: std::collections::HashSet<_> =
            assignments.iter().map(|a| a.community_id).collect();

        println!(
            "{:10} | {:11} | {:9} | {:9?} | {:20}",
            num_nodes,
            graph.edge_count(),
            config.max_iterations,
            elapsed,
            unique_communities.len()
        );
    }
}

#[tokio::test]
async fn bench_session_dag_branching_latency() {
    println!("\n[BENCHMARK] Session-DAG Branch Operation Latency vs Depth and Breadth");
    println!("Operations | Operation Type | Runtime   | Avg Latency / Op");
    println!("----------------------------------------------------------");

    let dag = Arc::new(SessionBranchTree::new("Root".into(), "Root Resp".into()));

    // 1. Deep Linear Chain (10,000 steps deep)
    let start_deep = Instant::now();
    for i in 1..=10_000 {
        let _ = dag.append_step(
            format!("Prompt {i}"),
            format!("Resp {i}"),
            None,
            vec![],
            "main",
        );
    }
    let elapsed_deep = start_deep.elapsed();
    let avg_deep = elapsed_deep / 10_000;

    println!(
        "{:10} | {:14} | {:9?} | {:16?}",
        10_000, "Linear Append", elapsed_deep, avg_deep
    );

    // 2. Wide Branching (10,000 branches off root node 0)
    let start_wide = Instant::now();
    for i in 1..=10_000 {
        let _ = dag.branch_from(
            0,
            format!("Branch Prompt {i}"),
            format!("Branch Resp {i}"),
            None,
            vec![],
            "explore",
        );
    }
    let elapsed_wide = start_wide.elapsed();
    let avg_wide = elapsed_wide / 10_000;

    println!(
        "{:10} | {:14} | {:9?} | {:16?}",
        10_000, "Grok Branching", elapsed_wide, avg_wide
    );
}
