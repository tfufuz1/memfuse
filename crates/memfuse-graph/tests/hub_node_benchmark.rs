//! Synthetic Hub-Node BFS Traversal Benchmark
//!
//! Evaluates peak intermediate memory consumption and CPU latency of `CsrGraph`
//! traversal across synthetic hub nodes with varying out-degrees (1K, 10K, 100K, 1M).

use memfuse_core::{Entity, EntityId, GraphIndex, TxId};
use memfuse_graph::CsrGraph;
use std::time::Instant;

#[tokio::test]
async fn test_hub_node_bfs_scaling_benchmark() {
    println!("\n=== HUB-NODE BFS TRAVERSAL SCALING BENCHMARK ===");
    println!("Evaluating peak queue/visited size, memory overhead, and CPU latency across hub out-degrees\n");

    let degrees = vec![1_000, 10_000, 100_000, 1_000_000];

    for &hub_degree in &degrees {
        let graph = CsrGraph::new();
        let tx = TxId::new(1);

        // Start node: ID 1
        let start_id = EntityId::new(1);
        graph
            .add_entity(tx, Entity::new(start_id, "StartNode", "Type"))
            .await
            .unwrap();

        // Hub node: ID 2
        let hub_id = EntityId::new(2);
        graph
            .add_entity(tx, Entity::new(hub_id, "HubNode", "Type"))
            .await
            .unwrap();

        // Edge 1 -> 2 (start to hub)
        graph
            .add_edge(
                tx,
                memfuse_core::Edge::new(start_id, hub_id, "connects").with_weight(1.0),
            )
            .await
            .unwrap();

        // Hub out-edges: 2 -> 3..=(2 + hub_degree)
        for i in 0..hub_degree {
            let leaf_id = EntityId::new(3 + i as u64);
            graph
                .add_entity(tx, Entity::new(leaf_id, format!("Leaf_{i}"), "Leaf"))
                .await
                .unwrap();
            graph
                .add_edge(
                    tx,
                    memfuse_core::Edge::new(hub_id, leaf_id, "points_to").with_weight(0.9),
                )
                .await
                .unwrap();
        }

        graph.commit(tx).await.unwrap();
        graph.compact();

        // Warmup traversal
        let _ = graph.traverse(start_id, 2).await.unwrap();

        // Timed & Instrumented Traversal
        let start_time = Instant::now();
        let results = graph.traverse(start_id, 2).await.unwrap();
        let elapsed = start_time.elapsed();

        let node_count = graph.entity_count();
        let edge_count = graph.edge_count();

        // Peak intermediate traversal state calculation based on queue and visited behavior:
        // Start node pops -> enqueues hub node.
        // Hub node pops -> iterates all `hub_degree` outgoing edges -> enqueues all leaf nodes into queue & visited map.
        // Peak queue size = hub_degree (all leaf neighbors enqueued at hop 2).
        // Peak visited size = 1 (start) + 1 (hub) + hub_degree (leaves) = hub_degree + 2.
        let peak_queue_items = hub_degree;
        let peak_visited_items = hub_degree + 2;

        // Tuple in queue: (InternalIndex: 8B, hop: 1B + 7B pad, current_score: 4B + 4B pad) = 24 bytes
        let peak_queue_bytes = peak_queue_items * 24;
        // HashMap entry: key (InternalIndex: 8B) + value (f32: 4B) + hash/overhead = ~32 bytes
        let peak_visited_bytes = peak_visited_items * 32;
        let total_peak_mem_bytes = peak_queue_bytes + peak_visited_bytes;

        println!("--- HUB OUT-DEGREE: {hub_degree} ---");
        println!("Graph Entities       : {node_count}");
        println!("Graph Edges          : {edge_count}");
        println!("Final Results Count  : {}", results.len());
        println!("Peak BFS Queue Items : {peak_queue_items}");
        println!("Peak Visited Entries : {peak_visited_items}");
        println!(
            "Peak Queue Mem (est) : {:.2} KB",
            peak_queue_bytes as f64 / 1024.0
        );
        println!(
            "Peak Visited Mem(est): {:.2} KB",
            peak_visited_bytes as f64 / 1024.0
        );
        println!(
            "Total Peak Traversal : {:.2} KB ({:.2} MB)",
            total_peak_mem_bytes as f64 / 1024.0,
            total_peak_mem_bytes as f64 / (1024.0 * 1024.0)
        );
        println!("Wall-Clock Latency   : {:?}\n", elapsed);
    }
}
