//! Logically isolated Collections inside the MemFuse database.

use memfuse_index::HnswIndex;
use memfuse_store::LsmStorage;
use std::sync::Arc;

/// A logically isolated collection of documents.
/// Each collection has its own HNSW vector index but shares the underlying LSM-Tree.
#[derive(Clone)]
#[allow(dead_code)]
pub struct Collection {
    name: String,
    prefix: Vec<u8>,
    index: Arc<HnswIndex>,
    storage: Arc<LsmStorage>,
    dimension: usize,
}

impl Collection {
    pub fn new(
        name: String,
        storage: Arc<LsmStorage>,
        index: Arc<HnswIndex>,
        _next_tx: std::sync::Arc<std::sync::atomic::AtomicU64>,
        dimension: usize,
    ) -> Self {
        let prefix = format!("__col:{}:", name).into_bytes();
        Self {
            name,
            prefix,
            index,
            storage,
            dimension,
        }
    }

    pub async fn insert(
        &self,
        _id: &str,
        _embedding: &[f32],
        _metadata: Option<serde_json::Value>,
    ) -> memfuse_core::Result<()> {
        // Here we would prefix the ID, insert into LSM, and insert into HNSW
        // self.storage.put(&self.namespaced_key(id), bincode::serialize(doc)?);
        // self.index.insert(id, embedding);
        Ok(())
    }

    pub async fn search(
        &self,
        _query_embedding: &[f32],
        _k: usize,
    ) -> memfuse_core::Result<Vec<crate::SearchResult>> {
        // Here we would perform HNSW search
        Ok(vec![])
    }

    #[allow(dead_code)]
    fn namespaced_key(&self, id: &str) -> Vec<u8> {
        let mut key = self.prefix.clone();
        key.extend_from_slice(id.as_bytes());
        key
    }

    pub async fn get(&self, _id: &str) -> memfuse_core::Result<Option<crate::Document>> {
        Ok(None)
    }

    pub async fn update(
        &self,
        _id: &str,
        _embedding: &[f32],
        _metadata: Option<serde_json::Value>,
    ) -> memfuse_core::Result<()> {
        Ok(())
    }

    pub async fn search_filtered(
        &self,
        _query: &[f32],
        _k: usize,
        _filter: Option<&(dyn Fn(memfuse_core::DocId) -> bool + Send + Sync)>,
    ) -> memfuse_core::Result<Vec<crate::SearchResult>> {
        Ok(vec![])
    }

    pub async fn delete(&self, _id: &str) -> memfuse_core::Result<()> {
        Ok(())
    }

    pub async fn relate(&self, _from: &str, _to: &str, _label: &str) -> memfuse_core::Result<()> {
        Ok(())
    }

    pub async fn scan_prefix(
        &self,
        _prefix: &str,
    ) -> memfuse_core::Result<Vec<(String, serde_json::Value)>> {
        Ok(vec![])
    }

    pub async fn len(&self) -> usize {
        0
    }

    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }

    pub async fn scan(
        &self,
        _start: std::ops::Bound<&[u8]>,
        _end: std::ops::Bound<&[u8]>,
    ) -> memfuse_core::Result<Vec<(String, serde_json::Value)>> {
        Ok(vec![])
    }

    pub async fn stats(&self) -> memfuse_core::Result<memfuse_core::VectorIndexStats> {
        Ok(memfuse_core::VectorIndexStats {
            num_vectors: 0,
            memory_usage_bytes: 0,
            num_layers: 0,
        })
    }

    pub async fn drop_collection(&self) -> memfuse_core::Result<()> {
        Ok(())
    }
}
