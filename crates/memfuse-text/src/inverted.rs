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
    pub async fn upsert_document(&self, tx: TxId, doc_id: DocId, text: &str) -> Result<()> {
        let tokens = tokenize(text);
        if tokens.is_empty() {
            // If the document becomes empty, it's effectively a deletion of its index content.
            // But we keep it simple here and just return.
            return Ok(());
        }

        let mut tfs = HashMap::new();
        for t in &tokens {
            *tfs.entry(t.clone()).or_insert(0u32) += 1;
        }
        let mut new_unique_terms: Vec<String> = tfs.keys().cloned().collect();
        new_unique_terms.sort();

        // 1. Fetch old state
        let dl_key = self.key(&format!("dl:{}", doc_id.inner()));
        let dt_key = self.key(&format!("dt:{}", doc_id.inner()));

        let old_dl = if let Some(bytes) = self.storage.get(&dl_key).await? {
            if bytes.len() == 4 {
                Some(u32::from_le_bytes(bytes.as_slice().try_into().map_err(
                    |_| MemFuseError::Storage("Invalid doc_len length".into()),
                )?))
            } else {
                None
            }
        } else {
            None
        };

        let old_terms = if let Some(bytes) = self.storage.get(&dt_key).await? {
            let config = bincode::config::standard();
            if let Ok((terms, _)) =
                bincode::serde::decode_from_slice::<Vec<String>, _>(&bytes, config)
            {
                Some(terms)
            } else {
                None
            }
        } else {
            None
        };

        // 2. Remove old terms from posting lists (ghost terms prevention)
        if let Some(terms) = old_terms {
            for term in terms {
                // Only remove if it's NOT in the new terms (optimization)
                if !tfs.contains_key(&term) {
                    let pl_key = self.key(&format!("pl:{}", term));
                    if let Some(bytes) = self.storage.get(&pl_key).await? {
                        let config = bincode::config::standard();
                        if let Ok((mut pl, _)) = bincode::serde::decode_from_slice::<
                            Vec<(DocId, u32)>,
                            _,
                        >(&bytes, config)
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
        }

        // 3. Update global stats
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
        if let Some(old_len) = old_dl {
            total_tokens = total_tokens.saturating_sub(old_len as u64);
        }
        total_tokens += tokens.len() as u64;
        self.storage
            .put(tx, &total_tok_key, &total_tokens.to_le_bytes())
            .await?;

        if old_dl.is_none() {
            let total_docs_key = self.key("meta:total_docs");
            let mut total_docs = 0u64;
            if let Some(bytes) = self.storage.get(&total_docs_key).await? {
                if bytes.len() == 8 {
                    total_docs = u64::from_le_bytes(
                        bytes.as_slice().try_into().map_err(|_| {
                            MemFuseError::Storage("Invalid total_docs length".into())
                        })?,
                    );
                }
            }
            total_docs += 1;
            self.storage
                .put(tx, &total_docs_key, &total_docs.to_le_bytes())
                .await?;
        }

        // 4. Update posting lists with new terms
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

        // 5. Store new doc metadata
        self.storage
            .put(tx, &dl_key, &(tokens.len() as u32).to_le_bytes())
            .await?;

        let term_bytes = bincode::serde::encode_to_vec(&new_unique_terms, bincode::config::standard())
            .map_err(|e| MemFuseError::Storage(format!("bincode: {}", e)))?;
        self.storage.put(tx, &dt_key, &term_bytes).await?;

        Ok(())
    }

    /// Deletes a document from the index.
    pub async fn delete_document(&self, tx: TxId, doc_id: DocId) -> Result<()> {
        let dl_key = self.key(&format!("dl:{}", doc_id.inner()));
        let dt_key = self.key(&format!("dt:{}", doc_id.inner()));

        // 1. Fetch metadata
        let old_dl = if let Some(bytes) = self.storage.get(&dl_key).await? {
            if bytes.len() == 4 {
                u32::from_le_bytes(bytes.as_slice().try_into().map_err(|_| {
                    MemFuseError::Storage("Invalid doc_len length".into())
                })?)
            } else {
                0
            }
        } else {
            0
        };

        let old_terms = if let Some(bytes) = self.storage.get(&dt_key).await? {
            let config = bincode::config::standard();
            if let Ok((terms, _)) =
                bincode::serde::decode_from_slice::<Vec<String>, _>(&bytes, config)
            {
                Some(terms)
            } else {
                None
            }
        } else {
            None
        };

        // 2. Remove from posting lists
        if let Some(terms) = old_terms {
            for term in terms {
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

        // 3. Update global stats
        if old_dl > 0 {
            let total_tok_key = self.key("meta:total_tokens");
            if let Some(bytes) = self.storage.get(&total_tok_key).await? {
                if bytes.len() == 8 {
                    let mut total_tokens = u64::from_le_bytes(
                        bytes.as_slice().try_into().map_err(|_| {
                            MemFuseError::Storage("Invalid total_tokens length".into())
                        })?,
                    );
                    total_tokens = total_tokens.saturating_sub(old_dl as u64);
                    self.storage
                        .put(tx, &total_tok_key, &total_tokens.to_le_bytes())
                        .await?;
                }
            }

            let total_docs_key = self.key("meta:total_docs");
            if let Some(bytes) = self.storage.get(&total_docs_key).await? {
                if bytes.len() == 8 {
                    let mut total_docs = u64::from_le_bytes(
                        bytes.as_slice().try_into().map_err(|_| {
                            MemFuseError::Storage("Invalid total_docs length".into())
                        })?,
                    );
                    total_docs = total_docs.saturating_sub(1);
                    self.storage
                        .put(tx, &total_docs_key, &total_docs.to_le_bytes())
                        .await?;
                }
            }
        }

        // 4. Cleanup
        self.storage.delete(tx, &dl_key).await?;
        self.storage.delete(tx, &dt_key).await?;

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
        let tmp = TempDir::new().expect("Failed to create temp dir");
        let config = LsmConfig {
            path: tmp.path().to_path_buf(),
            ..Default::default()
        };
        let storage = Arc::new(
            LsmStorage::new(config)
                .await
                .expect("Failed to create storage"),
        );

        let index = InvertedIndex::new(storage.clone(), "default");

        let tx1 = TxId::new(1);
        let d1 = DocId::new(1);
        index
            .upsert_document(tx1, d1, "Rust is a fast programming language.")
            .await
            .expect("Failed to insert doc 1");
        storage.commit(tx1).await.expect("Failed to commit tx1");

        let tx2 = TxId::new(2);
        let d2 = DocId::new(2);
        index
            .upsert_document(tx2, d2, "I like rust programming and rust ownership rules.")
            .await
            .expect("Failed to insert doc 2");
        storage.commit(tx2).await.expect("Failed to commit tx2");

        let tx3 = TxId::new(3);
        let d3 = DocId::new(3);
        index
            .upsert_document(tx3, d3, "Python is dynamically typed.")
            .await
            .expect("Failed to insert doc 3");
        storage.commit(tx3).await.expect("Failed to commit tx3");

        let results = index
            .search_bm25("rust programming", 3)
            .await
            .expect("Failed to search BM25");

        assert_eq!(results.len(), 2);
        // doc 2 has "rust" twice and "programming" once, should score higher than doc 1
        assert!(results[0].0 == d2 || results[1].0 == d2);

        let doc2_pos = results
            .iter()
            .position(|r| r.0 == d2)
            .expect("doc2 should be in results");
        let doc1_pos = results
            .iter()
            .position(|r| r.0 == d1)
            .expect("doc1 should be in results");
        assert!(
            doc2_pos < doc1_pos,
            "doc2 should be ranked higher due to higher TF"
        );
    }

    #[tokio::test]
    async fn test_upsert_removes_old_terms() {
        let tmp = TempDir::new().expect("Failed to create temp dir");
        let config = LsmConfig {
            path: tmp.path().to_path_buf(),
            ..Default::default()
        };
        let storage = Arc::new(
            LsmStorage::new(config)
                .await
                .expect("Failed to create storage"),
        );
        let index = InvertedIndex::new(storage.clone(), "default");

        let tx1 = TxId::new(1);
        let d1 = DocId::new(1);
        index
            .upsert_document(tx1, d1, "Apples are delicious.")
            .await
            .expect("First insert");
        storage.commit(tx1).await.expect("Commit tx1");

        // Verify it's found by "apples"
        let res = index.search_bm25("apples", 1).await.expect("Search 1");
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].0, d1);

        // Update document - no more "apples"
        let tx2 = TxId::new(2);
        index
            .upsert_document(tx2, d1, "Bananas are yellow.")
            .await
            .expect("Second insert");
        storage.commit(tx2).await.expect("Commit tx2");

        // Verify it's NOT found by "apples"
        let res = index.search_bm25("apples", 1).await.expect("Search 2");
        assert_eq!(res.len(), 0);

        // Verify it's found by "bananas"
        let res = index.search_bm25("bananas", 1).await.expect("Search 3");
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].0, d1);
    }
}
