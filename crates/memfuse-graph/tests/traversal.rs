use memfuse_graph::CsrGraph;
use memfuse_core::{Entity, EntityId, Edge, TxId, GraphIndex};

#[tokio::test]
async fn test_graph_traversal_and_isolation() {
    let graph = CsrGraph::new();
    let tx = TxId::new(1);

    // Component 1: A -> B -> C
    let id_a = EntityId::new(1);
    let id_b = EntityId::new(2);
    let id_c = EntityId::new(3);

    graph.add_entity(tx, Entity::new(id_a, "A", "Node")).await.unwrap();
    graph.add_entity(tx, Entity::new(id_b, "B", "Node")).await.unwrap();
    graph.add_entity(tx, Entity::new(id_c, "C", "Node")).await.unwrap();

    graph.add_edge(tx, Edge::new(id_a, id_b, "connects")).await.unwrap();
    graph.add_edge(tx, Edge::new(id_b, id_c, "connects")).await.unwrap();

    // Component 2: D -> E
    let id_d = EntityId::new(4);
    let id_e = EntityId::new(5);

    graph.add_entity(tx, Entity::new(id_d, "D", "Node")).await.unwrap();
    graph.add_entity(tx, Entity::new(id_e, "E", "Node")).await.unwrap();

    graph.add_edge(tx, Edge::new(id_d, id_e, "connects")).await.unwrap();

    // Traversal from A (max 2 hops)
    let results = graph.traverse(id_a, 2).await.unwrap();

    // Should reach B and C
    assert!(results.iter().any(|(id, _)| *id == id_b));
    assert!(results.iter().any(|(id, _)| *id == id_c));

    // Should NOT reach D or E (isolation)
    assert!(!results.iter().any(|(id, _)| *id == id_d));
    assert!(!results.iter().any(|(id, _)| *id == id_e));

    // Verify Score Decay (0.7 per hop)
    // A -> B (1 hop): 1.0 * 0.7 * 1.0 (edge weight) = 0.7
    // B -> C (2 hops): 0.7 * 0.7 * 1.0 = 0.49
    let score_b = results.iter().find(|(id, _)| *id == id_b).unwrap().1;
    let score_c = results.iter().find(|(id, _)| *id == id_c).unwrap().1;

    assert!((score_b - 0.7).abs() < 1e-6);
    assert!((score_c - 0.49).abs() < 1e-6);
}

#[tokio::test]
async fn test_traversal_depth_enforcement() {
    let graph = CsrGraph::new();
    let tx = TxId::new(1);

    // A -> B -> C -> D
    let id_a = EntityId::new(1);
    let id_b = EntityId::new(2);
    let id_c = EntityId::new(3);
    let id_d = EntityId::new(4);

    graph.add_entity(tx, Entity::new(id_a, "A", "N")).await.unwrap();
    graph.add_entity(tx, Entity::new(id_b, "B", "N")).await.unwrap();
    graph.add_entity(tx, Entity::new(id_c, "C", "N")).await.unwrap();
    graph.add_entity(tx, Entity::new(id_d, "D", "N")).await.unwrap();

    graph.add_edge(tx, Edge::new(id_a, id_b, "E")).await.unwrap();
    graph.add_edge(tx, Edge::new(id_b, id_c, "E")).await.unwrap();
    graph.add_edge(tx, Edge::new(id_c, id_d, "E")).await.unwrap();

    // 1 hop: only B
    let res1 = graph.traverse(id_a, 1).await.unwrap();
    assert_eq!(res1.len(), 1);
    assert_eq!(res1[0].0, id_b);

    // 2 hops: B and C
    let res2 = graph.traverse(id_a, 2).await.unwrap();
    assert_eq!(res2.len(), 2);
    assert!(res2.iter().any(|(id, _)| *id == id_c));

    // 3 hops: B, C, and D
    let res3 = graph.traverse(id_a, 3).await.unwrap();
    assert_eq!(res3.len(), 3);
    assert!(res3.iter().any(|(id, _)| *id == id_d));
}
