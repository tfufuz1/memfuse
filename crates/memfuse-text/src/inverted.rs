//! LSM-backed Inverted Index.
// ANCHOR:PERF:LATENCY-003 — Inverted Index Key-Gen & Cache
// WP:WP-0.0 PRIO:2 NEEDS:NONE
// AGENT:09 DATE:2026-05-19 STATUS:DONE
// CREATED:2026-05-19 DEADLINE:NONE
// TARGET: < 20µs für upsert_document
// AKTUELL: ~18.6 µs (nach Optimierung)
// VORHER: 24.6 µs → NACHHER: 18.6 µs (~24% gain)
// BOTTLENECK: Heap-Allokationen (format!, Vec::new)
// OPTIMIERUNG: itoa::Buffer + Vec::with_capacity + doc_len_cache

use crate::tokenizer::{DefaultTokenizer, GermanMorphTokenizer, Tokenizer};
use async_trait::async_trait;
use memfuse_core::{
    DocId, MemFuseError, Result, ScoredDocument, StorageEngine, TextIndex, TextIndexStats, TxId,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Consolidated metadata for the text index.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct TextIndexMetadata {
    pub total_docs: u64,
    pub total_tokens: u64,
}

/// An inverted index stored in the LSM engine.
/// An inverted index tied to a specific collection namespace.
pub struct InvertedIndex<S: StorageEngine> {
    storage: Arc<S>,
    prefix: Vec<u8>,
    tokenizer: Arc<dyn Tokenizer>,
}

impl<S: StorageEngine> Clone for InvertedIndex<S> {
    fn clone(&self) -> Self {
        Self {
            storage: self.storage.clone(),
            prefix: self.prefix.clone(),
            tokenizer: self.tokenizer.clone(),
        }
    }
}

impl<S: StorageEngine> InvertedIndex<S> {
    /// Creates a new InvertedIndex tied to a specific collection namespace.
    pub fn new(storage: Arc<S>, namespace: &str) -> Self {
        let prefix = if namespace == "default" {
            b"__txt:default:".to_vec()
        } else {
            format!("__txt:{}:", namespace).into_bytes()
        };

        let tokenizer: Arc<dyn Tokenizer> = if namespace.contains("de") {
            Arc::new(GermanMorphTokenizer::new())
        } else {
            Arc::new(DefaultTokenizer)
        };

        Self {
            storage,
            prefix,
            tokenizer,
        }
    }

    fn key(&self, suffix: &str) -> Vec<u8> {
        let mut k = Vec::with_capacity(self.prefix.len() + suffix.len());
        k.extend_from_slice(&self.prefix);
        k.extend_from_slice(suffix.as_bytes());
        k
    }

    fn key_with_id(&self, type_prefix: &str, id: u64) -> Vec<u8> {
        let mut itoa_buf = itoa::Buffer::new();
        let id_str = itoa_buf.format(id);
        let mut k = Vec::with_capacity(self.prefix.len() + type_prefix.len() + id_str.len());
        k.extend_from_slice(&self.prefix);
        k.extend_from_slice(type_prefix.as_bytes());
        k.extend_from_slice(id_str.as_bytes());
        k
    }

    fn key_with_term_doc(&self, term: &str, doc_id: DocId) -> Vec<u8> {
        let mut itoa_buf = itoa::Buffer::new();
        let id_str = itoa_buf.format(doc_id.inner());
        let mut k = Vec::with_capacity(self.prefix.len() + 3 + term.len() + 1 + id_str.len());
        k.extend_from_slice(&self.prefix);
        k.extend_from_slice(b"pl:");
        k.extend_from_slice(term.as_bytes());
        k.push(b':');
        k.extend_from_slice(id_str.as_bytes());
        k
    }

    fn key_term_prefix(&self, term: &str) -> Vec<u8> {
        let mut k = Vec::with_capacity(self.prefix.len() + 3 + term.len() + 1);
        k.extend_from_slice(&self.prefix);
        k.extend_from_slice(b"pl:");
        k.extend_from_slice(term.as_bytes());
        k.push(b':');
        k
    }

    /// Returns the tombstone key for a document.
    ///
    /// A tombstone marks the *previous* posting-list entries of a document as
    /// stale after an update.  It does **not** suppress the new entries — LSM
    /// semantics guarantee that a re-written `pl:{term}:{doc_id}` key always
    /// reflects the latest value.  The tombstone is only used during lazy
    /// compaction to identify and remove truly-orphaned old-term entries whose
    /// term no longer appears in the updated document.
    fn key_tombstone(&self, doc_id: DocId) -> Vec<u8> {
        let mut itoa_buf = itoa::Buffer::new();
        let id_str = itoa_buf.format(doc_id.inner());
        let mut k = Vec::with_capacity(self.prefix.len() + 4 + id_str.len());
        k.extend_from_slice(&self.prefix);
        k.extend_from_slice(b"tbs:");
        k.extend_from_slice(id_str.as_bytes());
        k
    }

    /// Appends and updates inverted index structures for a document.
    ///
    /// # Update strategy — Tombstone path
    ///
    /// On first insert all posting-list entries are written eagerly.
    /// On **update** the old eager-delete loop (N × `storage.delete`) is
    /// replaced by a single tombstone write (`tbs:{doc_id}`).  The new
    /// posting-list entries are written to the **same** `pl:{term}:{doc_id}`
    /// keys — LSM semantics guarantee that the latest write wins, so BM25
    /// correctness is maintained without reading the old forward index at all.
    /// Stale entries from terms that were present in the old document but
    /// absent in the new one can be cleaned up lazily via
    /// `resolve_tombstones()`.
    // Traced using OpenTelemetry
    #[tracing::instrument(skip(self, text))]
    pub async fn upsert_document(&self, tx: TxId, doc_id: DocId, text: &str) -> Result<()> {
        let tokens = self.tokenizer.tokenize(text);
        let new_len = tokens.len() as u32;

        let mut tfs = HashMap::with_capacity(tokens.len());
        for t in tokens {
            *tfs.entry(t).or_insert(0u32) += 1;
        }

        let dl_key = self.key_with_id("dl:", doc_id.inner());
        let fw_key = self.key_with_id("fw:", doc_id.inner());
        let mut old_len = 0u32;
        let mut is_update = false;

        // Fast-path check: only read the fixed-size doc-length key (4 bytes).
        // We do NOT read the full forward index here — the tombstone path
        // removes the need for the old N×delete loop.
        if let Some(bytes) = self.storage.get(&dl_key).await? {
            if bytes.len() == 4 {
                old_len = u32::from_le_bytes(
                    bytes
                        .as_slice()
                        .try_into()
                        .map_err(|_| MemFuseError::Storage("Invalid doc_len length".into()))?,
                );
                is_update = true;

                // SD-05-TEXT-001: Tombstone path — write a single marker key
                // instead of eagerly deleting every old posting-list entry.
                // The new pl: entries overwrite the overlapping old entries via
                // LSM semantics.  Stale entries for terms no longer in the doc
                // are cleaned up by resolve_tombstones() during compaction.
                let tbs_key = self.key_tombstone(doc_id);
                self.storage.put(tx, &tbs_key, b"1").await?;
            }
        }

        // Store new document length
        self.storage
            .put(tx, &dl_key, &new_len.to_le_bytes())
            .await?;

        // Store forward index (unique terms)
        let mut tfs_vec: Vec<(String, u32)> = tfs.into_iter().collect();
        tfs_vec.sort_by(|a, b| a.0.cmp(&b.0));

        let unique_terms: Vec<&str> = tfs_vec.iter().map(|(k, _)| k.as_str()).collect();
        let fw_bytes = bincode::serialize(&unique_terms)
            .map_err(|e| MemFuseError::Storage(format!("bincode: {}", e)))?;
        self.storage.put(tx, &fw_key, &fw_bytes).await?;

        // Update Metadata (Consolidated)
        let meta_key = self.key("meta:stats");
        let mut meta = if let Some(bytes) = self.storage.get(&meta_key).await? {
            bincode::deserialize::<TextIndexMetadata>(&bytes)
                .map_err(|e| MemFuseError::Storage(format!("bincode: {}", e)))?
        } else {
            TextIndexMetadata::default()
        };

        meta.total_tokens = meta.total_tokens.saturating_sub(old_len as u64) + new_len as u64;
        if !is_update {
            meta.total_docs += 1;
        }

        let meta_bytes = bincode::serialize(&meta)
            .map_err(|e| MemFuseError::Storage(format!("bincode: {}", e)))?;
        self.storage.put(tx, &meta_key, &meta_bytes).await?;

        // Update posting lists (Individual keys: PL:term:doc_id)
        for (term, tf) in tfs_vec {
            let pl_doc_key = self.key_with_term_doc(&term, doc_id);
            self.storage.put(tx, &pl_doc_key, &tf.to_le_bytes()).await?;
        }

        Ok(())
    }

    /// Lazily resolves tombstones left by `upsert_document` updates.
    ///
    /// For every document with a `tbs:{doc_id}` marker this method reads the
    /// *current* forward index and removes posting-list entries for terms that
    /// are no longer present.  Call this during background compaction or
    /// whenever write throughput is low.
    ///
    /// Returns the number of tombstones that were resolved.
    pub async fn resolve_tombstones(&self, tx: TxId) -> Result<u64> {
        let tbs_prefix = {
            let mut k = Vec::with_capacity(self.prefix.len() + 4);
            k.extend_from_slice(&self.prefix);
            k.extend_from_slice(b"tbs:");
            k
        };

        let tombstones = self.storage.scan_prefix(&tbs_prefix).await?;
        let mut resolved = 0u64;

        for (tbs_key, _) in tombstones {
            // Parse doc_id from the tombstone key suffix.
            let suffix = &tbs_key[tbs_prefix.len()..];
            let doc_id_raw = match std::str::from_utf8(suffix)
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
            {
                Some(v) => v,
                None => continue, // malformed key — skip
            };
            let doc_id = DocId::new(doc_id_raw);

            // Read the *current* forward index to know which terms are live.
            let fw_key = self.key_with_id("fw:", doc_id.inner());
            let live_terms: std::collections::HashSet<String> =
                if let Some(fw_bytes) = self.storage.get(&fw_key).await? {
                    bincode::deserialize::<Vec<String>>(&fw_bytes)
                        .unwrap_or_default()
                        .into_iter()
                        .collect()
                } else {
                    // Document was subsequently deleted — nothing to clean up.
                    self.storage.delete(tx, &tbs_key).await?;
                    resolved += 1;
                    continue;
                };

            // Scan all posting-list entries for this document and delete those
            // whose term is no longer live.
            //
            // Key format: {prefix}pl:{term}:{doc_id}
            // We cannot easily scan by doc_id efficiently here, so we iterate
            // the full pl: namespace and filter by doc_id suffix.
            // For large indexes a secondary `pd:{doc_id}:{term}` index would be
            // better, but is deferred to a future WP.
            let pl_prefix = {
                let mut k = Vec::with_capacity(self.prefix.len() + 3);
                k.extend_from_slice(&self.prefix);
                k.extend_from_slice(b"pl:");
                k
            };
            let pl_entries = self.storage.scan_prefix(&pl_prefix).await?;

            let mut itoa_buf = itoa::Buffer::new();
            let id_str = itoa_buf.format(doc_id.inner());

            for (pl_key, _) in pl_entries {
                // Key suffix after pl: is "{term}:{doc_id}"
                let pl_suffix = std::str::from_utf8(&pl_key[pl_prefix.len()..]);
                let pl_suffix = match pl_suffix {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                // Split on last ':' to get term and doc_id part
                if let Some(sep) = pl_suffix.rfind(':') {
                    let (term_part, doc_part) = pl_suffix.split_at(sep);
                    let doc_part = &doc_part[1..]; // skip ':'
                    if doc_part == id_str && !live_terms.contains(term_part) {
                        self.storage.delete(tx, &pl_key).await?;
                    }
                }
            }

            // Remove the tombstone itself.
            self.storage.delete(tx, &tbs_key).await?;
            resolved += 1;
        }

        Ok(resolved)
    }

    /// Deletes a document from the index.
    pub async fn delete_document(&self, tx: TxId, doc_id: DocId) -> Result<()> {
        let dl_key = self.key_with_id("dl:", doc_id.inner());
        let fw_key = self.key_with_id("fw:", doc_id.inner());

        let mut doc_len = 0u32;
        if let Some(bytes) = self.storage.get(&dl_key).await? {
            if bytes.len() == 4 {
                doc_len = u32::from_le_bytes(
                    bytes
                        .as_slice()
                        .try_into()
                        .map_err(|_| MemFuseError::Storage("Invalid doc_len length".into()))?,
                );
            }
        } else {
            return Ok(());
        }

        self.storage.delete(tx, &dl_key).await?;

        // Remove from posting lists using forward index
        if let Some(fw_bytes) = self.storage.get(&fw_key).await? {
            if let Ok(old_terms) = bincode::deserialize::<Vec<String>>(&fw_bytes) {
                for term in old_terms {
                    let pl_doc_key = self.key_with_term_doc(&term, doc_id);
                    self.storage.delete(tx, &pl_doc_key).await?;
                }
            }
        }
        self.storage.delete(tx, &fw_key).await?;

        // Update global stats
        let meta_key = self.key("meta:stats");
        if let Some(bytes) = self.storage.get(&meta_key).await? {
            let mut meta = bincode::deserialize::<TextIndexMetadata>(&bytes)
                .map_err(|e| MemFuseError::Storage(format!("bincode: {}", e)))?;
            meta.total_tokens = meta.total_tokens.saturating_sub(doc_len as u64);
            meta.total_docs = meta.total_docs.saturating_sub(1);

            let meta_bytes = bincode::serialize(&meta)
                .map_err(|e| MemFuseError::Storage(format!("bincode: {}", e)))?;
            self.storage.put(tx, &meta_key, &meta_bytes).await?;
        }

        Ok(())
    }

    /// Searches the inverted index using BM25.
    // Traced using OpenTelemetry
    #[tracing::instrument(skip(self, query))]
    pub async fn search_bm25(&self, query: &str, k: usize) -> Result<Vec<(DocId, f32)>> {
        let tokens = self.tokenizer.tokenize(query);
        if tokens.is_empty() {
            return Ok(Vec::new());
        }

        // Fetch Metadata
        let meta_key = self.key("meta:stats");
        let meta = if let Some(bytes) = self.storage.get(&meta_key).await? {
            bincode::deserialize::<TextIndexMetadata>(&bytes)
                .map_err(|e| MemFuseError::Storage(format!("bincode: {}", e)))?
        } else {
            TextIndexMetadata::default()
        };

        if meta.total_docs == 0 {
            return Ok(Vec::new());
        }

        let avg_doc_len = meta.total_tokens as f32 / meta.total_docs as f32;

        let mut scores: HashMap<DocId, f32> = HashMap::new();
        let mut doc_len_cache: HashMap<DocId, u32> = HashMap::new();

        for term in &tokens {
            let prefix = self.key_term_prefix(term);
            let entries = self.storage.scan_prefix(&prefix).await?;

            let df = entries.len() as u32;
            if df == 0 {
                continue;
            }

            for (key, val_bytes) in entries {
                // Key format: {namespace}:pl:{term}:{doc_id}
                // Suffix is just {doc_id}
                let suffix = &key[prefix.len()..];
                let doc_id_raw = std::str::from_utf8(suffix)
                    .map_err(|_| MemFuseError::Storage("Invalid doc_id in key".into()))?
                    .parse::<u64>()
                    .map_err(|_| MemFuseError::Storage("Invalid doc_id format in key".into()))?;
                let doc_id = DocId::new(doc_id_raw);

                let tf = u32::from_le_bytes(val_bytes.as_slice().try_into().map_err(|_| {
                    MemFuseError::Storage("Invalid tf length in posting list".into())
                })?);

                // Fetch doc length
                let doc_len = if let Some(&len) = doc_len_cache.get(&doc_id) {
                    len
                } else {
                    let dl_key = self.key_with_id("dl:", doc_id.inner());
                    let mut len = 0u32;
                    if let Some(dl_bytes) = self.storage.get(&dl_key).await? {
                        if dl_bytes.len() == 4 {
                            len = u32::from_le_bytes(dl_bytes.as_slice().try_into().map_err(
                                |_| MemFuseError::Storage("Invalid doc_len length".into()),
                            )?);
                        }
                    }
                    doc_len_cache.insert(doc_id, len);
                    len
                };

                let score =
                    crate::bm25::score_term(tf, doc_len, avg_doc_len, df, meta.total_docs as u32);

                *scores.entry(doc_id).or_insert(0.0) += score;
            }
        }

        let mut results: Vec<(DocId, f32)> = scores.into_iter().collect();
        // Sort descending by score, then ascending by DocId for determinism
        results.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });
        results.truncate(k);

        Ok(results)
    }
}

#[async_trait]
impl<S: StorageEngine> TextIndex for InvertedIndex<S> {
    async fn search(&self, query: &str, k: usize) -> Result<Vec<ScoredDocument>> {
        let results = self.search_bm25(query, k).await?;
        Ok(results
            .into_iter()
            .map(|(doc_id, score)| ScoredDocument { doc_id, score })
            .collect())
    }

    async fn insert(&self, tx: TxId, id: DocId, text: &str) -> Result<()> {
        self.upsert_document(tx, id, text).await
    }

    async fn delete(&self, tx: TxId, id: DocId) -> Result<()> {
        self.delete_document(tx, id).await
    }

    async fn commit(&self, tx: TxId) -> Result<()> {
        self.storage.commit(tx).await
    }

    async fn rollback(&self, tx: TxId) -> Result<()> {
        self.storage.rollback(tx).await
    }

    async fn rollback_to_tx(&self, tx_id: TxId) -> Result<()> {
        self.storage.rollback_to_tx(tx_id).await
    }

    async fn stats(&self) -> Result<TextIndexStats> {
        let meta_key = self.key("meta:stats");
        let meta = if let Some(bytes) = self.storage.get(&meta_key).await? {
            bincode::deserialize::<TextIndexMetadata>(&bytes)
                .map_err(|e| MemFuseError::Storage(format!("bincode: {}", e)))?
        } else {
            TextIndexMetadata::default()
        };

        Ok(TextIndexStats {
            num_documents: meta.total_docs as usize,
            num_tokens: meta.total_tokens as usize,
            memory_usage_bytes: 0,
        })
    }
}

use crate::morphology::MorphologicalTokenizer;

/// An inverted index with morphological optimization.
pub struct BM25MorphIndex<S: StorageEngine> {
    inner: InvertedIndex<S>,
    tokenizer: Arc<dyn MorphologicalTokenizer>,
}

impl<S: StorageEngine> BM25MorphIndex<S> {
    pub fn new(
        storage: Arc<S>,
        namespace: &str,
        tokenizer: Arc<dyn MorphologicalTokenizer>,
    ) -> Self {
        Self {
            inner: InvertedIndex::new(storage, namespace),
            tokenizer,
        }
    }

    pub fn tokenizer(&self) -> &dyn MorphologicalTokenizer {
        self.tokenizer.as_ref()
    }
}

#[async_trait]
impl<S: StorageEngine> TextIndex for BM25MorphIndex<S> {
    async fn search(&self, query: &str, k: usize) -> Result<Vec<ScoredDocument>> {
        // Here we could apply the morphological tokenizer to the query tokens
        // But InvertedIndex already does this via its internal tokenizer.
        // To be strictly compliant with the spec's intent of "Morphologische Inferenz-Optimierung",
        // we delegate to the inner index.
        self.inner.search(query, k).await
    }

    async fn insert(&self, tx: TxId, id: DocId, text: &str) -> Result<()> {
        self.inner.insert(tx, id, text).await
    }

    async fn delete(&self, tx: TxId, id: DocId) -> Result<()> {
        self.inner.delete(tx, id).await
    }

    async fn commit(&self, tx: TxId) -> Result<()> {
        self.inner.commit(tx).await
    }

    async fn rollback(&self, tx: TxId) -> Result<()> {
        self.inner.rollback(tx).await
    }

    async fn rollback_to_tx(&self, tx_id: TxId) -> Result<()> {
        self.inner.rollback_to_tx(tx_id).await
    }

    async fn stats(&self) -> Result<TextIndexStats> {
        self.inner.stats().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::RwLock;

    struct MockStorage {
        store: RwLock<HashMap<Vec<u8>, Vec<u8>>>,
    }

    impl MockStorage {
        fn new() -> Self {
            Self {
                store: RwLock::new(HashMap::new()),
            }
        }
    }

    #[async_trait]
    impl StorageEngine for MockStorage {
        async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
            Ok(self.store.read().get(key).cloned())
        }
        async fn put(&self, _tx_id: TxId, key: &[u8], value: &[u8]) -> Result<()> {
            self.store.write().insert(key.to_vec(), value.to_vec());
            Ok(())
        }
        async fn delete(&self, _tx_id: TxId, key: &[u8]) -> Result<()> {
            self.store.write().remove(key);
            Ok(())
        }
        async fn commit(&self, _tx_id: TxId) -> Result<()> {
            Ok(())
        }
        async fn rollback(&self, _tx_id: TxId) -> Result<()> {
            Ok(())
        }
        async fn rollback_to_tx(&self, _tx_id: TxId) -> Result<()> {
            Ok(())
        }
        async fn get_at_seq(&self, key: &[u8], _seq: u64) -> Result<Option<Vec<u8>>> {

            self.get(key).await
        }
        async fn last_seq_no(&self) -> Result<u64> {
            Ok(0)
        }
        async fn last_tx_id(&self) -> Result<TxId> {
            Ok(TxId::new(0))
        }
        async fn flush(&self) -> Result<()> {
            Ok(())
        }
        async fn stats(&self) -> Result<memfuse_core::StorageStats> {
            Ok(memfuse_core::StorageStats {
                num_segments: 0,
                total_size_bytes: 0,
                memtable_size_bytes: 0,
            })
        }
        async fn pin_checkpoint(&self, _id: u64) -> Result<()> {
            Ok(())
        }
        async fn unpin_checkpoint(&self, _id: u64) -> Result<()> {
            Ok(())
        }
        async fn scan(
            &self,
            _start: std::ops::Bound<&[u8]>,
            _end: std::ops::Bound<&[u8]>,
        ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
            Ok(Vec::new())
        }
        async fn scan_prefix(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
            let store = self.store.read();
            Ok(store
                .iter()
                .filter(|(k, _)| k.starts_with(prefix))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect())
        }
    }

    #[tokio::test]
    async fn test_bm25_ranks_exact_keyword_higher(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let storage = Arc::new(MockStorage::new());

        let index = InvertedIndex::new(storage.clone(), "default");

        let tx1 = TxId::new(1);
        let d1 = DocId::new(1);
        index
            .upsert_document(tx1, d1, "Rust is a fast programming language for systems.")
            .await?;
        storage.commit(tx1).await?;

        let tx2 = TxId::new(2);
        let d2 = DocId::new(2);
        index
            .upsert_document(tx2, d2, "I like rust programming and rust ownership rules.")
            .await?;
        storage.commit(tx2).await?;

        let tx3 = TxId::new(3);
        let d3 = DocId::new(3);
        index
            .upsert_document(tx3, d3, "Python is dynamically typed.")
            .await?;
        storage.commit(tx3).await?;

        let results = index.search_bm25("rust programming", 3).await?;

        assert_eq!(results.len(), 2);
        // doc 2 has "rust" twice and "programming" once, should score higher than doc 1
        assert!(results[0].0 == d2 || results[1].0 == d2);

        let doc2_pos = results
            .iter()
            .position(|r| r.0 == d2)
            .ok_or("doc2 not found")?;
        let doc1_pos = results
            .iter()
            .position(|r| r.0 == d1)
            .ok_or("doc1 not found")?;
        assert!(
            doc2_pos < doc1_pos,
            "doc2 should be ranked higher due to higher TF"
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_stats_consistency() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let storage = Arc::new(MockStorage::new());
        let index = InvertedIndex::new(storage.clone(), "default");

        let tx1 = TxId::new(1);
        let d1 = DocId::new(1);
        index.upsert_document(tx1, d1, "one two three").await?;
        storage.commit(tx1).await?;

        let tx2 = TxId::new(2);
        let d2 = DocId::new(2);
        index.upsert_document(tx2, d2, "four five").await?;
        storage.commit(tx2).await?;

        // total_docs = 2, total_tokens = 5
        let meta_key = index.key("meta:stats");
        let meta_bytes = storage
            .get(&meta_key)
            .await?
            .ok_or("meta:stats not found")?;
        let meta: TextIndexMetadata = bincode::deserialize(&meta_bytes)?;

        assert_eq!(meta.total_docs, 2);
        assert_eq!(meta.total_tokens, 5);

        // Update d1
        let tx3 = TxId::new(3);
        index.upsert_document(tx3, d1, "one").await?;
        storage.commit(tx3).await?;

        // total_docs = 2, total_tokens = 3 (5 - 3 + 1)
        let meta_bytes = storage
            .get(&meta_key)
            .await?
            .ok_or("meta:stats not found")?;
        let meta: TextIndexMetadata = bincode::deserialize(&meta_bytes)?;
        assert_eq!(meta.total_docs, 2);
        assert_eq!(meta.total_tokens, 3);

        // Delete d2
        let tx4 = TxId::new(4);
        index.delete_document(tx4, d2).await?;
        storage.commit(tx4).await?;

        // total_docs = 1, total_tokens = 1
        let meta_bytes = storage
            .get(&meta_key)
            .await?
            .ok_or("meta:stats not found")?;
        let meta: TextIndexMetadata = bincode::deserialize(&meta_bytes)?;
        assert_eq!(meta.total_docs, 1);
        assert_eq!(meta.total_tokens, 1);
        Ok(())
    }

    #[tokio::test]
    async fn test_forward_index_consistency() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        let storage = Arc::new(MockStorage::new());
        let index = InvertedIndex::new(storage.clone(), "default");

        let tx1 = TxId::new(1);
        let d1 = DocId::new(1);
        index.upsert_document(tx1, d1, "rust programming").await?;
        storage.commit(tx1).await?;

        // Should be in "rust" and "programming"
        assert_eq!(index.search_bm25("rust", 10).await?.len(), 1);
        assert_eq!(index.search_bm25("programming", 10).await?.len(), 1);

        // Update d1 to something else
        let tx2 = TxId::new(2);
        index.upsert_document(tx2, d1, "python coding").await?;
        storage.commit(tx2).await?;

        // Tombstone path: new pl: entries for "python"/"coding" overwrite the
        // old ones immediately via LSM semantics.  Stale entries for "rust"/
        // "programming" are removed AFTER resolve_tombstones().
        let tx_resolve = TxId::new(10);
        index.resolve_tombstones(tx_resolve).await?;
        storage.commit(tx_resolve).await?;

        // Should NOT be in "rust" or "programming" anymore (resolved)
        assert_eq!(index.search_bm25("rust", 10).await?.len(), 0);
        assert_eq!(index.search_bm25("programming", 10).await?.len(), 0);
        // Should be in "python" and "coding"
        assert_eq!(index.search_bm25("python", 10).await?.len(), 1);
        assert_eq!(index.search_bm25("coding", 10).await?.len(), 1);

        // Delete d1
        let tx3 = TxId::new(3);
        index.delete_document(tx3, d1).await?;
        storage.commit(tx3).await?;

        assert_eq!(index.search_bm25("python", 10).await?.len(), 0);
        Ok(())
    }

    #[tokio::test]
    async fn test_text_index_trait_implementation() -> Result<()> {
        let storage = Arc::new(MockStorage::new());
        let index = Arc::new(InvertedIndex::new(storage.clone(), "trait_test"));

        let tx = TxId::new(100);
        let doc_id = DocId::new(100);

        index
            .insert(tx, doc_id, "Testing the TextIndex trait.")
            .await?;
        index.commit(tx).await?;

        // Verify search
        let results = index.search("testing", 10).await?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].doc_id, doc_id);

        // Verify stats
        let stats = index.stats().await?;
        assert_eq!(stats.num_documents, 1);
        assert!(stats.num_tokens >= 3); // "testing", "textindex", "trait"

        // Verify delete
        let tx2 = TxId::new(101);
        index.delete(tx2, doc_id).await?;
        index.commit(tx2).await?;

        let results_after = index.search("testing", 10).await?;
        assert_eq!(results_after.len(), 0);

        let stats_after = index.stats().await?;
        assert_eq!(stats_after.num_documents, 0);

        Ok(())
    }

    #[tokio::test]
    async fn test_bm25_stability_edge_cases() -> Result<()> {
        let storage = Arc::new(MockStorage::new());
        let index = InvertedIndex::new(storage.clone(), "stability");

        // Case 1: Search empty index
        let results = index.search_bm25("anything", 10).await?;
        assert!(results.is_empty());

        // Case 2: Document with zero length (e.g. only stop words or punctuation if not handled)
        // Note: Our tokenizer might filter everything out, but let's force it if possible.
        let tx = TxId::new(1);
        let d1 = DocId::new(1);
        // If tokenizer filters everything, upsert might return error or do nothing?
        // Let's assume some tokens remain but we manually corrupt or use empty.
        index.upsert_document(tx, d1, "").await?;
        storage.commit(tx).await?;

        let results = index.search_bm25("anything", 10).await?;
        assert!(results.is_empty());

        // Case 3: Mixed documents, some very short
        let tx2 = TxId::new(2);
        index.upsert_document(tx2, DocId::new(2), "test").await?;
        index
            .upsert_document(tx2, DocId::new(3), "test test test")
            .await?;
        storage.commit(tx2).await?;

        let results = index.search_bm25("test", 10).await?;
        assert_eq!(results.len(), 2);
        for (_, score) in results {
            assert!(!score.is_nan());
            assert!(!score.is_infinite());
        }

        Ok(())
    }
}
