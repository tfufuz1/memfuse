// ANCHOR:INTEGRATION:GRAPH-001 STATUS:DONE AGENT:12 DATE:2026-06-21
//! Integration tests for CSR graph traversal.

use memfuse_core::{Edge, Entity, EntityId, GraphIndex, TxId};
use memfuse_graph::CsrGraph;

#[tokio::test]
async fn test_graph_bfs_and_scoring() {
    let graph = CsrGraph::new();
    let tx = TxId::new(1);

    // Create a chain: 1 --(1.0)--> 2 --(1.0)--> 3 --(1.0)--> 4
    for i in 1..=4 {
        graph
            .add_entity(
                tx,
                Entity::new(EntityId::new(i), format!("Node{}", i), "Test"),
            )
            .await
            .unwrap();
    }

    // 1 --(1.0)--> 2
    graph
        .add_edge(tx, Edge::new(EntityId::new(1), EntityId::new(2), "Next"))
        .await
        .unwrap();
    // 2 --(1.0)--> 3
    graph
        .add_edge(tx, Edge::new(EntityId::new(2), EntityId::new(3), "Next"))
        .await
        .unwrap();
    // 3 --(0.5)--> 4
    graph
        .add_edge(
            tx,
            Edge::new(EntityId::new(3), EntityId::new(4), "Weak").with_weight(0.5),
        )
        .await
        .unwrap();

    // Traverse from 1, max 3 hops
    // Hop 1: Node 2. Score = 1.0 * 0.7 * 1.0 = 0.7
    // Hop 2: Node 3. Score = 0.7 * 0.7 * 1.0 = 0.49
    // Hop 3: Node 4. Score = 0.49 * 0.7 * 0.5 = 0.1715

    let results = graph
        .traverse(EntityId::new(1), 3)
        .await
        .expect("Traversal failed");
    assert_eq!(results.len(), 3);

    let scores: std::collections::HashMap<u64, f32> =
        results.into_iter().map(|(id, s)| (id.inner(), s)).collect();

    // Verify scores with epsilon for float comparison
    assert!(
        (scores[&2] - 0.7).abs() < 1e-6,
        "Score for node 2: {}",
        scores[&2]
    );
    assert!(
        (scores[&3] - 0.49).abs() < 1e-6,
        "Score for node 3: {}",
        scores[&3]
    );

    // Debug info for failure
    if (scores[&4] - 0.1715).abs() >= 1e-6 {
        println!("Score for node 4: {}", scores[&4]);
    }
    assert!(
        (scores[&4] - 0.1715).abs() < 1e-6,
        "Score for node 4: {}",
        scores[&4]
    );
}

#[tokio::test]
async fn test_graph_max_depth() {
    let graph = CsrGraph::new();
    let tx = TxId::new(1);

    // Chain of 10 nodes
    for i in 1..=10 {
        graph
            .add_entity(tx, Entity::new(EntityId::new(i), format!("N{}", i), "T"))
            .await
            .unwrap();
        if i > 1 {
            graph
                .add_edge(tx, Edge::new(EntityId::new(i - 1), EntityId::new(i), "E"))
                .await
                .unwrap();
        }
    }

    // Traversal is limited to MAX_TRAVERSAL_HOPS = 3 in implementation
    let results = graph.traverse(EntityId::new(1), 5).await.unwrap();
    assert_eq!(
        results.len(),
        3,
        "Should only find 3 neighbors due to internal hard limit"
    );
}

#[tokio::test]
async fn test_graph_cycles() {
    let graph = CsrGraph::new();
    let tx = TxId::new(1);

    graph
        .add_entity(tx, Entity::new(EntityId::new(1), "A", "T"))
        .await
        .unwrap();
    graph
        .add_entity(tx, Entity::new(EntityId::new(2), "B", "T"))
        .await
        .unwrap();

    graph
        .add_edge(tx, Edge::new(EntityId::new(1), EntityId::new(2), "E"))
        .await
        .unwrap();
    graph
        .add_edge(tx, Edge::new(EntityId::new(2), EntityId::new(1), "E"))
        .await
        .unwrap();

    let results = graph.traverse(EntityId::new(1), 3).await.unwrap();
    assert_eq!(results.len(), 1, "Should not include start node in results");
    assert_eq!(results[0].0.inner(), 2);
}
