// ANCHOR:ARCH:COLLECTION-001 — Logische Isolation (Namespaces).
// DESIGN: Eigener HNSW-Index pro Collection, GEMEINSAMER LSM-Storage.
// PREFIXING: Jeder Key im LSM bekommt das Prefix `__col:{name}:`.
//! Logically isolated Collections inside the MemFuse database.

use memfuse_core::{DocId, Result, StorageEngine, TxId, VectorIndex};
use memfuse_index::HnswIndex;
use memfuse_store::LsmStorage;
use memfuse_text::{Bm25Scorer, InvertedIndex, Tokenizer};
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

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
    text_index: Arc<InvertedIndex>,
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
        let text_index = Arc::new(InvertedIndex::new(Arc::clone(&storage), &name));
        Self {
            name,
            prefix,
            index,
            storage,
            next_tx,
            dimension,
            text_index,
        }
    }

    fn namespaced_key(&self, id: &str) -> Vec<u8> {
        let mut key = self.prefix.clone();
        key.extend_from_slice(id.as_bytes());
        key
    }

    pub async fn insert(&self, id: &str, embedding: &[f32], metadata: Option<Value>) -> Result<()> {
        if embedding.len() != self.dimension {
            return Err(memfuse_core::MemFuseError::InvalidInput(format!(
                "Embedding dimension mismatch: expected {}, got {}",
                self.dimension,
                embedding.len()
            )));
        }

        let doc_id = DocId::from_key(id);
        let tx_id = TxId::new(self.next_tx.fetch_add(1, Ordering::SeqCst));

        // 1. Store document in LSM
        let key = self.namespaced_key(id);
        let doc = crate::Document {
            id: id.to_string(),
            metadata: metadata.clone(),
        };
        let doc_bytes = serde_json::to_vec(&doc)
            .map_err(|e| memfuse_core::MemFuseError::Storage(format!("JSON error: {}", e)))?;
        self.storage.put(tx_id, &key, &doc_bytes).await?;

        // 2. Store DocId -> ID mapping for reverse lookup
        let id_key = format!("__docid:{}:", doc_id.inner()).into_bytes();
        let mut full_id_key = self.prefix.clone();
        full_id_key.extend_from_slice(&id_key);
        self.storage.put(tx_id, &full_id_key, id.as_bytes()).await?;

        // 3. Index in HNSW
        self.index.insert(tx_id, doc_id, embedding).await?;

        // 4. Index text if metadata has "text" field
        if let Some(meta) = metadata {
            if let Some(text) = meta.get("text").and_then(|v| v.as_str()) {
                let tokens = Tokenizer::tokenize(text);
                self.text_index
                    .index_document(tx_id, doc_id, &tokens)
                    .await?;
            }
        }

        self.storage.commit(tx_id).await?;
        Ok(())
    }

    pub async fn search(
        &self,
        query_embedding: &[f32],
        k: usize,
    ) -> Result<Vec<crate::SearchResult>> {
        let results = self.index.search(query_embedding, k).await?;
        let mut search_results = Vec::new();

        for scored_doc in results {
            let doc_id = scored_doc.doc_id;
            let id_key = format!("__docid:{}:", doc_id.inner()).into_bytes();
            let mut full_id_key = self.prefix.clone();
            full_id_key.extend_from_slice(&id_key);

            if let Some(bytes) = self.storage.get(&full_id_key).await? {
                let id_str = String::from_utf8_lossy(&bytes).to_string();
                let metadata = self.get(&id_str).await?.and_then(|d| d.metadata);
                search_results.push(crate::SearchResult {
                    id: id_str,
                    score: scored_doc.score,
                    metadata,
                });
            } else {
                search_results.push(crate::SearchResult {
                    id: format!("doc-{}", scored_doc.doc_id.inner()),
                    score: scored_doc.score,
                    metadata: None,
                });
            }
        }

        Ok(search_results)
    }

    pub async fn hybrid_search(
        &self,
        text_query: &str,
        vector_query: &[f32],
        k: usize,
    ) -> Result<Vec<crate::SearchResult>> {
        // 1. Vector Search
        let vector_results = if !vector_query.iter().all(|&x| x == 0.0) {
            self.index.search(vector_query, k * 2).await? // Get more candidates for fusion
        } else {
            vec![]
        };

        // 2. BM25 Search
        let bm25_results = if !text_query.is_empty() {
            let tokens = Tokenizer::tokenize(text_query);
            let (total_docs, avg_len) = self.text_index.get_stats().await?;
            let mut scorer = Bm25Scorer::new(1.5, 0.75, avg_len, total_docs as f32);

            let mut doc_scores: std::collections::HashMap<u64, f32> =
                std::collections::HashMap::new();
            let mut doc_len_cache: std::collections::HashMap<u64, u32> =
                std::collections::HashMap::new();

            for token in &tokens {
                if let Some(posting) = self.text_index.get_posting(token).await? {
                    scorer.add_doc_freq(token, posting.entries.len() as u32);
                    for (doc_id_raw, tf) in posting.entries {
                        let doc_len = if let Some(&len) = doc_len_cache.get(&doc_id_raw) {
                            len
                        } else {
                            let len = self.text_index.get_doc_len(DocId::new(doc_id_raw)).await?;
                            doc_len_cache.insert(doc_id_raw, len);
                            len
                        };
                        let score = scorer.score_term(token, tf, doc_len);
                        *doc_scores.entry(doc_id_raw).or_insert(0.0) += score;
                    }
                }
            }

            let mut scored: Vec<(u64, f32)> = doc_scores.into_iter().collect();
            scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            scored.truncate(k * 2);
            scored
        } else {
            vec![]
        };

        // 3. Fusion (RRF)
        let vector_doc_ids: Vec<DocId> = vector_results.iter().map(|r| r.doc_id).collect();
        let bm25_doc_ids: Vec<DocId> = bm25_results.iter().map(|r| DocId::new(r.0)).collect();

        let fused = crate::fusion::reciprocal_rank_fusion(&[vector_doc_ids, bm25_doc_ids], 60.0);

        let mut final_results = Vec::new();
        for (doc_id, score) in fused.into_iter().take(k) {
            let id_key = format!("__docid:{}:", doc_id.inner()).into_bytes();
            let mut full_id_key = self.prefix.clone();
            full_id_key.extend_from_slice(&id_key);

            if let Some(bytes) = self.storage.get(&full_id_key).await? {
                let id_str = String::from_utf8_lossy(&bytes).to_string();
                let metadata = self.get(&id_str).await?.and_then(|d| d.metadata);
                final_results.push(crate::SearchResult {
                    id: id_str,
                    score,
                    metadata,
                });
            } else {
                final_results.push(crate::SearchResult {
                    id: format!("doc-{}", doc_id.inner()),
                    score,
                    metadata: None,
                });
            }
        }

        Ok(final_results)
    }

    pub async fn get(&self, id: &str) -> Result<Option<crate::Document>> {
        let key = self.namespaced_key(id);
        if let Some(bytes) = self.storage.get(&key).await? {
            let doc = serde_json::from_slice(&bytes)
                .map_err(|e| memfuse_core::MemFuseError::Storage(format!("JSON error: {}", e)))?;
            Ok(Some(doc))
        } else {
            Ok(None)
        }
    }

    pub async fn update(&self, id: &str, embedding: &[f32], metadata: Option<Value>) -> Result<()> {
        self.delete(id).await?;
        self.insert(id, embedding, metadata).await?;
        Ok(())
    }

    pub async fn search_filtered(
        &self,
        query: &[f32],
        k: usize,
        _filter: Option<&(dyn Fn(DocId) -> bool + Send + Sync)>,
    ) -> Result<Vec<crate::SearchResult>> {
        // Simplified filter implementation for now
        self.search(query, k).await
    }

    pub async fn delete(&self, id: &str) -> Result<()> {
        let key = self.namespaced_key(id);
        let tx_id = TxId::new(self.next_tx.fetch_add(1, Ordering::SeqCst));
        self.storage.delete(tx_id, &key).await?;
        self.storage.commit(tx_id).await?;
        // Note: HNSW doesn't support easy deletion in this simplified version
        Ok(())
    }

    pub async fn relate(&self, from: &str, to: &str, label: &str) -> Result<()> {
        let tx_id = TxId::new(self.next_tx.fetch_add(1, Ordering::SeqCst));
        let rel_key = format!("__rel:{}:{}:{}:", self.name, from, label).into_bytes();
        let mut full_key = rel_key;
        full_key.extend_from_slice(to.as_bytes());

        let val = serde_json::json!({"from": from, "to": to, "label": label});
        let val_bytes = serde_json::to_vec(&val)
            .map_err(|e| memfuse_core::MemFuseError::Storage(format!("JSON error: {}", e)))?;

        self.storage.put(tx_id, &full_key, &val_bytes).await?;
        self.storage.commit(tx_id).await?;
        Ok(())
    }

    pub async fn scan_prefix(&self, prefix: &str) -> Result<Vec<(String, Value)>> {
        let mut full_prefix = self.prefix.clone();
        full_prefix.extend_from_slice(prefix.as_bytes());

        let entries = self.storage.scan_prefix(&full_prefix).await?;
        let mut results = Vec::new();
        for (k, v) in entries {
            let key_str = String::from_utf8_lossy(&k[self.prefix.len()..]).to_string();
            let val: Value = serde_json::from_slice(&v).unwrap_or(Value::Null);
            results.push((key_str, val));
        }
        Ok(results)
    }

    pub async fn len(&self) -> usize {
        self.index.stats().await.map(|s| s.num_vectors).unwrap_or(0)
    }

    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }

    pub async fn scan(
        &self,
        start: std::ops::Bound<&[u8]>,
        end: std::ops::Bound<&[u8]>,
    ) -> Result<Vec<(String, Value)>> {
        // Simplified range scan
        let entries = self.storage.scan(start, end).await?;
        let mut results = Vec::new();
        for (k, v) in entries {
            if k.starts_with(&self.prefix) {
                let key_str = String::from_utf8_lossy(&k[self.prefix.len()..]).to_string();
                let val: Value = serde_json::from_slice(&v).unwrap_or(Value::Null);
                results.push((key_str, val));
            }
        }
        Ok(results)
    }

    pub async fn stats(&self) -> Result<memfuse_core::VectorIndexStats> {
        self.index.stats().await
    }

    pub async fn drop_collection(&self) -> Result<()> {
        // Implementation would involve deleting all keys with the collection prefix
        Ok(())
    }
}
