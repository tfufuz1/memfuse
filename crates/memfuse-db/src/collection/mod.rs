//! Collection orchestration layer for MemFuse.

pub mod maintenance;
pub mod relate;
pub mod search;
pub mod transaction;

use memfuse_core::{
    DocId, EntityId, GraphIndex, Result, StorageEngine, TextEmbeddingEngine, TextIndex, TxId,
    VectorIndex,
};
use memfuse_graph::CsrGraph;
use memfuse_index::HnswIndex;
use memfuse_store::LsmStorage;
use memfuse_text::inverted::InvertedIndex;
use memfuse_text::Language;
use serde::{Deserialize, Serialize};
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

/// Key in document metadata that marks TTL sequence expiry.
pub const EXPIRY_METADATA_KEY: &str = "_expire_at_seq";

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

/// Computes a default importance score for a document based on text entropy and length.
pub fn compute_default_importance(text_opt: Option<&str>) -> memfuse_core::ImportanceScore {
    let Some(text) = text_opt else {
        return memfuse_core::ImportanceScore::default(); // 0.5
    };

    let len = text.len();
    if len == 0 {
        return memfuse_core::ImportanceScore::default();
    }

    // Entropy calculation (char frequencies)
    let mut char_counts = std::collections::HashMap::new();
    let mut total_chars = 0f32;
    for c in text.chars() {
        *char_counts.entry(c).or_insert(0f32) += 1.0;
        total_chars += 1.0;
    }

    let mut entropy = 0.0f32;
    for &count in char_counts.values() {
        let p = count / total_chars;
        entropy -= p * p.log2();
    }

    // Normalized entropy factor (typical German/English text has entropy ~4.0-4.5)
    let entropy_factor = (entropy / 4.5).clamp(0.2, 1.0);

    // Length factor (asymptotically approaches 1.0 for long text)
    let length_factor = (len as f32 / (len as f32 + 200.0)).clamp(0.1, 1.0);

    let raw_score = 0.3 + 0.5 * entropy_factor + 0.2 * length_factor;
    memfuse_core::ImportanceScore::new(raw_score)
}

/// Ensures document metadata contains a valid `importance` entry (MemoryImportance).
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
        if memfuse_core::MemoryImportance::deserialize(imp_val).is_ok() {
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

    if let Ok(imp) = memfuse_core::MemoryImportance::deserialize(imp_val) {
        imp.effective_score(now_tx)
    } else if let Some(raw_f64) = imp_val.as_f64() {
        memfuse_core::ImportanceScore::new(raw_f64 as f32).value()
    } else {
        1.0
    }
}

pub(crate) fn extract_text(metadata: &Option<serde_json::Value>) -> Option<String> {
    let meta = metadata.as_ref()?;
    let obj = meta.as_object()?;

    let mut document_text = String::new();

    if let Some(text_val) = obj.get("text").or_else(|| obj.get("content")) {
        if let Some(text_str) = text_val.as_str() {
            document_text.push_str(text_str.trim());
        }
    }

    if let Some(prefix_val) = obj.get("contextual_prefix") {
        if let Some(prefix_str) = prefix_val.as_str() {
            let prefix_trimmed = prefix_str.trim();
            if !prefix_trimmed.is_empty() {
                if !document_text.is_empty() {
                    document_text = format!("{}\n\n{}", prefix_trimmed, document_text);
                } else {
                    document_text.push_str(prefix_trimmed);
                }
            }
        }
    }

    if let Some(headings_val) = obj.get("headings") {
        if let Some(headings_arr) = headings_val.as_array() {
            let headings_str: Vec<&str> =
                headings_arr.iter().filter_map(|h| h.as_str()).collect();
            if !headings_str.is_empty() {
                if !document_text.is_empty() {
                    document_text.push(' ');
                }
                document_text.push_str(&headings_str.join(" "));
            }
        }
    }

    if let Some(tags_val) = obj.get("tags") {
        if let Some(tags_arr) = tags_val.as_array() {
            let tags_str: Vec<&str> = tags_arr.iter().filter_map(|t| t.as_str()).collect();
            if !tags_str.is_empty() {
                if !document_text.is_empty() {
                    document_text.push(' ');
                }
                document_text.push_str(&tags_str.join(" "));
            }
        }
    }

    if let Some(summary_val) = obj.get("summary") {
        if let Some(summary_str) = summary_val.as_str() {
            let summary_trimmed = summary_str.trim();
            if !summary_trimmed.is_empty() {
                if !document_text.is_empty() {
                    document_text.push(' ');
                }
                document_text.push_str(summary_trimmed);
            }
        }
    }

    if let Some(title_val) = obj.get("title") {
        if let Some(title_str) = title_val.as_str() {
            let title_trimmed = title_str.trim();
            if !title_trimmed.is_empty() {
                if !document_text.is_empty() {
                    document_text.push(' ');
                }
                document_text.push_str(title_trimmed);
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
    pub(crate) insert_lock: Arc<tokio::sync::Mutex<()>>,
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
            insert_lock: self.insert_lock.clone(),
        }
    }
}

impl<S: StorageEngine> Collection<S> {
    /// Creates a new `Collection` instance with explicit language configuration.
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
            insert_lock: Arc::new(tokio::sync::Mutex::new(())),
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

    /// Sets the text embedder for this collection.
    #[tracing::instrument(level = "trace", skip(self, embedder))]
    pub async fn set_embedder(&self, embedder: Arc<dyn TextEmbeddingEngine>) -> Result<()> {
        let mut guard = self.embedder.write();
        *guard = Some(embedder);
        Ok(())
    }

    /// Repairs the index by re-syncing with the storage.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn repair(&self) -> Result<()> {
        let _guard = self.insert_lock.lock().await;
        let mut repair_count = 0;
        let docs = self.storage.scan_prefix(&self.prefix).await?;
        let indexed_ids: std::collections::HashSet<DocId> =
            self.index.all_doc_ids_from_map().into_iter().collect();

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

        // 2. Fallback: Full scan for documents missing from index
        let fallback_tx = self.next_tx()?;
        let mut fallback_any = false;
        let mut fallback_text = false;

        for (namespaced_key, value) in docs {
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

    /// Generates a namespaced key byte vector for storage.
    pub(crate) fn namespaced_key(&self, subkey: &[u8], key_type: u8) -> Vec<u8> {
        if self.name == "default" {
            let prefix_str = match key_type {
                0 => "", // user key
                1 => "__docid:",
                2 => "__rel:",
                3 => "__intent:",
                4 => "", // graph community
                _ => "__other:",
            };
            [prefix_str.as_bytes(), subkey].concat()
        } else {
            let mut key = self.prefix.clone();
            key.push(key_type);
            key.extend_from_slice(subkey);
            key
        }
    }

    /// Returns a reference to the underlying storage engine.
    pub fn storage(&self) -> &Arc<S> {
        &self.storage
    }

    /// Returns the prefix slice for this collection.
    pub fn user_key_prefix(&self) -> Vec<u8> {
        if self.name == "default" {
            Vec::new()
        } else {
            let mut p = self.prefix.clone();
            p.push(0);
            p
        }
    }

    /// Returns the name of the collection.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the total number of documents in the collection.
    pub async fn len(&self) -> usize {
        let seq = match self.snapshot_seq().await {
            Ok(s) => s,
            Err(_) => return 0,
        };
        let prefix = if self.name == "default" {
            b"__docid:".to_vec()
        } else {
            let mut p = self.prefix.clone();
            p.push(1);
            p
        };
        self.storage
            .scan_prefix_at(&prefix, seq)
            .await
            .map(|v| v.len())
            .unwrap_or(0)
    }

    /// Returns the configured vector dimension for this collection.
    pub fn dimension(&self) -> usize {
        self.dimension
    }

    /// Returns whether the collection contains no documents.
    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }

    /// Returns statistics for the vector index.
    pub async fn stats(&self) -> Result<memfuse_core::VectorIndexStats> {
        self.index.stats().await
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

        col.insert_with_ttl("temp_doc", &vec, None, 5)
            .await
            .unwrap();

        let doc = col.get("temp_doc").await.unwrap();
        assert!(doc.is_some(), "Document must exist before TTL expiration");

        for i in 0..5 {
            col.insert(&format!("dummy_{i}"), &vec, None)
                .await
                .unwrap();
        }

        let reaped = col.reap_expired_documents(100).await.unwrap();
        assert_eq!(reaped, 1, "Expired document should be reaped");

        let doc_after = col.get("temp_doc").await.unwrap();
        assert!(doc_after.is_none(), "Document must be deleted after TTL expiry");

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

        let rels = col.scan_prefix("__rel:").await.unwrap();
        assert_eq!(rels.len(), 1);
        assert!(rels[0].0.contains("doc1:references:doc2"));

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

        assert_eq!(graph.entity_count(), 0);
    }

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

        let res = col.relate("entity_a", "entity_b", "connects").await;
        assert!(
            res.is_err(),
            "relate() must return Err when graph commit fails"
        );

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

        let embedder: Arc<dyn TextEmbeddingEngine> = Arc::new(FakeEmbedder);
        let result = embedder.embed("hello").await.unwrap();
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
        let index = Arc::new(HnswIndex::try_new(hnsw_config).unwrap());
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
            .unwrap();

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
        let index = Arc::new(HnswIndex::try_new(hnsw_config).unwrap());
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

        let res_zero = col
            .hybrid_search("test", &[0.1, 0.2, 0.3, 0.4], 0, None)
            .await
            .unwrap();
        assert!(res_zero.is_empty(), "k=0 must return empty result list");

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
        let index = Arc::new(
            HnswIndex::try_new(memfuse_index::HnswConfig {
                dimension: 4,
                ..Default::default()
            })
            .unwrap(),
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

        let id1 = "key_alpha";
        let emb1 = vec![1.0, 0.0, 0.0, 0.0];
        col.insert(id1, &emb1, None).await.unwrap();

        let doc1 = col.get(id1).await.unwrap();
        assert!(doc1.is_some());

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

        let dir = tempdir().unwrap();
        let lsm_config = memfuse_store::LsmConfig {
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

        let tx1 = col.next_tx().unwrap();
        let tx2 = col.next_tx().unwrap();
        let tx3 = col.next_tx().unwrap();

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

        let dir = tempdir().unwrap();
        let lsm_config = memfuse_store::LsmConfig {
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

        let tx1 = col.allocate_tx().unwrap();
        let tx2 = col.allocate_tx().unwrap();
        let tx3 = col.allocate_tx().unwrap();

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

        let dir = tempdir().unwrap();
        let lsm_config = memfuse_store::LsmConfig {
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

        {
            let c = col.clone();
            handles.push(tokio::spawn(async move {
                for i in 0..10 {
                    let id = format!("single_doc_{i}");
                    c.insert(&id, &[1.0, 0.0, 0.0, 0.0], None).await.unwrap();
                }
            }));
        }

        {
            let c = col.clone();
            handles.push(tokio::spawn(async move {
                let docs: Vec<_> = (0..5)
                    .map(|i| (format!("batch_doc_{i}"), vec![0.0, 1.0, 0.0, 0.0], None))
                    .collect();
                c.insert_many(&docs).await.unwrap();
            }));
        }

        {
            let c = col.clone();
            handles.push(tokio::spawn(async move {
                for i in 0..5 {
                    let id = format!("upsert_doc_{i}");
                    c.upsert(&id, &[0.0, 0.0, 1.0, 0.0], None).await.unwrap();
                    c.update(&id, &[0.0, 0.0, 1.0, 1.0], None).await.unwrap();
                }
            }));
        }

        {
            let c = col.clone();
            handles.push(tokio::spawn(async move {
                let docs: Vec<_> = (0..5)
                    .map(|i| (format!("upsert_batch_{i}"), vec![0.5, 0.5, 0.0, 0.0], None))
                    .collect();
                c.upsert_many(&docs).await.unwrap();
            }));
        }

        for h in handles {
            h.await.unwrap();
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

        col.insert(
            "doc_no_created_at",
            &[1.0, 0.0, 0.0, 0.0],
            Some(json!({"ttl_ms": 10})),
        )
        .await
        .unwrap();
        let reaped = col.trigger_reaper().await.unwrap();
        assert_eq!(reaped, 0);
        assert!(col.get("doc_no_created_at").await.unwrap().is_some());
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

        col.insert(
            "doc_zero_ttl",
            &[1.0, 0.0, 0.0, 0.0],
            Some(json!({"created_at_ms": 100, "ttl_ms": 0})),
        )
        .await
        .unwrap();
        let reaped = col.trigger_reaper().await.unwrap();
        assert_eq!(reaped, 0);
        assert!(col.get("doc_zero_ttl").await.unwrap().is_some());
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
        let text = extracted.unwrap();
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

        col.insert(
            "doc_overflow",
            &[1.0, 0.0, 0.0, 0.0],
            Some(json!({"created_at_ms": u64::MAX - 10, "ttl_ms": 100})),
        )
        .await
        .unwrap();
        let reaped = col.trigger_reaper().await.unwrap();
        assert_eq!(reaped, 0);
        assert!(col.get("doc_overflow").await.unwrap().is_some());
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
            storage.clone(),
            index,
            Arc::new(CsrGraph::new()),
            next_tx.clone(),
            4,
            memfuse_text::Language::English,
        );

        let doc_id = DocId::from_key("legacy_doc_1").unwrap();
        let doc_key = col.namespaced_key(&doc_id.inner().to_le_bytes(), 1);
        let legacy_doc = super::StoredDocument {
            id: "legacy_doc_1".to_string(),
            embedding: vec![1.0, 0.0, 0.0, 0.0],
            metadata: Some(json!({"topic": "legacy"})),
        };
        let legacy_bytes = serde_json::to_vec(&legacy_doc).unwrap();

        let tx = TxId::new(next_tx.fetch_add(1, Ordering::SeqCst));
        let user_key = col.namespaced_key(b"legacy_doc_1", 0);
        storage.put(tx, &user_key, &legacy_bytes).await.unwrap();
        storage.put(tx, &doc_key, &legacy_bytes).await.unwrap();
        storage.commit(tx).await.unwrap();

        let raw_before = storage.get(&doc_key).await.unwrap().unwrap();
        assert!(serde_json::from_slice::<super::StoredDocument>(&raw_before).is_ok());

        let count = col.migrate_doc_keys_v1().await.unwrap();
        assert_eq!(count, 1);

        let raw_after = storage.get(&doc_key).await.unwrap().unwrap();
        let meta: super::StoredDocumentMeta = serde_json::from_slice(&raw_after).unwrap();
        assert_eq!(meta.id, "legacy_doc_1");
        assert_eq!(meta.metadata.unwrap()["topic"], "legacy");
        assert!(serde_json::from_slice::<super::StoredDocument>(&raw_after).is_err());

        let count_again = col.migrate_doc_keys_v1().await.unwrap();
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

        col.insert(
            "d1",
            &[1.0, 0.0, 0.0, 0.0],
            Some(serde_json::json!({"text": "rust language"})),
        )
        .await
        .unwrap();
        col.insert(
            "d2",
            &[0.9, 0.1, 0.0, 0.0],
            Some(serde_json::json!({"text": "python language"})),
        )
        .await
        .unwrap();

        let res = col
            .hybrid_search_reranked("rust", &[1.0, 0.0, 0.0, 0.0], 1, None, None)
            .await
            .unwrap();

        assert_eq!(res.len(), 1);
        assert_eq!(res[0].id, "d1");
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

    #[test]
    fn test_importance_metadata_integration_and_filtering() {
        use memfuse_core::{DecayFunction, ImportanceScore, MemoryImportance, TxId};
        use serde_json::json;

        let created_tx = TxId::new(10);
        let now_tx = TxId::new(30);

        let mut meta1 = Some(json!({"text": "Important factual doc"}));
        super::ensure_importance_metadata(&mut meta1, created_tx, Some("Important factual doc"));

        let imp1 = MemoryImportance::new(
            ImportanceScore::new(0.9),
            DecayFunction::Exponential { half_life_tx: 10 },
            created_tx,
        );
        meta1.as_mut().unwrap().as_object_mut().unwrap().insert(
            "importance".to_string(),
            serde_json::to_value(&imp1).unwrap(),
        );

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

        let filtered = super::Collection::<memfuse_store::LsmStorage>::filter_by_importance(
            results, 0.5, now_tx,
        );
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, "doc2");
        assert_eq!(filtered[0].score, 0.85);
    }
}
