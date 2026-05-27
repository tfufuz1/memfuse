//! Integration tests for CsrGraph and GraphIndex traversal.
// ANCHOR:INTEGRATION:GRAPH-001 STATUS:READY AGENT:12 DATE:2026-06-20

use memfuse_core::{Edge, Entity, EntityId, GraphIndex, TxId};
use memfuse_graph::CsrGraph;

#[tokio::test]
async fn test_graph_diamond_topology_score_decay() {
    let graph = CsrGraph::new();
    let tx = TxId::new(1);

    // Diamond topology:
    //      (2)
    //    /     \
    // (1)       (4)
    //    \     /
    //      (3)

    let entities = vec![
        (1, "A"), (2, "B"), (3, "C"), (4, "D")
    ];

    for (id, name) in entities {
        graph.add_entity(tx, Entity::new(EntityId::new(id), name, "Node")).await.unwrap();
    }

    // Edges
    graph.add_edge(tx, Edge::new(EntityId::new(1), EntityId::new(2), "link")).await.unwrap();
    graph.add_edge(tx, Edge::new(EntityId::new(1), EntityId::new(3), "link")).await.unwrap();
    graph.add_edge(tx, Edge::new(EntityId::new(2), EntityId::new(4), "link")).await.unwrap();
    graph.add_edge(tx, Edge::new(EntityId::new(3), EntityId::new(4), "link")).await.unwrap();

    // Traverse from 1
    let results = graph.traverse(EntityId::new(1), 2).await.expect("traverse failed");

    // Should find 2, 3, 4
    assert_eq!(results.len(), 3);

    let score_map: std::collections::HashMap<_, _> = results.into_iter().collect();

    // Hop 1: Score = 1.0 * 0.7 = 0.7
    let s2 = *score_map.get(&EntityId::new(2)).unwrap();
    let s3 = *score_map.get(&EntityId::new(3)).unwrap();
    assert!((s2 - 0.7).abs() < 1e-6);
    assert!((s3 - 0.7).abs() < 1e-6);

    // Hop 2: Score = 0.7 * 0.7 = 0.49
    let s4 = *score_map.get(&EntityId::new(4)).unwrap();
    assert!((s4 - 0.49).abs() < 1e-6);
}

#[tokio::test]
async fn test_graph_cycle_integrity() {
    let graph = CsrGraph::new();
    let tx = TxId::new(1);

    // Cycle: 1 -> 2 -> 3 -> 1
    for id in 1..=3 {
        graph.add_entity(tx, Entity::new(EntityId::new(id), format!("N{}", id), "Node")).await.unwrap();
    }

    graph.add_edge(tx, Edge::new(EntityId::new(1), EntityId::new(2), "next")).await.unwrap();
    graph.add_edge(tx, Edge::new(EntityId::new(2), EntityId::new(3), "next")).await.unwrap();
    graph.add_edge(tx, Edge::new(EntityId::new(3), EntityId::new(1), "next")).await.unwrap();

    // Traverse from 1 with high max_hops
    let results = graph.traverse(EntityId::new(1), 10).await.expect("traverse failed");

    // Should only contain 2 and 3 (start node 1 is excluded)
    assert_eq!(results.len(), 2);
    assert!(results.iter().any(|(id, _)| *id == EntityId::new(2)));
    assert!(results.iter().any(|(id, _)| *id == EntityId::new(3)));
}

#[tokio::test]
async fn test_graph_stats_e2e() {
    let graph = CsrGraph::new();
    let tx = TxId::new(1);

    graph.add_entity(tx, Entity::new(EntityId::new(1), "A", "T")).await.unwrap();
    graph.add_entity(tx, Entity::new(EntityId::new(2), "B", "T")).await.unwrap();
    graph.add_edge(tx, Edge::new(EntityId::new(1), EntityId::new(2), "E")).await.unwrap();

    let stats = graph.stats().await.expect("stats failed");
    assert_eq!(stats.num_entities, 2);
    assert_eq!(stats.num_edges, 1);
    assert!(stats.memory_usage_bytes > 0);
}
