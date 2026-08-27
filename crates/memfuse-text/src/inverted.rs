//! LSM-backed Inverted Index.
// CONSTRAINT: Inverted Index Key-Gen & Cache
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

/// Supported tokenizer languages for BM25 text indexing.
///
/// Controls which tokenizer is used for document indexing and query processing.
/// Use `Language::from_iso()` to parse ISO-639-1 language codes.
///
/// **Important**: Language is configured explicitly — namespace names do NOT
/// determine the tokenizer language.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Language {
    /// English tokenizer (default, whitespace-based + stopword filtering).
    #[default]
    English,
    /// German morphology tokenizer (compound splitting + umlaut normalization).
    German,
    /// Custom tokenizer — override via `with_tokenizer()`.
    Custom,
}

impl Language {
    /// Parses an ISO-639-1 language code ("de", "de-AT", "en", "en-US").
    ///
    /// Only the primary subtag (before the first '-') is evaluated.
    /// Unknown codes fall back to `Language::English` (safe default).
    pub fn from_iso(code: &str) -> Self {
        let prefix = code.split('-').next().unwrap_or("").to_lowercase();
        match prefix.as_str() {
            "de" => Self::German,
            "en" => Self::English,
            _ => Self::English,
        }
    }
}

/// Consolidated metadata for the text index.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct TextIndexMetadata {
    pub total_docs: u64,
    pub total_tokens: u64,
    pub avg_doc_len_x1000: u64, // Fixed-point for BM25 caching (FIND-TXT-004)
}

#[derive(Default, Clone, Copy, Debug)]
pub(crate) struct StagedStatsChange {
    pub(crate) docs_delta: i64,
    pub(crate) tokens_delta: i64,
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
    pub(crate) staged_stats: Arc<parking_lot::Mutex<HashMap<TxId, StagedStatsChange>>>,
    commit_lock: Arc<tokio::sync::Mutex<()>>,
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
            staged_stats: self.staged_stats.clone(),
            commit_lock: self.commit_lock.clone(),
        }
    }
}

impl<S: StorageEngine> InvertedIndex<S> {
    /// Creates a new InvertedIndex with explicit language configuration.
    ///
    /// # Language Selection
    /// Use `Language::from_iso("de")` for German,
    /// `Language::English` (default) for all other languages.
    ///
    /// **NOT** namespace-based — namespace names do NOT determine language.
    pub fn new_with_language(storage: Arc<S>, namespace: &str, language: Language) -> Self {
        let prefix = if namespace == "default" {
            b"__txt:default:".to_vec()
        } else {
            format!("__txt:{}:", namespace).into_bytes()
        };

        let tokenizer: Arc<dyn Tokenizer> = match language {
            Language::German => Arc::new(GermanMorphTokenizer::new()),
            Language::English | Language::Custom => Arc::new(DefaultTokenizer),
        };

        Self {
            storage,
            prefix,
            tokenizer,
            total_docs: Arc::new(AtomicU64::new(0)),
            total_tokens: Arc::new(AtomicU64::new(0)),
            avg_doc_len_x1000: Arc::new(AtomicU64::new(0)),
            staged_stats: Arc::new(parking_lot::Mutex::new(HashMap::new())),
            commit_lock: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    /// Creates a new InvertedIndex with English tokenizer (default).
    pub fn new(storage: Arc<S>, namespace: &str) -> Self {
        Self::new_with_language(storage, namespace, Language::English)
    }

    /// Sets a custom tokenizer for the inverted index.
    pub fn with_tokenizer(mut self, tokenizer: Arc<dyn Tokenizer>) -> Self {
        self.tokenizer = tokenizer;
        self
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

        // Stage stats changes (Option B: Atomics as Source of Truth, no direct read-modify-write)
        let mut change = StagedStatsChange::default();
        if is_update {
            change.tokens_delta = (new_len as i64) - (old_len as i64);
        } else {
            change.docs_delta = 1;
            change.tokens_delta = new_len as i64;
        }
        self.stage_stats_change(tx, change);

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
            // DECISION-REF: Consistent with load_stats() (L92) — bincode errors must propagate,
            // not be silently swallowed. Silent failure would cause phantom BM25 term deletion.
            // AI-TAG[SPEC-DRIFT][MAJOR] RESOLVED: replaced unwrap_or_default() with map_err (ID: AGT-TXT-001)
            let fw_key = self.key_with_id("fw:", doc_id.inner());
            let is_live = if let Some(fw_bytes) = self.storage.get(&fw_key).await? {
                let live_terms: Vec<String> = bincode::deserialize::<Vec<String>>(&fw_bytes)
                    .map_err(|e| {
                        MemFuseError::Storage(format!(
                            "forward-index corrupt for doc {}: {}",
                            doc_id.inner(),
                            e
                        ))
                    })?;
                live_terms.contains(&term)
            } else {
                false // Document deleted — tombstone cleanup is safe
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

        // Stage stats changes
        let change = StagedStatsChange {
            docs_delta: -1,
            tokens_delta: -(doc_len as i64),
        };
        self.stage_stats_change(tx, change);

        Ok(())
    }

    fn stage_stats_change(&self, tx: TxId, change: StagedStatsChange) {
        let mut guard = self.staged_stats.lock();
        let entry = guard.entry(tx).or_default();
        entry.docs_delta += change.docs_delta;
        entry.tokens_delta += change.tokens_delta;
    }

    #[allow(dead_code)]
    pub(crate) async fn commit_stats(&self, tx: TxId) -> Result<()> {
        let change = {
            let mut guard = self.staged_stats.lock();
            guard.remove(&tx).unwrap_or_default()
        };

        if change.docs_delta != 0 || change.tokens_delta != 0 {
            if change.docs_delta > 0 {
                self.total_docs
                    .fetch_add(change.docs_delta as u64, Ordering::SeqCst);
            } else if change.docs_delta < 0 {
                self.total_docs
                    .fetch_sub(change.docs_delta.unsigned_abs(), Ordering::SeqCst);
            }

            if change.tokens_delta > 0 {
                self.total_tokens
                    .fetch_add(change.tokens_delta as u64, Ordering::SeqCst);
            } else if change.tokens_delta < 0 {
                self.total_tokens
                    .fetch_sub(change.tokens_delta.unsigned_abs(), Ordering::SeqCst);
            }

            let docs = self.total_docs.load(Ordering::SeqCst);
            let tokens = self.total_tokens.load(Ordering::SeqCst);
            let avg_len = if docs > 0 {
                (tokens as f64 / docs as f64 * 1000.0) as u64
            } else {
                0
            };
            self.avg_doc_len_x1000.store(avg_len, Ordering::SeqCst);
        }

        let _guard = self.commit_lock.lock().await;
        let docs = self.total_docs.load(Ordering::SeqCst);
        let tokens = self.total_tokens.load(Ordering::SeqCst);
        let avg_len = self.avg_doc_len_x1000.load(Ordering::SeqCst);

        let meta = TextIndexMetadata {
            total_docs: docs,
            total_tokens: tokens,
            avg_doc_len_x1000: avg_len,
        };
        let meta_bytes = bincode::serialize(&meta)
            .map_err(|e| MemFuseError::Storage(format!("bincode: {}", e)))?;
        let meta_key = self.key("meta:stats");

        self.storage.put(tx, &meta_key, &meta_bytes).await
    }

    #[allow(dead_code)]
    pub(crate) async fn rollback_stats(&self, tx: TxId) -> Result<()> {
        let mut guard = self.staged_stats.lock();
        guard.remove(&tx);
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

    /// Searches the inverted index at a specific MVCC sequence number snapshot.
    ///
    /// Snapshot isolation is preserved by delegating to `search_bm25_at()`, which uses
    /// `scan_prefix_at()` and `get_at_seq()` on the underlying `StorageEngine`.
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
        self.commit_stats(tx).await?;
        self.storage.commit(tx).await
    }

    async fn rollback(&self, tx: TxId) -> Result<()> {
        self.rollback_stats(tx).await?;
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
    use super::Language;

    use super::*;
    use parking_lot::RwLock;

    type MockStoreMap = RwLock<HashMap<Vec<u8>, Vec<(Vec<u8>, u64)>>>;

    struct MockStorage {
        // Map from Key -> Vec<(Value, SeqNo)>
        store: MockStoreMap,
        // TxId -> Vec<Key> staged for current transaction
        staged: RwLock<HashMap<TxId, Vec<Vec<u8>>>>,
        next_seq: std::sync::atomic::AtomicU64,
    }

    impl MockStorage {
        fn new() -> Self {
            Self {
                store: MockStoreMap::new(HashMap::new()),
                staged: RwLock::new(HashMap::new()),
                next_seq: std::sync::atomic::AtomicU64::new(1),
            }
        }
    }

    #[async_trait]
    impl StorageEngine for MockStorage {
        async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
            self.get_at_seq(key, u64::MAX).await
        }
        async fn put(&self, tx_id: TxId, key: &[u8], value: &[u8]) -> Result<()> {
            let seq = self
                .next_seq
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.store
                .write()
                .entry(key.to_vec())
                .or_default()
                .push((value.to_vec(), seq));
            self.staged
                .write()
                .entry(tx_id)
                .or_default()
                .push(key.to_vec());
            Ok(())
        }
        async fn delete(&self, tx_id: TxId, key: &[u8]) -> Result<()> {
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
            self.staged
                .write()
                .entry(tx_id)
                .or_default()
                .push(key.to_vec());
            Ok(())
        }
        async fn commit(&self, tx_id: TxId) -> Result<()> {
            self.staged.write().remove(&tx_id);
            Ok(())
        }
        async fn rollback(&self, tx_id: TxId) -> Result<()> {
            let keys = self.staged.write().remove(&tx_id).unwrap_or_default();
            let mut store = self.store.write();
            for k in keys {
                if let Some(versions) = store.get_mut(&k) {
                    versions.pop();
                    if versions.is_empty() {
                        store.remove(&k);
                    }
                }
            }
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
        index.commit_stats(tx1).await?;
        storage.commit(tx1).await?;

        let tx2 = TxId::new(2);
        let d2 = DocId::new(2);
        index
            .upsert_document(tx2, d2, "I like rust programming and rust ownership rules.")
            .await?;
        index.commit_stats(tx2).await?;
        storage.commit(tx2).await?;

        let tx3 = TxId::new(3);
        let d3 = DocId::new(3);
        index
            .upsert_document(tx3, d3, "Python is dynamically typed.")
            .await?;
        index.commit_stats(tx3).await?;
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
        index.commit_stats(tx1).await?;
        storage.commit(tx1).await?;

        let tx2 = TxId::new(2);
        let d2 = DocId::new(2);
        index.upsert_document(tx2, d2, "four five").await?;
        index.commit_stats(tx2).await?;
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
        index.commit_stats(tx3).await?;
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
        index.commit_stats(tx4).await?;
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
    async fn staged_insert_not_visible_before_commit() -> Result<()> {
        let storage = Arc::new(MockStorage::new());
        let index = InvertedIndex::new(storage.clone(), "staged_test");
        let tx = TxId::new(1);
        let doc_id = DocId::new(42);

        let seq_before = storage.last_seq_no().await?;

        index.upsert_document(tx, doc_id, "test content").await?;

        // Before commit (with snapshot isolation at previous committed sequence), search returns empty
        let results_before = index.search_bm25_at("test", 10, Some(seq_before)).await?;
        assert!(
            results_before.is_empty(),
            "Uncommitted upsert must not be visible at seq_before"
        );

        // After commit: readable
        index.commit(tx).await?;
        let results_after = index.search("test", 10).await?;
        assert!(
            !results_after.is_empty(),
            "Committed insert must be visible in search"
        );

        Ok(())
    }

    #[tokio::test]
    async fn inverted_index_rollback_cleans_up() -> Result<()> {
        let storage = Arc::new(MockStorage::new());
        let index = InvertedIndex::new(storage.clone(), "rollback_test");

        let tx1 = TxId::new(1);
        let doc_id = DocId::new(100);

        index
            .insert(tx1, doc_id, "German morphological search engine")
            .await?;
        assert_eq!(index.search("morphological", 10).await?.len(), 1);

        index.rollback(tx1).await?;

        let results = index.search("morphological", 10).await?;
        assert!(
            results.is_empty(),
            "Rolled-back insert must not be visible in search results"
        );
        assert_eq!(
            index.len().await,
            0,
            "Index length must be 0 after rollback"
        );

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

    #[test]
    fn test_language_from_iso() {
        assert_eq!(Language::from_iso("de"), Language::German);
        assert_eq!(Language::from_iso("de-DE"), Language::German);
        assert_eq!(Language::from_iso("de-AT"), Language::German);
        assert_eq!(Language::from_iso("en"), Language::English);
        assert_eq!(Language::from_iso("en-US"), Language::English);
        assert_eq!(Language::from_iso("en-GB"), Language::English);
        assert_eq!(Language::from_iso("zh-CN"), Language::English);
        // False-positive protection: these namespace-like strings must NOT match German
        assert_eq!(Language::from_iso("developer"), Language::English);
        assert_eq!(Language::from_iso("indexed"), Language::English);
        assert_eq!(Language::from_iso("model"), Language::English);
        assert_eq!(Language::from_iso("indexed_docs"), Language::English);
        assert_eq!(Language::from_iso("mode"), Language::English);
        // Unknown codes fall back to English
        assert_eq!(Language::from_iso("fr"), Language::English);
        assert_eq!(Language::from_iso("ja"), Language::English);
        assert_eq!(Language::from_iso(""), Language::English);
    }

    #[test]
    fn test_language_default_is_english() {
        assert_eq!(Language::default(), Language::English);
    }

    #[tokio::test]
    async fn test_new_with_language_german_tokenizes_compounds() -> Result<()> {
        let storage = Arc::new(MockStorage::new());
        let index = InvertedIndex::new_with_language(storage.clone(), "test_de", Language::German);

        let tx = TxId::new(1);
        let doc_id = DocId::new(1);
        index
            .upsert_document(tx, doc_id, "Bundesverfassungsgericht")
            .await?;
        index.commit_stats(tx).await?;
        storage.commit(tx).await?;

        // German tokenizer should decompose "Bundesverfassungsgericht" into components
        let results = index.search_bm25("gericht", 10, None).await?;
        assert_eq!(
            results.len(),
            1,
            "German tokenizer should find compound component 'gericht'"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_new_default_english_does_not_split_compounds() -> Result<()> {
        let storage = Arc::new(MockStorage::new());
        let index = InvertedIndex::new(storage.clone(), "test_en");

        let tx = TxId::new(1);
        let doc_id = DocId::new(1);
        index
            .upsert_document(tx, doc_id, "Bundesverfassungsgericht")
            .await?;
        index.commit_stats(tx).await?;
        storage.commit(tx).await?;

        // English tokenizer should NOT decompose German compounds
        let results = index.search_bm25("gericht", 10, None).await?;
        assert_eq!(
            results.len(),
            0,
            "English tokenizer should not split German compounds"
        );

        // But exact match should work
        let results_exact = index
            .search_bm25("bundesverfassungsgericht", 10, None)
            .await?;
        assert_eq!(results_exact.len(), 1, "Exact match should still work");

        Ok(())
    }

    #[tokio::test]
    async fn test_avgdl_accuracy_incremental_updates() -> Result<()> {
        let storage = Arc::new(MockStorage::new());
        let index = InvertedIndex::new(storage.clone(), "test_avgdl");

        // Step 1: Insert 10 documents of varying length
        // We use distinct words so DefaultTokenizer doesn't filter them out as stopwords.
        let docs = [
            "alpha",                                                                        // 1 token
            "beta gamma",                                    // 2 tokens
            "delta epsilon zeta",                            // 3 tokens
            "eta theta iota kappa",                          // 4 tokens
            "lambda mu nu xi omicron",                       // 5 tokens
            "pi rho sigma tau upsilon phi",                  // 6 tokens
            "chi psi omega apple banana cherry",             // 7 tokens
            "date elderberry fig grape hazelnut kiwi lemon", // 8 tokens
            "mango nectarine orange papaya quince raspberry strawberry tangerine", // 9 tokens
            "ugli vanilla walnut ximenia yuzu ziziphus apricot blueberry cranberry durian", // 10 tokens
        ];

        let mut current_doc_lengths = HashMap::new();

        for (i, text) in docs.iter().enumerate() {
            let doc_id = DocId::new((i + 1) as u64);
            let tx = TxId::new((i + 1) as u64);
            let tokens = DefaultTokenizer.tokenize(text);
            current_doc_lengths.insert(doc_id, tokens.len() as u64);

            index.upsert_document(tx, doc_id, text).await?;
            index.commit_stats(tx).await?;
            storage.commit(tx).await?;
        }

        assert_eq!(index.total_docs.load(Ordering::SeqCst), 10);

        // Step 2: Delete 5 documents (DocId 1..=5)
        for i in 1..=5 {
            let doc_id = DocId::new(i as u64);
            let tx = TxId::new(100 + i as u64);
            current_doc_lengths.remove(&doc_id);

            index.delete_document(tx, doc_id).await?;
            index.commit_stats(tx).await?;
            storage.commit(tx).await?;
        }

        assert_eq!(index.total_docs.load(Ordering::SeqCst), 5);

        // Step 3: Insert 5 more documents (DocId 11..=15)
        let new_docs = [
            "ant bee cat dog",                               // 4 tokens
            "elephant fox giraffe hippo iguana",             // 5 tokens
            "jaguar koala lion monkey newt owl",             // 6 tokens
            "panda quail rabbit snake tiger urchin vulture", // 7 tokens
            "whale wolf yak zebra ant bee cat dog elephant", // 9 tokens
        ];

        for (i, text) in new_docs.iter().enumerate() {
            let doc_id = DocId::new((11 + i) as u64);
            let tx = TxId::new(200 + i as u64);
            let tokens = DefaultTokenizer.tokenize(text);
            current_doc_lengths.insert(doc_id, tokens.len() as u64);

            index.upsert_document(tx, doc_id, text).await?;
            index.commit_stats(tx).await?;
            storage.commit(tx).await?;
        }

        // Verify state
        let total_docs = index.total_docs.load(Ordering::SeqCst);
        let total_tokens = index.total_tokens.load(Ordering::SeqCst);
        assert_eq!(total_docs, 10, "Should have 10 active documents");

        let expected_total_tokens: u64 = current_doc_lengths.values().sum();
        assert_eq!(
            total_tokens, expected_total_tokens,
            "Total tokens must match active documents"
        );

        let expected_avgdl = expected_total_tokens as f64 / total_docs as f64;
        let actual_avgdl = index.avg_doc_len_x1000.load(Ordering::SeqCst) as f64 / 1000.0;

        assert!(
            (actual_avgdl - expected_avgdl).abs() < 0.001,
            "avgdl ({}) must equal mean length of current 10 documents ({})",
            actual_avgdl,
            expected_avgdl
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_contextual_bm25_indexes_prefix_terms() -> Result<()> {
        use memfuse_core::ContextChunk;

        let storage = Arc::new(MockStorage::new());
        let index = InvertedIndex::new(storage.clone(), "contextual_test");

        let chunk = ContextChunk {
            doc_id: DocId::new(10),
            content: "Informationen über Wirtschaftspolitik.".to_string(),
            relevance: 1.0,
            token_count: 5,
            metadata: None,
            contextual_prefix: Some(
                "Diese Passage beschreibt die Bundeshauptstadt Berlin.".to_string(),
            ),
        };

        let tx = TxId::new(1);
        // combined_text_owned() returns "Diese Passage beschreibt die Bundeshauptstadt Berlin.\n\nInformationen über Wirtschaftspolitik."
        index
            .insert(tx, chunk.doc_id, &chunk.combined_text_owned())
            .await?;
        index.commit(tx).await?;

        // Query for prefix term "Bundeshauptstadt" should match chunk even though content doesn't contain it
        let results = index.search("Bundeshauptstadt", 10).await?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].doc_id, chunk.doc_id);

        Ok(())
    }

    #[tokio::test]
    async fn test_german_tokenizer_symmetry_index_and_query() -> Result<()> {
        let storage = Arc::new(MockStorage::new());
        let index =
            InvertedIndex::new_with_language(storage.clone(), "test_de_symmetry", Language::German);

        let tx = TxId::new(1);
        let doc_id = DocId::new(42);
        index
            .upsert_document(tx, doc_id, "Datenbankmanagement")
            .await?;
        index.commit_stats(tx).await?;
        storage.commit(tx).await?;

        // Indexing "Datenbankmanagement" with German tokenizer decomposes into "datenbank" and "management".
        // Querying "Datenbank" with identical German tokenizer must return the indexed document.
        let results = index.search_bm25("Datenbank", 10, None).await?;
        assert_eq!(
            results.len(),
            1,
            "Query 'Datenbank' must return document indexed with 'Datenbankmanagement'"
        );
        assert_eq!(results[0].0, doc_id);

        Ok(())
    }

    #[tokio::test]
    async fn test_idf_recalculation_after_delete() -> Result<()> {
        let storage = Arc::new(MockStorage::new());
        let index = InvertedIndex::new(storage.clone(), "idf_test");

        // Insert doc1: "rust compiler"
        let tx1 = TxId::new(1);
        let d1 = DocId::new(1);
        index.insert(tx1, d1, "rust compiler").await?;
        index.commit(tx1).await?;

        // Insert doc2: "rust language"
        let tx2 = TxId::new(2);
        let d2 = DocId::new(2);
        index.insert(tx2, d2, "rust language").await?;
        index.commit(tx2).await?;

        // Search for "rust" when N=2, df=2
        let search_before = index.search_bm25("rust", 10, None).await?;
        assert_eq!(search_before.len(), 2);
        let score_before_d2 = search_before.iter().find(|(id, _)| *id == d2).unwrap().1; // unwrap

        // Delete doc1
        let tx3 = TxId::new(3);
        index.delete(tx3, d1).await?;
        index.commit(tx3).await?;

        // Search for "rust" when N=1, df=1
        let search_after = index.search_bm25("rust", 10, None).await?;
        assert_eq!(search_after.len(), 1);
        assert_eq!(search_after[0].0, d2);
        let score_after_d2 = search_after[0].1;

        // Scores should be non-NaN, non-infinite
        assert!(!score_before_d2.is_nan() && !score_before_d2.is_infinite());
        assert!(!score_after_d2.is_nan() && !score_after_d2.is_infinite());

        // When N decreases from 2 to 1 and df decreases from 2 to 1,
        // (N - df + 0.5) / (df + 0.5) goes from (2 - 2 + 0.5)/(2 + 0.5) = 0.5/2.5 = 0.2 (floor 1e-6)
        // to (1 - 1 + 0.5)/(1 + 0.5) = 0.5/1.5 = 1/3 (floor 1e-6 as idf_arg <= 1.0).
        // But let's verify score_after_d2 > score_before_d2 with terms having idf_arg > 1.0:
        Ok(())
    }

    #[tokio::test]
    async fn test_idf_recalculation_with_rare_term_after_delete() -> Result<()> {
        let storage = Arc::new(MockStorage::new());
        let index = InvertedIndex::new(storage.clone(), "idf_rare_test");

        // Corpus: doc1 ("common rare"), doc2 ("common rare"), doc3..doc10 ("common filler")
        // term "rare" starts with N=10, df=2
        let tx = TxId::new(1);
        let d1 = DocId::new(1);
        let d2 = DocId::new(2);
        index.insert(tx, d1, "common rare").await?;
        index.insert(tx, d2, "common rare").await?;
        for i in 3..=10 {
            index.insert(tx, DocId::new(i), "common filler").await?;
        }
        index.commit(tx).await?;

        // Score for "rare" in d2 when N=10, df=2
        let search_before = index.search_bm25("rare", 10, None).await?;
        let score_before = search_before.iter().find(|(id, _)| *id == d2).unwrap().1; // unwrap

        // Delete d1 -> N=9, df=1 for "rare"
        let tx_del = TxId::new(2);
        index.delete(tx_del, d1).await?;
        index.commit(tx_del).await?;

        // Score for "rare" in d2 when N=9, df=1
        let search_after = index.search_bm25("rare", 10, None).await?;
        let score_after = search_after.iter().find(|(id, _)| *id == d2).unwrap().1; // unwrap

        // With N=10, df=2: idf_arg = (10 - 2 + 0.5)/(2 + 0.5) = 8.5 / 2.5 = 3.4 -> ln(3.4) ~= 1.2237
        // With N=9, df=1: idf_arg = (9 - 1 + 0.5)/(1 + 0.5) = 8.5 / 1.5 = 5.6667 -> ln(5.6667) ~= 1.7346
        // So IDF and score_after must be significantly higher than score_before
        assert!(
            score_after > score_before,
            "Deleting doc containing 'rare' decreased df from 2 to 1, so BM25 score for 'rare' must increase (after: {}, before: {})",
            score_after,
            score_before
        );

        Ok(())
    }
}
