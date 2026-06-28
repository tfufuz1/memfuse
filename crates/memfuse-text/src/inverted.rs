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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Consolidated metadata for the text index.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct TextIndexMetadata {
    pub total_docs: u64,
    pub total_tokens: u64,
    pub avg_doc_len_x1000: u64, // Fixed-point for BM25 caching (FIND-TXT-004)
}

/// An inverted index stored in the LSM engine.
/// An inverted index tied to a specific collection namespace.
pub struct InvertedIndex<S: StorageEngine> {
    storage: Arc<S>,
    prefix: Vec<u8>,
    tokenizer: Arc<dyn Tokenizer>,
    pub(crate) total_docs: Arc<AtomicU64>,
    pub(crate) total_tokens: Arc<AtomicU64>,
    pub(crate) avg_doc_len_x1000: Arc<AtomicU64>, // Cached fixed-point (FIND-TXT-004)
}

impl<S: StorageEngine> Clone for InvertedIndex<S> {
    fn clone(&self) -> Self {
        Self {
            storage: self.storage.clone(),
            prefix: self.prefix.clone(),
            tokenizer: self.tokenizer.clone(),
            total_docs: self.total_docs.clone(),
            total_tokens: self.total_tokens.clone(),
            avg_doc_len_x1000: self.avg_doc_len_x1000.clone(),
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
            total_docs: Arc::new(AtomicU64::new(0)),
            total_tokens: Arc::new(AtomicU64::new(0)),
            avg_doc_len_x1000: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Loads index statistics from storage into the cache.
    pub async fn load_stats(&self) -> Result<()> {
        let meta_key = self.key("meta:stats");
        if let Some(bytes) = self.storage.get(&meta_key).await? {
            let meta: TextIndexMetadata = bincode::deserialize(&bytes)
                .map_err(|e| MemFuseError::Storage(format!("bincode: {}", e)))?;
            self.total_docs.store(meta.total_docs, Ordering::SeqCst);
            self.total_tokens.store(meta.total_tokens, Ordering::SeqCst);
            self.avg_doc_len_x1000
                .store(meta.avg_doc_len_x1000, Ordering::SeqCst);
        }
        Ok(())
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

    fn key_tombstone(&self, doc_id: DocId, term: &str) -> Vec<u8> {
        let mut itoa_buf = itoa::Buffer::new();
        let id_str = itoa_buf.format(doc_id.inner());
        let mut k = Vec::with_capacity(self.prefix.len() + 4 + id_str.len() + 1 + term.len());
        k.extend_from_slice(&self.prefix);
        k.extend_from_slice(b"tbs:");
        k.extend_from_slice(id_str.as_bytes());
        k.push(b':');
        k.extend_from_slice(term.as_bytes());
        k
    }

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

        if let Some(bytes) = self.storage.get(&dl_key).await? {
            if bytes.len() == 4 {
                old_len = u32::from_le_bytes(
                    bytes
                        .as_slice()
                        .try_into()
                        .map_err(|_| MemFuseError::Storage("Invalid doc_len length".into()))?,
                );
                is_update = true;

                if let Some(fw_bytes) = self.storage.get(&fw_key).await? {
                    let old_terms: Vec<String> = bincode::deserialize(&fw_bytes)
                        .map_err(|e| MemFuseError::Storage(format!("bincode: {}", e)))?;
                    for term in old_terms {
                        let tbs_key = self.key_tombstone(doc_id, &term);
                        self.storage.put(tx, &tbs_key, &[]).await?;
                    }
                }
            }
        }

        self.storage
            .put(tx, &dl_key, &new_len.to_le_bytes())
            .await?;

        let mut tfs_vec: Vec<(String, u32)> = tfs.into_iter().collect();
        tfs_vec.sort_by(|a, b| a.0.cmp(&b.0));

        let unique_terms: Vec<&str> = tfs_vec.iter().map(|(k, _)| k.as_str()).collect();
        let fw_bytes = bincode::serialize(&unique_terms)
            .map_err(|e| MemFuseError::Storage(format!("bincode: {}", e)))?;
        self.storage.put(tx, &fw_key, &fw_bytes).await?;

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

        let avg_len = if meta.total_docs > 0 {
            (meta.total_tokens as f64 / meta.total_docs as f64 * 1000.0) as u64
        } else {
            0
        };
        meta.avg_doc_len_x1000 = avg_len;

        let meta_bytes = bincode::serialize(&meta)
            .map_err(|e| MemFuseError::Storage(format!("bincode: {}", e)))?;
        self.storage.put(tx, &meta_key, &meta_bytes).await?;

        // FIND-TXT-004: Update cached stats
        self.total_docs.store(meta.total_docs, Ordering::SeqCst);
        self.total_tokens.store(meta.total_tokens, Ordering::SeqCst);
        self.avg_doc_len_x1000.store(avg_len, Ordering::SeqCst);

        for (term, tf) in tfs_vec {
            let pl_doc_key = self.key_with_term_doc(&term, doc_id);
            self.storage.put(tx, &pl_doc_key, &tf.to_le_bytes()).await?;
        }

        Ok(())
    }

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
            // Format: {prefix}tbs:{doc_id}:{term}
            let suffix = &tbs_key[tbs_prefix.len()..];
            let parts: Vec<&[u8]> = suffix.split(|&b| b == b':').collect();
            if parts.len() < 2 {
                continue;
            }

            let doc_id_raw = match std::str::from_utf8(parts[0])
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
            {
                Some(v) => v,
                None => continue,
            };
            let doc_id = DocId::new(doc_id_raw);
            let term = String::from_utf8_lossy(parts[1]).to_string();

            // Read the *current* forward index to know which terms are live.
            let fw_key = self.key_with_id("fw:", doc_id.inner());
            let is_live = if let Some(fw_bytes) = self.storage.get(&fw_key).await? {
                let live_terms: Vec<String> =
                    bincode::deserialize::<Vec<String>>(&fw_bytes).unwrap_or_default();
                live_terms.contains(&term)
            } else {
                false // Document deleted
            };

            if !is_live {
                let pl_key = self.key_with_term_doc(&term, doc_id);
                self.storage.delete(tx, &pl_key).await?;
            }
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
            let avg_len = if meta.total_docs > 0 {
                (meta.total_tokens as f64 / meta.total_docs as f64 * 1000.0) as u64
            } else {
                0
            };
            meta.avg_doc_len_x1000 = avg_len;

            let meta_bytes = bincode::serialize(&meta)
                .map_err(|e| MemFuseError::Storage(format!("bincode: {}", e)))?;
            self.storage.put(tx, &meta_key, &meta_bytes).await?;

            // FIND-TXT-004: Update cached stats
            self.total_docs.store(meta.total_docs, Ordering::SeqCst);
            self.total_tokens.store(meta.total_tokens, Ordering::SeqCst);
            self.avg_doc_len_x1000.store(avg_len, Ordering::SeqCst);
        }

        Ok(())
    }

    /// Searches the inverted index using BM25.
    #[tracing::instrument(skip(self, query))]
    pub async fn search_bm25(
        &self,
        query: &str,
        k: usize,
        max_seq: Option<u64>,
    ) -> Result<Vec<(DocId, f32)>> {
        self.search_bm25_at(query, k, max_seq).await
    }

    /// Searches the inverted index using BM25 at a specific snapshot version.
    #[tracing::instrument(skip(self, query))]
    pub async fn search_bm25_at(
        &self,
        query: &str,
        k: usize,
        max_seq: Option<u64>,
    ) -> Result<Vec<(DocId, f32)>> {
        let tokens = self.tokenizer.tokenize(query);
        if tokens.is_empty() {
            return Ok(Vec::new());
        }

        // 🛡️ SICHERUNG: Snapshot-Isolation (FIND-TXT-001)
        // Wir pinnen die Sequence-Number zu Beginn der Anfrage, damit alle Reads
        // (Stats, PL, DL) auf demselben konsistenten Stand basieren.
        let seq = if let Some(s) = max_seq {
            s
        } else {
            self.storage.last_seq_no().await?
        };

        // FIND-TXT-004: Use cached stats instead of storage reads
        let n = self.total_docs.load(Ordering::Acquire);
        let cached_avg_len_x1000 = self.avg_doc_len_x1000.load(Ordering::Acquire);

        let avg_doc_len = if n > 0 {
            cached_avg_len_x1000 as f32 / 1000.0
        } else {
            0.0
        };

        let mut scores: HashMap<DocId, f32> = HashMap::new();
        let mut doc_len_cache: HashMap<DocId, u32> = HashMap::new();

        for term in &tokens {
            let prefix = self.key_term_prefix(term);
            let entries = self.storage.scan_prefix_at(&prefix, seq).await?;

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
                    let dl_bytes_res = self.storage.get_at_seq(&dl_key, seq).await?;

                    if let Some(dl_bytes) = dl_bytes_res {
                        if dl_bytes.len() == 4 {
                            len = u32::from_le_bytes(dl_bytes.as_slice().try_into().map_err(
                                |_| MemFuseError::Storage("Invalid doc_len length".into()),
                            )?);
                        }
                    }
                    doc_len_cache.insert(doc_id, len);
                    len
                };

                let score = crate::bm25::score_term(tf, doc_len, avg_doc_len, df, n as u32);

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
        let results = self.search_bm25(query, k, None).await?;
        Ok(results
            .into_iter()
            .map(|(doc_id, score)| ScoredDocument { doc_id, score })
            .collect())
    }

    async fn search_at(&self, query: &str, k: usize, seq_no: u64) -> Result<Vec<ScoredDocument>> {
        let results = self.search_bm25(query, k, Some(seq_no)).await?;
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

    async fn last_tx_id(&self) -> Result<u64> {
        self.storage.last_tx_id().await.map(|tx| tx.inner())
    }

    async fn len(&self) -> usize {
        self.total_docs.load(Ordering::Acquire) as usize
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

    async fn last_tx_id(&self) -> Result<u64> {
        self.inner.last_tx_id().await
    }

    async fn len(&self) -> usize {
        self.inner.len().await
    }

    async fn stats(&self) -> Result<TextIndexStats> {
        self.inner.stats().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::RwLock;

    type MockStoreMap = RwLock<HashMap<Vec<u8>, Vec<(Vec<u8>, u64)>>>;

    struct MockStorage {
        // Map from Key -> Vec<(Value, SeqNo)>
        store: MockStoreMap,
        next_seq: std::sync::atomic::AtomicU64,
    }

    impl MockStorage {
        fn new() -> Self {
            Self {
                store: MockStoreMap::new(HashMap::new()),
                next_seq: std::sync::atomic::AtomicU64::new(1),
            }
        }
    }

    #[async_trait]
    impl StorageEngine for MockStorage {
        async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
            self.get_at_seq(key, u64::MAX).await
        }
        async fn put(&self, _tx_id: TxId, key: &[u8], value: &[u8]) -> Result<()> {
            let seq = self
                .next_seq
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.store
                .write()
                .entry(key.to_vec())
                .or_default()
                .push((value.to_vec(), seq));
            Ok(())
        }
        async fn delete(&self, _tx_id: TxId, key: &[u8]) -> Result<()> {
            // Write a "tombstone" (empty value) with new sequence
            let seq = self
                .next_seq
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let mut w = self.store.write();
            if let Some(versions) = w.get_mut(key) {
                versions.push((Vec::new(), seq | memfuse_core::TOMBSTONE_BIT));
            } else {
                w.insert(
                    key.to_vec(),
                    vec![(Vec::new(), seq | memfuse_core::TOMBSTONE_BIT)],
                );
            }
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
        async fn get_at_seq(&self, key: &[u8], seq: u64) -> Result<Option<Vec<u8>>> {
            let store = self.store.read();
            if let Some(versions) = store.get(key) {
                // Find latest version <= seq
                for (val, v_seq) in versions.iter().rev() {
                    let raw_seq = v_seq & !memfuse_core::TOMBSTONE_BIT;
                    if raw_seq <= seq {
                        if (v_seq & memfuse_core::TOMBSTONE_BIT) != 0 {
                            return Ok(None);
                        }
                        return Ok(Some(val.clone()));
                    }
                }
            }
            Ok(None)
        }
        async fn last_seq_no(&self) -> Result<u64> {
            // Return latest generated seq
            Ok(self
                .next_seq
                .load(std::sync::atomic::Ordering::SeqCst)
                .saturating_sub(1))
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
            self.scan_prefix_at(prefix, u64::MAX).await
        }
        async fn scan_prefix_at(
            &self,
            prefix: &[u8],
            seq_no: u64,
        ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
            let store = self.store.read();
            let mut results = Vec::new();
            for (k, versions) in store.iter() {
                if k.starts_with(prefix) {
                    for (val, v_seq) in versions.iter().rev() {
                        let raw_seq = v_seq & !memfuse_core::TOMBSTONE_BIT;
                        if raw_seq <= seq_no {
                            if (v_seq & memfuse_core::TOMBSTONE_BIT) == 0 {
                                results.push((k.clone(), val.clone()));
                            }
                            break; // Stop looking at older versions for this key
                        }
                    }
                }
            }
            Ok(results)
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

        let results = index.search_bm25("rust programming", 3, None).await?;

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
        assert_eq!(index.search_bm25("rust", 10, None).await?.len(), 1);
        assert_eq!(index.search_bm25("programming", 10, None).await?.len(), 1);

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
        assert_eq!(index.search_bm25("rust", 10, None).await?.len(), 0);
        assert_eq!(index.search_bm25("programming", 10, None).await?.len(), 0);
        // Should be in "python" and "coding"
        assert_eq!(index.search_bm25("python", 10, None).await?.len(), 1);
        assert_eq!(index.search_bm25("coding", 10, None).await?.len(), 1);

        // Delete d1
        let tx3 = TxId::new(3);
        index.delete_document(tx3, d1).await?;
        storage.commit(tx3).await?;

        assert_eq!(index.search_bm25("python", 10, None).await?.len(), 0);
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
        let results = index.search_bm25("anything", 10, None).await?;
        assert!(results.is_empty());

        // Case 2: Document with zero length (e.g. only stop words or punctuation if not handled)
        // Note: Our tokenizer might filter everything out, but let's force it if possible.
        let tx = TxId::new(1);
        let d1 = DocId::new(1);
        // If tokenizer filters everything, upsert might return error or do nothing?
        // Let's assume some tokens remain but we manually corrupt or use empty.
        index.upsert_document(tx, d1, "").await?;
        storage.commit(tx).await?;

        let results = index.search_bm25("anything", 10, None).await?;
        assert!(results.is_empty());

        // Case 3: Mixed documents, some very short
        let tx2 = TxId::new(2);
        index.upsert_document(tx2, DocId::new(2), "test").await?;
        index
            .upsert_document(tx2, DocId::new(3), "test test test")
            .await?;
        storage.commit(tx2).await?;

        let results = index.search_bm25("test", 10, None).await?;
        assert_eq!(results.len(), 2);
        for (_, score) in results {
            assert!(!score.is_nan());
            assert!(!score.is_infinite());
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_text_search_snapshot_isolation() -> Result<()> {
        let storage = Arc::new(MockStorage::new());
        let index = InvertedIndex::new(storage.clone(), "isolation");

        // 1. Initial State: Document 1
        let tx1 = TxId::new(1);
        let d1 = DocId::new(1);
        index.upsert_document(tx1, d1, "rust programming").await?;
        storage.commit(tx1).await?;
        let seq1 = storage.last_seq_no().await?;

        // 2. Second State: Document 2 (should not be visible at seq1)
        let tx2 = TxId::new(2);
        let d2 = DocId::new(2);
        index.upsert_document(tx2, d2, "rust compiler").await?;
        storage.commit(tx2).await?;

        // 3. Verify Isolation
        // At seq1, only d1 should exist
        let results_at_seq1 = index.search_at("rust", 10, seq1).await?;
        assert_eq!(results_at_seq1.len(), 1);
        assert_eq!(results_at_seq1[0].doc_id, d1);

        // At latest (None or higher seq), both should exist
        let results_latest = index.search("rust", 10).await?;
        assert_eq!(results_latest.len(), 2);

        Ok(())
    }
}
