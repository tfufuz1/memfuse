use super::Collection;
use memfuse_core::{DocId, Result, StorageEngine, VectorIndex};

impl<S: StorageEngine, V: VectorIndex> Collection<S, V> {
    // AI-TAG[CONCURRENCY][CRITICAL] RESOLVED: AGT-DB-005 — relate() rollback race behoben, siehe ADR-023 (TS:2026-08-28T00:00:00Z)
    /// Creates a directional relationship between two documents in the collection.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn relate(&self, from: &str, to: &str, label: &str) -> Result<()> {
        let _guard = self.insert_lock.lock().await;
        let db_tx = self.begin_transaction()?;

        let from_id = memfuse_core::EntityId::from_key(from)?;
        let to_id = memfuse_core::EntityId::from_key(to)?;

        let key_str = format!("{}:{}:{}", from, label, to);
        let key = self.namespaced_key(key_str.as_bytes(), 2);
        let val = serde_json::json!({
            "from": from,
            "to": to,
            "label": label,
        });
        let bytes = serde_json::to_vec(&val)?;

        if let Err(e) = self.storage.put(db_tx.tx_id, &key, &bytes).await {
            let _ = db_tx.rollback().await;
            return Err(e);
        }

        let dummy_doc_id = DocId::from_key(from)?;
        db_tx.record_keys(key.clone(), vec![], dummy_doc_id);

        let from_entity = memfuse_core::Entity::new(from_id, from, "Node");
        let to_entity = memfuse_core::Entity::new(to_id, to, "Node");
        db_tx.stage_graph_entity(from_entity);
        db_tx.stage_graph_entity(to_entity);

        let edge = memfuse_core::Edge::new(from_id, to_id, label);
        db_tx.stage_graph_edge(edge);

        match db_tx.commit().await {
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Creates a bidirectional relationship atomically.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn relate_bidirectional(&self, from: &str, to: &str, label: &str) -> Result<()> {
        self.relate(from, to, label).await?;
        self.relate(to, from, label).await?;
        Ok(())
    }
}
