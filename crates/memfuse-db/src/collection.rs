//! Collection (Namespace) support for multi-tenancy.
//!
//! A Collection provides logical isolation within a single MemFuse instance.
//! Each collection has:
//! - Its own HNSW vector index (can have different dimensions/metrics)
//! - A unique key prefix in the shared LSM-Tree storage (`__col:{name}:`)

use crate::{Document, SearchResult};
use memfuse_core::{DocId, Result, StorageEngine, TxId, VectorIndex};
use memfuse_index::HnswIndex;
use memfuse_store::LsmStorage;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// A logically isolated collection of documents.
#[derive(Clone)]
pub struct Collection {
    name: String,
    prefix: Vec<u8>,
    storage: Arc<LsmStorage>,
    index: Arc<HnswIndex>,
    next_tx: Arc<AtomicU64>,
    dimension: usize,
}

impl Collection {
    /// Creates a new Collection proxy.
    pub(crate) fn new(
        name: String,
        storage: Arc<LsmStorage>,
        index: Arc<HnswIndex>,
        next_tx: Arc<AtomicU64>,
        dimension: usize,
    ) -> Self {
        let prefix = format!("__col:{}:", name).into_bytes();
        Self {
            name,
            prefix,
            storage,
            index,
            next_tx,
            dimension,
        }
    }

    /// Returns the name of the collection.
    pub fn name(&self) -> &str {
        &self.name
    }

    fn next_tx_id(&self) -> TxId {
        TxId::new(self.next_tx.fetch_add(1, Ordering::SeqCst))
    }

    /// Helper to apply the namespace prefix to a user key.
    #[inline]
    fn prefix_key(&self, key: &str) -> Vec<u8> {
        let mut prefixed = Vec::with_capacity(self.prefix.len() + key.len());
        prefixed.extend_from_slice(&self.prefix);
        prefixed.extend_from_slice(key.as_bytes());
        prefixed
    }

    /// Helper to apply the namespace prefix to a reverse-lookup docid.
    #[inline]
    fn prefix_docid(&self, doc_id: DocId) -> Vec<u8> {
        let mut prefixed = Vec::with_capacity(self.prefix.len() + 32);
        prefixed.extend_from_slice(&self.prefix);
        prefixed.extend_from_slice(format!("__docid:{}", doc_id.inner()).as_bytes());
        prefixed
    }

    /// Helper to strip the prefix from a key. Returns None if it doesn't match.
    #[inline]
    fn strip_prefix<'a>(&self, prefixed_key: &'a [u8]) -> Option<&'a [u8]> {
        prefixed_key.strip_prefix(self.prefix.as_slice())
    }

    /// Inserts a document with an embedding and optional metadata.
    pub async fn insert(&self, id: &str, embedding: &[f32], metadata: Option<Value>) -> Result<()> {
        if embedding.len() != self.dimension {
            return Err(memfuse_core::MemFuseError::invalid_input(format!(
                "Dimension mismatch: expected {}, got {}",
                self.dimension,
                embedding.len()
            )));
        }

        let doc_id = {
            let hash = blake3::hash(id.as_bytes());
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(&hash.as_bytes()[..8]);
            DocId::new(u64::from_le_bytes(bytes))
        };

        let tx = self.next_tx_id();

        // 1. Insert into vector index
        self.index.insert(tx, doc_id, embedding).await?;

        // 2. Serialize user record
        let record = serde_json::to_vec(&Document {
            id: id.to_string(),
            metadata,
        })
        .map_err(|e| memfuse_core::MemFuseError::Storage(e.to_string()))?;

        // 3. Insert into storage with namespace prefix
        let db_key = self.prefix_key(id);
        let docid_key = self.prefix_docid(doc_id);

        self.storage.put(tx, &db_key, &record).await?;
        self.storage.put(tx, &docid_key, &record).await?; // Reverse lookup
        self.storage.commit(tx).await?;
        self.index.commit(tx).await?;

        Ok(())
    }

    /// Performs semantic search to find the top-K nearest neighbors.
    pub async fn search(&self, query: &[f32], k: usize) -> Result<Vec<SearchResult>> {
        if query.len() != self.dimension {
            return Err(memfuse_core::MemFuseError::invalid_input(format!(
                "Dimension mismatch: expected {}, got {}",
                self.dimension,
                query.len()
            )));
        }

        let neighbors = self.index.search(query, k).await?;

        let mut results = Vec::with_capacity(neighbors.len());
        for neighbor in neighbors {
            let docid_key = self.prefix_docid(neighbor.doc_id);
            if let Some(record_bytes) = self.storage.get(&docid_key).await?
                && let Ok(doc) = serde_json::from_slice::<Document>(&record_bytes)
            {
                results.push(SearchResult {
                    id: doc.id,
                    score: neighbor.score,
                    metadata: doc.metadata,
                });
            }
        }

        Ok(results)
    }

    /// Performs semantic k-NN search with an optional filter function over documents.
    pub async fn search_filtered(
        &self,
        query: &[f32],
        k: usize,
        filter: Option<&(dyn Fn(DocId) -> bool + Send + Sync)>,
    ) -> Result<Vec<SearchResult>> {
        if query.len() != self.dimension {
            return Err(memfuse_core::MemFuseError::invalid_input(format!(
                "Dimension mismatch: expected {}, got {}",
                self.dimension,
                query.len()
            )));
        }

        let neighbors = self.index.search_filtered(query, k, filter).await?;

        let mut results = Vec::with_capacity(neighbors.len());
        for neighbor in neighbors {
            let docid_key = self.prefix_docid(neighbor.doc_id);
            if let Some(record_bytes) = self.storage.get(&docid_key).await?
                && let Ok(doc) = serde_json::from_slice::<Document>(&record_bytes)
            {
                results.push(SearchResult {
                    id: doc.id,
                    score: neighbor.score,
                    metadata: doc.metadata,
                });
            }
        }

        Ok(results)
    }

    /// Retrieves a document by its exact string ID.
    pub async fn get(&self, id: &str) -> Result<Option<Document>> {
        let db_key = self.prefix_key(id);
        if let Some(record_bytes) = self.storage.get(&db_key).await? {
            let doc = serde_json::from_slice::<Document>(&record_bytes)
                .map_err(|e| memfuse_core::MemFuseError::Storage(e.to_string()))?;
            Ok(Some(doc))
        } else {
            Ok(None)
        }
    }

    /// Updates a document's embedding and/or metadata.
    pub async fn update(&self, id: &str, embedding: &[f32], metadata: Option<Value>) -> Result<()> {
        self.delete(id).await?;
        self.insert(id, embedding, metadata).await?;
        Ok(())
    }

    /// Deletes a document by its string ID.
    pub async fn delete(&self, id: &str) -> Result<()> {
        let db_key = self.prefix_key(id);

        // Find doc_id first so we can remove from vector index
        if self.storage.get(&db_key).await?.is_some() {
            let doc_id = {
                let hash = blake3::hash(id.as_bytes());
                let mut bytes = [0u8; 8];
                bytes.copy_from_slice(&hash.as_bytes()[..8]);
                DocId::new(u64::from_le_bytes(bytes))
            };

            let tx = self.next_tx_id();

            // 1. Delete from vector index
            self.index.delete(tx, doc_id).await?;

            // 2. Delete from storage
            let docid_key = self.prefix_docid(doc_id);
            self.storage.delete(tx, &db_key).await?;
            self.storage.delete(tx, &docid_key).await?;
            self.storage.commit(tx).await?;
            self.index.commit(tx).await?;
        }

        Ok(())
    }

    /// Establishes a directional relationship between two documents.
    pub async fn relate(&self, from_id: &str, to_id: &str, edge_type: &str) -> Result<()> {
        let tx = self.next_tx_id();

        let value = serde_json::json!({
            "from": from_id,
            "to": to_id,
            "label": edge_type,
        });

        let bytes = serde_json::to_vec(&value)
            .map_err(|e| memfuse_core::MemFuseError::Serialization(e.to_string()))?;

        let rel_key = format!("__rel:{}:{}:{}", from_id, edge_type, to_id);
        let prefixed_rel_key = self.prefix_key(&rel_key);

        self.storage.put(tx, &prefixed_rel_key, &bytes).await?;
        self.storage.commit(tx).await?;

        Ok(())
    }

    /// Scans for all documents starting with a given prefix.
    pub async fn scan_prefix(&self, prefix: &str) -> Result<Vec<(String, Value)>> {
        let prefixed = self.prefix_key(prefix);
        let entries = self.storage.scan_prefix(&prefixed).await?;

        let mut results = Vec::with_capacity(entries.len());
        for (k_bytes, value) in entries {
            if let Some(unprefixed) = self.strip_prefix(&k_bytes)
                && let Ok(k_str) = String::from_utf8(unprefixed.to_vec())
            {
                let val_json: Value = serde_json::from_slice(&value)
                    .unwrap_or(Value::String(String::from_utf8_lossy(&value).to_string()));
                results.push((k_str, val_json));
            }
        }

        Ok(results)
    }

    /// Scans a range of keys, returning key-value pairs.
    pub async fn scan(
        &self,
        start_key: std::ops::Bound<&[u8]>,
        end_key: std::ops::Bound<&[u8]>,
    ) -> Result<Vec<(String, Value)>> {
        use std::ops::Bound;

        let entries = self.storage.scan_prefix(&self.prefix).await?;

        let mut results = Vec::new();
        for (k_bytes, v_bytes) in entries {
            if let Some(unprefixed) = self.strip_prefix(&k_bytes) {
                let in_range = match start_key {
                    Bound::Included(s) => unprefixed >= s,
                    Bound::Excluded(s) => unprefixed > s,
                    Bound::Unbounded => true,
                } && match end_key {
                    Bound::Included(e) => unprefixed <= e,
                    Bound::Excluded(e) => unprefixed < e,
                    Bound::Unbounded => true,
                };

                if in_range && let Ok(key) = String::from_utf8(unprefixed.to_vec()) {
                    let value: Value = serde_json::from_slice(&v_bytes)
                        .unwrap_or(Value::String(String::from_utf8_lossy(&v_bytes).to_string()));
                    results.push((key, value));
                }
            }
        }
        Ok(results)
    }

    /// Returns the number of documents in the collection.
    pub async fn len(&self) -> usize {
        self.index.len().await
    }

    /// Returns true if the collection is empty.
    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }

    /// Returns index statistics.
    pub async fn stats(&self) -> Result<memfuse_core::VectorIndexStats> {
        self.index.stats().await
    }

    /// Drops the entire collection, deleting all its data from the storage engine.
    /// The HNSW index will be dropped when the Collection struct is dropped.
    pub async fn drop_collection(&self) -> Result<()> {
        let tx = self.next_tx_id();
        let entries = self.storage.scan_prefix(&self.prefix).await?;

        for (key, _) in entries {
            self.storage.delete(tx, &key).await?;
        }

        self.storage.commit(tx).await?;
        Ok(())
    }
}
