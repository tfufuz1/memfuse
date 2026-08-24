//! Logically isolated Collections inside the MemFuse database.
// INVARIANT: Logische Isolation (Namespaces).
// PREFIXING: Jeder Key im LSM bekommt das Prefix `__col:{name}:\x00`.

use crate::filter::MetadataFilter;
use memfuse_core::TextEmbeddingEngine;
use memfuse_core::{DocId, GraphIndex, Result, StorageEngine, TextIndex, TxId, VectorIndex};
use memfuse_graph::CsrGraph;
use memfuse_index::HnswIndex;
use memfuse_store::LsmStorage;
use memfuse_text::inverted::InvertedIndex;
use memfuse_text::Language;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Vollständiges Dokument (für user_key, key_type=0) — enthält Embedding.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct StoredDocument {
    pub id: String,
    pub embedding: Vec<f32>,
    pub metadata: Option<serde_json::Value>,
}

/// Leichtgewichtige Metadaten (für doc_key, key_type=1) — KEIN Embedding.
/// Wird für DocId-basierte Hydration nach HNSW/BM25-Suche verwendet.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct StoredDocumentMeta {
    pub id: String,
    pub metadata: Option<serde_json::Value>,
}

impl From<&StoredDocument> for StoredDocumentMeta {
    fn from(doc: &StoredDocument) -> Self {
        Self {
            id: doc.id.clone(),
            metadata: doc.metadata.clone(),
        }
    }
}

/// Helper to unify how we extract text from metadata.
fn extract_text(metadata: &Option<serde_json::Value>) -> Option<String> {
    let mut document_text = String::new();
    if let Some(m) = metadata {
        if let Some(m_obj) = m.as_object() {
            if let Some(s) = m_obj.get("text").and_then(|v| v.as_str()) {
                document_text.push_str(s);
                document_text.push(' ');
            }
            if let Some(s) = m_obj.get("content").and_then(|v| v.as_str()) {
                document_text.push_str(s);
                document_text.push(' ');
            }
        }
    }
    if document_text.is_empty() {
        None
    } else {
        Some(document_text.trim().to_string())
    }
}

/// A logically isolated collection of documents (namespace).
///
/// Each collection provides its own HNSW vector index and inverted text index,
/// while sharing the underlying LSM-Tree storage with other collections.
pub struct Collection<S: StorageEngine = LsmStorage> {
    pub(crate) name: String,
    pub(crate) prefix: Vec<u8>,
    pub(crate) index: Arc<HnswIndex>,
    pub(crate) text_index: InvertedIndex<S>,
    pub(crate) graph_index: Arc<CsrGraph>,
    pub(crate) storage: Arc<S>,
    pub(crate) next_tx: Arc<AtomicU64>,
    pub(crate) dimension: usize,
    pub(crate) embedder: parking_lot::RwLock<Option<Arc<dyn TextEmbeddingEngine>>>,
}

impl<S: StorageEngine> Clone for Collection<S> {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            prefix: self.prefix.clone(),
            index: self.index.clone(),
            text_index: self.text_index.clone(),
            graph_index: self.graph_index.clone(),
            storage: self.storage.clone(),
            next_tx: self.next_tx.clone(),
            dimension: self.dimension,
            embedder: parking_lot::RwLock::new(self.embedder.read().as_ref().map(Arc::clone)),
        }
    }
}

impl<S: StorageEngine> Collection<S> {
    /// Creates a new `Collection` instance with explicit language configuration.
    ///
    /// The `language` parameter controls the BM25 tokenizer. Use `Language::German`
    /// for German compound splitting, `Language::English` (default) for standard
    /// whitespace tokenization.
    pub fn new(
        name: String,
        storage: Arc<S>,
        index: Arc<HnswIndex>,
        graph_index: Arc<CsrGraph>,
        next_tx: Arc<AtomicU64>,
        dimension: usize,
        language: Language,
    ) -> Self {
        let prefix = if name == "default" {
            b"".to_vec()
        } else {
            format!("__col:{}:\x00", name).into_bytes()
        };

        let text_index = InvertedIndex::new_with_language(storage.clone(), &name, language);

        Self {
            name,
            prefix,
            index,
            text_index,
            graph_index,
            storage,
            next_tx,
            dimension,
            embedder: parking_lot::RwLock::new(None),
        }
    }

    /// Returns the CSR graph index for this collection.
    pub fn graph_index(&self) -> Arc<CsrGraph> {
        self.graph_index.clone()
    }

    /// Sets the text embedder for this collection (consuming version).
    #[tracing::instrument(level = "trace", skip(self, embedder))]
    pub fn with_embedder(self, embedder: Arc<dyn TextEmbeddingEngine>) -> Self {
        {
            let mut guard = self.embedder.write();
            *guard = Some(embedder);
        }
        self
    }

    /// Configures the text embedder for this collection.
    #[tracing::instrument(level = "trace", skip(self, embedder))]
    pub async fn set_embedder(&self, embedder: Arc<dyn TextEmbeddingEngine>) -> Result<()> {
        let mut guard = self.embedder.write();
        *guard = Some(embedder);
        Ok(())
    }

    /// Internal helper to generate namespaced keys.
    /// key_type: 0 = user key, 1 = docid mapping, 2 = relationship, 3 = tx intent
    pub(crate) fn namespaced_key(&self, key: &[u8], key_type: u8) -> Vec<u8> {
        if self.name == "default" {
            match key_type {
                0 => key.to_vec(),
                1 => {
                    let mut k = Vec::with_capacity(8 + key.len());
                    k.extend_from_slice(b"__docid:");
                    k.extend_from_slice(key);
                    k
                }
                2 => {
                    let mut k = Vec::with_capacity(6 + key.len());
                    k.extend_from_slice(b"__rel:");
                    k.extend_from_slice(key);
                    k
                }
                3 => {
                    let mut k = b"__tx_intent:".to_vec();
                    k.extend_from_slice(key);
                    k
                }
                _ => key.to_vec(),
            }
        } else {
            let mut k = Vec::with_capacity(self.prefix.len() + 1 + key.len());
            k.extend_from_slice(&self.prefix);
            k.push(key_type);
            k.extend_from_slice(key);
            k
        }
    }

    /// Repairs the index by re-syncing with the storage.
    ///
    /// Scans the storage for any documents that are missing from the index
    /// and reconciles them. This is critical for crash recovery.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn repair(&self) -> Result<()> {
        let mut repair_count = 0;
        let docs = self.storage.scan_prefix(&self.prefix).await?;
        // FIND-DB-004: Use doc_to_node map directly for O(1) lookup per DocId,
        // instead of iterating all nodes via all_doc_ids() which is O(N).
        let indexed_ids: std::collections::HashSet<DocId> =
            self.index.all_doc_ids_from_map().into_iter().collect();

        tracing::info!("Starting integrity repair for collection '{}'", self.name);
        let start_time = std::time::Instant::now();

        // 1. Scan for pending transaction intents (2-Phase Commit Recovery — FIND-DB-005)
        let intent_prefix = self.namespaced_key(&[], 3);
        let intents = self.storage.scan_prefix(&intent_prefix).await?;
        let recovery_tx = TxId::new(self.next_tx.fetch_add(1, Ordering::SeqCst));
        let mut recovered_any = false;

        for (intent_key, intent_val) in intents {
            use crate::transaction::CommitIntent;
            if let Ok(CommitIntent::Pending { doc_ids }) =
                serde_json::from_slice::<CommitIntent>(&intent_val)
            {
                tracing::info!(
                    "Found pending transaction intent, recovering {} documents",
                    doc_ids.len()
                );
                for doc_id in doc_ids {
                    if !indexed_ids.contains(&doc_id) {
                        let doc_key = self.namespaced_key(&doc_id.inner().to_le_bytes(), 1);
                        if let Some(val) = self.storage.get(&doc_key).await? {
                            if let Ok(stored) = serde_json::from_slice::<StoredDocument>(&val) {
                                self.index
                                    .insert(recovery_tx, doc_id, &stored.embedding)
                                    .await?;
                                repair_count += 1;
                                recovered_any = true;
                            }
                        }
                    }
                }
                // Cleanup recovered intent
                let _ = self.storage.delete(recovery_tx, &intent_key).await;
            }
        }
        if recovered_any {
            self.index.commit(recovery_tx).await?;
        }

        // 2. Fallback: Full scan for documents missing from index (FIND-DB-004: Parallel Batching)
        let fallback_tx = TxId::new(self.next_tx.fetch_add(1, Ordering::SeqCst));
        let mut fallback_any = false;

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
        }

        if fallback_any {
            self.index.commit(fallback_tx).await?;
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

    /// Begins a new atomic transaction for this collection.
    #[tracing::instrument(level = "trace", skip(self))]
    pub fn begin_transaction(&self) -> crate::transaction::DbTransaction<S> {
        let tx = TxId::new(self.next_tx.fetch_add(1, Ordering::SeqCst));
        crate::transaction::DbTransaction::new(self.clone(), tx)
    }

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

    /// Inserts a document with an embedding and optional metadata.
    #[tracing::instrument(level = "trace", skip(self, embedding, metadata))]
    pub async fn insert(
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

        let db_tx = self.begin_transaction();

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
    pub(crate) async fn check_doc_id_collision(&self, doc_id: DocId, id: &str) -> Result<()> {
        let doc_key = self.namespaced_key(&doc_id.inner().to_le_bytes(), 1);
        if let Some(val) = self.storage.get(&doc_key).await? {
            let existing_id = if let Ok(meta) = serde_json::from_slice::<StoredDocumentMeta>(&val) {
                Some(meta.id)
            } else if let Ok(full) = serde_json::from_slice::<StoredDocument>(&val) {
                Some(full.id)
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
        db_tx: &crate::transaction::DbTransaction<S>,
        id: &str,
        embedding: &[f32],
        metadata: Option<serde_json::Value>,
    ) -> Result<()> {
        let tx = db_tx.tx_id;
        let doc_id = DocId::from_key(id)?;

        self.check_doc_id_collision(doc_id, id).await?;

        let stored = StoredDocument {
            id: id.to_string(),
            embedding: embedding.to_vec(),
            metadata: metadata.clone(),
        };
        let meta_only = StoredDocumentMeta::from(&stored);

        // user_key (key_type=0): Vollständiges Dokument (für get() und repair())
        // Document serialization is unencrypted before being sent to storage.
        // If Encryption-at-Rest is enabled, it's encrypted in the storage layer (WP-3.2).
        let user_key = self.namespaced_key(id.as_bytes(), 0);
        let data = serde_json::to_vec(&stored)?;
        self.storage.put(tx, &user_key, &data).await?;

        // doc_key (key_type=1): NUR Metadaten (für Hydration nach Vektorsuche)
        let doc_key = self.namespaced_key(&doc_id.inner().to_le_bytes(), 1);
        let meta_data = serde_json::to_vec(&meta_only)?;
        self.storage.put(tx, &doc_key, &meta_data).await?;

        // Record for compensating transaction
        db_tx.record_keys(user_key, doc_key, doc_id);

        self.index.insert(tx, doc_id, embedding).await?;

        // Index text if present
        if let Some(text) = extract_text(&metadata) {
            self.text_index.upsert_document(tx, doc_id, &text).await?;
        }

        Ok(())
    }

    /// Inserts multiple documents in a single transaction.
    #[tracing::instrument(level = "trace", skip(self, docs))]
    pub async fn insert_many(
        &self,
        docs: &[(String, Vec<f32>, Option<serde_json::Value>)],
    ) -> Result<()> {
        let db_tx = self.begin_transaction();
        for (id, embedding, metadata) in docs {
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
        if embedding.len() != self.dimension {
            return Err(memfuse_core::MemFuseError::invalid_input(format!(
                "Dimension mismatch: expected {}, got {}",
                self.dimension,
                embedding.len()
            )));
        }

        let db_tx = self.begin_transaction();
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
        let db_tx = self.begin_transaction();
        for (id, embedding, metadata) in docs {
            if embedding.len() != self.dimension {
                let _ = db_tx.rollback().await;
                return Err(memfuse_core::MemFuseError::invalid_input(format!(
                    "Dimension mismatch: expected {}, got {}",
                    self.dimension,
                    embedding.len()
                )));
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

    // AI-TAG[CONVENTION-DRIFT][MAJOR] RESOLVED: snapshot_seq() now propagates storage errors
    // instead of silently mapping them to u64::MAX (ID: AGT-DB-001).
    // Consistent with every other error-propagation path in this file.
    async fn snapshot_seq(&self) -> Result<u64> {
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
        let key = self.namespaced_key(id.as_bytes(), 0);
        if let Some(data) = self.storage.get_at_seq(&key, seq_no).await? {
            let stored: StoredDocument = serde_json::from_slice(&data)?;
            return Ok(Some(crate::Document {
                id: stored.id,
                metadata: stored.metadata,
            }));
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

        let db_tx = self.begin_transaction();

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
        db_tx: &crate::transaction::DbTransaction<S>,
        id: &str,
        embedding: &[f32],
        metadata: Option<serde_json::Value>,
    ) -> Result<()> {
        let tx = db_tx.tx_id;
        let doc_id = DocId::from_key(id)?;

        self.check_doc_id_collision(doc_id, id).await?;

        let user_key = self.namespaced_key(id.as_bytes(), 0);

        // Remove from old text index
        self.text_index.delete_document(tx, doc_id).await?;

        let stored = StoredDocument {
            id: id.to_string(),
            embedding: embedding.to_vec(),
            metadata: metadata.clone(),
        };
        let meta_only = StoredDocumentMeta::from(&stored);
        let data = serde_json::to_vec(&stored)?;

        let doc_key = self.namespaced_key(&doc_id.inner().to_le_bytes(), 1);
        let meta_data = serde_json::to_vec(&meta_only)?;

        self.storage.put(tx, &user_key, &data).await?;
        self.storage.put(tx, &doc_key, &meta_data).await?;

        db_tx.record_keys(user_key, doc_key, doc_id);

        // Re-insert into text index if new text present
        if let Some(new_text) = extract_text(&metadata) {
            self.text_index
                .upsert_document(tx, doc_id, &new_text)
                .await?;
        }

        // Re-insert into HNSW
        let _ = self.index.delete(tx, doc_id).await;
        self.index.insert(tx, doc_id, embedding).await?;

        Ok(())
    }

    /// Deletes a document from the collection by its ID.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn delete(&self, id: &str) -> Result<()> {
        let mut db_tx = self.begin_transaction();

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
        db_tx: &mut crate::transaction::DbTransaction<S>,
        id: &str,
    ) -> Result<()> {
        let tx = db_tx.tx_id;
        let doc_id = DocId::from_key(id)?;

        let user_key = self.namespaced_key(id.as_bytes(), 0);

        // Remove from old text index
        self.text_index.delete_document(tx, doc_id).await?;

        let doc_key = self.namespaced_key(&doc_id.inner().to_le_bytes(), 1);

        self.storage.delete(tx, &user_key).await?;
        self.storage.delete(tx, &doc_key).await?;

        db_tx.record_keys(user_key, doc_key, doc_id);

        let _ = self.index.delete(tx, doc_id).await;

        Ok(())
    }

    /// Creates a directional relationship between two documents in the collection.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn relate(&self, from: &str, to: &str, label: &str) -> Result<()> {
        let tx = TxId::new(self.next_tx.fetch_add(1, Ordering::SeqCst));
        let key_str = format!("{}:{}:{}", from, label, to);
        let key = self.namespaced_key(key_str.as_bytes(), 2);
        let val = serde_json::json!({
            "from": from,
            "to": to,
            "label": label,
        });
        let bytes = serde_json::to_vec(&val)?;

        self.storage.put(tx, &key, &bytes).await?;
        self.storage.commit(tx).await?;
        Ok(())
    }

    /// Creates a bidirectional relationship atomically.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn relate_bidirectional(&self, from: &str, to: &str, label: &str) -> Result<()> {
        let db_tx = self.begin_transaction();
        let tx = db_tx.tx_id;

        let key1_str = format!("{}:{}:{}", from, label, to);
        let key1 = self.namespaced_key(key1_str.as_bytes(), 2);
        let val1 = serde_json::json!({"from": from, "to": to, "label": label});
        let bytes1 = serde_json::to_vec(&val1)?;
        self.storage.put(tx, &key1, &bytes1).await?;

        let key2_str = format!("{}:{}:{}", to, label, from);
        let key2 = self.namespaced_key(key2_str.as_bytes(), 2);
        let val2 = serde_json::json!({"from": to, "to": from, "label": label});
        let bytes2 = serde_json::to_vec(&val2)?;
        self.storage.put(tx, &key2, &bytes2).await?;

        db_tx.commit().await?;
        Ok(())
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
    #[tracing::instrument(level = "trace", skip(self, query_embedding))]
    pub async fn search(
        &self,
        query_embedding: &[f32],
        k: usize,
    ) -> Result<Vec<crate::SearchResult>> {
        self.search_with_filter(query_embedding, k, None).await
    }

    /// Performs semantic search with an advanced metadata filter.
    #[tracing::instrument(level = "trace", skip(self, query, filter))]
    pub async fn search_with_filter(
        &self,
        query: &[f32],
        k: usize,
        filter: Option<MetadataFilter>,
    ) -> Result<Vec<crate::SearchResult>> {
        // 🛡️ SICHERUNG: Snapshot-Isolation (FIND-DB-003)
        // Wir pinnen den Snapshot für die gesamte Dauer der gefilterten Suche,
        // um Konsistenz zwischen Vektor-Index, Metadaten-Filter und Re-Hydrierung zu garantieren.
        let seq = self.snapshot_seq().await?;
        self.storage.pin_checkpoint(seq).await?;

        let res = async {
            let filter = match filter {
                Some(f) => f,
                None => return self.search_filtered_at(query, k, None, seq).await,
            };

            let total_docs = self.len().await;

            // ADAPTIVE STRATEGY (WP-4.2):
            // If total documents are few, or if we suspect high selectivity,
            // we use Pre-filtering by scanning metadata first.
            // For now, we use a simple heuristic: if docs < 1000, always pre-filter.
            if total_docs < 1000 {
                let matched_ids = self.get_matching_doc_ids_at(&filter, seq).await?;

                // If no docs match the filter, return early
                if matched_ids.is_empty() {
                    return Ok(Vec::new());
                }

                let filter_fn = move |id: DocId| matched_ids.contains(&id);
                let scored_docs = self
                    .index
                    .search_filtered(query, k, Some(&filter_fn))
                    .await?;
                self.hydrate_from_scored_at(scored_docs, seq).await
            } else {
                // Post-filtering approach for larger collections:
                // 1. Search more than k (oversample) to account for filter drops.
                let oversample = (k * 10).min(total_docs).max(k);
                let scored_docs = self.index.search_filtered(query, oversample, None).await?;

                let mut results = Vec::new();
                for sd in scored_docs {
                    let doc_key = self.namespaced_key(&sd.doc_id.inner().to_le_bytes(), 1);
                    if let Some(bytes) = self.storage.get_at_seq(&doc_key, seq).await? {
                        let (id, doc_metadata) = if let Ok(meta) =
                            serde_json::from_slice::<StoredDocumentMeta>(&bytes)
                        {
                            (meta.id, meta.metadata)
                        } else if let Ok(full) = serde_json::from_slice::<StoredDocument>(&bytes) {
                            (full.id, full.metadata)
                        } else {
                            tracing::warn!(doc_id = ?sd.doc_id, "Could not deserialize doc_key");
                            continue;
                        };
                        let meta_ref = doc_metadata.as_ref().unwrap_or(&serde_json::Value::Null);
                        if filter.matches(meta_ref) {
                            results.push(crate::SearchResult {
                                id,
                                score: sd.score,
                                metadata: doc_metadata,
                                matched_signals: vec![],
                            });
                            if results.len() >= k {
                                break;
                            }
                        }
                    }
                }
                Ok(results)
            }
        }
        .await;

        let _ = self.storage.unpin_checkpoint(seq).await;
        res
    }

    /// Performs semantic search using a raw text query (automatically embedded).
    #[tracing::instrument(level = "trace", skip(self, query_text))]
    pub async fn search_text(
        &self,
        query_text: &str,
        k: usize,
    ) -> Result<Vec<crate::SearchResult>> {
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
            embedder.embed(query_text).await?
        };
        self.search(&embedding, k).await
    }

    async fn get_matching_doc_ids_at(
        &self,
        filter: &MetadataFilter,
        seq: u64,
    ) -> Result<std::collections::HashSet<DocId>> {
        let prefix = if self.name == "default" {
            b"__docid:".to_vec()
        } else {
            let mut p = self.prefix.clone();
            p.push(1); // docid mapping type
            p
        };

        let entries = self.storage.scan_prefix_at(&prefix, seq).await?;
        let mut matched = std::collections::HashSet::new();

        for (_, v) in entries {
            let (id, doc_metadata) =
                if let Ok(meta) = serde_json::from_slice::<StoredDocumentMeta>(&v) {
                    (meta.id, meta.metadata)
                } else if let Ok(full) = serde_json::from_slice::<StoredDocument>(&v) {
                    (full.id, full.metadata)
                } else {
                    continue;
                };
            let metadata = doc_metadata.as_ref().unwrap_or(&serde_json::Value::Null);
            if filter.matches(metadata) {
                matched.insert(DocId::from_key(&id)?);
            }
        }

        Ok(matched)
    }

    /// Performs filtered semantic vector search in the collection.
    #[tracing::instrument(level = "trace", skip(self, query, filter))]
    pub async fn search_filtered(
        &self,
        query: &[f32],
        k: usize,
        filter: Option<&(dyn Fn(DocId) -> bool + Send + Sync)>,
    ) -> Result<Vec<crate::SearchResult>> {
        let seq = self.snapshot_seq().await?;
        self.search_filtered_at(query, k, filter, seq).await
    }

    pub async fn search_filtered_at(
        &self,
        query: &[f32],
        k: usize,
        filter: Option<&(dyn Fn(DocId) -> bool + Send + Sync)>,
        seq: u64,
    ) -> Result<Vec<crate::SearchResult>> {
        let scored_docs = self.index.search_filtered(query, k, filter).await?;
        self.hydrate_from_scored_at(scored_docs, seq).await
    }

    async fn hydrate_from_scored_at(
        &self,
        scored_docs: Vec<memfuse_core::ScoredDocument>,
        seq: u64,
    ) -> Result<Vec<crate::SearchResult>> {
        if scored_docs.is_empty() {
            return Ok(Vec::new());
        }

        let mut results = Vec::with_capacity(scored_docs.len());
        for sd in scored_docs {
            let doc_key = self.namespaced_key(&sd.doc_id.inner().to_le_bytes(), 1);
            if let Some(bytes) = self.storage.get_at_seq(&doc_key, seq).await? {
                let (id, metadata) =
                    if let Ok(meta) = serde_json::from_slice::<StoredDocumentMeta>(&bytes) {
                        (meta.id, meta.metadata)
                    } else if let Ok(full) = serde_json::from_slice::<StoredDocument>(&bytes) {
                        (full.id, full.metadata)
                    } else {
                        tracing::warn!(doc_id = ?sd.doc_id, "Could not deserialize doc_key");
                        continue;
                    };
                results.push(crate::SearchResult {
                    id,
                    score: sd.score,
                    metadata,
                    matched_signals: vec![],
                });
            }
        }
        Ok(results)
    }

    async fn hydrate_from_tuples_at(
        &self,
        scored_tuples: Vec<(DocId, f32)>,
        seq: u64,
    ) -> Result<Vec<crate::SearchResult>> {
        if scored_tuples.is_empty() {
            return Ok(Vec::new());
        }

        let mut results = Vec::with_capacity(scored_tuples.len());
        for (doc_id, score) in scored_tuples {
            let doc_key = self.namespaced_key(&doc_id.inner().to_le_bytes(), 1);
            if let Some(bytes) = self.storage.get_at_seq(&doc_key, seq).await? {
                let (id, metadata) =
                    if let Ok(meta) = serde_json::from_slice::<StoredDocumentMeta>(&bytes) {
                        (meta.id, meta.metadata)
                    } else if let Ok(full) = serde_json::from_slice::<StoredDocument>(&bytes) {
                        (full.id, full.metadata)
                    } else {
                        tracing::warn!(doc_id = ?doc_id, "Could not deserialize doc_key");
                        continue;
                    };
                results.push(crate::SearchResult {
                    id,
                    score,
                    metadata,
                    matched_signals: vec![],
                });
            }
        }
        Ok(results)
    }

    /// Performs hybrid search combining BM25, vector search, and graph traversal results via RRF.
    #[tracing::instrument(level = "trace", skip(self, text, vector))]
    pub async fn hybrid_search(
        &self,
        text: &str,
        vector: &[f32],
        k: usize,
        anchor_entities: Option<&[memfuse_core::EntityId]>,
    ) -> Result<Vec<crate::SearchResult>> {
        self.hybrid_search_with_weights(text, vector, k, anchor_entities, None)
            .await
    }

    /// Performs hybrid search with custom fusion weights for vector, text, and graph signals.
    #[tracing::instrument(level = "trace", skip(self, text, vector))]
    pub async fn hybrid_search_with_weights(
        &self,
        text: &str,
        vector: &[f32],
        k: usize,
        anchor_entities: Option<&[memfuse_core::EntityId]>,
        weights: Option<&memfuse_core::FusionWeights>,
    ) -> Result<Vec<crate::SearchResult>> {
        if k == 0 {
            return Ok(Vec::new());
        }
        let k = k.min(memfuse_core::MAX_SEARCH_K);

        let seq = self.snapshot_seq().await?;
        let is_vector_zero = vector.iter().all(|&v| v == 0.0);
        let is_text_empty = text.trim().is_empty();

        const MAX_TRAVERSAL_HOPS: usize = 3;

        // 1. Vector Signal
        let vector_results = if is_vector_zero {
            Vec::new()
        } else {
            self.search_filtered_at(vector, k, None, seq).await?
        };

        // 2. Text Signal
        let text_results = if is_text_empty {
            Vec::new()
        } else {
            let bm25_results = self.text_index.search_at(text, k, seq).await?;
            self.hydrate_from_tuples_at(
                bm25_results
                    .into_iter()
                    .map(|sd| (sd.doc_id, sd.score))
                    .collect(),
                seq,
            )
            .await?
        };

        // 3. Graph Signal
        let graph_results = if let Some(anchors) = anchor_entities {
            if anchors.is_empty() {
                Vec::new()
            } else {
                let tuples = self
                    .graph_index
                    .multi_traverse(anchors, MAX_TRAVERSAL_HOPS)
                    .await?;
                let doc_tuples = tuples
                    .into_iter()
                    .map(|(eid, score)| (memfuse_core::DocId::new(eid.inner()), score))
                    .collect();
                self.hydrate_from_tuples_at(doc_tuples, seq).await?
            }
        } else if !text_results.is_empty() {
            let implicit_anchors: Vec<memfuse_core::EntityId> = text_results
                .iter()
                .map(|r| memfuse_core::EntityId::from_key(r.id.as_str()))
                .collect();
            let tuples = self
                .graph_index
                .multi_traverse(&implicit_anchors, MAX_TRAVERSAL_HOPS)
                .await?;
            let doc_tuples = tuples
                .into_iter()
                .map(|(eid, score)| (memfuse_core::DocId::new(eid.inner()), score))
                .collect();
            self.hydrate_from_tuples_at(doc_tuples, seq).await?
        } else {
            Vec::new()
        };

        if vector_results.is_empty() && text_results.is_empty() && graph_results.is_empty() {
            return Ok(Vec::new());
        }

        let (vw, tw, gw) = crate::fusion::weights_to_signal_factors(weights);

        let mut signal_sets = Vec::new();
        if !vector_results.is_empty() {
            signal_sets.push(("vector".to_string(), vector_results, vw));
        }
        if !text_results.is_empty() {
            signal_sets.push(("text".to_string(), text_results, tw));
        }
        if !graph_results.is_empty() {
            signal_sets.push(("graph".to_string(), graph_results, gw));
        }

        Ok(crate::fusion::weighted_reciprocal_rank_fusion(
            signal_sets,
            k,
        ))
    }

    /// Returns the name of the collection.
    #[tracing::instrument(level = "trace", skip(self))]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the number of documents in the collection.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn len(&self) -> usize {
        self.index.len().await
    }

    /// Returns the vector dimension for this collection.
    #[tracing::instrument(level = "trace", skip(self))]
    pub fn dimension(&self) -> usize {
        self.dimension
    }

    /// Returns true if the collection is empty.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn is_empty(&self) -> bool {
        self.index.is_empty().await
    }

    /// Performs a range scan of documents in the collection.
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

    /// Returns statistics for the collection's vector index.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn stats(&self) -> Result<memfuse_core::VectorIndexStats> {
        self.index.stats().await
    }

    /// Rebuilds the HNSW index from storage.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn load_index(&self) -> Result<()> {
        // AI-TAG[CONVENTION-DRIFT][MAJOR] RESOLVED: load_index now scans user_keys (key_type=0)
        // because doc_keys (key_type=1) no longer contain embeddings (ID: AGT-DB-002).
        let scan_prefix = if self.name == "default" {
            b"".to_vec()
        } else {
            let mut p = self.prefix.clone();
            p.push(0); // key_type=0
            p
        };

        let entries = self.storage.scan_prefix(&scan_prefix).await?;
        let tx = TxId::new(self.next_tx.fetch_add(1, Ordering::SeqCst));
        for (k, v) in entries {
            if self.name == "default" && k.starts_with(b"__") {
                continue;
            }

            let stored: StoredDocument = match serde_json::from_slice(&v) {
                Ok(d) => d,
                Err(_) => continue,
            };
            let doc_id = DocId::from_key(&stored.id)?;
            self.index.insert(tx, doc_id, &stored.embedding).await?;
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
        let tx = TxId::new(self.next_tx.fetch_add(1, Ordering::SeqCst));

        for (k, v) in entries {
            // Try parsing as full document first (which indicates it needs migration)
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

    /// Removes all data belonging to this collection from storage.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn drop_collection(&self) -> Result<()> {
        let prefix = if self.name == "default" {
            return Err(memfuse_core::MemFuseError::invalid_input(
                "Cannot drop default collection",
            ));
        } else {
            self.prefix.clone()
        };

        let tx = TxId::new(self.next_tx.fetch_add(1, Ordering::SeqCst));

        // 1. Clean collection data (user keys, docs, rels, intents)
        self.storage.delete_prefix(tx, &prefix).await?;

        // 2. Clean text index namespace (FIND-DB-002)
        let txt_prefix = format!("__txt:{}:", self.name).into_bytes();
        self.storage.delete_prefix(tx, &txt_prefix).await?;

        self.storage.commit(tx).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_collection_embedder_async_embed() {
        use async_trait::async_trait;
        use memfuse_core::TextEmbeddingEngine;
        use std::sync::Arc;

        struct FakeEmbedder;

        #[async_trait]
        impl TextEmbeddingEngine for FakeEmbedder {
            async fn embed(&self, text: &str) -> memfuse_core::Result<Vec<f32>> {
                Ok(vec![text.len() as f32 / 100.0; 4])
            }
        }

        // Verify: compile-time proof that the method signature is async and
        // accepts Arc<dyn TextEmbeddingEngine>.
        let embedder: Arc<dyn TextEmbeddingEngine> = Arc::new(FakeEmbedder);
        let result = embedder.embed("hello").await.unwrap();
        assert_eq!(result.len(), 4);
    }

    #[tokio::test]
    async fn test_hybrid_search_k_clamping_boundaries() {
        use memfuse_graph::CsrGraph;
        use memfuse_index::HnswIndex;
        use memfuse_store::LsmStorage;
        use std::sync::atomic::AtomicU64;
        use std::sync::Arc;
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let lsm_config = memfuse_store::LsmConfig {
            path: dir.path().to_path_buf(),
            ..Default::default()
        };
        let storage = Arc::new(LsmStorage::new(lsm_config).await.unwrap());
        let hnsw_config = memfuse_index::HnswConfig {
            dimension: 4,
            ..Default::default()
        };
        let index = Arc::new(HnswIndex::new(hnsw_config));
        let graph = Arc::new(CsrGraph::new());
        let next_tx = Arc::new(AtomicU64::new(1));

        let col = super::Collection::new(
            "default".to_string(),
            storage,
            index,
            graph,
            next_tx,
            4,
            memfuse_text::Language::English,
        );

        // 1. k = 0 boundary check (must short-circuit to empty results without panic)
        let res_zero = col
            .hybrid_search("test", &[0.1, 0.2, 0.3, 0.4], 0, None)
            .await
            .unwrap();
        assert!(res_zero.is_empty(), "k=0 must return empty result list");

        // 2. k = usize::MAX boundary check (must clamp to MAX_SEARCH_K without panic/overflow)
        let res_max = col
            .hybrid_search("test", &[0.0, 0.0, 0.0, 0.0], usize::MAX, None)
            .await
            .unwrap();
        assert!(
            res_max.is_empty(),
            "k=usize::MAX on empty DB must return empty without overflow panic"
        );
    }

    #[tokio::test]
    async fn test_doc_id_collision_rejected() {
        use memfuse_core::{DocId, MemFuseError, StorageEngine, TxId};
        use memfuse_graph::CsrGraph;
        use memfuse_index::HnswIndex;
        use memfuse_store::LsmStorage;
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::sync::Arc;
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let lsm_config = memfuse_store::LsmConfig {
            path: dir.path().to_path_buf(),
            ..Default::default()
        };
        let storage = Arc::new(LsmStorage::new(lsm_config).await.unwrap());
        let index = Arc::new(HnswIndex::new(memfuse_index::HnswConfig {
            dimension: 4,
            ..Default::default()
        }));
        let graph = Arc::new(CsrGraph::new());
        let next_tx = Arc::new(AtomicU64::new(1));

        let col = super::Collection::new(
            "default".to_string(),
            storage,
            index,
            graph,
            next_tx.clone(),
            4,
            memfuse_text::Language::English,
        );

        // 1. Insert first document normally
        let id1 = "key_alpha";
        let emb1 = vec![1.0, 0.0, 0.0, 0.0];
        col.insert(id1, &emb1, None).await.unwrap();

        // Verify key_alpha exists
        let doc1 = col.get(id1).await.unwrap();
        assert!(doc1.is_some());

        // 2. Synthetically inject a mapping for a fixed DocId (e.g. DocId::new(42)) pointing to "key_existing"
        let synthetic_doc_id = DocId::new(42);
        let tx = TxId::new(next_tx.fetch_add(1, Ordering::SeqCst));
        let doc_key = col.namespaced_key(&synthetic_doc_id.inner().to_le_bytes(), 1);
        let existing_meta = super::StoredDocumentMeta {
            id: "key_existing".to_string(),
            metadata: None,
        };
        let meta_bytes = serde_json::to_vec(&existing_meta).unwrap();
        col.storage.put(tx, &doc_key, &meta_bytes).await.unwrap();
        col.storage.commit(tx).await.unwrap();

        // 3. Directly test check_doc_id_collision with a different string key (e.g., "key_new")
        let collision_res = col.check_doc_id_collision(synthetic_doc_id, "key_new").await;
        assert!(collision_res.is_err());
        match collision_res {
            Err(MemFuseError::Internal(msg)) => {
                assert!(
                    msg.contains("DocId-Kollision erkannt für Schlüssel 'key_new'"),
                    "Unexpected error message: {}",
                    msg
                );
            }
            res => panic!("Expected MemFuseError::Internal, got {:?}", res),
        }

        // 4. Same key string should NOT be treated as a collision
        let same_key_res = col.check_doc_id_collision(synthetic_doc_id, "key_existing").await;
        assert!(same_key_res.is_ok());
    }
}
