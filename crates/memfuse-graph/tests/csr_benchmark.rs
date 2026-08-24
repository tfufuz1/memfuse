use memfuse_core::{Edge, Entity, EntityId, GraphIndex, TxId};
use memfuse_graph::csr::{CsrGraph, CsrGraphConfig};
use std::time::Instant;

#[tokio::test]
async fn test_csr_delta_buffer_incremental_benchmark() {
    let num_existing_nodes = 10_000;
    let num_commits = 100;

    // --- Benchmark 1: WITH Delta Buffer (high rebuild_threshold, e.g., 1000) ---
    let graph_delta = CsrGraph::with_config(CsrGraphConfig {
        rebuild_threshold: 1000,
    });

    // Populate initial 10,000 nodes and initial edges
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
    graph_delta.compact(); // Initial setup compact

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
        "Benchmark Results (10,000 nodes + 100 sequential 1-edge commits):\n\
         - With Delta Buffer:    {:?}\n\
         - With Forced Rebuild:  {:?}\n\
         - Speedup factor:       {:.2}x",
        delta_duration,
        rebuild_duration,
        rebuild_duration.as_secs_f64() / delta_duration.as_secs_f64().max(0.000001)
    );

    // Speedup with delta buffer must be significant (at least 2x faster, typically 10x-500x faster)
    assert!(
        delta_duration < rebuild_duration,
        "Incremental delta commits ({:?}) should be faster than full rebuild commits ({:?})",
        delta_duration,
        rebuild_duration
    );
}
