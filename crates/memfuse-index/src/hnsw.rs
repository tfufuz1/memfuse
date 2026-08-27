//! HNSW (Hierarchical Navigable Small World) vector index.
//! # Hierarchical Navigable Small World (HNSW) Index
//!
//! This module implements the HNSW algorithm for efficient approximate nearest neighbor (ANN) search.
// TODO: Module documentation added
// INVARIANT: Hierarchical Navigable Small World Index.
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
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tokio::sync::Mutex;

/// Default deletion threshold (30% deleted nodes) that triggers graph rebuild / connectivity warnings.
pub const HNSW_REBUILD_THRESHOLD: f64 = 0.30;

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
    /// Defaults to `HNSW_REBUILD_THRESHOLD` (0.30, i.e., 30% deleted nodes).
    pub rebuild_threshold: f64,
    /// Whether to apply SQ8 Scalar Quantization to the index vectors to reduce RAM.
    pub quantize: bool,
    /// Sample size used for ScalarQuantizer recalibration during rebuilds.
    /// Default is 10,000 to balance speed and accuracy.
    pub quantizer_recalibration_sample_size: usize,
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
            rebuild_threshold: 1.0 - HNSW_REBUILD_THRESHOLD,
            quantize: false,
            quantizer_recalibration_sample_size: 10_000,
        }
    }
}

impl HnswConfig {
    /// Validates that the configuration parameters are within acceptable bounds.
    pub fn validate(&self) -> Result<()> {
        // ANCHOR:ALG-FIX:D2-003 — ef_construction < M Guard fehlt
        // ANCHOR:ALG-FIX:D2-003 — ef_construction < M Guard fehlt
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
#[derive(Debug)]
struct HnswNode {
    doc_id: DocId,
    vector: VectorData,
    connections: Vec<Vec<u32>>,
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
        // ANCHOR:ALG-FIX:D2-005 — total_cmp statt unwrap_or(Equal) für NaN-Safety
        // ANCHOR:ALG-FIX:D2-005 — total_cmp statt unwrap_or(Equal) für NaN-Safety
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
    ram_entry_point: RwLock<Option<usize>>,
    max_layer: AtomicU64,
    ml: f64,
    tx_buffer: TxBuffer<Vec<f32>>,
    deleted_nodes: RwLock<RoaringTreemap>,
    deleted_count: AtomicU64,
    rebuilding: AtomicBool,
    write_mutex: Mutex<()>,
    pub quantizer: RwLock<Option<crate::quantize::ScalarQuantizer>>,
    mmap_index: RwLock<Option<crate::persistence::MmapIndex>>,
    last_tx_id: AtomicU64,
}

impl HnswIndex {
    /// Creates a new HNSW index, validating configuration upfront.
    pub fn try_new(config: HnswConfig) -> Result<Self> {
        config.validate()?;
        let ml = 1.0 / (config.m as f64).ln();
        Ok(Self {
            inner: std::sync::Arc::new(HnswIndexCore {
                config,
                validation_error: None,
                nodes: RwLock::new(Vec::new()),
                doc_to_node: RwLock::new(AHashMap::new()),
                entry_point: RwLock::new(None),
                ram_entry_point: RwLock::new(None),
                max_layer: AtomicU64::new(0),

                ml,
                tx_buffer: TxBuffer::new_with_config(16, std::time::Duration::from_secs(60)),
                deleted_nodes: RwLock::new(RoaringTreemap::new()),
                deleted_count: AtomicU64::new(0),
                rebuilding: AtomicBool::new(false),
                write_mutex: Mutex::new(()),
                quantizer: RwLock::new(None),
                mmap_index: RwLock::new(None),
                last_tx_id: AtomicU64::new(0),
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
        Self {
            inner: std::sync::Arc::new(HnswIndexCore {
                config,
                validation_error,
                nodes: RwLock::new(Vec::new()),
                doc_to_node: RwLock::new(AHashMap::new()),
                entry_point: RwLock::new(None),
                ram_entry_point: RwLock::new(None),
                max_layer: AtomicU64::new(0),

                ml,
                tx_buffer: TxBuffer::new_with_config(16, std::time::Duration::from_secs(60)),
                deleted_nodes: RwLock::new(RoaringTreemap::new()),
                deleted_count: AtomicU64::new(0),
                rebuilding: AtomicBool::new(false),
                write_mutex: Mutex::new(()),
                quantizer: RwLock::new(None),
                mmap_index: RwLock::new(None),
                last_tx_id: AtomicU64::new(0),
            }),
        }
    }

    /// Returns all active (non-deleted) DocIds by reading the `doc_to_node` map directly.
    ///
    /// This is O(M) where M = number of mapped doc IDs, compared to `all_doc_ids()` which
    /// is O(N) where N = total node count (including mmap). Use this for repair/reconciliation
    /// where only the DocId set matters, not positional node data.
    ///
    /// # FIND-DB-004: HNSW Repair Acceleration
    pub fn all_doc_ids_from_map(&self) -> Vec<DocId> {
        let map = self.doc_to_node.read();
        let deleted = self.deleted_nodes.read();
        map.iter()
            .filter(|(&_doc_id_raw, &node_idx)| !deleted.contains(node_idx as u64))
            .map(|(&doc_id_raw, _)| DocId::new(doc_id_raw))
            .collect()
    }

    /// Triggers an async rebuild if the deletion threshold is exceeded.
    pub fn trigger_rebuild_async(&self) -> Option<tokio::task::JoinHandle<()>> {
        if self.is_rebuild_required() {
            let inner = std::sync::Arc::clone(&self.inner);
            Some(tokio::spawn(async move {
                if let Err(e) = inner.rebuild().await {
                    tracing::error!("Failed to rebuild HNSW index: {}", e);
                }
            }))
        } else {
            None
        }
    }

    /// Persists the index to a flat file.
    // ANCHOR:REFACTOR:WP-0.0-ASYNCIO — Fix blocking I/O in HnswIndex::save
    // TEST: grep "std::fs" crates/memfuse-index/src/hnsw.rs
    // DONE: Alle std::fs Aufrufe in save() sind in spawn_blocking gekapselt oder durch tokio::fs ersetzt.
    pub async fn save(&self, path: impl AsRef<std::path::Path>) -> Result<()> {
        let _lock = self.write_mutex.lock().await;
        let inner = std::sync::Arc::clone(&self.inner);
        let path_buf = path.as_ref().to_path_buf();

        tokio::task::spawn_blocking(move || {
            use std::io::{Seek, Write};

            let nodes = inner.nodes.read();
            let entry_point = inner.entry_point.read();
            let q_guard = inner.quantizer.read();

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
            let mut header = crate::persistence::HnswHeader {
                magic: crate::persistence::HNSW_MAGIC,
                version: crate::persistence::HNSW_VERSION,
                dimension: inner.config.dimension as u32,
                m: inner.config.m as u32,
                metric: inner.config.distance_metric as u8,
                quantized: if inner.config.quantize { 1 } else { 0 },
                q_min,
                q_max,
                node_count: node_count as u64,
                entry_point: entry_point.map(|i| i as i64).unwrap_or(-1),
                nodes_offset,
                connections_offset: 0,
                last_tx_id: inner.last_tx_id.load(Ordering::SeqCst),
            };

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
                node_records[i].max_layer = node.connections.len().saturating_sub(1) as u8;
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
            header.connections_offset = connections_offset;

            if connections_offset > current_pos {
                let padding = [0u8; 4];
                writer
                    .write_all(&padding[..(connections_offset - current_pos) as usize])
                    .map_err(|e| MemFuseError::Storage(e.to_string()))?;
            }

            let mut conn_pos = connections_offset;
            for (i, node) in nodes.iter().enumerate() {
                node_records[i].connections_offset = conn_pos;
                let num_layers = node.connections.len() as u8;
                writer
                    .write_all(&[num_layers])
                    .map_err(|e| MemFuseError::Storage(e.to_string()))?;
                conn_pos += 1;

                for layer in 0..num_layers as usize {
                    let conns = &node.connections[layer];
                    let len = conns.len() as u32;
                    writer
                        .write_all(&len.to_le_bytes())
                        .map_err(|e| MemFuseError::Storage(e.to_string()))?;
                    for &conn in conns {
                        writer
                            .write_all(&conn.to_le_bytes())
                            .map_err(|e| MemFuseError::Storage(e.to_string()))?;
                    }
                    conn_pos += 4 + (conns.len() * 4) as u64;
                }
            }
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
        let ep = if mmap_index.header.entry_point >= 0 {
            Some(mmap_index.header.entry_point as usize)
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
            let mut ep_guard = self.entry_point.write();
            *ep_guard = ep;
            *self.ram_entry_point.write() = None;
            self.max_layer.store(max_layer, Ordering::SeqCst);
        }

        if mmap_index.header.quantized != 0 {
            let dim = self.config.dimension;
            let q_min = mmap_index.header.q_min;
            let q_max = mmap_index.header.q_max;
            let range = if (q_max - q_min).abs() < f32::EPSILON {
                1e-6
            } else {
                q_max - q_min
            };
            let scale = 255.0 / range;
            let inv_scale = range / 255.0;
            let mut q_guard = self.quantizer.write();
            *q_guard = Some(crate::quantize::ScalarQuantizer {
                mins: vec![q_min; dim],
                maxes: vec![q_max; dim],
                scales: vec![scale; dim],
                inv_scales: vec![inv_scale; dim],
                dimension: dim,
                total_queries: AtomicU64::new(0),
                out_of_range_queries: AtomicU64::new(0),
            });
        }

        let mut guard = self.mmap_index.write();
        self.last_tx_id
            .store(mmap_index.header.last_tx_id, Ordering::SeqCst);
        *guard = Some(mmap_index);
        Ok(())
    }
}

/// Helper for hybrid resolution of nodes (RAM vs Mmap).
struct SearchContext<'a> {
    nodes: &'a [HnswNode],
    mmap: Option<&'a crate::persistence::MmapIndex>,
    mmap_node_count: usize,
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
        if self.rebuilding.load(Ordering::SeqCst) {
            RebuildStatus::Running
        } else {
            RebuildStatus::Idle
        }
    }

    /// Wartet bis ein laufender Rebuild abgeschlossen ist.
    /// Gibt `true` zurück wenn Rebuild abgeschlossen, `false` bei Timeout.
    pub async fn wait_for_rebuild_with_timeout(&self, timeout: std::time::Duration) -> bool {
        let deadline = tokio::time::Instant::now() + timeout;
        while self.rebuilding.load(Ordering::Acquire) {
            if tokio::time::Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        true
    }

    fn random_layer(&self) -> usize {
        let mut rng = rand::thread_rng();
        // ANCHOR:ALG-FIX:D2-002 — Guard gegen ln(0) = -∞ (INV-HNSW-2)
        // ANCHOR:ALG-FIX:D2-002 — Guard gegen ln(0) = -∞ (INV-HNSW-2)
        // rng.gen() gibt [0, 1) — bei r=0.0: ln(0)=-∞ → usize::MAX → OOM.
        // max(f64::EPSILON) verhindert diesen Grenzfall.
        let r: f32 = rng.gen::<f32>();
        let r_clamped = r.max(f32::EPSILON);
        let layer = (-(r_clamped.ln()) as f64 * self.ml) as usize;
        layer.min(32) // Hard-cap
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
    fn compute_distance_with_mmap(
        &self,
        query_exact: &[f32],
        query_quantized: Option<&[u8]>,
        mmap: &crate::persistence::MmapIndex,
        record: &crate::persistence::NodeRecord,
    ) -> Result<f32> {
        let vector_bytes = mmap.get_vector(record)?;
        if mmap.header.quantized != 0 {
            let guard = self.quantizer.read();
            let q = guard
                .as_ref()
                .ok_or_else(|| memfuse_core::MemFuseError::Index("Quantizer not trained".into()))?;
            if let Some(qq) = query_quantized {
                q.symmetric_dist(qq, vector_bytes, self.config.distance_metric)
            } else {
                q.asymmetric_dist(query_exact, vector_bytes, self.config.distance_metric)
            }
        } else {
            // Safe unaligned F32 read
            #[allow(unknown_lints)]
            #[allow(clippy::chunks_exact_to_as_chunks)]
            let v: Vec<f32> = vector_bytes
                .chunks_exact(4)
                .take(self.config.dimension)
                .map(|chunk| -> Result<f32> {
                    Ok(f32::from_le_bytes(chunk.try_into().map_err(|_| {
                        MemFuseError::Index("Corrupt f32 in mmap vector".into())
                    })?))
                })
                .collect::<Result<Vec<f32>>>()?;
            compute_distance(query_exact, &v, self.config.distance_metric)
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
            // ANCHOR:ALG-FIX:PANIC-001 — Mixed VectorData Guard (Zero-Panic Policy)
            // FUNDORT: memfuse-index/src/hnsw.rs
            _ => Err(MemFuseError::Index(
                "Mixed vector representations (F32/U8) are not supported".into(),
            )),
        }
    }

    fn resolve_dist(
        &self,
        idx: usize,
        query: &[f32],
        query_q: Option<&[u8]>,
        ctx: &SearchContext,
    ) -> Result<f32> {
        if let Some(mmap) = ctx.mmap {
            if idx < ctx.mmap_node_count {
                let record = mmap.get_node_record(idx)?;
                return self.compute_distance_with_mmap(query, query_q, mmap, &record);
            }
            let ram_idx = idx - ctx.mmap_node_count;
            return self.compute_distance_with_data(query, query_q, &ctx.nodes[ram_idx].vector);
        }
        self.compute_distance_with_data(query, query_q, &ctx.nodes[idx].vector)
    }

    fn resolve_connections<'a>(
        &self,
        idx: usize,
        layer: usize,
        ctx: &'a SearchContext,
    ) -> Result<Cow<'a, [u32]>> {
        if let Some(mmap) = ctx.mmap {
            if idx < ctx.mmap_node_count {
                let record = mmap.get_node_record(idx)?;
                return Ok(Cow::Owned(mmap.get_connections(&record, layer)?));
            }
            let ram_idx = idx - ctx.mmap_node_count;
            return Ok(Cow::Borrowed(
                ctx.nodes[ram_idx]
                    .connections
                    .get(layer)
                    .map(|v| v.as_slice())
                    .unwrap_or(&[]),
            ));
        }
        Ok(Cow::Borrowed(
            ctx.nodes[idx]
                .connections
                .get(layer)
                .map(|v| v.as_slice())
                .unwrap_or(&[]),
        ))
    }

    fn resolve_doc_id(&self, idx: usize, ctx: &SearchContext) -> Result<DocId> {
        if let Some(mmap) = ctx.mmap {
            if idx < ctx.mmap_node_count {
                let record = mmap.get_node_record(idx)?;
                return Ok(DocId::new(record.doc_id));
            }
            let ram_idx = idx - ctx.mmap_node_count;
            return Ok(ctx.nodes[ram_idx].doc_id);
        }
        Ok(ctx.nodes[idx].doc_id)
    }

    fn search_layer(
        &self,
        query: &[f32],
        query_quantized: Option<&[u8]>,
        entry_points: &[usize],
        ef: usize,
        layer: usize,
    ) -> Result<Vec<Candidate>> {
        let nodes_guard = self.nodes.read();
        let mmap_guard = self.mmap_index.read();
        let mmap_node_count = mmap_guard
            .as_ref()
            .map(|m| m.header.node_count as usize)
            .unwrap_or(0);

        let ctx = SearchContext {
            nodes: &nodes_guard,
            mmap: mmap_guard.as_ref(),
            mmap_node_count,
        };

        let mut visited = AHashSet::new();
        let mut candidates = BinaryHeap::new();
        let mut results = BinaryHeap::new();

        for &ep in entry_points {
            if visited.insert(ep) {
                let dist = self.resolve_dist(ep, query, query_quantized, &ctx)?;
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

            let connections = self.resolve_connections(current.index, layer, &ctx)?;
            for &neighbor_u32 in connections.iter() {
                let neighbor = neighbor_u32 as usize;
                if visited.insert(neighbor) {
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
        }
        let mut vec = results.into_vec();
        vec.sort_by(|a, b| a.distance.total_cmp(&b.distance));
        Ok(vec)
    }

    fn compute_symmetric_distance_hybrid(
        &self,
        idx_a: usize,
        idx_b: usize,
        ctx: &SearchContext,
    ) -> Result<f32> {
        let get_vector_data = |idx: usize| -> Result<VectorData> {
            if let Some(mmap) = ctx.mmap {
                if idx < ctx.mmap_node_count {
                    let record = mmap.get_node_record(idx)?;
                    let bytes = mmap.get_vector(&record)?;
                    return if mmap.header.quantized != 0 {
                        Ok(VectorData::U8(bytes.to_vec()))
                    } else {
                        let mut v = vec![0.0f32; self.config.dimension];
                        for i in 0..self.config.dimension {
                            v[i] =
                                f32::from_le_bytes(bytes[i * 4..(i + 1) * 4].try_into().map_err(
                                    |_| MemFuseError::Index("Corrupt f32 in mmap vector".into()),
                                )?);
                        }
                        Ok(VectorData::F32(v))
                    };
                }
                return Ok(ctx.nodes[idx - ctx.mmap_node_count].vector.clone());
            }
            Ok(ctx.nodes[idx].vector.clone())
        };

        let data_a = get_vector_data(idx_a)?;
        let data_b = get_vector_data(idx_b)?;
        self.compute_symmetric_distance(&data_a, &data_b)
    }

    fn select_neighbors_heuristic(
        &self,
        ctx: &SearchContext,
        candidates: &[Candidate],
        m: usize,
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
        let relaxation = if self.config.quantize {
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
                let dist_between =
                    self.compute_symmetric_distance_hybrid(closest.index, selected.index, ctx)?;

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

    fn do_insert(&self, id: DocId, vector: &[f32]) -> Result<()> {
        if vector.len() != self.config.dimension {
            return Err(MemFuseError::invalid_input(format!(
                "Dimension mismatch: expected {}, got {}",
                self.config.dimension,
                vector.len()
            )));
        }

        // ANCHOR:ALG-FIX:D2-004 — NaN/Inf-Validierung bei Insert (Distanzfunktion)
        // ANCHOR:ALG-FIX:D2-004 — NaN/Inf-Validierung bei Insert (Distanzfunktion)
        // NaN-Vektoren würden in BinaryHeap stille Korrumpierung verursachen.
        // Validierung an der Grenze (insert) statt in distance.rs — distance bleibt rein.
        if vector.iter().any(|x| x.is_nan() || x.is_infinite()) {
            return Err(MemFuseError::invalid_input(
                "Vector contains NaN or Infinity values",
            ));
        }

        let vector_data = if self.config.quantize {
            if let Some(mut q_guard) = self.quantizer.try_write() {
                if let Some(q) = q_guard.as_mut() {
                    q.expand_bounds_to_fit(vector);
                    VectorData::U8(q.quantize(vector))
                } else {
                    VectorData::F32(vector.to_vec())
                }
            } else if let Some(q) = self.quantizer.read().as_ref() {
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

        let mmap_node_count = self
            .mmap_index
            .read()
            .as_ref()
            .map(|m| m.header.node_count as usize)
            .unwrap_or(0);

        let new_idx = {
            let mut nodes = self.nodes.write();
            let idx = nodes.len();
            nodes.push(HnswNode {
                doc_id: id,
                vector: vector_data,
                connections: vec![vec![]; new_layer + 1],
                max_layer: new_layer,
                committed_tx: 0,
            });
            mmap_node_count + idx
        };

        self.doc_to_node.write().insert(id.inner(), new_idx);

        let query_quantized: Option<Vec<u8>> = None;

        let (_ep, final_connections) = {
            let nodes_read = self.nodes.read();
            let mmap_guard = self.mmap_index.read();
            let mmap_node_count = mmap_guard
                .as_ref()
                .map(|m| m.header.node_count as usize)
                .unwrap_or(0);
            let ctx = SearchContext {
                nodes: &nodes_read,
                mmap: mmap_guard.as_ref(),
                mmap_node_count,
            };

            // If no entry point, we already returned after setting it.
            // But we need to handle the case where it was set but we didn't capture ep_idx.
            let mut ep = Vec::new();
            if let Some(global_ep) = entry_point_opt {
                ep.push(global_ep);
            }
            if let Some(ram_ep) = *self.ram_entry_point.read() {
                if !ep.contains(&ram_ep) {
                    ep.push(ram_ep);
                }
            }
            if ep.is_empty() {
                ep.push(new_idx);
            }

            for layer in (new_layer + 1..=current_max_layer).rev() {
                let best = self.search_layer(vector, query_quantized.as_deref(), &ep, 1, layer)?;
                if let Some(closest) = best.first() {
                    ep = vec![closest.index];
                }
            }

            // Before layer 0 (and other layers <= new_layer)
            // We should ensure ep is fresh and includes ram_ep if we lost it during top-down
            // But search_layer will continue from whatever ep we have.

            let mut final_connections = vec![vec![]; new_layer + 1];

            for layer in (0..=new_layer.min(current_max_layer)).rev() {
                // For hybrid recall: re-add ram_ep at each layer if not present?
                // Actually, let's just make sure we have it at the start of layer-by-layer search.
                if let Some(ram_ep) = *self.ram_entry_point.read() {
                    if !ep.contains(&ram_ep) {
                        ep.push(ram_ep);
                    }
                }

                let neighbors = self.search_layer(
                    vector,
                    query_quantized.as_deref(),
                    &ep,
                    self.config.ef_construction,
                    layer,
                )?;
                let selected = self.select_neighbors_heuristic(&ctx, &neighbors, self.config.m)?;
                final_connections[layer] = selected;
                ep = neighbors.iter().map(|c| c.index).collect();
            }
            (ep, final_connections)
        };

        {
            let mut nodes = self.nodes.write();
            let mmap_guard = self.mmap_index.read();
            let mmap_node_count = mmap_guard
                .as_ref()
                .map(|m| m.header.node_count as usize)
                .unwrap_or(0);

            if let Some(new_node) = nodes.get_mut(new_idx - mmap_node_count) {
                new_node.connections = final_connections.clone();
            } else {
                return Err(MemFuseError::Index(format!(
                    "HNSW new node missing at index {}",
                    new_idx
                )));
            }

            for layer in (0..=new_layer.min(current_max_layer)).rev() {
                for &ni in &final_connections[layer] {
                    let neighbor_idx = ni as usize;

                    // SAFETY: Read-only mmap index.
                    // We can only back-link to nodes that are in RAM.
                    if neighbor_idx < mmap_node_count {
                        continue;
                    }

                    // Scope for neighbor modification to release mutable borrow
                    let (should_shrink, conn_indices) = {
                        let neighbor_node = nodes
                            .get_mut(neighbor_idx - mmap_node_count)
                            .ok_or_else(|| {
                                MemFuseError::Index(format!(
                                    "HNSW neighbor node missing at RAM index {} (global {})",
                                    neighbor_idx - mmap_node_count,
                                    neighbor_idx
                                ))
                            })?;
                        if let Some(conn_layer) = neighbor_node.connections.get_mut(layer) {
                            conn_layer.push(new_idx as u32);
                            if conn_layer.len() > self.config.m * 2 {
                                (true, conn_layer.clone())
                            } else {
                                (false, vec![])
                            }
                        } else {
                            (false, vec![])
                        }
                    };

                    if should_shrink {
                        let mut conn_cands = Vec::with_capacity(conn_indices.len());
                        for &idx_u32 in conn_indices.iter() {
                            let idx = idx_u32 as usize;
                            let dist = {
                                let ctx = SearchContext {
                                    nodes: &nodes,
                                    mmap: mmap_guard.as_ref(),
                                    mmap_node_count,
                                };
                                self.compute_symmetric_distance_hybrid(idx, neighbor_idx, &ctx)?
                            };
                            conn_cands.push(Candidate {
                                index: idx,
                                distance: dist,
                            });
                        }
                        let selected = {
                            let ctx = SearchContext {
                                nodes: &nodes,
                                mmap: mmap_guard.as_ref(),
                                mmap_node_count,
                            };
                            self.select_neighbors_heuristic(&ctx, &conn_cands, self.config.m * 2)?
                        };

                        if let Some(neighbor_node) = nodes.get_mut(neighbor_idx - mmap_node_count) {
                            if let Some(cl) = neighbor_node.connections.get_mut(layer) {
                                *cl = selected;
                            }
                        }
                    }
                }
            }
        }

        {
            let mut ram_ep = self.ram_entry_point.write();
            let mut ep_global = self.entry_point.write();
            if ep_global.is_none() || new_layer > current_max_layer {
                *ep_global = Some(new_idx);
                self.max_layer.store(new_layer as u64, Ordering::SeqCst);
            }
            // For hybrid recall: track the best RAM node
            if ram_ep.is_none() || new_layer >= (self.max_layer.load(Ordering::SeqCst) as usize) {
                *ram_ep = Some(new_idx);
            }
        }
        Ok(())
    }

    fn do_delete(&self, id: DocId) -> Result<()> {
        let node_idx = self.doc_to_node.write().remove(&id.inner());
        if let Some(idx) = node_idx {
            self.deleted_nodes.write().insert(idx as u64);
            self.deleted_count.fetch_add(1, Ordering::SeqCst);

            // ANCHOR:ALG-FIX:D2-001 — Entry-Point-Aktualisierung nach Delete (INV-HNSW-4)
            // ANCHOR:ALG-FIX:D2-001 — Entry-Point-Aktualisierung nach Delete (INV-HNSW-4)
            // Wenn der gelöschte Knoten der Entry-Point war, muss ein neuer
            // Entry-Point gefunden werden. Strategie: Nachbar auf höchstem Layer.
            let mut ep = self.entry_point.write();
            let mut ram_ep = self.ram_entry_point.write();

            if *ep == Some(idx) || *ram_ep == Some(idx) {
                let nodes = self.nodes.read();
                let mmap_guard = self.mmap_index.read();
                let mmap_node_count = mmap_guard
                    .as_ref()
                    .map(|m| m.header.node_count as usize)
                    .unwrap_or(0);
                let deleted = self.deleted_nodes.read();

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

                if *ep == Some(idx) {
                    *ep = best_node;
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
                        self.max_layer
                            .store(node_max_layer as u64, Ordering::SeqCst);
                    } else {
                        self.max_layer.store(0, Ordering::SeqCst);
                    }
                }

                if *ram_ep == Some(idx) {
                    *ram_ep = best_ram_node;
                }
            }
        }
        Ok(())
    }

    /// Graph connectivity score (1.0 = perfect, 0.0 = fully fragmented).
    pub fn connectivity_score(&self) -> f64 {
        let deleted = self.deleted_count.load(Ordering::SeqCst);
        let mmap_count = self
            .mmap_index
            .read()
            .as_ref()
            .map(|m| m.header.node_count as usize)
            .unwrap_or(0);
        let total = mmap_count + self.nodes.read().len();
        if total == 0 {
            return 1.0;
        }
        (1.0 - deleted as f64 / total as f64).max(0.0)
    }

    /// Returns Ok(()) if the index is healthy, or
    /// Err(MemFuseError::HnswConnectivityDegraded { deleted_ratio }) if degraded.
    ///
    /// Checks the current graph connectivity against the configured rebuild threshold
    /// (by default `1.0 - HNSW_REBUILD_THRESHOLD`, where `HNSW_REBUILD_THRESHOLD` = 30% deleted nodes).
    pub fn check_connectivity(&self) -> memfuse_core::Result<()> {
        let score = self.connectivity_score();
        if score < self.config.rebuild_threshold {
            let deleted_ratio = (1.0 - score) * 100.0;
            return Err(memfuse_core::MemFuseError::HnswConnectivityDegraded { deleted_ratio });
        }
        Ok(())
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

        // 1. Snapshot active nodes (RAM segment only)
        let (active_nodes, config) = {
            let nodes = self.nodes.read();
            let mmap_count = self
                .mmap_index
                .read()
                .as_ref()
                .map(|m| m.header.node_count as usize)
                .unwrap_or(0);
            let deleted_nodes = self.deleted_nodes.read();
            let mut active = Vec::with_capacity(nodes.len());
            for (i, node) in nodes.iter().enumerate() {
                let global_idx = mmap_count + i;
                if !deleted_nodes.contains(global_idx as u64) {
                    active.push((node.doc_id, node.vector.clone(), node.committed_tx));
                }
            }
            (active, self.config.clone())
        };

        // 2. Build fresh index (this will be the NEW RAM segment)
        let new_index = HnswIndex::try_new(config)?;

        // Ensure new_index knows about the Mmap segment to link against it
        {
            let mmap_guard = self.mmap_index.read();
            if let Some(mmap) = mmap_guard.as_ref() {
                new_index.load_mmap_from_instance(mmap.clone())?;
            }
        }

        let quantizer_guard = self.quantizer.read();
        if let Some(old_q) = quantizer_guard.as_ref() {
            // Train a new quantizer on a sample of active nodes to prevent clamping loss
            let sample_size = self
                .config
                .quantizer_recalibration_sample_size
                .min(active_nodes.len());
            let mut train_data = Vec::with_capacity(sample_size);

            for (_, vector, _) in active_nodes.iter().take(sample_size) {
                match vector {
                    VectorData::F32(v) => train_data.push(v.clone()),
                    VectorData::U8(v) => train_data.push(old_q.dequantize(v)),
                }
            }

            if !train_data.is_empty() {
                let training_refs: Vec<&[f32]> = train_data.iter().map(|v| v.as_slice()).collect();
                let new_q =
                    crate::quantize::ScalarQuantizer::train(&training_refs, self.config.dimension);
                *new_index.quantizer.write() = Some(new_q);
            } else {
                *new_index.quantizer.write() = Some(old_q.clone());
            }
        }

        for (doc_id, vector, committed_tx) in active_nodes {
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
            let mmap_count = new_index
                .mmap_index
                .read()
                .as_ref()
                .map(|m| m.header.node_count as usize)
                .unwrap_or(0);
            if let Some(&global_idx) = new_index.doc_to_node.read().get(&doc_id.inner()) {
                if global_idx >= mmap_count {
                    let ram_idx = global_idx - mmap_count;
                    let mut nodes = new_index.nodes.write();
                    if let Some(node) = nodes.get_mut(ram_idx) {
                        node.committed_tx = committed_tx;
                    }
                }
            }
        }

        // 4. Atomic swap
        {
            let mut nodes = self.nodes.write();
            let mut doc_to_node = self.doc_to_node.write();
            let mut entry_point = self.entry_point.write();
            let mut ram_entry_point = self.ram_entry_point.write();
            let mut deleted_nodes = self.deleted_nodes.write();

            let new_nodes = std::mem::take(&mut *new_index.nodes.write());
            let new_doc_to_node = std::mem::take(&mut *new_index.doc_to_node.write());
            let new_entry_point = *new_index.entry_point.read();
            let new_ram_entry_point = *new_index.ram_entry_point.read();

            *nodes = new_nodes;
            *doc_to_node = new_doc_to_node;
            *entry_point = new_entry_point;
            *ram_entry_point = new_ram_entry_point;
            self.max_layer
                .store(new_index.max_layer.load(Ordering::SeqCst), Ordering::SeqCst);

            // Preserve mmap deletions, clear RAM deletions (since they are now in doc_to_node/nodes)
            let mmap_count = self
                .mmap_index
                .read()
                .as_ref()
                .map(|m| m.header.node_count as usize)
                .unwrap_or(0);
            let mut new_deleted = RoaringTreemap::new();
            for del_idx in deleted_nodes.iter() {
                if (del_idx as usize) < mmap_count {
                    new_deleted.insert(del_idx);
                }
            }
            *deleted_nodes = new_deleted;
            self.deleted_count
                .store(deleted_nodes.len(), Ordering::SeqCst);
        }

        self.rebuilding.store(false, Ordering::SeqCst);
        tracing::info!("HNSW rebuild completed in {:?}", start_time.elapsed());
        Ok(())
    }

    // Kept for backward compatibility or direct calls if needed, though facade should use `HnswIndex` wrapper
}

#[async_trait::async_trait]
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

        // FIND-IDX-002: NaN/Inf Poisoning prevention
        for &val in embedding {
            if val.is_nan() || val.is_infinite() {
                return Err(MemFuseError::Storage(
                    "Invalid vector: NaN or Infinity detected".to_string(),
                ));
            }
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

    // CONSTRAINT: HNSW Search Hotspot (Optimiert)
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

        let query_quantized: Option<Vec<u8>> = None;

        let mmap_guard = self.mmap_index.read();
        let mmap_node_count = mmap_guard
            .as_ref()
            .map(|m| m.header.node_count as usize)
            .unwrap_or(0);

        let mut ep = Vec::new();
        if let Some(global_ep) = *self.entry_point.read() {
            ep.push(global_ep);
        }
        if let Some(ram_ep) = *self.ram_entry_point.read() {
            if !ep.contains(&ram_ep) {
                ep.push(ram_ep);
            }
        }

        if ep.is_empty() {
            return Ok(Vec::new());
        }

        let max_layer = self.max_layer.load(Ordering::SeqCst) as usize;

        for layer in (1..=max_layer).rev() {
            let layer_ef = 1;
            let best =
                self.search_layer(query, query_quantized.as_deref(), &ep, layer_ef, layer)?;
            if let Some(closest) = best.first() {
                ep = vec![closest.index];
            }
        }

        // Add RAM entry point back for the final layer search to ensure hybrid recall
        if let Some(ram_ep) = *self.ram_entry_point.read() {
            if !ep.contains(&ram_ep) {
                ep.push(ram_ep);
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
            let deleted_ratio = (1.0 - score) * 100.0;
            let err = memfuse_core::MemFuseError::HnswConnectivityDegraded { deleted_ratio };
            tracing::warn!(
                error = %err,
                connectivity_score = score,
                rebuild_threshold = self.config.rebuild_threshold,
                "HNSW index degraded — consider calling rebuild()"
            );
        }

        let nodes = self.nodes.read();
        let deleted = self.deleted_nodes.read();
        let mut results = Vec::with_capacity(k);

        let ctx = SearchContext {
            nodes: &nodes,
            mmap: mmap_guard.as_ref(),
            mmap_node_count,
        };

        for c in candidates.iter() {
            if deleted.contains(c.index as u64) {
                continue;
            }
            let doc_id = self.resolve_doc_id(c.index, &ctx)?;

            // Phase 2: Exact Reranking (Asymmetric for SQ8)
            let final_dist = if self.config.quantize {
                self.resolve_dist(c.index, query, None, &ctx)?
            } else {
                c.distance
            };

            let score = match self.config.distance_metric {
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

        let mut ep = Vec::new();
        if let Some(global_ep) = *self.entry_point.read() {
            ep.push(global_ep);
        }
        if let Some(ram_ep) = *self.ram_entry_point.read() {
            if !ep.contains(&ram_ep) {
                ep.push(ram_ep);
            }
        }

        if ep.is_empty() {
            return Ok(Vec::new());
        }

        let max_layer = self.max_layer.load(Ordering::SeqCst) as usize;

        for layer in (1..=max_layer).rev() {
            let best = self.search_layer(query, query_quantized.as_deref(), &ep, 1, layer)?;
            if let Some(closest) = best.first() {
                ep = vec![closest.index];
            }
        }

        // Add RAM entry point back for the final layer search to ensure hybrid recall
        if let Some(ram_ep) = *self.ram_entry_point.read() {
            if !ep.contains(&ram_ep) {
                ep.push(ram_ep);
            }
        }

        // Over-fetch to compensate for filtered-out results and reranking
        let factor = if self.config.quantize { 4 } else { 2 };
        let ef = self.config.ef_search.max(k) * factor;
        let candidates = self.search_layer(query, query_quantized.as_deref(), &ep, ef, 0)?;

        let score = self.connectivity_score();
        if score < self.config.rebuild_threshold {
            let deleted_ratio = (1.0 - score) * 100.0;
            let err = memfuse_core::MemFuseError::HnswConnectivityDegraded { deleted_ratio };
            tracing::warn!(
                error = %err,
                connectivity_score = score,
                rebuild_threshold = self.config.rebuild_threshold,
                "HNSW index degraded — consider calling rebuild()"
            );
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
                other => {
                    return Err(MemFuseError::Index(format!(
                        "Unsupported DistanceMetric variant in search_filtered(): {other:?}"
                    )));
                }
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
                    let mmap_count = self
                        .mmap_index
                        .read()
                        .as_ref()
                        .map(|m| m.header.node_count as usize)
                        .unwrap_or(0);
                    if let Some(&global_idx) = self.doc_to_node.read().get(&doc_id.inner()) {
                        if global_idx >= mmap_count {
                            let ram_idx = global_idx - mmap_count;
                            let mut nodes = self.nodes.write();
                            if let Some(node) = nodes.get_mut(ram_idx) {
                                node.committed_tx = tx.inner();
                            }
                        }
                    }
                }
                IndexOp::Delete { doc_id, .. } => {
                    self.do_delete(doc_id)?;
                    deleted_any = true;
                }
                // AI-TAG[PANIC-SAFETY][CRITICAL] [RESOLVED] — IndexOp ist #[non_exhaustive]; neue Varianten
                // müssen hier explizit behandelt werden, bevor sie in den HNSW-Commit-Pfad gelangen.
                // ANWEISUNG: Neue IndexOp-Variante → Arm hier hinzufügen oder UpdateNotSupported zurückgeben.
                // ID: FIX-03-INDEXOP
                other => {
                    return Err(MemFuseError::Index(format!(
                        "HNSW commit received unsupported IndexOp variant: {:?}. \
                         Add a handler arm before enabling this operation.",
                        std::mem::discriminant(&other)
                    )));
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

        self.last_tx_id.store(tx.inner(), Ordering::SeqCst);
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

    async fn rollback_to_tx(&self, tx_id: TxId) -> Result<()> {
        if let Some(ref err) = self.validation_error {
            return Err(MemFuseError::invalid_input(format!(
                "Invalid index configuration: {}",
                err
            )));
        }

        let target = tx_id.inner();

        // Unter write_mutex um Konkurrenz mit laufenden Inserts zu verhindern
        let _guard = self.write_mutex.lock().await;

        // 1. Sammle alle Nodes mit committed_tx > target_tx_id
        let indices_to_remove: Vec<usize> = {
            let nodes = self.nodes.read();
            nodes
                .iter()
                .enumerate()
                .filter(|(_, node)| node.committed_tx > target && node.committed_tx != 0)
                .map(|(i, _)| i)
                .collect()
        };

        if indices_to_remove.is_empty() {
            self.last_tx_id.store(target, Ordering::SeqCst);
            return Ok(());
        }

        // 2. Aus doc_to_node-Map entfernen
        {
            let nodes = self.nodes.read();
            let mut map = self.doc_to_node.write();
            for &idx in &indices_to_remove {
                if let Some(node) = nodes.get(idx) {
                    map.remove(&node.doc_id.inner());
                }
            }
        }

        // 3. Als deleted markieren (Soft-Delete — kein Rebuild nötig)
        {
            let mmap_count = self
                .mmap_index
                .read()
                .as_ref()
                .map(|m| m.header.node_count as usize)
                .unwrap_or(0);
            let mut deleted = self.deleted_nodes.write();
            for &idx in &indices_to_remove {
                deleted.insert((mmap_count + idx) as u64);
            }
            self.deleted_count
                .fetch_add(indices_to_remove.len() as u64, Ordering::SeqCst);
        }

        // 4. TxBuffer bereinigen
        self.tx_buffer.discard(tx_id);

        // 5. last_tx_id zurücksetzen
        self.last_tx_id.store(target, Ordering::SeqCst);

        tracing::info!(
            removed = indices_to_remove.len(),
            rollback_target = target,
            "HNSW physical rollback completed"
        );

        Ok(())
    }

    async fn last_tx_id(&self) -> Result<u64> {
        Ok(self.last_tx_id.load(Ordering::SeqCst))
    }

    async fn all_doc_ids(&self) -> Result<Vec<DocId>> {
        if self.validation_error.is_some() {
            return Ok(Vec::new());
        }
        let nodes = self.nodes.read();
        let mmap_guard = self.mmap_index.read();
        let mmap_node_count = mmap_guard
            .as_ref()
            .map(|m| m.header.node_count as usize)
            .unwrap_or(0);
        let deleted = self.deleted_nodes.read();

        let ctx = SearchContext {
            nodes: &nodes,
            mmap: mmap_guard.as_ref(),
            mmap_node_count,
        };

        let total_nodes = mmap_node_count + nodes.len();
        let mut ids = Vec::with_capacity(total_nodes.saturating_sub(deleted.len() as usize));

        for i in 0..total_nodes {
            if !deleted.contains(i as u64) {
                ids.push(self.resolve_doc_id(i, &ctx)?);
            }
        }
        Ok(ids)
    }

    async fn len(&self) -> usize {
        if self.validation_error.is_some() {
            return 0;
        }
        let mmap_count = self
            .mmap_index
            .read()
            .as_ref()
            .map(|m| m.header.node_count as usize)
            .unwrap_or(0);
        let total = mmap_count + self.nodes.read().len();
        let deleted = self.deleted_count.load(Ordering::SeqCst) as usize;
        total.saturating_sub(deleted)
    }
    async fn stats(&self) -> Result<VectorIndexStats> {
        let nodes = self.nodes.read();
        let mmap_guard = self.mmap_index.read();
        let mmap_count = mmap_guard
            .as_ref()
            .map(|m| m.header.node_count as usize)
            .unwrap_or(0);

        let deleted_count = self.deleted_count.load(Ordering::SeqCst) as usize;
        let num_vectors = (mmap_count + nodes.len()).saturating_sub(deleted_count);

        let mut vector_memory: usize = nodes
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
                    .map(|c| c.len() * std::mem::size_of::<u32>())
                    .sum::<usize>()
            })
            .sum();

        if let Some(mmap) = mmap_guard.as_ref() {
            vector_memory += mmap.mmap.len(); // Simple approximation: entire mmap file
        }

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
        let err_msg = format!("{}", result.err().unwrap());
        assert!(
            err_msg.contains("ef_construction (1) must be >= m (100)"),
            "Unexpected error message: {}",
            err_msg
        );
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
        let result = index.insert(tx, DocId::new(1), &[1.0, 0.0, 0.0, 0.0]).await;
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("Invalid index configuration"));
        assert!(err_msg.contains("ef_construction (5) must be >= m (10)"));
    }

    #[tokio::test]
    async fn test_insert_and_search() {
        let index = HnswIndex::try_new(test_config(4)).unwrap();
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
        let index = HnswIndex::try_new(test_config(4)).unwrap();

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
    async fn test_entry_point_deletion_search() {
        let index = HnswIndex::try_new(test_config(4)).unwrap();
        let tx1 = TxId::new(1);

        // Insert 5 nodes. First node (DocId(0)) will be the initial entry point.
        for i in 0u64..5 {
            let v = vec![i as f32, 0.0, 0.0, 0.0];
            index.insert(tx1, DocId::new(i), &v).await.expect("insert");
        }
        index.commit(tx1).await.expect("commit");

        // Delete node 0 (the entry point)
        let tx2 = TxId::new(2);
        index.delete(tx2, DocId::new(0)).await.expect("delete");
        index.commit(tx2).await.expect("commit");

        // Search must successfully return results from remaining nodes without panicking
        let results = index
            .search(&[1.0, 0.0, 0.0, 0.0], 3)
            .await
            .expect("search should succeed after entry point deletion");

        assert_eq!(results.len(), 3);
        for res in &results {
            assert_ne!(
                res.doc_id,
                DocId::new(0),
                "Deleted entry point node 0 must not be returned"
            );
        }
    }

    #[tokio::test]
    async fn test_rollback() {
        let index = HnswIndex::try_new(test_config(4)).unwrap();

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
        let index = HnswIndex::try_new(test_config(4)).unwrap();
        let results = index
            .search(&[1.0, 0.0, 0.0, 0.0], 5)
            .await
            .expect("search");
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_dimension_mismatch() {
        let index = HnswIndex::try_new(test_config(4)).unwrap();
        let tx = TxId::new(1);
        let result = index.insert(tx, DocId::new(1), &[1.0, 0.0]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_filtered_search() {
        let index = HnswIndex::try_new(test_config(4)).unwrap();
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
        let index = HnswIndex::try_new(HnswConfig {
            dimension: 2,
            rebuild_threshold: 0.8,
            distance_metric: DistanceMetric::Euclidean,
            ..test_config(2)
        })
        .unwrap();
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
        let index = HnswIndex::try_new(HnswConfig {
            dimension: 4,
            quantize: true,
            rebuild_threshold: 0.1, // Trigger easily
            distance_metric: DistanceMetric::Euclidean,
            ..test_config(4)
        })
        .unwrap();
        let tx = TxId::new(1);

        // Insert enough vectors to train quantizer (>= 50)
        for i in 1..=60u64 {
            let v = [i as f32, i as f32 * 0.1, 0.0, 0.0];
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
            .search(&[60.0, 6.0, 0.0, 0.0], 1)
            .await
            .expect("search");
        assert_eq!(results[0].doc_id, DocId::new(60));
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
            let index = HnswIndex::try_new(config).unwrap();
            let result = index.do_insert(DocId::new(1), &v);

            proptest::prop_assert!(result.is_err(), "Inserting vector containing NaN must return error");
        }
    }

    #[tokio::test]
    async fn test_hnsw_persistence_lifecycle() {
        let temp_dir = tempfile::tempdir().unwrap();
        let index_path = temp_dir.path().join("test.hnsw");

        let config = HnswConfig {
            dimension: 4,
            m: 16,
            ef_construction: 40,
            quantize: false,
            ..test_config(4)
        };
        let index = HnswIndex::try_new(config.clone()).unwrap();
        let tx1 = TxId::new(1);

        // 1. Initial Insert (RAM)
        for i in 1..=50u64 {
            let v = [i as f32, i as f32 * 0.1, 0.0, 0.0];
            index.insert(tx1, DocId::new(i), &v).await.expect("test");
        }
        index.commit(tx1).await.expect("test");

        // 2. Save to disk
        index.save(&index_path).await.expect("save");

        // 3. Clear RAM and load via Mmap
        let index_mmap = HnswIndex::try_new(config.clone()).unwrap();
        index_mmap.load_mmap(&index_path).await.expect("load mmap");

        assert_eq!(index_mmap.len().await, 50);

        // 4. Verify Search on Mmap
        let results = index_mmap
            .search(&[25.0, 2.5, 0.0, 0.0], 1)
            .await
            .expect("search");
        assert_eq!(results[0].doc_id, DocId::new(25));

        // 5. Insert new nodes on top of Mmap (Hybrid)
        let tx2 = TxId::new(2);
        for i in 51..=60u64 {
            let v = [i as f32, i as f32 * 0.1, 0.0, 0.0];
            index_mmap
                .insert(tx2, DocId::new(i), &v)
                .await
                .expect("test");
        }
        index_mmap.commit(tx2).await.expect("test");

        assert_eq!(index_mmap.len().await, 60);

        // 6. Verify Hybrid Search (finding a RAM node)
        let results_hybrid = index_mmap
            .search(&[58.0, 5.8, 0.0, 0.0], 1)
            .await
            .expect("search");
        assert_eq!(results_hybrid[0].doc_id, DocId::new(58));

        // 7. Verify Hybrid Search (finding an Mmap node)
        let results_mmap = index_mmap
            .search(&[5.0, 0.5, 0.0, 0.0], 1)
            .await
            .expect("search");
        assert_eq!(results_mmap[0].doc_id, DocId::new(5));
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
        let index = HnswIndex::try_new(config).unwrap();
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
                .insert(tx, DocId::new(i as u64), v)
                .await
                .expect("insert");
        }
        index.commit(tx).await.expect("commit");

        // 2. Perform searches and calculate recall
        let mut hits = 0;
        let test_queries = 20;
        for i in 0..test_queries {
            let query = &data[i * 5];
            let results = index.search(query, 1).await.expect("search");
            if !results.is_empty() && results[0].doc_id == DocId::new((i * 5) as u64) {
                hits += 1;
            }
        }

        let recall = hits as f32 / test_queries as f32;
        tracing::info!("SQ8 Recall: {}", recall);
        assert!(recall >= 0.9, "Recall too low for SQ8: {}", recall);
    }

    #[tokio::test]
    async fn test_all_doc_ids() {
        let index = HnswIndex::try_new(test_config(4)).unwrap();
        let tx = TxId::new(1);

        for i in 1..=10u64 {
            index
                .insert(tx, DocId::new(i), &[i as f32, 0.0, 0.0, 0.0])
                .await
                .unwrap();
        }
        index.commit(tx).await.unwrap();

        let ids = index.all_doc_ids().await.unwrap();
        assert_eq!(ids.len(), 10);
        for i in 1..=10u64 {
            assert!(ids.contains(&DocId::new(i)));
        }

        // Delete some
        let tx2 = TxId::new(2);
        index.delete(tx2, DocId::new(5)).await.unwrap();
        index.delete(tx2, DocId::new(8)).await.unwrap();
        index.commit(tx2).await.unwrap();

        let ids2 = index.all_doc_ids().await.unwrap();
        assert_eq!(ids2.len(), 8);
        assert!(!ids2.contains(&DocId::new(5)));
        assert!(!ids2.contains(&DocId::new(8)));
    }

    #[tokio::test]
    async fn test_check_connectivity_returns_error_when_degraded() {
        // Build a small index, delete enough nodes to cross the rebuild threshold.
        let config = HnswConfig {
            rebuild_threshold: 0.8, // trigger when >20% deleted
            ..test_config(4)
        };
        let index = HnswIndex::try_new(config).unwrap();
        let tx = TxId::new(1);

        for i in 0u64..5 {
            let v = vec![i as f32, 0.0, 0.0, 0.0];
            index.insert(tx, DocId::new(i), &v).await.unwrap();
        }
        index.commit(tx).await.unwrap();

        // Delete 2 out of 5 → 40% deleted → score = 0.6 < threshold 0.8
        let tx2 = TxId::new(2);
        index.delete(tx2, DocId::new(0)).await.unwrap(); // #[test]
        index.delete(tx2, DocId::new(1)).await.unwrap();
        index.commit(tx2).await.unwrap();

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
        let index = HnswIndex::try_new(test_config(4)).unwrap();
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
        let idx = HnswIndex::try_new(config).unwrap();
        let tx = TxId::new(1);

        // Insert 100 vectors
        for i in 0u64..100 {
            let v = vec![i as f32, 1.0, 0.0, 0.0];
            idx.insert(tx, DocId::new(i), &v).await.unwrap();
        }
        idx.commit(tx).await.unwrap();

        // Delete 51 vectors -> >50% deleted, crossing 0.5 threshold
        let tx2 = TxId::new(2);
        for i in 0u64..51 {
            idx.delete(tx2, DocId::new(i)).await.unwrap();
        }
        idx.commit(tx2).await.unwrap();

        // Wait briefly for background rebuild task to complete
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Verify search() still works and returns non-deleted nodes
        let query = vec![75.0, 1.0, 0.0, 0.0];
        let results = idx.search(&query, 5).await.unwrap();
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
}
