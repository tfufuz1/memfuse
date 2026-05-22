//! HNSW (Hierarchical Navigable Small World) vector index.
//! # Hierarchical Navigable Small World (HNSW) Index
//!
//! This module implements the HNSW algorithm for efficient approximate nearest neighbor (ANN) search.
// ANCHOR:DOC:DOC-HNSW-001 — Module documentation added
// WP:WP-0.0 PRIO:3 NEEDS:NONE
// AGENT:03 DATE:2026-05-15 STATUS:DONE
// CREATED:2026-05-09 DEADLINE:NONE
// ANCHOR:ARCH:HNSW-001 — Hierarchical Navigable Small World Index.
// WP:WP-0.0 PRIO:1 NEEDS:NONE
// AGENT:01 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-05 DEADLINE:NONE
// IMPLEMENTS: VectorIndex Trait (memfuse-core/traits.rs)
// CONSTRUCT: Greedyensuche + Heuristik für Diversitätsauswahl der Nachbarn.
// SEARCH: Layer Descent (von max_layer bis 0), dann EF-Search in Layer 0.
// DELETE: Soft-Delete (Tombstone via deleted_nodes Roaring Bitmap).
// REBUILD-LOGIK: Wenn >20% gelöscht → async trigger_rebuild_async() -> Atomic Swap.
// TRANSAKTIONEN: Nutzt memfuse_core::TxBuffer zur Staging-Isolation.
//!
//! ## Key Components
//! - **HNSW Graph**: A multi-layered graph where the top layers provide coarse-grained navigation
//!   and the bottom layer (Layer 0) contains all data points for fine-grained search.
//! - **Greedy Search**: Each layer is traversed greedily to find the closest nodes to the query.
//! - **Ef Construction/Search**: Parameters that control the trade-off between search speed and recall.
//! - **Scalar Quantization (SQ8)**: Optional 8-bit quantization to reduce memory footprint by 4x.
//!
//! ## Features
//! - **Async Support**: Fully integrated with Tokio for non-blocking database operations.
//! - **Transactional**: Operations are buffered and committed atomically.
//! - **Dynamic Rebuild**: Automatically triggers a background rebuild when tombstone fragmentation is high.
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
use tokio::sync::Mutex;

/// Configuration parameters for the HNSW index.
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
    /// Whether to apply SQ8 Scalar Quantization to the index vectors to reduce RAM.
    pub quantize: bool,
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
            quantize: false,
        }
    }
}

impl HnswConfig {
    /// Validates that the configuration parameters are within acceptable bounds.
    pub fn validate(&self) -> Result<()> {
        // ANCHOR:ALG-FIX:D2-003 — ef_construction < M Guard fehlt
        // WP:WP-0.0 PRIO:1 NEEDS:NONE
        // AGENT:13 DATE:2026-05-08 STATUS:DONE
        // CREATED:2026-05-08 DEADLINE:NONE
        // INVARIANTE: ef_construction >= M (INV-HNSW-1)
        if self.ef_construction < self.m {
            return Err(MemFuseError::invalid_input(format!(
                "ef_construction ({}) must be >= m ({})",
                self.ef_construction, self.m
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
/// Represents the format of vector data stored in the index.
pub enum VectorData {
    /// Standard 32-bit floating point vectors.
    F32(Vec<f32>),
    /// 8-bit quantized vectors (SQ8).
    U8(Vec<u8>),
}

/// A node in the HNSW graph.
#[derive(Debug)]
struct HnswNode {
    doc_id: DocId,
    vector: VectorData,
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
        // ANCHOR:ALG-FIX:D2-005 — total_cmp statt unwrap_or(Equal) für NaN-Safety
        // WP:WP-0.0 PRIO:1 NEEDS:NONE
        // AGENT:13 DATE:2026-05-08 STATUS:DONE
        // CREATED:2026-05-08 DEADLINE:NONE
        // total_cmp gibt eine deterministische Ordnung für alle f32 inkl. NaN.
        self.distance.total_cmp(&other.distance)
    }
}

/// The HNSW (Hierarchical Navigable Small World) vector index.
pub struct HnswIndex {
    inner: std::sync::Arc<HnswIndexCore>,
}

impl std::ops::Deref for HnswIndex {
    type Target = HnswIndexCore;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

/// The core implementation of the HNSW index.
pub struct HnswIndexCore {
    config: HnswConfig,
    validation_error: Option<String>,
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
    quantizer: RwLock<Option<crate::quantize::ScalarQuantizer>>,
}

impl HnswIndex {
    /// Creates a new HNSW index.
    pub fn new(config: HnswConfig) -> Self {
        let validation_error = config.validate().err().map(|e| e.to_string());
        let ml = 1.0 / (config.m as f64).ln();
        Self {
            inner: std::sync::Arc::new(HnswIndexCore {
                config,
                validation_error,
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
                quantizer: RwLock::new(None),
            }),
        }
    }

    /// Triggers an async rebuild if the deletion threshold is exceeded.
    pub fn trigger_rebuild_async(&self) {
        if self.is_rebuild_required() {
            let inner = std::sync::Arc::clone(&self.inner);
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
        // ANCHOR:ALG-FIX:D2-002 — Guard gegen ln(0) = -∞ (INV-HNSW-2)
        // WP:WP-0.0 PRIO:1 NEEDS:NONE
        // AGENT:13 DATE:2026-05-08 STATUS:DONE
        // CREATED:2026-05-08 DEADLINE:NONE
        // rng.gen() gibt [0, 1) — bei r=0.0: ln(0)=-∞ → usize::MAX → OOM.
        // max(f64::EPSILON) verhindert diesen Grenzfall.
        let r: f64 = rng.r#gen::<f64>().max(f64::EPSILON);
        let layer = (-r.ln() * self.ml) as usize;
        layer.min(32) // Hard-cap: kein Graph braucht 32 Layer (= ~4 Mrd Knoten)
    }

    fn compute_distance_with_data(
        &self,
        query_exact: &[f32],
        query_quantized: Option<&[u8]>,
        data: &VectorData,
    ) -> Result<f32> {
        match data {
            VectorData::F32(v) => compute_distance(query_exact, v, self.config.distance_metric),
            VectorData::U8(v) => {
                let guard = self.quantizer.read();
                let q = guard.as_ref().ok_or_else(|| {
                    memfuse_core::MemFuseError::Index("Quantizer not trained".into())
                })?;
                if let Some(qq) = query_quantized {
                    q.symmetric_dist(qq, v, self.config.distance_metric)
                } else {
                    q.asymmetric_dist(query_exact, v, self.config.distance_metric)
                }
            }
        }
    }

    fn compute_symmetric_distance(&self, data_a: &VectorData, data_b: &VectorData) -> Result<f32> {
        match (data_a, data_b) {
            (VectorData::F32(a), VectorData::F32(b)) => {
                compute_distance(a, b, self.config.distance_metric)
            }
            (VectorData::U8(a), VectorData::U8(b)) => {
                let guard = self.quantizer.read();
                guard
                    .as_ref()
                    .ok_or_else(|| {
                        memfuse_core::MemFuseError::Index("Quantizer not trained".into())
                    })?
                    .symmetric_dist(a, b, self.config.distance_metric)
            }
            // ANCHOR:ALG-FIX:PANIC-001 — Mixed VectorData Guard (Zero-Panic Policy)
            // WP:WP-0.0 PRIO:1 NEEDS:NONE
            // AGENT:13 DATE:2026-05-09 STATUS:DONE
            // CREATED:2026-05-09 DEADLINE:NONE
            // FUNDORT: memfuse-index/src/hnsw.rs
            _ => Err(MemFuseError::Index(
                "Mixed vector representations (F32/U8) are not supported".into(),
            )),
        }
    }

    fn search_layer(
        &self,
        query: &[f32],
        query_quantized: Option<&[u8]>,
        entry_points: &[usize],
        ef: usize,
        layer: usize,
    ) -> Result<Vec<Candidate>> {
        let nodes = self.nodes.read();
        let mut visited = AHashSet::new();
        let mut candidates = BinaryHeap::new();
        let mut results = BinaryHeap::new();

        for &ep in entry_points {
            if visited.insert(ep) {
                // ANCHOR:SEC:SLICE-003 AGENT:10 PRIO:1 STATUS:READY
                // Safe access to nodes and connections.
                let node = nodes.get(ep).ok_or_else(|| {
                    MemFuseError::Index(format!("HNSW node missing at index {}", ep))
                })?;
                let dist = self.compute_distance_with_data(query, query_quantized, &node.vector)?;
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

            let current_node = nodes.get(current.index).ok_or_else(|| {
                MemFuseError::Index(format!(
                    "HNSW current node missing at index {}",
                    current.index
                ))
            })?;
            if let Some(connections) = current_node.connections.get(layer) {
                for &neighbor in connections {
                    if visited.insert(neighbor) {
                        let neighbor_node = nodes.get(neighbor).ok_or_else(|| {
                            MemFuseError::Index(format!(
                                "HNSW neighbor node missing at index {}",
                                neighbor
                            ))
                        })?;
                        let dist = self.compute_distance_with_data(
                            query,
                            query_quantized,
                            &neighbor_node.vector,
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
                let closest_node = nodes.get(closest.index).ok_or_else(|| {
                    MemFuseError::Index(format!(
                        "HNSW closest node missing at index {}",
                        closest.index
                    ))
                })?;
                let selected_node = nodes.get(selected.index).ok_or_else(|| {
                    MemFuseError::Index(format!(
                        "HNSW selected node missing at index {}",
                        selected.index
                    ))
                })?;
                let dist_between =
                    self.compute_symmetric_distance(&closest_node.vector, &selected_node.vector)?;
                if closest.distance > dist_between {
                    keep = false;
                    break;
                }
            }
            if keep {
                result.push(closest);
            }
        }

        // ANCHOR:ALG-FIX:D2-007 — Heuristic Fallback (Recall Guard)
        // WP:WP-0.0 PRIO:1 NEEDS:NONE
        // AGENT:13 DATE:2026-05-08 STATUS:DONE
        // CREATED:2026-05-08 DEADLINE:NONE
        // Wenn die Diversity-Heuristik zu aggressiv filtert (z.B. < M/2 Nachbarn),
        // fallen wir auf einfache KNN-Nachbarn zurück um Graph-Fragmentation zu vermeiden.
        // Wir garantieren hier mindestens m/2 Nachbarn, falls genug Kandidaten existieren.
        let min_neighbors = if self.config.quantize {
            m.saturating_sub(4)
        } else {
            m / 2
        };
        if result.len() < min_neighbors && !candidates.is_empty() {
            let mut fallback = result;
            for cand in candidates {
                if fallback.len() >= m {
                    break;
                }
                if !fallback.iter().any(|c| c.index == cand.index) {
                    fallback.push(*cand);
                }
            }
            return Ok(fallback.iter().map(|c| c.index).collect());
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

        // ANCHOR:ALG-FIX:D2-004 — NaN/Inf-Validierung bei Insert (Distanzfunktion)
        // WP:WP-0.0 PRIO:1 NEEDS:NONE
        // AGENT:13 DATE:2026-05-08 STATUS:DONE
        // CREATED:2026-05-08 DEADLINE:NONE
        // NaN-Vektoren würden in BinaryHeap stille Korrumpierung verursachen.
        // Validierung an der Grenze (insert) statt in distance.rs — distance bleibt rein.
        if vector.iter().any(|x| x.is_nan() || x.is_infinite()) {
            return Err(MemFuseError::invalid_input(
                "Vector contains NaN or Infinity values",
            ));
        }

        let vector_data = if self.config.quantize {
            if let Some(q) = self.quantizer.read().as_ref() {
                VectorData::U8(q.quantize(vector))
            } else {
                VectorData::F32(vector.to_vec())
            }
        } else {
            VectorData::F32(vector.to_vec())
        };

        let new_layer = self.random_layer();
        let entry_point_opt = *self.entry_point.read();
        let current_max_layer = self.max_layer.load(Ordering::SeqCst) as usize;

        let new_idx = {
            let mut nodes = self.nodes.write();
            let idx = nodes.len();
            nodes.push(HnswNode {
                doc_id: id,
                vector: vector_data,
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

        let query_quantized = if self.config.quantize {
            self.quantizer.read().as_ref().map(|q| q.quantize(vector))
        } else {
            None
        };

        let mut ep = vec![ep_idx];
        for layer in (new_layer + 1..=current_max_layer).rev() {
            let best = self.search_layer(vector, query_quantized.as_deref(), &ep, 1, layer)?;
            if let Some(closest) = best.first() {
                ep = vec![closest.index];
            }
        }

        // ANCHOR:ALG-FIX:D2-005 — TOCTOU bei concurrent Insert/Search
        // WP:WP-0.0 PRIO:1 NEEDS:NONE
        // AGENT:13 DATE:2026-05-08 STATUS:DONE
        // CREATED:2026-05-08 DEADLINE:NONE
        // INVARIANTE: ∀ Knoten v die während Search traversiert werden: v.neighbors ist vollständig
        // FIX: Insert generiert alle Kanten offline und committed in einem write-lock.
        let mut final_connections = vec![vec![]; new_layer + 1];

        for layer in (0..=new_layer.min(current_max_layer)).rev() {
            let neighbors = self.search_layer(
                vector,
                query_quantized.as_deref(),
                &ep,
                self.config.ef_construction,
                layer,
            )?;
            let selected = {
                let nodes = self.nodes.read();
                self.select_neighbors_heuristic(&nodes, &neighbors, self.config.m)?
            };
            if let Some(conn) = final_connections.get_mut(layer) {
                *conn = selected;
            } else {
                return Err(MemFuseError::Index(format!(
                    "HNSW final_connections missing for layer {}",
                    layer
                )));
            }
            ep = neighbors.iter().map(|c| c.index).collect();
        }

        {
            let mut nodes = self.nodes.write();
            if let Some(new_node) = nodes.get_mut(new_idx) {
                new_node.connections = final_connections.clone();
            } else {
                return Err(MemFuseError::Index(format!(
                    "HNSW new node missing at index {}",
                    new_idx
                )));
            }

            for layer in (0..=new_layer.min(current_max_layer)).rev() {
                let layer_neighbors = final_connections.get(layer).ok_or_else(|| {
                    MemFuseError::Index(format!(
                        "HNSW final_connections missing for layer {}",
                        layer
                    ))
                })?;
                for &neighbor_idx in layer_neighbors {
                    // Scope for neighbor modification to release mutable borrow
                    let (should_shrink, node_vec, conn_indices) = {
                        let neighbor_node = nodes.get_mut(neighbor_idx).ok_or_else(|| {
                            MemFuseError::Index(format!(
                                "HNSW neighbor node missing at index {}",
                                neighbor_idx
                            ))
                        })?;
                        if let Some(conn_layer) = neighbor_node.connections.get_mut(layer) {
                            conn_layer.push(new_idx);
                            if conn_layer.len() > self.config.m * 2 {
                                (true, neighbor_node.vector.clone(), conn_layer.clone())
                            } else {
                                (false, VectorData::F32(vec![]), vec![])
                            }
                        } else {
                            (false, VectorData::F32(vec![]), vec![])
                        }
                    };

                    if should_shrink {
                        let mut conn_cands = Vec::with_capacity(conn_indices.len());
                        for &idx in conn_indices.iter() {
                            let target_node = nodes.get(idx).ok_or_else(|| {
                                MemFuseError::Index(format!(
                                    "HNSW target node missing at index {}",
                                    idx
                                ))
                            })?;
                            let dist =
                                self.compute_symmetric_distance(&node_vec, &target_node.vector)?;
                            conn_cands.push(Candidate {
                                index: idx,
                                distance: dist,
                            });
                        }
                        let selected = self.select_neighbors_heuristic(
                            &nodes,
                            &conn_cands,
                            self.config.m * 2,
                        )?;

                        if let Some(neighbor_node) = nodes.get_mut(neighbor_idx) {
                            if let Some(cl) = neighbor_node.connections.get_mut(layer) {
                                *cl = selected;
                            }
                        }
                    }
                }
            }
        }

        if new_layer > current_max_layer {
            *self.entry_point.write() = Some(new_idx);
            self.max_layer.store(new_layer as u64, Ordering::SeqCst);
        }
        Ok(())
    }

    fn do_delete(&self, id: DocId) -> Result<()> {
        let node_idx = self.doc_to_node.write().remove(&id.inner());
        if let Some(idx) = node_idx {
            self.deleted_nodes.write().insert(idx as u64);
            self.deleted_count.fetch_add(1, Ordering::SeqCst);

            // ANCHOR:ALG-FIX:D2-001 — Entry-Point-Aktualisierung nach Delete (INV-HNSW-4)
            // WP:WP-0.0 PRIO:1 NEEDS:NONE
            // AGENT:13 DATE:2026-05-08 STATUS:DONE
            // CREATED:2026-05-08 DEADLINE:NONE
            // Wenn der gelöschte Knoten der Entry-Point war, muss ein neuer
            // Entry-Point gefunden werden. Strategie: Nachbar auf höchstem Layer.
            let mut ep = self.entry_point.write();
            if *ep == Some(idx) {
                let nodes = self.nodes.read();
                let deleted = self.deleted_nodes.read();
                // Try to find a neighbor of the deleted EP on any layer
                // HNSW requires the entry_point to be on the highest layer available.
                // Iterating globally guarantees we find the exact highest remaining node.
                let mut best_node = None;
                let mut max_layer = 0;
                for (i, node) in nodes.iter().enumerate() {
                    if i != idx && !deleted.contains(i as u64) && node._max_layer >= max_layer {
                        max_layer = node._max_layer;
                        best_node = Some(i);
                    }
                }
                *ep = best_node;
                if let Some(new_idx) = best_node {
                    let node = nodes.get(new_idx).ok_or_else(|| {
                        MemFuseError::Index(format!("HNSW node missing at index {}", new_idx))
                    })?;
                    self.max_layer
                        .store(node._max_layer as u64, Ordering::SeqCst);
                } else {
                    self.max_layer.store(0, Ordering::SeqCst);
                }
            }
        }
        Ok(())
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
                    active.push((node.doc_id, node.vector.clone()));
                }
            }
            (active, self.config.clone())
        };

        // 2. Build fresh index
        let new_index = HnswIndex::new(config);

        // Copy quantizer to new index to maintain parity
        let quantizer_guard = self.quantizer.read();
        if let Some(q) = quantizer_guard.as_ref() {
            *new_index.quantizer.write() = Some(q.clone());
        }

        for (doc_id, vector) in active_nodes {
            match vector {
                VectorData::F32(v) => {
                    new_index.do_insert(doc_id, &v)?;
                }
                VectorData::U8(v) => {
                    let dequantized = {
                        let q = quantizer_guard.as_ref().ok_or_else(|| {
                            MemFuseError::Index("Quantizer missing during rebuild".into())
                        })?;
                        q.dequantize(&v)
                    };
                    new_index.do_insert(doc_id, &dequantized)?;
                }
            }
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
        if let Some(ref err) = self.validation_error {
            return Err(MemFuseError::invalid_input(format!(
                "Invalid index configuration: {}",
                err
            )));
        }
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

    // ANCHOR:PERF:LATENCY-002 — HNSW Search Hotspot (Optimiert)
    // WP:WP-0.0 PRIO:2 NEEDS:NONE
    // AGENT:03 DATE:2026-05-15 STATUS:DONE
    // CREATED:2026-05-09 DEADLINE:NONE
    // TARGET: < 10ms bei 1M Vektoren
    // AKTUELL: Optimiert via Dynamic ef_search
    // BOTTLENECK: CPU / Cache Misses / ef_search Heuristik
    // FIX: Dynamische Anpassung von ef_search basierend auf Layer-Hierarchie.
    async fn search(&self, query: &[f32], k: usize) -> Result<Vec<ScoredDocument>> {
        if let Some(ref err) = self.validation_error {
            return Err(MemFuseError::invalid_input(format!(
                "Invalid index configuration: {}",
                err
            )));
        }
        if query.len() != self.config.dimension {
            return Err(MemFuseError::invalid_input(format!(
                "Expected dimension {}, got {}",
                self.config.dimension,
                query.len()
            )));
        }

        let query_quantized = if self.config.quantize {
            self.quantizer.read().as_ref().map(|q| q.quantize(query))
        } else {
            None
        };

        let entry = *self.entry_point.read();
        let entry_idx = match entry {
            Some(idx) => idx,
            None => return Ok(Vec::new()),
        };

        let max_layer = self.max_layer.load(Ordering::SeqCst) as usize;
        let mut ep = vec![entry_idx];

        for layer in (1..=max_layer).rev() {
            // Dynamische ef für Zwischenlayer (meist 1 ist ausreichend, aber für sehr tiefe Graphen kann leichtes Scaling helfen)
            let layer_ef = if layer > 1 { 1 } else { 2 };
            let best =
                self.search_layer(query, query_quantized.as_deref(), &ep, layer_ef, layer)?;
            if let Some(closest) = best.first() {
                ep = vec![closest.index];
            }
        }

        // Higher candidate list for reranking if quantized
        let ef = if self.config.quantize {
            self.config.ef_search.max(k) * 4
        } else {
            self.config.ef_search.max(k)
        };
        let candidates = self.search_layer(query, query_quantized.as_deref(), &ep, ef, 0)?;

        let score = self.connectivity_score();
        if score < self.config.rebuild_threshold {
            tracing::warn!("HNSW Index degraded: connectivity score {:.2} below threshold ({:.2}). Search quality may be reduced.", score, self.config.rebuild_threshold);
        }

        let nodes = self.nodes.read();
        let deleted = self.deleted_nodes.read();
        let mut results = Vec::with_capacity(k);

        for c in candidates.iter() {
            if deleted.contains(c.index as u64) {
                continue;
            }
            let node = nodes.get(c.index).ok_or_else(|| {
                MemFuseError::Index(format!("HNSW candidate node missing at index {}", c.index))
            })?;
            let doc_id = node.doc_id;

            // Phase 2: Exact Reranking (Asymmetric for SQ8)
            let final_dist = if self.config.quantize {
                if let VectorData::U8(v) = &node.vector {
                    let guard = self.quantizer.read();
                    let q = guard.as_ref().ok_or_else(|| {
                        memfuse_core::MemFuseError::Index("Quantizer not trained".into())
                    })?;
                    q.asymmetric_dist(query, v, self.config.distance_metric)?
                } else {
                    c.distance
                }
            } else {
                c.distance
            };

            let score = match self.config.distance_metric {
                DistanceMetric::Cosine => 1.0 - final_dist,
                DistanceMetric::Euclidean => 1.0 / (1.0 + final_dist),
                DistanceMetric::DotProduct => -final_dist,
            };
            results.push(ScoredDocument::new(doc_id, score));
        }

        // Must re-sort and truncate after Phase 2 reranking
        results.sort_by(|a, b| b.score.total_cmp(&a.score));
        results.truncate(k);

        Ok(results)
    }

    async fn search_filtered(
        &self,
        query: &[f32],
        k: usize,
        filter: Option<&(dyn Fn(DocId) -> bool + Send + Sync)>,
    ) -> Result<Vec<ScoredDocument>> {
        if let Some(ref err) = self.validation_error {
            return Err(MemFuseError::invalid_input(format!(
                "Invalid index configuration: {}",
                err
            )));
        }
        if query.len() != self.config.dimension {
            return Err(MemFuseError::invalid_input(format!(
                "Expected dimension {}, got {}",
                self.config.dimension,
                query.len()
            )));
        }

        let query_quantized = if self.config.quantize {
            self.quantizer.read().as_ref().map(|q| q.quantize(query))
        } else {
            None
        };

        let entry = *self.entry_point.read();
        let entry_idx = match entry {
            Some(idx) => idx,
            None => return Ok(Vec::new()),
        };

        let max_layer = self.max_layer.load(Ordering::SeqCst) as usize;
        let mut ep = vec![entry_idx];

        for layer in (1..=max_layer).rev() {
            let best = self.search_layer(query, query_quantized.as_deref(), &ep, 1, layer)?;
            if let Some(closest) = best.first() {
                ep = vec![closest.index];
            }
        }

        // Over-fetch to compensate for filtered-out results and reranking
        let factor = if self.config.quantize { 4 } else { 2 };
        let ef = self.config.ef_search.max(k) * factor;
        let candidates = self.search_layer(query, query_quantized.as_deref(), &ep, ef, 0)?;

        let score = self.connectivity_score();
        if score < self.config.rebuild_threshold {
            tracing::warn!("HNSW Index degraded: connectivity score {:.2} below threshold ({:.2}). Search quality may be reduced.", score, self.config.rebuild_threshold);
        }

        let nodes = self.nodes.read();
        let deleted = self.deleted_nodes.read();
        let mut results = Vec::with_capacity(k);

        for c in candidates.iter() {
            if deleted.contains(c.index as u64) {
                continue;
            }
            let node = nodes.get(c.index).ok_or_else(|| {
                MemFuseError::Index(format!("HNSW candidate node missing at index {}", c.index))
            })?;
            let doc_id = node.doc_id;
            if let Some(f) = filter {
                if !f(doc_id) {
                    continue;
                }
            }

            // Phase 2: Exact Reranking (Asymmetric for SQ8)
            let final_dist = if self.config.quantize {
                if let VectorData::U8(v) = &node.vector {
                    let guard = self.quantizer.read();
                    let q = guard.as_ref().ok_or_else(|| {
                        memfuse_core::MemFuseError::Index("Quantizer not trained".into())
                    })?;
                    q.asymmetric_dist(query, v, self.config.distance_metric)?
                } else {
                    c.distance
                }
            } else {
                c.distance
            };

            let score = match self.config.distance_metric {
                DistanceMetric::Cosine => 1.0 - final_dist,
                DistanceMetric::Euclidean => 1.0 / (1.0 + final_dist),
                DistanceMetric::DotProduct => -final_dist,
            };
            results.push(ScoredDocument::new(doc_id, score));
        }

        // Must re-sort and truncate after Phase 2 reranking
        results.sort_by(|a, b| b.score.total_cmp(&a.score));
        results.truncate(k);

        Ok(results)
    }

    async fn delete(&self, tx: TxId, id: DocId) -> Result<()> {
        if let Some(ref err) = self.validation_error {
            return Err(MemFuseError::invalid_input(format!(
                "Invalid index configuration: {}",
                err
            )));
        }
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
        if let Some(ref err) = self.validation_error {
            return Err(MemFuseError::invalid_input(format!(
                "Invalid index configuration: {}",
                err
            )));
        }
        let _lock = self.write_mutex.lock().await;
        let ops = self.tx_buffer.drain(tx);
        let mut deleted_any = false;

        // ANCHOR:SPEC:WP-2.2-SQ8TRAIN-001 — Lazy Training logic (Stabilized)
        // WP:WP-2.2 PRIO:2 NEEDS:NONE
        // AGENT:03 DATE:2026-05-15 STATUS:DONE
        // CREATED:2026-05-09 DEADLINE:NONE
        if self.config.quantize && self.quantizer.read().is_none() {
            let mut train_data = Vec::with_capacity(256.min(ops.len()));
            for op in &ops {
                if let IndexOp::Insert { data, .. } = op {
                    train_data.push(data.clone());
                    if train_data.len() >= 256 {
                        break;
                    }
                }
            }

            // If we don't have enough in this batch, check existing nodes
            if train_data.len() < 256 {
                let nodes = self.nodes.read();
                for node in nodes.iter() {
                    if let VectorData::F32(v) = &node.vector {
                        train_data.push(v.clone());
                        if train_data.len() >= 256 {
                            break;
                        }
                    }
                }
            }

            if train_data.len() >= 50 {
                let training_refs: Vec<&[f32]> = train_data.iter().map(|v| v.as_slice()).collect();
                let q =
                    crate::quantize::ScalarQuantizer::train(&training_refs, self.config.dimension);
                *self.quantizer.write() = Some(q.clone());

                let mut nodes = self.nodes.write();
                for node in nodes.iter_mut() {
                    if let VectorData::F32(v) = &node.vector {
                        node.vector = VectorData::U8(q.quantize(v));
                    }
                }
            }
        }

        for op in ops {
            match op {
                IndexOp::Insert { doc_id, data } => {
                    self.do_insert(doc_id, &data)?;
                }
                IndexOp::Delete { doc_id, .. } => {
                    self.do_delete(doc_id)?;
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
        if let Some(ref err) = self.validation_error {
            return Err(MemFuseError::invalid_input(format!(
                "Invalid index configuration: {}",
                err
            )));
        }
        self.tx_buffer.discard(tx);
        Ok(())
    }

    async fn len(&self) -> usize {
        if self.validation_error.is_some() {
            return 0;
        }
        let total = self.nodes.read().len();
        let deleted = self.deleted_count.load(Ordering::SeqCst) as usize;
        total.saturating_sub(deleted)
    }

    async fn stats(&self) -> Result<VectorIndexStats> {
        if let Some(ref err) = self.validation_error {
            return Err(MemFuseError::invalid_input(format!(
                "Invalid index configuration: {}",
                err
            )));
        }
        let nodes = self.nodes.read();
        let deleted_count = self.deleted_count.load(Ordering::SeqCst) as usize;
        let num_vectors = nodes.len().saturating_sub(deleted_count);
        let vector_memory: usize = nodes
            .iter()
            .map(|n| match &n.vector {
                VectorData::F32(v) => v.len() * std::mem::size_of::<f32>(),
                VectorData::U8(v) => v.len() * std::mem::size_of::<u8>(),
            })
            .sum();
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
            quantize: false,
        }
    }

    #[tokio::test]
    async fn test_invalid_config_error() {
        let config = HnswConfig {
            ef_construction: 5,
            m: 10, // Invalid: ef_construction < m
            ..test_config(4)
        };
        let index = HnswIndex::new(config);
        let tx = TxId::new(1);
        let result = index.insert(tx, DocId::new(1), &[1.0, 0.0, 0.0, 0.0]).await;
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("Invalid index configuration"));
        assert!(err_msg.contains("ef_construction (5) must be >= m (10)"));
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

    #[tokio::test]
    async fn test_rebuild_quantized_persistence() {
        let index = HnswIndex::new(HnswConfig {
            dimension: 4,
            quantize: true,
            rebuild_threshold: 0.1, // Trigger easily
            distance_metric: DistanceMetric::Euclidean,
            ..test_config(4)
        });
        let tx = TxId::new(1);

        // Insert enough vectors to train quantizer (>= 50)
        for i in 1..=60u64 {
            let v = [i as f32, 0.0, 0.0, 0.0];
            index.insert(tx, DocId::new(i), &v).await.expect("test");
        }
        index.commit(tx).await.expect("test");

        assert_eq!(index.len().await, 60);
        // Verify quantizer is trained
        assert!(index.quantizer.read().is_some());

        // Delete some to lower connectivity and allow rebuild
        let tx2 = TxId::new(2);
        for i in 1..=10u64 {
            index.delete(tx2, DocId::new(i)).await.expect("test");
        }
        index.commit(tx2).await.expect("test");

        assert_eq!(index.len().await, 50);

        // Rebuild
        index.rebuild().await.expect("rebuild");

        // Verify state after rebuild
        assert_eq!(index.len().await, 50);
        assert!(
            index.quantizer.read().is_some(),
            "Quantizer must be preserved"
        );

        // Verify search still works
        let results = index
            .search(&[60.0, 0.0, 0.0, 0.0], 1)
            .await
            .expect("search");
        assert_eq!(results[0].doc_id, DocId::new(60));
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
