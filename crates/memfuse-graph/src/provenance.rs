use crate::immune::EdgeId;
use ahash::{AHashMap, AHashSet};
use memfuse_core::{DocId, TxId};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

/// Herkunftsnachweis für eine Graph-Kante.
/// INV-GRAPH-PROV-1: Jede aktive CSR-Kante trägt einen EdgeProvenance-Eintrag,
/// der ihre Quell-Dokumente zurückverfolgbar macht.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeProvenance {
    /// Identifikator der Kante (from_entity_id, to_entity_id).
    pub edge_id: EdgeId,
    /// Dokumente deren Inhalt zu dieser Kante beigetragen hat.
    pub source_doc_ids: Vec<DocId>,
    /// Transaktions-ID bei der Erstellung der Kante.
    pub created_at_tx: TxId,
}

impl EdgeProvenance {
    pub fn new(edge_id: EdgeId, source_doc_ids: Vec<DocId>, created_at_tx: TxId) -> Self {
        Self {
            edge_id,
            source_doc_ids,
            created_at_tx,
        }
    }
}

/// Rückverfolgung DocId -> betroffene Kanten, für Cascading-Invalidation.
/// Wird bei jedem neuen EdgeProvenance-Eintrag aktualisiert.
#[derive(Debug, Default)]
pub struct DocEdgeIndex {
    index: RwLock<AHashMap<u64, AHashSet<EdgeId>>>,
}

impl DocEdgeIndex {
    pub fn new() -> Self {
        Self {
            index: RwLock::new(AHashMap::new()),
        }
    }

    /// Registriert die Abhängigkeit einer Kante von einem Dokument.
    pub fn record(&self, doc_id: DocId, edge_id: EdgeId) {
        let mut guard = self.index.write();
        guard.entry(doc_id.inner()).or_default().insert(edge_id);
    }

    /// Registriert einen EdgeProvenance-Eintrag und indiziert alle darin enthaltenen source_doc_ids.
    pub fn record_provenance(&self, provenance: &EdgeProvenance) {
        let mut guard = self.index.write();
        for &doc_id in &provenance.source_doc_ids {
            guard
                .entry(doc_id.inner())
                .or_default()
                .insert(provenance.edge_id);
        }
    }

    /// Gibt alle Kanten zurück, die von diesem Dokument abhängen.
    pub fn edges_for_doc(&self, doc_id: DocId) -> Vec<EdgeId> {
        let guard = self.index.read();
        guard
            .get(&doc_id.inner())
            .map(|set| set.iter().copied().collect())
            .unwrap_or_default()
    }

    /// Entfernt alle Kanten-Registrierungen für ein Dokument.
    pub fn remove_doc(&self, doc_id: DocId) {
        let mut guard = self.index.write();
        guard.remove(&doc_id.inner());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use memfuse_core::EntityId;

    #[test]
    fn test_doc_edge_index_record_and_retrieve() {
        let index = DocEdgeIndex::new();
        let doc1 = DocId::new(100);
        let doc2 = DocId::new(200);
        let edge1 = (EntityId::new(1), EntityId::new(2));
        let edge2 = (EntityId::new(2), EntityId::new(3));

        index.record(doc1, edge1);
        index.record(doc1, edge2);
        index.record(doc2, edge2);

        let edges_doc1 = index.edges_for_doc(doc1);
        assert_eq!(edges_doc1.len(), 2);
        assert!(edges_doc1.contains(&edge1));
        assert!(edges_doc1.contains(&edge2));

        let edges_doc2 = index.edges_for_doc(doc2);
        assert_eq!(edges_doc2.len(), 1);
        assert!(edges_doc2.contains(&edge2));

        index.remove_doc(doc1);
        assert!(index.edges_for_doc(doc1).is_empty());
        assert_eq!(index.edges_for_doc(doc2).len(), 1);
    }

    #[test]
    fn test_record_provenance() {
        let index = DocEdgeIndex::new();
        let doc1 = DocId::new(10);
        let doc2 = DocId::new(20);
        let edge = (EntityId::new(5), EntityId::new(6));

        let prov = EdgeProvenance::new(edge, vec![doc1, doc2], TxId::new(1));
        index.record_provenance(&prov);

        assert_eq!(index.edges_for_doc(doc1), vec![edge]);
        assert_eq!(index.edges_for_doc(doc2), vec![edge]);
    }
}
