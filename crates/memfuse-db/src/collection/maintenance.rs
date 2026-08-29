//! Maintenance, expiry reaper, community detection, and collection lifecycle operations for `Collection`.

use super::Collection;
use crate::collection::{StoredDocument, StoredDocumentMeta, EXPIRY_METADATA_KEY};
use memfuse_core::{DocId, EntityId, Result, StorageEngine, VectorIndex};
use memfuse_graph::community::{
    detect_communities, CommunityAssignment, CommunityDetectionConfig,
};

impl<S: StorageEngine> Collection<S> {
    /// Loads the HNSW vector index from storage during initialization.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn load_index(&self) -> Result<()> {
        let scan_prefix = if self.name == "default" {
            b"".to_vec()
        } else {
            let mut p = self.prefix.clone();
            p.push(0); // key_type=0
            p
        };

        let entries = self.storage.scan_prefix(&scan_prefix).await?;
        let tx = self.next_tx()?;
        for (k, v) in entries {
            if self.name == "default" && k.starts_with(b"__") {
                continue;
            }

            let stored: StoredDocument = match serde_json::from_slice(&v) {
                Ok(d) => d,
                Err(_) => continue,
            };
            let doc_id = DocId::from_key(&stored.id)?;
            if let Err(e) = self.index.insert(tx, doc_id, &stored.embedding).await {
                tracing::warn!(doc_id = ?doc_id, error = %e, "Konnte Dokument bei load_index nicht in Index einfügen");
            }
        }
        self.index.commit(tx).await?;
        Ok(())
    }

    /// Migrates old doc_keys (with Embedding) to new doc_keys (only Metadata).
    /// Safe to call multiple times (idempotent).
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn migrate_doc_keys_v1(&self) -> Result<u64> {
        let prefix = if self.name == "default" {
            b"__docid:".to_vec()
        } else {
            let mut p = self.prefix.clone();
            p.push(1); // docid mapping type
            p
        };

        let entries = self.storage.scan_prefix(&prefix).await?;
        let mut migrated_count = 0;
        let tx = self.next_tx()?;

        for (k, v) in entries {
            if let Ok(full) = serde_json::from_slice::<StoredDocument>(&v) {
                let meta_only = StoredDocumentMeta::from(&full);
                if let Ok(meta_data) = serde_json::to_vec(&meta_only) {
                    self.storage.put(tx, &k, &meta_data).await?;
                    migrated_count += 1;
                }
            }
        }

        if migrated_count > 0 {
            self.storage.commit(tx).await?;
            tracing::info!(
                "Migrated {} legacy doc_keys to new format in collection '{}'",
                migrated_count,
                self.name
            );
        }

        Ok(migrated_count)
    }

    /// Loads text index statistics from storage.
    pub async fn load_text_stats(&self) -> Result<()> {
        self.text_index.load_stats().await
    }

    /// Scans the collection for documents with expired sequence-based TTLs and deletes them in batches.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn reap_expired_documents(&self, max_expired: usize) -> Result<usize> {
        let current_seq = self.snapshot_seq().await?;
        let docs = self.scan_prefix("").await?;
        let mut expired_ids = Vec::new();

        for (id, val) in docs {
            if expired_ids.len() >= max_expired {
                break;
            }

            if self.name == "default" && id.starts_with("__") {
                continue;
            }

            let meta_obj = val
                .get("metadata")
                .and_then(|m| m.as_object())
                .or_else(|| val.as_object());

            if let Some(obj) = meta_obj {
                if let Some(expiry_seq) = obj.get(EXPIRY_METADATA_KEY).and_then(|v| v.as_u64()) {
                    if current_seq >= expiry_seq {
                        expired_ids.push(id);
                    }
                }
            }
        }

        let count = expired_ids.len();
        for id in &expired_ids {
            tracing::info!(collection = %self.name, id = %id, "Reaping expired document");
            if let Err(e) = self.delete(id).await {
                tracing::error!(
                    collection = %self.name,
                    id = %id,
                    error = %e,
                    "Expiry reaper failed to delete document"
                );
            }
        }

        if count > 0 && self.index.is_rebuild_required() {
            tracing::info!(
                collection = %self.name,
                "HNSW tombstone threshold reached after expiry reaping; triggering async rebuild"
            );
            self.index.trigger_rebuild_async();
        }

        Ok(count)
    }

    /// Scans the collection for documents with expired TTLs and deletes them.
    ///
    /// Reads `created_at_ms` (or `timestamp_ms`) and `ttl_ms` from document metadata.
    /// Time calculations use UTC unix timestamp milliseconds.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn trigger_reaper(&self) -> Result<usize> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| memfuse_core::MemFuseError::Internal(e.to_string()))?
            .as_millis() as u64;

        let docs = self.scan_prefix("").await?;
        let mut expired_ids = Vec::new();

        for (id, val) in docs {
            let meta_obj = val
                .get("metadata")
                .and_then(|m| m.as_object())
                .or_else(|| val.as_object());

            if let Some(obj) = meta_obj {
                let ttl_val = match obj.get("ttl_ms").and_then(|v| v.as_u64()) {
                    Some(ttl) if ttl > 0 => ttl,
                    _ => continue,
                };

                let created_at = match obj
                    .get("created_at_ms")
                    .or_else(|| obj.get("timestamp_ms"))
                    .and_then(|v| v.as_u64())
                {
                    Some(c) => c,
                    None => continue,
                };

                if let Some(expire_at) = created_at.checked_add(ttl_val) {
                    if now_ms >= expire_at {
                        expired_ids.push((id, expire_at));
                    }
                }
            }
        }

        let count = expired_ids.len();
        for (id, expire_at) in expired_ids {
            match self.delete(&id).await {
                Ok(_) => {
                    tracing::debug!(
                        collection = %self.name,
                        doc_id = %id,
                        expire_at = expire_at,
                        "Reaped expired TTL document"
                    );
                }
                Err(e) => {
                    tracing::error!(
                        collection = %self.name,
                        id = %id,
                        error = %e,
                        "Reaper failed to delete expired document"
                    );
                }
            }
        }

        Ok(count)
    }

    /// Runs Label Propagation Community Detection on the collection's graph index
    /// and persists the resulting assignments in storage using TxId allocation.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn run_community_detection(&self) -> Result<Vec<CommunityAssignment>> {
        self.run_community_detection_with_config(&CommunityDetectionConfig::default())
            .await
    }

    /// Runs Label Propagation Community Detection with custom configuration
    /// and persists the resulting assignments in storage using TxId allocation.
    #[tracing::instrument(level = "trace", skip(self, config))]
    pub async fn run_community_detection_with_config(
        &self,
        config: &CommunityDetectionConfig,
    ) -> Result<Vec<CommunityAssignment>> {
        let assignments = detect_communities(&self.graph_index, config).await?;
        if assignments.is_empty() {
            return Ok(assignments);
        }

        let tx = self.allocate_tx()?;

        for assignment in &assignments {
            let key = self.namespaced_key(
                format!("__graph:community:{}", assignment.entity_id.inner()).as_bytes(),
                4,
            );
            let val = serde_json::to_vec(&assignment.community_id)?;
            self.storage.put(tx, &key, &val).await?;
        }

        self.storage.commit(tx).await?;
        Ok(assignments)
    }

    /// Retrieves the persisted community ID for a given entity.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn get_community(&self, entity_id: EntityId) -> Result<Option<u64>> {
        let key = self.namespaced_key(
            format!("__graph:community:{}", entity_id.inner()).as_bytes(),
            4,
        );
        if let Some(bytes) = self.storage.get(&key).await? {
            let comm_id: u64 = serde_json::from_slice(&bytes).map_err(|e| {
                memfuse_core::MemFuseError::Internal(format!("community deserialize: {e}"))
            })?;
            Ok(Some(comm_id))
        } else {
            Ok(None)
        }
    }

    /// Removes all data belonging to this collection from storage.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn drop_collection(&self) -> Result<()> {
        let _guard = self.insert_lock.lock().await;
        let prefix = if self.name == "default" {
            return Err(memfuse_core::MemFuseError::invalid_input(
                "Cannot drop default collection",
            ));
        } else {
            self.prefix.clone()
        };

        let tx = self.next_tx()?;

        // 1. Clean collection data (user keys, docs, rels, intents)
        self.storage.delete_prefix(tx, &prefix).await?;

        // 2. Clean text index namespace (FIND-DB-002)
        let txt_prefix = format!("__txt:{}:", self.name).into_bytes();
        self.storage.delete_prefix(tx, &txt_prefix).await?;

        self.storage.commit(tx).await?;
        Ok(())
    }
}
