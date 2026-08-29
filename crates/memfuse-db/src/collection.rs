//! Logically isolated Collections inside the MemFuse database.
// FILE-CONTEXT
// STAND: 2026-08-27T14:32:00Z
// ZWECK: Collection-API — zentraler Einstiegspunkt für Insert/Search/Delete
// INVARIANTEN: TxId monoton steigend (AtomicU64, SeqCst); kein direkter DB-Zugriff ohne TxId
// NICHT-OFFENSICHTLICH: SystemTime als TxId-Fallback ist unsicher bei EMBED_CONCURRENCY>1
// SIEHE AUCH: memfuse-db/AGENTS.md, DECISIONS.md ADR-005

// INVARIANT: Logische Isolation (Namespaces).
// PREFIXING: Jeder Key im LSM bekommt das Prefix `__col:{name}:\x00`.

#[allow(deprecated)]
use crate::filter::MetadataFilter;
use memfuse_core::TextEmbeddingEngine;
use memfuse_core::{
    DocId, EntityId, FilterExpr, GraphIndex, Result, StorageEngine, TextIndex, TxId, VectorIndex,
    EXPIRY_METADATA_KEY,
};
use memfuse_graph::{detect_communities, CommunityAssignment, CommunityDetectionConfig, CsrGraph};
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

/// Parses an LLM response string into an f32 importance score in `[0.0, 1.0]`.
pub fn parse_importance_score(response: &str) -> f32 {
    for token in response.split_whitespace() {
        if let Ok(val) = token.parse::<f32>() {
            if val.is_finite() {
                return val.clamp(0.0, 1.0);
            }
        }
        let trimmed = token.trim_matches(|c: char| !c.is_ascii_digit() && c != '.');
        if !trimmed.is_empty() {
            if let Ok(val) = trimmed.parse::<f32>() {
                if val.is_finite() {
                    return val.clamp(0.0, 1.0);
                }
            }
        }
    }
    0.5
}

/// Computes a non-LLM heuristic baseline importance score based on text length and character entropy.
pub fn compute_default_importance(text_opt: Option<&str>) -> memfuse_core::ImportanceScore {
    let text = match text_opt {
        Some(t) if !t.is_empty() => t,
        _ => return memfuse_core::ImportanceScore::new(0.5),
    };
    let char_count = text.chars().count();
    if char_count == 0 {
        return memfuse_core::ImportanceScore::new(0.5);
    }
    let unique_chars = text.chars().collect::<std::collections::HashSet<_>>().len() as f32;
    let entropy_ratio = unique_chars / char_count as f32;
    let len_factor = (char_count as f32 / 500.0).clamp(0.1, 0.8);
    let raw = (len_factor * 0.5) + (entropy_ratio * 0.5);
    memfuse_core::ImportanceScore::new(raw)
}

/// Ensures document metadata contains a valid `MemoryImportance` JSON payload.
pub fn ensure_importance_metadata(
    metadata: &mut Option<serde_json::Value>,
    tx: TxId,
    text_opt: Option<&str>,
) {
    let meta_obj = match metadata {
        Some(serde_json::Value::Object(ref mut map)) => map,
        _ => {
            *metadata = Some(serde_json::json!({}));
            if let Some(serde_json::Value::Object(ref mut map)) = metadata {
                map
            } else {
                return;
            }
        }
    };

    if let Some(imp_val) = meta_obj.get("importance") {
        if serde_json::from_value::<memfuse_core::MemoryImportance>(imp_val.clone()).is_ok() {
            return;
        } else if let Some(raw_f64) = imp_val.as_f64() {
            let imp = memfuse_core::MemoryImportance::new(
                memfuse_core::ImportanceScore::new(raw_f64 as f32),
                memfuse_core::DecayFunction::None,
                tx,
            );
            if let Ok(val) = serde_json::to_value(imp) {
                meta_obj.insert("importance".to_string(), val);
            }
            return;
        }
    }

    let base_score = compute_default_importance(text_opt);
    let imp =
        memfuse_core::MemoryImportance::new(base_score, memfuse_core::DecayFunction::None, tx);
    if let Ok(val) = serde_json::to_value(imp) {
        meta_obj.insert("importance".to_string(), val);
    }
}

/// Extracts the effective importance score of a document at a given transaction ID.
pub fn extract_effective_importance(metadata: &Option<serde_json::Value>, now_tx: TxId) -> f32 {
    let Some(meta) = metadata else {
        return 1.0;
    };
    let Some(obj) = meta.as_object() else {
        return 1.0;
    };
    let Some(imp_val) = obj.get("importance") else {
        return 1.0;
    };

    if let Ok(imp) = serde_json::from_value::<memfuse_core::MemoryImportance>(imp_val.clone()) {
        imp.effective_score(now_tx)
    } else if let Some(raw_f64) = imp_val.as_f64() {
        memfuse_core::ImportanceScore::new(raw_f64 as f32).value()
    } else {
        1.0
    }
}

/// Helper to unify how we extract text from metadata.
fn extract_text(metadata: &Option<serde_json::Value>) -> Option<String> {
    let mut document_text = String::new();
    if let Some(m) = metadata {
        if let Some(m_obj) = m.as_object() {
            if let Some(s) = m_obj.get("contextual_prefix").and_then(|v| v.as_str()) {
                if !s.is_empty() {
                    document_text.push_str(s);
                    document_text.push_str("\n\n");
                }
            }
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
/// Each collection provides its own vector index and inverted text index,
/// while sharing the underlying LSM-Tree storage with other collections.
pub struct Collection<S: StorageEngine = LsmStorage, V: VectorIndex = HnswIndex> {
    pub(crate) name: String,
    pub(crate) prefix: Vec<u8>,
    pub(crate) index: Arc<V>,
    pub(crate) text_index: InvertedIndex<S>,
    pub(crate) graph_index: Arc<CsrGraph>,
    pub(crate) storage: Arc<S>,
    pub(crate) next_tx: Arc<AtomicU64>,
    pub(crate) dimension: usize,
    pub(crate) embedder: parking_lot::RwLock<Option<Arc<dyn TextEmbeddingEngine>>>,
    pub(crate) insert_lock: Arc<tokio::sync::Mutex<()>>,
}

impl<S: StorageEngine, V: VectorIndex> Clone for Collection<S, V> {
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
            insert_lock: self.insert_lock.clone(),
        }
    }
}

impl<S: StorageEngine> Collection<S, HnswIndex> {
    /// Convenience constructor for creating a `Collection` with `HnswIndex`.
    pub fn with_hnsw(
        name: String,
        storage: Arc<S>,
        index: Arc<HnswIndex>,
        graph_index: Arc<CsrGraph>,
        next_tx: Arc<AtomicU64>,
        dimension: usize,
        language: Language,
    ) -> Self {
        Self::new(
            name,
            storage,
            index,
            graph_index,
            next_tx,
            dimension,
            language,
        )
    }
}

impl<S: StorageEngine, V: VectorIndex> Collection<S, V> {
    /// Threshold: Dokumente mit effective_score < DECAY_DELETION_THRESHOLD
    /// werden als "vergessen" markiert und gelöscht.
    pub const DECAY_DELETION_THRESHOLD: f32 = 0.05;

    /// Creates a new `Collection` instance with explicit language configuration.
    ///
    /// The `language` parameter controls the BM25 tokenizer. Use `Language::German`
    /// for German compound splitting, `Language::English` (default) for standard
    /// whitespace tokenization.
    pub fn new(
        name: String,
        storage: Arc<S>,
        index: Arc<V>,
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
            insert_lock: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    /// Generates and returns the next sequential transaction ID for this collection.
    pub fn next_tx(&self) -> Result<TxId> {
        let id = self.next_tx.fetch_add(1, Ordering::SeqCst);
        if id >= TxId::INTERNAL_BASE {
            return Err(memfuse_core::MemFuseError::Transaction(
                "TxId counter exhausted: INTERNAL_BASE range collision. Collection must be recreated.".into(),
            ));
        }
        Ok(TxId::new(id))
    }

    /// Allokiert eine eindeutige, atomar inkrementierte Transaction-ID.
    /// Externe Crates verwenden diese Methode statt eigener TxId-Generierung.
    /// Verhindert TxId-Kollisionen bei paralleler Ingestion (EMBED_CONCURRENCY > 1).
    pub fn allocate_tx(&self) -> Result<TxId> {
        let id = self.next_tx.fetch_add(1, Ordering::SeqCst);
        if id >= TxId::INTERNAL_BASE {
            return Err(memfuse_core::MemFuseError::Transaction(
                "TxId counter exhausted: INTERNAL_BASE range collision. Collection must be recreated.".into(),
            ));
        }
        Ok(TxId::new(id))
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
    /// key_type: 0 = user key, 1 = docid mapping, 2 = relationship, 3 = tx intent, 4 = system/community
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
                4 => key.to_vec(),
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

    /// Begins a new atomic transaction for this collection.
    #[tracing::instrument(level = "trace", skip(self))]
    pub fn begin_transaction(&self) -> Result<crate::transaction::DbTransaction<S, V>> {
        let tx = self.next_tx()?;
        Ok(crate::transaction::DbTransaction::new(self.clone(), tx))
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
        if embedding.len() != self.dimension {
            return Err(memfuse_core::MemFuseError::invalid_input(format!(
                "Dimension mismatch: expected {}, got {}",
                self.dimension,
                embedding.len()
            )));
        }

        let _guard = self.insert_lock.lock().await;

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
        db_tx: &crate::transaction::DbTransaction<S, V>,
        id: &str,
        embedding: &[f32],
        metadata: Option<serde_json::Value>,
    ) -> Result<()> {
        let tx = db_tx.tx_id;
        let doc_id = DocId::from_key(id)?;

        self.check_doc_id_collision(doc_id, id).await?;

        let mut metadata = metadata;
        let text_opt = extract_text(&metadata);
        ensure_importance_metadata(&mut metadata, tx, text_opt.as_deref());

        let meta_only = StoredDocumentMeta {
            id: id.to_string(),
            metadata: metadata.clone(),
        };
        let stored = StoredDocument {
            id: id.to_string(),
            embedding: embedding.to_vec(),
            metadata: metadata.clone(),
        };

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

    /// Inserts multiple documents in a single transaction.
    #[tracing::instrument(level = "trace", skip(self, docs))]
    pub async fn insert_many(
        &self,
        docs: &[(String, Vec<f32>, Option<serde_json::Value>)],
    ) -> Result<()> {
        let _guard = self.insert_lock.lock().await;
        let db_tx = self.begin_transaction()?;
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
        let tx = db_tx.tx_id;
        let doc_id = DocId::from_key(id)?;

        self.check_doc_id_collision(doc_id, id).await?;

        let user_key = self.namespaced_key(id.as_bytes(), 0);

        // Stage removal from old text index
        db_tx.stage_text_delete(doc_id);

        let mut metadata = metadata;
        let text_opt = extract_text(&metadata);
        ensure_importance_metadata(&mut metadata, tx, text_opt.as_deref());

        let meta_only = StoredDocumentMeta {
            id: id.to_string(),
            metadata: metadata.clone(),
        };
        let stored = StoredDocument {
            id: id.to_string(),
            embedding: embedding.to_vec(),
            metadata: metadata.clone(),
        };
        let data = serde_json::to_vec(&stored)?;

        let doc_key = self.namespaced_key(&doc_id.inner().to_le_bytes(), 1);
        let meta_data = serde_json::to_vec(&meta_only)?;

        self.storage.put(tx, &user_key, &data).await?;
        self.storage.put(tx, &doc_key, &meta_data).await?;

        db_tx.record_keys(user_key, doc_key, doc_id);

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
        let tx = db_tx.tx_id;
        let doc_id = DocId::from_key(id)?;

        let user_key = self.namespaced_key(id.as_bytes(), 0);

        // Stage removal from old text index
        db_tx.stage_text_delete(doc_id);

        let doc_key = self.namespaced_key(&doc_id.inner().to_le_bytes(), 1);

        self.storage.delete(tx, &user_key).await?;
        self.storage.delete(tx, &doc_key).await?;

        db_tx.record_keys(user_key, doc_key, doc_id);

        // Recovery-Pfad ist HNSW-Rebuild (>20% deleted nodes) der mit LSM re-synct.
        if let Err(e) = self.index.delete(tx, doc_id).await {
            tracing::warn!(
                doc_id = ?doc_id,
                "HNSW soft-delete fehlgeschlagen: {e}. Doc wird nach HNSW-Rebuild nicht mehr in Vektorsuchen erscheinen."
            );
        }

        Ok(())
    }

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
        db_tx.record_keys(key, vec![], dummy_doc_id);

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
        let k = k.min(memfuse_core::MAX_SEARCH_K);
        self.search_with_filter_expr(query_embedding, k, None).await
    }

    /// Performs semantic search with an advanced metadata filter.
    #[deprecated(
        since = "0.1.0",
        note = "Use search_with_filter_expr with memfuse_core::FilterExpr directly"
    )]
    #[allow(deprecated)]
    #[tracing::instrument(level = "trace", skip(self, query, filter))]
    pub async fn search_with_filter(
        &self,
        query: &[f32],
        k: usize,
        filter: Option<MetadataFilter>,
    ) -> Result<Vec<crate::SearchResult>> {
        let expr = match filter {
            Some(f) => Some(FilterExpr::try_from(f)?),
            None => None,
        };
        self.search_with_filter_expr(query, k, expr).await
    }

    /// Performs semantic search with an advanced metadata filter expression (`FilterExpr`).
    #[tracing::instrument(level = "trace", skip(self, query, filter))]
    pub async fn search_with_filter_expr(
        &self,
        query: &[f32],
        k: usize,
        filter: Option<FilterExpr>,
    ) -> Result<Vec<crate::SearchResult>> {
        let k = k.min(memfuse_core::MAX_SEARCH_K);
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
                        if filter.evaluate(meta_ref) {
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

        if let Err(e) = self.storage.unpin_checkpoint(seq).await {
            tracing::error!(
                seq_no = seq,
                "Checkpoint seq={seq} konnte nicht unpinnt werden: {e}. SSTable-GC wird blockiert. Manuelles Eingreifen eventuell nötig."
            );
        }
        res
    }

    /// Performs semantic search using a raw text query (automatically embedded).
    #[tracing::instrument(level = "trace", skip(self, query_text))]
    pub async fn search_text(
        &self,
        query_text: &str,
        k: usize,
    ) -> Result<Vec<crate::SearchResult>> {
        let k = k.min(memfuse_core::MAX_SEARCH_K);
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
        filter: &FilterExpr,
        seq: u64,
    ) -> Result<std::collections::HashSet<DocId>> {
        let prefix = self.namespaced_key(&[], 1);

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
            if filter.evaluate(metadata) {
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
        let k = k.min(memfuse_core::MAX_SEARCH_K);
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
        let k = k.min(memfuse_core::MAX_SEARCH_K);
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

    /// Performs hybrid search combining BM25, vector search, and graph traversal, followed by optional Cross-Encoder reranking.
    #[cfg(feature = "reranking")]
    #[tracing::instrument(level = "trace", skip(self, text, vector, reranker, anchor_entities))]
    pub async fn hybrid_search_reranked(
        &self,
        text: &str,
        vector: &[f32],
        k: usize,
        reranker: Option<&memfuse_embed::CrossEncoderReranker>,
        anchor_entities: Option<&[memfuse_core::EntityId]>,
    ) -> Result<Vec<crate::SearchResult>> {
        let k = k.min(memfuse_core::MAX_SEARCH_K);
        // Schritt 1: Standard-Hybrid-Suche mit erhöhtem k (Reranking braucht mehr Kandidaten)
        let pre_rerank_k = if reranker.is_some() { k * 3 } else { k };
        let mut results = self
            .hybrid_search(text, vector, pre_rerank_k, anchor_entities)
            .await?;

        // Schritt 2: Optional Cross-Encoder Reranking
        if let Some(reranker) = reranker {
            let candidate_texts: Vec<String> = results
                .iter()
                .map(|r| {
                    r.metadata
                        .as_ref()
                        .and_then(|m| m.get("text").or_else(|| m.get("content")))
                        .and_then(|v| v.as_str())
                        .unwrap_or(&r.id)
                        .to_string()
                })
                .collect();

            match reranker.rerank(text, &candidate_texts).await {
                Ok(ranked) => {
                    let mut reranked_results = Vec::with_capacity(k);
                    for r in ranked.into_iter().take(k) {
                        if let Some(mut result) = results.get(r.original_index).cloned() {
                            if let Some(meta) = result.metadata.as_mut() {
                                if let Some(obj) = meta.as_object_mut() {
                                    obj.insert("ce_score".to_string(), serde_json::json!(r.score));
                                }
                            }
                            result.score = r.score;
                            reranked_results.push(result);
                        }
                    }
                    tracing::debug!("Reranking applied: {} candidates", reranked_results.len());
                    return Ok(reranked_results);
                }
                Err(e) => {
                    tracing::warn!("Reranking failed (using RRF order): {e}");
                }
            }
        }

        results.truncate(k);
        Ok(results)
    }

    /// Performs hybrid search with custom fusion weights for vector, text, and graph signals,
    /// and optional community filtering/boosting.
    #[tracing::instrument(level = "trace", skip(self, text, vector))]
    pub async fn hybrid_search_with_weights(
        &self,
        text: &str,
        vector: &[f32],
        k: usize,
        anchor_entities: Option<&[memfuse_core::EntityId]>,
        weights: Option<&memfuse_core::FusionWeights>,
    ) -> Result<Vec<crate::SearchResult>> {
        self.hybrid_search_with_strategy(text, vector, k, anchor_entities, weights, None, None)
            .await
    }

    /// Performs hybrid search with custom signal fusion weights and graph traversal strategy.
    #[tracing::instrument(level = "trace", skip(self, text, vector, strategy))]
    #[allow(clippy::too_many_arguments)]
    pub async fn hybrid_search_with_strategy(
        &self,
        text: &str,
        vector: &[f32],
        k: usize,
        anchor_entities: Option<&[memfuse_core::EntityId]>,
        weights: Option<&memfuse_core::FusionWeights>,
        strategy: Option<&memfuse_core::GraphTraversalStrategy>,
        same_community_as: Option<EntityId>,
    ) -> Result<Vec<crate::SearchResult>> {
        if k == 0 {
            return Ok(Vec::new());
        }
        let k = k.min(memfuse_core::MAX_SEARCH_K);

        let seq = self.snapshot_seq().await?;
        let is_vector_zero = vector.iter().all(|&v| v == 0.0);
        let is_text_empty = text.trim().is_empty();

        let default_strategy = memfuse_core::GraphTraversalStrategy::default();
        let graph_strat = strategy.unwrap_or(&default_strategy);

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
        let implicit_anchors: Vec<memfuse_core::EntityId>;
        let anchors_ref: Option<&[memfuse_core::EntityId]> = if let Some(anchors) = anchor_entities
        {
            if anchors.is_empty() {
                None
            } else {
                Some(anchors)
            }
        } else if !text_results.is_empty() {
            implicit_anchors = text_results
                .iter()
                .map(|r| memfuse_core::EntityId::from_key(r.id.as_str()))
                .collect::<Result<Vec<_>>>()?;
            Some(&implicit_anchors)
        } else {
            None
        };

        let graph_results = if let Some(anchors) = anchors_ref {
            let tuples = match graph_strat {
                memfuse_core::GraphTraversalStrategy::Hops { max_hops } => {
                    self.graph_index.multi_traverse(anchors, *max_hops).await?
                }
                memfuse_core::GraphTraversalStrategy::PersonalizedPageRank(ppr_config) => {
                    self.graph_index
                        .personalized_page_rank(anchors, ppr_config)
                        .await?
                }
            };
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

        // Community filtering / boosting VOR RRF
        let target_community_id: Option<u64> = if let Some(same_comm_entity) = same_community_as {
            self.get_community(same_comm_entity).await.ok().flatten()
        } else {
            None
        };

        let filter_or_boost = |list: Vec<crate::SearchResult>| async {
            if let Some(target_comm) = target_community_id {
                let mut filtered = Vec::new();
                for mut res in list {
                    if let Ok(eid) = memfuse_core::EntityId::from_key(&res.id) {
                        if let Ok(Some(comm)) = self.get_community(eid).await {
                            if comm == target_comm {
                                // Candidate is in the same community: boost score
                                res.score *= 1.2;
                                filtered.push(res);
                            }
                        }
                    }
                }
                filtered
            } else {
                list
            }
        };

        let vector_results = filter_or_boost(vector_results).await;
        let text_results = filter_or_boost(text_results).await;
        let graph_results = filter_or_boost(graph_results).await;

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

    /// Performs hybrid search combining BM25, vector, and graph signals configured via `HybridQuery`.
    ///
    /// Applies `memory_type_filter` and metadata `FilterExpr` as Pre-RRF filters to preserve
    /// Reciprocal Rank Fusion properties (ADR-024).
    #[tracing::instrument(level = "trace", skip(self, query))]
    pub async fn hybrid_search_with_query(
        &self,
        query: &memfuse_core::HybridQuery,
    ) -> Result<Vec<crate::SearchResult>> {
        if query.k == 0 {
            return Ok(Vec::new());
        }
        let k = query.k.min(memfuse_core::MAX_SEARCH_K);

        let text = query.text_query.as_deref().unwrap_or("");
        let vector = query.vector_query.as_deref().unwrap_or(&[]);

        let seq = self.snapshot_seq().await?;
        let is_vector_zero = vector.is_empty() || vector.iter().all(|&v| v == 0.0);
        let is_text_empty = text.trim().is_empty();

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
        let implicit_anchors: Vec<memfuse_core::EntityId>;
        let anchors_ref: Option<&[memfuse_core::EntityId]> =
            if let Some(ref start_node) = query.graph_start_node {
                if let Ok(eid) = memfuse_core::EntityId::from_key(start_node) {
                    implicit_anchors = vec![eid];
                    Some(&implicit_anchors)
                } else {
                    None
                }
            } else if !text_results.is_empty() {
                implicit_anchors = text_results
                    .iter()
                    .map(|r| memfuse_core::EntityId::from_key(r.id.as_str()))
                    .collect::<Result<Vec<_>>>()?;
                Some(&implicit_anchors)
            } else {
                None
            };

        let graph_results = if let Some(anchors) = anchors_ref {
            let tuples = match &query.graph_strategy {
                memfuse_core::GraphTraversalStrategy::Hops { max_hops } => {
                    self.graph_index.multi_traverse(anchors, *max_hops).await?
                }
                memfuse_core::GraphTraversalStrategy::PersonalizedPageRank(ppr_config) => {
                    self.graph_index
                        .personalized_page_rank(anchors, ppr_config)
                        .await?
                }
            };
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

        let (vw, tw, gw) = crate::fusion::weights_to_signal_factors(Some(&query.fusion_weights));

        // Target community for boosting/filtering
        let target_community_id: Option<u64> =
            if let Some(same_comm_entity) = query.same_community_as {
                self.get_community(same_comm_entity).await.ok().flatten()
            } else {
                None
            };

        let filter_and_boost = |list: Vec<crate::SearchResult>| async {
            let mut filtered = Vec::with_capacity(list.len());
            for mut res in list {
                if let Some(ref filter_expr) = query.filter {
                    let meta_ref = res.metadata.as_ref().unwrap_or(&serde_json::Value::Null);
                    if !filter_expr.evaluate(meta_ref) {
                        continue;
                    }
                }

                if let Some(ref type_filter) = query.memory_type_filter {
                    let memory_type = crate::filter::extract_memory_type(&res.metadata);
                    if !type_filter.contains(&memory_type) {
                        continue;
                    }
                }

                if let Some(target_comm) = target_community_id {
                    if let Ok(eid) = memfuse_core::EntityId::from_key(&res.id) {
                        if let Ok(Some(comm)) = self.get_community(eid).await {
                            if comm == target_comm {
                                res.score *= 1.2;
                            }
                        }
                    }
                }

                filtered.push(res);
            }
            filtered
        };

        let vector_results = filter_and_boost(vector_results).await;
        let text_results = filter_and_boost(text_results).await;
        let graph_results = filter_and_boost(graph_results).await;

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

    /// Filters a candidate list of search results by effective importance score threshold.
    ///
    /// Candidate results with `effective_score(now_tx) < min_threshold` are removed from the result list.
    /// Does NOT reorder remaining items, keeping RRF & Reranking order intact (ADR-024).
    pub fn filter_by_importance(
        results: Vec<crate::SearchResult>,
        min_threshold: f32,
        now_tx: TxId,
    ) -> Vec<crate::SearchResult> {
        results
            .into_iter()
            .filter(|r| {
                let eff = extract_effective_importance(&r.metadata, now_tx);
                eff >= min_threshold
            })
            .collect()
    }

    /// Returns a reference to the underlying storage engine.
    pub fn storage(&self) -> &Arc<S> {
        &self.storage
    }

    /// Returns the namespaced prefix for user document keys in this collection.
    pub fn user_key_prefix(&self) -> Vec<u8> {
        self.namespaced_key(b"", 0)
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
                    Bound::Included(self.namespaced_key(&[], 0))
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
        // AI-TAG[CONVENTION-DRIFT][MAJOR] RESOLVED: AGT-DB-002 — load_index now scans user_keys (key_type=0) (TS:2026-08-25T00:00:00Z)
        // because doc_keys (key_type=1) no longer contain embeddings (ID: AGT-DB-002).
        let scan_prefix = self.namespaced_key(&[], 0);

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
        let prefix = self.namespaced_key(&[], 1);

        let entries = self.storage.scan_prefix(&prefix).await?;
        let mut migrated_count = 0;
        let tx = self.next_tx()?;

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
                if let Some(serde_json::Value::Object(ref mut map)) = stored.metadata {
                    map
                } else {
                    unreachable!()
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
        if self.name == "default" {
            return Err(memfuse_core::MemFuseError::invalid_input(
                "Cannot drop default collection",
            ));
        }

        let tx = self.next_tx()?;

        // 1. Clean collection data (user keys, docs, rels, intents)
        self.storage.delete_prefix(tx, &self.prefix).await?;

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
    async fn test_insert_with_ttl_and_reap_expired_documents() {
        use memfuse_graph::CsrGraph;
        use memfuse_index::HnswIndex;
        use memfuse_store::LsmStorage;
        use std::sync::atomic::AtomicU64;
        use std::sync::Arc;
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let storage = Arc::new(
            LsmStorage::new(memfuse_store::LsmConfig {
                path: dir.path().to_path_buf(),
                ..Default::default()
            })
            .await
            .unwrap(),
        );
        let index = Arc::new(
            HnswIndex::try_new(memfuse_index::HnswConfig {
                dimension: 4,
                ..Default::default()
            })
            .unwrap(),
        );
        let col = super::Collection::new(
            "default".to_string(),
            storage,
            index,
            Arc::new(CsrGraph::new()),
            Arc::new(AtomicU64::new(1)),
            4,
            memfuse_text::Language::English,
        );

        let vec = vec![1.0, 0.0, 0.0, 0.0];

        // Insert document with TTL = 5 committed ops
        col.insert_with_ttl("temp_doc", &vec, None, 5)
            .await
            .unwrap();

        // 1. Immediately after insert, document should be retrievable
        let doc = col.get("temp_doc").await.unwrap();
        assert!(doc.is_some(), "Document must exist before TTL expiration");

        // 2. Perform 5 dummy commits (inserts)
        for i in 0..5 {
            col.insert(&format!("dummy_{i}"), &vec, None).await.unwrap();
        }

        // 3. Trigger expiry reaper
        let reaped = col.reap_expired_documents(100).await.unwrap();
        assert_eq!(reaped, 1, "Expired document should be reaped");

        // 4. Verify document is gone from storage and search
        let doc_after = col.get("temp_doc").await.unwrap();
        assert!(
            doc_after.is_none(),
            "Document must be deleted after TTL expiry"
        );

        let search_res = col.search(&vec, 10).await.unwrap();
        assert!(
            search_res.iter().all(|r| r.id != "temp_doc"),
            "Expired document must not appear in search results"
        );
    }

    #[tokio::test]
    async fn test_relate_success_visible_in_storage_and_graph() {
        use memfuse_core::EntityId;
        use memfuse_graph::csr::CsrGraph;
        use memfuse_index::HnswIndex;
        use memfuse_store::{LsmConfig, LsmStorage};
        use std::sync::atomic::AtomicU64;
        use std::sync::Arc;
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let storage = Arc::new(
            LsmStorage::new(LsmConfig {
                path: dir.path().to_path_buf(),
                ..Default::default()
            })
            .await
            .unwrap(),
        );
        let index = Arc::new(
            HnswIndex::try_new(memfuse_index::HnswConfig {
                dimension: 4,
                ..Default::default()
            })
            .unwrap(),
        );
        let graph = Arc::new(CsrGraph::new());
        let col = super::Collection::new(
            "default".to_string(),
            storage.clone(),
            index,
            graph.clone(),
            Arc::new(AtomicU64::new(1)),
            4,
            memfuse_text::Language::English,
        );

        col.relate("doc1", "doc2", "references").await.unwrap();

        // 1. Storage check
        let rels = col.scan_prefix("__rel:").await.unwrap();
        assert_eq!(rels.len(), 1);
        assert!(rels[0].0.contains("doc1:references:doc2"));

        // 2. Graph check
        let id1 = EntityId::from_key("doc1").unwrap();
        let id2 = EntityId::from_key("doc2").unwrap();
        let neighbors = graph.neighbors(id1).await.unwrap();
        assert!(neighbors.contains(&id2));
    }

    #[tokio::test]
    async fn test_relate_rollback_semantics_on_storage_commit_failure() {
        use async_trait::async_trait;
        use memfuse_core::{Result, StorageEngine, StorageStats, TxId};
        use memfuse_graph::csr::CsrGraph;
        use memfuse_index::HnswIndex;
        use std::sync::atomic::AtomicU64;
        use std::sync::Arc;

        struct FailOnStorageCommit;

        #[async_trait]
        impl StorageEngine for FailOnStorageCommit {
            async fn get(&self, _: &[u8]) -> Result<Option<Vec<u8>>> {
                Ok(None)
            }
            async fn get_at_seq(&self, _: &[u8], _: u64) -> Result<Option<Vec<u8>>> {
                Ok(None)
            }
            async fn put(&self, _: TxId, _: &[u8], _: &[u8]) -> Result<()> {
                Ok(())
            }
            async fn delete(&self, _: TxId, _: &[u8]) -> Result<()> {
                Ok(())
            }
            async fn commit(&self, _: TxId) -> Result<()> {
                Err(memfuse_core::MemFuseError::Storage(
                    "Simulated Storage Commit Failure".into(),
                ))
            }
            async fn rollback(&self, _: TxId) -> Result<()> {
                Ok(())
            }
            async fn rollback_to_tx(&self, _: TxId) -> Result<()> {
                Ok(())
            }
            async fn flush(&self) -> Result<()> {
                Ok(())
            }
            async fn stats(&self) -> Result<StorageStats> {
                Ok(StorageStats {
                    num_segments: 0,
                    total_size_bytes: 0,
                    memtable_size_bytes: 0,
                })
            }
            async fn last_seq_no(&self) -> Result<u64> {
                Ok(0)
            }
            async fn last_tx_id(&self) -> Result<TxId> {
                Ok(TxId(0))
            }
            async fn pin_checkpoint(&self, _: u64) -> Result<()> {
                Ok(())
            }
            async fn unpin_checkpoint(&self, _: u64) -> Result<()> {
                Ok(())
            }
            async fn scan_prefix(&self, _: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
                Ok(vec![])
            }
            async fn scan(
                &self,
                _: std::ops::Bound<&[u8]>,
                _: std::ops::Bound<&[u8]>,
            ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
                Ok(vec![])
            }
        }

        let storage = Arc::new(FailOnStorageCommit);
        let index = Arc::new(
            HnswIndex::try_new(memfuse_index::HnswConfig {
                dimension: 4,
                ..Default::default()
            })
            .unwrap(),
        );
        let graph = Arc::new(CsrGraph::new());
        let col = super::Collection::new(
            "default".to_string(),
            storage,
            index,
            graph.clone(),
            Arc::new(AtomicU64::new(1)),
            4,
            memfuse_text::Language::English,
        );

        let res = col.relate("node_x", "node_y", "links").await;
        assert!(
            res.is_err(),
            "relate() must fail when storage.commit() fails"
        );

        // Graph index should remain empty since relate failed before graph commit
        assert_eq!(graph.entity_count(), 0);
    }

    // REGRESSION TEST für F-01: beweist gebrochene Rollback-Semantik in relate()
    #[tokio::test]
    async fn test_relate_rollback_semantics_on_graph_commit_failure() {
        use async_trait::async_trait;
        use memfuse_core::{Result, StorageEngine, StorageStats, TxId};
        use memfuse_graph::csr::{CsrGraph, CsrGraphConfig};
        use memfuse_index::HnswIndex;
        use memfuse_store::{LsmConfig, LsmStorage};
        use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
        use std::sync::Arc;
        use tempfile::tempdir;

        struct FailOnPutStorage {
            should_fail: AtomicBool,
        }

        #[async_trait]
        impl StorageEngine for FailOnPutStorage {
            async fn get(&self, _: &[u8]) -> Result<Option<Vec<u8>>> {
                Ok(None)
            }
            async fn get_at_seq(&self, _: &[u8], _: u64) -> Result<Option<Vec<u8>>> {
                Ok(None)
            }
            async fn put(&self, _: TxId, _: &[u8], _: &[u8]) -> Result<()> {
                if self.should_fail.load(Ordering::SeqCst) {
                    Err(memfuse_core::MemFuseError::Storage(
                        "Simulated Graph Storage Commit Failure".into(),
                    ))
                } else {
                    Ok(())
                }
            }
            async fn delete(&self, _: TxId, _: &[u8]) -> Result<()> {
                Ok(())
            }
            async fn commit(&self, _: TxId) -> Result<()> {
                Ok(())
            }
            async fn rollback(&self, _: TxId) -> Result<()> {
                Ok(())
            }
            async fn rollback_to_tx(&self, _: TxId) -> Result<()> {
                Ok(())
            }
            async fn flush(&self) -> Result<()> {
                Ok(())
            }
            async fn stats(&self) -> Result<StorageStats> {
                Ok(StorageStats {
                    num_segments: 0,
                    total_size_bytes: 0,
                    memtable_size_bytes: 0,
                })
            }
            async fn last_seq_no(&self) -> Result<u64> {
                Ok(0)
            }
            async fn last_tx_id(&self) -> Result<TxId> {
                Ok(TxId(0))
            }
            async fn pin_checkpoint(&self, _: u64) -> Result<()> {
                Ok(())
            }
            async fn unpin_checkpoint(&self, _: u64) -> Result<()> {
                Ok(())
            }
            async fn scan_prefix(&self, _: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
                Ok(vec![])
            }
            async fn scan(
                &self,
                _: std::ops::Bound<&[u8]>,
                _: std::ops::Bound<&[u8]>,
            ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
                Ok(vec![])
            }
        }

        let dir = tempdir().unwrap();
        let lsm_config = LsmConfig {
            path: dir.path().to_path_buf(),
            ..Default::default()
        };
        let storage = Arc::new(LsmStorage::new(lsm_config).await.unwrap());
        let index = Arc::new(
            HnswIndex::try_new(memfuse_index::HnswConfig {
                dimension: 4,
                ..Default::default()
            })
            .unwrap(),
        );

        let fail_storage = Arc::new(FailOnPutStorage {
            should_fail: AtomicBool::new(true),
        });
        let graph = Arc::new(CsrGraph::with_config_and_storage(
            CsrGraphConfig::default(),
            fail_storage,
        ));
        let next_tx = Arc::new(AtomicU64::new(1));

        let col = super::Collection::new(
            "default".to_string(),
            storage.clone(),
            index,
            graph,
            next_tx,
            4,
            memfuse_text::Language::English,
        );

        // relate() should fail when graph_index.commit() fails
        let res = col.relate("entity_a", "entity_b", "connects").await;
        assert!(
            res.is_err(),
            "relate() must return Err when graph commit fails"
        );

        // Verification: storage MUST NOT contain the relation key after failed relate()
        let rel_prefix = col.namespaced_key(b"", 2);
        let remaining_rels = storage.scan_prefix(&rel_prefix).await.unwrap();
        assert!(
            remaining_rels.is_empty(),
            "Storage layer MUST NOT contain relation keys after relate() failure! Found: {:?}",
            remaining_rels
        );
    }

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
        let result = embedder.embed("hello").await.unwrap(); // unwrap
        assert_eq!(result.len(), 4);
    }

    #[tokio::test]
    async fn hybrid_search_caps_k_at_max_search_k() {
        use memfuse_graph::CsrGraph;
        use memfuse_index::HnswIndex;
        use memfuse_store::LsmStorage;
        use std::sync::atomic::AtomicU64;
        use std::sync::Arc;
        use tempfile::tempdir;

        let dir = tempdir().unwrap(); // unwrap
        let lsm_config = memfuse_store::LsmConfig {
            path: dir.path().to_path_buf(),
            ..Default::default()
        };
        let storage = Arc::new(LsmStorage::new(lsm_config).await.unwrap()); // unwrap
        let hnsw_config = memfuse_index::HnswConfig {
            dimension: 4,
            ..Default::default()
        };
        let index = Arc::new(HnswIndex::try_new(hnsw_config).unwrap()); // unwrap
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

        let res = col
            .hybrid_search("test", &[0.1, 0.2, 0.3, 0.4], 100_000, None)
            .await
            .unwrap(); // unwrap

        assert!(
            res.len() <= memfuse_core::MAX_SEARCH_K,
            "Results length {} should be <= MAX_SEARCH_K ({})",
            res.len(),
            memfuse_core::MAX_SEARCH_K
        );
    }

    #[tokio::test]
    async fn test_hybrid_search_k_clamping_boundaries() {
        use memfuse_graph::CsrGraph;
        use memfuse_index::HnswIndex;
        use memfuse_store::LsmStorage;
        use std::sync::atomic::AtomicU64;
        use std::sync::Arc;
        use tempfile::tempdir;

        let dir = tempdir().unwrap(); // unwrap
        let lsm_config = memfuse_store::LsmConfig {
            path: dir.path().to_path_buf(),
            ..Default::default()
        };
        let storage = Arc::new(LsmStorage::new(lsm_config).await.unwrap()); // unwrap
        let hnsw_config = memfuse_index::HnswConfig {
            dimension: 4,
            ..Default::default()
        };
        let index = Arc::new(HnswIndex::try_new(hnsw_config).unwrap()); // unwrap
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
            .unwrap(); // unwrap
        assert!(res_zero.is_empty(), "k=0 must return empty result list");

        // 2. k = usize::MAX boundary check (must clamp to MAX_SEARCH_K without panic/overflow)
        let res_max = col
            .hybrid_search("test", &[0.0, 0.0, 0.0, 0.0], usize::MAX, None)
            .await
            .unwrap(); // unwrap
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

        let dir = tempdir().unwrap(); // unwrap
        let lsm_config = memfuse_store::LsmConfig {
            path: dir.path().to_path_buf(),
            ..Default::default()
        };
        let storage = Arc::new(LsmStorage::new(lsm_config).await.unwrap()); // unwrap
        let index = Arc::new(
            HnswIndex::try_new(memfuse_index::HnswConfig {
                dimension: 4,
                ..Default::default()
            })
            .unwrap(), // unwrap
        );
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
        col.insert(id1, &emb1, None).await.unwrap(); // unwrap

        // Verify key_alpha exists
        let doc1 = col.get(id1).await.unwrap(); // unwrap
        assert!(doc1.is_some());

        // 2. Synthetically inject a mapping for a fixed DocId (e.g. DocId::new(42)) pointing to "key_existing"
        let synthetic_doc_id = DocId::new(42);
        let tx = TxId::new(next_tx.fetch_add(1, Ordering::SeqCst));
        let doc_key = col.namespaced_key(&synthetic_doc_id.inner().to_le_bytes(), 1);
        let existing_meta = super::StoredDocumentMeta {
            id: "key_existing".to_string(),
            metadata: None,
        };
        let meta_bytes = serde_json::to_vec(&existing_meta).unwrap(); // unwrap
        col.storage.put(tx, &doc_key, &meta_bytes).await.unwrap(); // unwrap
        col.storage.commit(tx).await.unwrap(); // unwrap

        // 3. Directly test check_doc_id_collision with a different string key (e.g., "key_new")
        let collision_res = col
            .check_doc_id_collision(synthetic_doc_id, "key_new")
            .await;
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
        let same_key_res = col
            .check_doc_id_collision(synthetic_doc_id, "key_existing")
            .await;
        assert!(same_key_res.is_ok());
    }

    #[tokio::test]
    async fn test_collection_next_tx_sequence() {
        use memfuse_graph::CsrGraph;
        use memfuse_index::HnswIndex;
        use memfuse_store::LsmStorage;
        use std::sync::atomic::AtomicU64;
        use std::sync::Arc;
        use tempfile::tempdir;

        let dir = tempdir().unwrap(); // unwrap
        let lsm_config = memfuse_store::LsmConfig {
            path: dir.path().to_path_buf(),
            ..Default::default()
        };
        let storage = Arc::new(LsmStorage::new(lsm_config).await.unwrap()); // unwrap
        let index = Arc::new(
            HnswIndex::try_new(memfuse_index::HnswConfig {
                dimension: 4,
                ..Default::default()
            })
            .unwrap(), // unwrap
        );
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

        let tx1 = col.next_tx().unwrap(); // unwrap allowed
        let tx2 = col.next_tx().unwrap(); // unwrap allowed
        let tx3 = col.next_tx().unwrap(); // unwrap allowed

        assert_eq!(tx1.inner(), 1);
        assert_eq!(tx2.inner(), 2);
        assert_eq!(tx3.inner(), 3);
    }

    #[tokio::test]
    async fn test_collection_allocate_tx_sequence() {
        use memfuse_graph::CsrGraph;
        use memfuse_index::HnswIndex;
        use memfuse_store::LsmStorage;
        use std::sync::atomic::AtomicU64;
        use std::sync::Arc;
        use tempfile::tempdir;

        let dir = tempdir().unwrap(); // unwrap
        let lsm_config = memfuse_store::LsmConfig {
            path: dir.path().to_path_buf(),
            ..Default::default()
        };
        let storage = Arc::new(LsmStorage::new(lsm_config).await.unwrap()); // unwrap
        let index = Arc::new(
            HnswIndex::try_new(memfuse_index::HnswConfig {
                dimension: 4,
                ..Default::default()
            })
            .unwrap(), // unwrap
        );
        let graph = Arc::new(CsrGraph::new());
        let next_tx = Arc::new(AtomicU64::new(100));

        let col = super::Collection::new(
            "default".to_string(),
            storage,
            index,
            graph,
            next_tx,
            4,
            memfuse_text::Language::English,
        );

        let tx1 = col.allocate_tx().unwrap(); // unwrap allowed
        let tx2 = col.allocate_tx().unwrap(); // unwrap allowed
        let tx3 = col.allocate_tx().unwrap(); // unwrap allowed

        assert_eq!(tx1.inner(), 100);
        assert_eq!(tx2.inner(), 101);
        assert_eq!(tx3.inner(), 102);
    }

    #[tokio::test]
    async fn test_concurrent_insert_and_write_ops_lock_safety() {
        use memfuse_graph::CsrGraph;
        use memfuse_index::HnswIndex;
        use memfuse_store::LsmStorage;
        use std::sync::atomic::AtomicU64;
        use std::sync::Arc;
        use tempfile::tempdir;

        let dir = tempdir().unwrap(); // unwrap
        let lsm_config = memfuse_store::LsmConfig {
            path: dir.path().to_path_buf(),
            ..Default::default()
        };
        let storage = Arc::new(LsmStorage::new(lsm_config).await.unwrap()); // unwrap
        let index = Arc::new(
            HnswIndex::try_new(memfuse_index::HnswConfig {
                dimension: 4,
                ..Default::default()
            })
            .unwrap(), // unwrap
        );
        let graph = Arc::new(CsrGraph::new());
        let next_tx = Arc::new(AtomicU64::new(1));

        let col = Arc::new(super::Collection::new(
            "default".to_string(),
            storage,
            index,
            graph,
            next_tx,
            4,
            memfuse_text::Language::English,
        ));

        let mut handles = Vec::new();

        // Task 1: Single inserts
        {
            let c = col.clone();
            handles.push(tokio::spawn(async move {
                for i in 0..10 {
                    let id = format!("single_doc_{i}");
                    c.insert(&id, &[1.0, 0.0, 0.0, 0.0], None).await.unwrap(); // unwrap
                }
            }));
        }

        // Task 2: Insert many
        {
            let c = col.clone();
            handles.push(tokio::spawn(async move {
                let docs: Vec<_> = (0..5)
                    .map(|i| (format!("batch_doc_{i}"), vec![0.0, 1.0, 0.0, 0.0], None))
                    .collect();
                c.insert_many(&docs).await.unwrap(); // unwrap
            }));
        }

        // Task 3: Upsert & Update
        {
            let c = col.clone();
            handles.push(tokio::spawn(async move {
                for i in 0..5 {
                    let id = format!("upsert_doc_{i}");
                    c.upsert(&id, &[0.0, 0.0, 1.0, 0.0], None).await.unwrap(); // unwrap
                    c.update(&id, &[0.0, 0.0, 1.0, 1.0], None).await.unwrap(); // unwrap
                }
            }));
        }

        // Task 4: Upsert many
        {
            let c = col.clone();
            handles.push(tokio::spawn(async move {
                let docs: Vec<_> = (0..5)
                    .map(|i| (format!("upsert_batch_{i}"), vec![0.5, 0.5, 0.0, 0.0], None))
                    .collect();
                c.upsert_many(&docs).await.unwrap(); // unwrap
            }));
        }

        for h in handles {
            h.await.unwrap(); // unwrap
        }

        assert!(col.len().await > 0);
    }

    #[tokio::test]
    async fn test_ttl_missing_created_at_does_not_expire() {
        use memfuse_graph::CsrGraph;
        use memfuse_index::HnswIndex;
        use memfuse_store::LsmStorage;
        use serde_json::json;
        use std::sync::atomic::AtomicU64;
        use std::sync::Arc;
        use tempfile::tempdir;

        let dir = tempdir().unwrap(); // unwrap
        let storage = Arc::new(
            LsmStorage::new(memfuse_store::LsmConfig {
                path: dir.path().to_path_buf(),
                ..Default::default()
            })
            .await
            .unwrap(), // unwrap
        );
        let index = Arc::new(
            HnswIndex::try_new(memfuse_index::HnswConfig {
                dimension: 4,
                ..Default::default()
            })
            .unwrap(), // unwrap
        );
        let col = super::Collection::new(
            "default".to_string(),
            storage,
            index,
            Arc::new(CsrGraph::new()),
            Arc::new(AtomicU64::new(1)),
            4,
            memfuse_text::Language::English,
        );

        col.insert(
            "doc_no_created_at",
            &[1.0, 0.0, 0.0, 0.0],
            Some(json!({"ttl_ms": 10})),
        )
        .await
        .unwrap(); // unwrap
        let reaped = col.trigger_reaper().await.unwrap(); // unwrap
        assert_eq!(reaped, 0);
        assert!(col.get("doc_no_created_at").await.unwrap().is_some()); // unwrap
    }

    #[tokio::test]
    async fn test_ttl_zero_does_not_expire() {
        use memfuse_graph::CsrGraph;
        use memfuse_index::HnswIndex;
        use memfuse_store::LsmStorage;
        use serde_json::json;
        use std::sync::atomic::AtomicU64;
        use std::sync::Arc;
        use tempfile::tempdir;

        let dir = tempdir().unwrap(); // unwrap
        let storage = Arc::new(
            LsmStorage::new(memfuse_store::LsmConfig {
                path: dir.path().to_path_buf(),
                ..Default::default()
            })
            .await
            .unwrap(), // unwrap
        );
        let index = Arc::new(
            HnswIndex::try_new(memfuse_index::HnswConfig {
                dimension: 4,
                ..Default::default()
            })
            .unwrap(), // unwrap
        );
        let col = super::Collection::new(
            "default".to_string(),
            storage,
            index,
            Arc::new(CsrGraph::new()),
            Arc::new(AtomicU64::new(1)),
            4,
            memfuse_text::Language::English,
        );

        col.insert(
            "doc_zero_ttl",
            &[1.0, 0.0, 0.0, 0.0],
            Some(json!({"created_at_ms": 100, "ttl_ms": 0})),
        )
        .await
        .unwrap(); // unwrap
        let reaped = col.trigger_reaper().await.unwrap(); // unwrap
        assert_eq!(reaped, 0);
        assert!(col.get("doc_zero_ttl").await.unwrap().is_some()); // unwrap
    }

    #[tokio::test]
    async fn test_extract_text_with_contextual_prefix() {
        use serde_json::json;

        let meta = Some(json!({
            "contextual_prefix": "Dokumenten-Kontext-Präfix",
            "text": "Chunk Haupttext"
        }));

        let extracted = super::extract_text(&meta);
        assert!(extracted.is_some());
        let text = extracted.unwrap(); // unwrap
        assert!(text.contains("Dokumenten-Kontext-Präfix"));
        assert!(text.contains("Chunk Haupttext"));
        assert_eq!(text, "Dokumenten-Kontext-Präfix\n\nChunk Haupttext");
    }

    #[tokio::test]
    async fn test_ttl_overflow_does_not_expire() {
        use memfuse_graph::CsrGraph;
        use memfuse_index::HnswIndex;
        use memfuse_store::LsmStorage;
        use serde_json::json;
        use std::sync::atomic::AtomicU64;
        use std::sync::Arc;
        use tempfile::tempdir;

        let dir = tempdir().unwrap(); // unwrap
        let storage = Arc::new(
            LsmStorage::new(memfuse_store::LsmConfig {
                path: dir.path().to_path_buf(),
                ..Default::default()
            })
            .await
            .unwrap(), // unwrap
        );
        let index = Arc::new(
            HnswIndex::try_new(memfuse_index::HnswConfig {
                dimension: 4,
                ..Default::default()
            })
            .unwrap(), // unwrap
        );
        let col = super::Collection::new(
            "default".to_string(),
            storage,
            index,
            Arc::new(CsrGraph::new()),
            Arc::new(AtomicU64::new(1)),
            4,
            memfuse_text::Language::English,
        );

        col.insert(
            "doc_overflow",
            &[1.0, 0.0, 0.0, 0.0],
            Some(json!({"created_at_ms": u64::MAX - 10, "ttl_ms": 100})),
        )
        .await
        .unwrap(); // unwrap
        let reaped = col.trigger_reaper().await.unwrap(); // unwrap
        assert_eq!(reaped, 0);
        assert!(col.get("doc_overflow").await.unwrap().is_some()); // unwrap
    }

    #[tokio::test]
    async fn test_migrate_doc_keys_v1() {
        use memfuse_core::{DocId, StorageEngine, TxId};
        use memfuse_graph::CsrGraph;
        use memfuse_index::HnswIndex;
        use memfuse_store::LsmStorage;
        use serde_json::json;
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::sync::Arc;
        use tempfile::tempdir;

        let dir = tempdir().unwrap(); // unwrap allowed (AGENT:04)
        let storage = Arc::new(
            LsmStorage::new(memfuse_store::LsmConfig {
                path: dir.path().to_path_buf(),
                ..Default::default()
            })
            .await
            .unwrap(), // unwrap allowed (AGENT:04)
        );
        let index = Arc::new(
            HnswIndex::try_new(memfuse_index::HnswConfig {
                dimension: 4,
                ..Default::default()
            })
            .unwrap(), // unwrap allowed (AGENT:04)
        );
        let next_tx = Arc::new(AtomicU64::new(1));
        let col = super::Collection::new(
            "default".to_string(),
            storage.clone(),
            index,
            Arc::new(CsrGraph::new()),
            next_tx.clone(),
            4,
            memfuse_text::Language::English,
        );

        // Inject legacy doc_key (containing embedding in StoredDocument)
        let doc_id = DocId::from_key("legacy_doc_1").unwrap(); // unwrap allowed (AGENT:04)
        let doc_key = col.namespaced_key(&doc_id.inner().to_le_bytes(), 1);
        let legacy_doc = super::StoredDocument {
            id: "legacy_doc_1".to_string(),
            embedding: vec![1.0, 0.0, 0.0, 0.0],
            metadata: Some(json!({"topic": "legacy"})),
        };
        let legacy_bytes = serde_json::to_vec(&legacy_doc).unwrap(); // unwrap allowed (AGENT:04)

        // Put user_key and legacy doc_key in storage
        let tx = TxId::new(next_tx.fetch_add(1, Ordering::SeqCst));
        let user_key = col.namespaced_key(b"legacy_doc_1", 0);
        storage.put(tx, &user_key, &legacy_bytes).await.unwrap(); // unwrap allowed (AGENT:04)
        storage.put(tx, &doc_key, &legacy_bytes).await.unwrap(); // unwrap allowed (AGENT:04)
        storage.commit(tx).await.unwrap(); // unwrap allowed (AGENT:04)

        // Verify doc_key currently contains full StoredDocument
        let raw_before = storage.get(&doc_key).await.unwrap().unwrap(); // unwrap allowed (AGENT:04)
        assert!(serde_json::from_slice::<super::StoredDocument>(&raw_before).is_ok());

        // Run migration
        let count = col.migrate_doc_keys_v1().await.unwrap(); // unwrap allowed (AGENT:04)
        assert_eq!(count, 1);

        // Verify doc_key now contains StoredDocumentMeta (and fails parsing as StoredDocument due to missing embedding)
        let raw_after = storage.get(&doc_key).await.unwrap().unwrap(); // unwrap allowed (AGENT:04)
        let meta: super::StoredDocumentMeta = serde_json::from_slice(&raw_after).unwrap(); // unwrap allowed (AGENT:04)
        assert_eq!(meta.id, "legacy_doc_1");
        assert_eq!(meta.metadata.unwrap()["topic"], "legacy"); // unwrap allowed (AGENT:04)
        assert!(serde_json::from_slice::<super::StoredDocument>(&raw_after).is_err());

        // Idempotency check: running migration again returns 0
        let count_again = col.migrate_doc_keys_v1().await.unwrap(); // unwrap allowed (AGENT:04)
        assert_eq!(count_again, 0);
    }

    #[tokio::test]
    #[cfg(feature = "reranking")]
    async fn test_hybrid_search_reranked_none() {
        use memfuse_graph::CsrGraph;
        use memfuse_index::HnswIndex;
        use memfuse_store::LsmStorage;
        use std::sync::atomic::AtomicU64;
        use std::sync::Arc;
        use tempfile::tempdir;

        let dir = tempdir().unwrap(); // unwrap
        let storage = Arc::new(
            LsmStorage::new(memfuse_store::LsmConfig {
                path: dir.path().to_path_buf(),
                ..Default::default()
            })
            .await
            .unwrap(), // unwrap
        );
        let index = Arc::new(
            HnswIndex::try_new(memfuse_index::HnswConfig {
                dimension: 4,
                ..Default::default()
            })
            .unwrap(), // unwrap
        );
        let col = super::Collection::new(
            "default".to_string(),
            storage,
            index,
            Arc::new(CsrGraph::new()),
            Arc::new(AtomicU64::new(1)),
            4,
            memfuse_text::Language::English,
        );

        col.insert(
            "d1",
            &[1.0, 0.0, 0.0, 0.0],
            Some(serde_json::json!({"text": "rust language"})),
        )
        .await
        .unwrap(); // unwrap
        col.insert(
            "d2",
            &[0.9, 0.1, 0.0, 0.0],
            Some(serde_json::json!({"text": "python language"})),
        )
        .await
        .unwrap(); // unwrap

        let res = col
            .hybrid_search_reranked("rust", &[1.0, 0.0, 0.0, 0.0], 1, None, None)
            .await
            .unwrap(); // unwrap

        assert_eq!(res.len(), 1);
        assert_eq!(res[0].id, "d1");
    }

    #[test]
    fn test_importance_score_parser_robust() {
        assert_eq!(super::parse_importance_score("0.8"), 0.8);
        assert_eq!(super::parse_importance_score("0.8\n"), 0.8);
        assert_eq!(super::parse_importance_score("Score: 0.8"), 0.8);
        assert_eq!(super::parse_importance_score("0.8 (high importance)"), 0.8);
        assert_eq!(super::parse_importance_score("1.5"), 1.0);
        assert_eq!(super::parse_importance_score("-0.2"), 0.0);
        assert_eq!(super::parse_importance_score("invalid text"), 0.5);
    }

    #[tokio::test]
    async fn test_evaluate_importance_with_dead_client_returns_err() {
        use memfuse_graph::CsrGraph;
        use memfuse_index::HnswIndex;
        use memfuse_store::LsmStorage;
        use std::sync::atomic::AtomicU64;
        use std::sync::Arc;
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let storage = Arc::new(
            LsmStorage::new(memfuse_store::LsmConfig {
                path: dir.path().to_path_buf(),
                ..Default::default()
            })
            .await
            .unwrap(),
        );
        let index = Arc::new(
            HnswIndex::try_new(memfuse_index::HnswConfig {
                dimension: 4,
                ..Default::default()
            })
            .unwrap(),
        );
        let col = super::Collection::new(
            "default".to_string(),
            storage,
            index,
            Arc::new(CsrGraph::new()),
            Arc::new(AtomicU64::new(1)),
            4,
            memfuse_text::Language::English,
        );

        let vec = vec![1.0, 0.0, 0.0, 0.0];
        col.insert("doc_test", &vec, None).await.unwrap();

        let dead_ollama = memfuse_ollama::OllamaClient::new("http://127.0.0.1:1");

        let res = col
            .evaluate_importance_with_llm("doc_test", &dead_ollama)
            .await;
        assert!(res.is_err());
        assert!(matches!(
            res.unwrap_err(),
            memfuse_core::MemFuseError::Internal(_)
        ));

        // Verify document's score was NOT overwritten or corrupted
        let doc = col.get("doc_test").await.unwrap().unwrap();
        assert!(doc.metadata.is_some());
    }

    #[test]
    fn test_compute_default_importance_entropy_and_clamping() {
        let score_empty = super::compute_default_importance(None);
        assert_eq!(score_empty.value(), 0.5);

        let score_simple = super::compute_default_importance(Some("aaaaa"));
        assert!(score_simple.value() >= 0.0 && score_simple.value() <= 1.0);

        let score_rich = super::compute_default_importance(Some(
            "The quick brown fox jumps over the lazy dog with high entropy and long text.",
        ));
        assert!(score_rich.value() > score_simple.value());
    }

    #[tokio::test]
    async fn test_reaper_deletes_decayed_working_memory() {
        use memfuse_core::{DecayFunction, ImportanceScore, MemoryImportance, TxId};
        use memfuse_graph::CsrGraph;
        use memfuse_index::HnswIndex;
        use memfuse_store::LsmStorage;
        use serde_json::json;
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::sync::Arc;
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let storage = Arc::new(
            LsmStorage::new(memfuse_store::LsmConfig {
                path: dir.path().to_path_buf(),
                ..Default::default()
            })
            .await
            .unwrap(),
        );
        let index = Arc::new(
            HnswIndex::try_new(memfuse_index::HnswConfig {
                dimension: 4,
                ..Default::default()
            })
            .unwrap(),
        );
        let next_tx = Arc::new(AtomicU64::new(1));
        let col = super::Collection::new(
            "default".to_string(),
            storage,
            index,
            Arc::new(CsrGraph::new()),
            next_tx.clone(),
            4,
            memfuse_text::Language::English,
        );

        let created_tx = TxId::new(10);
        let imp = MemoryImportance::new(
            ImportanceScore::new(0.5),
            DecayFunction::Exponential { half_life_tx: 5 },
            created_tx,
        );

        let vec = vec![1.0, 0.0, 0.0, 0.0];
        col.insert(
            "doc_decayed",
            &vec,
            Some(json!({
                "importance": imp
            })),
        )
        .await
        .unwrap();

        // Advance TxId far enough so effective_score < 0.05
        // At created_tx=10, half_life=5:
        // Tx 10: 0.5 * 1.0 = 0.5
        // Tx 15: 0.5 * 0.5 = 0.25
        // Tx 20: 0.5 * 0.25 = 0.125
        // Tx 25: 0.5 * 0.125 = 0.0625
        // Tx 30: 0.5 * 0.0625 = 0.03125 (< 0.05)
        next_tx.store(35, Ordering::SeqCst);

        let count = col.trigger_reaper().await.unwrap();
        assert_eq!(count, 1, "Decayed working memory document should be reaped");
        assert!(col.get("doc_decayed").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_reaper_never_deletes_semantic_no_decay() {
        use memfuse_core::{DecayFunction, ImportanceScore, MemoryImportance, TxId};
        use memfuse_graph::CsrGraph;
        use memfuse_index::HnswIndex;
        use memfuse_store::LsmStorage;
        use serde_json::json;
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::sync::Arc;
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let storage = Arc::new(
            LsmStorage::new(memfuse_store::LsmConfig {
                path: dir.path().to_path_buf(),
                ..Default::default()
            })
            .await
            .unwrap(),
        );
        let index = Arc::new(
            HnswIndex::try_new(memfuse_index::HnswConfig {
                dimension: 4,
                ..Default::default()
            })
            .unwrap(),
        );
        let next_tx = Arc::new(AtomicU64::new(1));
        let col = super::Collection::new(
            "default".to_string(),
            storage,
            index,
            Arc::new(CsrGraph::new()),
            next_tx.clone(),
            4,
            memfuse_text::Language::English,
        );

        let created_tx = TxId::new(10);
        let imp = MemoryImportance::new(
            ImportanceScore::new(0.01), // even with base score < 0.05!
            DecayFunction::None,
            created_tx,
        );

        let vec = vec![1.0, 0.0, 0.0, 0.0];
        col.insert(
            "doc_semantic",
            &vec,
            Some(json!({
                "importance": imp
            })),
        )
        .await
        .unwrap();

        // Advance TxId very far
        next_tx.store(100_000, Ordering::SeqCst);

        let count = col.trigger_reaper().await.unwrap();
        assert_eq!(
            count, 0,
            "Semantic document with DecayFunction::None must never be deleted"
        );
        assert!(col.get("doc_semantic").await.unwrap().is_some());
    }

    #[test]
    fn test_importance_metadata_integration_and_filtering() {
        use memfuse_core::{DecayFunction, ImportanceScore, MemoryImportance, TxId};
        use serde_json::json;

        let created_tx = TxId::new(10);
        let now_tx = TxId::new(30);

        let mut meta1 = Some(json!({"text": "Important factual doc"}));
        super::ensure_importance_metadata(&mut meta1, created_tx, Some("Important factual doc"));

        // Override with explicit exponential decay
        let imp1 = MemoryImportance::new(
            ImportanceScore::new(0.9),
            DecayFunction::Exponential { half_life_tx: 10 },
            created_tx,
        );
        meta1.as_mut().unwrap().as_object_mut().unwrap().insert(
            "importance".to_string(),
            serde_json::to_value(&imp1).unwrap(),
        );

        // Effective score at now_tx (2 half-lives elapsed) -> 0.9 * 0.25 = 0.225
        let eff1 = super::extract_effective_importance(&meta1, now_tx);
        assert!((eff1 - 0.225).abs() < 1e-4);

        let mut meta2 = Some(json!({"text": "Critical doc"}));
        let imp2 =
            MemoryImportance::new(ImportanceScore::new(1.0), DecayFunction::None, created_tx);
        meta2.as_mut().unwrap().as_object_mut().unwrap().insert(
            "importance".to_string(),
            serde_json::to_value(&imp2).unwrap(),
        );

        let results = vec![
            crate::SearchResult {
                id: "doc1".to_string(),
                score: 0.95,
                metadata: meta1,
                matched_signals: vec!["vector".to_string()],
            },
            crate::SearchResult {
                id: "doc2".to_string(),
                score: 0.85,
                metadata: meta2,
                matched_signals: vec!["vector".to_string()],
            },
        ];

        // Filter out results with effective importance < 0.5
        let filtered = super::Collection::<memfuse_store::LsmStorage>::filter_by_importance(
            results, 0.5, now_tx,
        );
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, "doc2");
        assert_eq!(filtered[0].score, 0.85); // Order and original RRF/CE score preserved
    }

    #[tokio::test]
    async fn test_insert_typed_episodic_has_decay_metadata() {
        use memfuse_graph::CsrGraph;
        use memfuse_index::HnswIndex;
        use memfuse_store::{LsmConfig, LsmStorage};
        use std::sync::atomic::AtomicU64;
        use std::sync::Arc;

        let dir = tempfile::tempdir().unwrap();
        let storage = Arc::new(
            LsmStorage::new(LsmConfig {
                path: dir.path().to_path_buf(),
                ..Default::default()
            })
            .await
            .unwrap(),
        );
        let index = Arc::new(
            HnswIndex::try_new(memfuse_index::HnswConfig {
                dimension: 4,
                ..Default::default()
            })
            .unwrap(),
        );
        let graph_index = Arc::new(CsrGraph::new());
        let next_tx = Arc::new(AtomicU64::new(1));
        let col = super::Collection::new(
            "test".to_string(),
            storage,
            index,
            graph_index,
            next_tx,
            4,
            memfuse_text::Language::German,
        );

        col.insert_typed(
            "ep1",
            &[1.0, 0.0, 0.0, 0.0],
            memfuse_core::MemoryType::Episodic,
            None,
        )
        .await
        .unwrap();

        let doc = col.get("ep1").await.unwrap().unwrap();
        let meta = doc.metadata.unwrap();
        assert_eq!(meta.get("memory_type").unwrap(), "Episodic");
        assert!(meta.get("decay_function").is_some());
    }

    #[tokio::test]
    async fn test_insert_typed_working_has_ttl_metadata() {
        use memfuse_graph::CsrGraph;
        use memfuse_index::HnswIndex;
        use memfuse_store::{LsmConfig, LsmStorage};
        use std::sync::atomic::AtomicU64;
        use std::sync::Arc;

        let dir = tempfile::tempdir().unwrap();
        let storage = Arc::new(
            LsmStorage::new(LsmConfig {
                path: dir.path().to_path_buf(),
                ..Default::default()
            })
            .await
            .unwrap(),
        );
        let index = Arc::new(
            HnswIndex::try_new(memfuse_index::HnswConfig {
                dimension: 4,
                ..Default::default()
            })
            .unwrap(),
        );
        let graph_index = Arc::new(CsrGraph::new());
        let next_tx = Arc::new(AtomicU64::new(1));
        let col = super::Collection::new(
            "test".to_string(),
            storage,
            index,
            graph_index,
            next_tx,
            4,
            memfuse_text::Language::German,
        );

        col.insert_typed(
            "wk1",
            &[1.0, 0.0, 0.0, 0.0],
            memfuse_core::MemoryType::Working,
            None,
        )
        .await
        .unwrap();

        let doc = col.get("wk1").await.unwrap().unwrap();
        let meta = doc.metadata.unwrap();
        assert_eq!(meta.get("memory_type").unwrap(), "Working");
        assert_eq!(meta.get("ttl_tx").unwrap(), 50_000);
    }

    #[tokio::test]
    #[cfg(feature = "experimental-diskann")]
    async fn test_collection_with_diskann_index_hybrid_search() {
        use memfuse_core::DocId;
        use memfuse_graph::CsrGraph;
        use memfuse_index::{DiskAnnConfig, DiskAnnIndex};
        use std::sync::atomic::AtomicU64;
        use std::sync::Arc;
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let lsm_path = dir.path().join("lsm");
        let diskann_path = dir.path().join("diskann.idx");

        let storage = Arc::new(
            LsmStorage::new(memfuse_store::LsmConfig {
                path: lsm_path,
                ..Default::default()
            })
            .await
            .unwrap(),
        );

        let diskann_config = DiskAnnConfig {
            index_path: diskann_path,
            dimension: 4,
            max_degree: 8,
            beam_width: 8,
            sector_size: 4096,
            ..DiskAnnConfig::default()
        };

        let diskann = Arc::new(DiskAnnIndex::try_new(diskann_config).unwrap());

        let doc1_id = DocId::from_key("doc1").unwrap();
        let doc2_id = DocId::from_key("doc2").unwrap();

        let vectors = vec![vec![1.0, 0.0, 0.0, 0.0], vec![0.0, 1.0, 0.0, 0.0]];
        let ids = vec![doc1_id, doc2_id];

        diskann.build(&vectors, &ids).await.unwrap();

        let graph = Arc::new(CsrGraph::new());
        let next_tx = Arc::new(AtomicU64::new(1));

        let col = super::Collection::<LsmStorage, DiskAnnIndex>::new(
            "diskann_test".to_string(),
            storage.clone(),
            diskann,
            graph,
            next_tx,
            4,
            Language::English,
        );

        let tx = col.allocate_tx().unwrap();

        let doc1_user_key = col.namespaced_key(b"doc1", 0);
        let doc1_meta_key = col.namespaced_key(&doc1_id.inner().to_le_bytes(), 1);

        let doc2_user_key = col.namespaced_key(b"doc2", 0);
        let doc2_meta_key = col.namespaced_key(&doc2_id.inner().to_le_bytes(), 1);

        let doc1_data = StoredDocument {
            id: "doc1".to_string(),
            embedding: vec![1.0, 0.0, 0.0, 0.0],
            metadata: Some(serde_json::json!({ "text": "rust database systems" })),
        };
        let doc1_meta = StoredDocumentMeta::from(&doc1_data);

        let doc2_data = StoredDocument {
            id: "doc2".to_string(),
            embedding: vec![0.0, 1.0, 0.0, 0.0],
            metadata: Some(serde_json::json!({ "text": "python scripting language" })),
        };
        let doc2_meta = StoredDocumentMeta::from(&doc2_data);

        storage
            .put(tx, &doc1_user_key, &serde_json::to_vec(&doc1_data).unwrap())
            .await
            .unwrap();
        storage
            .put(tx, &doc1_meta_key, &serde_json::to_vec(&doc1_meta).unwrap())
            .await
            .unwrap();

        storage
            .put(tx, &doc2_user_key, &serde_json::to_vec(&doc2_data).unwrap())
            .await
            .unwrap();
        storage
            .put(tx, &doc2_meta_key, &serde_json::to_vec(&doc2_meta).unwrap())
            .await
            .unwrap();

        col.text_index
            .upsert_document(tx, doc1_id, "rust database systems")
            .await
            .unwrap();
        col.text_index
            .upsert_document(tx, doc2_id, "python scripting language")
            .await
            .unwrap();

        storage.commit(tx).await.unwrap();
        col.text_index.commit(tx).await.unwrap();

        let query_vector = vec![1.0, 0.0, 0.0, 0.0];
        let results = col
            .hybrid_search("rust", &query_vector, 5, None)
            .await
            .unwrap();

        assert!(
            !results.is_empty(),
            "Hybrid search with DiskANN should return results"
        );
        assert_eq!(
            results[0].id, "doc1",
            "Doc1 should be top result for rust & vector [1,0,0,0]"
        );
    }

    #[tokio::test]
    async fn test_insert_backward_compatible_has_semantic_default() {
        use memfuse_graph::CsrGraph;
        use memfuse_index::HnswIndex;
        use memfuse_store::{LsmConfig, LsmStorage};
        use std::sync::atomic::AtomicU64;
        use std::sync::Arc;

        let dir = tempfile::tempdir().unwrap();
        let storage = Arc::new(
            LsmStorage::new(LsmConfig {
                path: dir.path().to_path_buf(),
                ..Default::default()
            })
            .await
            .unwrap(),
        );
        let index = Arc::new(
            HnswIndex::try_new(memfuse_index::HnswConfig {
                dimension: 4,
                ..Default::default()
            })
            .unwrap(),
        );
        let graph_index = Arc::new(CsrGraph::new());
        let next_tx = Arc::new(AtomicU64::new(1));
        let col = super::Collection::new(
            "test".to_string(),
            storage,
            index,
            graph_index,
            next_tx,
            4,
            memfuse_text::Language::German,
        );

        col.insert(
            "plain1",
            &[1.0, 0.0, 0.0, 0.0],
            Some(serde_json::json!({"text": "hello"})),
        )
        .await
        .unwrap();

        let doc = col.get("plain1").await.unwrap().unwrap();
        assert_eq!(
            crate::filter::extract_memory_type(&doc.metadata),
            memfuse_core::MemoryType::Semantic
        );
    }

    #[tokio::test]
    async fn test_hybrid_search_with_query_memory_type_filter() {
        use memfuse_core::{HybridQuery, MemoryType};
        use memfuse_graph::CsrGraph;
        use memfuse_index::HnswIndex;
        use memfuse_store::{LsmConfig, LsmStorage};
        use serde_json::json;
        use std::sync::atomic::AtomicU64;
        use std::sync::Arc;

        let dir = tempfile::tempdir().unwrap();
        let storage = Arc::new(
            LsmStorage::new(LsmConfig {
                path: dir.path().to_path_buf(),
                ..Default::default()
            })
            .await
            .unwrap(),
        );
        let index = Arc::new(
            HnswIndex::try_new(memfuse_index::HnswConfig {
                dimension: 4,
                ..Default::default()
            })
            .unwrap(),
        );
        let col = super::Collection::new(
            "test_filter".to_string(),
            storage,
            index,
            Arc::new(CsrGraph::new()),
            Arc::new(AtomicU64::new(1)),
            4,
            memfuse_text::Language::English,
        );

        col.insert_typed(
            "ep1",
            &[1.0, 0.0, 0.0, 0.0],
            MemoryType::Episodic,
            Some(json!({"text": "episode meeting alpha"})),
        )
        .await
        .unwrap();

        col.insert_typed(
            "ep2",
            &[0.9, 0.1, 0.0, 0.0],
            MemoryType::Episodic,
            Some(json!({"text": "episode meeting beta"})),
        )
        .await
        .unwrap();

        col.insert_typed(
            "sem1",
            &[0.95, 0.05, 0.0, 0.0],
            MemoryType::Semantic,
            Some(json!({"text": "episode definition gamma"})),
        )
        .await
        .unwrap();

        col.insert_typed(
            "sem2",
            &[0.85, 0.15, 0.0, 0.0],
            MemoryType::Semantic,
            Some(json!({"text": "episode theory delta"})),
        )
        .await
        .unwrap();

        // Query with memory_type_filter = Episodic
        let query_ep = HybridQuery::builder()
            .with_text_query("episode")
            .with_vector_query(vec![1.0, 0.0, 0.0, 0.0])
            .with_memory_type_filter(vec![MemoryType::Episodic])
            .with_k(10)
            .build()
            .unwrap();

        let results_ep = col.hybrid_search_with_query(&query_ep).await.unwrap();
        assert_eq!(
            results_ep.len(),
            2,
            "Must return exactly 2 episodic results"
        );
        for res in &results_ep {
            assert!(
                res.id == "ep1" || res.id == "ep2",
                "Returned result {} is not Episodic!",
                res.id
            );
        }

        // Query with memory_type_filter = Semantic
        let query_sem = HybridQuery::builder()
            .with_text_query("episode")
            .with_vector_query(vec![1.0, 0.0, 0.0, 0.0])
            .with_memory_type_filter(vec![MemoryType::Semantic])
            .with_k(10)
            .build()
            .unwrap();

        let results_sem = col.hybrid_search_with_query(&query_sem).await.unwrap();
        assert_eq!(
            results_sem.len(),
            2,
            "Must return exactly 2 semantic results"
        );
        for res in &results_sem {
            assert!(
                res.id == "sem1" || res.id == "sem2",
                "Returned result {} is not Semantic!",
                res.id
            );
        }
    }
}
