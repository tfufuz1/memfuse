//! Logically isolated Collections inside the MemFuse database.
// ANCHOR:ARCH:COLLECTION-001 — Logische Isolation (Namespaces).
// WP:WP-1.2 PRIO:1 NEEDS:NONE
// AGENT:04 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-05 DEADLINE:NONE
// DESIGN: Eigener HNSW-Index pro Collection, GEMEINSAMER LSM-Storage.
// PREFIXING: Jeder Key im LSM bekommt das Prefix `__col:{name}:\x00`.
// STATUS: Full Implementation für WP-1.2.

use crate::filter::MetadataFilter;
use memfuse_core::{DocId, Result, StorageEngine, TxId, VectorIndex};
use memfuse_index::HnswIndex;
use memfuse_store::LsmStorage;
use memfuse_text::inverted::InvertedIndex;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Internal document structure for persistence.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct StoredDocument {
    pub id: String,
    pub embedding: Vec<f32>,
    pub metadata: Option<serde_json::Value>,
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
#[derive(Clone)]
pub struct Collection {
    pub(crate) name: String,
    pub(crate) prefix: Vec<u8>,
    pub(crate) index: Arc<HnswIndex>,
    pub(crate) text_index: InvertedIndex,
    pub(crate) storage: Arc<LsmStorage>,
    pub(crate) next_tx: Arc<AtomicU64>,
    pub(crate) dimension: usize,
}

impl Collection {
    /// Creates a new `Collection` instance.
    pub fn new(
        name: String,
        storage: Arc<LsmStorage>,
        index: Arc<HnswIndex>,
        next_tx: Arc<AtomicU64>,
        dimension: usize,
    ) -> Self {
        let prefix = if name == "default" {
            b"".to_vec()
        } else {
            format!("__col:{}:\x00", name).into_bytes()
        };

        let text_index = InvertedIndex::new(storage.clone(), &name);

        Self {
            name,
            prefix,
            index,
            text_index,
            storage,
            next_tx,
            dimension,
        }
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
    pub async fn repair(&self) -> Result<()> {
        tracing::info!("Starting integrity repair for collection '{}'", self.name);
        let start_time = std::time::Instant::now();

        // 1. Scan storage for all documents in this collection
        let docs = self.storage.scan_prefix(&self.prefix).await?;
        let mut repair_count = 0;

        // 2. Cross-reference with index
        for (namespaced_key, value) in docs {
            // Only process user data (key_type 0)
            if self.name != "default" && namespaced_key.get(self.prefix.len()) != Some(&0) {
                continue;
            }
            // For default collection, we don't have a prefix, so check if it starts with internal prefixes
            if self.name == "default"
                && (namespaced_key.starts_with(b"__docid:")
                    || namespaced_key.starts_with(b"__rel:")
                    || namespaced_key.starts_with(b"__tx_intent:"))
            {
                continue;
            }

            let stored: StoredDocument = match serde_json::from_slice(&value) {
                Ok(d) => d,
                Err(_) => continue, // Skip invalid entries
            };

            let doc_id = DocId::from_key(&stored.id).unwrap_or_else(|_| DocId::new(0));

            // Check if present in index
            // We use k=1 search to check presence (if we find it with distance 0, it's there)
            let results = self.index.search(&stored.embedding, 1).await?;
            let found = results
                .iter()
                .any(|r| r.doc_id == doc_id && r.score > 0.9999);

            if !found {
                let tx = TxId::new(self.next_tx.fetch_add(1, Ordering::SeqCst));
                self.index.insert(tx, doc_id, &stored.embedding).await?;
                self.index.commit(tx).await?;
                repair_count += 1;
            }
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
    pub fn begin_transaction(&self) -> crate::transaction::DbTransaction<'_> {
        let tx = TxId::new(self.next_tx.fetch_add(1, Ordering::SeqCst));
        crate::transaction::DbTransaction::new(self, tx)
    }

    /// Inserts a document with an embedding and optional metadata.
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

    async fn insert_op(
        &self,
        db_tx: &crate::transaction::DbTransaction<'_>,
        id: &str,
        embedding: &[f32],
        metadata: Option<serde_json::Value>,
    ) -> Result<()> {
        let tx = db_tx.tx_id;
        let doc_id = DocId::from_key(id)?;

        let stored = StoredDocument {
            id: id.to_string(),
            embedding: embedding.to_vec(),
            metadata: metadata.clone(),
        };
        // ANCHOR:SEC:ENCRYPT-001 AGENT:10 PRIO:1 STATUS:REVIEW
        // Document serialization is unencrypted before being sent to storage.
        // If Encryption-at-Rest is enabled, it's encrypted in the storage layer (WP-3.2).
        let data = serde_json::to_vec(&stored)?;

        let user_key = self.namespaced_key(id.as_bytes(), 0);
        let doc_key = self.namespaced_key(&doc_id.inner().to_le_bytes(), 1);

        self.storage.put(tx, &user_key, &data).await?;
        self.storage.put(tx, &doc_key, &data).await?;

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
        let exists = {
            let key = self.namespaced_key(id.as_bytes(), 0);
            self.storage.get(&key).await?.is_some()
        };

        let result = if exists {
            self.update_op(&db_tx, id, embedding, metadata).await
        } else {
            self.insert_op(&db_tx, id, embedding, metadata).await
        };

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
            let exists = {
                let key = self.namespaced_key(id.as_bytes(), 0);
                self.storage.get(&key).await?.is_some()
            };
            let result = if exists {
                self.update_op(&db_tx, id, embedding, metadata.clone())
                    .await
            } else {
                self.insert_op(&db_tx, id, embedding, metadata.clone())
                    .await
            };
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

    /// Retrieves a document by its user-provided string ID.
    pub async fn get(&self, id: &str) -> Result<Option<crate::Document>> {
        self.get_at_snapshot(id, u64::MAX).await
    }

    /// Retrieves a document at a specific snapshot point.
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

    async fn update_op(
        &self,
        db_tx: &crate::transaction::DbTransaction<'_>,
        id: &str,
        embedding: &[f32],
        metadata: Option<serde_json::Value>,
    ) -> Result<()> {
        let tx = db_tx.tx_id;
        let doc_id = DocId::from_key(id)?;

        let user_key = self.namespaced_key(id.as_bytes(), 0);

        // Remove from old text index
        self.text_index.delete_document(tx, doc_id).await?;

        let stored = StoredDocument {
            id: id.to_string(),
            embedding: embedding.to_vec(),
            metadata: metadata.clone(),
        };
        // ANCHOR:SEC:ENCRYPT-001 AGENT:10 PRIO:1 STATUS:REVIEW
        let data = serde_json::to_vec(&stored)?;

        let doc_key = self.namespaced_key(&doc_id.inner().to_le_bytes(), 1);

        self.storage.put(tx, &user_key, &data).await?;
        self.storage.put(tx, &doc_key, &data).await?;

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
    pub async fn delete(&self, id: &str) -> Result<()> {
        let db_tx = self.begin_transaction();

        match self.delete_op(&db_tx, id).await {
            Ok(_) => db_tx.commit().await,
            Err(e) => {
                if let Err(rollback_err) = db_tx.rollback().await {
                    tracing::error!("[INV-DB-3] Failed to rollback delete: {}", rollback_err);
                }
                Err(e)
            }
        }
    }

    async fn delete_op(
        &self,
        db_tx: &crate::transaction::DbTransaction<'_>,
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
    pub async fn relate(&self, from: &str, to: &str, label: &str) -> Result<()> {
        let tx = TxId::new(self.next_tx.fetch_add(1, Ordering::SeqCst));
        let key_str = format!("{}:{}:{}", from, label, to);
        let key = self.namespaced_key(key_str.as_bytes(), 2);
        let val = serde_json::json!({
            "from": from,
            "to": to,
            "label": label,
        });
        // ANCHOR:SEC:ENCRYPT-001 AGENT:10 PRIO:1 STATUS:REVIEW
        let bytes = serde_json::to_vec(&val)?;

        self.storage.put(tx, &key, &bytes).await?;
        self.storage.commit(tx).await?;
        Ok(())
    }

    /// Creates a bidirectional relationship atomically.
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
    pub async fn search(
        &self,
        query_embedding: &[f32],
        k: usize,
    ) -> Result<Vec<crate::SearchResult>> {
        self.search_with_filter(query_embedding, k, None).await
    }

    /// Performs semantic search with an advanced metadata filter.
    pub async fn search_with_filter(
        &self,
        query: &[f32],
        k: usize,
        filter: Option<MetadataFilter>,
    ) -> Result<Vec<crate::SearchResult>> {
        let filter = match filter {
            Some(f) => f,
            None => return self.search_filtered(query, k, None).await,
        };

        let total_docs = self.len().await;

        // ADAPTIVE STRATEGY (WP-4.2):
        // If total documents are few, or if we suspect high selectivity,
        // we use Pre-filtering by scanning metadata first.
        // For now, we use a simple heuristic: if docs < 1000, always pre-filter.
        if total_docs < 1000 {
            let matched_ids = self.get_matching_doc_ids(&filter).await?;

            // If no docs match the filter, return early
            if matched_ids.is_empty() {
                return Ok(Vec::new());
            }

            let filter_fn = move |id: DocId| matched_ids.contains(&id);
            let scored_docs = self
                .index
                .search_filtered(query, k, Some(&filter_fn))
                .await?;
            self.hydrate_from_scored(scored_docs).await
        } else {
            // Post-filtering approach for larger collections:
            // 1. Search more than k (oversample) to account for filter drops.
            let oversample = (k * 10).min(total_docs).max(k);
            let scored_docs = self.index.search_filtered(query, oversample, None).await?;

            let mut results = Vec::new();
            for sd in scored_docs {
                let doc_key = self.namespaced_key(&sd.doc_id.inner().to_le_bytes(), 1);
                if let Some(bytes) = self.storage.get(&doc_key).await? {
                    let stored: StoredDocument = serde_json::from_slice(&bytes)?;
                    let metadata = stored.metadata.as_ref().unwrap_or(&serde_json::Value::Null);
                    if filter.matches(metadata) {
                        results.push(crate::SearchResult {
                            id: stored.id,
                            score: sd.score,
                            metadata: stored.metadata,
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

    /// Internal helper to find all DocIds matching a filter by scanning metadata.
    async fn get_matching_doc_ids(
        &self,
        filter: &MetadataFilter,
    ) -> Result<std::collections::HashSet<DocId>> {
        let prefix = if self.name == "default" {
            b"__docid:".to_vec()
        } else {
            let mut p = self.prefix.clone();
            p.push(1); // docid mapping type
            p
        };

        let entries = self.storage.scan_prefix(&prefix).await?;
        let mut matched = std::collections::HashSet::new();

        for (_, v) in entries {
            let stored: StoredDocument = serde_json::from_slice(&v)?;
            let metadata = stored.metadata.as_ref().unwrap_or(&serde_json::Value::Null);
            if filter.matches(metadata) {
                matched.insert(DocId::from_key(&stored.id)?);
            }
        }

        Ok(matched)
    }

    /// Performs filtered semantic vector search in the collection.
    pub async fn search_filtered(
        &self,
        query: &[f32],
        k: usize,
        filter: Option<&(dyn Fn(DocId) -> bool + Send + Sync)>,
    ) -> Result<Vec<crate::SearchResult>> {
        let scored_docs = self.index.search_filtered(query, k, filter).await?;
        self.hydrate_from_scored(scored_docs).await
    }

    async fn hydrate_from_scored(
        &self,
        scored_docs: Vec<memfuse_core::ScoredDocument>,
    ) -> Result<Vec<crate::SearchResult>> {
        if scored_docs.is_empty() {
            return Ok(Vec::new());
        }

        let mut results = Vec::with_capacity(scored_docs.len());
        for sd in scored_docs {
            let doc_key = self.namespaced_key(&sd.doc_id.inner().to_le_bytes(), 1);
            if let Some(bytes) = self.storage.get(&doc_key).await? {
                let stored: StoredDocument = serde_json::from_slice(&bytes)?;
                results.push(crate::SearchResult {
                    id: stored.id,
                    score: sd.score,
                    metadata: stored.metadata,
                });
            }
        }
        Ok(results)
    }

    async fn hydrate_from_tuples(
        &self,
        scored_tuples: Vec<(DocId, f32)>,
    ) -> Result<Vec<crate::SearchResult>> {
        if scored_tuples.is_empty() {
            return Ok(Vec::new());
        }

        let mut results = Vec::with_capacity(scored_tuples.len());
        for (doc_id, score) in scored_tuples {
            let doc_key = self.namespaced_key(&doc_id.inner().to_le_bytes(), 1);
            if let Some(bytes) = self.storage.get(&doc_key).await? {
                let stored: StoredDocument = serde_json::from_slice(&bytes)?;
                results.push(crate::SearchResult {
                    id: stored.id,
                    score,
                    metadata: stored.metadata,
                });
            }
        }
        Ok(results)
    }

    /// Performs hybrid search combining BM25 and vector search results via RRF.
    pub async fn hybrid_search(
        &self,
        text: &str,
        vector: &[f32],
        k: usize,
    ) -> Result<Vec<crate::SearchResult>> {
        let is_vector_zero = vector.iter().all(|&v| v == 0.0);
        let is_text_empty = text.trim().is_empty();

        match (is_text_empty, is_vector_zero) {
            (true, true) => Ok(Vec::new()),
            (true, false) => self.search(vector, k).await,
            (false, is_v_zero) => {
                let bm25_results = self.text_index.search_bm25(text, k).await?;
                let text_results = self.hydrate_from_tuples(bm25_results).await?;

                if is_v_zero {
                    return Ok(text_results);
                }

                let vector_results = self.search(vector, k).await?;
                Ok(crate::fusion::reciprocal_rank_fusion(
                    vec![vector_results, text_results],
                    k,
                ))
            }
        }
    }

    /// Returns the number of documents in the collection.
    pub async fn len(&self) -> usize {
        self.index.len().await
    }

    /// Returns true if the collection is empty.
    pub async fn is_empty(&self) -> bool {
        self.index.is_empty().await
    }

    /// Performs a range scan of documents in the collection.
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
    pub async fn stats(&self) -> Result<memfuse_core::VectorIndexStats> {
        self.index.stats().await
    }

    /// Rebuilds the HNSW index from storage.
    pub async fn load_index(&self) -> Result<()> {
        let prefix = if self.name == "default" {
            b"__docid:".to_vec()
        } else {
            let mut p = self.prefix.clone();
            p.push(1);
            p
        };

        let entries = self.storage.scan_prefix(&prefix).await?;
        let tx = TxId::new(self.next_tx.fetch_add(1, Ordering::SeqCst));
        for (_, v) in entries {
            let stored: StoredDocument = serde_json::from_slice(&v)?;
            let doc_id = DocId::from_key(&stored.id)?;
            self.index.insert(tx, doc_id, &stored.embedding).await?;
        }
        self.index.commit(tx).await?;
        Ok(())
    }

    /// Removes all data belonging to this collection from storage.
    pub async fn drop_collection(&self) -> Result<()> {
        let prefix = if self.name == "default" {
            return Err(memfuse_core::MemFuseError::invalid_input(
                "Cannot drop default collection",
            ));
        } else {
            self.prefix.clone()
        };

        let entries = self.storage.scan_prefix(&prefix).await?;
        let tx = TxId::new(self.next_tx.fetch_add(1, Ordering::SeqCst));
        for (k, _) in entries {
            self.storage.delete(tx, &k).await?;
        }
        self.storage.commit(tx).await?;
        Ok(())
    }
}
