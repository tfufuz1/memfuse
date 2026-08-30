use super::{extract_text, parse_importance_score, Collection, StoredDocument, StoredDocumentMeta};
use memfuse_core::{
    DocId, EntityId, GraphIndex, MemFuseError, Result, StorageEngine, TextIndex, TxId, VectorIndex,
    EXPIRY_METADATA_KEY,
};
use memfuse_graph::{detect_communities, CommunityAssignment, CommunityDetectionConfig};
use std::sync::atomic::Ordering;

impl<S: StorageEngine, V: VectorIndex> Collection<S, V> {
    /// Repairs the index by re-syncing with the storage.
    ///
    /// Scans the storage for any documents that are missing from the index
    /// and reconciles them. This is critical for crash recovery.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn repair(&self) -> Result<()> {
        let _guard = self.insert_lock.lock().await;
        let mut repair_count = 0;
        let docs = self.storage.scan_prefix(&self.prefix).await?;
        // FIND-DB-004: Use doc_to_node map directly for O(1) lookup per DocId,
        // instead of iterating all nodes via all_doc_ids() which is O(N).
        let indexed_ids: std::collections::HashSet<DocId> =
            self.index.all_doc_ids().await?.into_iter().collect();

        tracing::info!("Starting integrity repair for collection '{}'", self.name);
        let start_time = std::time::Instant::now();

        // 1. Scan for pending transaction intents (2-Phase Commit Recovery — FIND-DB-005)
        let intent_prefix = self.namespaced_key(&[], 3);
        let intents = self.storage.scan_prefix(&intent_prefix).await?;
        let recovery_tx = self.next_tx()?;
        let mut recovered_any = false;
        let mut recovered_text = false;
        let mut recovered_graph = false;

        for (intent_key, intent_val) in intents {
            use crate::transaction::CommitIntent;
            if let Ok(intent_variant) = serde_json::from_slice::<CommitIntent>(&intent_val) {
                let (doc_ids, has_text, has_graph) = match intent_variant {
                    CommitIntent::Pending {
                        doc_ids,
                        has_text,
                        has_graph,
                    } => (doc_ids, has_text, has_graph),
                    _ => continue,
                };

                tracing::info!(
                    "Found pending transaction intent, recovering {} documents (has_text={}, has_graph={})",
                    doc_ids.len(),
                    has_text,
                    has_graph
                );

                for doc_id in doc_ids {
                    let doc_key = self.namespaced_key(&doc_id.inner().to_le_bytes(), 1);
                    if let Some(val) = self.storage.get(&doc_key).await? {
                        let meta_id = serde_json::from_slice::<StoredDocumentMeta>(&val)
                            .map(|m| m.id)
                            .ok();

                        let mut stored_doc = None;
                        if let Some(ref id_str) = meta_id {
                            let user_key = self.namespaced_key(id_str.as_bytes(), 0);
                            if let Some(user_val) = self.storage.get(&user_key).await? {
                                if let Ok(stored) =
                                    serde_json::from_slice::<StoredDocument>(&user_val)
                                {
                                    stored_doc = Some(stored);
                                }
                            }
                        }

                        if stored_doc.is_none() {
                            if let Ok(full) = serde_json::from_slice::<StoredDocument>(&val) {
                                stored_doc = Some(full);
                            }
                        }

                        if let Some(stored) = stored_doc {
                            if !indexed_ids.contains(&doc_id) {
                                self.index
                                    .insert(recovery_tx, doc_id, &stored.embedding)
                                    .await?;
                                repair_count += 1;
                                recovered_any = true;
                            }

                            if has_text {
                                if let Some(text) = extract_text(&stored.metadata) {
                                    self.text_index
                                        .upsert_document(recovery_tx, doc_id, &text)
                                        .await?;
                                    recovered_text = true;
                                }
                            }

                            if has_graph {
                                if let Ok(eid) = EntityId::from_key(&stored.id) {
                                    let entity =
                                        memfuse_core::Entity::new(eid, &stored.id, "Document");
                                    let _ = self.graph_index.add_entity(recovery_tx, entity).await;
                                    recovered_graph = true;
                                }
                            }
                        }
                    }
                }
                // Cleanup recovered intent
                if let Err(e) = self.storage.delete(recovery_tx, &intent_key).await {
                    tracing::warn!(key = ?intent_key, "Konnte wiederhergestellte TxIntent nicht löschen: {e}");
                }
            }
        }
        if recovered_any {
            self.index.commit(recovery_tx).await?;
        }
        if recovered_text {
            self.text_index.commit(recovery_tx).await?;
        }
        if recovered_graph {
            self.graph_index.commit(recovery_tx).await?;
        }

        // 2. Fallback: Full scan for documents missing from index (FIND-DB-004: Parallel Batching)
        let fallback_tx = self.next_tx()?;
        let mut fallback_any = false;
        let mut fallback_text = false;

        for (namespaced_key, value) in docs {
            // Only process user data (key_type 0)
            if self.name != "default" {
                if namespaced_key.get(self.prefix.len()) != Some(&0) {
                    continue;
                }
            } else if namespaced_key.starts_with(b"__") {
                continue;
            }

            let stored: StoredDocument = match serde_json::from_slice(&value) {
                Ok(d) => d,
                Err(e) => {
                    tracing::debug!(
                        key = ?namespaced_key,
                        error = %e,
                        "Überspringe nicht-deserialisierbare Einträge bei repair (erwartet für Metadaten-Keys)"
                    );
                    continue;
                }
            };

            let doc_id = DocId::from_key(&stored.id)?;
            if !indexed_ids.contains(&doc_id) {
                self.index
                    .insert(fallback_tx, doc_id, &stored.embedding)
                    .await?;
                repair_count += 1;
                fallback_any = true;
            }

            // Ensure text index coverage
            if let Some(text) = extract_text(&stored.metadata) {
                if let Ok(bm25_res) = self.text_index.search_bm25(&text, 1, None).await {
                    if !bm25_res.iter().any(|(id, _)| *id == doc_id) {
                        self.text_index
                            .upsert_document(fallback_tx, doc_id, &text)
                            .await?;
                        fallback_text = true;
                    }
                }
            }
        }

        if fallback_any {
            self.index.commit(fallback_tx).await?;
        }
        if fallback_text {
            self.text_index.commit(fallback_tx).await?;
        }

        if repair_count > 0 {
            tracing::info!(
                "Repaired {} missing documents in collection '{}' in {:?}",
                repair_count,
                self.name,
                start_time.elapsed()
            );
        } else {
            tracing::debug!(
                "Integrity check passed for collection '{}' in {:?}",
                self.name,
                start_time.elapsed()
            );
        }

        Ok(())
    }

    /// Returns statistics for the collection's vector index.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn stats(&self) -> Result<memfuse_core::VectorIndexStats> {
        self.index.stats().await
    }

    /// Rebuilds the HNSW index from storage.
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

    /// Scans the collection for documents with expired TTLs or decayed importance scores and deletes them.
    ///
    /// Reads `created_at_ms` (or `timestamp_ms`) and `ttl_ms` from document metadata for wall-clock TTL,
    /// and `importance` metadata for TxId-based decay sweep (`effective_score < DECAY_DELETION_THRESHOLD`).
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn trigger_reaper(&self) -> Result<usize> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| memfuse_core::MemFuseError::Internal(e.to_string()))?
            .as_millis() as u64;

        let now_tx = self.next_tx.load(Ordering::SeqCst);
        let docs = self.scan_prefix("").await?;
        let mut expired_ids = Vec::new();

        for (id, val) in docs {
            if self.name == "default" && id.starts_with("__") {
                continue;
            }

            let meta_obj = val
                .get("metadata")
                .and_then(|m| m.as_object())
                .or_else(|| val.as_object());

            let mut marked_for_deletion = false;

            if let Some(obj) = meta_obj {
                // 1. Working-Memory wall-clock TTL check ZUERST
                if let Some(ttl_val) = obj.get("ttl_ms").and_then(|v| v.as_u64()) {
                    if ttl_val > 0 {
                        if let Some(created_at) = obj
                            .get("created_at_ms")
                            .or_else(|| obj.get("timestamp_ms"))
                            .and_then(|v| v.as_u64())
                        {
                            if let Some(expire_at) = created_at.checked_add(ttl_val) {
                                if now_ms >= expire_at {
                                    expired_ids.push(id.clone());
                                    marked_for_deletion = true;
                                }
                            }
                        }
                    }
                }

                // 2. TxId-basierter Decay-Sweep (nur wenn decay != None)
                if !marked_for_deletion {
                    if let Some(imp_val) = obj.get("importance") {
                        if let Ok(imp) = serde_json::from_value::<memfuse_core::MemoryImportance>(
                            imp_val.clone(),
                        ) {
                            if imp.decay != memfuse_core::DecayFunction::None {
                                let effective = imp.effective_score(TxId::new(now_tx));
                                if effective < Self::DECAY_DELETION_THRESHOLD {
                                    expired_ids.push(id);
                                }
                            }
                        }
                    }
                }
            }
        }

        let count = expired_ids.len();
        for id in expired_ids {
            match self.delete(&id).await {
                Ok(_) => {
                    tracing::debug!(
                        collection = %self.name,
                        doc_id = %id,
                        "Reaped expired TTL / decayed document"
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

    /// Bewertet die Wichtigkeit eines Dokuments via LLM (Ollama) und
    /// aktualisiert den Importance-Score in den Metadaten.
    ///
    /// # Fehlerverhalten
    /// Bei LLM-Fehler wird der bestehende Score NICHT überschrieben.
    /// Fehler werden als Err(MemFuseError::Internal) zurückgegeben.
    pub async fn evaluate_importance_with_llm(
        &self,
        doc_id: &str,
        ollama: &memfuse_ollama::OllamaClient,
    ) -> Result<memfuse_core::ImportanceScore> {
        let user_key = self.namespaced_key(doc_id.as_bytes(), 0);
        let Some(data) = self.storage.get(&user_key).await? else {
            return Err(memfuse_core::MemFuseError::NotFound(format!(
                "Document not found: {doc_id}"
            )));
        };
        let mut stored: StoredDocument = serde_json::from_slice(&data)?;

        let text = extract_text(&stored.metadata).unwrap_or_else(|| stored.id.clone());

        let prompt = format!(
            "Bewerte die langfristige Wichtigkeit dieser Information für einen KI-Agenten \
             auf einer Skala von 0.0 (unwichtig, vergänglich) bis 1.0 (sehr wichtig, dauerhaft).\n\
             Antworte NUR mit einer Dezimalzahl zwischen 0.0 und 1.0, ohne Erklärung.\n\n\
             Information: {}\n\nWichtigkeits-Score:",
            text.chars().take(500).collect::<String>()
        );

        let model = &ollama.config().model;
        let response = ollama.generate_text(model, &prompt).await.map_err(|e| {
            memfuse_core::MemFuseError::Internal(format!("LLM importance evaluation failed: {e}"))
        })?;

        let score = parse_importance_score(&response);
        let importance_score = memfuse_core::ImportanceScore::new(score);

        let tx = self.allocate_tx()?;
        let doc_id_typed = DocId::from_key(doc_id)?;
        let doc_key = self.namespaced_key(&doc_id_typed.inner().to_le_bytes(), 1);

        let meta_obj = match stored.metadata {
            Some(serde_json::Value::Object(ref mut map)) => map,
            _ => {
                stored.metadata = Some(serde_json::json!({}));
                match stored.metadata {
                    Some(serde_json::Value::Object(ref mut map)) => map,
                    _ => {
                        return Err(MemFuseError::Serialization(
                            "Failed to initialize document metadata map".to_string(),
                        ))
                    }
                }
            }
        };

        let imp = if let Some(imp_val) = meta_obj.get("importance") {
            if let Ok(mut existing_imp) =
                serde_json::from_value::<memfuse_core::MemoryImportance>(imp_val.clone())
            {
                existing_imp.base_score = importance_score;
                existing_imp
            } else {
                memfuse_core::MemoryImportance::new(
                    importance_score,
                    memfuse_core::DecayFunction::None,
                    tx,
                )
            }
        } else {
            memfuse_core::MemoryImportance::new(
                importance_score,
                memfuse_core::DecayFunction::None,
                tx,
            )
        };

        if let Ok(val) = serde_json::to_value(imp) {
            meta_obj.insert("importance".to_string(), val);
        }

        let meta_only = StoredDocumentMeta::from(&stored);
        let user_bytes = serde_json::to_vec(&stored)?;
        let doc_bytes = serde_json::to_vec(&meta_only)?;

        let _guard = self.insert_lock.lock().await;
        self.storage.put(tx, &user_key, &user_bytes).await?;
        self.storage.put(tx, &doc_key, &doc_bytes).await?;
        self.storage.commit(tx).await?;

        Ok(importance_score)
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
