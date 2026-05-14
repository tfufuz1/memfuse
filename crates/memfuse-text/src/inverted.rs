//! LSM-backed Inverted Index.

use crate::tokenizer::{DefaultTokenizer, GermanMorphTokenizer, Tokenizer};
use memfuse_core::{DocId, MemFuseError, Result, StorageEngine, TxId};
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
            Arc::new(GermanMorphTokenizer)
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
        let mut k = self.prefix.clone();
        k.extend_from_slice(suffix.as_bytes());
        k
    }

    /// Appends and updates inverted index structures for a document.
    pub async fn upsert_document(&self, tx: TxId, doc_id: DocId, text: &str) -> Result<()> {
        let tokens = self.tokenizer.tokenize(text);
        let new_len = tokens.len() as u32;

        let mut tfs = HashMap::new();
        for t in &tokens {
            *tfs.entry(t.clone()).or_insert(0u32) += 1;
        }

        // Check if document already exists to adjust total_tokens and total_docs
        let dl_key = self.key(&format!("dl:{}", doc_id.inner()));
        let fw_key = self.key(&format!("fw:{}", doc_id.inner()));
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

                // Remove from old posting lists if update
                if let Some(fw_bytes) = self.storage.get(&fw_key).await? {
                    let config = bincode::config::standard();
                    if let Ok((old_terms, _)) =
                        bincode::serde::decode_from_slice::<Vec<String>, _>(&fw_bytes, config)
                    {
                        for term in old_terms {
                            let pl_key = self.key(&format!("pl:{}", term));
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

        // Store forward index (unique terms)
        let mut unique_terms: Vec<String> = tfs.keys().cloned().collect();
        unique_terms.sort();
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
        for (term, tf) in tfs {
            let pl_key = self.key(&format!("pl:{}", term));
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
        let dl_key = self.key(&format!("dl:{}", doc_id.inner()));
        let fw_key = self.key(&format!("fw:{}", doc_id.inner()));

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
            // Document doesn't exist in inverted index
            return Ok(());
        }

        self.storage.delete(tx, &dl_key).await?;

        // Remove from posting lists using forward index
        if let Some(fw_bytes) = self.storage.get(&fw_key).await? {
            let config = bincode::config::standard();
            if let Ok((old_terms, _)) =
                bincode::serde::decode_from_slice::<Vec<String>, _>(&fw_bytes, config)
            {
                for term in old_terms {
                    let pl_key = self.key(&format!("pl:{}", term));
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

        for term in &tokens {
            let pl_key = self.key(&format!("pl:{}", term));
            if let Some(bytes) = self.storage.get(&pl_key).await? {
                let config = bincode::config::standard();
                if let Ok((pl, _)) =
                    bincode::serde::decode_from_slice::<Vec<(DocId, u32)>, _>(&bytes, config)
                {
                    let df = pl.len() as u32;

                    for (doc_id, tf) in pl {
                        // Fetch doc length
                        let dl_key = self.key(&format!("dl:{}", doc_id.inner()));
                        let mut doc_len = 0u32;
                        if let Some(dl_bytes) = self.storage.get(&dl_key).await? {
                            if dl_bytes.len() == 4 {
                                doc_len =
                                    u32::from_le_bytes(dl_bytes.as_slice().try_into().map_err(
                                        |_| MemFuseError::Storage("Invalid doc_len length".into()),
                                    )?);
                            }
                        }

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

#[cfg(test)]
mod tests {
    use super::*;
    use memfuse_store::{LsmConfig, LsmStorage};
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_bm25_ranks_exact_keyword_higher(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let tmp = TempDir::new()?;
        let config = LsmConfig {
            path: tmp.path().to_path_buf(),
            ..Default::default()
        };
        let storage = Arc::new(LsmStorage::new(config).await?);

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
        let tmp = TempDir::new()?;
        let config = LsmConfig {
            path: tmp.path().to_path_buf(),
            ..Default::default()
        };
        let storage = Arc::new(LsmStorage::new(config).await?);
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
        let tmp = TempDir::new()?;
        let config = LsmConfig {
            path: tmp.path().to_path_buf(),
            ..Default::default()
        };
        let storage = Arc::new(LsmStorage::new(config).await?);
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
}
