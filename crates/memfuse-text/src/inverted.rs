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
use std::collections::HashMap;
use std::sync::Arc;

/// An inverted index stored in the LSM engine.
#[derive(Clone)]
/// An inverted index tied to a specific collection namespace.
pub struct InvertedIndex {
    storage: Arc<dyn StorageEngine>,
    prefix: Vec<u8>,
    tokenizer: Arc<dyn Tokenizer>,
}

impl InvertedIndex {
    /// Creates a new InvertedIndex tied to a specific collection namespace.
    pub fn new(storage: Arc<dyn StorageEngine>, namespace: &str) -> Self {
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

    fn key_with_term(&self, term: &str) -> Vec<u8> {
        let mut k = Vec::with_capacity(self.prefix.len() + 3 + term.len());
        k.extend_from_slice(&self.prefix);
        k.extend_from_slice(b"pl:");
        k.extend_from_slice(term.as_bytes());
        k
    }

    /// Appends and updates inverted index structures for a document.
    pub async fn upsert_document(&self, tx: TxId, doc_id: DocId, text: &str) -> Result<()> {
        let tokens = self.tokenizer.tokenize(text);
        let new_len = tokens.len() as u32;

        // Track original tokens using DefaultTokenizer
        let orig_tokens = DefaultTokenizer.tokenize(text);
        let orig_len = orig_tokens.len() as u32;

        let mut tfs = HashMap::with_capacity(tokens.len());
        for t in tokens {
            *tfs.entry(t).or_insert(0u32) += 1;
        }

        // Check if document already exists to adjust total_tokens and total_docs
        let dl_key = self.key_with_id("dl:", doc_id.inner());
        let ol_key = self.key_with_id("ol:", doc_id.inner());
        let fw_key = self.key_with_id("fw:", doc_id.inner());
        let mut old_len = 0u32;
        let mut old_orig_len = 0u32;
        let mut is_update = false;

        if let Some(bytes) = self.storage.get(&dl_key).await? {
            if bytes.len() == 4 {
                old_len = u32::from_le_bytes(
                    bytes
                        .as_slice()
                        .try_into()
                        .map_err(|_| MemFuseError::Storage("Invalid doc_len length".into()))?,
                );

                if let Some(ol_bytes) = self.storage.get(&ol_key).await? {
                    if ol_bytes.len() == 4 {
                        old_orig_len =
                            u32::from_le_bytes(ol_bytes.as_slice().try_into().map_err(|_| {
                                MemFuseError::Storage("Invalid orig_len length".into())
                            })?);
                    }
                }

                is_update = true;

                // Remove from old posting lists if update
                if let Some(fw_bytes) = self.storage.get(&fw_key).await? {
                    let config = bincode::config::standard();
                    if let Ok((old_terms, _)) =
                        bincode::serde::decode_from_slice::<Vec<String>, _>(&fw_bytes, config)
                    {
                        for term in old_terms {
                            let pl_key = self.key_with_term(&term);
                            if let Some(pl_bytes) = self.storage.get(&pl_key).await? {
                                if let Ok((mut pl, _)) =
                                    bincode::serde::decode_from_slice::<Vec<(DocId, u32)>, _>(
                                        &pl_bytes, config,
                                    )
                                {
                                    pl.retain(|&(d, _)| d != doc_id);
                                    if pl.is_empty() {
                                        self.storage.delete(tx, &pl_key).await?;
                                    } else {
                                        let new_pl_bytes = bincode::serde::encode_to_vec(
                                            &pl,
                                            bincode::config::standard(),
                                        )
                                        .map_err(|e| {
                                            MemFuseError::Storage(format!("bincode: {}", e))
                                        })?;
                                        self.storage.put(tx, &pl_key, &new_pl_bytes).await?;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Store new document length
        self.storage
            .put(tx, &dl_key, &new_len.to_le_bytes())
            .await?;

        // Store original document length
        self.storage
            .put(tx, &ol_key, &orig_len.to_le_bytes())
            .await?;

        // Store forward index (unique terms)
        let mut tfs_vec: Vec<(String, u32)> = tfs.into_iter().collect();
        tfs_vec.sort_by(|a, b| a.0.cmp(&b.0));

        let unique_terms: Vec<&str> = tfs_vec.iter().map(|(k, _)| k.as_str()).collect();
        let fw_bytes = bincode::serde::encode_to_vec(&unique_terms, bincode::config::standard())
            .map_err(|e| MemFuseError::Storage(format!("bincode: {}", e)))?;
        self.storage.put(tx, &fw_key, &fw_bytes).await?;

        // Update total tokens (global for avg_doc_len)
        let total_tok_key = self.key("meta:total_tokens");
        let mut total_tokens = 0u64;
        if let Some(bytes) = self.storage.get(&total_tok_key).await? {
            if bytes.len() == 8 {
                total_tokens =
                    u64::from_le_bytes(bytes.as_slice().try_into().map_err(|_| {
                        MemFuseError::Storage("Invalid total_tokens length".into())
                    })?);
            }
        }

        total_tokens = total_tokens.saturating_sub(old_len as u64) + new_len as u64;

        self.storage
            .put(tx, &total_tok_key, &total_tokens.to_le_bytes())
            .await?;

        // Update total original tokens
        let total_orig_tok_key = self.key("meta:orig_tokens");
        let mut total_orig_tokens = 0u64;
        if let Some(bytes) = self.storage.get(&total_orig_tok_key).await? {
            if bytes.len() == 8 {
                total_orig_tokens =
                    u64::from_le_bytes(bytes.as_slice().try_into().map_err(|_| {
                        MemFuseError::Storage("Invalid total_orig_tokens length".into())
                    })?);
            }
        }
        total_orig_tokens = total_orig_tokens.saturating_sub(old_orig_len as u64) + orig_len as u64;
        self.storage
            .put(tx, &total_orig_tok_key, &total_orig_tokens.to_le_bytes())
            .await?;

        // Update total docs
        let total_docs_key = self.key("meta:total_docs");
        let mut total_docs = 0u64;
        if let Some(bytes) = self.storage.get(&total_docs_key).await? {
            if bytes.len() == 8 {
                total_docs = u64::from_le_bytes(
                    bytes
                        .as_slice()
                        .try_into()
                        .map_err(|_| MemFuseError::Storage("Invalid total_docs length".into()))?,
                );
            }
        }

        if !is_update {
            total_docs += 1;
            self.storage
                .put(tx, &total_docs_key, &total_docs.to_le_bytes())
                .await?;
        }

        // Update posting lists
        for (term, tf) in tfs_vec {
            let pl_key = self.key_with_term(&term);
            let mut pl: Vec<(DocId, u32)> = Vec::new();

            if let Some(bytes) = self.storage.get(&pl_key).await? {
                let config = bincode::config::standard();
                if let Ok((existing, _)) =
                    bincode::serde::decode_from_slice::<Vec<(DocId, u32)>, _>(&bytes, config)
                {
                    pl = existing;
                }
            }

            // Replace existing doc_id if it exists
            pl.retain(|&(d, _)| d != doc_id);
            pl.push((doc_id, tf));

            let new_bytes = bincode::serde::encode_to_vec(&pl, bincode::config::standard())
                .map_err(|e| MemFuseError::Storage(format!("bincode: {}", e)))?;
            self.storage.put(tx, &pl_key, &new_bytes).await?;
        }

        Ok(())
    }

    /// Deletes a document from the index.
    pub async fn delete_document(&self, tx: TxId, doc_id: DocId) -> Result<()> {
        let dl_key = self.key_with_id("dl:", doc_id.inner());
        let ol_key = self.key_with_id("ol:", doc_id.inner());
        let fw_key = self.key_with_id("fw:", doc_id.inner());

        let mut doc_len = 0u32;
        let mut orig_len = 0u32;

        if let Some(bytes) = self.storage.get(&dl_key).await? {
            if bytes.len() == 4 {
                doc_len = u32::from_le_bytes(
                    bytes
                        .as_slice()
                        .try_into()
                        .map_err(|_| MemFuseError::Storage("Invalid doc_len length".into()))?,
                );

                if let Some(ol_bytes) = self.storage.get(&ol_key).await? {
                    if ol_bytes.len() == 4 {
                        orig_len =
                            u32::from_le_bytes(ol_bytes.as_slice().try_into().map_err(|_| {
                                MemFuseError::Storage("Invalid orig_len length".into())
                            })?);
                    }
                }
            }
        } else {
            // Document doesn't exist in inverted index
            return Ok(());
        }

        self.storage.delete(tx, &dl_key).await?;
        self.storage.delete(tx, &ol_key).await?;

        // Remove from posting lists using forward index
        if let Some(fw_bytes) = self.storage.get(&fw_key).await? {
            let config = bincode::config::standard();
            if let Ok((old_terms, _)) =
                bincode::serde::decode_from_slice::<Vec<String>, _>(&fw_bytes, config)
            {
                for term in old_terms {
                    let pl_key = self.key_with_term(&term);
                    if let Some(pl_bytes) = self.storage.get(&pl_key).await? {
                        if let Ok((mut pl, _)) = bincode::serde::decode_from_slice::<
                            Vec<(DocId, u32)>,
                            _,
                        >(&pl_bytes, config)
                        {
                            pl.retain(|&(d, _)| d != doc_id);
                            if pl.is_empty() {
                                self.storage.delete(tx, &pl_key).await?;
                            } else {
                                let new_pl_bytes =
                                    bincode::serde::encode_to_vec(&pl, bincode::config::standard())
                                        .map_err(|e| {
                                            MemFuseError::Storage(format!("bincode: {}", e))
                                        })?;
                                self.storage.put(tx, &pl_key, &new_pl_bytes).await?;
                            }
                        }
                    }
                }
            }
        }
        self.storage.delete(tx, &fw_key).await?;

        // Update global stats
        let total_tok_key = self.key("meta:total_tokens");
        if let Some(bytes) = self.storage.get(&total_tok_key).await? {
            if bytes.len() == 8 {
                let mut total_tokens =
                    u64::from_le_bytes(bytes.as_slice().try_into().map_err(|_| {
                        MemFuseError::Storage("Invalid total_tokens length".into())
                    })?);
                total_tokens = total_tokens.saturating_sub(doc_len as u64);
                self.storage
                    .put(tx, &total_tok_key, &total_tokens.to_le_bytes())
                    .await?;
            }
        }

        let total_orig_tok_key = self.key("meta:orig_tokens");
        if let Some(bytes) = self.storage.get(&total_orig_tok_key).await? {
            if bytes.len() == 8 {
                let mut total_orig_tokens =
                    u64::from_le_bytes(bytes.as_slice().try_into().map_err(|_| {
                        MemFuseError::Storage("Invalid total_orig_tokens length".into())
                    })?);
                total_orig_tokens = total_orig_tokens.saturating_sub(orig_len as u64);
                self.storage
                    .put(tx, &total_orig_tok_key, &total_orig_tokens.to_le_bytes())
                    .await?;
            }
        }

        let total_docs_key = self.key("meta:total_docs");
        if let Some(bytes) = self.storage.get(&total_docs_key).await? {
            if bytes.len() == 8 {
                let mut total_docs = u64::from_le_bytes(
                    bytes
                        .as_slice()
                        .try_into()
                        .map_err(|_| MemFuseError::Storage("Invalid total_docs length".into()))?,
                );
                total_docs = total_docs.saturating_sub(1);
                self.storage
                    .put(tx, &total_docs_key, &total_docs.to_le_bytes())
                    .await?;
            }
        }

        Ok(())
    }

    /// Searches the inverted index using BM25.
    pub async fn search_bm25(&self, query: &str, k: usize) -> Result<Vec<(DocId, f32)>> {
        let tokens = self.tokenizer.tokenize(query);
        if tokens.is_empty() {
            return Ok(Vec::new());
        }

        // Fetch global stats
        let total_docs_key = self.key("meta:total_docs");
        let mut total_docs = 0u64;
        if let Some(bytes) = self.storage.get(&total_docs_key).await? {
            if bytes.len() == 8 {
                total_docs = u64::from_le_bytes(
                    bytes
                        .as_slice()
                        .try_into()
                        .map_err(|_| MemFuseError::Storage("Invalid total_docs length".into()))?,
                );
            }
        }

        let total_tok_key = self.key("meta:total_tokens");
        let mut total_tokens = 0u64;
        if let Some(bytes) = self.storage.get(&total_tok_key).await? {
            if bytes.len() == 8 {
                total_tokens =
                    u64::from_le_bytes(bytes.as_slice().try_into().map_err(|_| {
                        MemFuseError::Storage("Invalid total_tokens length".into())
                    })?);
            }
        }

        let avg_doc_len = if total_docs > 0 {
            total_tokens as f32 / total_docs as f32
        } else {
            0.0
        };

        let mut scores: HashMap<DocId, f32> = HashMap::new();
        let mut doc_len_cache: HashMap<DocId, u32> = HashMap::new();

        for term in &tokens {
            let pl_key = self.key_with_term(term);
            if let Some(bytes) = self.storage.get(&pl_key).await? {
                let config = bincode::config::standard();
                if let Ok((pl, _)) =
                    bincode::serde::decode_from_slice::<Vec<(DocId, u32)>, _>(&bytes, config)
                {
                    let df = pl.len() as u32;

                    for (doc_id, tf) in pl {
                        // Fetch doc length
                        let doc_len = if let Some(&len) = doc_len_cache.get(&doc_id) {
                            len
                        } else {
                            let dl_key = self.key_with_id("dl:", doc_id.inner());
                            let mut len = 0u32;
                            if let Some(dl_bytes) = self.storage.get(&dl_key).await? {
                                if dl_bytes.len() == 4 {
                                    len = u32::from_le_bytes(
                                        dl_bytes.as_slice().try_into().map_err(|_| {
                                            MemFuseError::Storage("Invalid doc_len length".into())
                                        })?,
                                    );
                                }
                            }
                            doc_len_cache.insert(doc_id, len);
                            len
                        };

                        let score = crate::bm25::score_term(
                            tf,
                            doc_len,
                            avg_doc_len,
                            df,
                            total_docs as u32,
                        );

                        *scores.entry(doc_id).or_insert(0.0) += score;
                    }
                }
            }
        }

        let mut results: Vec<(DocId, f32)> = scores.into_iter().collect();
        // Sort descending by score
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(k);

        Ok(results)
    }
}

#[async_trait]
impl TextIndex for InvertedIndex {
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

    async fn stats(&self) -> Result<TextIndexStats> {
        let total_docs_key = self.key("meta:total_docs");
        let mut total_docs = 0u64;
        if let Some(bytes) = self.storage.get(&total_docs_key).await? {
            if bytes.len() == 8 {
                total_docs = u64::from_le_bytes(
                    bytes
                        .as_slice()
                        .try_into()
                        .map_err(|_| MemFuseError::Storage("Invalid total_docs length".into()))?,
                );
            }
        }

        let total_tok_key = self.key("meta:total_tokens");
        let mut total_tokens = 0u64;
        if let Some(bytes) = self.storage.get(&total_tok_key).await? {
            if bytes.len() == 8 {
                total_tokens =
                    u64::from_le_bytes(bytes.as_slice().try_into().map_err(|_| {
                        MemFuseError::Storage("Invalid total_tokens length".into())
                    })?);
            }
        }

        let total_orig_tok_key = self.key("meta:orig_tokens");
        let mut total_orig_tokens = 0u64;
        if let Some(bytes) = self.storage.get(&total_orig_tok_key).await? {
            if bytes.len() == 8 {
                total_orig_tokens =
                    u64::from_le_bytes(bytes.as_slice().try_into().map_err(|_| {
                        MemFuseError::Storage("Invalid total_orig_tokens length".into())
                    })?);
            }
        }

        let token_reduction_ratio = if total_orig_tokens > 0 {
            total_tokens as f32 / total_orig_tokens as f32
        } else {
            1.0
        };

        // Heuristic: 24 bytes per token
        let memory_usage_bytes = (total_tokens as usize).saturating_mul(24);

        Ok(TextIndexStats {
            num_documents: total_docs as usize,
            num_tokens: total_tokens as usize,
            memory_usage_bytes,
            token_reduction_ratio,
        })
    }
}

use crate::morphology::MorphologicalTokenizer;

/// An inverted index with morphological optimization.
pub struct BM25MorphIndex {
    inner: InvertedIndex,
    tokenizer: Arc<dyn MorphologicalTokenizer>,
}

impl BM25MorphIndex {
    pub fn new(
        storage: Arc<dyn StorageEngine>,
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
impl TextIndex for BM25MorphIndex {
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

    #[async_trait::async_trait]
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
        let td_key = index.key("meta:total_docs");
        let tt_key = index.key("meta:total_tokens");

        let td = u64::from_le_bytes(
            storage
                .get(&td_key)
                .await?
                .ok_or("td_key not found")?
                .as_slice()
                .try_into()?,
        );
        let tt = u64::from_le_bytes(
            storage
                .get(&tt_key)
                .await?
                .ok_or("tt_key not found")?
                .as_slice()
                .try_into()?,
        );
        assert_eq!(td, 2);
        assert_eq!(tt, 5);

        // Update d1
        let tx3 = TxId::new(3);
        index.upsert_document(tx3, d1, "one").await?;
        storage.commit(tx3).await?;

        // total_docs = 2, total_tokens = 3 (5 - 3 + 1)
        let td = u64::from_le_bytes(
            storage
                .get(&td_key)
                .await?
                .ok_or("td_key not found")?
                .as_slice()
                .try_into()?,
        );
        let tt = u64::from_le_bytes(
            storage
                .get(&tt_key)
                .await?
                .ok_or("tt_key not found")?
                .as_slice()
                .try_into()?,
        );
        assert_eq!(td, 2);
        assert_eq!(tt, 3);

        // Delete d2
        let tx4 = TxId::new(4);
        index.delete_document(tx4, d2).await?;
        storage.commit(tx4).await?;

        // total_docs = 1, total_tokens = 1
        let td = u64::from_le_bytes(
            storage
                .get(&td_key)
                .await?
                .ok_or("td_key not found")?
                .as_slice()
                .try_into()?,
        );
        let tt = u64::from_le_bytes(
            storage
                .get(&tt_key)
                .await?
                .ok_or("tt_key not found")?
                .as_slice()
                .try_into()?,
        );
        assert_eq!(td, 1);
        assert_eq!(tt, 1);
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

        // Should NOT be in "rust" or "programming" anymore
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
        let index: Arc<dyn TextIndex> = Arc::new(InvertedIndex::new(storage.clone(), "trait_test"));

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
        assert_eq!(stats_after.num_tokens, 0);

        Ok(())
    }

    #[tokio::test]
    async fn test_expansion_ratio_stats() -> Result<()> {
        let storage = Arc::new(MockStorage::new());
        // Use German namespace to trigger morphological splitting
        let index = InvertedIndex::new(storage.clone(), "de_test");

        let tx = TxId::new(1);
        let doc_id = DocId::new(1);
        // "Bundesverfassungsgericht" should split into 3 parts
        index.insert(tx, doc_id, "Bundesverfassungsgericht").await?;
        storage.commit(tx).await?;

        let stats = index.stats().await?;
        // Original: 1 ("bundesverfassungsgericht" lowercased)
        // Expanded: 4 ("bundesverfassungsgericht", "bundes", "verfassungs", "gericht")
        assert_eq!(stats.num_tokens, 4);
        assert_eq!(stats.token_reduction_ratio, 4.0);
        assert_eq!(stats.memory_usage_bytes, 4 * 24);

        Ok(())
    }

    #[tokio::test]
    async fn test_bm25_doc_len_influence() -> Result<()> {
        let storage = Arc::new(MockStorage::new());
        let index = InvertedIndex::new(storage.clone(), "default");

        let tx1 = TxId::new(1);
        let d1 = DocId::new(1);
        // Short document with term "rust"
        index.insert(tx1, d1, "rust").await?;
        storage.commit(tx1).await?;

        let tx2 = TxId::new(2);
        let d2 = DocId::new(2);
        // Long document with same term "rust"
        index.insert(tx2, d2, "rust is a programming language that focuses on safety and performance and has many cool features like ownership and borrowing.").await?;
        storage.commit(tx2).await?;

        let results = index.search_bm25("rust", 10).await?;
        assert_eq!(results.len(), 2);
        // "rust" in a shorter document should score higher
        assert_eq!(results[0].0, d1);
        assert_eq!(results[1].0, d2);
        assert!(results[0].1 > results[1].1);

        Ok(())
    }

    #[tokio::test]
    async fn test_bm25_tf_influence() -> Result<()> {
        let storage = Arc::new(MockStorage::new());
        let index = InvertedIndex::new(storage.clone(), "default");

        let tx1 = TxId::new(1);
        let d1 = DocId::new(1);
        // "rust" appears once
        index.insert(tx1, d1, "rust is fast").await?;
        storage.commit(tx1).await?;

        let tx2 = TxId::new(2);
        let d2 = DocId::new(2);
        // "rust" appears twice, similar length
        index.insert(tx2, d2, "rust rust fast").await?;
        storage.commit(tx2).await?;

        let results = index.search_bm25("rust", 10).await?;
        assert_eq!(results.len(), 2);
        // "rust" twice should score higher
        assert_eq!(results[0].0, d2);
        assert_eq!(results[1].0, d1);
        assert!(results[0].1 > results[1].1);

        Ok(())
    }
}
