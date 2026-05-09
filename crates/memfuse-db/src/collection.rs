// ANCHOR:ARCH:COLLECTION-001 — Logische Isolation (Namespaces).
// WP:WP-1.2 PRIO:1 NEEDS:NONE
// AGENT:04 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-05 DEADLINE:NONE
// DESIGN: Eigener HNSW-Index pro Collection, GEMEINSAMER LSM-Storage.
// PREFIXING: Jeder Key im LSM bekommt das Prefix `__col:{name}:\x00`.
// STATUS: Full Implementation für WP-1.2.
//! Logically isolated Collections inside the MemFuse database.

use memfuse_core::{DocId, StorageEngine, TxId, VectorIndex};
use memfuse_index::HnswIndex;
use memfuse_store::LsmStorage;
use memfuse_text::inverted::InvertedIndex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

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
    #[allow(dead_code)]
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

    pub(crate) fn namespaced_key(&self, id: &str) -> Vec<u8> {
        if self.name == "default" {
            id.as_bytes().to_vec()
        } else {
            let mut key = self.prefix.clone();
            key.extend_from_slice(id.as_bytes());
            key
        }
    }

    fn namespaced_raw_key(&self, key: &[u8]) -> Vec<u8> {
        if self.name == "default" {
            key.to_vec()
        } else {
            let mut res = self.prefix.clone();
            res.extend_from_slice(key);
            res
        }
    }

    pub(crate) fn namespaced_docid_key(&self, doc_id: DocId) -> Vec<u8> {
        let mut key = if self.name == "default" {
            b"__col_docid:default:\x00".to_vec()
        } else {
            format!("__col_docid:{}:\x00", self.name).into_bytes()
        };
        key.extend_from_slice(&doc_id.inner().to_le_bytes());
        key
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
    ) -> memfuse_core::Result<()> {
        let db_tx = self.begin_transaction();
        let tx = db_tx.tx_id;
        let doc_id = DocId::from_key(id);

        let doc = crate::Document {
            id: id.to_string(),
            metadata: metadata.clone(),
        };
        let doc_bytes = serde_json::to_vec(&doc)?;

        // Forward lookup key -> document metadata
        let forward_key = self.namespaced_key(id);
        self.storage.put(tx, &forward_key, &doc_bytes).await?;

        // Reverse lookup DocId -> string id
        let reverse_key = self.namespaced_docid_key(doc_id);
        self.storage.put(tx, &reverse_key, id.as_bytes()).await?;

        // Record for compensating translation
        db_tx.record_keys(forward_key, reverse_key, doc_id);

        // Index the embedding
        self.index.insert(tx, doc_id, embedding).await?;

        // Index text if present
        if let Some(text) = extract_text(&metadata) {
            self.text_index.upsert_document(tx, doc_id, &text).await?;
        }

        db_tx.commit().await?;

        Ok(())
    }

    pub async fn get(&self, id: &str) -> memfuse_core::Result<Option<crate::Document>> {
        let forward_key = self.namespaced_key(id);
        if let Some(bytes) = self.storage.get(&forward_key).await? {
            let doc: crate::Document = serde_json::from_slice(&bytes)?;
            Ok(Some(doc))
        } else {
            Ok(None)
        }
    }

    pub async fn search(
        &self,
        query_embedding: &[f32],
        k: usize,
    ) -> memfuse_core::Result<Vec<crate::SearchResult>> {
        self.search_filtered(query_embedding, k, None).await
    }

    pub async fn hybrid_search(
        &self,
        text: &str,
        vector: &[f32],
        k: usize,
    ) -> memfuse_core::Result<Vec<crate::SearchResult>> {
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
            let reverse_key = self.namespaced_docid_key(doc_id);
            if let Some(id_bytes) = self.storage.get(&reverse_key).await? {
                if let Ok(id_str) = String::from_utf8(id_bytes) {
                    if let Some(doc) = self.get(&id_str).await? {
                        text_set.push(crate::SearchResult {
                            id: doc.id,
                            score,
                            metadata: doc.metadata,
                        });
                    }
                }
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

    pub async fn search_filtered(
        &self,
        query: &[f32],
        k: usize,
        filter: Option<&(dyn Fn(DocId) -> bool + Send + Sync)>,
    ) -> memfuse_core::Result<Vec<crate::SearchResult>> {
        let scored_docs = self.index.search_filtered(query, k, filter).await?;
        let mut results = Vec::with_capacity(scored_docs.len());

        for sd in scored_docs {
            let reverse_key = self.namespaced_docid_key(sd.doc_id);
            if let Some(id_bytes) = self.storage.get(&reverse_key).await? {
                if let Ok(id_str) = String::from_utf8(id_bytes) {
                    if let Some(doc) = self.get(&id_str).await? {
                        results.push(crate::SearchResult {
                            id: doc.id,
                            score: sd.score,
                            metadata: doc.metadata,
                        });
                    }
                }
            }
        }
        Ok(results)
    }

    pub async fn update(
        &self,
        id: &str,
        embedding: &[f32],
        metadata: Option<serde_json::Value>,
    ) -> memfuse_core::Result<()> {
        let db_tx = self.begin_transaction();
        let tx = db_tx.tx_id;
        let doc_id = DocId::from_key(id);

        let forward_key = self.namespaced_key(id);

        // Remove from old text index
        if let Some(old_bytes) = self.storage.get(&forward_key).await? {
            if let Ok(old_doc) = serde_json::from_slice::<crate::Document>(&old_bytes) {
                if let Some(old_text) = extract_text(&old_doc.metadata) {
                    self.text_index
                        .delete_document(tx, doc_id, &old_text)
                        .await?;
                }
            }
        }

        let doc = crate::Document {
            id: id.to_string(),
            metadata: metadata.clone(),
        };
        let doc_bytes = serde_json::to_vec(&doc)?;

        self.storage.put(tx, &forward_key, &doc_bytes).await?;

        let reverse_key = self.namespaced_docid_key(doc_id);
        self.storage.put(tx, &reverse_key, id.as_bytes()).await?;

        db_tx.record_keys(forward_key, reverse_key, doc_id);

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

    pub async fn delete(&self, id: &str) -> memfuse_core::Result<()> {
        let db_tx = self.begin_transaction();
        let tx = db_tx.tx_id;
        let doc_id = DocId::from_key(id);

        let forward_key = self.namespaced_key(id);

        // Remove from old text index
        if let Some(old_bytes) = self.storage.get(&forward_key).await? {
            if let Ok(old_doc) = serde_json::from_slice::<crate::Document>(&old_bytes) {
                if let Some(old_text) = extract_text(&old_doc.metadata) {
                    self.text_index
                        .delete_document(tx, doc_id, &old_text)
                        .await?;
                }
            }
        }

        self.storage.delete(tx, &forward_key).await?;

        let reverse_key = self.namespaced_docid_key(doc_id);
        self.storage.delete(tx, &reverse_key).await?;

        db_tx.record_keys(forward_key, reverse_key, doc_id);

        let _ = self.index.delete(tx, doc_id).await;

        db_tx.commit().await?;

        Ok(())
    }

    pub async fn relate(&self, from: &str, to: &str, label: &str) -> memfuse_core::Result<()> {
        let tx = TxId::new(self.next_tx.fetch_add(1, Ordering::SeqCst));
        let rel_key = self.namespaced_key(&format!("__rel:{}:{}:{}", from, label, to));
        let val = serde_json::json!({
            "from": from,
            "to": to,
            "label": label,
        });
        let bytes = serde_json::to_vec(&val)?;

        self.storage.put(tx, &rel_key, &bytes).await?;
        self.storage.commit(tx).await?;
        Ok(())
    }

    pub async fn scan_prefix(
        &self,
        prefix: &str,
    ) -> memfuse_core::Result<Vec<(String, serde_json::Value)>> {
        let search_prefix = self.namespaced_key(prefix);
        let kvs = self.storage.scan_prefix(&search_prefix).await?;

        let mut results = Vec::new();
        for (k, v) in kvs {
            let key_str = String::from_utf8(k).unwrap_or_default();
            // strip the namespace prefix if any
            let user_key = if self.name == "default" {
                key_str
            } else {
                let pfx = String::from_utf8_lossy(&self.prefix);
                key_str
                    .strip_prefix(pfx.as_ref())
                    .unwrap_or(&key_str)
                    .to_string()
            };

            if let Ok(val) = serde_json::from_slice(&v) {
                results.push((user_key, val));
            }
        }
        Ok(results)
    }

    pub async fn len(&self) -> usize {
        self.index.len().await
    }

    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }

    pub async fn scan(
        &self,
        start: std::ops::Bound<&[u8]>,
        end: std::ops::Bound<&[u8]>,
    ) -> memfuse_core::Result<Vec<(String, serde_json::Value)>> {
        use std::ops::Bound;

        let ns_start_vec;
        let ns_start = match start {
            Bound::Included(s) => {
                ns_start_vec = self.namespaced_raw_key(s);
                Bound::Included(ns_start_vec.as_slice())
            }
            Bound::Excluded(s) => {
                ns_start_vec = self.namespaced_raw_key(s);
                Bound::Excluded(ns_start_vec.as_slice())
            }
            Bound::Unbounded => {
                if self.name == "default" {
                    Bound::Unbounded
                } else {
                    Bound::Included(self.prefix.as_slice())
                }
            }
        };

        let ns_end_vec;
        let mut upper_bound_vec;
        let ns_end = match end {
            Bound::Included(e) => {
                ns_end_vec = self.namespaced_raw_key(e);
                Bound::Included(ns_end_vec.as_slice())
            }
            Bound::Excluded(e) => {
                ns_end_vec = self.namespaced_raw_key(e);
                Bound::Excluded(ns_end_vec.as_slice())
            }
            Bound::Unbounded => {
                if self.name == "default" {
                    Bound::Unbounded
                } else {
                    upper_bound_vec = self.prefix.clone();
                    if let Some(last) = upper_bound_vec.last_mut() {
                        *last = last.saturating_add(1);
                    }
                    Bound::Excluded(upper_bound_vec.as_slice())
                }
            }
        };

        let kvs = self.storage.scan(ns_start, ns_end).await?;
        let mut results = Vec::new();
        for (k, v) in kvs {
            if let Ok(key_str) = String::from_utf8(k) {
                let user_key = if self.name == "default" {
                    key_str
                } else {
                    let pfx = String::from_utf8_lossy(&self.prefix);
                    key_str
                        .strip_prefix(pfx.as_ref())
                        .unwrap_or(&key_str)
                        .to_string()
                };
                if let Ok(val) = serde_json::from_slice(&v) {
                    results.push((user_key, val));
                }
            }
        }
        Ok(results)
    }

    pub async fn stats(&self) -> memfuse_core::Result<memfuse_core::VectorIndexStats> {
        self.index.stats().await
    }

    pub async fn drop_collection(&self) -> memfuse_core::Result<()> {
        if self.name == "default" {
            return Ok(()); // Drop of default collection not requested
        }

        let tx = TxId::new(self.next_tx.fetch_add(1, Ordering::SeqCst));

        // 1. Delete all forward keys
        let keys = self.storage.scan_prefix(&self.prefix).await?;
        for (k, _) in keys {
            self.storage.delete(tx, &k).await?;
        }

        // 2. Delete all docid keys
        let docid_prefix = format!("__col_docid:{}:\x00", self.name).into_bytes();
        let docid_keys = self.storage.scan_prefix(&docid_prefix).await?;
        for (k, _) in docid_keys {
            self.storage.delete(tx, &k).await?;
        }

        self.storage.commit(tx).await?;
        Ok(())
    }
}
