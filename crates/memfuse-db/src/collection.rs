//! Logically isolated Collections inside the MemFuse database.
// ANCHOR:ARCH:COLLECTION-001 — Logische Isolation (Namespaces).
// WP:WP-1.2 PRIO:1 NEEDS:NONE
// AGENT:04 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-05 DEADLINE:NONE
// DESIGN: Eigener HNSW-Index pro Collection, GEMEINSAMER LSM-Storage.
// PREFIXING: Jeder Key im LSM bekommt das Prefix `__col:{name}:\x00`.
// STATUS: Full Implementation für WP-1.2.

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
        let tx = db_tx.tx_id;
        let doc_id = DocId::from_key(id);

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

        db_tx.commit().await?;

        Ok(())
    }

    /// Retrieves a document by its user-provided string ID.
    pub async fn get(&self, id: &str) -> Result<Option<crate::Document>> {
        let key = self.namespaced_key(id.as_bytes(), 0);
        if let Some(data) = self.storage.get(&key).await? {
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
        let tx = db_tx.tx_id;
        let doc_id = DocId::from_key(id);

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

        db_tx.commit().await?;

        Ok(())
    }

    /// Deletes a document from the collection by its ID.
    pub async fn delete(&self, id: &str) -> Result<()> {
        let db_tx = self.begin_transaction();
        let tx = db_tx.tx_id;
        let doc_id = DocId::from_key(id);

        let user_key = self.namespaced_key(id.as_bytes(), 0);

        // Remove from old text index
        self.text_index.delete_document(tx, doc_id).await?;

        let doc_key = self.namespaced_key(&doc_id.inner().to_le_bytes(), 1);

        self.storage.delete(tx, &user_key).await?;
        self.storage.delete(tx, &doc_key).await?;

        db_tx.record_keys(user_key, doc_key, doc_id);

        let _ = self.index.delete(tx, doc_id).await;

        db_tx.commit().await?;

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
        self.search_filtered(query_embedding, k, None).await
    }

    /// Performs filtered semantic vector search in the collection.
    /// Supports both closure-based filters and structured `FilterExpr`.
    pub async fn search_filtered(
        &self,
        query: &[f32],
        k: usize,
        filter: Option<&(dyn Fn(DocId) -> bool + Send + Sync)>,
    ) -> Result<Vec<crate::SearchResult>> {
        // If we have a complex filter, we might want to pre-filter or post-filter
        let scored_docs = self.index.search_filtered(query, k, filter).await?;
        self.hydrate_from_scored(scored_docs).await
    }

    /// Performs vector search with a structured metadata filter.
    /// Automatically decides between Pre-filtering and Post-filtering based on selectivity.
    pub async fn search_with_filter(
        &self,
        query: &[f32],
        k: usize,
        filter_expr: crate::FilterExpr,
    ) -> Result<Vec<crate::SearchResult>> {
        // 1. Estimation (Pseudo-selectivity for now)
        // In a real system, we would use histograms or sample the storage.
        // For WP-4.2, we implement the logic for both paths.

        let total_docs = self.len().await;

        // Threshold: if we expect < 10% of docs to match, pre-filter might be better
        // but HNSW pre-filtering is only efficient if we have a small candidate set.
        // If candidates are very few, we just brute-force search the filtered set.

        // Path A: Post-filtering (Default for HNSW)
        // We use the HNSW's internal filtered search which does post-filtering (or filtered traversal).
        // Since we need to access metadata for evaluation, we need a way to pass the filter to the index.

        // let _storage = Arc::clone(&self.storage);
        // let _filter_expr_clone = filter_expr.clone();
        // let _collection = self.clone();

        // let _filter_fn = move |doc_id: DocId| -> bool {
        //     let _doc_key = _collection.namespaced_key(&doc_id.inner().to_le_bytes(), 1);
        // Synchronous block inside HNSW search might be problematic if storage is async
        // but HNSW search_filtered takes a Fn(DocId) -> bool.
        // THIS IS A KNOWN ARCHITECTURAL CHALLENGE: HNSW filter closure is sync, Storage is async.
        // For now, we might need to use a pre-filtered list or a sync cache.

        // Hack for WP-4.2: We pre-calculate the matching DocIds if the collection is small enough.
        // Or we perform the search and filter results after (true post-filtering).
        //     true // Placeholder
        // };

        // Real Implementation for WP-4.2 selectivity:
        // If it's a small match set, we find all DocIds and then search them.

        // For now, let's implement the "Post-filtering" by searching more and then filtering.
        // And "Pre-filtering" by scanning and then brute-force.

        if total_docs < 1000 {
            // Brute force pre-filter path
            let mut matching_ids = Vec::new();
            let prefix = if self.name == "default" {
                b"__docid:".to_vec()
            } else {
                [self.prefix.as_slice(), &[1]].concat()
            };
            let entries = self.storage.scan_prefix(&prefix).await?;
            for (_, v) in entries {
                let stored: StoredDocument = serde_json::from_slice(&v)?;
                if let Some(meta) = &stored.metadata {
                    if filter_expr.matches(meta) {
                        matching_ids.push((DocId::from_key(&stored.id), stored.embedding));
                    }
                }
            }

            // Brute force distance calculation
            let mut results = Vec::new();
            for (doc_id, emb) in matching_ids {
                let score = memfuse_index::distance::cosine_distance(&emb, query);
                results.push((doc_id, score));
            }
            results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            results.truncate(k);
            return self.hydrate_from_tuples(results).await;
        }

        // Default to index search with post-filter if large
        let results = self.search(query, k * 5).await?; // Search more to account for filtering
        let filtered: Vec<crate::SearchResult> = results
            .into_iter()
            .filter(|r| {
                r.metadata
                    .as_ref()
                    .map(|m| filter_expr.matches(m))
                    .unwrap_or(false)
            })
            .take(k)
            .collect();

        Ok(filtered)
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
        let query = crate::HybridQuery {
            vector: Some(vector.to_vec()),
            text: Some(text.to_string()),
            graph_seed: None,
            metadata_filter: None,
            weights: None,
            limit: k,
        };
        self.unified_search(query).await
    }

    /// Unified 4-Signal Fusion API (WP-6.1).
    pub async fn unified_search(
        &self,
        query: crate::HybridQuery,
    ) -> Result<Vec<crate::SearchResult>> {
        let mut result_sets = Vec::new();
        let mut weights = Vec::new();
        let k = query.limit;
        let query_weights = query.weights.unwrap_or_default();

        // 1. Vector Signal
        if let Some(vec) = query.vector {
            if !vec.iter().all(|&v| v == 0.0) {
                let vector_results = self.search(&vec, k).await?;
                result_sets.push(vector_results);
                weights.push(query_weights.vector);
            }
        }

        // 2. Text Signal (BM25)
        if let Some(text) = query.text {
            if !text.trim().is_empty() {
                let bm25_results = self.text_index.search_bm25(&text, k).await?;
                let text_results = self.hydrate_from_tuples(bm25_results).await?;
                result_sets.push(text_results);
                weights.push(query_weights.text);
            }
        }

        // 3. Metadata Signal (Filtering)
        if let Some(ref filter) = query.metadata_filter {
            // We can treat the filter as a signal by giving it a constant score for matches
            // or just using it to prune other results.
            // For true 4-signal fusion as per GS-01, we should probably integrate it into RRF.
            // If we only have a filter, we return matching docs.
            if result_sets.is_empty() {
                // Brute force scan for matching docs (signal only)
                // In a real system, this would be an index scan.
                let mut results = Vec::new();
                let prefix = if self.name == "default" {
                    b"__docid:".to_vec()
                } else {
                    [self.prefix.as_slice(), &[1]].concat()
                };
                let entries = self.storage.scan_prefix(&prefix).await?;
                for (_, v) in entries {
                    let stored: StoredDocument = serde_json::from_slice(&v)?;
                    if let Some(meta) = &stored.metadata {
                        if filter.matches(meta) {
                            results.push(crate::SearchResult {
                                id: stored.id,
                                score: 1.0, // Constant score for filter matches
                                metadata: stored.metadata,
                            });
                        }
                    }
                    if results.len() >= k {
                        break;
                    }
                }
                result_sets.push(results);
                weights.push(query_weights.metadata);
            }
            // If result_sets is NOT empty, we currently use filter as a post-filter for fusion
            // but the spec says "Native Verschmelzung aller vier Retrieval-Signale".
        }

        // 4. Graph Signal (Stub for WP-6.1)
        if let Some((_seed, _hops)) = query.graph_seed {
            // CSR-Graph retrieval not yet fully integrated in this layer
            // result_sets.push(graph_results);
            // weights.push(query_weights.graph);
        }

        if result_sets.is_empty() {
            return Ok(Vec::new());
        }

        if result_sets.len() == 1 {
            let mut res = result_sets.remove(0);
            res.truncate(k);
            return Ok(res);
        }

        let fused = crate::fusion::weighted_reciprocal_rank_fusion(result_sets, weights, k);

        // Final post-filter if metadata_filter was provided and we had other signals
        if let Some(ref filter) = query.metadata_filter {
            let filtered: Vec<crate::SearchResult> = fused
                .into_iter()
                .filter(|r| {
                    r.metadata
                        .as_ref()
                        .map(|m| filter.matches(m))
                        .unwrap_or(false)
                })
                .collect();
            return Ok(filtered);
        }

        Ok(fused)
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
            let doc_id = DocId::from_key(&stored.id);
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
