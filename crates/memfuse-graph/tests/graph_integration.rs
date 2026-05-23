use memfuse_core::{Edge, Entity, EntityId, GraphIndex, TxId};
use memfuse_graph::csr::CsrGraph;

// ANCHOR:INTEGRATION:GRAPH-001 STATUS:DONE AGENT:12 DATE:2026-05-23
// Complex traversal test for CSR Graph with multiple paths and weights.
#[tokio::test]
async fn test_graph_complex_traversal_integration() {
    let graph = CsrGraph::new();
    let tx = TxId::new(100);

    // Setup a small social graph
    // 1 (User) -> 2 (Interests) -> 3 (Products)
    // 1 -> 4 (Recent) -> 3 (Products)

    let entities = [
        (1, "User1", "User"),
        (2, "AI", "Topic"),
        (3, "MemFuse-Book", "Product"),
        (4, "Search", "Topic"),
    ];

    for (id, name, kind) in entities {
        graph
            .add_entity(tx, Entity::new(EntityId::new(id), name, kind))
            .await
            .expect("add entity");
    }

    // Path 1: User -> AI -> Book (weight 1.0, 0.9)
    graph
        .add_edge(
            tx,
            Edge::new(EntityId::new(1), EntityId::new(2), "interested_in").with_weight(1.0),
        )
        .await
        .expect("edge 1-2");
    graph
        .add_edge(
            tx,
            Edge::new(EntityId::new(2), EntityId::new(3), "recommends").with_weight(0.9),
        )
        .await
        .expect("edge 2-3");

    // Path 2: User -> Search -> Book (weight 0.5, 0.5)
    graph
        .add_edge(
            tx,
            Edge::new(EntityId::new(1), EntityId::new(4), "searched").with_weight(0.5),
        )
        .await
        .expect("edge 1-4");
    graph
        .add_edge(
            tx,
            Edge::new(EntityId::new(4), EntityId::new(3), "related").with_weight(0.5),
        )
        .await
        .expect("edge 4-3");

    // Traverse from User (1)
    let results = graph.traverse(EntityId::new(1), 2).await.expect("traverse");

    // Should find 2, 4 at hop 1 and 3 at hop 2
    assert!(results.iter().any(|(id, _)| id.inner() == 2));
    assert!(results.iter().any(|(id, _)| id.inner() == 4));
    assert!(results.iter().any(|(id, _)| id.inner() == 3));

    // Verify score for node 3
    // Path 1: 1.0 * 0.7 * 0.9 = 0.63
    // Path 2: 0.5 * 0.7 * 0.5 = 0.175
    // BFS implementation keeps the BEST score.
    // 0.63 * 0.7 (decay for 2 hops) = 0.441 (if hop decay is applied per hop)
    // Wait, let's re-calculate based on csr.rs logic:
    // next_score = current_score * SCORE_DECAY * weight
    // Hop 1 (Node 2): 1.0 * 0.7 * 1.0 = 0.7
    // Hop 2 (Node 3 via 2): 0.7 * 0.7 * 0.9 = 0.441
    // Hop 1 (Node 4): 1.0 * 0.7 * 0.5 = 0.35
    // Hop 2 (Node 3 via 4): 0.35 * 0.7 * 0.5 = 0.1225

    let score_3 = results
        .iter()
        .find(|(id, _)| id.inner() == 3)
        .map(|(_, s)| *s)
        .unwrap();
    assert!((score_3 - 0.441).abs() < f32::EPSILON);
}

#[tokio::test]
async fn test_graph_stats_and_isolation() {
    let graph = CsrGraph::new();
    let tx1 = TxId::new(1);

    graph
        .add_entity(tx1, Entity::new(EntityId::new(1), "E1", "T1"))
        .await
        .expect("add");
    graph
        .add_edge(tx1, Edge::new(EntityId::new(1), EntityId::new(1), "self"))
        .await
        .expect("edge");

    let stats = graph.stats().await.expect("stats");
    assert_eq!(stats.num_entities, 1);
    assert_eq!(stats.num_edges, 1);

    // Test transaction methods (placeholders for now)
    graph.commit(tx1).await.expect("commit");
    graph.rollback(TxId::new(2)).await.expect("rollback");
}
