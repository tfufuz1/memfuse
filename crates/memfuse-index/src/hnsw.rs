// ANCHOR:ARCH:HNSW-001 — Hierarchical Navigable Small World Index.
// IMPLEMENTS: VectorIndex Trait (memfuse-core/traits.rs)
// CONSTRUCT: Greedyensuche + Heuristik für Diversitätsauswahl der Nachbarn.
// SEARCH: Layer Descent (von max_layer bis 0), dann EF-Search in Layer 0.
// DELETE: Soft-Delete (Tombstone via deleted_nodes Roaring Bitmap).
// REBUILD-LOGIK: Wenn >20% gelöscht → async trigger_rebuild_async() -> Atomic Swap.
// TRANSAKTIONEN: Nutzt memfuse_core::TxBuffer zur Staging-Isolation.
//! HNSW (Hierarchical Navigable Small World) vector index.
//!
//! Provides approximate nearest neighbor search with:
//! - Diversity heuristic neighbor selection
//! - Automatic rebuild on >20% deletions
//! - Transactional inserts/deletes via TxBuffer

use crate::distance::compute_distance;
use ahash::{AHashMap, AHashSet};
use async_trait::async_trait;
use memfuse_core::{
    DistanceMetric, DocId, IndexOp, MemFuseError, Result, ScoredDocument, TxBuffer, TxId,
    VectorIndex, VectorIndexStats,
};
use parking_lot::RwLock;
use rand::Rng;
use roaring::RoaringTreemap;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

/// HNSW index configuration.
#[derive(Debug, Clone)]
pub struct HnswConfig {
    /// Vector dimensionality.
    pub dimension: usize,
    /// Maximum number of elements.
    pub max_elements: usize,
    /// Number of connections per element (M parameter).
    pub m: usize,
    /// Dynamic candidate list size during construction.
    pub ef_construction: usize,
    /// Dynamic candidate list size during search.
    pub ef_search: usize,
    /// Distance metric.
    pub distance_metric: DistanceMetric,
    /// Rebuild threshold (ratio of deleted nodes).
    pub rebuild_threshold: f64,
}

impl Default for HnswConfig {
    fn default() -> Self {
        Self {
            dimension: 1536,
            max_elements: 1_000_000,
            m: 16,
            ef_construction: 200,
            ef_search: 50,
            distance_metric: DistanceMetric::Cosine,
            rebuild_threshold: 0.8,
        }
    }
}

/// A node in the HNSW graph.
#[derive(Debug)]
struct HnswNode {
    doc_id: DocId,
    vector: Arc<[f32]>,
    connections: Vec<Vec<usize>>,
    _max_layer: usize,
}

/// Search candidate.
#[derive(Clone, Copy, Debug)]
struct Candidate {
    index: usize,
    distance: f32,
}

impl PartialEq for Candidate {
    fn eq(&self, other: &Self) -> bool {
        self.distance == other.distance
    }
}
impl Eq for Candidate {}

impl PartialOrd for Candidate {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Candidate {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.distance
            .partial_cmp(&other.distance)
            .unwrap_or(std::cmp::Ordering::Equal)
    }
}

pub struct HnswIndex {
    inner: Arc<HnswIndexCore>,
}

impl std::ops::Deref for HnswIndex {
    type Target = HnswIndexCore;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

pub struct HnswIndexCore {
    config: HnswConfig,
    nodes: RwLock<Vec<HnswNode>>,
    doc_to_node: RwLock<AHashMap<u64, usize>>,
    entry_point: RwLock<Option<usize>>,
    max_layer: AtomicU64,
    ml: f64,
    tx_buffer: TxBuffer<Vec<f32>>,
    deleted_nodes: RwLock<RoaringTreemap>,
    deleted_count: AtomicU64,
    rebuilding: AtomicBool,
    write_mutex: Mutex<()>,
}

impl HnswIndex {
    /// Creates a new HNSW index.
    pub fn new(config: HnswConfig) -> Self {
        let ml = 1.0 / (config.m as f64).ln();
        Self {
            inner: Arc::new(HnswIndexCore {
                config,
                nodes: RwLock::new(Vec::new()),
                doc_to_node: RwLock::new(AHashMap::new()),
                entry_point: RwLock::new(None),
                max_layer: AtomicU64::new(0),
                ml,
                tx_buffer: TxBuffer::new_with_config(16, std::time::Duration::from_secs(60)),
                deleted_nodes: RwLock::new(RoaringTreemap::new()),
                deleted_count: AtomicU64::new(0),
                rebuilding: AtomicBool::new(false),
                write_mutex: Mutex::new(()),
            }),
        }
    }

    /// Triggers an async rebuild if the deletion threshold is exceeded.
    pub fn trigger_rebuild_async(&self) {
        if self.is_rebuild_required() {
            let inner = Arc::clone(&self.inner);
            tokio::spawn(async move {
                if let Err(e) = inner.rebuild().await {
                    tracing::error!("Failed to rebuild HNSW index: {}", e);
                }
            });
        }
    }
}

impl HnswIndexCore {
    fn random_layer(&self) -> usize {
        let mut rng = rand::thread_rng();
        let r: f64 = rng.r#gen();
        (-r.ln() * self.ml).floor() as usize
    }

    fn search_layer(
        &self,
        query: &[f32],
        entry_points: &[usize],
        ef: usize,
        layer: usize,
    ) -> Result<Vec<Candidate>> {
        let nodes = self.nodes.read();
        let mut visited = AHashSet::with_capacity(ef);
        let mut candidates = BinaryHeap::new();
        let mut results = BinaryHeap::with_capacity(ef);

        for &ep in entry_points {
            if visited.insert(ep) {
                let dist = compute_distance(query, &nodes[ep].vector, self.config.distance_metric)?;
                let cand = Candidate {
                    index: ep,
                    distance: dist,
                };
                candidates.push(Reverse(cand));
                results.push(cand);
            }
        }

        while let Some(Reverse(current)) = candidates.pop() {
            if let Some(worst_result) = results.peek() {
                if current.distance > worst_result.distance && results.len() >= ef {
                    break;
                }
            }

            if layer < nodes[current.index].connections.len() {
                for &neighbor in &nodes[current.index].connections[layer] {
                    if visited.insert(neighbor) {
                        let dist = compute_distance(
                            query,
                            &nodes[neighbor].vector,
                            self.config.distance_metric,
                        )?;
                        let is_better = match results.peek() {
                            Some(worst) => dist < worst.distance,
                            None => true,
                        };

                        if results.len() < ef || is_better {
                            let cand = Candidate {
                                index: neighbor,
                                distance: dist,
                            };
                            candidates.push(Reverse(cand));
                            results.push(cand);
                            if results.len() > ef {
                                results.pop();
                            }
                        }
                    }
                }
            }
        }
        Ok(results.into_sorted_vec())
    }

    fn select_neighbors_heuristic(
        &self,
        nodes: &[HnswNode],
        candidates: &[Candidate],
        m: usize,
    ) -> Result<Vec<usize>> {
        if candidates.len() <= m {
            return Ok(candidates.iter().map(|c| c.index).collect());
        }

        let mut result: Vec<Candidate> = Vec::with_capacity(m);
        let mut remaining: BinaryHeap<Reverse<Candidate>> =
            candidates.iter().copied().map(Reverse).collect();

        while let Some(Reverse(closest)) = remaining.pop() {
            if result.len() >= m {
                break;
            }
            let mut keep = true;
            for selected in &result {
                let dist_between = compute_distance(
                    &nodes[closest.index].vector,
                    &nodes[selected.index].vector,
                    self.config.distance_metric,
                )?;
                if closest.distance > dist_between {
                    keep = false;
                    break;
                }
            }
            if keep {
                result.push(closest);
            }
        }
        Ok(result.iter().map(|c| c.index).collect())
    }

    fn do_insert(&self, id: DocId, vector: &[f32]) -> Result<()> {
        if vector.len() != self.config.dimension {
            return Err(MemFuseError::invalid_input(format!(
                "Dimension mismatch: expected {}, got {}",
                self.config.dimension,
                vector.len()
            )));
        }

        let new_layer = self.random_layer();
        let entry_point_opt = *self.entry_point.read();
        let current_max_layer = self.max_layer.load(Ordering::SeqCst) as usize;

        let new_idx = {
            let mut nodes = self.nodes.write();
            let idx = nodes.len();
            nodes.push(HnswNode {
                doc_id: id,
                vector: Arc::from(vector),
                connections: vec![vec![]; new_layer + 1],
                _max_layer: new_layer,
            });
            idx
        };

        self.doc_to_node.write().insert(id.inner(), new_idx);

        let ep_idx = match entry_point_opt {
            Some(idx) => idx,
            None => {
                *self.entry_point.write() = Some(new_idx);
                self.max_layer.store(new_layer as u64, Ordering::SeqCst);
                return Ok(());
            }
        };

        let mut ep = vec![ep_idx];
        for layer in (new_layer + 1..=current_max_layer).rev() {
            let best = self.search_layer(vector, &ep, 1, layer)?;
            if !best.is_empty() {
                ep = vec![best[0].index];
            }
        }

        for layer in (0..=new_layer.min(current_max_layer)).rev() {
            let neighbors = self.search_layer(vector, &ep, self.config.ef_construction, layer)?;
            let selected = {
                let nodes = self.nodes.read();
                self.select_neighbors_heuristic(&nodes, &neighbors, self.config.m)?
            };

            {
                let mut nodes = self.nodes.write();
                nodes[new_idx].connections[layer] = selected.clone();
                for &neighbor_idx in &selected {
                    if layer < nodes[neighbor_idx].connections.len() {
                        nodes[neighbor_idx].connections[layer].push(new_idx);
                        if nodes[neighbor_idx].connections[layer].len() > self.config.m * 2 {
                            let node_vec = Arc::clone(&nodes[neighbor_idx].vector);
                            let mut conn_cands =
                                Vec::with_capacity(nodes[neighbor_idx].connections[layer].len());
                            for &idx in &nodes[neighbor_idx].connections[layer] {
                                let dist = compute_distance(
                                    &node_vec,
                                    &nodes[idx].vector,
                                    self.config.distance_metric,
                                )?;
                                conn_cands.push(Candidate {
                                    index: idx,
                                    distance: dist,
                                });
                            }
                            nodes[neighbor_idx].connections[layer] = self
                                .select_neighbors_heuristic(
                                    &nodes,
                                    &conn_cands,
                                    self.config.m * 2,
                                )?;
                        }
                    }
                }
            }
            ep = neighbors.iter().map(|c| c.index).collect();
        }

        if new_layer > current_max_layer {
            *self.entry_point.write() = Some(new_idx);
            self.max_layer.store(new_layer as u64, Ordering::SeqCst);
        }
        Ok(())
    }

    fn do_delete(&self, id: DocId) {
        let node_idx = self.doc_to_node.write().remove(&id.inner());
        if let Some(idx) = node_idx {
            self.deleted_nodes.write().insert(idx as u64);
            self.deleted_count.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// Graph connectivity score (1.0 = perfect, 0.0 = fully fragmented).
    pub fn connectivity_score(&self) -> f64 {
        let deleted = self.deleted_count.load(Ordering::SeqCst);
        let total = self.nodes.read().len();
        if total == 0 {
            return 1.0;
        }
        (1.0 - deleted as f64 / total as f64).max(0.0)
    }

    /// Checks if a rebuild is required based on the deletion ratio.
    pub fn is_rebuild_required(&self) -> bool {
        self.connectivity_score() < self.config.rebuild_threshold
    }

    /// Rebuilds the HNSW index from scratch, removing all deleted nodes.
    /// Restores optimal search performance and connectivity.
    pub async fn rebuild(&self) -> Result<()> {
        let _write_lock = self.write_mutex.lock().await;

        if self.rebuilding.swap(true, Ordering::SeqCst) {
            tracing::debug!("HNSW rebuild already in progress, skipping");
            return Ok(());
        }

        tracing::info!("Starting HNSW index rebuild");
        let start_time = std::time::Instant::now();

        // 1. Snapshot active nodes
        let (active_nodes, config) = {
            let nodes = self.nodes.read();
            let deleted_nodes = self.deleted_nodes.read();
            let mut active = Vec::with_capacity(
                nodes
                    .len()
                    .saturating_sub(self.deleted_count.load(Ordering::SeqCst) as usize),
            );
            for (idx, node) in nodes.iter().enumerate() {
                if !deleted_nodes.contains(idx as u64) {
                    active.push((node.doc_id, Arc::clone(&node.vector)));
                }
            }
            (active, self.config.clone())
        };

        // 2. Build fresh index
        let new_index = HnswIndex::new(config);
        for (doc_id, vector) in active_nodes {
            new_index.do_insert(doc_id, &vector)?;
        }

        // 3. Atomic swap
        {
            let mut nodes = self.nodes.write();
            let mut doc_to_node = self.doc_to_node.write();
            let mut entry_point = self.entry_point.write();
            let mut deleted_nodes = self.deleted_nodes.write();

            let new_nodes = std::mem::take(&mut *new_index.nodes.write());
            let new_doc_to_node = std::mem::take(&mut *new_index.doc_to_node.write());
            let new_entry_point = *new_index.entry_point.read();

            *nodes = new_nodes;
            *doc_to_node = new_doc_to_node;
            *entry_point = new_entry_point;
            self.max_layer
                .store(new_index.max_layer.load(Ordering::SeqCst), Ordering::SeqCst);
            deleted_nodes.clear();
            self.deleted_count.store(0, Ordering::SeqCst);
        }

        self.rebuilding.store(false, Ordering::SeqCst);
        tracing::info!("HNSW rebuild completed in {:?}", start_time.elapsed());
        Ok(())
    }

    // Kept for backward compatibility or direct calls if needed, though facade should use `HnswIndex` wrapper
}

#[async_trait]
impl VectorIndex for HnswIndex {
    async fn insert(&self, tx: TxId, id: DocId, embedding: &[f32]) -> Result<()> {
        if embedding.len() != self.config.dimension {
            return Err(MemFuseError::invalid_input(format!(
                "Expected dimension {}, got {}",
                self.config.dimension,
                embedding.len()
            )));
        }
        self.tx_buffer.stage(
            tx,
            IndexOp::Insert {
                doc_id: id,
                data: embedding.to_vec(),
            },
        );
        Ok(())
    }

    async fn search(&self, query: &[f32], k: usize) -> Result<Vec<ScoredDocument>> {
        if query.len() != self.config.dimension {
            return Err(MemFuseError::invalid_input(format!(
                "Expected dimension {}, got {}",
                self.config.dimension,
                query.len()
            )));
        }

        let entry = *self.entry_point.read();
        let entry_idx = match entry {
            Some(idx) => idx,
            None => return Ok(Vec::new()),
        };

        let max_layer = self.max_layer.load(Ordering::SeqCst) as usize;
        let mut ep = vec![entry_idx];

        for layer in (1..=max_layer).rev() {
            let best = self.search_layer(query, &ep, 1, layer)?;
            if !best.is_empty() {
                ep = vec![best[0].index];
            }
        }

        let candidates = self.search_layer(query, &ep, self.config.ef_search.max(k), 0)?;

        let nodes = self.nodes.read();
        let deleted = self.deleted_nodes.read();
        let mut results = Vec::new();

        for c in candidates.iter() {
            if deleted.contains(c.index as u64) {
                continue;
            }
            let doc_id = nodes[c.index].doc_id;
            let score = match self.config.distance_metric {
                DistanceMetric::Cosine => 1.0 - c.distance,
                DistanceMetric::Euclidean => 1.0 / (1.0 + c.distance),
                DistanceMetric::DotProduct => -c.distance,
            };
            results.push(ScoredDocument::new(doc_id, score));
            if results.len() >= k {
                break;
            }
        }

        Ok(results)
    }

    async fn search_filtered(
        &self,
        query: &[f32],
        k: usize,
        filter: Option<&(dyn Fn(DocId) -> bool + Send + Sync)>,
    ) -> Result<Vec<ScoredDocument>> {
        if query.len() != self.config.dimension {
            return Err(MemFuseError::invalid_input(format!(
                "Expected dimension {}, got {}",
                self.config.dimension,
                query.len()
            )));
        }

        let entry = *self.entry_point.read();
        let entry_idx = match entry {
            Some(idx) => idx,
            None => return Ok(Vec::new()),
        };

        let max_layer = self.max_layer.load(Ordering::SeqCst) as usize;
        let mut ep = vec![entry_idx];

        for layer in (1..=max_layer).rev() {
            let best = self.search_layer(query, &ep, 1, layer)?;
            if !best.is_empty() {
                ep = vec![best[0].index];
            }
        }

        // Over-fetch to compensate for filtered-out results
        let ef = self.config.ef_search.max(k) * 2;
        let candidates = self.search_layer(query, &ep, ef, 0)?;

        let nodes = self.nodes.read();
        let deleted = self.deleted_nodes.read();
        let mut results = Vec::new();

        for c in candidates.iter() {
            if deleted.contains(c.index as u64) {
                continue;
            }
            let doc_id = nodes[c.index].doc_id;
            if let Some(f) = filter {
                if !f(doc_id) {
                    continue;
                }
            }
            let score = match self.config.distance_metric {
                DistanceMetric::Cosine => 1.0 - c.distance,
                DistanceMetric::Euclidean => 1.0 / (1.0 + c.distance),
                DistanceMetric::DotProduct => -c.distance,
            };
            results.push(ScoredDocument::new(doc_id, score));
            if results.len() >= k {
                break;
            }
        }

        Ok(results)
    }

    async fn delete(&self, tx: TxId, id: DocId) -> Result<()> {
        self.tx_buffer.stage(
            tx,
            IndexOp::Delete {
                doc_id: id,
                data: None,
            },
        );
        Ok(())
    }

    async fn commit(&self, tx: TxId) -> Result<()> {
        let _lock = self.write_mutex.lock().await;
        let ops = self.tx_buffer.drain(tx);
        let mut deleted_any = false;

        for op in ops {
            match op {
                IndexOp::Insert { doc_id, data } => {
                    self.do_insert(doc_id, &data)?;
                }
                IndexOp::Delete { doc_id, .. } => {
                    self.do_delete(doc_id);
                    deleted_any = true;
                }
            }
        }

        if deleted_any && self.is_rebuild_required() {
            tracing::warn!(
                "HNSW index rebuild threshold reached (threshold: {:.2})",
                self.config.rebuild_threshold
            );
            self.trigger_rebuild_async();
        }

        Ok(())
    }

    async fn rollback(&self, tx: TxId) -> Result<()> {
        self.tx_buffer.discard(tx);
        Ok(())
    }

    async fn len(&self) -> usize {
        let total = self.nodes.read().len();
        let deleted = self.deleted_count.load(Ordering::SeqCst) as usize;
        total.saturating_sub(deleted)
    }

    async fn stats(&self) -> Result<VectorIndexStats> {
        let nodes = self.nodes.read();
        let deleted_count = self.deleted_count.load(Ordering::SeqCst) as usize;
        let num_vectors = nodes.len().saturating_sub(deleted_count);
        let vector_memory = nodes.len() * self.config.dimension * std::mem::size_of::<f32>();
        let connection_memory: usize = nodes
            .iter()
            .map(|n| {
                n.connections
                    .iter()
                    .map(|c| c.len() * std::mem::size_of::<usize>())
                    .sum::<usize>()
            })
            .sum();

        Ok(VectorIndexStats {
            num_vectors,
            memory_usage_bytes: vector_memory
                + connection_memory
                + (nodes.len() * std::mem::size_of::<HnswNode>()),
            num_layers: self.max_layer.load(Ordering::SeqCst) as usize + 1,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config(dim: usize) -> HnswConfig {
        HnswConfig {
            dimension: dim,
            max_elements: 10_000,
            m: 8,
            ef_construction: 100,
            ef_search: 50,
            distance_metric: DistanceMetric::Cosine,
            rebuild_threshold: 0.8,
        }
    }

    #[tokio::test]
    async fn test_insert_and_search() {
        let index = HnswIndex::new(test_config(4));
        let tx = TxId::new(1);

        // Insert 3 vectors
        index
            .insert(tx, DocId::new(1), &[1.0, 0.0, 0.0, 0.0])
            .await
            .expect("insert 1");
        index
            .insert(tx, DocId::new(2), &[0.0, 1.0, 0.0, 0.0])
            .await
            .expect("insert 2");
        index
            .insert(tx, DocId::new(3), &[0.9, 0.1, 0.0, 0.0])
            .await
            .expect("insert 3");
        index.commit(tx).await.expect("commit");

        // Search for vector closest to [1, 0, 0, 0]
        let results = index
            .search(&[1.0, 0.0, 0.0, 0.0], 2)
            .await
            .expect("search");
        assert!(!results.is_empty());
        assert_eq!(results[0].doc_id, DocId::new(1));
    }

    #[tokio::test]
    async fn test_delete() {
        let index = HnswIndex::new(test_config(4));

        let tx1 = TxId::new(1);
        index
            .insert(tx1, DocId::new(1), &[1.0, 0.0, 0.0, 0.0])
            .await
            .expect("insert");
        index.commit(tx1).await.expect("commit");

        assert_eq!(index.len().await, 1);

        let tx2 = TxId::new(2);
        index.delete(tx2, DocId::new(1)).await.expect("delete");
        index.commit(tx2).await.expect("commit");

        assert_eq!(index.len().await, 0);
    }

    #[tokio::test]
    async fn test_rollback() {
        let index = HnswIndex::new(test_config(4));

        let tx = TxId::new(1);
        index
            .insert(tx, DocId::new(1), &[1.0, 0.0, 0.0, 0.0])
            .await
            .expect("insert");
        index.rollback(tx).await.expect("rollback");

        assert_eq!(index.len().await, 0);
    }

    #[tokio::test]
    async fn test_empty_search() {
        let index = HnswIndex::new(test_config(4));
        let results = index
            .search(&[1.0, 0.0, 0.0, 0.0], 5)
            .await
            .expect("search");
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_dimension_mismatch() {
        let index = HnswIndex::new(test_config(4));
        let tx = TxId::new(1);
        let result = index.insert(tx, DocId::new(1), &[1.0, 0.0]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_filtered_search() {
        let index = HnswIndex::new(test_config(4));
        let tx = TxId::new(1);

        index
            .insert(tx, DocId::new(1), &[1.0, 0.0, 0.0, 0.0])
            .await
            .expect("test");
        index
            .insert(tx, DocId::new(2), &[0.9, 0.1, 0.0, 0.0])
            .await
            .expect("test");
        index
            .insert(tx, DocId::new(3), &[0.8, 0.2, 0.0, 0.0])
            .await
            .expect("test");
        index.commit(tx).await.expect("test");

        // Filtered: exclude DocId 1
        let filter_fn = |doc: DocId| doc.inner() != 1;
        let filter_ref: &(dyn Fn(DocId) -> bool + Send + Sync) = &filter_fn;
        let filtered = index
            .search_filtered(&[1.0, 0.0, 0.0, 0.0], 2, Some(filter_ref))
            .await
            .expect("test");
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().all(|r| r.doc_id != DocId::new(1)));
    }

    #[tokio::test]
    async fn test_rebuild_and_stats() {
        let index = HnswIndex::new(HnswConfig {
            dimension: 2,
            rebuild_threshold: 0.8,
            distance_metric: DistanceMetric::Euclidean,
            ..test_config(2)
        });
        let tx = TxId::new(1);

        for i in 1..=5u64 {
            index
                .insert(tx, DocId::new(i), &[i as f32, 0.0])
                .await
                .expect("test");
        }
        index.commit(tx).await.expect("test");

        assert_eq!(index.len().await, 5);
        assert!((index.connectivity_score() - 1.0).abs() < f64::EPSILON);
        assert!(!index.is_rebuild_required());

        // Delete 2 nodes → 40% deleted, connectivity = 0.6
        let tx2 = TxId::new(2);
        index.delete(tx2, DocId::new(2)).await.expect("test");
        index.delete(tx2, DocId::new(4)).await.expect("test");
        index.commit(tx2).await.expect("test");

        assert_eq!(index.len().await, 3);
        assert!(index.connectivity_score() < 0.8);
        assert!(index.is_rebuild_required());

        let stats_pre = index.stats().await.expect("test");
        assert_eq!(stats_pre.num_vectors, 3);

        // Rebuild
        index.rebuild().await.expect("test");

        assert_eq!(index.len().await, 3);
        assert!((index.connectivity_score() - 1.0).abs() < f64::EPSILON);
        assert!(!index.is_rebuild_required());

        let stats_post = index.stats().await.expect("test");
        assert_eq!(stats_post.num_vectors, 3);

        // Ensure rebuilt index still works
        let results = index.search(&[1.0, 0.0], 1).await.expect("test");
        assert_eq!(results[0].doc_id, DocId::new(1));
    }

    #[test]
    fn test_normalize() {
        use crate::distance::normalize_inplace;
        let mut v = vec![3.0, 4.0];
        normalize_inplace(&mut v);
        assert!((v[0] - 0.6).abs() < 1e-5);
        assert!((v[1] - 0.8).abs() < 1e-5);
    }
}
