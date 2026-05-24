// ANCHOR:INTEGRATION:GRAPH-001 STATUS:READY AGENT:12
use memfuse_core::{Edge, Entity, EntityId, GraphIndex, TxId};
use memfuse_graph::CsrGraph;

#[tokio::test]
async fn test_graph_complex_traversal() {
    let graph = CsrGraph::new();
    let tx = TxId::new(1);

    // Create a small network
    // A -> B (1.0)
    // B -> C (1.0)
    // C -> D (1.0)
    // A -> D (0.1)

    let nodes = ["A", "B", "C", "D"];
    for (i, name) in nodes.iter().enumerate() {
        graph.add_entity(tx, Entity::new(EntityId::new(i as u64), name.to_string(), "Node")).await.unwrap();
    }

    graph.add_edge(tx, Edge::new(EntityId::new(0), EntityId::new(1), "link").with_weight(1.0)).await.unwrap();
    graph.add_edge(tx, Edge::new(EntityId::new(1), EntityId::new(2), "link").with_weight(1.0)).await.unwrap();
    graph.add_edge(tx, Edge::new(EntityId::new(2), EntityId::new(3), "link").with_weight(1.0)).await.unwrap();
    graph.add_edge(tx, Edge::new(EntityId::new(0), EntityId::new(3), "link").with_weight(0.1)).await.unwrap();

    // Traverse from A
    let results = graph.traverse(EntityId::new(0), 3).await.expect("traverse failed");

    // Expected scores:
    // B (1 hop): 1.0 * 0.7 * 1.0 = 0.7
    // C (2 hops via B): 0.7 * 0.7 * 1.0 = 0.49
    // D (3 hops via C): 0.49 * 0.7 * 1.0 = 0.343
    // D (1 hop direct): 1.0 * 0.7 * 0.1 = 0.07
    // D should have the better score (0.343)

    let score_map: std::collections::HashMap<_, _> = results.into_iter().collect();

    let sd = *score_map.get(&EntityId::new(3)).expect("D missing");
    assert!((sd - 0.343).abs() < 1e-6);
}
