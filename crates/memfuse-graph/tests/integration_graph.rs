use memfuse_core::{Edge, Entity, EntityId, GraphIndex, TxId};
use memfuse_graph::CsrGraph;

#[tokio::test]
async fn test_graph_integration_pipeline() {
    let graph = CsrGraph::new();
    let tx1 = TxId::new(400);

    // 1. Establish node ontology
    let e1 = Entity::new(EntityId::new(10), "Root", "Directory");
    let e2 = Entity::new(EntityId::new(20), "ChildA", "File");
    let e3 = Entity::new(EntityId::new(30), "ChildB", "File");
    
    graph.add_entity(tx1, e1).await.expect("valid entity");
    graph.add_entity(tx1, e2).await.expect("valid entity");
    graph.add_entity(tx1, e3).await.expect("valid entity");

    // 2. Establish edge geometry
    let edge_a = Edge::new(EntityId::new(10), EntityId::new(20), "contains").with_weight(0.9);
    let edge_b = Edge::new(EntityId::new(10), EntityId::new(30), "contains").with_weight(0.5);

    graph.add_edge(tx1, edge_a).await.expect("valid edge");
    graph.add_edge(tx1, edge_b).await.expect("valid edge");

    // 3. Commit
    graph.commit(tx1).await.expect("commit");

    // 4. Validate Traversal
    let results = graph.traverse(EntityId::new(10), 1).await.expect("traverse");
    
    assert_eq!(results.len(), 2);
    // Highest weight should be first (ChildA / 20)
    assert_eq!(results[0].0, EntityId::new(20));
    assert_eq!(results[1].0, EntityId::new(30));

    // Decayed weight validation
    assert!(results[0].1 > results[1].1, "Scoring must be proportional to weight");
}
