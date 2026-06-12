//! # MemFuse Database
//!
//! The core orchestration layer that combines LSM storage and HNSW indexing.

#![forbid(unsafe_code)]

use memfuse_core::{Result, StorageEngine, TxId};
use memfuse_index::{HnswConfig, HnswIndex};
use memfuse_store::LsmStorage;
pub use memfuse_core::DistanceMetric;
pub use serde_json::json;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub mod chunker;
pub mod collection;
pub mod context;
pub mod filter;
pub mod fusion;
pub mod reaper;
pub mod transaction;

use async_trait::async_trait;
use memfuse_sandbox::SandboxBridge;
pub use collection::Collection;
pub use filter::MetadataFilter;

#[cfg(feature = "embed")]
use memfuse_embed::TextEmbedder;

/// User-facing search result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub score: f32,
    pub metadata: Option<Value>,
}

/// User-facing document structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub id: String,
    pub metadata: Option<Value>,
}

/// Overall database statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbStats {
    pub index_stats: memfuse_core::VectorIndexStats,
    pub storage_stats: memfuse_core::StorageStats,
}

/// Global configuration settings.
#[derive(Debug, Clone)]
pub struct MemFuseConfig {
    pub dimension: usize,
    pub max_elements: usize,
    pub distance_metric: memfuse_core::DistanceMetric,
    pub encryption_passphrase: Option<String>,
}

impl Default for MemFuseConfig {
    fn default() -> Self {
        Self {
            dimension: 1536,
            max_elements: 1_000_000,
            distance_metric: memfuse_core::DistanceMetric::Cosine,
            encryption_passphrase: None,
        }
    }
}

/// MemFuse — Embedded hybrid-search database for AI agents.
pub struct MemFuse {
    storage: Arc<LsmStorage>,
    next_tx: Arc<AtomicU64>,
    dimension: usize,
    collections: tokio::sync::RwLock<std::collections::HashMap<String, Collection<LsmStorage>>>,
    #[cfg(feature = "cluster")]
    raft: tokio::sync::OnceCell<memfuse_cluster::node::MemFuseRaft>,
    #[cfg(feature = "embed")]
    embedder: std::sync::RwLock<Option<Arc<TextEmbedder>>>,
}

impl MemFuse {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_with_config(path, MemFuseConfig::default()).await
    }

    pub async fn open_with_config(path: impl AsRef<Path>, config: MemFuseConfig) -> Result<Self> {
        let lsm_config = memfuse_store::LsmConfig {
            path: path.as_ref().to_path_buf(),
            encryption_passphrase: config.encryption_passphrase.clone(),
            ..Default::default()
        };

        let storage = Arc::new(LsmStorage::new(lsm_config).await?);
        let last_tx = storage.last_tx_id().await?.inner();
        let next_tx = Arc::new(AtomicU64::new(last_tx + 1));

        let db = Self {
            storage,
            next_tx,
            dimension: config.dimension,
            collections: tokio::sync::RwLock::new(std::collections::HashMap::new()),
            #[cfg(feature = "cluster")]
            raft: tokio::sync::OnceCell::new(),
            #[cfg(feature = "embed")]
            embedder: std::sync::RwLock::new(None),
        };

        db.initialize_collections().await?;
        Ok(db)
    }

    async fn initialize_collections(&self) -> Result<()> {
        let names = self.list_collections().await?;
        for name in names {
            let _ = self.collection(&name).await?;
        }
        Ok(())
    }

    pub async fn collection(&self, name: &str) -> Result<Collection<LsmStorage>> {
        let read_guard = self.collections.read().await;
        if let Some(col) = read_guard.get(name) {
            return Ok(col.clone());
        }
        drop(read_guard);

        let mut write_guard = self.collections.write().await;
        if let Some(col) = write_guard.get(name) {
            return Ok(col.clone());
        }

        let hnsw_config = HnswConfig {
            dimension: self.dimension,
            ..Default::default()
        };
        let index = Arc::new(HnswIndex::new(hnsw_config));

        let col = Collection::new(
            name.to_string(),
            Arc::clone(&self.storage),
            index,
            Arc::clone(&self.next_tx),
            self.dimension,
        );

        if name != "default" {
            let col_idx_key = [b"__col_idx:\x00", name.as_bytes()].concat();
            let tx = TxId::new(self.next_tx.fetch_add(1, Ordering::SeqCst));
            self.storage.put(tx, &col_idx_key, b"{}").await?;
            self.storage.commit(tx).await?;
        }

        col.load_index().await?;
        write_guard.insert(name.to_string(), col.clone());

        Ok(col)
    }

    pub async fn list_collections(&self) -> Result<Vec<String>> {
        let col_idx_prefix = b"__col_idx:\x00";
        let entries = self.storage.scan_prefix(col_idx_prefix).await?;

        let mut names = std::collections::HashSet::new();
        names.insert("default".to_string());

        for (k, _) in entries {
            let name_bytes = &k[col_idx_prefix.len()..];
            if let Ok(name) = String::from_utf8(name_bytes.to_vec()) {
                names.insert(name);
            }
        }

        let guard = self.collections.read().await;
        for name in guard.keys() {
            names.insert(name.clone());
        }

        let mut sorted_names: Vec<String> = names.into_iter().collect();
        sorted_names.sort();
        Ok(sorted_names)
    }

    pub async fn drop_collection(&self, name: &str) -> Result<()> {
        if name == "default" {
            return Err(memfuse_core::MemFuseError::invalid_input("Cannot drop default collection"));
        }

        let mut guard = self.collections.write().await;
        if let Some(col) = guard.remove(name) {
            col.drop_collection().await?;
            let col_idx_key = [b"__col_idx:\x00", name.as_bytes()].concat();
            let tx = TxId::new(self.next_tx.fetch_add(1, Ordering::SeqCst));
            self.storage.delete(tx, &col_idx_key).await?;
            self.storage.commit(tx).await?;
        }
        Ok(())
    }

    async fn default_col(&self) -> Result<Collection<LsmStorage>> {
        self.collection("default").await
    }

    pub async fn insert(&self, id: &str, embedding: &[f32], metadata: Option<Value>) -> Result<()> {
        self.default_col().await?.insert(id, embedding, metadata).await
    }

    pub async fn upsert(&self, id: &str, embedding: &[f32], metadata: Option<Value>) -> Result<()> {
        self.default_col().await?.upsert(id, embedding, metadata).await
    }

    pub async fn update(&self, id: &str, embedding: &[f32], metadata: Option<Value>) -> Result<()> {
        self.default_col().await?.update(id, embedding, metadata).await
    }

    pub async fn search(&self, query: &[f32], k: usize) -> Result<Vec<SearchResult>> {
        self.default_col().await?.search(query, k).await
    }

    pub async fn search_with_filter(&self, query: &[f32], k: usize, filter: Option<MetadataFilter>) -> Result<Vec<SearchResult>> {
        self.default_col().await?.search_with_filter(query, k, filter).await
    }

    pub async fn hybrid_search(&self, text: &str, vector: &[f32], k: usize) -> Result<Vec<SearchResult>> {
        self.default_col().await?.hybrid_search(text, vector, k).await
    }

    pub async fn get(&self, id: &str) -> Result<Option<Document>> {
        self.default_col().await?.get(id).await
    }

    pub async fn get_at_snapshot(&self, id: &str, seq_no: u64) -> Result<Option<Document>> {
        self.default_col().await?.get_at_snapshot(id, seq_no).await
    }

    pub async fn create_snapshot(&self) -> Result<u64> {
        self.storage.last_seq_no().await
    }

    pub async fn last_committed_seq(&self) -> Result<u64> {
        self.storage.last_seq_no().await
    }

    pub async fn delete(&self, id: &str) -> Result<()> {
        self.default_col().await?.delete(id).await
    }

    pub async fn len(&self) -> Result<usize> {
        Ok(self.default_col().await?.len().await)
    }

    pub async fn stats(&self) -> Result<DbStats> {
        Ok(DbStats {
            index_stats: self.default_col().await?.stats().await?,
            storage_stats: self.storage.stats().await?,
        })
    }

    pub async fn flush(&self) -> Result<()> {
        self.storage.flush().await?;
        Ok(())
    }

    pub async fn close(self) -> Result<()> {
        self.storage.flush().await?;
        Ok(())
    }

    pub async fn begin_transaction(&self) -> Result<crate::transaction::DbTransaction<LsmStorage>> {
        Ok(self.default_col().await?.begin_transaction())
    }

    pub fn inner_storage(&self) -> Arc<LsmStorage> {
        self.storage.clone()
    }
}

#[async_trait]
impl SandboxBridge for MemFuse {
    async fn db_search(&self, query: &[u8], k: usize) -> Result<Vec<u8>> {
        let f32_count = query.len() / 4;
        let mut vector = Vec::with_capacity(f32_count);
        for i in 0..f32_count {
            let bits = u32::from_le_bytes(query[i*4..i*4+4].try_into().unwrap());
            vector.push(f32::from_bits(bits));
        }
        let results = self.search(&vector, k).await?;
        Ok(serde_json::to_vec(&results).unwrap())
    }

    async fn db_insert(&self, key: &[u8], _value: &[u8]) -> Result<()> {
        let id = String::from_utf8_lossy(key).to_string();
        self.insert(&id, &[], None).await
    }

    async fn db_get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let id = String::from_utf8_lossy(key).to_string();
        let doc = self.get(&id).await?;
        Ok(doc.map(|d| serde_json::to_vec(&d).unwrap()))
    }
}
