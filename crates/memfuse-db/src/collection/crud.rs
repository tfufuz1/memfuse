// FILE-CONTEXT
// ZWECK: CRUD-Operationen (Insert, Upsert, Update, Delete, Get) für Collection.
// INVARIANTEN: Atomare Multi-Index Commits via DbTransaction; Validierung aller Eingabegrenzen (ID-Länge, Batch-Größe).
// NICHT-OFFENSICHTLICH: check_doc_id_collision wird strikt innerhalb des insert_lock ausgeführt.
// STAND: TS:2026-08-29T17:22:29Z (SESSION: 0dcb9f3b)

use super::{ensure_importance_metadata, extract_text, Collection, StoredDocumentMeta};
use memfuse_core::{
    DocId, EntityId, Result, StorageEngine, TxId, VectorIndex, EXPIRY_METADATA_KEY,
};

pub(super) fn validate_doc_id(id: &str) -> Result<()> {
    if id.is_empty() {
        return Err(memfuse_core::MemFuseError::invalid_input(
            "Document ID cannot be empty",
        ));
    }
    if id.len() > 256 {
        return Err(memfuse_core::MemFuseError::invalid_input(
            "Document ID exceeds maximum length of 256 bytes",
        ));
    }
    if id.contains('\0') {
        return Err(memfuse_core::MemFuseError::invalid_input(
            "Document ID cannot contain null bytes",
        ));
    }
    Ok(())
}

pub(super) fn validate_embedding(embedding: &[f32]) -> Result<()> {
    if embedding.is_empty() {
        return Err(memfuse_core::MemFuseError::invalid_input(
            "Embedding vector cannot be empty",
        ));
    }
    if embedding.iter().all(|&x| x == 0.0) {
        return Err(memfuse_core::MemFuseError::invalid_input(
            "Zero vector embeddings are not allowed in regular Collection insertion. Use put_kv for non-vector entries.",
        ));
    }
    if embedding.iter().any(|&x| !x.is_finite()) {
        return Err(memfuse_core::MemFuseError::invalid_input(
            "Embedding vector contains NaN or Infinite values",
        ));
    }
    Ok(())
}

impl<S: StorageEngine, V: VectorIndex> Collection<S, V> {
    /// Inserts a text document, automatically generating its embedding.
    #[tracing::instrument(level = "trace", skip(self, text, metadata))]
    pub async fn insert_text_only(
        &self,
        id: &str,
        text: &str,
        mut metadata: Option<serde_json::Value>,
    ) -> Result<()> {
        let embedding = {
            let embedder = {
                let guard = self.embedder.read();
                guard
                    .as_ref()
                    .ok_or_else(|| {
                        memfuse_core::MemFuseError::Internal(
                            "No embedder configured for this collection".into(),
                        )
                    })?
                    .clone()
            };
            embedder.embed(text).await?
        };

        // Ensure text is in metadata for indexing
        let meta = metadata.get_or_insert(serde_json::json!({}));
        if let Some(obj) = meta.as_object_mut() {
            if !obj.contains_key("text") {
                obj.insert(
                    "text".to_string(),
                    serde_json::Value::String(text.to_string()),
                );
            }
        }

        self.insert(id, &embedding, metadata).await
    }

    /// Upserts a text document, automatically generating its embedding.
    #[tracing::instrument(level = "trace", skip(self, text, metadata))]
    pub async fn upsert_text_only(
        &self,
        id: &str,
        text: &str,
        mut metadata: Option<serde_json::Value>,
    ) -> Result<()> {
        let embedding = {
            let embedder = {
                let guard = self.embedder.read();
                guard
                    .as_ref()
                    .ok_or_else(|| {
                        memfuse_core::MemFuseError::Internal(
                            "No embedder configured for this collection".into(),
                        )
                    })?
                    .clone()
            };
            embedder.embed(text).await?
        };

        // Ensure text is in metadata for indexing
        let meta = metadata.get_or_insert(serde_json::json!({}));
        if let Some(obj) = meta.as_object_mut() {
            if !obj.contains_key("text") {
                obj.insert(
                    "text".to_string(),
                    serde_json::Value::String(text.to_string()),
                );
            }
        }

        self.upsert(id, &embedding, metadata).await
    }

    /// Inserts a document with a Sequence-based Time-To-Live (TTL in committed ops).
    #[tracing::instrument(level = "trace", skip(self, embedding, metadata))]
    pub async fn insert_with_ttl(
        &self,
        id: &str,
        embedding: &[f32],
        metadata: Option<serde_json::Value>,
        ttl_committed_ops: u64,
    ) -> Result<()> {
        let current_seq = self.snapshot_seq().await?;
        let expiry_seq = current_seq.saturating_add(ttl_committed_ops);

        let mut meta = metadata.unwrap_or_else(|| serde_json::json!({}));
        if let Some(obj) = meta.as_object_mut() {
            obj.insert(
                EXPIRY_METADATA_KEY.to_string(),
                serde_json::json!(expiry_seq),
            );
        } else {
            meta = serde_json::json!({
                EXPIRY_METADATA_KEY: expiry_seq
            });
        }

        self.insert(id, embedding, Some(meta)).await
    }

    /// Speichert ein Dokument mit expliziter kognitiver Gedächtnisklassifikation.
    ///
    /// # Memory Type Integration
    /// Der MemoryType wird als "memory_type"-Feld in die Metadaten eingebettet
    /// und ist für Lifecycle-Operationen (Decay, TTL, Sweep) abrufbar.
    pub async fn insert_typed(
        &self,
        id: &str,
        embedding: &[f32],
        memory_type: memfuse_core::MemoryType,
        metadata: Option<serde_json::Value>,
    ) -> Result<()> {
        let mut meta = metadata.unwrap_or_else(|| serde_json::json!({}));
        if let Some(obj) = meta.as_object_mut() {
            obj.insert(
                "memory_type".to_string(),
                serde_json::to_value(memory_type)
                    .map_err(|e| memfuse_core::MemFuseError::Serialization(e.to_string()))?,
            );
            // Setze Standard-Decay falls nicht gesetzt
            if !obj.contains_key("decay_function") {
                if let Ok(decay_val) = serde_json::to_value(memory_type.default_decay()) {
                    obj.insert("decay_function".to_string(), decay_val);
                }
            }
            // Setze Standard-TTL falls nicht gesetzt (Working Memory)
            if !obj.contains_key("ttl_tx") {
                if let Some(ttl) = memory_type.default_ttl_tx() {
                    obj.insert("ttl_tx".to_string(), serde_json::json!(ttl));
                }
            }
        }
        self.insert(id, embedding, Some(meta)).await
    }

    /// Inserts a document with an embedding and optional metadata.
    #[tracing::instrument(level = "trace", skip(self, embedding, metadata))]
    pub async fn insert(
        &self,
        id: &str,
        embedding: &[f32],
        metadata: Option<serde_json::Value>,
    ) -> Result<()> {
        let _guard = self.insert_lock.lock().await;
        self.insert_inner_unlocked(id, embedding, metadata).await
    }

    /// Stores a non-vector key-value entry directly in LSM storage without touching vector, text, or graph indices.
    #[tracing::instrument(level = "trace", skip(self, value))]
    pub async fn put_kv(&self, id: &str, value: &serde_json::Value) -> Result<()> {
        validate_doc_id(id)?;
        let _guard = self.kv_locks.lock_for(id).await;
        let tx = self.allocate_tx()?;
        let user_key = self.namespaced_key(id.as_bytes(), 0);
        let data = serde_json::to_vec(value)?;
        self.storage.put(tx, &user_key, &data).await?;
        self.storage.commit(tx).await?;
        Ok(())
    }

    /// Stores a non-vector key-value entry directly in LSM storage only if the key does not already exist.
    /// Returns `MemFuseError::Conflict` if the key is already present.
    #[tracing::instrument(level = "trace", skip(self, value))]
    pub async fn put_kv_if_absent(&self, id: &str, value: &serde_json::Value) -> Result<()> {
        validate_doc_id(id)?;
        let _guard = self.kv_locks.lock_for(id).await;
        let tx = self.allocate_tx()?;
        let user_key = self.namespaced_key(id.as_bytes(), 0);
        if self.storage.get(&user_key).await?.is_some() {
            self.storage.rollback(tx).await.ok();
            return Err(memfuse_core::MemFuseError::Conflict(format!(
                "Key '{}' already exists in collection KV store",
                id
            )));
        }
        let data = serde_json::to_vec(value)?;
        self.storage.put(tx, &user_key, &data).await?;
        self.storage.commit(tx).await?;
        Ok(())
    }

    /// Retrieves a key-value entry directly from LSM storage.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn get_kv(&self, id: &str) -> Result<Option<serde_json::Value>> {
        validate_doc_id(id)?;
        let key = self.namespaced_key(id.as_bytes(), 0);
        if let Some(data) = self.storage.get(&key).await? {
            let val: serde_json::Value = serde_json::from_slice(&data)?;
            return Ok(Some(val));
        }
        Ok(None)
    }

    /// Internal single document insert method without lock acquisition.
    ///
    /// Assumes `self.insert_lock` is held by caller to ensure TOCTOU safety.
    async fn insert_inner_unlocked(
        &self,
        id: &str,
        embedding: &[f32],
        metadata: Option<serde_json::Value>,
    ) -> Result<()> {
        if id.is_empty() {
            return Err(memfuse_core::MemFuseError::invalid_input(
                "Document ID cannot be empty",
            ));
        }
        if id.len() > 1024 {
            return Err(memfuse_core::MemFuseError::invalid_input(
                "Document ID length exceeds maximum allowed limit of 1024 bytes",
            ));
        }
        if embedding.len() != self.dimension {
            return Err(memfuse_core::MemFuseError::invalid_input(format!(
                "Dimension mismatch: expected {}, got {}",
                self.dimension,
                embedding.len()
            )));
        }
        validate_embedding(embedding)?;

        let db_tx = self.begin_transaction()?;

        match self.insert_op(&db_tx, id, embedding, metadata).await {
            Ok(_) => db_tx.commit().await,
            Err(e) => {
                if let Err(rollback_err) = db_tx.rollback().await {
                    tracing::error!("[INV-DB-3] Failed to rollback insert: {}", rollback_err);
                }
                Err(e)
            }
        }
    }

    /// Checks if a `doc_id` collision exists for a different user key string.
    ///
    /// Reads the `doc_key` mapping (key_type=1) for `doc_id`. If a document already exists under this `doc_id`
    /// but points to a different string key `id`, this indicates a 64-bit hash collision (BEFUND AGT-CORE-002).
    /// Returns `MemFuseError::Internal` to enforce fail-safe operation (ADR-016).
    pub(super) async fn check_doc_id_collision(&self, doc_id: DocId, id: &str) -> Result<()> {
        let doc_key = self.namespaced_key(&doc_id.inner().to_le_bytes(), 1);
        if let Some(val) = self.storage.get(&doc_key).await? {
            let existing_id = if let Ok(meta) = serde_json::from_slice::<StoredDocumentMeta>(&val) {
                Some(meta.id)
            } else {
                None
            };

            if let Some(existing) = existing_id {
                if existing != id {
                    return Err(memfuse_core::MemFuseError::Internal(format!(
                        "DocId-Kollision erkannt für Schlüssel '{id}' — bitte Support kontaktieren"
                    )));
                }
            }
        }
        Ok(())
    }

    pub async fn insert_op(
        &self,
        db_tx: &crate::transaction::DbTransaction<S, V>,
        id: &str,
        embedding: &[f32],
        metadata: Option<serde_json::Value>,
    ) -> Result<()> {
        validate_doc_id(id)?;
        validate_embedding(embedding)?;
        let tx = db_tx.tx_id;
        let doc_id = DocId::from_key(id)?;

        self.check_doc_id_collision(doc_id, id).await?;

        let mut metadata = metadata;
        let text_opt = extract_text(&metadata);
        ensure_importance_metadata(&mut metadata, tx, text_opt.as_deref());
        if let Some(serde_json::Value::Object(ref mut map)) = metadata {
            map.insert("updated_at_tx".to_string(), serde_json::json!(tx.inner()));
        }

        let meta = StoredDocumentMeta {
            id: id.to_string(),
            metadata: metadata.clone(),
        };

        // user_key (key_type=0) and doc_key (key_type=1) hold light metadata without embedding vector.
        // Vector index (HNSW) is the single source of truth for vector embeddings.
        let user_key = self.namespaced_key(id.as_bytes(), 0);
        let doc_key = self.namespaced_key(&doc_id.inner().to_le_bytes(), 1);

        let old_user_val = self.storage.get_at_seq(&user_key, u64::MAX).await?;
        let old_doc_val = self.storage.get_at_seq(&doc_key, u64::MAX).await?;

        let meta_data = serde_json::to_vec(&meta)?;
        self.storage.put(tx, &user_key, &meta_data).await?;
        self.storage.put(tx, &doc_key, &meta_data).await?;

        // Record for compensating transaction with pre-write values
        db_tx.record_keys_with_old_values(user_key, old_user_val, doc_key, old_doc_val, doc_id);

        self.index.insert(tx, doc_id, embedding).await?;

        // Stage text if present
        if let Some(text) = extract_text(&metadata) {
            db_tx.stage_text_insert(doc_id, text);
        }

        // Stage graph entity
        if let Ok(eid) = EntityId::from_key(id) {
            let entity = memfuse_core::Entity::new(eid, id, "Document");
            db_tx.stage_graph_entity(entity);
        }

        Ok(())
    }

    /// Inserts multiple documents in a single atomic transaction under a single lock scope.
    ///
    /// # Lock Granularity & Concurrency
    /// `insert_lock` is acquired **once** for the entire batch rather than once per document.
    /// The DocId collision check (`check_doc_id_collision`) is executed per document sequentially
    /// inside `insert_op` within this held lock, guaranteeing TOCTOU safety (§18.4, ADR-016).
    ///
    /// # Partial Failure & Atomicity (Option a)
    /// If an error occurs on any document in the batch (e.g., validation failure or DocId collision),
    /// the batch iteration is aborted immediately and `db_tx.rollback()` is invoked. All staged writes
    /// for previous documents in this transaction are discarded, ensuring atomic all-or-nothing
    /// batch behavior (Option a).
    ///
    /// # Performance
    /// Holding `insert_lock` once per batch avoids N-1 lock acquisitions and N-1 separate
    /// transaction commits, resulting in an expected 10-50x throughput improvement for bulk insertion workloads.
    #[tracing::instrument(level = "trace", skip(self, docs))]
    pub async fn insert_many(
        &self,
        docs: &[(String, Vec<f32>, Option<serde_json::Value>)],
    ) -> Result<()> {
        if docs.is_empty() {
            return Err(memfuse_core::MemFuseError::invalid_input(
                "insert_many requires at least one document",
            ));
        }
        if docs.len() > 10_000 {
            return Err(memfuse_core::MemFuseError::invalid_input(format!(
                "Batch size {} exceeds maximum allowed limit 10000",
                docs.len()
            )));
        }

        let _guard = self.insert_lock.lock().await;
        let db_tx = self.begin_transaction()?;

        for (id, embedding, metadata) in docs {
            if embedding.len() != self.dimension {
                if let Err(rollback_err) = db_tx.rollback().await {
                    tracing::error!(
                        "[INV-DB-3] Failed to rollback insert_many on dimension mismatch: {}",
                        rollback_err
                    );
                }
                return Err(memfuse_core::MemFuseError::invalid_input(format!(
                    "Dimension mismatch: expected {}, got {}",
                    self.dimension,
                    embedding.len()
                )));
            }
            if let Err(e) = validate_embedding(embedding) {
                if let Err(rollback_err) = db_tx.rollback().await {
                    tracing::error!(
                        "[INV-DB-3] Failed to rollback insert_many on invalid embedding: {}",
                        rollback_err
                    );
                }
                return Err(e);
            }

            if let Err(e) = self
                .insert_op(&db_tx, id, embedding, metadata.clone())
                .await
            {
                if let Err(rollback_err) = db_tx.rollback().await {
                    tracing::error!(
                        "[INV-DB-3] Failed to rollback insert_many: {}",
                        rollback_err
                    );
                }
                return Err(e);
            }
        }
        db_tx.commit().await
    }

    /// Upserts a document (inserts if missing, updates if exists) atomically.
    #[tracing::instrument(level = "trace", skip(self, embedding, metadata))]
    pub async fn upsert(
        &self,
        id: &str,
        embedding: &[f32],
        metadata: Option<serde_json::Value>,
    ) -> Result<()> {
        if id.is_empty() {
            return Err(memfuse_core::MemFuseError::invalid_input(
                "Document ID cannot be empty",
            ));
        }
        if id.len() > 1024 {
            return Err(memfuse_core::MemFuseError::invalid_input(
                "Document ID length exceeds maximum allowed limit of 1024 bytes",
            ));
        }
        if embedding.len() != self.dimension {
            return Err(memfuse_core::MemFuseError::invalid_input(format!(
                "Dimension mismatch: expected {}, got {}",
                self.dimension,
                embedding.len()
            )));
        }

        let _guard = self.insert_lock.lock().await;
        let db_tx = self.begin_transaction()?;
        let result = self.update_op(&db_tx, id, embedding, metadata).await;

        match result {
            Ok(_) => db_tx.commit().await,
            Err(e) => {
                if let Err(rollback_err) = db_tx.rollback().await {
                    tracing::error!("[INV-DB-3] Failed to rollback upsert: {}", rollback_err);
                }
                Err(e)
            }
        }
    }

    /// Upserts multiple documents in a single transaction.
    #[tracing::instrument(level = "trace", skip(self, docs))]
    pub async fn upsert_many(
        &self,
        docs: &[(String, Vec<f32>, Option<serde_json::Value>)],
    ) -> Result<()> {
        if docs.is_empty() {
            return Err(memfuse_core::MemFuseError::invalid_input(
                "upsert_many requires at least one document",
            ));
        }
        if docs.len() > 10_000 {
            return Err(memfuse_core::MemFuseError::invalid_input(format!(
                "Batch size {} exceeds maximum allowed limit 10000",
                docs.len()
            )));
        }
        let _guard = self.insert_lock.lock().await;
        let db_tx = self.begin_transaction()?;
        for (id, embedding, metadata) in docs {
            if embedding.len() != self.dimension {
                if let Err(rollback_err) = db_tx.rollback().await {
                    tracing::error!(
                        "[INV-DB-3] Failed to rollback upsert_many on dimension mismatch: {}",
                        rollback_err
                    );
                }
                return Err(memfuse_core::MemFuseError::invalid_input(format!(
                    "Dimension mismatch: expected {}, got {}",
                    self.dimension,
                    embedding.len()
                )));
            }
            if let Err(e) = validate_embedding(embedding) {
                if let Err(rollback_err) = db_tx.rollback().await {
                    tracing::error!(
                        "[INV-DB-3] Failed to rollback upsert_many on invalid embedding: {}",
                        rollback_err
                    );
                }
                return Err(e);
            }
            let result = self
                .update_op(&db_tx, id, embedding, metadata.clone())
                .await;
            if let Err(e) = result {
                if let Err(rollback_err) = db_tx.rollback().await {
                    tracing::error!(
                        "[INV-DB-3] Failed to rollback upsert_many: {}",
                        rollback_err
                    );
                }
                return Err(e);
            }
        }
        db_tx.commit().await
    }

    // AI-TAG[CONVENTION-DRIFT][MAJOR] RESOLVED: AGT-DB-001 — snapshot_seq() now propagates storage errors (TS:2026-08-25T00:00:00Z)
    // instead of silently mapping them to u64::MAX (ID: AGT-DB-001).
    // Consistent with every other error-propagation path in this file.
    pub async fn snapshot_seq(&self) -> Result<u64> {
        self.storage.last_seq_no().await
    }

    /// Retrieves a document by its user-provided string ID.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn get(&self, id: &str) -> Result<Option<crate::Document>> {
        self.get_at_snapshot(id, u64::MAX).await
    }

    /// Retrieves a document at a specific snapshot point.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn get_at_snapshot(&self, id: &str, seq_no: u64) -> Result<Option<crate::Document>> {
        validate_doc_id(id)?;
        let key = self.namespaced_key(id.as_bytes(), 0);
        if let Some(data) = self.storage.get_at_seq(&key, seq_no).await? {
            if let Ok(meta) = serde_json::from_slice::<StoredDocumentMeta>(&data) {
                return Ok(Some(crate::Document {
                    id: meta.id,
                    metadata: meta.metadata,
                }));
            } else if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&data) {
                return Ok(Some(crate::Document {
                    id: id.to_string(),
                    metadata: Some(val),
                }));
            }
        }
        Ok(None)
    }

    /// Updates an existing document in the collection.
    #[tracing::instrument(level = "trace", skip(self, embedding, metadata))]
    pub async fn update(
        &self,
        id: &str,
        embedding: &[f32],
        metadata: Option<serde_json::Value>,
    ) -> Result<()> {
        if embedding.len() != self.dimension {
            return Err(memfuse_core::MemFuseError::invalid_input(format!(
                "Dimension mismatch: expected {}, got {}",
                self.dimension,
                embedding.len()
            )));
        }

        let _guard = self.insert_lock.lock().await;
        let db_tx = self.begin_transaction()?;

        match self.update_op(&db_tx, id, embedding, metadata).await {
            Ok(_) => db_tx.commit().await,
            Err(e) => {
                if let Err(rollback_err) = db_tx.rollback().await {
                    tracing::error!("[INV-DB-3] Failed to rollback update: {}", rollback_err);
                }
                Err(e)
            }
        }
    }

    pub async fn update_op(
        &self,
        db_tx: &crate::transaction::DbTransaction<S, V>,
        id: &str,
        embedding: &[f32],
        metadata: Option<serde_json::Value>,
    ) -> Result<()> {
        validate_doc_id(id)?;
        validate_embedding(embedding)?;
        let tx = db_tx.tx_id;
        let doc_id = DocId::from_key(id)?;

        self.check_doc_id_collision(doc_id, id).await?;

        let user_key = self.namespaced_key(id.as_bytes(), 0);
        let doc_key = self.namespaced_key(&doc_id.inner().to_le_bytes(), 1);

        let old_user_val = self.storage.get_at_seq(&user_key, u64::MAX).await?;
        let old_doc_val = self.storage.get_at_seq(&doc_key, u64::MAX).await?;

        // Stage removal from old text index
        db_tx.stage_text_delete(doc_id);

        let mut metadata = metadata;
        let text_opt = extract_text(&metadata);
        ensure_importance_metadata(&mut metadata, tx, text_opt.as_deref());
        if let Some(serde_json::Value::Object(ref mut map)) = metadata {
            map.insert("updated_at_tx".to_string(), serde_json::json!(tx.inner()));
        }

        let meta = StoredDocumentMeta {
            id: id.to_string(),
            metadata: metadata.clone(),
        };
        let meta_data = serde_json::to_vec(&meta)?;

        self.storage.put(tx, &user_key, &meta_data).await?;
        self.storage.put(tx, &doc_key, &meta_data).await?;

        db_tx.record_keys_with_old_values(user_key, old_user_val, doc_key, old_doc_val, doc_id);

        // Stage re-insertion into text index if new text present
        if let Some(new_text) = extract_text(&metadata) {
            db_tx.stage_text_insert(doc_id, new_text);
        }

        // Stage graph entity update
        if let Ok(eid) = EntityId::from_key(id) {
            let entity = memfuse_core::Entity::new(eid, id, "Document");
            db_tx.stage_graph_entity(entity);
        }

        // Re-insert into HNSW
        // Recovery-Pfad ist HNSW-Rebuild (>20% deleted nodes) der mit LSM re-synct.
        if let Err(e) = self.index.delete(tx, doc_id).await {
            tracing::warn!(
                doc_id = ?doc_id,
                "HNSW soft-delete fehlgeschlagen: {e}. Doc wird nach HNSW-Rebuild nicht mehr in Vektorsuchen erscheinen."
            );
        }
        self.index.insert(tx, doc_id, embedding).await?;

        Ok(())
    }

    /// Returns the latest transaction ID for a document (via `updated_at_tx` or `created_at_tx`).
    pub async fn get_doc_tx(&self, doc_id: DocId) -> Result<Option<TxId>> {
        let doc_key = self.namespaced_key(&doc_id.inner().to_le_bytes(), 1);
        if let Some(bytes) = self.storage.get(&doc_key).await? {
            if let Ok(meta) = serde_json::from_slice::<StoredDocumentMeta>(&bytes) {
                if let Some(ref m) = meta.metadata {
                    if let Some(tx_val) = m.get("updated_at_tx") {
                        if let Some(u) = tx_val.as_u64() {
                            return Ok(Some(TxId::new(u)));
                        }
                    }
                    if let Some(imp) = m.get("importance") {
                        if let Some(tx_val) = imp.get("created_at_tx") {
                            if let Ok(tx) = serde_json::from_value::<TxId>(tx_val.clone()) {
                                return Ok(Some(tx));
                            }
                        }
                    }
                }
            }
        }
        Ok(None)
    }

    /// Deletes a document from the collection by its ID.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn delete(&self, id: &str) -> Result<()> {
        let _guard = self.insert_lock.lock().await;
        let mut db_tx = self.begin_transaction()?;

        match self.delete_op(&mut db_tx, id).await {
            Ok(_) => db_tx.commit().await,
            Err(e) => {
                if let Err(rollback_err) = db_tx.rollback().await {
                    tracing::error!("[INV-DB-3] Failed to rollback delete: {}", rollback_err);
                }
                Err(e)
            }
        }
    }

    pub async fn delete_op(
        &self,
        db_tx: &mut crate::transaction::DbTransaction<S, V>,
        id: &str,
    ) -> Result<()> {
        // Find doc keys and tombstone them
        let doc_id = DocId::from_key(id)?;
        let doc_key = self.namespaced_key(&doc_id.inner().to_le_bytes(), 1);
        let user_key = self.namespaced_key(id.as_bytes(), 0);

        let old_user_val = self.storage.get_at_seq(&user_key, u64::MAX).await?;
        let old_doc_val = self.storage.get_at_seq(&doc_key, u64::MAX).await?;

        let tx = db_tx.tx_id;

        db_tx.stage_text_delete(doc_id);

        if let Ok(eid) = EntityId::from_key(id) {
            db_tx.stage_graph_entity_delete(eid);
        }

        self.storage.delete(tx, &user_key).await?;
        self.storage.delete(tx, &doc_key).await?;

        db_tx.record_keys_with_old_values(user_key, old_user_val, doc_key, old_doc_val, doc_id);

        self.index.delete(tx, doc_id).await?;

        Ok(())
    }

    /// Links two memories together with a specific relation (Zettelkasten A-MEM).
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn link_memories(
        &self,
        from: DocId,
        to: DocId,
        relation: memfuse_core::types::domain::LinkRelation,
    ) -> Result<()> {
        if from == to {
            return Err(memfuse_core::MemFuseError::InvalidInput(
                "Cannot link a document to itself".into(),
            ));
        }

        let _guard = self.insert_lock.lock().await;

        // Prevent cycles for ALL relation types: if `to` transitively reaches `from`
        // via the same relation, adding `from -> to` creates a cycle.
        // For Supersedes, cycles cause post-RRF displacement of all docs in the cycle (BL-1).
        // For Associates/DerivedFrom/Elaborates, cycles cause unbounded BFS queue growth
        // in traverse_links (P0 audit fix).
        {
            let mut visited = std::collections::HashSet::new();
            let mut queue = std::collections::VecDeque::new();
            visited.insert(to);
            queue.push_back(to);

            let mut steps = 0u32;
            const MAX_BFS_STEPS: u32 = 1000;
            while let Some(curr) = queue.pop_front() {
                steps += 1;
                if steps > MAX_BFS_STEPS {
                    break;
                }
                if curr == from {
                    return Err(memfuse_core::MemFuseError::InvalidInput(format!(
                        "Cyclic {:?} relation detected: document {:?} transitively reaches {:?}",
                        relation, to, from
                    )));
                }
                let links = self.get_links(curr).await?;
                for link in links {
                    // Only follow edges of the same relation type to detect typed cycles
                    if link.relation == relation && visited.insert(link.target) {
                        queue.push_back(link.target);
                    }
                }
            }
        }

        let tx = self.allocate_tx()?;
        let doc_key = self.namespaced_key(&from.inner().to_le_bytes(), 1);

        if let Some(bytes) = self.storage.get_at_seq(&doc_key, u64::MAX).await? {
            let mut doc_id_str = None;
            let mut updated_links = false;

            // Determine which struct it was saved as
            if let Ok(mut meta) = serde_json::from_slice::<StoredDocumentMeta>(&bytes) {
                doc_id_str = Some(meta.id.clone());
                let meta_obj = meta.metadata.get_or_insert_with(|| serde_json::json!({}));
                if let Some(obj) = meta_obj.as_object_mut() {
                    let mut links: Vec<memfuse_core::types::domain::MemoryLink> = obj
                        .get("links")
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                        .unwrap_or_default();

                    // Check if link already exists to avoid duplicates
                    if !links
                        .iter()
                        .any(|l| l.target == to && l.relation == relation)
                    {
                        links.push(memfuse_core::types::domain::MemoryLink {
                            target: to,
                            relation,
                            created_at_tx: tx,
                        });

                        let links_val = serde_json::to_value(links).map_err(|e| {
                            memfuse_core::MemFuseError::Serialization(e.to_string())
                        })?;
                        obj.insert("links".to_string(), links_val);
                        obj.insert("updated_at_tx".to_string(), serde_json::json!(tx.inner()));
                        let updated_bytes = serde_json::to_vec(&meta)?;
                        self.storage.put(tx, &doc_key, &updated_bytes).await?;
                        updated_links = true;
                    }
                }
            }

            // Also update user_key (key_type=0) if links were updated and string id is known
            if updated_links {
                if let Some(ref id_str) = doc_id_str {
                    let user_key = self.namespaced_key(id_str.as_bytes(), 0);
                    if let Some(user_bytes) = self.storage.get_at_seq(&user_key, u64::MAX).await? {
                        if let Ok(mut meta_doc) =
                            serde_json::from_slice::<StoredDocumentMeta>(&user_bytes)
                        {
                            let doc_obj = meta_doc
                                .metadata
                                .get_or_insert_with(|| serde_json::json!({}));
                            if let Some(obj) = doc_obj.as_object_mut() {
                                let mut links: Vec<memfuse_core::types::domain::MemoryLink> = obj
                                    .get("links")
                                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                                    .unwrap_or_default();
                                if !links
                                    .iter()
                                    .any(|l| l.target == to && l.relation == relation)
                                {
                                    links.push(memfuse_core::types::domain::MemoryLink {
                                        target: to,
                                        relation,
                                        created_at_tx: tx,
                                    });
                                    let links_val = serde_json::to_value(links).map_err(|e| {
                                        memfuse_core::MemFuseError::Serialization(e.to_string())
                                    })?;
                                    obj.insert("links".to_string(), links_val);
                                    let new_user_bytes = serde_json::to_vec(&meta_doc)?;
                                    self.storage.put(tx, &user_key, &new_user_bytes).await?;
                                }
                            }
                        }
                    }
                }

                // Commit transaction to persist the link updates
                self.storage.commit(tx).await?;
            }
        }

        Ok(())
    }

    /// Retrieves all links for a given document.
    pub async fn get_links(
        &self,
        doc_id: DocId,
    ) -> Result<Vec<memfuse_core::types::domain::MemoryLink>> {
        let doc_key = self.namespaced_key(&doc_id.inner().to_le_bytes(), 1);
        if let Some(bytes) = self.storage.get_at_seq(&doc_key, u64::MAX).await? {
            if let Ok(meta) = serde_json::from_slice::<StoredDocumentMeta>(&bytes) {
                if let Some(obj) = meta.metadata.as_ref().and_then(|m| m.as_object()) {
                    return Ok(obj
                        .get("links")
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                        .unwrap_or_default());
                }
            }
        }
        Ok(Vec::new())
    }

    /// Scans documents in the collection that match a given key prefix.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn scan_prefix(&self, prefix: &str) -> Result<Vec<(String, serde_json::Value)>> {
        let real_prefix = if prefix.starts_with("__rel:") {
            self.namespaced_key(
                prefix.strip_prefix("__rel:").unwrap_or(prefix).as_bytes(),
                2,
            )
        } else {
            self.namespaced_key(prefix.as_bytes(), 0)
        };

        let kvs = self.storage.scan_prefix(&real_prefix).await?;

        let mut results = Vec::with_capacity(kvs.len());
        for (k, v) in kvs {
            let key_str = String::from_utf8_lossy(&k).to_string();
            // We should ideally strip the prefix to return the user-facing key
            // but for simplicity and compatibility with existing tests we keep it as is or strip carefully
            let user_key = if self.name == "default" {
                key_str
            } else {
                // Strip the internal prefix: self.prefix (variable) + 1 byte (key_type)
                let prefix_len = self.prefix.len() + 1;
                if key_str.len() >= prefix_len {
                    key_str[prefix_len..].to_string()
                } else {
                    key_str
                }
            };

            if let Ok(val) = serde_json::from_slice(&v) {
                results.push((user_key, val));
            }
        }
        Ok(results)
    }

    /// Performs semantic k-NN search over the collection's embeddings.
    #[tracing::instrument(level = "trace", skip(self, start, end))]
    pub async fn scan(
        &self,
        start: std::ops::Bound<&[u8]>,
        end: std::ops::Bound<&[u8]>,
    ) -> Result<Vec<(String, serde_json::Value)>> {
        use std::ops::Bound;

        let start_ns = match start {
            Bound::Included(b) => Bound::Included(self.namespaced_key(b, 0)),
            Bound::Excluded(b) => Bound::Excluded(self.namespaced_key(b, 0)),
            Bound::Unbounded => {
                if self.name == "default" {
                    Bound::Unbounded
                } else {
                    let mut b = self.prefix.clone();
                    b.push(0);
                    Bound::Included(b)
                }
            }
        };

        let end_ns = match end {
            Bound::Included(b) => Bound::Included(self.namespaced_key(b, 0)),
            Bound::Excluded(b) => Bound::Excluded(self.namespaced_key(b, 0)),
            Bound::Unbounded => {
                if self.name == "default" {
                    Bound::Unbounded
                } else {
                    let mut b = self.prefix.clone();
                    b.push(1);
                    Bound::Excluded(b)
                }
            }
        };

        let start_bytes = match &start_ns {
            Bound::Included(v) => Bound::Included(v.as_slice()),
            Bound::Excluded(v) => Bound::Excluded(v.as_slice()),
            Bound::Unbounded => Bound::Unbounded,
        };
        let end_bytes = match &end_ns {
            Bound::Included(v) => Bound::Included(v.as_slice()),
            Bound::Excluded(v) => Bound::Excluded(v.as_slice()),
            Bound::Unbounded => Bound::Unbounded,
        };

        let kvs = self.storage.scan(start_bytes, end_bytes).await?;
        let mut results = Vec::new();
        for (k, v) in kvs {
            let key_str = String::from_utf8_lossy(&k).to_string();
            let user_key = if self.name == "default" {
                key_str
            } else {
                let prefix_len = self.prefix.len() + 1;
                if key_str.len() >= prefix_len {
                    key_str[prefix_len..].to_string()
                } else {
                    key_str
                }
            };
            if let Ok(val) = serde_json::from_slice(&v) {
                results.push((user_key, val));
            }
        }
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use tokio::task::JoinSet;

    #[tokio::test]
    async fn test_concurrent_put_kv_if_absent_race() {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::MemFuse::open_with_config(
            dir.path(),
            crate::MemFuseConfig {
                dimension: 4,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let collection = Arc::new(db.collection("test_race").await.unwrap());

        let mut set = JoinSet::new();
        let key = "shared_race_key";

        for i in 0..50 {
            let col = collection.clone();
            let val = serde_json::json!({ "task_id": i });
            set.spawn(async move { col.put_kv_if_absent(key, &val).await });
        }

        let mut ok_count = 0;
        let mut conflict_count = 0;

        while let Some(res) = set.join_next().await {
            match res.unwrap() {
                Ok(_) => ok_count += 1,
                Err(memfuse_core::MemFuseError::Conflict(_)) => conflict_count += 1,
                Err(other) => panic!("Unexpected error: {:?}", other),
            }
        }

        assert_eq!(ok_count, 1, "Exactly one task must succeed");
        assert_eq!(
            conflict_count, 49,
            "49 tasks must fail with MemFuseError::Conflict"
        );
    }
}
