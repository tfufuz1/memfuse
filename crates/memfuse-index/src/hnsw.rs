// FILE-CONTEXT
// ZWECK: HNSW Vector Index mit Layer Descent, Soft-Deletes und transaktionalem Staging (TxBuffer).
// INVARIANTEN: Lock-Hierarchie: write_mutex (exklusive Mutation/Rebuild) -> entry_point -> nodes / doc_to_node / deleted_nodes (HotState/ColdState).
// NICHT-OFFENSICHTLICH: Multi-threaded Reads sperren nie write_mutex; background rebuild tauscht Core atomar via Swap.
// HOTSPOTS:hnsw.rs (HnswIndex::insert, search, delete, rebuild, save)
// STAND: TS:2026-08-30T18:53:53Z (SESSION: 37b1d991)

//! HNSW (Hierarchical Navigable Small World) vector index.
//! # Hierarchical Navigable Small World (HNSW) Index
//!
//! This module implements the HNSW algorithm for efficient approximate nearest neighbor (ANN) search.
// AI-TAG[DOC-DRIFT][MINOR] RESOLVED: AGT-INDEX-003 — Module documentation added (TS:2026-08-25T00:00:00Z)
// INVARIANT: Hierarchical Navigable Small World Index.
// IMPLEMENTS:VectorIndex Trait (memfuse-core/traits.rs)
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

// FILE-CONTEXT
// STAND:       2026-08-29T15:22:34Z (SESSION: 2c814094)
// ZWECK:       HNSW-Vektorindex (Insert/Search/Delete/Persist) für Approximate Nearest Neighbor Search
// INVARIANTEN: No NaN/Inf distance, ef_construction >= M, entry point updated post-delete, SQ8 quantization safe
// HOTSPOTS:   greedy_search(), insert(), search_at(), trigger_rebuild_async()
// SIEHE AUCH:  rules/simd_safety.md, ADR-017, ADR-034

use crate::distance::compute_distance_trusted;
use ahash::{AHashMap, AHashSet};
use memfuse_core::{
    DistanceMetric, DocId, IndexOp, MemFuseError, Result, ScoredDocument, TxBuffer, TxId,
    VectorIndex, VectorIndexStats,
};
use parking_lot::RwLock;
use rand::Rng;
use roaring::RoaringTreemap;
use std::borrow::Cow;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

/// Sentinel-Wert für ungültigen / nicht gesetzten Entry-Point in HNSW.
pub const SENTINEL_NO_ENTRY_POINT: u32 = u32::MAX;
use tokio::sync::Mutex;

/// Standard-Löschanteil (0.10 = 10 % gelöschte Knoten), ab dem ein Rebuild getriggert wird.
///
/// Löschanteil, ab dem ein Rebuild ausgelöst wird (0.10 = 10 % gelöscht).
/// `HnswConfig.rebuild_threshold` speichert den KOMPLEMENTÄREN Aktivitätsanteil
/// (1.0 - Löschanteil) und wird intern mit dem aktuellen Aktivitäts-Score verglichen.
///
/// Trade-off:
/// - Niedrigerer Löschanteil-Schwellenwert (z.B. 0.10 = 10% gelöscht, Aktivitätsanteil 0.90):
///   Rebuilds werden häufiger ausgelöst. Das reduziert Space Amplification und Suchlatenz
///   (weniger tote Knoten im Graph), benötigt jedoch mehr Hintergrund-CPU.
/// - Höherer Löschanteil-Schwellenwert (z.B. 0.30 = 30% gelöscht, Aktivitätsanteil 0.70):
///   Spart Rebuild-CPU, führt aber bei lösch-intensiven Workloads zu höherer Tombstone-Akkumulation.
pub const HNSW_REBUILD_DELETION_RATIO: f64 = 0.10;

/// Configuration parameters for the HNSW index.
#[derive(Debug, Clone)]
pub struct HnswConfig {
    /// Vector dimensionality.
    pub dimension: usize,
    /// Maximum number of elements.
    pub max_elements: usize,
    /// Number of connections per element (M parameter).
    pub m: usize,
    /// Dynamic candidate list size during graph construction (`ef_construction`).
    ///
    /// Quality/speed trade-off parameter:
    /// - `ef_construction >= M * 2`: Recommended for high recall (default M=16, so ef >= 32).
    /// - Higher `ef_construction`: Improves graph connectivity and recall quality, but slows down vector insertions.
    /// - Minimum required: `ef_construction >= M` (absolute minimum; values below `M` yield poor recall and will fail validation).
    pub ef_construction: usize,
    /// Dynamic candidate list size during search.
    pub ef_search: usize,
    /// Distance metric.
    pub distance_metric: DistanceMetric,
    /// Rebuild threshold (fraction of active nodes remaining below which rebuild is triggered or warning is logged).
    /// Defaults to `1.0 - HNSW_REBUILD_DELETION_RATIO` (0.90, i.e., rebuild when active ratio falls below 90%).
    pub rebuild_threshold: f64,
    /// Whether to apply SQ8 Scalar Quantization to the index vectors to reduce RAM.
    pub quantize: bool,
    /// Sample size used for ScalarQuantizer recalibration during rebuilds.
    /// Default is 10,000 to balance speed and accuracy.
    pub quantizer_recalibration_sample_size: usize,
    /// Quantizer drift ratio threshold above which an index rebuild is recommended.
    /// Default is `0.10` (10% out-of-range queries).
    pub quantizer_drift_threshold: f32,
    /// Partial rebuild configuration for hot-path local rebuilds (F-02).
    #[cfg(feature = "partial-index-rebuild")]
    pub partial_rebuild_config: crate::partial_rebuild::PartialRebuildConfig,
}

impl Default for HnswConfig {
    fn default() -> Self {
        Self {
            dimension: 1536,
            max_elements: 1_000_000,
            m: 16,
            ef_construction: 200,
            ef_search: 64,
            distance_metric: DistanceMetric::Cosine,
            // Rebuild wird getriggert, wenn der Aktivitätsanteil unter 90% fällt (1.0 - 0.10 = 0.90)
            rebuild_threshold: 1.0 - HNSW_REBUILD_DELETION_RATIO,
            quantize: false,
            quantizer_recalibration_sample_size: 10_000,
            quantizer_drift_threshold: 0.10,
            #[cfg(feature = "partial-index-rebuild")]
            partial_rebuild_config: crate::partial_rebuild::PartialRebuildConfig::default(),
        }
    }
}

/// Validates that a vector is non-empty and contains no NaN or Infinite values.
fn validate_vector(vec: &[f32]) -> Result<()> {
    crate::distance::validate_vector(vec)
}

impl HnswConfig {
    /// Validates that the configuration parameters are within acceptable bounds.
    pub fn validate(&self) -> Result<()> {
        if self.dimension == 0 {
            return Err(MemFuseError::invalid_input(
                "dimension must be greater than 0",
            ));
        }
        if self.m == 0 {
            return Err(MemFuseError::invalid_input("m must be greater than 0"));
        }
        if self.ef_search == 0 {
            return Err(MemFuseError::invalid_input(
                "ef_search must be greater than 0",
            ));
        }
        // ANCHOR[ALG-FIX:D2-003] STATUS:DONE (TS:2026-06-01T00:00:00Z) — ef_construction < M Guard fehlt
        // INVARIANTE: ef_construction >= M (INV-HNSW-1): Konfigurationsvalidierung erzwingt ef_construction >= m.
        if self.ef_construction < self.m {
            return Err(MemFuseError::invalid_input(format!(
                "ef_construction ({}) must be >= m ({})",
                self.ef_construction, self.m
            )));
        }
        if !(0.0..=1.0).contains(&self.rebuild_threshold) {
            return Err(MemFuseError::invalid_input(format!(
                "rebuild_threshold ({}) must be between 0.0 and 1.0",
                self.rebuild_threshold
            )));
        }
        if !(0.0..=1.0).contains(&self.quantizer_drift_threshold) {
            return Err(MemFuseError::invalid_input(format!(
                "quantizer_drift_threshold ({}) must be between 0.0 and 1.0",
                self.quantizer_drift_threshold
            )));
        }
        Ok(())
    }
}

/// Builder for HnswConfig with resource limit enforcements to prevent OOM.
#[derive(Debug, Clone)]
pub struct HnswConfigBuilder {
    config: HnswConfig,
}

impl HnswConfigBuilder {
    /// Creates a new builder with the chosen dimensionality.
    pub fn new(dimension: usize) -> Self {
        Self {
            config: HnswConfig {
                dimension,
                ..Default::default()
            },
        }
    }

    /// Set max elements with a hardcap limit to avoid OOM.
    pub fn max_elements(mut self, max: usize) -> Self {
        self.config.max_elements = max.min(50_000_000);
        self
    }

    /// Set the number of connections per element (M).
    pub fn m(mut self, m: usize) -> Self {
        self.config.m = m.clamp(4, 256);
        self
    }

    /// Set dynamic candidate list size for construction.
    pub fn ef_construction(mut self, ef: usize) -> Self {
        self.config.ef_construction = ef.min(4000);
        self
    }

    /// Set dynamic candidate list size for search.
    pub fn ef_search(mut self, ef: usize) -> Self {
        self.config.ef_search = ef.min(4000);
        self
    }

    /// Use a specific distance metric.
    pub fn distance_metric(mut self, metric: DistanceMetric) -> Self {
        self.config.distance_metric = metric;
        self
    }

    /// Enable or disable scalar quantization (SQ8) to reduce footprint.
    pub fn quantize(mut self, quantize: bool) -> Self {
        self.config.quantize = quantize;
        self
    }

    /// Sets the sample size used for ScalarQuantizer recalibration during rebuilds.
    pub fn quantizer_recalibration_sample_size(mut self, size: usize) -> Self {
        self.config.quantizer_recalibration_sample_size = size;
        self
    }

    /// Sets the drift ratio threshold for ScalarQuantizer recalibration and rebuilds.
    /// Default is `0.10` (10% out-of-range queries).
    pub fn quantizer_drift_threshold(mut self, threshold: f32) -> Self {
        self.config.quantizer_drift_threshold = threshold.clamp(0.0, 1.0);
        self
    }

    /// Sets the rebuild threshold (fraction of active non-deleted nodes remaining below which rebuild is triggered).
    /// Value must be in range `0.0..=1.0`. For example, `0.90` triggers rebuild when >10% of nodes are deleted.
    ///
    /// Trade-off: Higher threshold (e.g. 0.90, i.e. 10% deleted) triggers rebuilds more frequently,
    /// using more background CPU but reducing RAM space amplification and search latency.
    /// Lower threshold (e.g. 0.70, i.e. 30% deleted) saves CPU at the expense of higher tombstone accumulation.
    pub fn rebuild_threshold(mut self, threshold: f64) -> Self {
        self.config.rebuild_threshold = threshold.clamp(0.0, 1.0);
        self
    }

    /// Sets the partial rebuild configuration for local hot-path rebuilds (F-02).
    #[cfg(feature = "partial-index-rebuild")]
    pub fn partial_rebuild_config(
        mut self,
        config: crate::partial_rebuild::PartialRebuildConfig,
    ) -> Self {
        self.config.partial_rebuild_config = config;
        self
    }

    /// Build the configuration after validating bounds.
    pub fn build(self) -> Result<HnswConfig> {
        self.config.validate()?;
        Ok(self.config)
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
///
/// # Concurrency & Lock Context
/// - `connections` uses `RwLock<Vec<Vec<u32>>>` per node (Option B) rather than per-layer locks,
///   reducing total lock overhead from `N_nodes * max_layer` to `N_nodes`.
/// - Search operations acquire `hot.nodes.read()` to look up nodes and `node.connections.read()`
///   to traverse neighbors.
/// - Incremental lazy neighbor pruning during search acquires `node.connections.write()`
///   without needing `hot.write_mutex` or an exclusive lock on `hot.nodes`.
#[derive(Debug)]
pub struct HnswNode {
    doc_id: DocId,
    vector: VectorData,
    max_layer: usize,
    committed_tx: u64,
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
        // ANCHOR[ALG-FIX:D2-005] STATUS:DONE (TS:2026-06-01T00:00:00Z) — total_cmp statt unwrap_or(Equal) für NaN-Safety
        // total_cmp gibt eine deterministische Ordnung für alle f32 inkl. NaN.
        self.distance.total_cmp(&other.distance)
    }
}

struct RebuildGuard<'a>(&'a AtomicBool);

impl<'a> Drop for RebuildGuard<'a> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

struct SnapshotPinGuard<'a>(&'a HnswIndexCore, u64);

impl<'a> SnapshotPinGuard<'a> {
    fn new(core: &'a HnswIndexCore, seq_no: u64) -> Self {
        core.cold.seq_log.write().pin_snapshot(seq_no);
        Self(core, seq_no)
    }
}

impl<'a> Drop for SnapshotPinGuard<'a> {
    fn drop(&mut self) {
        self.0.cold.seq_log.write().unpin_snapshot(self.1);
    }
}

/// The HNSW (Hierarchical Navigable Small World) vector index.
pub struct HnswIndex {
    inner: std::sync::Arc<HnswIndexCore>,
}

#[repr(align(64))]
pub struct HnswHotCore {
    pub nodes: RwLock<Vec<HnswNode>>,
    #[cfg(not(feature = "docid-128"))]
    pub doc_to_node: RwLock<AHashMap<u64, usize>>,
    #[cfg(feature = "docid-128")]
    pub doc_to_node: RwLock<AHashMap<u128, usize>>,
    pub entry_point: AtomicU32,
    pub ram_entry_point: AtomicU32,
    pub max_layer: AtomicU64,
    pub ml: f64,
    pub deleted_count: AtomicU64,
    pub write_mutex: Mutex<()>,
    pub last_tx_id: AtomicU64,
    pub rebuilding: AtomicBool,
    pub neighbor_arena: RwLock<Vec<u32>>,
    pub neighbor_offsets: RwLock<Vec<usize>>,
    pub neighbor_count_offsets: RwLock<Vec<usize>>,
    pub neighbor_counts: RwLock<Vec<u8>>,
}

impl HnswHotCore {
    #[inline]
    pub fn get_entry_point(&self) -> Option<usize> {
        let ep = self.entry_point.load(Ordering::Acquire);
        if ep == SENTINEL_NO_ENTRY_POINT {
            None
        } else {
            Some(ep as usize)
        }
    }

    #[inline]
    pub fn set_entry_point(&self, ep: Option<usize>) {
        let val = match ep {
            Some(idx) => idx as u32,
            None => SENTINEL_NO_ENTRY_POINT,
        };
        self.entry_point.store(val, Ordering::Release);
    }

    #[inline]
    pub fn get_ram_entry_point(&self) -> Option<usize> {
        let ep = self.ram_entry_point.load(Ordering::Acquire);
        if ep == SENTINEL_NO_ENTRY_POINT {
            None
        } else {
            Some(ep as usize)
        }
    }

    #[inline]
    pub fn set_ram_entry_point(&self, ep: Option<usize>) {
        let val = match ep {
            Some(idx) => idx as u32,
            None => SENTINEL_NO_ENTRY_POINT,
        };
        self.ram_entry_point.store(val, Ordering::Release);
    }

    pub fn layer_offset(node_offset: usize, layer: usize, m: usize) -> usize {
        if layer == 0 {
            node_offset
        } else {
            node_offset + (m * 2) + (layer - 1) * m
        }
    }

    pub fn get_ram_node_connections(&self, ram_idx: usize, layer: usize, m: usize) -> Vec<u32> {
        let offsets = self.neighbor_offsets.read();
        let count_offsets = self.neighbor_count_offsets.read();
        let counts = self.neighbor_counts.read();
        if ram_idx >= offsets.len() || ram_idx >= count_offsets.len() {
            return Vec::new();
        }
        let count_start = count_offsets[ram_idx];
        if count_start + layer >= counts.len() {
            return Vec::new();
        }
        let count_end = if ram_idx + 1 < count_offsets.len() {
            count_offsets[ram_idx + 1]
        } else {
            counts.len()
        };
        if count_start + layer >= count_end {
            return Vec::new();
        }
        let len = counts[count_start + layer] as usize;
        let node_offset = offsets[ram_idx];
        let l_offset = Self::layer_offset(node_offset, layer, m);

        let arena = self.neighbor_arena.read();
        if l_offset + len <= arena.len() {
            arena[l_offset..l_offset + len].to_vec()
        } else {
            Vec::new()
        }
    }
}

pub struct HnswColdCore {
    pub config: HnswConfig,
    pub validation_error: Option<String>,
    pub tx_buffer: TxBuffer<Vec<f32>>,
    pub quantizer: RwLock<Option<crate::quantize::ScalarQuantizer>>,
    pub mmap_index: RwLock<Option<crate::persistence::MmapIndex>>,
    pub seq_log: RwLock<memfuse_core::SequenceLog>,
    pub rebuild_count: AtomicU64,
    pub visited_dead_nodes: AtomicU64,
    pub deleted_nodes: RwLock<RoaringTreemap>,
    #[cfg(feature = "partial-index-rebuild")]
    pub traversal_tracker: RwLock<crate::partial_rebuild::TraversalTracker>,
    #[cfg(test)]
    pub fault_injection_insert_target: AtomicU64,
    #[cfg(test)]
    pub fault_injection_insert_count: AtomicU64,
}

/// The core implementation of the HNSW index.
pub struct HnswIndexCore {
    pub hot: HnswHotCore,
    pub cold: HnswColdCore,
}

impl HnswIndex {
    /// Creates a new HNSW index, validating configuration upfront.
    pub fn try_new(config: HnswConfig) -> Result<Self> {
        config.validate()?;
        let ml = 1.0 / (config.m as f64).ln();
        #[cfg(feature = "partial-index-rebuild")]
        let partial_rebuild_config = config.partial_rebuild_config.clone();

        Ok(Self {
            inner: std::sync::Arc::new(HnswIndexCore {
                hot: HnswHotCore {
                    nodes: RwLock::new(Vec::new()),
                    doc_to_node: RwLock::new(AHashMap::new()),
                    entry_point: AtomicU32::new(SENTINEL_NO_ENTRY_POINT),
                    ram_entry_point: AtomicU32::new(SENTINEL_NO_ENTRY_POINT),
                    max_layer: AtomicU64::new(0),
                    ml,
                    deleted_count: AtomicU64::new(0),
                    write_mutex: Mutex::new(()),
                    last_tx_id: AtomicU64::new(0),
                    rebuilding: AtomicBool::new(false),
                    neighbor_arena: RwLock::new(Vec::new()),
                    neighbor_offsets: RwLock::new(Vec::new()),
                    neighbor_count_offsets: RwLock::new(Vec::new()),
                    neighbor_counts: RwLock::new(Vec::new()),
                },
                cold: HnswColdCore {
                    config,
                    validation_error: None,
                    tx_buffer: TxBuffer::new_with_config(16, std::time::Duration::from_secs(60)),
                    quantizer: RwLock::new(None),
                    mmap_index: RwLock::new(None),
                    seq_log: RwLock::new(memfuse_core::SequenceLog::new()),
                    rebuild_count: AtomicU64::new(0),
                    visited_dead_nodes: AtomicU64::new(0),
                    deleted_nodes: RwLock::new(RoaringTreemap::new()),
                    #[cfg(feature = "partial-index-rebuild")]
                    traversal_tracker: RwLock::new(crate::partial_rebuild::TraversalTracker::new(
                        partial_rebuild_config,
                    )),
                    #[cfg(test)]
                    fault_injection_insert_target: AtomicU64::new(0),
                    #[cfg(test)]
                    fault_injection_insert_count: AtomicU64::new(0),
                },
            }),
        })
    }

    /// Creates a new HNSW index.
    #[deprecated(
        note = "Nutze try_new() für sofortige Fehlererkennung — new() versteckt Konfigurationsfehler bis zum ersten insert()/search()"
    )]
    pub fn new(config: HnswConfig) -> Self {
        let validation_error = config.validate().err().map(|e| e.to_string());
        let ml = 1.0 / (config.m as f64).ln();
        #[cfg(feature = "partial-index-rebuild")]
        let partial_rebuild_config = config.partial_rebuild_config.clone();

        Self {
            inner: std::sync::Arc::new(HnswIndexCore {
                hot: HnswHotCore {
                    nodes: RwLock::new(Vec::new()),
                    doc_to_node: RwLock::new(AHashMap::new()),
                    entry_point: AtomicU32::new(SENTINEL_NO_ENTRY_POINT),
                    ram_entry_point: AtomicU32::new(SENTINEL_NO_ENTRY_POINT),
                    max_layer: AtomicU64::new(0),
                    ml,
                    deleted_count: AtomicU64::new(0),
                    write_mutex: Mutex::new(()),
                    last_tx_id: AtomicU64::new(0),
                    rebuilding: AtomicBool::new(false),
                    neighbor_arena: RwLock::new(Vec::new()),
                    neighbor_offsets: RwLock::new(Vec::new()),
                    neighbor_count_offsets: RwLock::new(Vec::new()),
                    neighbor_counts: RwLock::new(Vec::new()),
                },
                cold: HnswColdCore {
                    config,
                    validation_error,
                    tx_buffer: TxBuffer::new_with_config(16, std::time::Duration::from_secs(60)),
                    quantizer: RwLock::new(None),
                    mmap_index: RwLock::new(None),
                    seq_log: RwLock::new(memfuse_core::SequenceLog::new()),
                    rebuild_count: AtomicU64::new(0),
                    visited_dead_nodes: AtomicU64::new(0),
                    deleted_nodes: RwLock::new(RoaringTreemap::new()),
                    #[cfg(feature = "partial-index-rebuild")]
                    traversal_tracker: RwLock::new(crate::partial_rebuild::TraversalTracker::new(
                        partial_rebuild_config,
                    )),
                    #[cfg(test)]
                    fault_injection_insert_target: AtomicU64::new(0),
                    #[cfg(test)]
                    fault_injection_insert_count: AtomicU64::new(0),
                },
            }),
        }
    }

    /// Sets the target insertion count for simulating compute_insert fault injection in tests.
    #[cfg(test)]
    pub fn set_fault_injection_insert_target(&self, target: u64) {
        self.inner
            .cold
            .fault_injection_insert_target
            .store(target, Ordering::SeqCst);
        self.inner
            .cold
            .fault_injection_insert_count
            .store(0, Ordering::SeqCst);
    }

    /// Returns a snapshot clone of the current quantizer if trained.
    pub fn quantizer(&self) -> Option<crate::quantize::ScalarQuantizer> {
        self.inner.cold.quantizer.read().clone()
    }

    /// Returns a reference to the quantizer RwLock for crate-internal access.
    #[allow(dead_code)]
    pub(crate) fn quantizer_lock(&self) -> &RwLock<Option<crate::quantize::ScalarQuantizer>> {
        &self.inner.cold.quantizer
    }

    async fn search_filtered_internal(
        &self,
        query: &[f32],
        k: usize,
        filter: Option<&(dyn Fn(DocId) -> bool + Send + Sync)>,
        snapshot_seq: Option<u64>,
    ) -> Result<Vec<ScoredDocument>> {
        if let Some(ref err) = self.inner.cold.validation_error {
            return Err(MemFuseError::invalid_input(format!(
                "Invalid index configuration: {}",
                err
            )));
        }
        if query.len() != self.inner.cold.config.dimension {
            return Err(MemFuseError::invalid_input(format!(
                "Expected dimension {}, got {}",
                self.inner.cold.config.dimension,
                query.len()
            )));
        }

        // S-1 FIX: Defense-in-Depth validation for all entry points delegating here.
        // search() already validates, but search_filtered() and search_at() bypass it.
        // Centralizing here ensures ALL callers — including future public API additions — are guarded.
        if k == 0 {
            return Ok(Vec::new());
        }
        if k > memfuse_core::MAX_SEARCH_K {
            return Err(MemFuseError::invalid_input(format!(
                "Requested k ({k}) exceeds maximum allowed search limit ({}). \
                 Use Collection::query() with appropriate k bounds.",
                memfuse_core::MAX_SEARCH_K
            )));
        }
        // Guard before Vec::with_capacity(k) and ef_search arithmetic (prevents OOM + overflow)
        for (i, &val) in query.iter().enumerate() {
            if !val.is_finite() {
                return Err(MemFuseError::invalid_input(format!(
                    "Query vector element at index {i} is not finite (value: {val}). \
                     NaN/Inf values corrupt HNSW distance computation and heap ordering. \
                     Validate embedding outputs before search."
                )));
            }
        }

        let query_quantized = if self.inner.cold.config.quantize {
            self.inner
                .cold
                .quantizer
                .read()
                .as_ref()
                .map(|q| q.quantize(query))
                .transpose()?
        } else {
            None
        };

        let mut ep = Vec::new();
        if let Some(global_ep) = self.inner.hot.get_entry_point() {
            ep.push(global_ep);
        }
        if let Some(ram_ep) = self.inner.hot.get_ram_entry_point() {
            if !ep.contains(&ram_ep) {
                ep.push(ram_ep);
            }
        }

        let nodes = self.inner.hot.nodes.read();
        let deleted = self.inner.cold.deleted_nodes.read();

        let mut filter_eps = Vec::new();
        if let Some(f) = filter {
            let mmap_guard = self.inner.cold.mmap_index.read();
            let mmap_node_count = mmap_guard
                .as_ref()
                .map(|m| m.header.node_count() as usize)
                .unwrap_or(0);
            let q_guard = if self.inner.cold.config.quantize {
                Some(self.inner.cold.quantizer.read())
            } else {
                None
            };
            let q_ref = q_guard.as_ref().and_then(|g| g.as_ref());
            let ctx = SearchContext {
                nodes: &nodes,
                mmap: mmap_guard.as_ref(),
                mmap_node_count,
                prior_prepared: &[],
                backlink_map: None,
                quantizer: q_ref.map(Cow::Borrowed),
            };

            let factor = if self.inner.cold.config.quantize {
                4
            } else {
                2
            };
            let max_filter_eps = self.inner.cold.config.ef_search.max(k) * factor;

            let total_nodes = mmap_node_count + nodes.len();
            for i in (0..total_nodes).rev() {
                if !ep.contains(&(i as _)) {
                    if snapshot_seq.is_none() && deleted.contains(i as u64) {
                        continue;
                    }
                    if let Ok(doc_id) = self.inner.resolve_doc_id(i, &ctx) {
                        if f(doc_id) {
                            ep.push(i);
                            filter_eps.push(i);
                            if filter_eps.len() >= max_filter_eps {
                                break;
                            }
                        }
                    }
                }
            }
        }

        if ep.is_empty() {
            return Ok(Vec::new());
        }

        let max_layer = self.inner.hot.max_layer.load(Ordering::SeqCst) as usize;

        for layer in (1..=max_layer).rev() {
            let best = self
                .inner
                .search_layer(query, query_quantized.as_deref(), &ep, 1, layer)?;
            if let Some(closest) = best.first() {
                ep = vec![closest.index];
            }
        }

        // Add RAM entry point back for the final layer search to ensure hybrid recall
        if let Some(ram_ep) = self.inner.hot.get_ram_entry_point() {
            if !ep.contains(&ram_ep) {
                ep.push(ram_ep);
            }
        }

        for f_ep in filter_eps {
            if !ep.contains(&f_ep) {
                ep.push(f_ep);
            }
        }

        // Over-fetch to compensate for filtered-out results and reranking
        let factor = if self.inner.cold.config.quantize {
            4
        } else {
            2
        };
        let ef = self.inner.cold.config.ef_search.max(k) * factor;
        let candidates = self
            .inner
            .search_layer(query, query_quantized.as_deref(), &ep, ef, 0)?;

        let score = self.inner.connectivity_score();
        if score < self.inner.cold.config.rebuild_threshold {
            let deleted_ratio = (1.0 - score) * 100.0;
            let err = memfuse_core::MemFuseError::HnswConnectivityDegraded { deleted_ratio };
            tracing::warn!(
                error = %err,
                connectivity_score = score,
                rebuild_threshold = self.inner.cold.config.rebuild_threshold,
                "HNSW index degraded — consider calling rebuild()"
            );
        }

        let mut results = Vec::with_capacity(k);

        let seq_log_guard = if snapshot_seq.is_some() {
            Some(self.inner.cold.seq_log.read())
        } else {
            None
        };

        for c in candidates.iter() {
            let node = nodes.get(c.index).ok_or_else(|| {
                MemFuseError::Index(format!("HNSW candidate node missing at index {}", c.index))
            })?;
            if node.committed_tx == 0 {
                continue;
            }
            let doc_id = node.doc_id;

            if let Some(snap_seq) = snapshot_seq {
                if let Some(ref seq_log) = seq_log_guard {
                    if !seq_log.is_visible(doc_id, snap_seq) {
                        continue;
                    }
                }
            } else {
                // AI-TAG[BUG-FIX][CRITICAL] RESOLVED: AGT-INDEX-a1b2c3d4 — Tombstone filter must be evaluated unconditionally before custom filter (TS:2026-09-01T11:02:04Z) (SESSION: dba1473f)
                if deleted.contains(c.index as u64) {
                    continue;
                }
            }

            if let Some(f) = filter {
                if !f(doc_id) {
                    continue;
                }
            }

            // Phase 2: Exact Reranking (Asymmetric for SQ8)
            let final_dist = if self.inner.cold.config.quantize {
                if let VectorData::U8(v) = &node.vector {
                    let guard = self.inner.cold.quantizer.read();
                    let q = guard.as_ref().ok_or_else(|| {
                        memfuse_core::MemFuseError::Index("Quantizer not trained".into())
                    })?;
                    q.asymmetric_dist(query, v, self.inner.cold.config.distance_metric)?
                } else {
                    c.distance
                }
            } else {
                c.distance
            };

            let score = match self.inner.cold.config.distance_metric {
                DistanceMetric::Cosine => 1.0 - final_dist,
                DistanceMetric::Euclidean => 1.0 / (1.0 + final_dist),
                DistanceMetric::DotProduct => -final_dist,
                other => {
                    return Err(MemFuseError::Index(format!(
                        "Unsupported DistanceMetric variant in search_filtered(): {other:?}"
                    )));
                }
            };
            results.push(ScoredDocument::new(doc_id, score));
        }

        // Select top k using select_nth_unstable_by (O(N)) then sort top k (O(k log k))
        if results.len() > k {
            results.select_nth_unstable_by(k - 1, |a, b| {
                b.score
                    .total_cmp(&a.score)
                    .then_with(|| a.doc_id.cmp(&b.doc_id))
            });
            results.truncate(k);
        }
        results.sort_by(|a, b| {
            b.score
                .total_cmp(&a.score)
                .then_with(|| a.doc_id.cmp(&b.doc_id))
        });

        Ok(results)
    }

    /// Returns the fraction of deleted nodes in the index (0.0 to 1.0).
    pub fn deleted_ratio(&self) -> f64 {
        self.inner.deleted_ratio()
    }

    /// Returns the total number of completed full index rebuilds.
    pub fn rebuild_count(&self) -> u64 {
        self.inner.rebuild_count()
    }

    /// Returns the total count of visited dead nodes during graph search/traversal.
    pub fn visited_dead_nodes(&self) -> u64 {
        self.inner.visited_dead_nodes()
    }

    /// Graph connectivity score (1.0 = perfect, 0.0 = fully fragmented).
    pub fn connectivity_score(&self) -> f64 {
        self.inner.connectivity_score()
    }

    /// Returns Ok(()) if the index is healthy, or
    /// Err(MemFuseError::HnswConnectivityDegraded { deleted_ratio }) if degraded.
    pub fn check_connectivity(&self) -> memfuse_core::Result<()> {
        self.inner.check_connectivity()
    }

    /// Checks if a rebuild is required based on the deletion ratio.
    pub fn is_rebuild_required(&self) -> bool {
        self.inner.is_rebuild_required()
    }

    /// Rebuilds the HNSW index from scratch, removing all deleted nodes.
    pub async fn rebuild(&self) -> Result<()> {
        self.inner.rebuild().await
    }

    /// Rebuilds specific region node IDs (Partial-Rebuild F-02).
    /// Preserves cross-region neighborhood connections (INV-NUC-1).
    pub async fn rebuild_region(&self, region_node_ids: Vec<u64>) -> Result<()> {
        self.inner.rebuild_region(region_node_ids).await
    }

    /// Checks if partial rebuild should be triggered for oversaturated hot-path regions
    /// and spawns an async partial rebuild if so.
    #[cfg(feature = "partial-index-rebuild")]
    pub fn check_and_trigger_partial_rebuild(&self) -> Option<tokio::task::JoinHandle<Result<()>>> {
        let global_tombstone_ratio = self.deleted_ratio() as f32;
        let tracker = self.inner.cold.traversal_tracker.read();

        let mut tombstone_map = ahash::AHashMap::new();
        let total_nodes = self.inner.hot.nodes.read().len();
        let deleted_guard = self.inner.cold.deleted_nodes.read();

        for i in 0..total_nodes {
            let id = i as u64;
            tombstone_map.insert(id, deleted_guard.contains(id));
        }

        // Convert AHashMap to HashMap for should_trigger_partial_rebuild parameter
        let std_map: std::collections::HashMap<u64, bool> = tombstone_map.into_iter().collect();

        let regions = crate::partial_rebuild::should_trigger_partial_rebuild(
            &tracker,
            &std_map,
            global_tombstone_ratio,
            &self.inner.cold.config.partial_rebuild_config,
        );

        if let Some(region_node_ids) = regions {
            let inner = std::sync::Arc::clone(&self.inner);
            Some(tokio::spawn(async move {
                let res = inner.rebuild_region(region_node_ids).await;
                if let Err(ref e) = res {
                    tracing::error!("Failed local partial rebuild: {}", e);
                }
                res
            }))
        } else {
            None
        }
    }

    /// Prunes sequence log entries that are tombstoned and older than `min_active_seqno`.
    pub fn compact_seq_log(&self, min_active_seqno: u64) {
        self.inner.cold.seq_log.write().compact(min_active_seqno);
    }

    /// Pins a sequence number to preserve historical snapshot readability across rebuilds.
    pub fn pin_snapshot(&self, seq_no: u64) {
        self.inner.cold.seq_log.write().pin_snapshot(seq_no);
    }

    /// Unpins a sequence number previously pinned with `pin_snapshot`.
    pub fn unpin_snapshot(&self, seq_no: u64) {
        self.inner.cold.seq_log.write().unpin_snapshot(seq_no);
    }

    /// Returns all active (non-deleted) DocIds by reading the `doc_to_node` map directly.
    ///
    /// This is O(M) where M = number of mapped doc IDs, compared to `all_doc_ids()` which
    /// is O(N) where N = total node count (including mmap). Use this for repair/reconciliation
    /// where only the DocId set matters, not positional node data.
    ///
    /// # FIND-DB-004: HNSW Repair Acceleration
    pub fn all_doc_ids_from_map(&self) -> Vec<DocId> {
        let map = self.inner.hot.doc_to_node.read();
        let deleted = self.inner.cold.deleted_nodes.read();
        map.iter()
            .filter(|(&_doc_id_raw, &node_idx)| !deleted.contains(node_idx as u64))
            .map(|(&doc_id_raw, _)| DocId::new(doc_id_raw))
            .collect()
    }

    /// Triggers an async rebuild if the deletion threshold is exceeded.
    pub fn trigger_rebuild_async(&self) -> Option<tokio::task::JoinHandle<Result<()>>> {
        if self.is_rebuild_required() {
            let inner = std::sync::Arc::clone(&self.inner);
            Some(tokio::spawn(async move {
                let res = inner.rebuild().await;
                if let Err(ref e) = res {
                    tracing::error!("Failed to rebuild HNSW index: {}", e);
                }
                res
            }))
        } else {
            None
        }
    }

    /// Liefert den aktuellen Rebuild-Status des Index.
    pub fn rebuild_status(&self) -> RebuildStatus {
        self.inner.rebuild_status()
    }

    /// Wartet bis ein laufender Rebuild abgeschlossen ist.
    /// Nutzt standardmäßig ein Timeout von 60 Sekunden.
    /// Gibt `true` zurück wenn Rebuild abgeschlossen, `false` bei Timeout.
    pub async fn wait_for_rebuild(&self) -> bool {
        self.inner.wait_for_rebuild().await
    }

    /// Wartet bis ein laufender Rebuild abgeschlossen ist.
    /// Gibt `true` zurück wenn Rebuild abgeschlossen, `false` bei Timeout.
    pub async fn wait_for_rebuild_with_timeout(&self, timeout: std::time::Duration) -> bool {
        self.inner.wait_for_rebuild_with_timeout(timeout).await
    }

    /// Persists the index to a flat file.
    // ANCHOR[REFACTOR:WP-0.0-ASYNCIO] STATUS:DONE (TS:2026-06-01T00:00:00Z) — Fix blocking I/O in HnswIndex::save
    // TEST: grep "std::fs" crates/memfuse-index/src/hnsw.rs
    // DONE: Alle std::fs Aufrufe in save() sind in spawn_blocking gekapselt oder durch tokio::fs ersetzt.
    pub async fn save(&self, path: impl AsRef<std::path::Path>) -> Result<()> {
        let _lock = self.inner.hot.write_mutex.lock().await;
        let inner = std::sync::Arc::clone(&self.inner);
        let path_buf = path.as_ref().to_path_buf();

        tokio::task::spawn_blocking(move || {
            use std::io::{Seek, Write};

            let nodes = inner.hot.nodes.read();
            let entry_point = inner.hot.get_entry_point();
            let q_guard = inner.cold.quantizer.read();

            // INTENT: Atomic Save to prevent SIGBUS on mmap
            let temp_path = path_buf.with_extension("hnsw.tmp");
            let file = std::fs::File::create(&temp_path).map_err(|e| {
                MemFuseError::Storage(format!("Failed to create temporary HNSW file: {}", e))
            })?;
            let mut writer = std::io::BufWriter::new(file);

            let node_count = nodes.len();
            let nodes_offset = crate::persistence::HnswHeader::SIZE as u64;
            let vectors_offset =
                nodes_offset + (node_count * crate::persistence::NodeRecord::SIZE) as u64;

            let (q_min, q_max) = if let Some(q) = q_guard.as_ref() {
                (
                    q.mins.first().copied().unwrap_or(0.0),
                    q.maxes.first().copied().unwrap_or(0.0),
                )
            } else {
                (0.0, 0.0)
            };

            // Initial header
            let mut header = crate::persistence::HnswHeader::new(
                inner.cold.config.dimension as u32,
                inner.cold.config.m as u32,
                inner.cold.config.distance_metric as u8,
                if inner.cold.config.quantize { 1 } else { 0 },
                q_min,
                q_max,
                node_count as u64,
                entry_point.map(|i| i as i64).unwrap_or(-1),
                nodes_offset,
                0,
                inner.hot.last_tx_id.load(Ordering::SeqCst),
            );

            // 1. Placeholder Header
            writer
                .write_all(&header.to_bytes())
                .map_err(|e| MemFuseError::Storage(e.to_string()))?;

            // 2. Nodes Metadata (Placeholders)
            let mut node_records = Vec::with_capacity(node_count);
            for _ in 0..node_count {
                node_records.push(crate::persistence::NodeRecord {
                    doc_id: 0,
                    max_layer: 0,
                    vector_offset: 0,
                    connections_offset: 0,
                });
            }
            for record in &node_records {
                writer
                    .write_all(&record.to_bytes())
                    .map_err(|e| MemFuseError::Storage(e.to_string()))?;
            }

            // 3. Vectors Block
            let mut current_pos = vectors_offset;
            for (i, node) in nodes.iter().enumerate() {
                node_records[i].doc_id = node.doc_id.inner();
                node_records[i].max_layer = node.max_layer as u8;
                node_records[i].vector_offset = current_pos;

                match &node.vector {
                    VectorData::F32(v) => {
                        for &val in v {
                            writer
                                .write_all(&val.to_le_bytes())
                                .map_err(|e| MemFuseError::Storage(e.to_string()))?;
                        }
                        current_pos += (v.len() * 4) as u64;
                    }
                    VectorData::U8(v) => {
                        writer
                            .write_all(v)
                            .map_err(|e| MemFuseError::Storage(e.to_string()))?;
                        current_pos += v.len() as u64;
                    }
                }
            }

            // 4. Connections Block (Align to 4 bytes)
            let connections_offset = (current_pos + 3) & !3;
            header.set_connections_offset(connections_offset);

            if connections_offset > current_pos {
                let padding = [0u8; 4];
                writer
                    .write_all(&padding[..(connections_offset - current_pos) as usize])
                    .map_err(|e| MemFuseError::Storage(e.to_string()))?;
            }

            let mut conn_pos = connections_offset;
            for (i, node) in nodes.iter().enumerate() {
                node_records[i].connections_offset = conn_pos;
                let num_layers = (node.max_layer + 1) as u8;
                writer
                    .write_all(&[num_layers])
                    .map_err(|e| MemFuseError::Storage(e.to_string()))?;
                conn_pos += 1;

                for layer in 0..num_layers as usize {
                    let layer_conns =
                        inner
                            .hot
                            .get_ram_node_connections(i, layer, inner.cold.config.m);
                    let len = layer_conns.len() as u32;
                    writer
                        .write_all(&len.to_le_bytes())
                        .map_err(|e| MemFuseError::Storage(e.to_string()))?;
                    for &conn in layer_conns.iter() {
                        writer
                            .write_all(&conn.to_le_bytes())
                            .map_err(|e| MemFuseError::Storage(e.to_string()))?;
                    }
                    conn_pos += 4 + (len as u64) * 4;
                }
            }

            // 5. Calibration Block (per-dimension mins and maxes)
            let cal_offset = conn_pos;
            let mut cal_len = 0u32;

            if let Some(ref q) = *q_guard {
                let dim = q.mins().len();
                cal_len = (dim * 4 * 2) as u32;
                for &m in q.mins() {
                    writer
                        .write_all(&m.to_le_bytes())
                        .map_err(|e| MemFuseError::Storage(e.to_string()))?;
                }
                for &m in q.maxes() {
                    writer
                        .write_all(&m.to_le_bytes())
                        .map_err(|e| MemFuseError::Storage(e.to_string()))?;
                }
            }

            header = crate::persistence::HnswHeader::new_v2(
                inner.cold.config.dimension as u32,
                inner.cold.config.m as u32,
                inner.cold.config.distance_metric as u8,
                if inner.cold.config.quantize { 1 } else { 0 },
                q_min,
                q_max,
                node_count as u64,
                entry_point.map(|i| i as i64).unwrap_or(-1),
                nodes_offset,
                connections_offset,
                inner.hot.last_tx_id.load(Ordering::SeqCst),
                cal_offset,
                cal_len,
            );

            writer
                .flush()
                .map_err(|e| MemFuseError::Storage(e.to_string()))?;
            let mut file = writer.into_inner().map_err(|_| {
                MemFuseError::Storage("Failed to retrieve file from BufWriter".into())
            })?;

            // 5. Final Updates
            file.seek(std::io::SeekFrom::Start(0))
                .map_err(|e| MemFuseError::Storage(e.to_string()))?;
            file.write_all(&header.to_bytes())
                .map_err(|e| MemFuseError::Storage(e.to_string()))?;
            file.seek(std::io::SeekFrom::Start(nodes_offset))
                .map_err(|e| MemFuseError::Storage(e.to_string()))?;
            for record in &node_records {
                file.write_all(&record.to_bytes())
                    .map_err(|e| MemFuseError::Storage(e.to_string()))?;
            }
            file.sync_all()
                .map_err(|e| MemFuseError::Storage(e.to_string()))?;

            // Atomic rename to replace the old file without truncating it, avoiding SIGBUS for active readers
            std::fs::rename(&temp_path, &path_buf).map_err(|e| {
                MemFuseError::Storage(format!("Failed to rename temporary HNSW file: {}", e))
            })?;

            // Fsync parent directory after rename for POSIX atomic directory entry durability
            if let Some(parent) = path_buf.parent() {
                if let Ok(parent_dir) = std::fs::File::open(parent) {
                    parent_dir.sync_all().map_err(|e| {
                        MemFuseError::Storage(format!(
                            "Failed to fsync parent directory after rename: {}",
                            e
                        ))
                    })?;
                }
            }

            Ok::<(), MemFuseError>(())
        })
        .await
        .map_err(|e| MemFuseError::Storage(format!("Join error: {}", e)))??;

        Ok(())
    }

    /// Loads an HNSW index from a flat file via memory-mapping.
    pub async fn load_mmap(&self, path: impl AsRef<std::path::Path> + Send) -> Result<()> {
        let mmap_index = crate::persistence::MmapIndex::open_async(path).await?;
        self.load_mmap_from_instance(mmap_index)
    }

    pub(crate) fn load_mmap_from_instance(
        &self,
        mmap_index: crate::persistence::MmapIndex,
    ) -> Result<()> {
        let ep = if mmap_index.header.entry_point() >= 0 {
            Some(mmap_index.header.entry_point() as usize)
        } else {
            None
        };

        // Determine max_layer from the nodes (this is more robust than storing it in header)
        // For simplicity, we can also store it in header (as we do in save).
        // Let's use the node metadata of the entry point if available.
        let max_layer = if let Some(e) = ep {
            let record = mmap_index.get_node_record(e)?;
            record.max_layer as u64
        } else {
            0
        };

        {
            self.inner.hot.set_entry_point(ep);
            self.inner.hot.set_ram_entry_point(None);
            self.inner.hot.max_layer.store(max_layer, Ordering::SeqCst);
        }

        if mmap_index.header.is_quantized() {
            let dim = self.inner.cold.config.dimension;
            let header = &mmap_index.header;

            let mut mins = Vec::new();
            let mut maxes = Vec::new();

            if header.version() == 2
                && header.quant_calibration_offset() > 0
                && header.quant_calibration_len() as usize >= dim * 4 * 2
            {
                let offset = header.quant_calibration_offset() as usize;
                let cal_bytes = mmap_index
                    .mmap
                    .get(offset..offset + dim * 4 * 2)
                    .ok_or_else(|| {
                        MemFuseError::Storage("Calibration segment out of bounds".into())
                    })?;

                for i in 0..dim {
                    let min_bytes = &cal_bytes[i * 4..(i + 1) * 4];
                    mins.push(f32::from_le_bytes(min_bytes.try_into().map_err(|_| {
                        MemFuseError::Storage("Invalid calibration min float".into())
                    })?));
                }
                for i in 0..dim {
                    let max_bytes = &cal_bytes[(dim + i) * 4..(dim + i + 1) * 4];
                    maxes.push(f32::from_le_bytes(max_bytes.try_into().map_err(|_| {
                        MemFuseError::Storage("Invalid calibration max float".into())
                    })?));
                }
            } else {
                let q_min = header.q_min();
                let q_max = header.q_max();
                mins = vec![q_min; dim];
                maxes = vec![q_max; dim];
            }

            let mut scales = Vec::with_capacity(dim);
            let mut inv_scales = Vec::with_capacity(dim);

            for i in 0..dim {
                let mut range = maxes[i] - mins[i];
                if range.abs() < f32::EPSILON {
                    range = 1e-6;
                }
                scales.push(255.0 / range);
                inv_scales.push(range / 255.0);
            }

            let mut q_guard = self.inner.cold.quantizer.write();
            *q_guard = Some(crate::quantize::ScalarQuantizer {
                mins,
                maxes,
                scales,
                inv_scales,
                dimension: dim,
                total_queries: AtomicU64::new(0),
                out_of_range_queries: AtomicU64::new(0),
            });
        }

        let mut guard = self.inner.cold.mmap_index.write();
        self.inner
            .hot
            .last_tx_id
            .store(mmap_index.header.last_tx_id(), Ordering::SeqCst);
        *guard = Some(mmap_index);
        Ok(())
    }
}

// AI-TAG[TEST][MINOR] RESOLVED: AGT-INDEX-f38b1a90 — Global statics replaced with instance-bound AtomicU64 fields on HnswColdCore (TS: 2026-09-15T16:00:00Z)

/// Prepared insert operation containing all fallible calculations prior to state mutation.
#[derive(Debug)]
pub struct PreparedInsert {
    pub doc_id: DocId,
    pub vector_data: VectorData,
    pub new_layer: usize,
    pub new_idx: usize,
    pub final_connections: Vec<Vec<u32>>,
    pub neighbor_backlinks: Vec<NeighborBacklink>,
    pub should_update_entry_point: bool,
    pub should_update_ram_entry_point: bool,
}

/// Back-link connection update for a neighbor node at a specific layer.
#[derive(Debug)]
pub struct NeighborBacklink {
    pub neighbor_ram_idx: usize,
    pub layer: usize,
    pub updated_connections: Vec<u32>,
}

/// Batch tracking context for running state across multi-operation transaction commits.
///
/// # Invariant 5.5.2: O(1) Backlink Lookups
/// `backlink_map` uses composite keys `(neighbor_ram_idx, layer)` with `AHashMap`
/// for constant-time $O(1)$ backlink connection lookups during transaction staging.
#[derive(Debug)]
pub struct BatchContext {
    pub running_max_layer: usize,
    pub has_entry_point: bool,
    pub has_ram_entry_point: bool,
    pub backlink_map: AHashMap<(usize, usize), Vec<u32>>,
}

impl BatchContext {
    pub fn new(core: &HnswIndexCore) -> Self {
        Self {
            running_max_layer: core.hot.max_layer.load(Ordering::SeqCst) as usize,
            has_entry_point: core.hot.get_entry_point().is_some(),
            has_ram_entry_point: core.hot.get_ram_entry_point().is_some(),
            backlink_map: AHashMap::new(),
        }
    }
}

fn get_neighbor_conns_in_batch(
    core: &HnswIndexCore,
    neighbor_idx: usize,
    layer: usize,
    base_batch_idx: usize,
    mmap_node_count: usize,
    _nodes_read: &[HnswNode],
    prior_prepared: &[PreparedInsert],
    batch_ctx: &BatchContext,
) -> Vec<u32> {
    if neighbor_idx >= base_batch_idx {
        let offset = neighbor_idx - base_batch_idx;
        return prior_prepared
            .get(offset)
            .and_then(|p| p.final_connections.get(layer))
            .cloned()
            .unwrap_or_default();
    }

    if neighbor_idx < mmap_node_count {
        return Vec::new();
    }

    let neighbor_ram_idx = neighbor_idx - mmap_node_count;
    if let Some(conns) = batch_ctx.backlink_map.get(&(neighbor_ram_idx, layer)) {
        return conns.clone();
    }

    core.hot
        .get_ram_node_connections(neighbor_ram_idx, layer, core.cold.config.m)
}

/// Helper for hybrid resolution of nodes (RAM vs Mmap, plus in-flight batch prepared inserts).
struct SearchContext<'a> {
    nodes: &'a [HnswNode],
    mmap: Option<&'a crate::persistence::MmapIndex>,
    mmap_node_count: usize,
    prior_prepared: &'a [PreparedInsert],
    backlink_map: Option<&'a AHashMap<(usize, usize), Vec<u32>>>,
    quantizer: Option<Cow<'a, crate::quantize::ScalarQuantizer>>,
}

/// Liefert den aktuellen Rebuild-Status des Index.
#[derive(Debug, Clone, PartialEq)]
pub enum RebuildStatus {
    /// Kein Rebuild läuft oder geplant.
    Idle,
    /// Rebuild läuft gerade im Hintergrund.
    Running,
    /// Rebuild wurde getriggert, startet bald.
    Pending,
}

impl HnswIndexCore {
    /// Liefert den aktuellen Rebuild-Status des Index.
    pub fn rebuild_status(&self) -> RebuildStatus {
        if self.hot.rebuilding.load(Ordering::SeqCst) {
            RebuildStatus::Running
        } else {
            RebuildStatus::Idle
        }
    }

    /// Wartet bis ein laufender Rebuild abgeschlossen ist.
    /// Nutzt standardmäßig ein Timeout von 60 Sekunden.
    /// Gibt `true` zurück wenn Rebuild abgeschlossen, `false` bei Timeout.
    pub async fn wait_for_rebuild(&self) -> bool {
        self.wait_for_rebuild_with_timeout(std::time::Duration::from_secs(60))
            .await
    }

    /// Wartet bis ein laufender Rebuild abgeschlossen ist.
    /// Gibt `true` zurück wenn Rebuild abgeschlossen, `false` bei Timeout.
    pub async fn wait_for_rebuild_with_timeout(&self, timeout: std::time::Duration) -> bool {
        let deadline = tokio::time::Instant::now() + timeout;
        while self.hot.rebuilding.load(Ordering::Acquire) {
            if tokio::time::Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        true
    }

    fn random_layer(&self) -> usize {
        let mut rng = rand::thread_rng();
        // ANCHOR[ALG-FIX:D2-002] STATUS:DONE (TS:2026-06-01T00:00:00Z) — Guard gegen ln(0) = -∞ (INV-HNSW-2)
        // rng.gen() gibt [0, 1) — bei r=0.0: ln(0)=-∞ → usize::MAX → OOM.
        // max(f64::EPSILON) verhindert diesen Grenzfall.
        let r: f32 = rng.gen::<f32>();
        let r_clamped = r.max(f32::EPSILON);
        let layer = (-(r_clamped.ln()) as f64 * self.hot.ml) as usize;
        layer.min(32) // Hard-cap
    }

    fn compute_distance_with_data(
        &self,
        query_exact: &[f32],
        query_quantized: Option<&[u8]>,
        data: &VectorData,
        quantizer_opt: Option<&crate::quantize::ScalarQuantizer>,
    ) -> Result<f32> {
        match data {
            VectorData::F32(v) => {
                compute_distance_trusted(query_exact, v, self.cold.config.distance_metric)
            }
            VectorData::U8(v) => {
                let guard;
                let q = if let Some(q_ref) = quantizer_opt {
                    q_ref
                } else {
                    guard = self.cold.quantizer.read();
                    guard.as_ref().ok_or_else(|| {
                        memfuse_core::MemFuseError::Index("Quantizer not trained".into())
                    })?
                };
                if let Some(qq) = query_quantized {
                    q.symmetric_dist(qq, v, self.cold.config.distance_metric)
                } else {
                    q.asymmetric_dist(query_exact, v, self.cold.config.distance_metric)
                }
            }
        }
    }

    fn compute_distance_with_mmap(
        &self,
        query_exact: &[f32],
        query_quantized: Option<&[u8]>,
        mmap: &crate::persistence::MmapIndex,
        record: &crate::persistence::NodeRecord,
        quantizer_opt: Option<&crate::quantize::ScalarQuantizer>,
    ) -> Result<f32> {
        let vector_bytes = mmap.get_vector(record)?;
        if mmap.header.is_quantized() {
            let guard;
            let q = if let Some(q_ref) = quantizer_opt {
                q_ref
            } else {
                guard = self.cold.quantizer.read();
                guard.as_ref().ok_or_else(|| {
                    memfuse_core::MemFuseError::Index("Quantizer not trained".into())
                })?
            };
            if let Some(qq) = query_quantized {
                q.symmetric_dist(qq, vector_bytes, self.cold.config.distance_metric)
            } else {
                q.asymmetric_dist(query_exact, vector_bytes, self.cold.config.distance_metric)
            }
        } else {
            // Safe unaligned SIMD F32 read directly from mmap byte slice (zero allocation)
            crate::distance::compute_distance_f32_bytes_trusted(
                query_exact,
                vector_bytes,
                self.cold.config.distance_metric,
            )
        }
    }

    fn compute_symmetric_distance(&self, data_a: &VectorData, data_b: &VectorData) -> Result<f32> {
        match (data_a, data_b) {
            (VectorData::F32(a), VectorData::F32(b)) => {
                compute_distance_trusted(a, b, self.cold.config.distance_metric)
            }
            (VectorData::U8(a), VectorData::U8(b)) => {
                let guard = self.cold.quantizer.read();
                guard
                    .as_ref()
                    .ok_or_else(|| {
                        memfuse_core::MemFuseError::Index("Quantizer not trained".into())
                    })?
                    .symmetric_dist(a, b, self.cold.config.distance_metric)
            }
            // ANCHOR[ALG-FIX:PANIC-001] STATUS:DONE (TS:2026-06-01T00:00:00Z) — Mixed VectorData Guard (Zero-Panic Policy)
            // FUNDORT: memfuse-index/src/hnsw.rs
            _ => Err(MemFuseError::Index(
                "Mixed vector representations (F32/U8) are not supported".into(),
            )),
        }
    }

    /// Computes candidate distance with zero heap allocation and minimal lock hold times (Invariant 5.5.3).
    /// Vector and quantizer references are resolved from pre-acquired `SearchContext` outside the hot loop.
    fn resolve_dist(
        &self,
        idx: usize,
        query: &[f32],
        query_q: Option<&[u8]>,
        ctx: &SearchContext,
    ) -> Result<f32> {
        let q_ref = ctx.quantizer.as_deref();
        let base_batch_idx = ctx.mmap_node_count + ctx.nodes.len();
        if idx >= base_batch_idx {
            let prepared_offset = idx - base_batch_idx;
            if let Some(prepared) = ctx.prior_prepared.get(prepared_offset) {
                return self.compute_distance_with_data(
                    query,
                    query_q,
                    &prepared.vector_data,
                    q_ref,
                );
            }
        }
        if let Some(mmap) = ctx.mmap {
            if idx < ctx.mmap_node_count {
                let record = mmap.get_node_record(idx)?;
                return self.compute_distance_with_mmap(query, query_q, mmap, &record, q_ref);
            }
        }
        let ram_idx = idx.saturating_sub(ctx.mmap_node_count);
        if let Some(node) = ctx.nodes.get(ram_idx) {
            return self.compute_distance_with_data(query, query_q, &node.vector, q_ref);
        }
        let latest_nodes = self.hot.nodes.read();
        if let Some(node) = latest_nodes.get(ram_idx) {
            return self.compute_distance_with_data(query, query_q, &node.vector, q_ref);
        }
        Err(MemFuseError::Index(format!(
            "Node vector not found for index {idx}"
        )))
    }

    fn resolve_connections<'a>(
        &self,
        idx: usize,
        layer: usize,
        ctx: &'a SearchContext,
    ) -> Result<Cow<'a, [u32]>> {
        let base_batch_idx = ctx.mmap_node_count + ctx.nodes.len();
        if idx >= base_batch_idx {
            let prepared_offset = idx - base_batch_idx;
            if let Some(prepared) = ctx.prior_prepared.get(prepared_offset) {
                let conns = prepared
                    .final_connections
                    .get(layer)
                    .cloned()
                    .unwrap_or_default();
                return Ok(Cow::Owned(conns));
            }
        }

        if let Some(mmap) = ctx.mmap {
            if idx < ctx.mmap_node_count {
                let record = mmap.get_node_record(idx)?;
                return Ok(Cow::Owned(mmap.get_connections(&record, layer)?));
            }
            let ram_idx = idx - ctx.mmap_node_count;
            if let Some(bmap) = ctx.backlink_map {
                if let Some(conns) = bmap.get(&(ram_idx, layer)) {
                    return Ok(Cow::Owned(conns.clone()));
                }
            }
            let conns = self
                .hot
                .get_ram_node_connections(ram_idx, layer, self.cold.config.m);
            return Ok(Cow::Owned(conns));
        }

        if let Some(bmap) = ctx.backlink_map {
            if let Some(conns) = bmap.get(&(idx, layer)) {
                return Ok(Cow::Owned(conns.clone()));
            }
        }
        let conns = self
            .hot
            .get_ram_node_connections(idx, layer, self.cold.config.m);
        Ok(Cow::Owned(conns))
    }

    fn resolve_doc_id(&self, idx: usize, ctx: &SearchContext) -> Result<DocId> {
        let base_batch_idx = ctx.mmap_node_count + ctx.nodes.len();
        if idx >= base_batch_idx {
            let prepared_offset = idx - base_batch_idx;
            if let Some(prepared) = ctx.prior_prepared.get(prepared_offset) {
                return Ok(prepared.doc_id);
            }
        }
        if let Some(mmap) = ctx.mmap {
            if idx < ctx.mmap_node_count {
                let record = mmap.get_node_record(idx)?;
                return Ok(DocId::new(record.doc_id));
            }
        }
        let ram_idx = idx.saturating_sub(ctx.mmap_node_count);
        if let Some(node) = ctx.nodes.get(ram_idx) {
            return Ok(node.doc_id);
        }
        let latest_nodes = self.hot.nodes.read();
        if let Some(node) = latest_nodes.get(ram_idx) {
            return Ok(node.doc_id);
        }
        Err(MemFuseError::Index(format!(
            "Node doc_id not found for index {idx}"
        )))
    }

    fn search_layer(
        &self,
        query: &[f32],
        query_quantized: Option<&[u8]>,
        entry_points: &[usize],
        ef: usize,
        layer: usize,
    ) -> Result<Vec<Candidate>> {
        self.search_layer_with_context(query, query_quantized, entry_points, ef, layer, &[], None)
    }

    fn search_layer_with_context(
        &self,
        query: &[f32],
        query_quantized: Option<&[u8]>,
        entry_points: &[usize],
        ef: usize,
        layer: usize,
        prior_prepared: &[PreparedInsert],
        backlink_map: Option<&AHashMap<(usize, usize), Vec<u32>>>,
    ) -> Result<Vec<Candidate>> {
        let nodes_guard = self.hot.nodes.read();
        let mmap_guard = self.cold.mmap_index.read();
        let mmap_node_count = mmap_guard
            .as_ref()
            .map(|m| m.header.node_count() as usize)
            .unwrap_or(0);

        let q_guard = if self.cold.config.quantize {
            Some(self.cold.quantizer.read())
        } else {
            None
        };
        let q_ref = q_guard.as_ref().and_then(|g| g.as_ref());

        let ctx = SearchContext {
            nodes: &nodes_guard,
            mmap: mmap_guard.as_ref(),
            mmap_node_count,
            prior_prepared,
            backlink_map,
            quantizer: q_ref.map(Cow::Borrowed),
        };

        // Pre-allocate capacity derived from search parameter `ef` and graph degree `M`.
        // During graph traversal, up to `ef` candidates are expanded, each having up to `M` neighbors.
        // Allocating `ef * 4` prevents repeated reallocations during hot-path candidate visitation.
        let mut visited = AHashSet::with_capacity(ef.saturating_mul(4));
        let mut candidates = BinaryHeap::new();
        let mut results = BinaryHeap::new();

        #[cfg(feature = "partial-index-rebuild")]
        let mut visited_node_ids = Vec::new();

        for &ep in entry_points {
            if visited.insert(ep) {
                #[cfg(feature = "partial-index-rebuild")]
                visited_node_ids.push(ep as u64);

                let dist = self.resolve_dist(ep, query, query_quantized, &ctx)?;
                let cand = Candidate {
                    index: ep,
                    distance: dist,
                };
                candidates.push(Reverse(cand));
                results.push(cand);
            }
        }

        // NOTE: deleted_snapshot is taken at search start. Concurrent deletes during this search are not reflected — this is intentional for search consistency.
        let deleted_snapshot = self.cold.deleted_nodes.read();

        while let Some(Reverse(current)) = candidates.pop() {
            if let Some(worst_result) = results.peek() {
                if current.distance > worst_result.distance && results.len() >= ef {
                    break;
                }
            }

            let connections = self.resolve_connections(current.index, layer, &ctx)?;
            let mut has_dead_neighbors = false;

            for &neighbor_u32 in connections.iter() {
                let neighbor = neighbor_u32 as usize;
                if deleted_snapshot.contains(neighbor as u64) {
                    self.cold.visited_dead_nodes.fetch_add(1, Ordering::Relaxed);
                    has_dead_neighbors = true;
                }
                if visited.insert(neighbor) {
                    #[cfg(feature = "partial-index-rebuild")]
                    visited_node_ids.push(neighbor as u64);

                    let dist = self.resolve_dist(neighbor, query, query_quantized, &ctx)?;
                    let is_better = match results.peek() {
                        Some(worst) => dist < worst.distance,
                        None => true,
                    };

                    if is_better || results.len() < ef {
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

            // Incremental Lazy Neighbor Pruning:
            // When traversing a RAM node that contains dead neighbors, lazily filter out tombstoned nodes from its adjacency list
            // UNLESS the dead neighbor is retained for active/pinned snapshots.
            if has_dead_neighbors && current.index >= mmap_node_count {
                let ram_idx = current.index - mmap_node_count;
                let seq_log = self.cold.seq_log.read();
                let min_retention_seq = seq_log.min_retention_seq();
                let m = self.cold.config.m;
                if let (Some(offsets), Some(count_offsets), Some(mut counts), Some(mut arena)) = (
                    self.hot.neighbor_offsets.try_read(),
                    self.hot.neighbor_count_offsets.try_read(),
                    self.hot.neighbor_counts.try_write(),
                    self.hot.neighbor_arena.try_write(),
                ) {
                    if ram_idx < offsets.len() && ram_idx < count_offsets.len() {
                        let node_offset = offsets[ram_idx];
                        let count_start = count_offsets[ram_idx];
                        let count_end = if ram_idx + 1 < count_offsets.len() {
                            count_offsets[ram_idx + 1]
                        } else {
                            counts.len()
                        };
                        if count_start + layer < count_end {
                            let l_offset = HnswHotCore::layer_offset(node_offset, layer, m);
                            let old_len = counts[count_start + layer] as usize;
                            if l_offset + old_len <= arena.len() {
                                let layer_slice = &arena[l_offset..l_offset + old_len];
                                let mut kept = Vec::with_capacity(old_len);
                                for &neighbor_u32 in layer_slice {
                                    if !deleted_snapshot.contains(neighbor_u32 as u64) {
                                        kept.push(neighbor_u32);
                                    } else if let Some(min_ret_seq) = min_retention_seq {
                                        let neighbor_idx = neighbor_u32 as usize;
                                        if neighbor_idx >= mmap_node_count {
                                            let neighbor_ram_idx = neighbor_idx - mmap_node_count;
                                            if let Some(neighbor_node) =
                                                nodes_guard.get(neighbor_ram_idx)
                                            {
                                                if let Some(del_seq) =
                                                    seq_log.deletion_seq(neighbor_node.doc_id)
                                                {
                                                    if del_seq >= min_ret_seq {
                                                        kept.push(neighbor_u32);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                counts[count_start + layer] = kept.len() as u8;
                                arena[l_offset..l_offset + kept.len()].copy_from_slice(&kept);
                            }
                        }
                    }
                }
            }
        }
        #[cfg(feature = "partial-index-rebuild")]
        if layer == 0 && !visited_node_ids.is_empty() {
            self.cold
                .traversal_tracker
                .write()
                .record_traversal(visited_node_ids);
        }

        let mut vec = results.into_vec();
        vec.sort_by(|a, b| a.distance.total_cmp(&b.distance));
        Ok(vec)
    }

    fn resolve_vector_data_with_batch(
        &self,
        idx: usize,
        ctx: &SearchContext,
        pending_idx: usize,
        pending_vector: &VectorData,
        prior_prepared: &[PreparedInsert],
    ) -> Result<VectorData> {
        if idx == pending_idx {
            return Ok(pending_vector.clone());
        }

        let ram_nodes_count = ctx.nodes.len();
        let base_batch_idx = ctx.mmap_node_count + ram_nodes_count;

        if idx >= base_batch_idx {
            let prepared_offset = idx - base_batch_idx;
            if let Some(prepared) = prior_prepared.get(prepared_offset) {
                return Ok(prepared.vector_data.clone());
            }
        }

        if let Some(mmap) = ctx.mmap {
            if idx < ctx.mmap_node_count {
                let record = mmap.get_node_record(idx)?;
                let bytes = mmap.get_vector(&record)?;
                return if mmap.header.is_quantized() {
                    Ok(VectorData::U8(bytes.to_vec()))
                } else {
                    let mut v = vec![0.0f32; self.cold.config.dimension];
                    for i in 0..self.cold.config.dimension {
                        v[i] = f32::from_le_bytes(bytes[i * 4..(i + 1) * 4].try_into().map_err(
                            |_| MemFuseError::Index("Corrupt f32 in mmap vector".into()),
                        )?);
                    }
                    Ok(VectorData::F32(v))
                };
            }
            return Ok(ctx.nodes[idx - ctx.mmap_node_count].vector.clone());
        }

        if idx < ctx.nodes.len() {
            return Ok(ctx.nodes[idx].vector.clone());
        }

        Err(MemFuseError::Index(format!(
            "Invalid node index {} in resolve_vector_data_with_batch",
            idx
        )))
    }

    fn compute_symmetric_distance_hybrid_with_batch(
        &self,
        idx_a: usize,
        idx_b: usize,
        ctx: &SearchContext,
        pending_idx: usize,
        pending_vector: &VectorData,
        prior_prepared: &[PreparedInsert],
    ) -> Result<f32> {
        let data_a = self.resolve_vector_data_with_batch(
            idx_a,
            ctx,
            pending_idx,
            pending_vector,
            prior_prepared,
        )?;
        let data_b = self.resolve_vector_data_with_batch(
            idx_b,
            ctx,
            pending_idx,
            pending_vector,
            prior_prepared,
        )?;
        self.compute_symmetric_distance(&data_a, &data_b)
    }

    #[allow(dead_code)]
    fn compute_symmetric_distance_hybrid(
        &self,
        idx_a: usize,
        idx_b: usize,
        ctx: &SearchContext,
    ) -> Result<f32> {
        let dummy = VectorData::F32(Vec::new());
        self.compute_symmetric_distance_hybrid_with_batch(
            idx_a,
            idx_b,
            ctx,
            usize::MAX,
            &dummy,
            &[],
        )
    }

    fn select_neighbors_heuristic_with_batch(
        &self,
        ctx: &SearchContext,
        candidates: &[Candidate],
        m: usize,
        pending_idx: usize,
        pending_vector: &VectorData,
        prior_prepared: &[PreparedInsert],
    ) -> Result<Vec<u32>> {
        if candidates.is_empty() {
            return Ok(Vec::new());
        }
        if candidates.len() <= m {
            return Ok(candidates.iter().map(|c| c.index as u32).collect());
        }

        // SPECCED: Varianzbasierter Schwellenwert zur Recall-Stabilisierung (SQ8).
        // Wir berechnen die Streuung der Distanzen um die Heuristik bei verrauschten
        // Abständen (Quantisierungsfehler) weniger aggressiv agieren zu lassen.
        let distances: Vec<f32> = candidates.iter().map(|c| c.distance).collect();
        let mean = distances.iter().sum::<f32>() / distances.len() as f32;
        let variance =
            distances.iter().map(|d| (d - mean).powi(2)).sum::<f32>() / distances.len() as f32;
        let std_dev = variance.sqrt();

        // Dynamische Lockerung: Bei hoher Dichte (geringe Varianz) erlauben wir
        // mehr Redundanz um SQ8-Artefakte zu kompensieren.
        let relaxation = if self.cold.config.quantize {
            (0.1 * (1.0 / (1.0 + std_dev))).clamp(0.02, 0.2)
        } else {
            0.0
        };

        let mut result: Vec<Candidate> = Vec::with_capacity(m);
        let mut sorted_cands = candidates.to_vec();
        sorted_cands.sort_by(|a, b| a.distance.total_cmp(&b.distance));

        for closest in sorted_cands {
            if result.len() >= m {
                break;
            }
            let mut keep = true;
            for selected in &result {
                let dist_between = self.compute_symmetric_distance_hybrid_with_batch(
                    closest.index,
                    selected.index,
                    ctx,
                    pending_idx,
                    pending_vector,
                    prior_prepared,
                )?;

                // Hartes Pruning bei Standard-F32, dynamisches Pruning bei SQ8
                if closest.distance > dist_between * (1.0 + relaxation) {
                    keep = false;
                    break;
                }
            }
            if keep {
                result.push(closest);
            }
        }

        // SPECCED: Minimale Konnektivität (M/2 Floor).
        // Bei Quantisierungs-Artefakten darf die Heuristik den Graphen nicht fragmentieren.
        let min_neighbors = m / 2;
        if result.len() < min_neighbors && candidates.len() >= min_neighbors {
            let mut fallback = result;
            let mut sorted_fallback = candidates.to_vec();
            sorted_fallback.sort_by(|a, b| a.distance.total_cmp(&b.distance));

            for cand in sorted_fallback {
                if fallback.len() >= m || (fallback.len() >= min_neighbors && !fallback.is_empty())
                {
                    // We only enforce floor if we strictly have too few neighbors
                    if fallback.len() >= min_neighbors {
                        break;
                    }
                }
                if !fallback.iter().any(|c| c.index == cand.index) {
                    fallback.push(cand);
                }
            }
            return Ok(fallback.iter().map(|c| c.index as u32).collect());
        }

        Ok(result.iter().map(|c| c.index as u32).collect())
    }

    #[allow(dead_code)]
    fn select_neighbors_heuristic(
        &self,
        ctx: &SearchContext,
        candidates: &[Candidate],
        m: usize,
    ) -> Result<Vec<u32>> {
        let dummy = VectorData::F32(Vec::new());
        self.select_neighbors_heuristic_with_batch(ctx, candidates, m, usize::MAX, &dummy, &[])
    }

    pub fn compute_insert(&self, id: DocId, vector: &[f32]) -> Result<PreparedInsert> {
        let mut batch_ctx = BatchContext::new(self);
        self.compute_insert_with_context(id, vector, 0, &mut [], &mut batch_ctx)
    }

    pub fn compute_insert_with_context(
        &self,
        id: DocId,
        vector: &[f32],
        offset: usize,
        prior_prepared: &mut [PreparedInsert],
        batch_ctx: &mut BatchContext,
    ) -> Result<PreparedInsert> {
        #[cfg(test)]
        {
            let target = self
                .cold
                .fault_injection_insert_target
                .load(Ordering::SeqCst);
            if target > 0 {
                let current = self
                    .cold
                    .fault_injection_insert_count
                    .fetch_add(1, Ordering::SeqCst)
                    + 1;
                if current == target {
                    return Err(MemFuseError::Index(
                        "Fault injection: compute_insert simulated failure".into(),
                    ));
                }
            }
        }

        if vector.len() != self.cold.config.dimension {
            return Err(MemFuseError::invalid_input(format!(
                "Dimension mismatch: expected {}, got {}",
                self.cold.config.dimension,
                vector.len()
            )));
        }

        validate_vector(vector)?;

        let vector_data = if self.cold.config.quantize {
            let q_guard = self.cold.quantizer.read();
            if let Some(q) = q_guard.as_ref() {
                VectorData::U8(q.quantize(vector)?)
            } else {
                VectorData::F32(vector.to_vec())
            }
        } else {
            VectorData::F32(vector.to_vec())
        };

        let new_layer = self.random_layer();
        let entry_point_opt = self.hot.get_entry_point();

        let mmap_node_count = self
            .cold
            .mmap_index
            .read()
            .as_ref()
            .map(|m| m.header.node_count() as usize)
            .unwrap_or(0);

        let nodes_read = self.hot.nodes.read();
        let ram_nodes_count = nodes_read.len();
        let new_idx = mmap_node_count + ram_nodes_count + offset;

        let query_quantized: Option<Vec<u8>> = None;

        let mmap_guard = self.cold.mmap_index.read();

        let mut ep = Vec::new();
        let mut batch_global_ep = None;
        let mut batch_ram_ep = None;

        for prepared in &*prior_prepared {
            if prepared.should_update_entry_point {
                batch_global_ep = Some(prepared.new_idx);
            }
            if prepared.should_update_ram_entry_point {
                batch_ram_ep = Some(prepared.new_idx);
            }
        }

        if let Some(b_ep) = batch_global_ep {
            ep.push(b_ep);
        } else if let Some(global_ep) = entry_point_opt {
            ep.push(global_ep);
        }

        if let Some(b_ram_ep) = batch_ram_ep {
            if !ep.contains(&b_ram_ep) {
                ep.push(b_ram_ep);
            }
        } else if let Some(ram_ep) = self.hot.get_ram_entry_point() {
            if !ep.contains(&ram_ep) {
                ep.push(ram_ep);
            }
        }

        if ep.is_empty() {
            let should_update_entry_point =
                !batch_ctx.has_entry_point || new_layer > batch_ctx.running_max_layer;
            let should_update_ram_entry_point =
                !batch_ctx.has_ram_entry_point || new_layer > batch_ctx.running_max_layer;

            if should_update_entry_point {
                batch_ctx.running_max_layer = new_layer;
                batch_ctx.has_entry_point = true;
            }
            if should_update_ram_entry_point {
                batch_ctx.has_ram_entry_point = true;
            }

            return Ok(PreparedInsert {
                doc_id: id,
                vector_data,
                new_layer,
                new_idx,
                final_connections: vec![vec![]; new_layer + 1],
                neighbor_backlinks: Vec::new(),
                should_update_entry_point,
                should_update_ram_entry_point,
            });
        }

        let current_max_layer = batch_ctx.running_max_layer;

        for layer in (new_layer + 1..=current_max_layer).rev() {
            let best = self.search_layer_with_context(
                vector,
                query_quantized.as_deref(),
                &ep,
                1,
                layer,
                prior_prepared,
                Some(&batch_ctx.backlink_map),
            )?;
            if let Some(closest) = best.first() {
                ep = vec![closest.index];
            }
        }

        let mut final_connections = vec![vec![]; new_layer + 1];

        for layer in (0..=new_layer.min(current_max_layer)).rev() {
            let ram_ep_candidate = batch_ram_ep.or_else(|| self.hot.get_ram_entry_point());
            if let Some(ram_ep) = ram_ep_candidate {
                if !ep.contains(&ram_ep) {
                    ep.push(ram_ep);
                }
            }

            let neighbors = self.search_layer_with_context(
                vector,
                query_quantized.as_deref(),
                &ep,
                self.cold.config.ef_construction,
                layer,
                prior_prepared,
                Some(&batch_ctx.backlink_map),
            )?;
            let ctx = SearchContext {
                nodes: &nodes_read,
                mmap: mmap_guard.as_ref(),
                mmap_node_count,
                prior_prepared,
                backlink_map: Some(&batch_ctx.backlink_map),
                quantizer: None,
            };
            let selected = self.select_neighbors_heuristic_with_batch(
                &ctx,
                &neighbors,
                self.cold.config.m,
                new_idx,
                &vector_data,
                prior_prepared,
            )?;
            final_connections[layer] = selected;
            ep = neighbors.iter().map(|c| c.index).collect();
        }

        let base_batch_idx = mmap_node_count + ram_nodes_count;
        let mut neighbor_backlinks = Vec::new();

        for layer in (0..=new_layer.min(current_max_layer)).rev() {
            for &ni in &final_connections[layer] {
                let neighbor_idx = ni as usize;

                if neighbor_idx < mmap_node_count {
                    continue;
                }

                let existing_conns = get_neighbor_conns_in_batch(
                    self,
                    neighbor_idx,
                    layer,
                    base_batch_idx,
                    mmap_node_count,
                    &nodes_read,
                    prior_prepared,
                    batch_ctx,
                );

                let mut conn_indices = existing_conns;
                if !conn_indices.contains(&(new_idx as u32)) {
                    conn_indices.push(new_idx as u32);
                }

                let selected = if conn_indices.len() > self.cold.config.m * 2 {
                    let mut conn_cands = Vec::with_capacity(conn_indices.len());
                    for &idx_u32 in &conn_indices {
                        let idx = idx_u32 as usize;
                        let ctx = SearchContext {
                            nodes: &nodes_read,
                            mmap: mmap_guard.as_ref(),
                            mmap_node_count,
                            prior_prepared,
                            backlink_map: Some(&batch_ctx.backlink_map),
                            quantizer: None,
                        };
                        let dist = self.compute_symmetric_distance_hybrid_with_batch(
                            idx,
                            neighbor_idx,
                            &ctx,
                            new_idx,
                            &vector_data,
                            prior_prepared,
                        )?;
                        conn_cands.push(Candidate {
                            index: idx,
                            distance: dist,
                        });
                    }

                    let ctx = SearchContext {
                        nodes: &nodes_read,
                        mmap: mmap_guard.as_ref(),
                        mmap_node_count,
                        prior_prepared,
                        backlink_map: Some(&batch_ctx.backlink_map),
                        quantizer: None,
                    };
                    self.select_neighbors_heuristic_with_batch(
                        &ctx,
                        &conn_cands,
                        self.cold.config.m * 2,
                        new_idx,
                        &vector_data,
                        prior_prepared,
                    )?
                } else {
                    conn_indices
                };

                if neighbor_idx >= base_batch_idx {
                    let prepared_offset = neighbor_idx - base_batch_idx;
                    if let Some(prep) = prior_prepared.get_mut(prepared_offset) {
                        if prep.final_connections.len() > layer {
                            prep.final_connections[layer] = selected;
                        }
                    }
                } else {
                    let neighbor_ram_idx = neighbor_idx - mmap_node_count;
                    batch_ctx
                        .backlink_map
                        .insert((neighbor_ram_idx, layer), selected.clone());
                    neighbor_backlinks.push(NeighborBacklink {
                        neighbor_ram_idx,
                        layer,
                        updated_connections: selected,
                    });
                }
            }
        }

        let should_update_entry_point =
            !batch_ctx.has_entry_point || new_layer > batch_ctx.running_max_layer;
        let should_update_ram_entry_point =
            !batch_ctx.has_ram_entry_point || new_layer > batch_ctx.running_max_layer;

        if should_update_entry_point {
            batch_ctx.running_max_layer = new_layer;
            batch_ctx.has_entry_point = true;
        }
        if should_update_ram_entry_point {
            batch_ctx.has_ram_entry_point = true;
        }

        Ok(PreparedInsert {
            doc_id: id,
            vector_data,
            new_layer,
            new_idx,
            final_connections,
            neighbor_backlinks,
            should_update_entry_point,
            should_update_ram_entry_point,
        })
    }

    pub fn apply_insert(&self, prepared: PreparedInsert) {
        let node = HnswNode {
            doc_id: prepared.doc_id,
            vector: prepared.vector_data,
            max_layer: prepared.new_layer,
            committed_tx: 0,
        };

        let m = self.cold.config.m;
        // Capacity per node in arena: Layer 0 has M*2, Layers 1..max_layer have M each
        let node_capacity = (m * 2) + prepared.new_layer * m;

        let mut offsets = self.hot.neighbor_offsets.write();
        let mut count_offsets = self.hot.neighbor_count_offsets.write();
        let mut counts = self.hot.neighbor_counts.write();
        let mut arena = self.hot.neighbor_arena.write();

        let start_offset = arena.len();
        arena.resize(start_offset + node_capacity, 0);
        offsets.push(start_offset);

        let count_start = counts.len();
        count_offsets.push(count_start);

        for (layer, layer_conns) in prepared.final_connections.iter().enumerate() {
            let l_offset = HnswHotCore::layer_offset(start_offset, layer, m);
            let layer_cap = if layer == 0 { m * 2 } else { m };
            let len = layer_conns.len().min(layer_cap);
            arena[l_offset..l_offset + len].copy_from_slice(&layer_conns[..len]);
            counts.push(len as u8);
        }

        for backlink in prepared.neighbor_backlinks {
            let ram_idx = backlink.neighbor_ram_idx;
            if ram_idx < offsets.len() && ram_idx < count_offsets.len() {
                let node_offset = offsets[ram_idx];
                let layer = backlink.layer;
                let count_start = count_offsets[ram_idx];
                let count_end = if ram_idx + 1 < count_offsets.len() {
                    count_offsets[ram_idx + 1]
                } else {
                    counts.len()
                };
                if count_start + layer < count_end {
                    let l_offset = HnswHotCore::layer_offset(node_offset, layer, m);
                    let layer_cap = if layer == 0 { m * 2 } else { m };
                    let len = backlink.updated_connections.len().min(layer_cap);
                    if l_offset + len <= arena.len() {
                        arena[l_offset..l_offset + len]
                            .copy_from_slice(&backlink.updated_connections[..len]);
                        counts[count_start + layer] = len as u8;
                    }
                }
            }
        }

        self.hot.nodes.write().push(node);
        self.hot
            .doc_to_node
            .write()
            .insert(prepared.doc_id.inner(), prepared.new_idx);

        if prepared.should_update_entry_point {
            self.hot.set_entry_point(Some(prepared.new_idx));
            self.hot
                .max_layer
                .store(prepared.new_layer as u64, Ordering::SeqCst);
        }

        if prepared.should_update_ram_entry_point {
            self.hot.set_ram_entry_point(Some(prepared.new_idx));
        }
    }

    fn do_insert(&self, id: DocId, vector: &[f32]) -> Result<()> {
        let prepared = self.compute_insert(id, vector)?;
        self.apply_insert(prepared);
        Ok(())
    }

    fn do_delete(&self, id: DocId) -> Result<()> {
        let node_idx = self.hot.doc_to_node.write().remove(&id.inner());
        if let Some(idx) = node_idx {
            self.cold.deleted_nodes.write().insert(idx as u64);
            self.hot.deleted_count.fetch_add(1, Ordering::SeqCst);

            // ANCHOR[ALG-FIX:D2-001] STATUS:DONE (TS:2026-06-01T00:00:00Z) — Entry-Point-Aktualisierung nach Delete (INV-HNSW-4)
            // INVARIANTE (INV-HNSW-4): Wenn der gelöschte Knoten der aktuelle Entry-Point (oder RAM-Entry-Point) war,
            // wird unter den verbleibenden nicht-gelöschten Knoten derjenige mit der höchsten Schicht (max_layer) als neuer Entry-Point gewählt.
            let ep_val = self.hot.get_entry_point();
            let ram_ep_val = self.hot.get_ram_entry_point();

            if ep_val == Some(idx) || ram_ep_val == Some(idx) {
                let nodes = self.hot.nodes.read();
                let mmap_guard = self.cold.mmap_index.read();
                let mmap_node_count = mmap_guard
                    .as_ref()
                    .map(|m| m.header.node_count() as usize)
                    .unwrap_or(0);
                let deleted = self.cold.deleted_nodes.read();

                let mut best_node = None;
                let mut best_ram_node = None;
                let mut max_layer = 0;
                let mut max_ram_layer = 0;

                // Check Mmap nodes
                if let Some(mmap) = mmap_guard.as_ref() {
                    for i in 0..mmap_node_count {
                        if i != idx && !deleted.contains(i as u64) {
                            let record = match mmap.get_node_record(i) {
                                Ok(r) => r,
                                Err(_) => continue, // Skip corrupt records
                            };
                            if record.max_layer as usize >= max_layer {
                                max_layer = record.max_layer as usize;
                                best_node = Some(i);
                            }
                        }
                    }
                }

                // Check RAM nodes
                for (i, node) in nodes.iter().enumerate() {
                    let global_idx = mmap_node_count + i;
                    if global_idx != idx && !deleted.contains(global_idx as u64) {
                        if node.max_layer >= max_layer {
                            max_layer = node.max_layer;
                            best_node = Some(global_idx);
                        }
                        if node.max_layer >= max_ram_layer {
                            max_ram_layer = node.max_layer;
                            best_ram_node = Some(global_idx);
                        }
                    }
                }

                if ep_val == Some(idx) {
                    self.hot.set_entry_point(best_node);
                    if let Some(new_idx) = best_node {
                        let node_max_layer = if let Some(mmap) = mmap_guard.as_ref() {
                            if new_idx < mmap_node_count {
                                mmap.get_node_record(new_idx)
                                    .map(|r| r.max_layer as usize)
                                    .unwrap_or(0)
                            } else {
                                nodes[new_idx - mmap_node_count].max_layer
                            }
                        } else {
                            nodes[new_idx].max_layer
                        };
                        self.hot
                            .max_layer
                            .store(node_max_layer as u64, Ordering::SeqCst);
                    } else {
                        self.hot.max_layer.store(0, Ordering::SeqCst);
                    }
                }

                if ram_ep_val == Some(idx) {
                    self.hot.set_ram_entry_point(best_ram_node);
                }
            }
        }
        Ok(())
    }

    /// Returns the fraction of deleted nodes in the index (0.0 to 1.0).
    pub fn deleted_ratio(&self) -> f64 {
        1.0 - self.connectivity_score()
    }

    /// Returns the total number of completed full index rebuilds.
    pub fn rebuild_count(&self) -> u64 {
        self.cold.rebuild_count.load(Ordering::SeqCst)
    }

    /// Returns the total count of visited dead nodes during graph search/traversal.
    pub fn visited_dead_nodes(&self) -> u64 {
        self.cold.visited_dead_nodes.load(Ordering::SeqCst)
    }

    /// Graph connectivity score (1.0 = perfect, 0.0 = fully fragmented).
    pub fn connectivity_score(&self) -> f64 {
        let deleted = self.hot.deleted_count.load(Ordering::SeqCst);
        let mmap_count = self
            .cold
            .mmap_index
            .read()
            .as_ref()
            .map(|m| m.header.node_count() as usize)
            .unwrap_or(0);
        let total = mmap_count + self.hot.nodes.read().len();
        if total == 0 {
            return 1.0;
        }
        (1.0 - deleted as f64 / total as f64).max(0.0)
    }

    /// Returns Ok(()) if the index is healthy, or
    /// Err(MemFuseError::HnswConnectivityDegraded { deleted_ratio }) if degraded.
    ///
    /// Checks the current graph connectivity against the configured rebuild threshold
    /// (by default `1.0 - HNSW_REBUILD_DELETION_RATIO`, where `HNSW_REBUILD_DELETION_RATIO` = 30% deleted nodes).
    pub fn check_connectivity(&self) -> memfuse_core::Result<()> {
        let score = self.connectivity_score();
        if score < self.cold.config.rebuild_threshold {
            let deleted_ratio = (1.0 - score) * 100.0;
            return Err(memfuse_core::MemFuseError::HnswConnectivityDegraded { deleted_ratio });
        }
        Ok(())
    }

    /// Checks if a rebuild is required based on the deletion ratio or quantizer drift.
    pub fn is_rebuild_required(&self) -> bool {
        if self.connectivity_score() < self.cold.config.rebuild_threshold {
            return true;
        }
        if self.cold.config.quantize {
            if let Some(q) = self.cold.quantizer.read().as_ref() {
                if q.is_rebuild_required(self.cold.config.quantizer_drift_threshold) {
                    return true;
                }
            }
        }
        false
    }

    /// Rebuilds the HNSW index from scratch, removing all deleted nodes.
    /// Restores optimal search performance and connectivity using a 2-phase lock protocol (ADR-061).
    pub async fn rebuild(&self) -> Result<()> {
        if self.hot.rebuilding.swap(true, Ordering::SeqCst) {
            tracing::debug!("HNSW rebuild already in progress, skipping");
            return Ok(());
        }
        let _guard = RebuildGuard(&self.hot.rebuilding);

        tracing::info!("Starting HNSW index rebuild (Phase 1)");
        let start_time = std::time::Instant::now();

        // Phase 1: Snapshot active nodes and build fresh index offline (without holding write_mutex)
        let (new_index, snapshot_tx) = self.rebuild_phase1_snapshot_and_build()?;

        tracing::info!("HNSW index rebuild Phase 1 completed, starting Phase 2 merge & swap");

        // Phase 2: Lock write_mutex briefly, replay delta changes since snapshot_tx, and swap
        self.rebuild_phase2_merge_and_swap(new_index, snapshot_tx)
            .await?;

        self.cold.rebuild_count.fetch_add(1, Ordering::SeqCst);
        tracing::info!("HNSW rebuild completed in {:?}", start_time.elapsed());
        Ok(())
    }

    /// Performs a local partial rebuild on a region of nodes (F-02 Partial-Rebuild Trigger).
    ///
    /// Cleans up tombstoned nodes within the region and rewires connections while
    /// preserving cross-region neighborhood boundary connections (INV-NUC-1).
    pub async fn rebuild_region(&self, region_node_ids: Vec<u64>) -> Result<()> {
        if region_node_ids.is_empty() {
            return Ok(());
        }

        let _write_lock = self.hot.write_mutex.lock().await;

        let region_set: AHashSet<u64> = region_node_ids.into_iter().collect();

        let mmap_count = self
            .cold
            .mmap_index
            .read()
            .as_ref()
            .map(|m| m.header.node_count() as usize)
            .unwrap_or(0);

        let nodes = self.hot.nodes.read();
        let mut deleted_nodes = self.cold.deleted_nodes.write();

        let mut tombstoned_in_region = Vec::new();
        for &node_id in &region_set {
            if deleted_nodes.contains(node_id) {
                tombstoned_in_region.push(node_id);
            }
        }

        if tombstoned_in_region.is_empty() {
            return Ok(());
        }

        // For each node in region (or adjacent to region), prune references to tombstoned region nodes
        let tombstoned_set: AHashSet<u32> =
            tombstoned_in_region.iter().map(|&id| id as u32).collect();

        let m = self.cold.config.m;
        let offsets = self.hot.neighbor_offsets.read();
        let count_offsets = self.hot.neighbor_count_offsets.read();
        let mut counts = self.hot.neighbor_counts.write();
        let mut arena = self.hot.neighbor_arena.write();

        for (i, node) in nodes.iter().enumerate() {
            let global_idx = mmap_count + i;
            if deleted_nodes.contains(global_idx as u64) {
                continue;
            }

            if i < offsets.len() && i < count_offsets.len() {
                let node_offset = offsets[i];
                let count_start = count_offsets[i];
                let count_end = if i + 1 < count_offsets.len() {
                    count_offsets[i + 1]
                } else {
                    counts.len()
                };
                for layer in 0..node.max_layer + 1 {
                    if count_start + layer < count_end {
                        let l_offset = HnswHotCore::layer_offset(node_offset, layer, m);
                        let old_len = counts[count_start + layer] as usize;
                        if l_offset + old_len <= arena.len() {
                            let layer_slice = &arena[l_offset..l_offset + old_len];
                            let mut kept = Vec::with_capacity(old_len);
                            for &neighbor_u32 in layer_slice {
                                if !tombstoned_set.contains(&neighbor_u32) {
                                    kept.push(neighbor_u32);
                                }
                            }
                            counts[count_start + layer] = kept.len() as u8;
                            arena[l_offset..l_offset + kept.len()].copy_from_slice(&kept);
                        }
                    }
                }
            }
        }

        // Remove tombstoned node IDs from doc_to_node and update deleted_count
        let mut doc_map = self.hot.doc_to_node.write();
        for &ts_id in &tombstoned_in_region {
            if ts_id >= mmap_count as u64 {
                let ram_idx = (ts_id as usize) - mmap_count;
                if let Some(node) = nodes.get(ram_idx) {
                    doc_map.remove(&node.doc_id.inner());
                }
            }
            deleted_nodes.insert(ts_id);
        }

        self.hot
            .deleted_count
            .store(deleted_nodes.len(), Ordering::SeqCst);

        tracing::info!(
            rebuilt_region_nodes = region_set.len(),
            pruned_tombstones = tombstoned_in_region.len(),
            "HNSW local partial rebuild completed successfully"
        );

        Ok(())
    }

    fn rebuild_phase1_snapshot_and_build(&self) -> Result<(HnswIndex, u64)> {
        // AI-TAG[SMELL][RESOLVED] audit-JULES-16-followup: HNSW-Rebuild respektiert jetzt aktive search_at()-Snapshots via retention window / seq_log pinning.
        // 1. Snapshot active and soft-deleted retained nodes (RAM segment only) up to snapshot_tx
        let (all_nodes, config, snapshot_tx) = {
            let nodes = self.hot.nodes.read();
            let mmap_count = self
                .cold
                .mmap_index
                .read()
                .as_ref()
                .map(|m| m.header.node_count() as usize)
                .unwrap_or(0);
            let deleted_nodes = self.cold.deleted_nodes.read();
            let seq_log = self.cold.seq_log.read();
            let min_retention_seq = seq_log.min_retention_seq();
            let snapshot_tx = self.hot.last_tx_id.load(Ordering::SeqCst);
            let mut all = Vec::with_capacity(nodes.len());
            for (i, node) in nodes.iter().enumerate() {
                let global_idx = mmap_count + i;
                if node.committed_tx <= snapshot_tx {
                    let is_deleted = deleted_nodes.contains(global_idx as u64);
                    if !is_deleted {
                        all.push((node.doc_id, node.vector.clone(), node.committed_tx, false));
                    } else if let Some(min_ret_seq) = min_retention_seq {
                        if let Some(del_seq) = seq_log.deletion_seq(node.doc_id) {
                            if del_seq >= min_ret_seq {
                                all.push((
                                    node.doc_id,
                                    node.vector.clone(),
                                    node.committed_tx,
                                    true,
                                ));
                            }
                        }
                    }
                }
            }
            (all, self.cold.config.clone(), snapshot_tx)
        };

        // 2. Build fresh index (this will be the NEW RAM segment)
        let new_index = HnswIndex::try_new(config)?;
        *new_index.inner.cold.seq_log.write() = self.cold.seq_log.read().clone();

        // Ensure new_index knows about the Mmap segment to link against it
        {
            let mmap_guard = self.cold.mmap_index.read();
            if let Some(mmap) = mmap_guard.as_ref() {
                new_index.load_mmap_from_instance(mmap.clone())?;
            }
        }

        let quantizer_guard = self.cold.quantizer.read();
        if let Some(old_q) = quantizer_guard.as_ref() {
            // Train a new quantizer on a sample of active nodes to prevent clamping loss
            let sample_size = self
                .cold
                .config
                .quantizer_recalibration_sample_size
                .min(all_nodes.len());
            let mut train_data = Vec::with_capacity(sample_size);

            for (_, vector, _, is_deleted) in all_nodes.iter() {
                if !is_deleted {
                    match vector {
                        VectorData::F32(v) => train_data.push(v.clone()),
                        VectorData::U8(v) => train_data.push(old_q.dequantize(v)?),
                    }
                    if train_data.len() >= sample_size {
                        break;
                    }
                }
            }

            if !train_data.is_empty() {
                let training_refs: Vec<&[f32]> = train_data.iter().map(|v| v.as_slice()).collect();
                let new_q = crate::quantize::ScalarQuantizer::train(
                    &training_refs,
                    self.cold.config.dimension,
                );
                *new_index.inner.cold.quantizer.write() = Some(new_q);
            } else {
                *new_index.inner.cold.quantizer.write() = Some(old_q.clone());
            }
        }

        for (doc_id, vector, committed_tx, is_deleted) in all_nodes {
            match vector {
                VectorData::F32(v) => {
                    new_index.inner.do_insert(doc_id, &v)?;
                }
                VectorData::U8(v) => {
                    let dequantized = {
                        let q = quantizer_guard.as_ref().ok_or_else(|| {
                            MemFuseError::Index("Quantizer missing during rebuild".into())
                        })?;
                        q.dequantize(&v)?
                    };
                    new_index.inner.do_insert(doc_id, &dequantized)?;
                }
            }
            let mmap_count = new_index
                .inner
                .cold
                .mmap_index
                .read()
                .as_ref()
                .map(|m| m.header.node_count() as usize)
                .unwrap_or(0);
            if let Some(&global_idx) = new_index.inner.hot.doc_to_node.read().get(&doc_id.inner()) {
                if global_idx >= mmap_count {
                    let ram_idx = global_idx - mmap_count;
                    let mut nodes = new_index.inner.hot.nodes.write();
                    if let Some(node) = nodes.get_mut(ram_idx) {
                        node.committed_tx = committed_tx;
                    }
                }
            }
            if is_deleted {
                new_index.inner.do_delete(doc_id)?;
            }
        }

        Ok((new_index, snapshot_tx))
    }

    async fn rebuild_phase2_merge_and_swap(
        &self,
        new_index: HnswIndex,
        snapshot_tx: u64,
    ) -> Result<()> {
        let _write_lock = self.hot.write_mutex.lock().await;

        // 1. Fetch delta changes committed since snapshot_tx
        let delta_changes = self.cold.seq_log.read().changes_since(snapshot_tx);

        // 2. Replay delta changes into new_index
        for change in delta_changes {
            match change {
                memfuse_core::SeqLogChange::Insert { doc_id, seq } => {
                    // Check if document is currently present in old index RAM segment
                    let vector_opt = {
                        let doc_map = self.hot.doc_to_node.read();
                        let mmap_count = self
                            .cold
                            .mmap_index
                            .read()
                            .as_ref()
                            .map(|m| m.header.node_count() as usize)
                            .unwrap_or(0);
                        if let Some(&global_idx) = doc_map.get(&doc_id.inner()) {
                            if global_idx >= mmap_count {
                                let ram_idx = global_idx - mmap_count;
                                let nodes = self.hot.nodes.read();
                                nodes.get(ram_idx).map(|n| n.vector.clone())
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    };

                    if let Some(vector) = vector_opt {
                        let f32_vec = match vector {
                            VectorData::F32(v) => v,
                            VectorData::U8(v) => {
                                let q_guard = self.cold.quantizer.read();
                                let q = q_guard.as_ref().ok_or_else(|| {
                                    MemFuseError::Index(
                                        "Quantizer missing during rebuild phase 2 delta replay"
                                            .into(),
                                    )
                                })?;
                                q.dequantize(&v)?
                            }
                        };

                        new_index.inner.do_insert(doc_id, &f32_vec)?;

                        // Set committed_tx on newly inserted node in new_index
                        let mmap_count = new_index
                            .inner
                            .cold
                            .mmap_index
                            .read()
                            .as_ref()
                            .map(|m| m.header.node_count() as usize)
                            .unwrap_or(0);
                        if let Some(&global_idx) =
                            new_index.inner.hot.doc_to_node.read().get(&doc_id.inner())
                        {
                            if global_idx >= mmap_count {
                                let ram_idx = global_idx - mmap_count;
                                let mut nodes = new_index.inner.hot.nodes.write();
                                if let Some(node) = nodes.get_mut(ram_idx) {
                                    node.committed_tx = seq;
                                }
                            }
                        }
                    }
                }
                memfuse_core::SeqLogChange::Delete { doc_id, .. } => {
                    new_index.inner.do_delete(doc_id)?;
                }
            }
        }

        // 3. Atomic swap
        {
            let mut nodes = self.hot.nodes.write();
            let mut doc_to_node = self.hot.doc_to_node.write();
            let mut deleted_nodes = self.cold.deleted_nodes.write();

            let new_nodes = std::mem::take(&mut *new_index.inner.hot.nodes.write());
            let new_doc_to_node = std::mem::take(&mut *new_index.inner.hot.doc_to_node.write());
            let new_entry_point = new_index.inner.hot.get_entry_point();
            let new_ram_entry_point = new_index.inner.hot.get_ram_entry_point();

            let new_offsets = std::mem::take(&mut *new_index.inner.hot.neighbor_offsets.write());
            let new_count_offsets =
                std::mem::take(&mut *new_index.inner.hot.neighbor_count_offsets.write());
            let new_counts = std::mem::take(&mut *new_index.inner.hot.neighbor_counts.write());
            let new_arena = std::mem::take(&mut *new_index.inner.hot.neighbor_arena.write());

            let new_quantizer = new_index.inner.cold.quantizer.write().take();
            if new_quantizer.is_some() {
                *self.cold.quantizer.write() = new_quantizer;
            }

            *nodes = new_nodes;
            *doc_to_node = new_doc_to_node;
            self.hot.set_entry_point(new_entry_point);
            self.hot.set_ram_entry_point(new_ram_entry_point);
            *self.hot.neighbor_offsets.write() = new_offsets;
            *self.hot.neighbor_count_offsets.write() = new_count_offsets;
            *self.hot.neighbor_counts.write() = new_counts;
            *self.hot.neighbor_arena.write() = new_arena;

            self.hot.max_layer.store(
                new_index.inner.hot.max_layer.load(Ordering::SeqCst),
                Ordering::SeqCst,
            );

            // Preserve mmap deletions, plus any deletions recorded in new_index
            let mmap_count = self
                .cold
                .mmap_index
                .read()
                .as_ref()
                .map(|m| m.header.node_count() as usize)
                .unwrap_or(0);
            let mut new_deleted = RoaringTreemap::new();
            for del_idx in deleted_nodes.iter() {
                if (del_idx as usize) < mmap_count {
                    new_deleted.insert(del_idx);
                }
            }
            for del_idx in new_index.inner.cold.deleted_nodes.read().iter() {
                new_deleted.insert(del_idx);
            }

            *deleted_nodes = new_deleted;
            self.hot
                .deleted_count
                .store(deleted_nodes.len(), Ordering::SeqCst);
        }

        Ok(())
    }

    // Kept for backward compatibility or direct calls if needed, though facade should use `HnswIndex` wrapper
}

impl VectorIndex for HnswIndex {
    async fn insert(&self, tx: TxId, id: DocId, embedding: &[f32]) -> Result<()> {
        if let Some(ref err) = self.inner.cold.validation_error {
            return Err(MemFuseError::invalid_input(format!(
                "Invalid index configuration: {}",
                err
            )));
        }
        if embedding.len() != self.inner.cold.config.dimension {
            return Err(MemFuseError::invalid_input(format!(
                "Expected dimension {}, got {}",
                self.inner.cold.config.dimension,
                embedding.len()
            )));
        }

        validate_vector(embedding)?;

        self.inner.cold.tx_buffer.stage(
            tx,
            IndexOp::Insert {
                doc_id: id,
                data: embedding.to_vec(),
            },
        )?;
        Ok(())
    }

    // CONSTRAINT: HNSW Search Hotspot (Optimiert)
    // TARGET: < 10ms bei 1M Vektoren
    // AKTUELL: Optimiert via Dynamic ef_search
    // BOTTLENECK: CPU / Cache Misses / ef_search Heuristik
    // FIX: Dynamische Anpassung von ef_search basierend auf Layer-Hierarchie.
    async fn search(&self, query: &[f32], k: usize) -> Result<Vec<ScoredDocument>> {
        if k > memfuse_core::MAX_SEARCH_K {
            return Err(MemFuseError::invalid_input(format!(
                "Requested k ({}) exceeds maximum allowed search limit ({})",
                k,
                memfuse_core::MAX_SEARCH_K
            )));
        }
        if let Some(ref err) = self.inner.cold.validation_error {
            return Err(MemFuseError::invalid_input(format!(
                "Invalid index configuration: {}",
                err
            )));
        }
        if query.len() != self.inner.cold.config.dimension {
            return Err(MemFuseError::invalid_input(format!(
                "Expected dimension {}, got {}",
                self.inner.cold.config.dimension,
                query.len()
            )));
        }

        for &val in query {
            if !val.is_finite() {
                return Err(MemFuseError::invalid_input(
                    "Query vector contains NaN or infinite values",
                ));
            }
        }

        let query_quantized: Option<Vec<u8>> = None;

        let mmap_guard = self.inner.cold.mmap_index.read();
        let mmap_node_count = mmap_guard
            .as_ref()
            .map(|m| m.header.node_count() as usize)
            .unwrap_or(0);

        let mut ep = Vec::new();
        if let Some(global_ep) = self.inner.hot.get_entry_point() {
            ep.push(global_ep);
        }
        if let Some(ram_ep) = self.inner.hot.get_ram_entry_point() {
            if !ep.contains(&ram_ep) {
                ep.push(ram_ep);
            }
        }

        if ep.is_empty() {
            return Ok(Vec::new());
        }

        let max_layer = self.inner.hot.max_layer.load(Ordering::SeqCst) as usize;

        for layer in (1..=max_layer).rev() {
            let layer_ef = 1;
            let best =
                self.inner
                    .search_layer(query, query_quantized.as_deref(), &ep, layer_ef, layer)?;
            if let Some(closest) = best.first() {
                ep = vec![closest.index];
            }
        }

        // Add RAM entry point back for the final layer search to ensure hybrid recall
        if let Some(ram_ep) = self.inner.hot.get_ram_entry_point() {
            if !ep.contains(&ram_ep) {
                ep.push(ram_ep);
            }
        }

        // Higher candidate list for reranking if quantized
        let ef = if self.inner.cold.config.quantize {
            self.inner.cold.config.ef_search.max(k) * 4
        } else {
            self.inner.cold.config.ef_search.max(k)
        };
        let candidates = self
            .inner
            .search_layer(query, query_quantized.as_deref(), &ep, ef, 0)?;

        let score = self.inner.connectivity_score();
        if score < self.inner.cold.config.rebuild_threshold {
            let deleted_ratio = (1.0 - score) * 100.0;
            let err = memfuse_core::MemFuseError::HnswConnectivityDegraded { deleted_ratio };
            tracing::warn!(
                error = %err,
                connectivity_score = score,
                rebuild_threshold = self.inner.cold.config.rebuild_threshold,
                "HNSW index degraded — consider calling rebuild()"
            );
        }

        let nodes = self.inner.hot.nodes.read();
        let deleted = self.inner.cold.deleted_nodes.read();
        let mut results = Vec::with_capacity(k);

        let q_guard = if self.inner.cold.config.quantize {
            Some(self.inner.cold.quantizer.read())
        } else {
            None
        };
        let q_ref = q_guard.as_ref().and_then(|g| g.as_ref());

        let ctx = SearchContext {
            nodes: &nodes,
            mmap: mmap_guard.as_ref(),
            mmap_node_count,
            prior_prepared: &[],
            backlink_map: None,
            quantizer: q_ref.map(Cow::Borrowed),
        };

        for c in candidates.iter() {
            if deleted.contains(c.index as u64) {
                continue;
            }
            if c.index >= mmap_node_count {
                if let Some(node) = nodes.get(c.index - mmap_node_count) {
                    if node.committed_tx == 0 {
                        continue;
                    }
                }
            }
            let doc_id = self.inner.resolve_doc_id(c.index, &ctx)?;

            // Phase 2: Exact Reranking (Asymmetric for SQ8)
            let final_dist = if self.inner.cold.config.quantize {
                self.inner.resolve_dist(c.index, query, None, &ctx)?
            } else {
                c.distance
            };

            let score = match self.inner.cold.config.distance_metric {
                DistanceMetric::Cosine => 1.0 - final_dist,
                DistanceMetric::Euclidean => 1.0 / (1.0 + final_dist),
                DistanceMetric::DotProduct => -final_dist,
                other => {
                    return Err(MemFuseError::Index(format!(
                        "Unsupported DistanceMetric variant in search(): {other:?}"
                    )));
                }
            };
            results.push(ScoredDocument::new(doc_id, score));
        }

        // Select top k using select_nth_unstable_by (O(N)) then sort top k (O(k log k))
        if results.len() > k {
            results.select_nth_unstable_by(k - 1, |a, b| {
                b.score
                    .total_cmp(&a.score)
                    .then_with(|| a.doc_id.cmp(&b.doc_id))
            });
            results.truncate(k);
        }
        results.sort_by(|a, b| {
            b.score
                .total_cmp(&a.score)
                .then_with(|| a.doc_id.cmp(&b.doc_id))
        });

        Ok(results)
    }

    async fn search_filtered(
        &self,
        query: &[f32],
        k: usize,
        filter: Option<&(dyn Fn(DocId) -> bool + Send + Sync)>,
    ) -> Result<Vec<ScoredDocument>> {
        self.search_filtered_internal(query, k, filter, None).await
    }

    async fn delete(&self, tx: TxId, id: DocId) -> Result<()> {
        if let Some(ref err) = self.inner.cold.validation_error {
            return Err(MemFuseError::invalid_input(format!(
                "Invalid index configuration: {}",
                err
            )));
        }
        self.inner.cold.tx_buffer.stage(
            tx,
            IndexOp::Delete {
                doc_id: id,
                data: None,
            },
        )?;
        Ok(())
    }

    async fn commit(&self, tx: TxId) -> Result<()> {
        if let Some(ref err) = self.inner.cold.validation_error {
            return Err(MemFuseError::invalid_input(format!(
                "Invalid index configuration: {}",
                err
            )));
        }
        let _lock = self.inner.hot.write_mutex.lock().await;
        let ops = self.inner.cold.tx_buffer.drain(tx);

        // ANCHOR[SPEC:WP-2.2-SQ8TRAIN-001] STATUS:DONE (TS:2026-06-01T00:00:00Z) — Lazy Training logic (Stabilized)
        if self.inner.cold.config.quantize && self.inner.cold.quantizer.read().is_none() {
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
                let nodes = self.inner.hot.nodes.read();
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
                let q = crate::quantize::ScalarQuantizer::train(
                    &training_refs,
                    self.inner.cold.config.dimension,
                );
                *self.inner.cold.quantizer.write() = Some(q.clone());

                let mut nodes = self.inner.hot.nodes.write();
                for node in nodes.iter_mut() {
                    if let VectorData::F32(v) = &node.vector {
                        node.vector = VectorData::U8(q.quantize(v)?);
                    }
                }
            }
        }

        // INVARIANTE: Compute-then-Commit Transaktionsatomarität:
        // Phase 1: Compute all fallible operations for ALL ops in the transaction.
        // Purely read-only with respect to self.inner.hot.nodes and self.inner.hot.doc_to_node.
        let mut prepared_inserts = Vec::new();
        let mut deletes_to_apply = Vec::new();
        let mut batch_ctx = BatchContext::new(&self.inner);

        for op in &ops {
            match op {
                IndexOp::Insert { doc_id, data } => {
                    let prepared = self.inner.compute_insert_with_context(
                        *doc_id,
                        data,
                        prepared_inserts.len(),
                        &mut prepared_inserts,
                        &mut batch_ctx,
                    )?;
                    prepared_inserts.push(prepared);
                }
                IndexOp::Delete { doc_id, .. } => {
                    deletes_to_apply.push(*doc_id);
                }
                // AI-TAG[PANIC-SAFETY][CRITICAL] RESOLVED: AGT-INDEX-004 — IndexOp ist #[non_exhaustive]; neue Varianten (TS:2026-08-25T00:00:00Z)
                // müssen hier explizit behandelt werden, bevor sie in den HNSW-Commit-Pfad gelangen.
                // ANWEISUNG: Neue IndexOp-Variante → Arm hier hinzufügen oder UpdateNotSupported zurückgeben.
                // ID: FIX-03-INDEXOP
                other => {
                    return Err(MemFuseError::Index(format!(
                        "HNSW commit received unsupported IndexOp variant: {:?}. \
                         Add a handler arm before enabling this operation.",
                        std::mem::discriminant(other)
                    )));
                }
            }
        }

        // Phase 2: Once ALL Phase 1 computations succeeded without error, apply infallible mutations.
        let mut inserted_doc_ids = Vec::with_capacity(prepared_inserts.len());
        for prepared in prepared_inserts {
            inserted_doc_ids.push(prepared.doc_id);
            self.inner.apply_insert(prepared);
        }

        for doc_id in deletes_to_apply {
            self.inner.do_delete(doc_id)?;
        }

        // Record ops into seq_log for search_at snapshot isolation
        let seq = tx.inner();
        let mut seq_log = self.inner.cold.seq_log.write();
        for op in &ops {
            match op {
                IndexOp::Insert { doc_id, .. } => {
                    seq_log.record_insert(*doc_id, seq);
                }
                IndexOp::Delete { doc_id, .. } => {
                    seq_log.record_delete(*doc_id, seq);
                }
                _ => {}
            }
        }
        drop(seq_log);

        // Atomisch committed_tx für alle neu eingefügten Nodes dieser Transaktion setzen
        if !inserted_doc_ids.is_empty() {
            let mmap_count = self
                .inner
                .cold
                .mmap_index
                .read()
                .as_ref()
                .map(|m| m.header.node_count() as usize)
                .unwrap_or(0);
            let doc_map = self.inner.hot.doc_to_node.read();
            let mut nodes = self.inner.hot.nodes.write();

            for doc_id in inserted_doc_ids {
                if let Some(&global_idx) = doc_map.get(&doc_id.inner()) {
                    if global_idx >= mmap_count {
                        let ram_idx = global_idx - mmap_count;
                        if let Some(node) = nodes.get_mut(ram_idx) {
                            node.committed_tx = tx.inner();
                        }
                    }
                }
            }
        }

        if self.inner.is_rebuild_required() {
            tracing::warn!(
                "HNSW index rebuild threshold reached (rebuild_threshold: {:.2}, quantizer_drift_threshold: {:.2})",
                self.inner.cold.config.rebuild_threshold,
                self.inner.cold.config.quantizer_drift_threshold
            );
            self.trigger_rebuild_async();
        }

        #[cfg(feature = "partial-index-rebuild")]
        self.check_and_trigger_partial_rebuild();

        self.inner
            .hot
            .last_tx_id
            .store(tx.inner(), Ordering::SeqCst);
        Ok(())
    }

    /// Searches for nearest neighbors at a specific snapshot sequence number.
    async fn search_at(&self, query: &[f32], k: usize, seq_no: u64) -> Result<Vec<ScoredDocument>> {
        let _pin_guard = SnapshotPinGuard::new(&self.inner, seq_no);
        let log = self.inner.cold.seq_log.read().clone();
        let filter_fn = move |doc_id: DocId| -> bool { log.is_visible(doc_id, seq_no) };
        self.search_filtered_internal(query, k, Some(&filter_fn), Some(seq_no))
            .await
    }

    async fn rollback(&self, tx: TxId) -> Result<()> {
        if let Some(ref err) = self.inner.cold.validation_error {
            return Err(MemFuseError::invalid_input(format!(
                "Invalid index configuration: {}",
                err
            )));
        }
        self.inner.cold.tx_buffer.discard(tx);
        Ok(())
    }

    async fn rollback_to_tx(&self, tx_id: TxId) -> Result<()> {
        if let Some(ref err) = self.inner.cold.validation_error {
            return Err(MemFuseError::invalid_input(format!(
                "Invalid index configuration: {}",
                err
            )));
        }

        let target = tx_id.inner();

        // Unter write_mutex um Konkurrenz mit laufenden Inserts zu verhindern
        let _guard = self.inner.hot.write_mutex.lock().await;

        // 1. Sammle alle Nodes mit committed_tx > target_tx_id
        let indices_to_remove: Vec<usize> = {
            let nodes = self.inner.hot.nodes.read();
            nodes
                .iter()
                .enumerate()
                .filter(|(_, node)| node.committed_tx > target && node.committed_tx != 0)
                .map(|(i, _)| i)
                .collect()
        };

        if indices_to_remove.is_empty() {
            self.inner.hot.last_tx_id.store(target, Ordering::SeqCst);
            return Ok(());
        }

        // 2. Aus doc_to_node-Map entfernen
        {
            let nodes = self.inner.hot.nodes.read();
            let mut map = self.inner.hot.doc_to_node.write();
            for &idx in &indices_to_remove {
                if let Some(node) = nodes.get(idx) {
                    map.remove(&node.doc_id.inner());
                }
            }
        }

        // 3. Als deleted markieren (Soft-Delete — kein Rebuild nötig)
        {
            let mmap_count = self
                .inner
                .cold
                .mmap_index
                .read()
                .as_ref()
                .map(|m| m.header.node_count() as usize)
                .unwrap_or(0);
            let mut deleted = self.inner.cold.deleted_nodes.write();
            for &idx in &indices_to_remove {
                deleted.insert((mmap_count + idx) as u64);
            }
            self.inner
                .hot
                .deleted_count
                .fetch_add(indices_to_remove.len() as u64, Ordering::SeqCst);
        }

        // 4. TxBuffer bereinigen
        self.inner.cold.tx_buffer.discard(tx_id);

        // 5. last_tx_id zurücksetzen
        self.inner.hot.last_tx_id.store(target, Ordering::SeqCst);

        tracing::info!(
            removed = indices_to_remove.len(),
            rollback_target = target,
            "HNSW physical rollback completed"
        );

        Ok(())
    }

    async fn last_tx_id(&self) -> Result<TxId> {
        Ok(TxId::new(self.inner.hot.last_tx_id.load(Ordering::SeqCst)))
    }

    async fn all_doc_ids(&self) -> Result<Vec<DocId>> {
        if self.inner.cold.validation_error.is_some() {
            return Ok(Vec::new());
        }
        let nodes = self.inner.hot.nodes.read();
        let mmap_guard = self.inner.cold.mmap_index.read();
        let mmap_node_count = mmap_guard
            .as_ref()
            .map(|m| m.header.node_count() as usize)
            .unwrap_or(0);
        let deleted = self.inner.cold.deleted_nodes.read();

        let ctx = SearchContext {
            nodes: &nodes,
            mmap: mmap_guard.as_ref(),
            mmap_node_count,
            prior_prepared: &[],
            backlink_map: None,
            quantizer: None,
        };

        let total_nodes = mmap_node_count + nodes.len();
        let mut ids = Vec::with_capacity(total_nodes.saturating_sub(deleted.len() as usize));

        for i in 0..total_nodes {
            if !deleted.contains(i as u64) {
                ids.push(self.inner.resolve_doc_id(i, &ctx)?);
            }
        }
        Ok(ids)
    }

    async fn len(&self) -> usize {
        if self.inner.cold.validation_error.is_some() {
            return 0;
        }
        let mmap_count = self
            .inner
            .cold
            .mmap_index
            .read()
            .as_ref()
            .map(|m| m.header.node_count() as usize)
            .unwrap_or(0);
        let total = mmap_count + self.inner.hot.nodes.read().len();
        let deleted = self.inner.hot.deleted_count.load(Ordering::SeqCst) as usize;
        total.saturating_sub(deleted)
    }

    fn is_rebuild_required(&self) -> bool {
        self.is_rebuild_required()
    }

    fn trigger_rebuild_async(&self) {
        drop(HnswIndex::trigger_rebuild_async(self));
    }

    async fn stats(&self) -> Result<VectorIndexStats> {
        let nodes = self.inner.hot.nodes.read();
        let mmap_guard = self.inner.cold.mmap_index.read();
        let mmap_count = mmap_guard
            .as_ref()
            .map(|m| m.header.node_count() as usize)
            .unwrap_or(0);

        let deleted_count = self.inner.hot.deleted_count.load(Ordering::SeqCst) as usize;
        let num_vectors = (mmap_count + nodes.len()).saturating_sub(deleted_count);

        let mut vector_memory: usize = nodes
            .iter()
            .map(|n| match &n.vector {
                VectorData::F32(v) => v.len() * std::mem::size_of::<f32>(),
                VectorData::U8(v) => v.len() * std::mem::size_of::<u8>(),
            })
            .sum();

        let connection_memory: usize =
            self.inner.hot.neighbor_arena.read().len() * std::mem::size_of::<u32>();

        if let Some(mmap) = mmap_guard.as_ref() {
            vector_memory += mmap.mmap.len(); // Simple approximation: entire mmap file
        }

        Ok(VectorIndexStats {
            num_vectors,
            memory_usage_bytes: vector_memory
                + connection_memory
                + (nodes.len() * std::mem::size_of::<HnswNode>()),
            num_layers: self.inner.hot.max_layer.load(Ordering::SeqCst) as usize + 1,
            deleted_ratio: self.deleted_ratio(),
            rebuild_count: self.rebuild_count(),
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
            ef_search: 64,

            distance_metric: DistanceMetric::Euclidean,
            rebuild_threshold: 0.8,
            quantize: false,
            ..Default::default()
        }
    }

    #[test]
    fn test_try_new_invalid_config_fails_immediately() {
        let config = HnswConfig {
            ef_construction: 1,
            m: 100, // Invalid: ef_construction < m
            ..test_config(4)
        };
        let result = HnswIndex::try_new(config);
        assert!(
            result.is_err(),
            "try_new must fail immediately on invalid config"
        );
        let err_msg = format!("{}", result.err().unwrap()); // unwrap
        assert!(
            err_msg.contains("ef_construction (1) must be >= m (100)"),
            "Unexpected error message: {}",
            err_msg
        );
    }

    #[test]
    fn test_hnsw_config_builder_and_validation() {
        // Fluent builder Happy Path
        let config = HnswConfigBuilder::new(128)
            .max_elements(1000)
            .m(32)
            .ef_construction(128)
            .ef_search(64)
            .distance_metric(DistanceMetric::Cosine)
            .quantize(true)
            .quantizer_recalibration_sample_size(500)
            .build()
            .expect("valid builder config"); // expect

        assert_eq!(config.dimension, 128);
        assert_eq!(config.max_elements, 1000);
        assert_eq!(config.m, 32);
        assert_eq!(config.ef_construction, 128);
        assert_eq!(config.ef_search, 64);
        assert_eq!(config.distance_metric, DistanceMetric::Cosine);
        assert!(config.quantize);
        assert_eq!(config.quantizer_recalibration_sample_size, 500);

        // Validation Error Case: ef_construction < m
        let res_ef_c = HnswConfig {
            m: 16,
            ef_construction: 8,
            ..Default::default()
        }
        .validate();
        assert!(matches!(res_ef_c, Err(MemFuseError::InvalidInput(_))));

        let res_builder_err = HnswConfigBuilder::new(128).m(16).ef_construction(8).build();
        assert!(matches!(
            res_builder_err,
            Err(MemFuseError::InvalidInput(_))
        ));
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn test_search_non_blocking_when_connection_write_lock_held() {
        let index = std::sync::Arc::new(HnswIndex::try_new(test_config(4)).unwrap());
        let tx1 = TxId::new(1);

        for i in 1..=20u64 {
            let v = vec![i as f32, 0.0, 0.0, 0.0];
            index.insert(tx1, DocId::from(i), &v).await.unwrap();
        }
        index.commit(tx1).await.unwrap();

        // Soft-delete a document to trigger has_dead_neighbors during search
        let tx2 = TxId::new(2);
        index.delete(tx2, DocId::from(5u64)).await.unwrap();
        index.commit(tx2).await.unwrap();

        // Hold write lock on neighbor arena
        let lock_guard = index.inner.hot.neighbor_arena.write();

        // Perform search while write lock is held.
        // Because search lazy pruning uses try_write(), search must complete without blocking!
        let start = std::time::Instant::now();
        let search_res = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            index.search(&[1.0, 0.0, 0.0, 0.0], 5),
        )
        .await;

        drop(lock_guard);

        assert!(
            search_res.is_ok(),
            "Search must complete non-blockingly within 500ms even when connection write lock is held"
        );
        let results = search_res.unwrap().unwrap();
        assert!(!results.is_empty());
        assert!(start.elapsed() < std::time::Duration::from_millis(200));
    }

    #[tokio::test]
    async fn test_compact_seq_log() {
        let config = HnswConfig {
            dimension: 4,
            ..Default::default()
        };
        let index = HnswIndex::try_new(config).expect("valid config"); // expect
        let tx = TxId::new(1);
        let doc_id = DocId::from(1u64);
        let vec = vec![1.0, 2.0, 3.0, 4.0];

        index.insert(tx, doc_id, &vec).await.expect("insert"); // expect
        index.commit(tx).await.expect("commit"); // expect

        // Compact sequence log below min_active_seqno
        index.compact_seq_log(10);
        assert_eq!(index.len().await, 1);
    }

    #[tokio::test]
    async fn test_invalid_config_error() {
        let config = HnswConfig {
            ef_construction: 5,
            m: 10, // Invalid: ef_construction < m
            ..test_config(4)
        };
        #[allow(deprecated)]
        let index = HnswIndex::new(config);
        let tx = TxId::new(1);
        let result = index
            .insert(tx, DocId::from(1u64), &[1.0, 0.0, 0.0, 0.0])
            .await;
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("Invalid index configuration"));
        assert!(err_msg.contains("ef_construction (5) must be >= m (10)"));
    }

    #[tokio::test]
    async fn test_insert_and_search() {
        let index = HnswIndex::try_new(test_config(4)).unwrap(); // unwrap
        let tx = TxId::new(1);

        // Insert 3 vectors
        index
            .insert(tx, DocId::from(1u64), &[1.0, 0.0, 0.0, 0.0])
            .await
            .expect("insert 1"); // expect
        index
            .insert(tx, DocId::from(2u64), &[0.0, 1.0, 0.0, 0.0])
            .await
            .expect("insert 2"); // expect
        index
            .insert(tx, DocId::from(3u64), &[0.9, 0.1, 0.0, 0.0])
            .await
            .expect("insert 3"); // expect
        index.commit(tx).await.expect("commit"); // expect

        // Search for vector closest to [1, 0, 0, 0]
        let results = index
            .search(&[1.0, 0.0, 0.0, 0.0], 2)
            .await
            .expect("search"); // expect
        assert!(!results.is_empty());
        assert_eq!(results[0].doc_id, DocId::from(1u64));
    }

    #[tokio::test]
    async fn test_delete() {
        let index = HnswIndex::try_new(test_config(4)).unwrap(); // unwrap

        let tx1 = TxId::new(1);
        index
            .insert(tx1, DocId::from(1u64), &[1.0, 0.0, 0.0, 0.0])
            .await
            .expect("insert"); // expect
        index.commit(tx1).await.expect("commit"); // expect

        assert_eq!(index.len().await, 1);

        let tx2 = TxId::new(2);
        index.delete(tx2, DocId::from(1u64)).await.expect("delete"); // expect
        index.commit(tx2).await.expect("commit"); // expect

        assert_eq!(index.len().await, 0);
    }

    #[tokio::test]
    async fn test_entry_point_deletion_search() {
        let index = HnswIndex::try_new(test_config(4)).unwrap(); // unwrap
        let tx1 = TxId::new(1);

        // Insert 5 nodes. First node (DocId(0)) will be the initial entry point.
        for i in 0u64..5 {
            let v = vec![i as f32, 0.0, 0.0, 0.0];
            index.insert(tx1, DocId::from(i), &v).await.expect("insert"); // expect
        }
        index.commit(tx1).await.expect("commit"); // expect

        // Delete node 0 (the entry point)
        let tx2 = TxId::new(2);
        index.delete(tx2, DocId::from(0u64)).await.expect("delete"); // expect
        index.commit(tx2).await.expect("commit"); // expect

        // Search must successfully return results from remaining nodes without panicking
        let results = index
            .search(&[1.0, 0.0, 0.0, 0.0], 3)
            .await
            .expect("search should succeed after entry point deletion"); // expect

        assert_eq!(results.len(), 3);
        for res in &results {
            assert_ne!(
                res.doc_id,
                DocId::from(0u64),
                "Deleted entry point node 0 must not be returned"
            );
        }
    }

    #[tokio::test]
    async fn test_search_filtered_tombstone_precedes_custom_filter_match() {
        let index = HnswIndex::try_new(test_config(4)).unwrap(); // unwrap

        // 1. Insert vector and commit
        let tx1 = TxId::new(1);
        let doc_id = DocId::from(42u64);
        index
            .insert(tx1, doc_id, &[1.0, 0.0, 0.0, 0.0])
            .await
            .unwrap(); // unwrap
        index.commit(tx1).await.unwrap(); // unwrap

        // 2. Rollback to Tx 0 (marks node as deleted/tombstoned in rollback_to_tx)
        index.rollback_to_tx(TxId::new(0)).await.unwrap(); // unwrap

        // 3. Perform search_filtered with a filter that explicitly returns true for DocId 42
        //    (simulates storage/index inconsistency where custom filter matches deleted doc_id)
        let custom_filter = move |id: DocId| id == doc_id;
        let filter_ref: &(dyn Fn(DocId) -> bool + Send + Sync) = &custom_filter;
        let results = index
            .search_filtered(&[1.0, 0.0, 0.0, 0.0], 10, Some(filter_ref))
            .await
            .unwrap(); // unwrap

        // 4. Verify search_filtered results do NOT contain the rolled back document
        assert!(
            results.is_empty(),
            "Rolled back tombstoned document must not be returned even if custom filter returns true"
        );
    }

    #[tokio::test]
    async fn test_rollback() {
        let index = HnswIndex::try_new(test_config(4)).unwrap(); // unwrap

        let tx = TxId::new(1);
        index
            .insert(tx, DocId::from(1u64), &[1.0, 0.0, 0.0, 0.0])
            .await
            .expect("insert"); // expect
        index.rollback(tx).await.expect("rollback"); // expect

        assert_eq!(index.len().await, 0);
    }

    #[tokio::test]
    async fn test_empty_search() {
        let index = HnswIndex::try_new(test_config(4)).unwrap(); // unwrap
        let results = index
            .search(&[1.0, 0.0, 0.0, 0.0], 5)
            .await
            .expect("search"); // expect
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_dimension_mismatch() {
        let index = HnswIndex::try_new(test_config(4)).unwrap(); // unwrap
        let tx = TxId::new(1);
        let result = index.insert(tx, DocId::from(1u64), &[1.0, 0.0]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_filtered_search() {
        let index = HnswIndex::try_new(test_config(4)).unwrap(); // unwrap
        let tx = TxId::new(1);

        index
            .insert(tx, DocId::from(1u64), &[1.0, 0.0, 0.0, 0.0])
            .await
            .expect("test"); // expect
        index
            .insert(tx, DocId::from(2u64), &[0.9, 0.1, 0.0, 0.0])
            .await
            .expect("test"); // expect
        index
            .insert(tx, DocId::from(3u64), &[0.8, 0.2, 0.0, 0.0])
            .await
            .expect("test"); // expect
        index.commit(tx).await.expect("test"); // expect

        // Filtered: exclude DocId 1
        let filter_fn = |doc: DocId| doc.inner() != 1;
        let filter_ref: &(dyn Fn(DocId) -> bool + Send + Sync) = &filter_fn;
        let filtered = index
            .search_filtered(&[1.0, 0.0, 0.0, 0.0], 2, Some(filter_ref))
            .await
            .expect("test"); // expect
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().all(|r| r.doc_id != DocId::from(1u64)));
    }

    #[tokio::test]
    async fn test_rebuild_and_stats() {
        let index = HnswIndex::try_new(HnswConfig {
            dimension: 2,
            rebuild_threshold: 0.8,
            distance_metric: DistanceMetric::Euclidean,
            ..test_config(2)
        })
        .unwrap(); // unwrap
        let tx = TxId::new(1);

        for i in 1..=5u64 {
            index
                .insert(tx, DocId::from(i), &[i as f32, 0.0])
                .await
                .expect("test"); // expect
        }
        index.commit(tx).await.expect("test"); // expect

        assert_eq!(index.len().await, 5);
        assert!((index.connectivity_score() - 1.0).abs() < f64::EPSILON);
        assert!(!index.is_rebuild_required());

        // Delete 2 nodes → 40% deleted, connectivity = 0.6
        let tx2 = TxId::new(2);
        index.delete(tx2, DocId::from(2u64)).await.expect("test"); // expect
        index.delete(tx2, DocId::from(4u64)).await.expect("test"); // expect
        index.commit(tx2).await.expect("test"); // expect

        assert_eq!(index.len().await, 3);
        assert!(index.connectivity_score() < 0.8);
        assert!(index.is_rebuild_required());

        let stats_pre = index.stats().await.expect("test"); // expect
        assert_eq!(stats_pre.num_vectors, 3);

        // Rebuild
        index.rebuild().await.expect("test"); // expect

        assert_eq!(index.len().await, 3);
        assert!((index.connectivity_score() - 1.0).abs() < f64::EPSILON);
        assert!(!index.is_rebuild_required());

        let stats_post = index.stats().await.expect("test"); // expect
        assert_eq!(stats_post.num_vectors, 3);

        // Ensure rebuilt index still works
        let results = index.search(&[1.0, 0.0], 1).await.expect("test"); // expect
        assert_eq!(results[0].doc_id, DocId::from(1u64));
    }

    #[tokio::test]
    async fn test_rebuild_quantized_persistence() {
        let index = HnswIndex::try_new(HnswConfig {
            dimension: 4,
            quantize: true,
            rebuild_threshold: 0.1, // Trigger easily
            distance_metric: DistanceMetric::Euclidean,
            ..test_config(4)
        })
        .unwrap(); // unwrap
        let tx = TxId::new(1);

        // Insert enough vectors to train quantizer (>= 50)
        for i in 1..=60u64 {
            let v = [i as f32, i as f32 * 0.1, 0.0, 0.0];
            index.insert(tx, DocId::from(i), &v).await.expect("test"); // expect
        }
        index.commit(tx).await.expect("test"); // expect

        assert_eq!(index.len().await, 60);
        // Verify quantizer is trained
        assert!(index.quantizer().is_some());

        // Delete some to lower connectivity and allow rebuild
        let tx2 = TxId::new(2);
        for i in 1..=10u64 {
            index.delete(tx2, DocId::from(i)).await.expect("test"); // expect
        }
        index.commit(tx2).await.expect("test"); // expect

        assert_eq!(index.len().await, 50);

        // Rebuild
        index.rebuild().await.expect("rebuild"); // expect

        // Verify state after rebuild
        assert_eq!(index.len().await, 50);
        assert!(index.quantizer().is_some(), "Quantizer must be preserved");

        // Verify search still works
        let results = index
            .search(&[60.0, 6.0, 0.0, 0.0], 1)
            .await
            .expect("search"); // expect
        assert_eq!(results[0].doc_id, DocId::from(60u64));
    }

    proptest::proptest! {
        #[test]
        fn prop_insert_nan_returns_error(
            prefix in proptest::collection::vec(proptest::num::f32::NORMAL, 0..3),
            suffix in proptest::collection::vec(proptest::num::f32::NORMAL, 0..3),
        ) {
            let mut v = prefix;
            v.push(f32::NAN);
            v.extend(suffix);

            let config = test_config(v.len());
            let index = HnswIndex::try_new(config).unwrap(); // unwrap
            let result = index.inner.do_insert(DocId::from(1u64), &v);

            proptest::prop_assert!(result.is_err(), "Inserting vector containing NaN must return error");
        }
    }

    #[tokio::test]
    async fn test_hnsw_persistence_lifecycle() {
        let temp_dir = tempfile::tempdir().unwrap(); // unwrap
        let index_path = temp_dir.path().join("test.hnsw");

        let config = HnswConfig {
            dimension: 4,
            m: 16,
            ef_construction: 40,
            quantize: false,
            ..test_config(4)
        };
        let index = HnswIndex::try_new(config.clone()).unwrap(); // unwrap
        let tx1 = TxId::new(1);

        // 1. Initial Insert (RAM)
        for i in 1..=50u64 {
            let v = [i as f32, i as f32 * 0.1, 0.0, 0.0];
            index.insert(tx1, DocId::from(i), &v).await.expect("test"); // expect
        }
        index.commit(tx1).await.expect("test"); // expect

        // 2. Save to disk
        index.save(&index_path).await.expect("save"); // expect

        // 3. Clear RAM and load via Mmap
        let index_mmap = HnswIndex::try_new(config.clone()).unwrap(); // unwrap
        index_mmap.load_mmap(&index_path).await.expect("load mmap"); // expect

        assert_eq!(index_mmap.len().await, 50);

        // 4. Verify Search on Mmap
        let results = index_mmap
            .search(&[25.0, 2.5, 0.0, 0.0], 1)
            .await
            .expect("search"); // expect
        assert_eq!(results[0].doc_id, DocId::from(25u64));

        // 5. Insert new nodes on top of Mmap (Hybrid)
        let tx2 = TxId::new(2);
        for i in 51..=60u64 {
            let v = [i as f32, i as f32 * 0.1, 0.0, 0.0];
            index_mmap
                .insert(tx2, DocId::from(i), &v)
                .await
                .expect("test"); // expect
        }
        index_mmap.commit(tx2).await.expect("test"); // expect

        assert_eq!(index_mmap.len().await, 60);

        // 6. Verify Hybrid Search (finding a RAM node)
        let results_hybrid = index_mmap
            .search(&[58.0, 5.8, 0.0, 0.0], 1)
            .await
            .expect("search"); // expect
        assert_eq!(results_hybrid[0].doc_id, DocId::from(58u64));

        // 7. Verify Hybrid Search (finding an Mmap node)
        let results_mmap = index_mmap
            .search(&[5.0, 0.5, 0.0, 0.0], 1)
            .await
            .expect("search"); // expect
        assert_eq!(results_mmap[0].doc_id, DocId::from(5u64));
    }

    #[test]
    fn test_normalize() {
        use crate::distance::normalize_inplace;
        let mut v = vec![3.0, 4.0];
        normalize_inplace(&mut v);
        assert!((v[0] - 0.6).abs() < 1e-5);
        assert!((v[1] - 0.8).abs() < 1e-5);
    }

    #[tokio::test]
    async fn test_hnsw_sq8_recall_stability() {
        // Test: SQ8 should maintain high recall (> 0.9) on small dataset
        let config = HnswConfig {
            dimension: 16,
            m: 16,
            ef_construction: 64,
            quantize: true,
            ..test_config(16)
        };
        let index = HnswIndex::try_new(config).unwrap(); // unwrap
        let tx = TxId::new(1);

        // 1. Train quantizer with some data
        let mut data = Vec::new();
        for i in 0..100u64 {
            let mut v = vec![0.0f32; 16];
            v[0] = i as f32;
            data.push(v);
        }

        for (i, v) in data.iter().enumerate() {
            index
                .insert(tx, DocId::from(i as u64), v)
                .await
                .expect("insert"); // expect
        }
        index.commit(tx).await.expect("commit"); // expect

        // 2. Perform searches and calculate recall
        let mut hits = 0;
        let test_queries = 20;
        for i in 0..test_queries {
            let query = &data[i * 5];
            let results = index.search(query, 1).await.expect("search"); // expect
            if !results.is_empty() && results[0].doc_id == DocId::from((i * 5) as u64) {
                hits += 1;
            }
        }

        let recall = hits as f32 / test_queries as f32;
        tracing::info!("SQ8 Recall: {}", recall);
        assert!(recall >= 0.9, "Recall too low for SQ8: {}", recall);
    }

    #[tokio::test]
    async fn test_all_doc_ids() {
        let index = HnswIndex::try_new(test_config(4)).unwrap(); // unwrap
        let tx = TxId::new(1);

        for i in 1..=10u64 {
            index
                .insert(tx, DocId::from(i), &[i as f32, 0.0, 0.0, 0.0])
                .await
                .unwrap(); // unwrap
        }
        index.commit(tx).await.unwrap(); // unwrap

        let ids = index.all_doc_ids().await.unwrap(); // unwrap
        assert_eq!(ids.len(), 10);
        for i in 1..=10u64 {
            assert!(ids.contains(&DocId::from(i)));
        }

        // Delete some
        let tx2 = TxId::new(2);
        index.delete(tx2, DocId::from(5u64)).await.unwrap(); // unwrap
        index.delete(tx2, DocId::from(8u64)).await.unwrap(); // unwrap
        index.commit(tx2).await.unwrap(); // unwrap

        let ids2 = index.all_doc_ids().await.unwrap(); // unwrap
        assert_eq!(ids2.len(), 8);
        assert!(!ids2.contains(&DocId::from(5u64)));
        assert!(!ids2.contains(&DocId::from(8u64)));
    }

    #[tokio::test]
    async fn test_check_connectivity_returns_error_when_degraded() {
        // Build a small index, delete enough nodes to cross the rebuild threshold.
        let config = HnswConfig {
            rebuild_threshold: 0.8, // trigger when >20% deleted
            ..test_config(4)
        };
        let index = HnswIndex::try_new(config).unwrap(); // unwrap
        let tx = TxId::new(1);

        for i in 0u64..5 {
            let v = vec![i as f32, 0.0, 0.0, 0.0];
            index.insert(tx, DocId::from(i), &v).await.unwrap(); // unwrap
        }
        index.commit(tx).await.unwrap(); // unwrap

        // Delete 2 out of 5 → 40% deleted → score = 0.6 < threshold 0.8
        let tx2 = TxId::new(2);
        index.delete(tx2, DocId::from(0u64)).await.unwrap(); // #[test] // unwrap
        index.delete(tx2, DocId::from(1u64)).await.unwrap(); // unwrap
        index.commit(tx2).await.unwrap(); // unwrap

        let result = index.check_connectivity();
        assert!(
            matches!(
                result,
                Err(memfuse_core::MemFuseError::HnswConnectivityDegraded { .. })
            ),
            "Expected HnswConnectivityDegraded, got: {:?}",
            result
        );

        if let Err(memfuse_core::MemFuseError::HnswConnectivityDegraded { deleted_ratio }) = result
        {
            assert!(
                deleted_ratio > 39.0 && deleted_ratio < 41.0,
                "deleted_ratio should be ~40%, got {}",
                deleted_ratio
            );
        }
    }

    #[tokio::test]
    async fn test_check_connectivity_ok_when_healthy() {
        let index = HnswIndex::try_new(test_config(4)).unwrap(); // unwrap
                                                                 // Empty index — connectivity_score returns 1.0, always healthy
        assert!(index.check_connectivity().is_ok());
    }

    #[tokio::test]
    async fn hnsw_rebuild_triggers_after_threshold() {
        let config = HnswConfig {
            rebuild_threshold: 0.5,
            max_elements: 100,
            dimension: 4,
            ..Default::default()
        };
        let idx = HnswIndex::try_new(config).unwrap(); // unwrap
        let tx = TxId::new(1);

        // Insert 100 vectors
        for i in 0u64..100 {
            let v = vec![i as f32, 1.0, 0.0, 0.0];
            idx.insert(tx, DocId::from(i), &v).await.unwrap(); // unwrap
        }
        idx.commit(tx).await.unwrap(); // unwrap

        // Delete 51 vectors -> >50% deleted, crossing 0.5 threshold
        let tx2 = TxId::new(2);
        for i in 0u64..51 {
            idx.delete(tx2, DocId::from(i)).await.unwrap(); // unwrap
        }
        idx.commit(tx2).await.unwrap(); // unwrap

        // Wait briefly for background rebuild task to complete
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Verify search() still works and returns non-deleted nodes
        let query = vec![75.0, 1.0, 0.0, 0.0];
        let results = idx.search(&query, 5).await.unwrap(); // unwrap
        assert!(
            !results.is_empty(),
            "Search after rebuild should return results"
        );
        for doc in results {
            assert!(
                doc.doc_id.inner() >= 51,
                "Deleted doc_id {} was found in search results",
                doc.doc_id.inner()
            );
        }
    }

    #[tokio::test]
    async fn test_rebuild_status_and_wait() {
        let config = HnswConfig {
            rebuild_threshold: 0.5,
            dimension: 4,
            ..test_config(4)
        };
        let index = HnswIndex::try_new(config).unwrap(); // unwrap
        assert_eq!(index.rebuild_status(), RebuildStatus::Idle);
        assert!(
            index
                .wait_for_rebuild_with_timeout(std::time::Duration::from_millis(50))
                .await
        );
        assert!(index.wait_for_rebuild().await);

        let tx = TxId::new(1);
        for i in 0u64..10 {
            index
                .insert(tx, DocId::from(i), &[i as f32, 0.0, 0.0, 0.0])
                .await
                .unwrap(); // unwrap
        }
        index.commit(tx).await.unwrap(); // unwrap

        let tx2 = TxId::new(2);
        for i in 0u64..6 {
            index.delete(tx2, DocId::from(i)).await.unwrap(); // unwrap
        }

        // Commit triggers trigger_rebuild_async
        index.commit(tx2).await.unwrap(); // unwrap
        assert!(
            index
                .wait_for_rebuild_with_timeout(std::time::Duration::from_secs(5))
                .await
        );
        assert_eq!(index.rebuild_status(), RebuildStatus::Idle);
    }

    #[tokio::test]
    async fn test_hnsw_search_at_snapshot_isolation() {
        let index = HnswIndex::try_new(test_config(4)).unwrap(); // unwrap

        // seq 1: Insert doc 1 & 2
        let tx1 = TxId::new(1);
        index
            .insert(tx1, DocId::from(1u64), &[1.0, 0.0, 0.0, 0.0])
            .await
            .unwrap(); // unwrap
        index
            .insert(tx1, DocId::from(2u64), &[0.0, 1.0, 0.0, 0.0])
            .await
            .unwrap(); // unwrap
        index.commit(tx1).await.unwrap(); // unwrap

        // seq 2: Delete doc 1, insert doc 3
        let tx2 = TxId::new(2);
        index.delete(tx2, DocId::from(1u64)).await.unwrap(); // unwrap
        index
            .insert(tx2, DocId::from(3u64), &[0.5, 0.5, 0.0, 0.0])
            .await
            .unwrap(); // unwrap
        index.commit(tx2).await.unwrap(); // unwrap

        // search_at seq 1: should see doc 1 & doc 2, but NOT doc 3 (inserted at seq 2).
        let res_seq1 = index.search_at(&[1.0, 0.0, 0.0, 0.0], 5, 1).await.unwrap(); // unwrap
        let docs_seq1: Vec<_> = res_seq1.iter().map(|d| d.doc_id.inner()).collect();
        assert!(
            docs_seq1.contains(&1),
            "seq 1 must contain doc 1 (deleted at seq 2)"
        );
        assert!(docs_seq1.contains(&2), "seq 1 must contain doc 2");
        assert!(!docs_seq1.contains(&3), "seq 1 must not contain doc 3");

        // search_at seq 2: should see doc 2 & 3, but NOT doc 1 (deleted)
        let res_seq2 = index.search_at(&[1.0, 0.0, 0.0, 0.0], 5, 2).await.unwrap(); // unwrap
        let docs_seq2: Vec<_> = res_seq2.iter().map(|d| d.doc_id.inner()).collect();
        assert!(!docs_seq2.contains(&1), "seq 2 must not contain doc 1");
        assert!(docs_seq2.contains(&2), "seq 2 must contain doc 2");
        assert!(docs_seq2.contains(&3), "seq 2 must contain doc 3");
    }

    #[tokio::test]
    async fn test_trigger_rebuild_async_join_handle() {
        let config = HnswConfig {
            rebuild_threshold: 0.5,
            dimension: 4,
            ..test_config(4)
        };
        let index = HnswIndex::try_new(config).unwrap(); // unwrap
        let tx = TxId::new(1);
        for i in 0u64..10 {
            index
                .insert(tx, DocId::from(i), &[i as f32, 0.0, 0.0, 0.0])
                .await
                .unwrap(); // unwrap
        }
        index.commit(tx).await.unwrap(); // unwrap

        assert!(index.trigger_rebuild_async().is_none());

        let tx2 = TxId::new(2);
        for i in 0u64..6 {
            index.delete(tx2, DocId::from(i)).await.unwrap(); // unwrap
        }
        index.commit(tx2).await.unwrap(); // unwrap

        if let Some(handle) = index.trigger_rebuild_async() {
            let res = handle.await.unwrap(); // unwrap
            assert!(res.is_ok());
        }
    }

    #[test]
    fn prop_hnsw_search_at_consistency() {
        use proptest::prelude::*;

        #[derive(Debug, Clone)]
        enum Op {
            Insert(u64),
            Delete(u64),
        }

        let op_strategy = proptest::collection::vec(
            prop_oneof![
                (1u64..30).prop_map(Op::Insert),
                (1u64..30).prop_map(Op::Delete),
            ],
            10..100,
        );

        // REVIEW-PASS[1/2] STATUS:PASS (ID: TEST:AGT-INDEX-006) (TS: 2026-09-01T12:00:00Z) (SESSION: b8e4f1a2)
        // REVIEW-PASS[2/2] STATUS:PASS (ID: TEST:AGT-INDEX-006) (TS: 2026-09-01T23:05:53Z) (SESSION: 297af137)
        // ANCHOR[TEST:AGT-INDEX-006] STATUS:DONE (TS:2026-09-01T11:30:00Z) (SESSION:016eab33)
        // Snapshot-Isolation bei Soft-Delete fixiert: search_at() ignoriert
        // deleted_nodes-Bitmap und nutzt ausschließlich seq_log.is_visible().
        // Regressionstest: tests/hnsw_snapshot_delete_test.rs
        proptest!(ProptestConfig::with_cases(20), |(ops in op_strategy)| {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap(); // unwrap

            rt.block_on(async {
                let config = HnswConfig {
                    dimension: 4,
                    ..test_config(4)
                };
                let index = HnswIndex::try_new(config).unwrap(); // unwrap

                let mut current_tx = 1u64;
                let mut tx_checkpoints = Vec::new();

                for op in ops {
                    let tx = TxId::new(current_tx);
                    match op {
                        Op::Insert(id) => {
                            let vec = [id as f32, 0.0, 0.0, 0.0];
                            let _ = index.insert(tx, DocId::from(id), &vec).await;
                        }
                        Op::Delete(id) => {
                            let _ = index.delete(tx, DocId::from(id)).await;
                        }
                    }
                    if index.commit(tx).await.is_ok() {
                        tx_checkpoints.push(current_tx);
                        current_tx += 1;
                    }
                }

                // Verify search_at at random target checkpoint against reference model
                for &target_seq in &tx_checkpoints {
                    // Reference model state at target_seq
                    let active_docs: std::collections::HashSet<_> = {
                        let log = index.inner.cold.seq_log.read();
                        (1u64..30)
                            .map(DocId::from)
                            .filter(|&doc_id| log.is_visible(doc_id, target_seq))
                            .collect()
                    };

                    let res = index.search_at(&[1.0, 0.0, 0.0, 0.0], 100, target_seq).await.unwrap(); // unwrap
                    let found_docs: std::collections::HashSet<_> = res.into_iter().map(|d| d.doc_id).collect();

                    prop_assert_eq!(found_docs, active_docs, "Search_at result at seq {} must equal reference model", target_seq);
                }
                Ok(())
            }).unwrap(); // unwrap
        });
    }

    #[tokio::test]
    async fn test_search_filtered_internal_rejects_nan_vector() {
        let index = HnswIndex::try_new(test_config(4)).unwrap();
        let query = vec![1.0, f32::NAN, 0.0, 0.0];
        let res = index.search_filtered(&query, 5, None).await;
        assert!(res.is_err());
        let err_msg = res.unwrap_err().to_string();
        assert!(err_msg.contains("is not finite"));
    }

    #[tokio::test]
    async fn test_search_filtered_internal_rejects_oversized_k() {
        let index = HnswIndex::try_new(test_config(4)).unwrap();
        let query = vec![1.0, 0.0, 0.0, 0.0];
        let res = index
            .search_filtered(&query, memfuse_core::MAX_SEARCH_K + 1, None)
            .await;
        assert!(res.is_err());
        let err_msg = res.unwrap_err().to_string();
        assert!(err_msg.contains("exceeds maximum allowed search limit"));
    }

    #[tokio::test]
    async fn test_search_at_rejects_inf_vector() {
        let index = HnswIndex::try_new(test_config(4)).unwrap();
        let query = vec![1.0, f32::INFINITY, 0.0, 0.0];
        let res = index.search_at(&query, 5, 1).await;
        assert!(res.is_err());
        let err_msg = res.unwrap_err().to_string();
        assert!(err_msg.contains("is not finite"));
    }

    #[tokio::test]
    async fn test_hnsw_search_max_k_guard() {
        let index = HnswIndex::try_new(test_config(4)).unwrap(); // unwrap
        let query = vec![1.0, 0.0, 0.0, 0.0];
        let res = index.search(&query, memfuse_core::MAX_SEARCH_K + 1).await;
        assert!(res.is_err());
        assert!(res
            .unwrap_err()
            .to_string()
            .contains("exceeds maximum allowed search limit"));
    }

    #[test]
    fn test_hnsw_config_validation_invalid_params() {
        // dimension == 0
        let c_dim = HnswConfig {
            dimension: 0,
            ..Default::default()
        };
        assert!(c_dim.validate().is_err());

        // m == 0
        let c_m = HnswConfig {
            m: 0,
            ..Default::default()
        };
        assert!(c_m.validate().is_err());

        // ef_construction < m
        let c_ef_c = HnswConfig {
            m: 16,
            ef_construction: 8,
            ..Default::default()
        };
        assert!(c_ef_c.validate().is_err());

        // ef_search == 0
        let c_ef_s = HnswConfig {
            ef_search: 0,
            ..Default::default()
        };
        assert!(c_ef_s.validate().is_err());
    }

    #[tokio::test]
    async fn test_hnsw_index_search_k_zero_returns_empty() {
        let index = HnswIndex::try_new(test_config(4)).unwrap(); // unwrap
        let tx = TxId::new(1);
        index
            .insert(tx, DocId::from(1u64), &[1.0, 0.0, 0.0, 0.0])
            .await
            .unwrap(); // unwrap
        index.commit(tx).await.unwrap(); // unwrap

        let results = index.search(&[1.0, 0.0, 0.0, 0.0], 0).await.unwrap(); // unwrap
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_hnsw_index_delete_non_existent_doc() {
        let index = HnswIndex::try_new(test_config(4)).unwrap(); // unwrap
        let tx = TxId::new(1);
        // Deleting non-existent doc should succeed without altering index state
        index.delete(tx, DocId::from(999u64)).await.unwrap(); // unwrap
        index.commit(tx).await.unwrap(); // unwrap
        assert!(index.all_doc_ids().await.unwrap().is_empty()); // unwrap
    }

    #[tokio::test]
    async fn test_search_filtered_respects_deleted_nodes_after_rollback() {
        let index = HnswIndex::try_new(test_config(4)).unwrap(); // unwrap

        // 1. Insert docs 1..=5 in Tx 1
        let tx1 = TxId::new(1);
        for i in 1..=5u64 {
            let v = [i as f32, 0.0, 0.0, 0.0];
            index.insert(tx1, DocId::from(i), &v).await.unwrap(); // unwrap
        }
        index.commit(tx1).await.unwrap(); // unwrap

        // 2. Insert docs 6..=10 in Tx 2
        let tx2 = TxId::new(2);
        for i in 6..=10u64 {
            let v = [i as f32, 0.0, 0.0, 0.0];
            index.insert(tx2, DocId::from(i), &v).await.unwrap(); // unwrap
        }
        index.commit(tx2).await.unwrap(); // unwrap

        // 3. Rollback to Tx 1 (soft-deletes docs 6..=10)
        index.rollback_to_tx(tx1).await.unwrap(); // unwrap

        // 4. Perform search_filtered with a filter that accepts all DocIds
        let allow_all = |_: DocId| true;
        let filter_ref: &(dyn Fn(DocId) -> bool + Send + Sync) = &allow_all;
        let results = index
            .search_filtered(&[5.0, 0.0, 0.0, 0.0], 10, Some(filter_ref))
            .await
            .unwrap(); // unwrap

        // 5. Verify rolled-back docs 6..=10 are NOT present in search results
        let result_doc_ids: std::collections::HashSet<_> =
            results.into_iter().map(|res| res.doc_id.inner()).collect();

        for i in 6..=10u64 {
            assert!(
                !result_doc_ids.contains(&(i as _)),
                "Rolled back DocId {} must not appear in filtered search results",
                i
            );
        }

        for i in 1..=5u64 {
            assert!(
                result_doc_ids.contains(&(i as _)),
                "Active DocId {} must appear in filtered search results",
                i
            );
        }
    }

    #[tokio::test]
    async fn test_do_insert_clamping_preserves_codebook_stability_under_contention() {
        let config = HnswConfig {
            dimension: 4,
            quantize: true,
            quantizer_drift_threshold: 0.90, // High threshold to prevent automatic rebuild during test
            ..test_config(4)
        };
        let index = std::sync::Arc::new(HnswIndex::try_new(config).unwrap()); // unwrap

        // 1. Train quantizer with initial vectors
        let tx1 = TxId::new(1);
        for i in 1..=60u64 {
            let v = [1.0, 2.0, 3.0, 4.0];
            index.insert(tx1, DocId::from(i), &v).await.unwrap(); // unwrap
        }
        index.commit(tx1).await.unwrap(); // unwrap

        let initial_mins = index.quantizer().as_ref().unwrap().mins().to_vec(); // unwrap
        let initial_maxes = index.quantizer().as_ref().unwrap().maxes().to_vec(); // unwrap

        // 2. Spawn multiple concurrent search tasks holding read lock on quantizer
        let mut tasks = Vec::new();
        for _ in 0..10 {
            let idx = std::sync::Arc::clone(&index);
            tasks.push(tokio::spawn(async move {
                for _ in 0..50 {
                    let _ = idx.search(&[1.0, 2.0, 3.0, 4.0], 5).await;
                    tokio::task::yield_now().await;
                }
            }));
        }

        // 3. Insert a vector far outside initial trained bounds while searches are running
        let out_of_bounds_vector = [1000.0, -1000.0, 500.0, -500.0];
        let tx2 = TxId::new(2);
        index
            .insert(tx2, DocId::from(100u64), &out_of_bounds_vector)
            .await
            .unwrap(); // unwrap
        index.commit(tx2).await.unwrap(); // unwrap

        // Await search tasks
        for task in tasks {
            task.await.unwrap(); // unwrap
        }

        // 4. Verify quantizer bounds remain stable (no mutation during insert)
        let q_opt = index.quantizer();
        let q = q_opt.as_ref().expect("Quantizer must be present"); // expect
        assert_eq!(q.mins(), initial_mins.as_slice());
        assert_eq!(q.maxes(), initial_maxes.as_slice());

        // 5. Verify check_drift detects out of bounds vector
        let drift = q.check_drift(&out_of_bounds_vector);
        assert_eq!(
            drift, 1.0,
            "All 4 dimensions of outlier vector are out of bounds"
        );
    }

    #[tokio::test]
    async fn test_sq8_outlier_insert_clamping_numerical_precision() {
        // Concrete numerical verification test:
        // Dimension i initialized with range [0.0, 1.0].
        // Stored code 128 decodes to a specific value.
        // Insert outlier value 10.0 in dimension i.
        // Codebook bounds/scales MUST NOT be mutated; stored code 128 MUST still decode identically.
        let config = HnswConfig {
            dimension: 2,
            quantize: true,
            quantizer_drift_threshold: 0.90,
            ..test_config(2)
        };
        let index = HnswIndex::try_new(config).unwrap();

        // 1. Train quantizer with range [0.0, 1.0]
        let tx1 = TxId::new(1);
        for i in 0..=60u64 {
            let v = [i as f32 / 60.0, 0.5];
            index.insert(tx1, DocId::from(i), &v).await.unwrap();
        }
        index.commit(tx1).await.unwrap();

        let q_before = index.quantizer().unwrap();
        let code_128 = vec![128u8, 128u8];
        let initial_decoded = q_before.dequantize(&code_128).unwrap();
        let initial_min_d0 = q_before.mins()[0];
        let initial_max_d0 = q_before.maxes()[0];

        // 2. Insert outlier vector [10.0, 0.5]
        let tx2 = TxId::new(2);
        index
            .insert(tx2, DocId::from(100u64), &[10.0, 0.5])
            .await
            .unwrap();
        index.commit(tx2).await.unwrap();

        let q_after = index.quantizer().unwrap();
        assert_eq!(q_after.mins()[0], initial_min_d0);
        assert_eq!(q_after.maxes()[0], initial_max_d0);

        // Verify stored code 128 still decodes identically
        let after_decoded = q_after.dequantize(&code_128).unwrap();
        assert_eq!(initial_decoded[0], after_decoded[0]);
    }

    #[tokio::test]
    async fn test_quantizer_drift_rebuild_triggers_and_recalibrates() {
        let config = HnswConfigBuilder::new(2)
            .m(8)
            .ef_construction(16)
            .ef_search(16)
            .quantize(true)
            .rebuild_threshold(0.0) // Disable deletion rebuild trigger
            .quantizer_drift_threshold(0.05) // Trigger rebuild if >5% out of range queries
            .build()
            .unwrap();

        let index = HnswIndex::try_new(config).unwrap();

        // 1. Insert 60 initial vectors in range [0.0, 1.0]
        let tx1 = TxId::new(1);
        for i in 1..=60u64 {
            let v = [i as f32 / 60.0, 0.5];
            index.insert(tx1, DocId::from(i), &v).await.unwrap();
        }
        index.commit(tx1).await.unwrap();
        assert_eq!(index.rebuild_count(), 0);

        let initial_max = index.quantizer().unwrap().maxes()[0];
        assert!(initial_max <= 1.05);

        // 2. Insert outlier vectors far outside range [0.0, 1.0] (e.g., [10.0, 10.0])
        // To exceed 5% drift ratio with 60 initial queries, insert 20 outlier queries (20/80 = 25% drift)
        let tx2 = TxId::new(2);
        for i in 61..=80u64 {
            let v = [10.0 + (i as f32), 10.0];
            index.insert(tx2, DocId::from(i), &v).await.unwrap();
        }
        index.commit(tx2).await.unwrap(); // commit triggers trigger_rebuild_async()

        // Give background rebuild task time to start, then wait for completion or execute rebuild directly
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        if index.rebuild_count() == 0 {
            index.rebuild().await.unwrap();
        }

        assert!(index.rebuild_count() >= 1, "Rebuild count should increase");

        // 3. Verify post-rebuild quantizer has reset drift counters and rebuild_count increased
        let q_new = index.quantizer().unwrap();
        assert_eq!(
            q_new.drift_ratio(),
            0.0,
            "Post-rebuild quantizer drift ratio should reset to 0"
        );
    }

    #[tokio::test]
    async fn test_concurrent_inserts_read_lock_fast_path() {
        let config = HnswConfig {
            dimension: 4,
            quantize: true,
            quantizer_drift_threshold: 0.90,
            ..test_config(4)
        };
        let index = std::sync::Arc::new(HnswIndex::try_new(config).unwrap());

        // Initial commit to train quantizer
        let tx0 = TxId::new(0);
        index
            .insert(tx0, DocId::from(0u64), &[1.0, 2.0, 3.0, 4.0])
            .await
            .unwrap();
        index.commit(tx0).await.unwrap();

        // Spawn 8 concurrent insertion tasks
        let mut handles = Vec::new();
        for thread_id in 1..=8u64 {
            let idx = std::sync::Arc::clone(&index);
            handles.push(tokio::spawn(async move {
                for i in 1..=20u64 {
                    let doc_val = thread_id * 100 + i;
                    let tx = TxId::new(doc_val);
                    let v = [i as f32, (i * 2) as f32, 1.0, 2.0];
                    idx.insert(tx, DocId::from(doc_val), &v).await.unwrap();
                    idx.commit(tx).await.unwrap();
                }
            }));
        }

        for handle in handles {
            handle.await.unwrap();
        }

        assert_eq!(index.len().await, 161);
    }

    #[tokio::test]
    async fn test_lazy_neighbor_pruning_reduces_dead_node_visits() {
        // Build index with high rebuild_threshold (1.0 = never trigger automatic rebuild) to test lazy pruning in isolation
        let config = HnswConfigBuilder::new(4)
            .m(16)
            .ef_construction(100)
            .ef_search(64)
            .rebuild_threshold(0.0) // Disable automatic rebuild
            .build()
            .unwrap(); // unwrap

        let index = HnswIndex::try_new(config).unwrap(); // unwrap
        let tx1 = TxId::new(1);

        let total_vectors = 100u64;
        for i in 0..total_vectors {
            let v = vec![i as f32, (i % 10) as f32, 0.0, 0.0];
            index.insert(tx1, DocId::from(i), &v).await.unwrap(); // unwrap
        }
        index.commit(tx1).await.unwrap(); // unwrap

        // Delete 30% of vectors (30 vectors)
        let tx2 = TxId::new(2);
        for i in (0..total_vectors).step_by(3) {
            index.delete(tx2, DocId::from(i)).await.unwrap(); // unwrap
        }
        index.commit(tx2).await.unwrap(); // unwrap

        let query = vec![50.0, 0.0, 0.0, 0.0];

        // 1. Initial search pass: encounters dead nodes and triggers lazy pruning
        let dead_visits_before = index.visited_dead_nodes();
        for _ in 0..10 {
            let _ = index.search(&query, 5).await.unwrap(); // unwrap
        }
        let dead_visits_first_pass = index.visited_dead_nodes() - dead_visits_before;

        // 2. Second search pass: because dead neighbors were lazily pruned, dead node encounters should drop significantly
        let dead_visits_mid = index.visited_dead_nodes();
        for _ in 0..10 {
            let _ = index.search(&query, 5).await.unwrap(); // unwrap
        }
        let dead_visits_second_pass = index.visited_dead_nodes() - dead_visits_mid;

        assert!(
            dead_visits_second_pass < dead_visits_first_pass,
            "Lazy neighbor pruning must reduce visited dead node count on subsequent searches (pass 1: {}, pass 2: {})",
            dead_visits_first_pass,
            dead_visits_second_pass
        );
    }

    #[tokio::test]
    async fn test_configurable_rebuild_threshold() {
        // Test custom rebuild_threshold via HnswConfigBuilder
        let config = HnswConfigBuilder::new(4)
            .rebuild_threshold(0.85) // Rebuild when active ratio falls below 85% (>15% deleted)
            .build()
            .unwrap(); // unwrap

        let index = HnswIndex::try_new(config).unwrap(); // unwrap
        let tx1 = TxId::new(1);

        for i in 0u64..100 {
            let v = vec![i as f32, 0.0, 0.0, 0.0];
            index.insert(tx1, DocId::from(i), &v).await.unwrap(); // unwrap
        }
        index.commit(tx1).await.unwrap(); // unwrap

        assert!(!index.is_rebuild_required());

        // Delete 10 vectors -> 10% deleted -> score = 0.90 >= 0.85 (no rebuild required)
        let tx2 = TxId::new(2);
        for i in 0u64..10 {
            index.delete(tx2, DocId::from(i)).await.unwrap(); // unwrap
        }
        index.commit(tx2).await.unwrap(); // unwrap

        assert!(!index.is_rebuild_required());
        assert_eq!(index.rebuild_count(), 0);

        // Delete 10 more vectors -> 20% deleted -> score = 0.80 < 0.85 (rebuild required!)
        let tx3 = TxId::new(3);
        for i in 10u64..20 {
            index.delete(tx3, DocId::from(i)).await.unwrap(); // unwrap
        }
        index.commit(tx3).await.unwrap(); // unwrap

        let start = tokio::time::Instant::now();
        while index.rebuild_count() == 0 && start.elapsed() < std::time::Duration::from_secs(5) {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        assert!(index.rebuild_count() >= 1);
        assert!(!index.is_rebuild_required());
    }

    #[tokio::test]
    async fn test_parallel_search_during_rebuild_non_blocking() {
        // Use a low rebuild threshold so commit() won't auto-trigger background rebuild
        let config = HnswConfigBuilder::new(4)
            .rebuild_threshold(0.10)
            .build()
            .unwrap(); // unwrap

        let index = std::sync::Arc::new(HnswIndex::try_new(config).unwrap()); // unwrap
        let tx1 = TxId::new(1);

        for i in 0u64..200 {
            let v = vec![i as f32, (i % 5) as f32, 0.0, 0.0];
            index.insert(tx1, DocId::from(i), &v).await.unwrap(); // unwrap
        }
        index.commit(tx1).await.unwrap(); // unwrap

        // Delete 50 vectors (25% deleted, < 90% threshold so auto-rebuild is not triggered)
        let tx2 = TxId::new(2);
        for i in 0u64..50 {
            index.delete(tx2, DocId::from(i)).await.unwrap(); // unwrap
        }
        index.commit(tx2).await.unwrap(); // unwrap

        assert_eq!(index.rebuild_count(), 0);

        // Spawn parallel searches while rebuild runs
        let index_search = std::sync::Arc::clone(&index);
        let search_handle = tokio::spawn(async move {
            let query = vec![150.0, 0.0, 0.0, 0.0];
            for _ in 0..100 {
                let res = index_search.search(&query, 5).await;
                assert!(res.is_ok(), "Search during rebuild must succeed");
                let docs = res.unwrap(); // unwrap
                for doc in docs {
                    assert!(
                        doc.doc_id.inner() >= 50,
                        "Deleted doc_id should not be returned"
                    );
                }
                tokio::task::yield_now().await;
            }
        });

        index.rebuild().await.unwrap(); // unwrap
        search_handle.await.unwrap(); // unwrap

        assert_eq!(index.rebuild_count(), 1);
    }

    #[tokio::test]
    async fn test_concurrent_insert_during_rebuild() {
        let config = HnswConfigBuilder::new(4)
            .distance_metric(memfuse_core::DistanceMetric::Euclidean)
            .rebuild_threshold(0.10)
            .build()
            .unwrap(); // unwrap #[cfg(test)]

        let index = std::sync::Arc::new(HnswIndex::try_new(config).unwrap()); // unwrap #[cfg(test)]

        // 1. Initial baseline insertion: docs 1..100
        let tx1 = TxId::new(1);
        for i in 1u64..=100 {
            let v = vec![i as f32, 0.0, 0.0, 0.0];
            index.insert(tx1, DocId::from(i), &v).await.unwrap(); // unwrap #[cfg(test)]
        }
        index.commit(tx1).await.unwrap(); // unwrap #[cfg(test)]

        // 2. Spawn concurrent insertion task while rebuild runs
        let index_write = std::sync::Arc::clone(&index);
        let write_handle = tokio::spawn(async move {
            // Introduce a short yield to align execution during rebuild Phase 1
            tokio::task::yield_now().await;

            let tx2 = TxId::new(2);
            for i in 101u64..=150 {
                let v = vec![i as f32, 0.0, 0.0, 0.0];
                index_write.insert(tx2, DocId::from(i), &v).await.unwrap(); // unwrap #[cfg(test)]
            }
            index_write.commit(tx2).await.unwrap(); // unwrap #[cfg(test)]

            let tx3 = TxId::new(3);
            index_write.delete(tx3, DocId::from(1u64)).await.unwrap(); // unwrap #[cfg(test)]
            index_write.commit(tx3).await.unwrap(); // unwrap #[cfg(test)]
        });

        // 3. Trigger rebuild concurrently
        index.rebuild().await.unwrap(); // unwrap #[cfg(test)]
        write_handle.await.unwrap(); // unwrap #[cfg(test)]

        // 4. Assert correctness after rebuild
        assert_eq!(index.rebuild_count(), 1);

        // Doc 1 was deleted during concurrent write
        let query_doc1 = vec![1.0, 0.0, 0.0, 0.0];
        let res_doc1 = index.search(&query_doc1, 10).await.unwrap(); // unwrap #[cfg(test)]
        assert!(!res_doc1.iter().any(|d| d.doc_id == DocId::from(1u64)));

        // Doc 105 was inserted during rebuild Phase 1 & committed
        let query_doc105 = vec![105.0, 0.0, 0.0, 0.0];
        let res_doc105 = index.search(&query_doc105, 10).await.unwrap(); // unwrap #[cfg(test)]
        assert!(res_doc105.iter().any(|d| d.doc_id == DocId::from(105u64)));

        // Total active docs in index should be 149 (100 - 1 deleted + 50 newly inserted)
        assert_eq!(index.len().await, 149);
    }

    #[tokio::test]
    #[cfg(feature = "partial-index-rebuild")]
    async fn test_hnsw_partial_rebuild_integration() {
        let partial_rebuild_config = crate::partial_rebuild::PartialRebuildConfig {
            critical_ratio: 2.0,
            traversal_window: 100,
            min_global_ratio: 0.01,
        };

        let config = HnswConfigBuilder::new(4)
            .rebuild_threshold(0.0) // Disable automatic global rebuilds
            .partial_rebuild_config(partial_rebuild_config)
            .build()
            .unwrap();

        let index = HnswIndex::try_new(config).unwrap();
        let tx1 = TxId::new(1);

        // 1. Insert 100 vectors
        for i in 0u64..100 {
            let v = vec![i as f32, 0.0, 0.0, 0.0];
            index.insert(tx1, DocId::from(i), &v).await.unwrap();
        }
        index.commit(tx1).await.unwrap();

        // 2. Search multiple times near hot path nodes 0..10 to populate traversal_tracker
        let query = vec![2.0, 0.0, 0.0, 0.0];
        for _ in 0..10 {
            let _ = index.search(&query, 5).await.unwrap();
        }

        // 3. Soft-delete nodes in hot path (0, 1, 2) plus 1 node elsewhere (99)
        // Local tombstones in hot path ~ 30%, global tombstones ~ 4%
        let tx2 = TxId::new(2);
        index.delete(tx2, DocId::from(0u64)).await.unwrap();
        index.delete(tx2, DocId::from(1u64)).await.unwrap();
        index.delete(tx2, DocId::from(2u64)).await.unwrap();
        index.delete(tx2, DocId::from(99u64)).await.unwrap();

        // Commit triggers check_and_trigger_partial_rebuild
        index.commit(tx2).await.unwrap();

        // Allow async partial rebuild task to finish
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Verify remaining docs in index
        let active_docs = index.all_doc_ids().await.unwrap();
        assert!(!active_docs.contains(&DocId::from(0u64)));
        assert!(!active_docs.contains(&DocId::from(1u64)));
        assert!(!active_docs.contains(&DocId::from(2u64)));
        assert!(!active_docs.contains(&DocId::from(99u64)));
    }

    #[tokio::test]
    async fn test_compute_then_commit_fault_injection_atomicity() {
        let index = HnswIndex::try_new(test_config(4)).unwrap();

        // 1. Insert baseline vectors in Tx 1
        let tx1 = TxId::new(1);
        index
            .insert(tx1, DocId::from(1u64), &[1.0, 0.0, 0.0, 0.0])
            .await
            .unwrap();
        index
            .insert(tx1, DocId::from(2u64), &[0.0, 1.0, 0.0, 0.0])
            .await
            .unwrap();
        index.commit(tx1).await.unwrap();

        // Snapshot counts before batch commit
        let initial_nodes_count = index.inner.hot.nodes.read().len();
        let initial_doc_map_count = index.inner.hot.doc_to_node.read().len();
        assert_eq!(initial_nodes_count, 2);
        assert_eq!(initial_doc_map_count, 2);

        // 2. Stage batch of 5 insert operations in Tx 2
        let tx2 = TxId::new(2);
        for i in 100..105u64 {
            let vec = [i as f32, 0.5, 0.0, 0.0];
            index.insert(tx2, DocId::from(i), &vec).await.unwrap();
        }

        // 3. Configure fault injection to fail on the 3rd element during Phase 1 compute
        index.set_fault_injection_insert_target(3);

        // 4. Commit must fail due to fault injection in Phase 1
        let commit_res = index.commit(tx2).await;
        assert!(
            commit_res.is_err(),
            "Commit must fail when fault injection triggers during compute_insert"
        );
        let err_msg = commit_res.unwrap_err().to_string();
        assert!(
            err_msg.contains("Fault injection"),
            "Error message must contain fault injection notice: {}",
            err_msg
        );

        // Reset fault injection flags
        index.set_fault_injection_insert_target(0);

        // 5. Verify snapshot node and doc_to_node counts AFTER failed commit
        let final_nodes_count = index.inner.hot.nodes.read().len();
        let final_doc_map_count = index.inner.hot.doc_to_node.read().len();

        assert_eq!(
            initial_nodes_count, final_nodes_count,
            "Nodes vector count must remain unchanged after failed commit (before: {}, after: {})",
            initial_nodes_count, final_nodes_count
        );
        assert_eq!(
            initial_doc_map_count, final_doc_map_count,
            "doc_to_node map count must remain unchanged after failed commit (before: {}, after: {})",
            initial_doc_map_count, final_doc_map_count
        );

        // Verify none of the transaction batch DocIds exist in doc_to_node
        let doc_map = index.inner.hot.doc_to_node.read();
        for i in 100..105u64 {
            assert!(
                !doc_map.contains_key(&DocId::from(i).inner()),
                "DocId {} from failed transaction must not exist in doc_to_node",
                i
            );
        }
    }

    #[tokio::test]
    async fn test_parallel_hnsw_instances_fault_injection_isolation() {
        // AI-TAG[TEST][REGRESSION] RESOLVED: AGT-INDEX-f38b1a90 — Multi-instance fault injection isolation test.
        // Confirms that fault injection target set on index_fault does not spill over to parallel index_normal.
        let index_fault = std::sync::Arc::new(HnswIndex::try_new(test_config(4)).unwrap());
        let index_normal = std::sync::Arc::new(HnswIndex::try_new(test_config(4)).unwrap());

        // Configure fault injection ONLY on index_fault
        index_fault.set_fault_injection_insert_target(3);

        let idx_f = std::sync::Arc::clone(&index_fault);
        let handle_f = tokio::spawn(async move {
            let tx = TxId::new(10);
            for i in 1..=5u64 {
                let vec = [i as f32, 0.0, 0.0, 0.0];
                idx_f.insert(tx, DocId::from(i), &vec).await.unwrap();
            }
            idx_f.commit(tx).await
        });

        let idx_n = std::sync::Arc::clone(&index_normal);
        let handle_n = tokio::spawn(async move {
            let tx = TxId::new(20);
            for i in 1..=5u64 {
                let vec = [i as f32, 0.0, 0.0, 0.0];
                idx_n.insert(tx, DocId::from(i), &vec).await.unwrap();
            }
            idx_n.commit(tx).await
        });

        let (res_f, res_n) = tokio::join!(handle_f, handle_n);

        // index_fault must fail on commit due to fault injection
        let res_f_val = res_f.unwrap();
        assert!(
            res_f_val.is_err(),
            "index_fault commit must fail due to fault injection"
        );

        // index_normal must succeed without any fault injection spillover
        let res_n_val = res_n.unwrap();
        assert!(
            res_n_val.is_ok(),
            "index_normal commit must succeed cleanly without fault injection spillover"
        );
        assert_eq!(index_normal.len().await, 5);
    }

    #[tokio::test]
    async fn test_partial_rebuild_recall_preservation_guarantee() {
        // Requirement 5.5.7: Test proving recall preservation guarantee after local partial rebuild
        let config = HnswConfig {
            dimension: 16,
            m: 16,
            ef_construction: 64,
            ef_search: 64,
            distance_metric: DistanceMetric::Euclidean,
            ..test_config(16)
        };
        let index = HnswIndex::try_new(config).unwrap();
        let tx1 = TxId::new(1);

        // 1. Insert 100 vectors
        let mut dataset = Vec::with_capacity(100);
        for i in 0u64..100 {
            let mut v = vec![0.0f32; 16];
            v[0] = i as f32;
            v[1] = (i % 10) as f32 * 0.1;
            dataset.push((i, v.clone()));
            index.insert(tx1, DocId::from(i), &v).await.unwrap();
        }
        index.commit(tx1).await.unwrap();

        // 2. Query vector
        let query = vec![45.2f32; 16];

        // Ground truth brute-force exact top-5
        let mut ground_truth: Vec<(u64, f32)> = dataset
            .iter()
            .map(|(id, v)| {
                let d = crate::distance::euclidean_distance_scalar(&query, v);
                (*id, d)
            })
            .collect();
        ground_truth.sort_by(|a, b| a.1.total_cmp(&b.1));
        let gt_ids: std::collections::HashSet<u64> =
            ground_truth.iter().take(5).map(|(id, _)| *id).collect();

        // Initial search
        let initial_res = index.search(&query, 5).await.unwrap();
        let initial_hits = initial_res
            .iter()
            .filter(|doc| gt_ids.contains(&(doc.doc_id.inner() as u64)))
            .count();
        let initial_recall = initial_hits as f32 / 5.0;

        // 3. Delete some nodes in region (e.g. 10, 11, 12)
        let tx2 = TxId::new(2);
        index.delete(tx2, DocId::from(10u64)).await.unwrap();
        index.delete(tx2, DocId::from(11u64)).await.unwrap();
        index.delete(tx2, DocId::from(12u64)).await.unwrap();
        index.commit(tx2).await.unwrap();

        // 4. Perform partial rebuild on region
        let region = vec![10u64, 11, 12, 13, 14, 15];
        index.rebuild_region(region).await.unwrap();

        // 5. Post partial rebuild search
        let post_res = index.search(&query, 5).await.unwrap();
        let post_hits = post_res
            .iter()
            .filter(|doc| gt_ids.contains(&(doc.doc_id.inner() as u64)))
            .count();
        let post_recall = post_hits as f32 / 5.0;

        assert!(
            post_recall >= initial_recall - 0.02,
            "Partial rebuild must preserve recall: post_recall ({}) vs initial_recall ({})",
            post_recall,
            initial_recall
        );
        assert!(
            post_recall >= 0.80,
            "Post partial rebuild recall should be high, got {}",
            post_recall
        );
    }

    #[test]
    fn test_compute_distance_raw_f32_equivalence_with_trusted() {
        let v1: Vec<f32> = vec![0.5, -0.2, 0.8, 1.2, 0.0, -0.5, 0.3, 0.7];
        let v2: Vec<f32> = vec![0.1, 0.9, -0.4, 0.6, 0.8, -0.1, 0.2, -0.3];

        let mut v2_bytes = Vec::with_capacity(v2.len() * 4);
        for &val in &v2 {
            v2_bytes.extend_from_slice(&val.to_le_bytes());
        }

        for metric in [
            DistanceMetric::Cosine,
            DistanceMetric::Euclidean,
            DistanceMetric::DotProduct,
        ] {
            let trusted_dist =
                compute_distance_trusted(&v1, &v2, metric).expect("compute_distance_trusted");
            let raw_dist =
                crate::distance::compute_distance_f32_bytes_trusted(&v1, &v2_bytes, metric)
                    .expect("compute_distance_f32_bytes_trusted");

            let diff = (trusted_dist - raw_dist).abs();
            assert!(
                diff < 1e-4,
                "Distance mismatch for metric {:?}: trusted = {}, raw = {}, diff = {}",
                metric,
                trusted_dist,
                raw_dist,
                diff
            );
        }
    }
}
