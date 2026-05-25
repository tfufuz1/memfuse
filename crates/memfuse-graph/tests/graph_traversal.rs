// AGENT:12 DATE:2026-05-25 STATUS:READY
// ANCHOR:INTEGRATION:GRAPH-001 — Complex CSR Graph traversals and score decay.

use memfuse_core::{Entity, EntityId, Edge, TxId, GraphIndex};
use memfuse_graph::CsrGraph;

#[tokio::test]
async fn test_graph_complex_traversal_integration() {
    let graph = CsrGraph::new();
    let tx = TxId::new(100);

    // Create a chain with branching
    // A -> B (1.0) -> C (0.9) -> D (0.8)
    //      B -> E (0.5)
    // A -> F (0.2)

    let nodes = vec![
        (1, "A"), (2, "B"), (3, "C"), (4, "D"), (5, "E"), (6, "F")
    ];

    for (id, name) in nodes {
        graph.add_entity(tx, Entity::new(EntityId::new(id), name, "Test")).await.unwrap();
    }

    let edges = vec![
        (1, 2, 1.0), (2, 3, 0.9), (3, 4, 0.8),
        (2, 5, 0.5), (1, 6, 0.2)
    ];

    for (from, to, weight) in edges {
        graph.add_edge(tx, Edge::new(EntityId::new(from), EntityId::new(to), "link").with_weight(weight)).await.unwrap();
    }

    // Traverse from A, max 3 hops
    let results = graph.traverse(EntityId::new(1), 3).await.unwrap();

    // Expected scores (decay = 0.7)
    // B (hop 1): 1.0 * 0.7 * 1.0 = 0.7
    // F (hop 1): 1.0 * 0.7 * 0.2 = 0.14
    // C (hop 2): 0.7 * 0.7 * 0.9 = 0.441
    // E (hop 2): 0.7 * 0.7 * 0.5 = 0.245
    // D (hop 3): 0.441 * 0.7 * 0.8 = 0.24696

    let score_map: std::collections::HashMap<_, _> = results.into_iter().collect();

    assert_eq!(score_map.len(), 5);

    let s_b = *score_map.get(&EntityId::new(2)).unwrap();
    let s_f = *score_map.get(&EntityId::new(6)).unwrap();
    let s_c = *score_map.get(&EntityId::new(3)).unwrap();
    let s_e = *score_map.get(&EntityId::new(5)).unwrap();
    let s_d = *score_map.get(&EntityId::new(4)).unwrap();

    assert!((s_b - 0.7).abs() < 1e-6);
    assert!((s_f - 0.14).abs() < 1e-6);
    assert!((s_c - 0.441).abs() < 1e-6);
    assert!((s_e - 0.245).abs() < 1e-6);
    assert!((s_d - 0.24696).abs() < 1e-6);
}

#[tokio::test]
async fn test_graph_cycle_and_multiple_paths() {
    let graph = CsrGraph::new();
    let tx = TxId::new(1);

    // A -> B (1.0) -> C (1.0)
    // A -> C (0.1)
    // C -> A (1.0)

    graph.add_entity(tx, Entity::new(EntityId::new(1), "A", "N")).await.unwrap();
    graph.add_entity(tx, Entity::new(EntityId::new(2), "B", "N")).await.unwrap();
    graph.add_entity(tx, Entity::new(EntityId::new(3), "C", "N")).await.unwrap();

    graph.add_edge(tx, Edge::new(EntityId::new(1), EntityId::new(2), "e").with_weight(1.0)).await.unwrap();
    graph.add_edge(tx, Edge::new(EntityId::new(2), EntityId::new(3), "e").with_weight(1.0)).await.unwrap();
    graph.add_edge(tx, Edge::new(EntityId::new(1), EntityId::new(3), "e").with_weight(0.1)).await.unwrap();
    graph.add_edge(tx, Edge::new(EntityId::new(3), EntityId::new(1), "e").with_weight(1.0)).await.unwrap();

    let results = graph.traverse(EntityId::new(1), 5).await.unwrap();

    // C can be reached via A->B->C (hop 2, score 0.49) or A->C (hop 1, score 0.07)
    // CSR graph implementation should pick the best score

    let score_map: std::collections::HashMap<_, _> = results.into_iter().collect();
    let s_c = *score_map.get(&EntityId::new(3)).unwrap();
    assert!((s_c - 0.49).abs() < 1e-6);

    // A should not be in results as it is the start node
    assert!(!score_map.contains_key(&EntityId::new(1)));
}
