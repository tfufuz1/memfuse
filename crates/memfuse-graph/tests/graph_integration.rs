//! Integration tests for the CSR Graph (Signal 3).
// ANCHOR:INTEGRATION:GRAPH-001 STATUS:READY AGENT:12 DATE:2026-05-24

use memfuse_core::{Edge, Entity, EntityId, GraphIndex, TxId};
use memfuse_graph::CsrGraph;

#[tokio::test]
async fn test_graph_traversal_decay_and_cycles() {
    let graph = CsrGraph::new();
    let tx = TxId::new(1);

    // Setup a small graph
    // A -> B (1.0), B -> C (1.0), C -> A (1.0)
    // A -> D (0.5)

    let nodes = vec![(1, "A"), (2, "B"), (3, "C"), (4, "D")];

    for (id, name) in nodes {
        graph
            .add_entity(tx, Entity::new(EntityId::new(id), name, "Node"))
            .await
            .expect("add entity");
    }

    graph
        .add_edge(
            tx,
            Edge::new(EntityId::new(1), EntityId::new(2), "link").with_weight(1.0),
        )
        .await
        .expect("add edge 1-2");
    graph
        .add_edge(
            tx,
            Edge::new(EntityId::new(2), EntityId::new(3), "link").with_weight(1.0),
        )
        .await
        .expect("add edge 2-3");
    graph
        .add_edge(
            tx,
            Edge::new(EntityId::new(3), EntityId::new(1), "link").with_weight(1.0),
        )
        .await
        .expect("add edge 3-1");
    graph
        .add_edge(
            tx,
            Edge::new(EntityId::new(1), EntityId::new(4), "link").with_weight(0.5),
        )
        .await
        .expect("add edge 1-4");

    // Traverse from A with 2 hops
    // Hop 1: B (Score: 1.0 * 0.7 * 1.0 = 0.7), D (Score: 1.0 * 0.7 * 0.5 = 0.35)
    // Hop 2: C (Score: 0.7 * 0.7 * 1.0 = 0.49)

    let results = graph.traverse(EntityId::new(1), 2).await.expect("traverse");

    assert_eq!(results.len(), 3);

    let scores: std::collections::HashMap<_, _> = results.into_iter().collect();

    let score_b = *scores.get(&EntityId::new(2)).expect("B missing");
    let score_c = *scores.get(&EntityId::new(3)).expect("C missing");
    let score_d = *scores.get(&EntityId::new(4)).expect("D missing");

    assert!((score_b - 0.7).abs() < 1e-6);
    assert!((score_c - 0.49).abs() < 1e-6);
    assert!((score_d - 0.35).abs() < 1e-6);

    // Check stats
    let stats = graph.stats().await.expect("stats");
    assert_eq!(stats.num_entities, 4);
    assert_eq!(stats.num_edges, 4);
}

#[tokio::test]
async fn test_graph_max_hops_enforcement() {
    let graph = CsrGraph::new();
    let tx = TxId::new(1);

    // Line: 1 -> 2 -> 3 -> 4 -> 5
    for id in 1..=5 {
        graph
            .add_entity(tx, Entity::new(EntityId::new(id), "N", "T"))
            .await
            .unwrap();
        if id > 1 {
            graph
                .add_edge(
                    tx,
                    Edge::new(EntityId::new(id - 1), EntityId::new(id), "next"),
                )
                .await
                .unwrap();
        }
    }

    // Traverse with 1 hop from 1
    let res1 = graph.traverse(EntityId::new(1), 1).await.unwrap();
    assert_eq!(res1.len(), 1);
    assert_eq!(res1[0].0, EntityId::new(2));

    // Traverse with 3 hops from 1
    let res3 = graph.traverse(EntityId::new(1), 3).await.unwrap();
    assert_eq!(res3.len(), 3); // 2, 3, 4
}
