//! Integration tests for CsrGraph.
// ANCHOR:INTEGRATION:GRAPH-001 STATUS:DONE AGENT:12 DATE:2026-06-21

use memfuse_core::{Edge, Entity, EntityId, GraphIndex, TxId};
use memfuse_graph::CsrGraph;

#[tokio::test]
async fn test_graph_integration_lifecycle() {
    let graph = CsrGraph::new();
    let tx = TxId::new(100);

    // 1. Add entities
    let e1 = Entity::new(EntityId::new(1), "Alice", "Person");
    let e2 = Entity::new(EntityId::new(2), "Bob", "Person");
    let e3 = Entity::new(EntityId::new(3), "MemFuse", "Project");

    graph.add_entity(tx, e1).await.expect("failed to add e1");
    graph.add_entity(tx, e2).await.expect("failed to add e2");
    graph.add_entity(tx, e3).await.expect("failed to add e3");

    // 2. Add edges
    // Alice -> Bob (friendship, weight 1.0)
    graph.add_edge(tx, Edge::new(EntityId::new(1), EntityId::new(2), "friend").with_weight(1.0))
        .await
        .expect("failed to add edge 1-2");

    // Bob -> MemFuse (contributor, weight 0.8)
    graph.add_edge(tx, Edge::new(EntityId::new(2), EntityId::new(3), "contributor").with_weight(0.8))
        .await
        .expect("failed to add edge 2-3");

    // 3. Traverse from Alice
    // Hop 1: Alice -> Bob. Score = 1.0 * 0.7 * 1.0 = 0.7
    // Hop 2: Bob -> MemFuse. Score = 0.7 * 0.7 * 0.8 = 0.392
    let results = graph.traverse(EntityId::new(1), 2).await.expect("failed to traverse");

    assert_eq!(results.len(), 2);

    let bob_score = results.iter().find(|(id, _)| *id == EntityId::new(2)).map(|(_, s)| *s).unwrap();
    let project_score = results.iter().find(|(id, _)| *id == EntityId::new(3)).map(|(_, s)| *s).unwrap();

    assert!((bob_score - 0.7).abs() < 1e-6);
    assert!((project_score - 0.392).abs() < 1e-6);

    // 4. Verify stats
    let stats = graph.stats().await.expect("failed to get stats");
    assert_eq!(stats.num_entities, 3);
    assert_eq!(stats.num_edges, 2);
}
