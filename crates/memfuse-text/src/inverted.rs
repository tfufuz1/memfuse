//! LSM-backed Inverted Index.

use crate::tokenizer::tokenize;
use memfuse_core::{DocId, MemFuseError, Result, StorageEngine, TxId};
use std::collections::HashMap;
use std::sync::Arc;

/// An inverted index stored in the LSM engine.
#[derive(Clone)]
pub struct InvertedIndex {
    storage: Arc<dyn StorageEngine>,
    prefix: Vec<u8>,
}

impl InvertedIndex {
    /// Creates a new InvertedIndex tied to a specific collection namespace.
    pub fn new(storage: Arc<dyn StorageEngine>, namespace: &str) -> Self {
        let prefix = if namespace == "default" {
            b"__txt:default:".to_vec()
        } else {
            format!("__txt:{}:", namespace).into_bytes()
        };
        Self { storage, prefix }
    }

    fn key(&self, suffix: &str) -> Vec<u8> {
        let mut k = self.prefix.clone();
        k.extend_from_slice(suffix.as_bytes());
        k
    }

    /// Appends and updates inverted index structures for a document.
    pub async fn upsert_document(
        &self,
        tx: TxId,
        doc_id: DocId,
        text: &str,
        old_text: Option<&str>,
    ) -> Result<()> {
        let tokens = tokenize(text);
        if tokens.is_empty() {
            // If new text has no tokens, we should effectively delete the document from index
            if let Some(ot) = old_text {
                return self.delete_document(tx, doc_id, ot).await;
            }
            return Ok(());
        }

        // If we are updating, remove old terms first
        if let Some(ot) = old_text {
            let old_tokens = tokenize(ot);
            let mut unique_old_terms = old_tokens;
            unique_old_terms.sort();
            unique_old_terms.dedup();

            for term in unique_old_terms {
                let pl_key = self.key(&format!("pl:{}", term));
                if let Some(bytes) = self.storage.get(&pl_key).await? {
                    let config = bincode::config::standard();
                    if let Ok((mut pl, _)) =
                        bincode::serde::decode_from_slice::<Vec<(DocId, u32)>, _>(&bytes, config)
                    {
                        pl.retain(|&(d, _)| d != doc_id);
                        if pl.is_empty() {
                            self.storage.delete(tx, &pl_key).await?;
                        } else {
                            let new_bytes =
                                bincode::serde::encode_to_vec(&pl, bincode::config::standard())
                                    .map_err(|e| {
                                        MemFuseError::Storage(format!("bincode: {}", e))
                                    })?;
                            self.storage.put(tx, &pl_key, &new_bytes).await?;
                        }
                    }
                }
            }
        }

        let mut tfs = HashMap::new();
        for t in &tokens {
            *tfs.entry(t.clone()).or_insert(0u32) += 1;
        }

        // Store document length
        let dl_key = self.key(&format!("dl:{}", doc_id.inner()));
        let mut is_new = true;
        let mut old_len = 0u32;

        if let Some(bytes) = self.storage.get(&dl_key).await? {
            if bytes.len() == 4 {
                old_len = u32::from_le_bytes(
                    bytes
                        .as_slice()
                        .try_into()
                        .map_err(|_| MemFuseError::Storage("Invalid doc_len length".into()))?,
                );
                is_new = false;
            }
        }

        self.storage
            .put(tx, &dl_key, &(tokens.len() as u32).to_le_bytes())
            .await?;

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
        if is_new {
            total_tokens += tokens.len() as u64;
        } else {
            total_tokens = total_tokens
                .saturating_sub(old_len as u64)
                .saturating_add(tokens.len() as u64);
        }
        self.storage
            .put(tx, &total_tok_key, &total_tokens.to_le_bytes())
            .await?;

        // Update total docs
        if is_new {
            let total_docs_key = self.key("meta:total_docs");
            let mut total_docs = 0u64;
            if let Some(bytes) = self.storage.get(&total_docs_key).await? {
                if bytes.len() == 8 {
                    total_docs =
                        u64::from_le_bytes(bytes.as_slice().try_into().map_err(|_| {
                            MemFuseError::Storage("Invalid total_docs length".into())
                        })?);
                }
            }
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
    pub async fn delete_document(
        &self,
        tx: TxId,
        doc_id: DocId,
        original_text: &str,
    ) -> Result<()> {
        let tokens = tokenize(original_text);

        let dl_key = self.key(&format!("dl:{}", doc_id.inner()));
        let mut old_len = 0u32;
        if let Some(bytes) = self.storage.get(&dl_key).await? {
            if bytes.len() == 4 {
                old_len = u32::from_le_bytes(
                    bytes
                        .as_slice()
                        .try_into()
                        .map_err(|_| MemFuseError::Storage("Invalid doc_len length".into()))?,
                );
            }
        }

        self.storage.delete(tx, &dl_key).await?;

        // Update total tokens
        let total_tok_key = self.key("meta:total_tokens");
        if let Some(bytes) = self.storage.get(&total_tok_key).await? {
            if bytes.len() == 8 {
                let mut total_tokens =
                    u64::from_le_bytes(bytes.as_slice().try_into().map_err(|_| {
                        MemFuseError::Storage("Invalid total_tokens length".into())
                    })?);
                total_tokens = total_tokens.saturating_sub(old_len as u64);
                self.storage
                    .put(tx, &total_tok_key, &total_tokens.to_le_bytes())
                    .await?;
            }
        }

        // Update total docs
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

        let mut unique_terms = tokens;
        unique_terms.sort();
        unique_terms.dedup();

        for term in unique_terms {
            let pl_key = self.key(&format!("pl:{}", term));
            if let Some(bytes) = self.storage.get(&pl_key).await? {
                let config = bincode::config::standard();
                if let Ok((mut pl, _)) =
                    bincode::serde::decode_from_slice::<Vec<(DocId, u32)>, _>(&bytes, config)
                {
                    pl.retain(|&(d, _)| d != doc_id);
                    if pl.is_empty() {
                        self.storage.delete(tx, &pl_key).await?;
                    } else {
                        let new_bytes =
                            bincode::serde::encode_to_vec(&pl, bincode::config::standard())
                                .map_err(|e| MemFuseError::Storage(format!("bincode: {}", e)))?;
                        self.storage.put(tx, &pl_key, &new_bytes).await?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Searches the inverted index using BM25.
    pub async fn search_bm25(&self, query: &str, k: usize) -> Result<Vec<(DocId, f32)>> {
        let tokens = tokenize(query);
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
    async fn test_bm25_ranks_exact_keyword_higher() {
        let tmp = TempDir::new().expect("tmp");
        let config = LsmConfig {
            path: tmp.path().to_path_buf(),
            ..Default::default()
        };
        let storage = Arc::new(LsmStorage::new(config).await.expect("storage"));

        let index = InvertedIndex::new(storage.clone(), "default");

        let tx1 = TxId::new(1);
        let d1 = DocId::new(1);
        index
            .upsert_document(tx1, d1, "Rust is a fast programming language.", None)
            .await
            .expect("insert");
        storage.commit(tx1).await.expect("commit");

        let tx2 = TxId::new(2);
        let d2 = DocId::new(2);
        index
            .upsert_document(
                tx2,
                d2,
                "I like rust programming and rust ownership rules.",
                None,
            )
            .await
            .expect("insert");
        storage.commit(tx2).await.expect("commit");

        let tx3 = TxId::new(3);
        let d3 = DocId::new(3);
        index
            .upsert_document(tx3, d3, "Python is dynamically typed.", None)
            .await
            .expect("insert");
        storage.commit(tx3).await.expect("commit");

        let results = index
            .search_bm25("rust programming", 3)
            .await
            .expect("search");

        assert_eq!(results.len(), 2);
        // doc 2 has "rust" twice and "programming" once, should score higher than doc 1
        assert!(results[0].0 == d2 || results[1].0 == d2);

        let doc2_pos = results
            .iter()
            .position(|r| r.0 == d2)
            .expect("d2 should be in results");
        let doc1_pos = results
            .iter()
            .position(|r| r.0 == d1)
            .expect("d1 should be in results");
        assert!(
            doc2_pos < doc1_pos,
            "doc2 should be ranked higher due to higher TF"
        );
    }

    #[tokio::test]
    async fn test_upsert_update_metadata() {
        let tmp = TempDir::new().expect("tmp");
        let config = LsmConfig {
            path: tmp.path().to_path_buf(),
            ..Default::default()
        };
        let storage = Arc::new(LsmStorage::new(config).await.expect("storage"));
        let index = InvertedIndex::new(storage.clone(), "default");

        let tx1 = TxId::new(1);
        let d1 = DocId::new(1);
        index
            .upsert_document(tx1, d1, "one two three", None)
            .await
            .unwrap();
        storage.commit(tx1).await.unwrap();

        // Check metadata
        let total_docs_key = index.key("meta:total_docs");
        let total_tok_key = index.key("meta:total_tokens");

        let docs = u64::from_le_bytes(
            storage
                .get(&total_docs_key)
                .await
                .expect("get")
                .expect("exists")
                .try_into()
                .expect("u64"),
        );
        let tokens = u64::from_le_bytes(
            storage
                .get(&total_tok_key)
                .await
                .expect("get")
                .expect("exists")
                .try_into()
                .expect("u64"),
        );
        assert_eq!(docs, 1);
        assert_eq!(tokens, 3);

        // Update same doc
        let tx2 = TxId::new(2);
        index
            .upsert_document(tx2, d1, "one two", Some("one two three"))
            .await
            .expect("upsert");
        storage.commit(tx2).await.expect("commit");

        let docs = u64::from_le_bytes(
            storage
                .get(&total_docs_key)
                .await
                .expect("get")
                .expect("exists")
                .try_into()
                .expect("u64"),
        );
        let tokens = u64::from_le_bytes(
            storage
                .get(&total_tok_key)
                .await
                .expect("get")
                .expect("exists")
                .try_into()
                .expect("u64"),
        );
        assert_eq!(docs, 1); // Should still be 1
        assert_eq!(tokens, 2); // Should be 2 now

        // Delete doc
        let tx3 = TxId::new(3);
        index
            .delete_document(tx3, d1, "one two")
            .await
            .expect("delete");
        storage.commit(tx3).await.expect("commit");

        let docs = u64::from_le_bytes(
            storage
                .get(&total_docs_key)
                .await
                .expect("get")
                .expect("exists")
                .try_into()
                .expect("u64"),
        );
        let tokens = u64::from_le_bytes(
            storage
                .get(&total_tok_key)
                .await
                .expect("get")
                .expect("exists")
                .try_into()
                .expect("u64"),
        );
        assert_eq!(docs, 0);
        assert_eq!(tokens, 0);
    }

    #[tokio::test]
    async fn test_update_removes_old_terms() {
        let tmp = TempDir::new().expect("tmp");
        let config = LsmConfig {
            path: tmp.path().to_path_buf(),
            ..Default::default()
        };
        let storage = Arc::new(LsmStorage::new(config).await.expect("storage"));
        let index = InvertedIndex::new(storage.clone(), "default");

        let d1 = DocId::new(1);
        let tx1 = TxId::new(1);
        index
            .upsert_document(tx1, d1, "apple banana", None)
            .await
            .expect("upsert");
        storage.commit(tx1).await.expect("commit");

        // Search for apple
        let results = index.search_bm25("apple", 10).await.expect("search");
        assert_eq!(results.len(), 1);

        // Update doc 1: apple -> cherry
        let tx2 = TxId::new(2);
        index
            .upsert_document(tx2, d1, "cherry banana", Some("apple banana"))
            .await
            .expect("upsert");
        storage.commit(tx2).await.expect("commit");

        // Search for apple: should be 0
        let results = index.search_bm25("apple", 10).await.expect("search");
        assert_eq!(results.len(), 0, "apple should be removed");

        // Search for cherry: should be 1
        let results = index.search_bm25("cherry", 10).await.expect("search");
        assert_eq!(results.len(), 1, "cherry should be added");
    }

    #[tokio::test]
    async fn test_delete_removes_from_search() {
        let tmp = TempDir::new().expect("tmp");
        let config = LsmConfig {
            path: tmp.path().to_path_buf(),
            ..Default::default()
        };
        let storage = Arc::new(LsmStorage::new(config).await.expect("storage"));
        let index = InvertedIndex::new(storage.clone(), "default");

        let d1 = DocId::new(1);
        let tx1 = TxId::new(1);
        index
            .upsert_document(tx1, d1, "quick brown fox", None)
            .await
            .expect("upsert");
        storage.commit(tx1).await.expect("commit");

        let results = index.search_bm25("fox", 10).await.expect("search");
        assert_eq!(results.len(), 1);

        let tx2 = TxId::new(2);
        index
            .delete_document(tx2, d1, "quick brown fox")
            .await
            .expect("delete");
        storage.commit(tx2).await.expect("commit");

        let results = index.search_bm25("fox", 10).await.expect("search");
        assert_eq!(
            results.len(),
            0,
            "fox should be removed from search after deletion"
        );
    }
}
