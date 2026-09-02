// FILE-CONTEXT
// ZWECK: Sammlung/Collection-Namespace Verwaltung und gemeinsame Hilfsfunktionen.
// INVARIANTEN: Strikte Isolation durch Präfixe; doc_keys (key_type=1) halten nur Metadaten (keine Vektoren).
// NICHT-OFFENSICHTLICH: insert_lock schützt Mutationen zur Vermeidung von TOCTOU-Kollisionsrassen.
// STAND: TS:2026-08-29T17:22:29Z (SESSION: 0dcb9f3b)

//! Logically isolated Collections inside the MemFuse database.
// INVARIANT: Logische Isolation (Namespaces).
// PREFIXING: Jeder Key im LSM bekommt das Prefix `__col:{name}:\x00`.

pub mod crud;
pub mod maintenance;
pub mod query_builder;
pub mod relate;
pub mod search;
pub mod tx;

#[cfg(test)]
mod tests;

use memfuse_core::{DocId, Result, StorageEngine, TextEmbeddingEngine, TxId, VectorIndex};
use memfuse_graph::CsrGraph;
use memfuse_index::HnswIndex;
use memfuse_store::LsmStorage;
use memfuse_text::inverted::InvertedIndex;
use memfuse_text::Language;
use serde::{Deserialize, Serialize};
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

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

    // Determine default decay function based on explicit decay_function or memory_type
    let decay = if let Some(decay_val) = meta_obj.get("decay_function") {
        serde_json::from_value::<memfuse_core::DecayFunction>(decay_val.clone())
            .unwrap_or(memfuse_core::DecayFunction::None)
    } else if let Some(mem_type_val) = meta_obj.get("memory_type") {
        if let Ok(mem_type) =
            serde_json::from_value::<memfuse_core::MemoryType>(mem_type_val.clone())
        {
            mem_type.default_decay()
        } else {
            memfuse_core::DecayFunction::None
        }
    } else {
        memfuse_core::DecayFunction::None
    };

    // Also populate default TTL if memory_type defines one and not already set
    if !meta_obj.contains_key("ttl_tx") {
        if let Some(mem_type_val) = meta_obj.get("memory_type") {
            if let Ok(mem_type) =
                serde_json::from_value::<memfuse_core::MemoryType>(mem_type_val.clone())
            {
                if let Some(ttl) = mem_type.default_ttl_tx() {
                    meta_obj.insert("ttl_tx".to_string(), serde_json::json!(ttl));
                }
            }
        }
    }

    if let Some(imp_val) = meta_obj.get("importance").cloned() {
        if serde_json::from_value::<memfuse_core::MemoryImportance>(imp_val.clone()).is_ok() {
            return;
        } else if let Some(raw_f64) = imp_val.as_f64() {
            let imp = memfuse_core::MemoryImportance::new(
                memfuse_core::ImportanceScore::new(raw_f64 as f32),
                decay,
                tx,
            );
            if let Ok(val) = serde_json::to_value(imp) {
                meta_obj.insert("importance".to_string(), val);
            }
            return;
        }
    }

    let base_score = compute_default_importance(text_opt);
    let imp = memfuse_core::MemoryImportance::new(base_score, decay, tx);
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
pub(super) fn extract_text(metadata: &Option<serde_json::Value>) -> Option<String> {
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

/// # Concurrency & Lock Hierarchy
///
/// Lock acquisition within `Collection` follows strict ordering to prevent deadlocks:
///
/// 1. `Collection::insert_lock` (`tokio::sync::Mutex`):
///    Serializes mutations (`insert`, `update`, `delete`, `relate`, `repair`, `drop_collection`) and prevents
///    TOCTOU races during `check_doc_id_collision`.
/// 2. `Collection::embedder` (`parking_lot::RwLock`):
///    Read/write lock for the configured `TextEmbeddingEngine`. Never acquired before `insert_lock` if both are needed.
///
/// A logically isolated collection of documents (namespace).
///
/// Each collection provides its own vector index and inverted text index,
/// while sharing the underlying LSM-Tree storage with other collections.
pub struct Collection<S: StorageEngine = LsmStorage, V: VectorIndex = HnswIndex> {
    pub(super) name: String,
    pub(super) prefix: Vec<u8>,
    pub(super) index: Arc<V>,
    pub(super) text_index: InvertedIndex<S>,
    pub(super) graph_index: Arc<CsrGraph>,
    pub(super) storage: Arc<S>,
    pub(super) next_tx: Arc<AtomicU64>,
    pub(super) dimension: usize,
    pub(super) embedder: parking_lot::RwLock<Option<Arc<dyn TextEmbeddingEngine>>>,
    pub(super) insert_lock: Arc<tokio::sync::Mutex<()>>,
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
    pub(super) fn namespaced_key(&self, key: &[u8], key_type: u8) -> Vec<u8> {
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
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn load_index(&self) -> Result<()> {
        // AI-TAG[CONVENTION-DRIFT][MAJOR] RESOLVED: AGT-DB-002 — load_index now scans user_keys (key_type=0) (TS:2026-08-25T00:00:00Z)
        // because doc_keys (key_type=1) no longer contain embeddings (ID: AGT-DB-002).
        let scan_prefix = if self.name == "default" {
            b"".to_vec()
        } else {
            let mut p = self.prefix.clone();
            p.push(0); // key_type=0
            p
        };

        let entries = self.storage.scan_prefix(&scan_prefix).await?;
        let tx = self.allocate_tx()?;
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
        let tx = self.allocate_tx()?;

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
}
