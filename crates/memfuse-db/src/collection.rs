// ANCHOR:ARCH:COLLECTION-001 — Logische Isolation (Namespaces).
// DESIGN: Eigener HNSW-Index pro Collection, GEMEINSAMER LSM-Storage.
// PREFIXING: Jeder Key im LSM bekommt das Prefix `__col:{name}:`.
// STATUS: Dies ist die Teilimplementierung für WP-1.2.
//! Logically isolated Collections inside the MemFuse database.

use memfuse_core::{DocId, Result, StorageEngine, TxId, VectorIndex};
use memfuse_index::HnswIndex;
use memfuse_store::LsmStorage;
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

/// A logically isolated collection of documents.
/// Each collection has its own HNSW vector index but shares the underlying LSM-Tree.
#[derive(Clone)]
pub struct Collection {
    name: String,
    prefix: Vec<u8>,
    index: Arc<HnswIndex>,
    storage: Arc<LsmStorage>,
    next_tx: Arc<AtomicU64>,
    dimension: usize,
}

impl Collection {
    pub fn new(
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
            index,
            storage,
            next_tx,
            dimension,
        }
    }

    /// Internal helper to generate namespaced keys.
    /// key_type: 0 = user key, 1 = docid mapping, 2 = relationship
    fn namespaced_key(&self, key: &[u8], key_type: u8) -> Vec<u8> {
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

    /// Generates a new transaction ID.
    fn next_tx(&self) -> TxId {
        TxId::new(self.next_tx.fetch_add(1, Ordering::SeqCst))
    }

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

        let doc_id = DocId::from_key(id);
        let stored = StoredDocument {
            id: id.to_string(),
            embedding: embedding.to_vec(),
            metadata,
        };
        let data = serde_json::to_vec(&stored)?;

        let tx = self.next_tx();
        let user_key = self.namespaced_key(id.as_bytes(), 0);
        let doc_key = self.namespaced_key(&doc_id.inner().to_le_bytes(), 1);

        self.storage.put(tx, &user_key, &data).await?;
        self.storage.put(tx, &doc_key, &data).await?;
        self.index.insert(tx, doc_id, embedding).await?;

        self.storage.commit(tx).await?;
        self.index.commit(tx).await?;

        Ok(())
    }

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

    pub async fn update(
        &self,
        id: &str,
        embedding: &[f32],
        metadata: Option<serde_json::Value>,
    ) -> Result<()> {
        // Simple update: delete if exists, then insert
        self.delete(id).await?;
        self.insert(id, embedding, metadata).await
    }

    pub async fn delete(&self, id: &str) -> Result<()> {
        let user_key = self.namespaced_key(id.as_bytes(), 0);
        if let Some(data) = self.storage.get(&user_key).await? {
            let stored: StoredDocument = serde_json::from_slice(&data)?;
            let doc_id = DocId::from_key(&stored.id);
            let doc_key = self.namespaced_key(&doc_id.inner().to_le_bytes(), 1);

            let tx = self.next_tx();
            self.storage.delete(tx, &user_key).await?;
            self.storage.delete(tx, &doc_key).await?;
            self.index.delete(tx, doc_id).await?;

            self.storage.commit(tx).await?;
            self.index.commit(tx).await?;
        }
        Ok(())
    }

    pub async fn search(
        &self,
        query_embedding: &[f32],
        k: usize,
    ) -> Result<Vec<crate::SearchResult>> {
        self.search_filtered(query_embedding, k, None).await
    }

    pub async fn search_filtered(
        &self,
        query: &[f32],
        k: usize,
        filter: Option<&(dyn Fn(DocId) -> bool + Send + Sync)>,
    ) -> Result<Vec<crate::SearchResult>> {
        let scored = self.index.search_filtered(query, k, filter).await?;
        let mut results = Vec::new();

        for s in scored {
            let doc_key = self.namespaced_key(&s.doc_id.inner().to_le_bytes(), 1);
            if let Some(data) = self.storage.get(&doc_key).await? {
                let stored: StoredDocument = serde_json::from_slice(&data)?;
                results.push(crate::SearchResult {
                    id: stored.id,
                    score: s.score,
                    metadata: stored.metadata,
                });
            }
        }

        Ok(results)
    }

    pub async fn relate(&self, from: &str, to: &str, label: &str) -> Result<()> {
        let tx = self.next_tx();
        let key_str = format!("{}:{}:{}", from, label, to);
        let key = self.namespaced_key(key_str.as_bytes(), 2);
        let val = serde_json::json!({
            "from": from,
            "to": to,
            "label": label
        });
        let data = serde_json::to_vec(&val)?;
        self.storage.put(tx, &key, &data).await?;
        self.storage.commit(tx).await?;
        Ok(())
    }

    pub async fn scan_prefix(&self, prefix: &str) -> Result<Vec<(String, serde_json::Value)>> {
        let real_prefix = if prefix.starts_with("__rel:") {
            self.namespaced_key(
                prefix.strip_prefix("__rel:").unwrap_or(prefix).as_bytes(),
                2,
            )
        } else {
            // Fallback or generic scan
            self.namespaced_key(prefix.as_bytes(), 0)
        };

        let raw = self.storage.scan_prefix(&real_prefix).await?;
        let mut results = Vec::new();
        for (k, v) in raw {
            let key_str = String::from_utf8_lossy(&k).to_string();
            let val: serde_json::Value = serde_json::from_slice(&v)?;
            results.push((key_str, val));
        }
        Ok(results)
    }

    pub async fn len(&self) -> usize {
        self.index.len().await
    }

    pub async fn is_empty(&self) -> bool {
        self.index.is_empty().await
    }

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
                    // Everything in this collection's user-key space (type 0)
                    // The next namespace would be type 1
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

        let raw = self.storage.scan(start_bytes, end_bytes).await?;
        let mut results = Vec::new();
        for (k, v) in raw {
            let key_str = String::from_utf8_lossy(&k).to_string();
            let val: serde_json::Value = serde_json::from_slice(&v)?;
            results.push((key_str, val));
        }
        Ok(results)
    }

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
        let tx = self.next_tx();
        for (_, v) in entries {
            let stored: StoredDocument = serde_json::from_slice(&v)?;
            let doc_id = DocId::from_key(&stored.id);
            self.index.insert(tx, doc_id, &stored.embedding).await?;
        }
        self.index.commit(tx).await?;
        Ok(())
    }

    pub async fn drop_collection(&self) -> Result<()> {
        let prefix = if self.name == "default" {
            // We don't drop default usually, but if requested:
            // Need to be careful. SPEC says drop_collection(name) drops ALL data.
            // Default doesn't have a common prefix for ALL data (user keys are as-is).
            return Err(memfuse_core::MemFuseError::invalid_input(
                "Cannot drop default collection",
            ));
        } else {
            self.prefix.clone()
        };

        // Scan ALL data with this prefix and delete
        let entries = self.storage.scan_prefix(&prefix).await?;
        let tx = self.next_tx();
        for (k, _) in entries {
            self.storage.delete(tx, &k).await?;
        }
        self.storage.commit(tx).await?;

        // HNSW index is in-memory and will be dropped when the Collection is dropped.
        Ok(())
    }
}
