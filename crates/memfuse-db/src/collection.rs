// ANCHOR:ARCH:COLLECTION-001 — Logische Isolation (Namespaces).
// WP:WP-1.2 PRIO:1 NEEDS:NONE
// AGENT:04 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-05 DEADLINE:NONE
// DESIGN: Eigener HNSW-Index pro Collection, GEMEINSAMER LSM-Storage.
// PREFIXING: Jeder Key im LSM bekommt das Prefix `__col:{name}:\x00`.
// STATUS: Full Implementation für WP-1.2.
//! Logically isolated Collections inside the MemFuse database.

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

/// A logically isolated collection of documents.
/// Each collection has its own HNSW vector index but shares the underlying LSM-Tree.
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

    pub fn begin_transaction(&self) -> crate::transaction::DbTransaction<'_> {
        let tx = TxId::new(self.next_tx.fetch_add(1, Ordering::SeqCst));
        crate::transaction::DbTransaction::new(self, tx)
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

        let db_tx = self.begin_transaction();
        let tx = db_tx.tx_id;
        let doc_id = DocId::from_key(id);

        let stored = StoredDocument {
            id: id.to_string(),
            embedding: embedding.to_vec(),
            metadata: metadata.clone(),
        };
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
        if let Some(old_bytes) = self.storage.get(&user_key).await? {
            let old_stored: StoredDocument = serde_json::from_slice(&old_bytes)?;
            if let Some(old_text) = extract_text(&old_stored.metadata) {
                self.text_index
                    .delete_document(tx, doc_id, &old_text)
                    .await?;
            }
        }

        let stored = StoredDocument {
            id: id.to_string(),
            embedding: embedding.to_vec(),
            metadata: metadata.clone(),
        };
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

    pub async fn delete(&self, id: &str) -> Result<()> {
        let db_tx = self.begin_transaction();
        let tx = db_tx.tx_id;
        let doc_id = DocId::from_key(id);

        let user_key = self.namespaced_key(id.as_bytes(), 0);

        // Remove from old text index
        if let Some(old_bytes) = self.storage.get(&user_key).await? {
            let old_stored: StoredDocument = serde_json::from_slice(&old_bytes)?;
            if let Some(old_text) = extract_text(&old_stored.metadata) {
                self.text_index
                    .delete_document(tx, doc_id, &old_text)
                    .await?;
            }
        }

        let doc_key = self.namespaced_key(&doc_id.inner().to_le_bytes(), 1);

        self.storage.delete(tx, &user_key).await?;
        self.storage.delete(tx, &doc_key).await?;

        db_tx.record_keys(user_key, doc_key, doc_id);

        let _ = self.index.delete(tx, doc_id).await;

        db_tx.commit().await?;

        Ok(())
    }

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

    pub async fn scan_prefix(
        &self,
        prefix: &str,
    ) -> Result<Vec<(String, serde_json::Value)>> {
        let real_prefix = if prefix.starts_with("__rel:") {
            self.namespaced_key(
                prefix.strip_prefix("__rel:").unwrap_or(prefix).as_bytes(),
                2,
            )
        } else {
            self.namespaced_key(prefix.as_bytes(), 0)
        };

        let kvs = self.storage.scan_prefix(&real_prefix).await?;

        let mut results = Vec::new();
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
        let scored_docs = self.index.search_filtered(query, k, filter).await?;
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

    pub async fn hybrid_search(
        &self,
        text: &str,
        vector: &[f32],
        k: usize,
    ) -> Result<Vec<crate::SearchResult>> {
        let is_vector_zero = vector.iter().all(|&v| v == 0.0);
        let is_text_empty = text.trim().is_empty();

        if is_text_empty && is_vector_zero {
            return Ok(Vec::new());
        }

        if is_text_empty {
            return self.search(vector, k).await;
        }

        let bm25_results = self.text_index.search_bm25(text, k).await?;

        let mut text_set = Vec::new();
        for (doc_id, score) in bm25_results {
            let doc_key = self.namespaced_key(&doc_id.inner().to_le_bytes(), 1);
            if let Some(bytes) = self.storage.get(&doc_key).await? {
                let stored: StoredDocument = serde_json::from_slice(&bytes)?;
                text_set.push(crate::SearchResult {
                    id: stored.id,
                    score,
                    metadata: stored.metadata,
                });
            }
        }

        if is_vector_zero {
            return Ok(text_set);
        }

        let vec_results = self.search(vector, k).await?;

        Ok(crate::fusion::reciprocal_rank_fusion(
            vec![vec_results, text_set],
            k,
        ))
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
