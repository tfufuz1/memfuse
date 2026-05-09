// ANCHOR:ARCH:INVERTED-001 — Inverted Index over LSM-Storage.
//! Inverted Index backend using LsmStorage for persistence.

use memfuse_core::{DocId, Result, StorageEngine, TxId};
use memfuse_store::LsmStorage;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// A posting list for a single term.
#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct PostingList {
    /// List of (doc_id, tf) pairs.
    pub entries: Vec<(u64, u32)>,
}

pub struct InvertedIndex {
    storage: Arc<LsmStorage>,
    prefix: Vec<u8>,
}

impl InvertedIndex {
    /// Creates a new InvertedIndex instance for a collection.
    pub fn new(storage: Arc<LsmStorage>, collection_name: &str) -> Self {
        let prefix = format!("__col:{}:text:", collection_name).into_bytes();
        Self { storage, prefix }
    }

    fn key_posting(&self, term: &str) -> Vec<u8> {
        let mut k = self.prefix.clone();
        k.extend_from_slice(b"p:");
        k.extend_from_slice(term.as_bytes());
        k
    }

    fn key_doc_len(&self, doc_id: DocId) -> Vec<u8> {
        let mut k = self.prefix.clone();
        k.extend_from_slice(b"l:");
        k.extend_from_slice(&doc_id.inner().to_le_bytes());
        k
    }

    fn key_stat_total_docs(&self) -> Vec<u8> {
        let mut k = self.prefix.clone();
        k.extend_from_slice(b"s:td");
        k
    }

    fn key_stat_sum_lens(&self) -> Vec<u8> {
        let mut k = self.prefix.clone();
        k.extend_from_slice(b"s:sl");
        k
    }

    /// Indexes a document's tokens.
    pub async fn index_document(
        &self,
        tx_id: TxId,
        doc_id: DocId,
        tokens: &[String],
    ) -> Result<()> {
        if tokens.is_empty() {
            return Ok(());
        }

        // 1. Calculate TFs
        let mut counts = HashMap::new();
        for token in tokens {
            *counts.entry(token).or_insert(0u32) += 1;
        }

        // 2. Update Postings (RMW)
        for (token, tf) in counts {
            let key = self.key_posting(token);
            let mut list = if let Some(bytes) = self.storage.get(&key).await? {
                bincode::deserialize::<PostingList>(&bytes).map_err(|e| {
                    memfuse_core::MemFuseError::Storage(format!("Bincode error: {}", e))
                })?
            } else {
                PostingList::default()
            };

            // Update or add entry
            if let Some(entry) = list.entries.iter_mut().find(|e| e.0 == doc_id.inner()) {
                entry.1 = tf;
            } else {
                list.entries.push((doc_id.inner(), tf));
            }

            let bytes = bincode::serialize(&list).map_err(|e| {
                memfuse_core::MemFuseError::Storage(format!("Bincode error: {}", e))
            })?;
            self.storage.put(tx_id, &key, &bytes).await?;
        }

        // 3. Update doc length
        let len_key = self.key_doc_len(doc_id);
        let len_bytes = bincode::serialize(&(tokens.len() as u32))
            .map_err(|e| memfuse_core::MemFuseError::Storage(format!("Bincode error: {}", e)))?;
        self.storage.put(tx_id, &len_key, &len_bytes).await?;

        // 4. Update stats (RMW)
        let td_key = self.key_stat_total_docs();
        let total_docs: u32 = if let Some(bytes) = self.storage.get(&td_key).await? {
            bincode::deserialize(&bytes).unwrap_or(0)
        } else {
            0
        };
        let td_bytes = bincode::serialize(&(total_docs + 1))
            .map_err(|e| memfuse_core::MemFuseError::Storage(format!("Bincode error: {}", e)))?;
        self.storage.put(tx_id, &td_key, &td_bytes).await?;

        let sl_key = self.key_stat_sum_lens();
        let sum_lens: u64 = if let Some(bytes) = self.storage.get(&sl_key).await? {
            bincode::deserialize(&bytes).unwrap_or(0)
        } else {
            0
        };
        let sl_bytes = bincode::serialize(&(sum_lens + tokens.len() as u64))
            .map_err(|e| memfuse_core::MemFuseError::Storage(format!("Bincode error: {}", e)))?;
        self.storage.put(tx_id, &sl_key, &sl_bytes).await?;

        Ok(())
    }

    /// Retrieves the posting list for a term.
    pub async fn get_posting(&self, term: &str) -> Result<Option<PostingList>> {
        let key = self.key_posting(term);
        if let Some(bytes) = self.storage.get(&key).await? {
            let list = bincode::deserialize(&bytes).map_err(|e| {
                memfuse_core::MemFuseError::Storage(format!("Bincode error: {}", e))
            })?;
            Ok(Some(list))
        } else {
            Ok(None)
        }
    }

    /// Retrieves the length of a document.
    pub async fn get_doc_len(&self, doc_id: DocId) -> Result<u32> {
        let key = self.key_doc_len(doc_id);
        if let Some(bytes) = self.storage.get(&key).await? {
            let len = bincode::deserialize(&bytes).map_err(|e| {
                memfuse_core::MemFuseError::Storage(format!("Bincode error: {}", e))
            })?;
            Ok(len)
        } else {
            Ok(0)
        }
    }

    /// Retrieves global statistics for BM25.
    pub async fn get_stats(&self) -> Result<(u32, f32)> {
        let td_key = self.key_stat_total_docs();
        let total_docs: u32 = if let Some(bytes) = self.storage.get(&td_key).await? {
            bincode::deserialize(&bytes).unwrap_or(0)
        } else {
            0
        };

        if total_docs == 0 {
            return Ok((0, 0.0));
        }

        let sl_key = self.key_stat_sum_lens();
        let sum_lens: u64 = if let Some(bytes) = self.storage.get(&sl_key).await? {
            bincode::deserialize(&bytes).unwrap_or(0)
        } else {
            0
        };

        let avg_len = (sum_lens as f32) / (total_docs as f32);
        Ok((total_docs, avg_len))
    }
}
