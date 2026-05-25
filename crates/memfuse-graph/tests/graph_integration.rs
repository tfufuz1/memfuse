//! Integration tests for CsrGraph.
// ANCHOR:INTEGRATION:GRAPH-001 STATUS:DONE AGENT:12 DATE:2026-06-20

use memfuse_core::{Edge, Entity, EntityId, GraphIndex, TxId};
use memfuse_graph::CsrGraph;

#[tokio::test]
async fn test_graph_traversal_integration() {
    let graph = CsrGraph::new();
    let tx = TxId::new(1);

    // 1. Build a small knowledge graph
    // (A) --knows--> (B) --knows--> (C)
    //  |              |
    //  +----works_at--> (D)

    let nodes = [
        (1, "Alice", "Person"),
        (2, "Bob", "Person"),
        (3, "Charlie", "Person"),
        (4, "TechCorp", "Company"),
    ];

    for (id, name, kind) in nodes {
        graph
            .add_entity(tx, Entity::new(EntityId::new(id), name, kind))
            .await
            .unwrap(); // expect #[cfg(test)]
    }

    graph
        .add_edge(
            tx,
            Edge::new(EntityId::new(1), EntityId::new(2), "knows").with_weight(0.9),
        )
        .await
        .unwrap(); // expect #[cfg(test)]
    graph
        .add_edge(
            tx,
            Edge::new(EntityId::new(2), EntityId::new(3), "knows").with_weight(0.8),
        )
        .await
        .unwrap(); // expect #[cfg(test)]
    graph
        .add_edge(
            tx,
            Edge::new(EntityId::new(1), EntityId::new(4), "works_at").with_weight(1.0),
        )
        .await
        .unwrap(); // expect #[cfg(test)]

    // 2. Test 1-hop traversal from Alice (1)
    let results_1hop = graph
        .traverse(EntityId::new(1), 1)
        .await
        .expect("traverse failed"); // expect #[cfg(test)]
                                    // Should find Bob (2) and TechCorp (4)
    assert_eq!(results_1hop.len(), 2);

    let map_1hop: std::collections::HashMap<u64, f32> = results_1hop
        .into_iter()
        .map(|(id, s)| (id.inner(), s))
        .collect();

    // Scores:
    // Bob: 1.0 (start) * 0.7 (decay) * 0.9 (weight) = 0.63
    // TechCorp: 1.0 (start) * 0.7 (decay) * 1.0 (weight) = 0.7
    assert!((map_1hop[&2] - 0.63).abs() < 1e-6);
    assert!((map_1hop[&4] - 0.7).abs() < 1e-6);

    // 3. Test 2-hop traversal from Alice (1)
    let results_2hop = graph
        .traverse(EntityId::new(1), 2)
        .await
        .expect("traverse failed"); // expect #[cfg(test)]
                                    // Should find Bob (2), TechCorp (4) AND Charlie (3)
    assert_eq!(results_2hop.len(), 3);

    let map_2hop: std::collections::HashMap<u64, f32> = results_2hop
        .into_iter()
        .map(|(id, s)| (id.inner(), s))
        .collect();

    // Charlie Score: Score(Bob) * 0.7 (decay) * 0.8 (weight) = 0.63 * 0.7 * 0.8 = 0.3528
    assert!((map_2hop[&3] - 0.3528).abs() < 1e-6);
}

#[tokio::test]
async fn test_graph_cycle_and_multipath() {
    let graph = CsrGraph::new();
    let tx = TxId::new(1);

    // 1 -> 2 (0.5)
    // 1 -> 3 (0.9)
    // 3 -> 2 (0.8)
    // 2 -> 1 (1.0) cycle

    for id in 1..=3 {
        graph
            .add_entity(tx, Entity::new(EntityId::new(id), id.to_string(), "Node"))
            .await
            .unwrap(); // expect #[cfg(test)]
    }

    graph
        .add_edge(
            tx,
            Edge::new(EntityId::new(1), EntityId::new(2), "e").with_weight(0.5),
        )
        .await
        .unwrap(); // expect #[cfg(test)]
    graph
        .add_edge(
            tx,
            Edge::new(EntityId::new(1), EntityId::new(3), "e").with_weight(0.9),
        )
        .await
        .unwrap(); // expect #[cfg(test)]
    graph
        .add_edge(
            tx,
            Edge::new(EntityId::new(3), EntityId::new(2), "e").with_weight(0.8),
        )
        .await
        .unwrap(); // expect #[cfg(test)]
    graph
        .add_edge(
            tx,
            Edge::new(EntityId::new(2), EntityId::new(1), "e").with_weight(1.0),
        )
        .await
        .unwrap(); // expect #[cfg(test)]

    // Traverse from 1, max 2 hops
    let results = graph.traverse(EntityId::new(1), 2).await.unwrap(); // expect #[cfg(test)]
    let map: std::collections::HashMap<u64, f32> =
        results.into_iter().map(|(id, s)| (id.inner(), s)).collect();

    // Node 2 can be reached via two paths:
    // P1: 1 -> 2 : 1.0 * 0.7 * 0.5 = 0.35
    // P2: 1 -> 3 -> 2 : (1.0 * 0.7 * 0.9) * 0.7 * 0.8 = 0.63 * 0.7 * 0.8 = 0.3528
    // Implementations should pick the better score.
    assert!((map[&2] - 0.3528).abs() < 1e-6);

    // Node 1 (start) should not be in results even though there is a cycle 1->2->1
    assert!(!map.contains_key(&1));
}

#[tokio::test]
async fn test_graph_stats() {
    let graph = CsrGraph::new();
    let tx = TxId::new(1);

    graph
        .add_entity(tx, Entity::new(EntityId::new(1), "A", "N"))
        .await
        .unwrap(); // expect #[cfg(test)]
    graph
        .add_entity(tx, Entity::new(EntityId::new(2), "B", "N"))
        .await
        .unwrap(); // expect #[cfg(test)]
    graph
        .add_edge(tx, Edge::new(EntityId::new(1), EntityId::new(2), "E"))
        .await
        .unwrap(); // expect #[cfg(test)]

    let stats = graph.stats().await.unwrap(); // expect #[cfg(test)]
    assert_eq!(stats.num_entities, 2);
    assert_eq!(stats.num_edges, 1);
}
