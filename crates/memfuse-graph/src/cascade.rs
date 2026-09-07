// FILE-CONTEXT
// ZWECK: Cascading-Invalidation (Supersedes-Chunk -> Graph-Kanten-Tombstone)
// INVARIANTEN: Jede tombstonierte Kante erhaelt einen Provenienz-Eintrag mit WAL-Seq (INV-GRAPH-PROV-1).
// STAND: TS:2026-08-30T19:00:00Z

use crate::csr::CsrGraph;
use memfuse_core::{DocId, EntityId, Result, TxId};

/// Report summarizing the cascade invalidation of graph edges derived from a superseded document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CascadeInvalidationReport {
    /// Number of edges tombstoned as a result of the superseded document.
    pub tombstoned_edge_count: usize,
    /// Unique node (entity) IDs affected by the tombstoned edges.
    pub affected_node_ids: Vec<EntityId>,
}

/// Tombstoniert alle Kanten, die von `superseded_doc_id` abgeleitet wurden,
/// als Reaktion auf ein Supersedes-Ereignis. Idempotent: mehrfacher Aufruf
/// mit derselben DocId hat keinen zusätzlichen Effekt.
///
/// INVARIANTE: Jede hier tombstonierte Kante erhält einen Provenienz-Eintrag
/// mit der WAL-Sequenznummer dieses Aufrufs (INV-GRAPH-PROV-1).
pub async fn cascade_invalidate_edges_for_superseded_doc(
    graph: &CsrGraph,
    superseded_doc_id: DocId,
    wal_seq: u64,
) -> Result<CascadeInvalidationReport> {
    let wal_tx = TxId::new(wal_seq);

    // 1. Lookup aller EdgeIds via DocId -> Set<EdgeId>-Index.
    let edge_ids = graph.edges_for_doc(superseded_doc_id);
    if edge_ids.is_empty() {
        return Ok(CascadeInvalidationReport {
            tombstoned_edge_count: 0,
            affected_node_ids: Vec::new(),
        });
    }

    // 2. Für jede gefundene EdgeId: existierendes Tombstone-Verfahren aufrufen (idempotent).
    let (tombstoned_count, newly_tombstoned_edges, affected_nodes) =
        graph.tombstone_edges_direct(&edge_ids, wal_tx)?;

    // 3. Persistieren falls Storage vorhanden.
    if let Some(storage) = graph.storage() {
        for (from_id, to_id) in &newly_tombstoned_edges {
            graph
                .delete_edge_persistence(storage.as_ref(), wal_tx, from_id, to_id)
                .await?;
        }
    }

    Ok(CascadeInvalidationReport {
        tombstoned_edge_count: tombstoned_count,
        affected_node_ids: affected_nodes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path_rag::PathRAGEngine;
    use memfuse_core::{Edge, GraphIndex};

    #[tokio::test]
    async fn test_cascade_invalidation_tombstones_edges_of_superseded_doc() {
        let graph = std::sync::Arc::new(CsrGraph::new());
        let tx1 = TxId::new(1);
        let doc_a = DocId::from_key("doc-a").unwrap();
        let doc_b = DocId::from_key("doc-b").unwrap();

        let node1 = EntityId::new(100);
        let node2 = EntityId::new(200);
        let node3 = EntityId::new(300);

        GraphIndex::add_entity(graph.as_ref(), tx1, memfuse_core::Entity::new(node1, "n1", "Node")).await.unwrap();
        GraphIndex::add_entity(graph.as_ref(), tx1, memfuse_core::Entity::new(node2, "n2", "Node")).await.unwrap();
        GraphIndex::add_entity(graph.as_ref(), tx1, memfuse_core::Entity::new(node3, "n3", "Node")).await.unwrap();

        // Edge 1 derived from Doc A
        let edge1 = Edge::new(node1, node2, "rel_a").with_source_doc_id(doc_a);
        // Edge 2 derived from Doc B
        let edge2 = Edge::new(node2, node3, "rel_b").with_source_doc_id(doc_b);

        GraphIndex::add_edge(graph.as_ref(), tx1, edge1).await.unwrap();
        GraphIndex::add_edge(graph.as_ref(), tx1, edge2).await.unwrap();
        GraphIndex::commit(graph.as_ref(), tx1).await.unwrap();

        assert_eq!(graph.neighbors(node1).await.unwrap().len(), 1);
        assert_eq!(graph.neighbors(node2).await.unwrap().len(), 1);

        // Cascade invalidate doc A
        let report = cascade_invalidate_edges_for_superseded_doc(&graph, doc_a, 2)
            .await
            .unwrap();

        assert_eq!(report.tombstoned_edge_count, 1);
        assert!(report.affected_node_ids.contains(&node1));
        assert!(report.affected_node_ids.contains(&node2));

        // Node 1 -> Node 2 edge should now be tombstoned
        assert!(graph.neighbors(node1).await.unwrap().is_empty());
        // Node 2 -> Node 3 edge (Doc B) remains unaffected
        assert_eq!(graph.neighbors(node2).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_cascade_invalidation_is_idempotent() {
        let graph = std::sync::Arc::new(CsrGraph::new());
        let tx1 = TxId::new(1);
        let doc_a = DocId::from_key("doc-a").unwrap();

        let node1 = EntityId::new(100);
        let node2 = EntityId::new(200);

        GraphIndex::add_entity(graph.as_ref(), tx1, memfuse_core::Entity::new(node1, "n1", "Node")).await.unwrap();
        GraphIndex::add_entity(graph.as_ref(), tx1, memfuse_core::Entity::new(node2, "n2", "Node")).await.unwrap();

        let edge = Edge::new(node1, node2, "rel").with_source_doc_id(doc_a);
        GraphIndex::add_edge(graph.as_ref(), tx1, edge).await.unwrap();
        GraphIndex::commit(graph.as_ref(), tx1).await.unwrap();

        // First call
        let report1 = cascade_invalidate_edges_for_superseded_doc(&graph, doc_a, 2)
            .await
            .unwrap();
        assert_eq!(report1.tombstoned_edge_count, 1);

        // Second call (idempotent)
        let report2 = cascade_invalidate_edges_for_superseded_doc(&graph, doc_a, 3)
            .await
            .unwrap();
        assert_eq!(report2.tombstoned_edge_count, 0);
        assert!(report2.affected_node_ids.is_empty());
    }

    #[tokio::test]
    async fn test_pathrag_sufficiency_excludes_cascaded_tombstones() {
        let graph = std::sync::Arc::new(CsrGraph::new());
        let tx1 = TxId::new(1);
        let doc_a = DocId::from_key("doc-a").unwrap();

        let node1 = EntityId::new(10);
        let node2 = EntityId::new(20);

        GraphIndex::add_entity(graph.as_ref(), tx1, memfuse_core::Entity::new(node1, "n1", "Node")).await.unwrap();
        GraphIndex::add_entity(graph.as_ref(), tx1, memfuse_core::Entity::new(node2, "n2", "Node")).await.unwrap();

        let edge = Edge::new(node1, node2, "derived").with_source_doc_id(doc_a);
        GraphIndex::add_edge(graph.as_ref(), tx1, edge).await.unwrap();
        GraphIndex::commit(graph.as_ref(), tx1).await.unwrap();

        let engine = PathRAGEngine::new(graph.as_ref(), 3, 0.5);

        // Before invalidation, path is found
        let paths_before = engine.find_all_paths(node1);
        assert!(!paths_before.is_empty());

        // Invalidate doc A
        cascade_invalidate_edges_for_superseded_doc(&graph, doc_a, 5)
            .await
            .unwrap();

        // After invalidation, PathRAGEngine finds no paths through tombstoned edge
        let paths_after = engine.find_all_paths(node1);
        assert!(paths_after.is_empty());
    }
}
