use memfuse_core::{Edge, Entity, EntityId, GraphIndex, TxId};
use memfuse_graph::CsrGraph;
#[tokio::test]
async fn test_graph() {
    let graph = CsrGraph::new();
    let tx = TxId::new(1);
    graph
        .add_entity(tx, Entity::new(EntityId::new(1), "A", "N"))
        .await
        .unwrap(); // unwrap
    graph
        .add_entity(tx, Entity::new(EntityId::new(2), "B", "N"))
        .await
        .unwrap(); // unwrap
    graph
        .add_edge(tx, Edge::new(EntityId::new(1), EntityId::new(2), "e"))
        .await
        .unwrap(); // unwrap
    let res = graph.traverse(EntityId::new(1), 1).await.unwrap(); // unwrap
    assert_eq!(res.len(), 1);
}
