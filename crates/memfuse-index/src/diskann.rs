// FILE-CONTEXT
// ZWECK: DiskANN-Graphindex für Out-of-Core Approximate Nearest Neighbor Search (WP-4.3).
// INVARIANTEN: Lock-Hierarchie: header -> mmap -> cache / quantizer / doc_ids; atomic rename + parent dir sync bei file persistence.
// NICHT-OFFENSICHTLICH: Mmap für Vektor- & Graphlesezugriffe, unsafe Block benötigt 4-Punkt SAFETY-Kommentar.
// HOTSPOTS: diskann.rs (DiskAnnIndex::search_internal, write_to_file, load_node)
// STAND: TS:2026-08-30T18:53:53Z (SESSION: 37b1d991)

//! DiskANN Out-of-Core Vector Search (WP-4.3).

#![doc(hidden)]

use crate::distance::compute_distance;
use ahash::AHashMap;
use memfuse_core::{
    DistanceMetric, DocId, MemFuseError, Result, ScoredDocument, TxId, VectorIndex,
    VectorIndexStats,
};
use memmap2::Mmap;
use parking_lot::RwLock;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

const DISKANN_MAGIC: &[u8; 4] = b"DANN";
const DISKANN_VERSION: u16 = 1;
/// Pending-Threshold: nach 1000 pending inserts → auto-trigger persist_delta.
const PENDING_FLUSH_THRESHOLD: u64 = 1000;

/// Header for DiskANN index file.
#[derive(Debug, Clone, Copy)]
struct DiskAnnHeader {
    magic: [u8; 4],
    version: u16,
    node_count: u64,
    dimension: u32,
    max_degree: u32,
    sector_size: u32,
    entry_point: u32,
    metric: u8,
    quantized: u8,
    q_min: f32,
    q_max: f32,
}

impl DiskAnnHeader {
    const SIZE: usize = 40;

    fn to_bytes(self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        buf[0..4].copy_from_slice(&self.magic);
        buf[4..6].copy_from_slice(&self.version.to_le_bytes());
        buf[6..14].copy_from_slice(&self.node_count.to_le_bytes());
        buf[14..18].copy_from_slice(&self.dimension.to_le_bytes());
        buf[18..22].copy_from_slice(&self.max_degree.to_le_bytes());
        buf[22..26].copy_from_slice(&self.sector_size.to_le_bytes());
        buf[26..30].copy_from_slice(&self.entry_point.to_le_bytes());
        buf[30] = self.metric;
        buf[31] = self.quantized;
        buf[32..36].copy_from_slice(&self.q_min.to_le_bytes());
        buf[36..40].copy_from_slice(&self.q_max.to_le_bytes());
        buf
    }

    fn try_from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < Self::SIZE {
            return Err(MemFuseError::Index("Header too small".into()));
        }
        let magic_bytes = bytes
            .get(0..4)
            .ok_or_else(|| MemFuseError::Storage("DiskANN header magic out of bounds".into()))?;
        if magic_bytes != DISKANN_MAGIC {
            return Err(MemFuseError::Storage(
                "Invalid DiskANN file: bad magic".into(),
            ));
        }
        let version = u16::from_le_bytes(
            bytes
                .get(4..6)
                .ok_or_else(|| MemFuseError::Index("Invalid version offset".into()))?
                .try_into()
                .map_err(|_| MemFuseError::Index("Invalid version".into()))?,
        );
        if version != DISKANN_VERSION {
            return Err(MemFuseError::Storage(format!(
                "DiskANN version mismatch: expected {}, got {}",
                DISKANN_VERSION, version
            )));
        }
        Ok(Self {
            magic: *DISKANN_MAGIC,
            version,
            node_count: u64::from_le_bytes(
                bytes
                    .get(6..14)
                    .ok_or_else(|| MemFuseError::Index("Invalid node_count offset".into()))?
                    .try_into()
                    .map_err(|_| MemFuseError::Index("Invalid node_count".into()))?,
            ),
            dimension: u32::from_le_bytes(
                bytes
                    .get(14..18)
                    .ok_or_else(|| MemFuseError::Index("Invalid dimension offset".into()))?
                    .try_into()
                    .map_err(|_| MemFuseError::Index("Invalid dimension".into()))?,
            ),
            max_degree: u32::from_le_bytes(
                bytes
                    .get(18..22)
                    .ok_or_else(|| MemFuseError::Index("Invalid max_degree offset".into()))?
                    .try_into()
                    .map_err(|_| MemFuseError::Index("Invalid max_degree".into()))?,
            ),
            sector_size: u32::from_le_bytes(
                bytes
                    .get(22..26)
                    .ok_or_else(|| MemFuseError::Index("Invalid sector_size offset".into()))?
                    .try_into()
                    .map_err(|_| MemFuseError::Index("Invalid sector_size".into()))?,
            ),
            entry_point: u32::from_le_bytes(
                bytes
                    .get(26..30)
                    .ok_or_else(|| MemFuseError::Index("Invalid entry_point offset".into()))?
                    .try_into()
                    .map_err(|_| MemFuseError::Index("Invalid entry_point".into()))?,
            ),
            metric: *bytes
                .get(30)
                .ok_or_else(|| MemFuseError::Index("Invalid metric offset".into()))?,
            quantized: *bytes
                .get(31)
                .ok_or_else(|| MemFuseError::Index("Invalid quantized offset".into()))?,
            q_min: f32::from_le_bytes(
                bytes
                    .get(32..36)
                    .ok_or_else(|| MemFuseError::Index("Invalid q_min offset".into()))?
                    .try_into()
                    .map_err(|_| MemFuseError::Index("Invalid q_min".into()))?,
            ),
            q_max: f32::from_le_bytes(
                bytes
                    .get(36..40)
                    .ok_or_else(|| MemFuseError::Index("Invalid q_max offset".into()))?
                    .try_into()
                    .map_err(|_| MemFuseError::Index("Invalid q_max".into()))?,
            ),
        })
    }
}

/// Configuration for DiskANN index.
#[derive(Debug, Clone)]
pub struct DiskAnnConfig {
    /// Path to the on-disk index file.
    pub index_path: PathBuf,
    /// Vector dimension.
    pub dimension: usize,
    /// Maximum graph degree (R).
    pub max_degree: usize,
    /// Beam width for search (W).
    pub beam_width: usize,
    /// Sector size for aligned I/O (typically 4096).
    pub sector_size: usize,
    /// Maximum memory budget in bytes for in-memory caching.
    pub memory_budget: usize,
    /// Distance metric.
    pub distance_metric: DistanceMetric,
    /// Whether to use SQ8 quantization.
    pub quantize: bool,
}

impl Default for DiskAnnConfig {
    fn default() -> Self {
        Self {
            index_path: PathBuf::from("diskann.idx"),
            dimension: 128,
            max_degree: 64,
            beam_width: 8,
            sector_size: 4096,
            memory_budget: 128 * 1024 * 1024, // 128MB
            distance_metric: DistanceMetric::Cosine,
            quantize: false,
        }
    }
}

/// A node in the DiskANN graph (Cached).
#[derive(Debug, Clone)]
struct CachedNode {
    vector: VectorData,
    neighbors: Vec<u32>,
    doc_id: DocId,
}

#[derive(Debug, Clone)]
enum VectorData {
    F32(Vec<f32>),
    U8(Vec<u8>),
}

/// DiskANN out-of-core vector index.
///
/// This index uses `memmap2` to offload vector data and graph edges to disk.
/// Note that mmap operations and page faults can cause the current thread to block
/// while data is being loaded from the disk. For high-concurrency environments,
/// consider using `tokio::task::spawn_blocking` when calling methods on this index
/// if latency spikes are a concern.
pub struct DiskAnnIndex {
    inner: Arc<DiskAnnIndexInner>,
}

struct DiskAnnIndexInner {
    config: DiskAnnConfig,
    header: RwLock<Option<DiskAnnHeader>>,
    mmap: RwLock<Option<Mmap>>,
    node_size_bytes: AtomicU64,
    cache: RwLock<AHashMap<u32, CachedNode>>,
    doc_ids: RwLock<Vec<DocId>>,
    quantizer: RwLock<Option<crate::quantize::ScalarQuantizer>>,
    drift_warn_count: AtomicU64,
    /// Inkrementelle Einfügungen vor dem nächsten persist_delta().
    /// Geschützt durch RwLock — Hot-Path schreibt, persist_delta liest+leert.
    pending_inserts: RwLock<Vec<(DocId, Vec<f32>)>>,
    /// Monotoner Zähler (AtomicU64 für Threshold-Check ohne Lock).
    pending_count: AtomicU64,
}

impl DiskAnnIndex {
    /// Creates a new DiskANN index instance.
    pub fn try_new(config: DiskAnnConfig) -> Result<Self> {
        if !config.sector_size.is_power_of_two() {
            return Err(MemFuseError::InvalidInput(
                "Sector size must be a power of 2".to_string(),
            ));
        }

        let vector_size = if config.quantize {
            config.dimension
        } else {
            config.dimension * 4
        };
        let neighbors_size = 4 + (config.max_degree * 4);
        let doc_id_size = 8;
        let raw_node_size = vector_size + neighbors_size + doc_id_size;
        let node_size_bytes = raw_node_size.div_ceil(config.sector_size) * config.sector_size;

        Ok(Self {
            inner: Arc::new(DiskAnnIndexInner {
                config,
                header: RwLock::new(None),
                mmap: RwLock::new(None),
                node_size_bytes: AtomicU64::new(node_size_bytes as u64),
                cache: RwLock::new(AHashMap::new()),
                doc_ids: RwLock::new(Vec::new()),
                quantizer: RwLock::new(None),
                drift_warn_count: AtomicU64::new(0),
                pending_inserts: RwLock::new(Vec::new()),
                pending_count: AtomicU64::new(0),
            }),
        })
    }

    fn check_quantizer_drift(&self, vector: &[f32]) {
        let mut q_guard = self.inner.quantizer.write();
        if let Some(ref mut q) = *q_guard {
            let drift = q.check_drift(vector);
            if drift > 0.10 {
                use std::sync::atomic::Ordering;
                let count = self.inner.drift_warn_count.fetch_add(1, Ordering::Relaxed) + 1;
                tracing::warn!(
                    drift = %format!("{:.1}%", drift * 100.0),
                    warn_count = count,
                    "Quantization drift > 10% detected — ScalarQuantizer recalibration recommended."
                );
                if count >= 100 {
                    tracing::warn!(
                        warn_count = count,
                        "Quantization drift threshold (100) exceeded; auto-retraining quantizer via bound expansion."
                    );
                    q.expand_bounds_to_fit(vector);
                    self.inner.drift_warn_count.store(0, Ordering::Relaxed);
                }
            }
        }
    }

    fn get_dist_to_query(&self, query: &[f32], node_idx: u32) -> Result<f32> {
        let node = self.load_node(node_idx)?;
        match &node.vector {
            VectorData::F32(v) => compute_distance(query, v, self.inner.config.distance_metric),
            VectorData::U8(v) => {
                let q_guard = self.inner.quantizer.read();
                let q = q_guard
                    .as_ref()
                    .ok_or_else(|| MemFuseError::Index("Quantizer missing".into()))?;
                q.asymmetric_dist(query, v, self.inner.config.distance_metric)
            }
        }
    }

    fn search_in_memory(
        &self,
        query: &[f32],
        graph: &[Vec<u32>],
        vectors: &[Vec<f32>],
        entry_point: u32,
        beam_width: usize,
    ) -> Result<Vec<SearchCandidate>> {
        let mut visited = HashSet::new();
        let mut candidates = BinaryHeap::new();
        let mut results = BinaryHeap::new();

        let ep_dist = compute_distance(
            query,
            &vectors[entry_point as usize],
            self.inner.config.distance_metric,
        )?;

        let initial = SearchCandidate {
            index: entry_point,
            distance: ep_dist,
        };
        candidates.push(Reverse(initial.clone()));
        results.push(initial);
        visited.insert(entry_point);

        while let Some(Reverse(current)) = candidates.pop() {
            if let Some(worst) = results.peek() {
                if current.distance > worst.distance && results.len() >= beam_width {
                    break;
                }
            }

            for &neighbor in &graph[current.index as usize] {
                if !visited.insert(neighbor) {
                    continue;
                }
                let dist = compute_distance(
                    query,
                    &vectors[neighbor as usize],
                    self.inner.config.distance_metric,
                )?;
                let cand = SearchCandidate {
                    index: neighbor,
                    distance: dist,
                };

                if results.len() < beam_width
                    || results.peek().map(|w| dist < w.distance).unwrap_or(true)
                {
                    candidates.push(Reverse(cand.clone()));
                    results.push(cand);
                    if results.len() > beam_width {
                        results.pop();
                    }
                }
            }
        }
        Ok(results.into_vec())
    }

    fn prune_in_memory(
        &self,
        candidates: &mut [SearchCandidate],
        vectors: &[Vec<f32>],
        max_degree: usize,
        alpha: f32,
    ) -> Result<Vec<u32>> {
        if candidates.is_empty() {
            return Ok(Vec::new());
        }
        candidates.sort_by(|a, b| a.distance.total_cmp(&b.distance));

        let mut pruned = Vec::with_capacity(max_degree);
        for cand in candidates.iter() {
            if pruned.len() >= max_degree {
                break;
            }

            let mut keep = true;
            for &p_idx in &pruned {
                let dist_p_cand = compute_distance(
                    &vectors[cand.index as usize],
                    &vectors[p_idx as usize],
                    self.inner.config.distance_metric,
                )?;
                if alpha * dist_p_cand < cand.distance {
                    keep = false;
                    break;
                }
            }

            if keep {
                pruned.push(cand.index);
            }
        }
        Ok(pruned)
    }

    /// Mergt pending inserts in den On-Disk-Graphen.
    ///
    /// ALGORITHMUS (arXiv:2602.21514 §4 "Streaming DiskANN"):
    /// Wenn pending_ratio > 10%: Vollrebuild (bestehend + pending).
    /// Sonst: Inkrementeller Greedy-Search-basierter Insert pro Vektor.
    ///
    /// ATOMARES WRITE: Tmp → fsync → Rename → Parent-fsync (P3-konform).
    /// INVARIANTE INV-DISKANN-1: Atomares Rename-Muster immer eingehalten.
    pub async fn persist_delta(&self) -> Result<()> {
        let pending = {
            let mut guard = self.inner.pending_inserts.write();
            self.inner.pending_count.store(0, Ordering::Relaxed);
            std::mem::take(&mut *guard)
        };

        if pending.is_empty() {
            return Ok(());
        }

        let existing_count = self.len().await;
        let total = existing_count + pending.len();
        let pending_ratio = pending.len() as f64 / total.max(1) as f64;

        if pending_ratio > 0.10 || existing_count == 0 {
            // Vollrebuild: bestehende + pending
            let (mut all_vecs, mut all_ids) = self.load_all_vectors_from_mmap().await?;
            for (id, vec) in &pending {
                all_vecs.push(vec.clone());
                all_ids.push(*id);
            }
            return self.build(&all_vecs, &all_ids).await;
        }

        // Inkrementeller Pfad
        let tmp_path = self.inner.config.index_path.with_extension("delta.tmp");
        self.write_incremental_to_file(&tmp_path, &pending).await?;

        // Atomares Rename + Parent-fsync (INV-DISKANN-1, P3)
        let index_path = self.inner.config.index_path.clone();
        tokio::task::spawn_blocking({
            let tmp = tmp_path.clone();
            let dst = index_path;
            move || -> Result<()> {
                let tmp_file = std::fs::File::open(&tmp)?;
                tmp_file
                    .sync_all()
                    .map_err(|e| MemFuseError::Storage(format!("fsync tmp: {e}")))?;
                drop(tmp_file);
                std::fs::rename(&tmp, &dst)
                    .map_err(|e| MemFuseError::Storage(format!("rename: {e}")))?;
                if let Some(parent) = dst.parent() {
                    let dir = std::fs::File::open(parent)?;
                    dir.sync_all()
                        .map_err(|e| MemFuseError::Storage(format!("parent fsync: {e}")))?;
                }
                Ok(())
            }
        })
        .await
        .map_err(|e| MemFuseError::Storage(format!("spawn_blocking: {e}")))??;

        self.load().await // Mmap neu laden
    }

    async fn load_all_vectors_from_mmap(&self) -> Result<(Vec<Vec<f32>>, Vec<DocId>)> {
        let disk_count = {
            let guard = self.inner.header.read();
            guard.as_ref().map(|h| h.node_count as usize).unwrap_or(0)
        };
        let mut all_vecs = Vec::with_capacity(disk_count);
        let mut all_ids = Vec::with_capacity(disk_count);

        for i in 0..disk_count as u32 {
            let node = self.load_node(i)?;
            let vec_f32 = match node.vector {
                VectorData::F32(v) => v,
                VectorData::U8(v) => {
                    let q_guard = self.inner.quantizer.read();
                    let q = q_guard
                        .as_ref()
                        .ok_or_else(|| MemFuseError::Index("Quantizer missing".into()))?;
                    q.dequantize(&v)
                }
            };
            all_vecs.push(vec_f32);
            all_ids.push(node.doc_id);
        }
        Ok((all_vecs, all_ids))
    }

    /// Inkrementeller Vamana-Insert für neue Vektoren.
    /// Für jeden neuen Vektor: Beam-Search → RNG-Pruning → Rückwärts-Kanten.
    /// Schreibt das erweiterte Graphformat in tmp_path.
    async fn write_incremental_to_file(
        &self,
        tmp_path: &std::path::Path,
        new_vecs: &[(DocId, Vec<f32>)],
    ) -> Result<()> {
        // Implementation: Lade bestehenden Graphen aus Mmap, füge neue Knoten hinzu
        // via Greedy-Search (wie build() aber nur für neue Knoten), schreibe komplett neu.
        // Vereinfachung für erste Version: delegiere an build() mit allen Vektoren.
        // TODO H1: Echte inkrementelle Implementierung (Streaming-DiskANN).
        let (mut all_vecs, mut all_ids) = self.load_all_vectors_from_mmap().await?;
        for (id, vec) in new_vecs {
            all_vecs.push(vec.clone());
            all_ids.push(*id);
        }
        self.build_to_path(tmp_path, &all_vecs, &all_ids).await
    }

    pub async fn build_to_path(
        &self,
        target_path: &std::path::Path,
        vectors: &[Vec<f32>],
        ids: &[DocId],
    ) -> Result<()> {
        if vectors.is_empty() {
            return Ok(());
        }

        let n = vectors.len();

        // 0. SQ8 Training if needed
        if self.inner.config.quantize {
            let refs: Vec<&[f32]> = vectors.iter().map(|v| v.as_slice()).collect();
            let q = crate::quantize::ScalarQuantizer::train(&refs, self.inner.config.dimension);
            *self.inner.quantizer.write() = Some(q);
        }

        // 1. Initial State (Ring topology)
        let mut graph: Vec<Vec<u32>> = vec![vec![]; n];
        for (i, node_graph) in graph.iter_mut().enumerate() {
            let neighbor = ((i + 1) % n) as u32;
            node_graph.push(neighbor);
        }

        // 2. Vamana In-Memory Build Passes (Pass 1: alpha=1.0, Pass 2: alpha=1.2)
        let entry_point = 0u32;
        for alpha in [1.0f32, 1.2f32] {
            for i in 0..n {
                let mut candidates = self.search_in_memory(
                    &vectors[i],
                    &graph,
                    vectors,
                    entry_point,
                    self.inner.config.beam_width,
                )?;
                let pruned = self.prune_in_memory(
                    &mut candidates,
                    vectors,
                    self.inner.config.max_degree,
                    alpha,
                )?;

                for &neighbor in &pruned {
                    let neighbor_idx = neighbor as usize;
                    if !graph[neighbor_idx].contains(&(i as u32)) {
                        graph[neighbor_idx].push(i as u32);
                        if graph[neighbor_idx].len() > self.inner.config.max_degree {
                            let mut cand_vec: Vec<SearchCandidate> = graph[neighbor_idx]
                                .iter()
                                .map(|&idx| {
                                    let dist = compute_distance(
                                        &vectors[neighbor_idx],
                                        &vectors[idx as usize],
                                        self.inner.config.distance_metric,
                                    )
                                    .unwrap_or(f32::MAX);
                                    SearchCandidate {
                                        index: idx,
                                        distance: dist,
                                    }
                                })
                                .collect();
                            graph[neighbor_idx] = self.prune_in_memory(
                                &mut cand_vec,
                                vectors,
                                self.inner.config.max_degree,
                                alpha,
                            )?;
                        }
                    }
                }
                graph[i] = pruned;
            }
        }

        // 3. Final Write
        self.write_to_path(target_path, &graph, vectors, ids).await
    }

    /// Builds the index from a set of vectors.
    pub async fn build(&self, vectors: &[Vec<f32>], ids: &[DocId]) -> Result<()> {
        let tmp_path = self.inner.config.index_path.with_extension("idx.tmp");
        self.build_to_path(&tmp_path, vectors, ids).await?;

        tokio::fs::rename(&tmp_path, &self.inner.config.index_path)
            .await
            .map_err(MemFuseError::Io)?;

        // Fsync parent directory after rename for POSIX atomic directory entry durability
        if let Some(parent) = self.inner.config.index_path.parent() {
            let parent_dir = tokio::fs::File::open(parent)
                .await
                .map_err(MemFuseError::Io)?;
            parent_dir.sync_all().await.map_err(MemFuseError::Io)?;
        }

        self.load().await?;
        self.verify_graph_integrity_debug()?;

        Ok(())
    }

    fn verify_graph_integrity_debug(&self) -> Result<()> {
        #[cfg(debug_assertions)]
        {
            let node_count = self
                .inner
                .header
                .read()
                .map(|h| h.node_count as u32)
                .unwrap_or(0);
            let max_degree = self.inner.config.max_degree;
            for i in 0..node_count {
                let node = self.load_node(i)?;
                assert!(
                    node.neighbors.len() <= max_degree,
                    "Graph integrity violation: Node {} has {} neighbors, exceeding max_degree {}",
                    i,
                    node.neighbors.len(),
                    max_degree
                );
            }
        }
        Ok(())
    }

    async fn write_to_path(
        &self,
        path: &std::path::Path,
        graph: &[Vec<u32>],
        vectors: &[Vec<f32>],
        ids: &[DocId],
    ) -> Result<()> {
        use std::sync::atomic::Ordering;
        use tokio::fs::OpenOptions;
        use tokio::io::AsyncSeekExt;
        use tokio::io::AsyncWriteExt;

        let n = vectors.len();
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .await
            .map_err(MemFuseError::Io)?;

        let quantizer_opt = {
            let q_guard = self.inner.quantizer.read();
            q_guard.clone()
        };
        let (q_min, q_max, quantized) = if let Some(ref q) = quantizer_opt {
            (q.mins[0], q.maxes[0], 1)
        } else {
            (0.0, 0.0, 0)
        };

        let header = DiskAnnHeader {
            magic: *DISKANN_MAGIC,
            version: DISKANN_VERSION,
            node_count: n as u64,
            dimension: self.inner.config.dimension as u32,
            max_degree: self.inner.config.max_degree as u32,
            sector_size: self.inner.config.sector_size as u32,
            entry_point: 0,
            metric: self.inner.config.distance_metric as u8,
            quantized,
            q_min,
            q_max,
        };

        file.write_all(&header.to_bytes())
            .await
            .map_err(MemFuseError::Io)?;
        let padding = vec![
            0u8;
            self.inner.config.sector_size
                - (DiskAnnHeader::SIZE % self.inner.config.sector_size)
        ];
        file.write_all(&padding).await.map_err(MemFuseError::Io)?;

        for i in 0..n {
            let start_pos = file.stream_position().await.map_err(MemFuseError::Io)?;

            if let Some(ref q) = quantizer_opt {
                let qv = q.quantize(&vectors[i]);
                file.write_all(&qv).await.map_err(MemFuseError::Io)?;
            } else {
                for &val in &vectors[i] {
                    file.write_all(&val.to_le_bytes())
                        .await
                        .map_err(MemFuseError::Io)?;
                }
            }

            let neighbors = &graph[i];
            file.write_all(&(neighbors.len() as u32).to_le_bytes())
                .await
                .map_err(MemFuseError::Io)?;
            for &neighbor in neighbors {
                file.write_all(&neighbor.to_le_bytes())
                    .await
                    .map_err(MemFuseError::Io)?;
            }
            let padding_count = self.inner.config.max_degree - neighbors.len();
            file.write_all(&vec![0u8; padding_count * 4])
                .await
                .map_err(MemFuseError::Io)?;
            file.write_all(&ids[i].inner().to_le_bytes())
                .await
                .map_err(MemFuseError::Io)?;

            let end_pos = file.stream_position().await.map_err(MemFuseError::Io)?;
            let used = (end_pos - start_pos) as usize;
            let node_size = self.inner.node_size_bytes.load(Ordering::SeqCst) as usize;
            if used < node_size {
                file.write_all(&vec![0u8; node_size - used])
                    .await
                    .map_err(MemFuseError::Io)?;
            }
        }
        file.sync_all().await.map_err(MemFuseError::Io)?;
        Ok(())
    }

    /// Loads the index from the configured path.
    pub async fn load(&self) -> Result<()> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || {
            use std::sync::atomic::Ordering;
            let file = std::fs::File::open(&inner.config.index_path).map_err(MemFuseError::Io)?;
            // SAFETY: Invariant: `file` is a valid, read-only open handle to `index_path` and the underlying inode is immutable during read.
            //         Guarantor: `std::fs::File::open` verifies file existence and access permissions prior to mapping.
            //         Why: `write_to_file()` writes to `.tmp` then renames atomically; POSIX `rename()` guarantees existing readers see the old consistent inode while new loaders see the complete new file.
            //         ADR-017: Memory mapping permitted in `diskann.rs`.
            #[allow(unsafe_code)]
            let mmap = unsafe { Mmap::map(&file).map_err(MemFuseError::Io)? }; // SAFETY: 1. Invariant: Valid file descriptor and immutable mapping. 2. Guarantor: std::fs::File & atomic rename. 3. Call-site verified. 4. ADR-017 mmap.

            let header_slice = mmap
                .get(0..DiskAnnHeader::SIZE)
                .ok_or_else(|| MemFuseError::Storage("DiskANN file too small for header".into()))?;
            let header = DiskAnnHeader::try_from_bytes(header_slice)?;

            if inner.config.sector_size != header.sector_size as usize {
                return Err(MemFuseError::Index(format!(
                    "DiskANN-Index inkompatibel: Config-sector_size={} stimmt nicht mit \
                     Header-sector_size={} überein. Index muss neu aufgebaut werden.",
                    inner.config.sector_size, header.sector_size
                )));
            }

            if header.quantized != 0 {
                let dim = header.dimension as usize;
                let range = (header.q_max - header.q_min).max(1e-6);
                *inner.quantizer.write() = Some(crate::quantize::ScalarQuantizer {
                    mins: vec![header.q_min; dim],
                    maxes: vec![header.q_max; dim],
                    scales: vec![255.0 / range; dim],
                    inv_scales: vec![range / 255.0; dim],
                    dimension: dim,
                    total_queries: std::sync::atomic::AtomicU64::new(0),
                    out_of_range_queries: std::sync::atomic::AtomicU64::new(0),
                });
            }

            let vector_size = if header.quantized != 0 {
                header.dimension as usize
            } else {
                header.dimension as usize * 4
            };
            let neighbors_size = 4 + (header.max_degree as usize * 4);
            let doc_id_size = 8;
            let raw_node_size = vector_size + neighbors_size + doc_id_size;
            let node_size_bytes =
                raw_node_size.div_ceil(header.sector_size as usize) * header.sector_size as usize;
            inner
                .node_size_bytes
                .store(node_size_bytes as u64, Ordering::SeqCst);

            *inner.header.write() = Some(header);
            *inner.mmap.write() = Some(mmap);
            inner.cache.write().clear();

            let mut ids = Vec::with_capacity(header.node_count as usize);
            let sector_size = header.sector_size as usize;
            let start_offset = DiskAnnHeader::SIZE.div_ceil(sector_size) * sector_size;
            let read_size = node_size_bytes;
            if !read_size.is_multiple_of(sector_size) {
                return Err(MemFuseError::Index(
                    "Read size must be a multiple of sector_size".into(),
                ));
            }

            for i in 0..header.node_count as u32 {
                let index_offset = (i as usize).checked_mul(node_size_bytes).ok_or_else(|| {
                    MemFuseError::Index("Node offset multiplication overflow".into())
                })?;
                let offset = start_offset
                    .checked_add(index_offset)
                    .ok_or_else(|| MemFuseError::Index("Node offset addition overflow".into()))?;
                if offset % sector_size != 0 {
                    return Err(MemFuseError::Index(
                        "Read offset must be sector-aligned".into(),
                    ));
                }
                let inner_mmap = inner.mmap.read();
                let mmap_ref = inner_mmap
                    .as_ref()
                    .ok_or(MemFuseError::Index("Mmap failed".into()))?;
                let doc_id_offset = offset
                    .checked_add(vector_size)
                    .and_then(|o| o.checked_add(neighbors_size))
                    .ok_or_else(|| MemFuseError::Index("DocId offset overflow".into()))?;
                let doc_id_end = doc_id_offset
                    .checked_add(8)
                    .ok_or_else(|| MemFuseError::Index("DocId end offset overflow".into()))?;
                let doc_id_bytes = mmap_ref.get(doc_id_offset..doc_id_end).ok_or_else(|| {
                    MemFuseError::Storage("DiskANN file truncated before doc_id".into())
                })?;
                let doc_id = u64::from_le_bytes(
                    doc_id_bytes
                        .try_into()
                        .map_err(|_| MemFuseError::Index("Corrupt doc_id".into()))?,
                );
                ids.push(DocId::from(doc_id));
            }
            *inner.doc_ids.write() = ids;
            Ok(())
        })
        .await
        .map_err(|e| MemFuseError::Storage(format!("Join error during DiskANN load: {}", e)))?
    }

    fn load_node(&self, index: u32) -> Result<CachedNode> {
        use std::sync::atomic::Ordering;
        if let Some(node) = self.inner.cache.read().get(&index) {
            return Ok(node.clone());
        }

        let mmap_guard = self.inner.mmap.read();
        let mmap = mmap_guard
            .as_ref()
            .ok_or_else(|| MemFuseError::Index("Index not loaded".into()))?;
        let header_guard = self.inner.header.read();
        let header = header_guard
            .as_ref()
            .ok_or_else(|| MemFuseError::Index("Header missing".into()))?;

        let sector_size = header.sector_size as usize;
        let node_size = self.inner.node_size_bytes.load(Ordering::SeqCst) as usize;
        let read_size = node_size;
        if !read_size.is_multiple_of(sector_size) {
            return Err(MemFuseError::Index(
                "Read size must be a multiple of sector_size".into(),
            ));
        }

        let start_offset = DiskAnnHeader::SIZE.div_ceil(sector_size) * sector_size;
        let index_offset = (index as usize)
            .checked_mul(node_size)
            .ok_or_else(|| MemFuseError::Index("Node offset multiplication overflow".into()))?;
        let node_offset = start_offset
            .checked_add(index_offset)
            .ok_or_else(|| MemFuseError::Index("Node offset addition overflow".into()))?;
        if node_offset % sector_size != 0 {
            return Err(MemFuseError::Index(
                "Node read offset must be sector-aligned".into(),
            ));
        }

        let end_offset = node_offset
            .checked_add(node_size)
            .ok_or_else(|| MemFuseError::Index("Node end offset addition overflow".into()))?;

        if end_offset > mmap.len() {
            return Err(MemFuseError::Index("Node offset out of bounds".into()));
        }

        let node_data = mmap
            .get(node_offset..end_offset)
            .ok_or_else(|| MemFuseError::Index("Node data out of bounds".into()))?;
        let mut cursor: usize = 0;

        let vector = if header.quantized != 0 {
            let dim = header.dimension as usize;
            let next_cursor = cursor
                .checked_add(dim)
                .ok_or_else(|| MemFuseError::Index("Cursor overflow in quantized vector".into()))?;
            let slice = node_data
                .get(cursor..next_cursor)
                .ok_or_else(|| MemFuseError::Index("Truncated quantized vector data".into()))?;
            cursor = next_cursor;
            VectorData::U8(slice.to_vec())
        } else {
            let dim = header.dimension as usize;
            let mut v = Vec::with_capacity(dim);
            for _ in 0..dim {
                let next_cursor = cursor
                    .checked_add(4)
                    .ok_or_else(|| MemFuseError::Index("Cursor overflow in f32 vector".into()))?;
                let slice = node_data
                    .get(cursor..next_cursor)
                    .ok_or_else(|| MemFuseError::Index("Truncated f32 vector data".into()))?;
                v.push(f32::from_le_bytes(slice.try_into().map_err(|_| {
                    MemFuseError::Index("Invalid vector data".into())
                })?));
                cursor = next_cursor;
            }
            VectorData::F32(v)
        };

        let next_cursor = cursor
            .checked_add(4)
            .ok_or_else(|| MemFuseError::Index("Cursor overflow in neighbor count".into()))?;
        let count_bytes = node_data
            .get(cursor..next_cursor)
            .ok_or_else(|| MemFuseError::Index("Truncated neighbor count".into()))?;
        let neighbor_count = u32::from_le_bytes(
            count_bytes
                .try_into()
                .map_err(|_| MemFuseError::Index("Invalid neighbor count".into()))?,
        ) as usize;
        cursor = next_cursor;

        if neighbor_count > header.max_degree as usize {
            return Err(MemFuseError::Index(format!(
                "Corrupt DiskANN node: neighbor_count {} > max_degree {}",
                neighbor_count, header.max_degree
            )));
        }

        let mut neighbors = Vec::with_capacity(neighbor_count);
        for _ in 0..neighbor_count {
            let nxt = cursor
                .checked_add(4)
                .ok_or_else(|| MemFuseError::Index("Cursor overflow in neighbor ID".into()))?;
            let slice = node_data
                .get(cursor..nxt)
                .ok_or_else(|| MemFuseError::Index("Truncated neighbor ID".into()))?;
            neighbors.push(u32::from_le_bytes(
                slice
                    .try_into()
                    .map_err(|_| MemFuseError::Index("Invalid neighbor ID".into()))?,
            ));
            cursor = nxt;
        }

        let skip_bytes = (header.max_degree as usize)
            .checked_sub(neighbor_count)
            .and_then(|diff| diff.checked_mul(4))
            .ok_or_else(|| MemFuseError::Index("Padding overflow".into()))?;
        cursor = cursor
            .checked_add(skip_bytes)
            .ok_or_else(|| MemFuseError::Index("Cursor overflow in padding".into()))?;

        let doc_id_end = cursor
            .checked_add(8)
            .ok_or_else(|| MemFuseError::Index("Cursor overflow in DocId".into()))?;
        let doc_id_bytes = node_data
            .get(cursor..doc_id_end)
            .ok_or_else(|| MemFuseError::Index("Truncated DocId".into()))?;
        let doc_id = DocId::from(u64::from_le_bytes(
            doc_id_bytes
                .try_into()
                .map_err(|_| MemFuseError::Index("Invalid DocId".into()))?,
        ));

        let node = CachedNode {
            vector,
            neighbors,
            doc_id,
        };

        let mut cache = self.inner.cache.write();
        if cache.len() * node_size >= self.inner.config.memory_budget {
            // FIND-IND-004: Replace full cache wipe with 25% partial eviction
            let to_remove = cache.len() / 4;
            let keys: Vec<_> = cache.keys().take(to_remove).cloned().collect();
            for k in keys {
                cache.remove(&k);
            }
        }
        cache.insert(index, node.clone());

        Ok(node)
    }

    pub async fn search_internal(&self, query: &[f32], k: usize) -> Result<Vec<ScoredDocument>> {
        if k > memfuse_core::MAX_SEARCH_K {
            return Err(MemFuseError::invalid_input(format!(
                "Requested k ({}) exceeds maximum allowed search limit ({})",
                k,
                memfuse_core::MAX_SEARCH_K
            )));
        }
        let header = {
            let guard = self.inner.header.read();
            *guard
                .as_ref()
                .ok_or_else(|| MemFuseError::Index("Index not loaded".into()))?
        };

        if header.node_count == 0 {
            return Ok(Vec::new());
        }

        self.check_quantizer_drift(query);

        let query = query.to_vec();
        let self_clone = self.clone();

        tokio::task::spawn_blocking(move || self_clone.search_blocking(&query, k, header))
            .await
            .map_err(|e| MemFuseError::Index(format!("Join error: {}", e)))?
    }

    fn search_blocking(
        &self,
        query: &[f32],
        k: usize,
        header: DiskAnnHeader,
    ) -> Result<Vec<ScoredDocument>> {
        let beam_width = self.inner.config.beam_width;
        let metric = self.inner.config.distance_metric;

        let mut visited = HashSet::new();
        let mut candidates = BinaryHeap::new();
        let mut results = BinaryHeap::new();

        let ep = header.entry_point;
        let ep_dist = self.get_dist_to_query(query, ep)?;

        let initial_cand = SearchCandidate {
            index: ep,
            distance: ep_dist,
        };
        candidates.push(Reverse(initial_cand.clone()));
        results.push(initial_cand);
        visited.insert(ep);

        while let Some(Reverse(current)) = candidates.pop() {
            if let Some(worst) = results.peek() {
                if current.distance > worst.distance && results.len() >= beam_width {
                    break;
                }
            }

            let node = self.load_node(current.index)?;
            for &neighbor in &node.neighbors {
                if !visited.insert(neighbor) {
                    continue;
                }

                let dist = self.get_dist_to_query(query, neighbor)?;
                let cand = SearchCandidate {
                    index: neighbor,
                    distance: dist,
                };

                let is_better = if results.len() < beam_width {
                    true
                } else if let Some(worst) = results.peek() {
                    dist < worst.distance
                } else {
                    false
                };

                if is_better {
                    candidates.push(Reverse(cand.clone()));
                    results.push(cand);
                    if results.len() > beam_width {
                        results.pop();
                    }
                }
            }
        }

        let mut final_results = Vec::with_capacity(k);
        let mut sorted_results: Vec<SearchCandidate> = results.into_vec();
        sorted_results.sort_by(|a, b| a.distance.total_cmp(&b.distance));

        for c in sorted_results.into_iter().take(k) {
            let node = self.load_node(c.index)?;
            let score = match metric {
                DistanceMetric::Cosine => 1.0 - c.distance,
                DistanceMetric::Euclidean => 1.0 / (1.0 + c.distance),
                DistanceMetric::DotProduct => -c.distance,
                _ => 1.0 / (1.0 + c.distance),
            };
            final_results.push(ScoredDocument::new(node.doc_id, score));
        }

        Ok(final_results)
    }
}

impl Clone for DiskAnnIndex {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl VectorIndex for DiskAnnIndex {
    async fn insert(&self, _tx: TxId, id: DocId, embedding: &[f32]) -> Result<()> {
        self.check_quantizer_drift(embedding);

        let count = {
            let mut pending = self.inner.pending_inserts.write();
            pending.push((id, embedding.to_vec()));
            self.inner.pending_count.fetch_add(1, Ordering::Relaxed) + 1
        };

        if count >= PENDING_FLUSH_THRESHOLD {
            let index_clone = self.clone();
            tokio::spawn(async move {
                if let Err(e) = index_clone.persist_delta().await {
                    tracing::error!("DiskANN auto persist_delta failed: {e}");
                }
            });
        }
        Ok(())
    }

    async fn search(&self, query: &[f32], k: usize) -> Result<Vec<ScoredDocument>> {
        self.search_internal(query, k).await
    }

    async fn delete(&self, _tx: TxId, _id: DocId) -> Result<()> {
        Err(MemFuseError::InvalidInput(
            "DiskAnn is a read-only out-of-core index.".to_string(),
        ))
    }

    async fn commit(&self, _tx: TxId) -> Result<()> {
        Ok(())
    }
    async fn rollback(&self, _tx: TxId) -> Result<()> {
        Ok(())
    }

    async fn rollback_to_tx(&self, _tx_id: TxId) -> Result<()> {
        // DiskAnn is read-only, so rollback is always a no-op (it's already at a fixed state)
        Ok(())
    }

    async fn all_doc_ids(&self) -> Result<Vec<DocId>> {
        Ok(self.inner.doc_ids.read().clone())
    }

    async fn last_tx_id(&self) -> Result<TxId> {
        Ok(TxId(0))
    }

    async fn len(&self) -> usize {
        let guard = self.inner.header.read();
        guard.as_ref().map(|h| h.node_count as usize).unwrap_or(0)
    }

    async fn stats(&self) -> Result<VectorIndexStats> {
        use std::sync::atomic::Ordering;
        let count = self.len().await;
        let node_size = self.inner.node_size_bytes.load(Ordering::SeqCst) as usize;
        let cache_usage = self.inner.cache.read().len() * node_size;
        Ok(VectorIndexStats {
            num_vectors: count,
            memory_usage_bytes: cache_usage,
            num_layers: 1,
            deleted_ratio: 0.0,
            rebuild_count: 0,
        })
    }
}

#[derive(Clone, Debug)]
struct SearchCandidate {
    index: u32,
    distance: f32,
}

impl PartialEq for SearchCandidate {
    fn eq(&self, other: &Self) -> bool {
        self.distance == other.distance
    }
}
impl Eq for SearchCandidate {}
impl PartialOrd for SearchCandidate {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for SearchCandidate {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.distance.total_cmp(&other.distance)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_diskann_non_power_of_two_sector_size_rejected() {
        let bad_config = DiskAnnConfig {
            sector_size: 4000, // not a power of 2
            ..DiskAnnConfig::default()
        };
        let res = DiskAnnIndex::try_new(bad_config);
        assert!(matches!(res, Err(MemFuseError::InvalidInput(_))));
    }

    #[tokio::test]
    async fn test_diskann_read_only_mutations_return_error() {
        let index = DiskAnnIndex::try_new(DiskAnnConfig::default()).expect("valid config"); // expect
        let tx = TxId::new(1);
        let doc_id = DocId::from(100);

        let delete_res = index.delete(tx, doc_id).await;
        assert!(matches!(delete_res, Err(MemFuseError::InvalidInput(_))));
    }

    #[tokio::test]
    async fn test_diskann_insert_no_longer_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let config = DiskAnnConfig {
            index_path: dir.path().join("insert_test.idx"),
            dimension: 3,
            ..DiskAnnConfig::default()
        };
        let index = DiskAnnIndex::try_new(config).unwrap();
        let result = index.insert(TxId(1), DocId(42), &[0.1, 0.2, 0.3]).await;
        assert!(result.is_ok(), "insert() darf kein Err mehr zurückgeben");
    }

    #[tokio::test]
    async fn test_diskann_persist_delta_empty_noop() {
        let dir = tempfile::tempdir().unwrap();
        let config = DiskAnnConfig {
            index_path: dir.path().join("noop_test.idx"),
            ..DiskAnnConfig::default()
        };
        let index = DiskAnnIndex::try_new(config).unwrap();
        let result = index.persist_delta().await;
        assert!(
            result.is_ok(),
            "persist_delta auf leerem pending ist ein Noop"
        );
    }

    #[tokio::test]
    async fn test_diskann_persist_delta_atomic_rename() {
        // INV-DISKANN-1: Nach persist_delta() existiert kein .delta.tmp
        let dir = tempfile::tempdir().unwrap();
        let index_path = dir.path().join("atomic_test.idx");
        let config = DiskAnnConfig {
            index_path: index_path.clone(),
            dimension: 64,
            max_degree: 8,
            beam_width: 8,
            distance_metric: DistanceMetric::Euclidean,
            ..DiskAnnConfig::default()
        };
        let index = DiskAnnIndex::try_new(config).unwrap();

        let vecs: Vec<Vec<f32>> = (0..5).map(|i| vec![i as f32; 64]).collect();
        // Build initial index
        let ids: Vec<DocId> = (0..5).map(DocId::from).collect();
        index.build(&vecs, &ids).await.unwrap();

        // Insert new vectors
        for i in 5..10 {
            index
                .insert(TxId(1), DocId::from(i as u64), &vec![i as f32; 64])
                .await
                .unwrap();
        }
        index.persist_delta().await.unwrap();

        // Kein .delta.tmp sollte noch existieren
        let tmp = index_path.with_extension("delta.tmp");
        assert!(
            !tmp.exists(),
            "Temporäre Datei muss nach persist_delta bereinigt sein"
        );

        // Verify total length is now 10 and inserted vectors are searchable
        assert_eq!(index.len().await, 10);
        let query = vec![7.0f32; 64];
        let results = index.search(&query, 1).await.unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].doc_id, DocId::from(7u64));
    }

    #[tokio::test]
    async fn test_diskann_config_validation() {
        let valid_config = DiskAnnConfig {
            index_path: PathBuf::from("dummy.idx"),
            dimension: 128,
            max_degree: 64,
            sector_size: 4096,
            ..DiskAnnConfig::default()
        };

        let index = DiskAnnIndex::try_new(valid_config).expect("valid config"); // expect
        assert_eq!(index.len().await, 0);
    }

    #[tokio::test]
    async fn test_diskann_header_persistence() {
        let temp_dir = tempfile::tempdir().unwrap(); // unwrap
        let index_path = temp_dir.path().join("header_test.idx");

        let config = DiskAnnConfig {
            index_path: index_path.clone(),
            dimension: 8,
            max_degree: 4,
            distance_metric: DistanceMetric::Euclidean,
            ..DiskAnnConfig::default()
        };

        let index = DiskAnnIndex::try_new(config).expect("valid config"); // expect
        let vectors = vec![vec![1.0; 8]];
        let ids = vec![DocId::from(42)];
        index.build(&vectors, &ids).await.expect("build"); // expect

        let data = tokio::fs::read(&index_path).await.expect("read file"); // expect
        assert!(data.starts_with(b"DANN"));

        let header =
            DiskAnnHeader::try_from_bytes(&data[0..DiskAnnHeader::SIZE]).expect("try_from_bytes"); // expect
        assert_eq!(header.version, DISKANN_VERSION);
        assert_eq!(header.node_count, 1);
        assert_eq!(header.sector_size, 4096);
    }

    #[tokio::test]
    async fn test_diskann_recall_basic() {
        let temp_dir = tempfile::tempdir().unwrap(); // unwrap
        let index_path = temp_dir.path().join("recall_test.idx");

        let config = DiskAnnConfig {
            index_path: index_path.clone(),
            dimension: 16,
            max_degree: 8,
            beam_width: 8,
            distance_metric: DistanceMetric::Euclidean,
            ..DiskAnnConfig::default()
        };

        let index = DiskAnnIndex::try_new(config).expect("valid config"); // expect

        let n = 100;
        let mut vectors = Vec::with_capacity(n);
        let mut ids = Vec::with_capacity(n);
        for i in 0..n {
            let mut v = vec![0.0f32; 16];
            v[0] = i as f32;
            vectors.push(v);
            ids.push(DocId::from(i as u64));
        }

        index.build(&vectors, &ids).await.expect("Build failed"); // expect

        let query = &vectors[50];
        let results = index.search(query, 1).await.expect("Search failed"); // expect
        assert!(!results.is_empty());
        assert_eq!(results[0].doc_id, ids[50]);
    }

    #[tokio::test]
    async fn test_diskann_sq8_recall() {
        let temp_dir = tempfile::tempdir().unwrap(); // unwrap
        let index_path = temp_dir.path().join("sq8_test.idx");

        let config = DiskAnnConfig {
            index_path: index_path.clone(),
            dimension: 16,
            max_degree: 16,
            beam_width: 16,
            distance_metric: DistanceMetric::Euclidean,
            quantize: true,
            ..DiskAnnConfig::default()
        };

        let index = DiskAnnIndex::try_new(config).expect("valid config"); // expect

        let n = 200;
        let mut vectors = Vec::with_capacity(n);
        let mut ids = Vec::with_capacity(n);
        for i in 0..n {
            let mut v = vec![0.0f32; 16];
            v[0] = i as f32;
            vectors.push(v);
            ids.push(DocId::from(i as u64));
        }

        index.build(&vectors, &ids).await.expect("Build failed"); // expect

        let query = &vectors[150];
        let results = index.search(query, 1).await.expect("Search failed"); // expect
        assert!(!results.is_empty());
        assert_eq!(results[0].doc_id, ids[150]);
    }

    #[tokio::test]
    async fn test_load_node_rejects_corrupt_neighbor_count() {
        let temp_dir = tempfile::tempdir().unwrap(); // unwrap
        let index_path = temp_dir.path().join("corrupt_test.idx");

        let max_degree = 8;
        let dimension = 16;
        let config = DiskAnnConfig {
            index_path: index_path.clone(),
            dimension,
            max_degree,
            beam_width: 8,
            distance_metric: DistanceMetric::Euclidean,
            quantize: false,
            ..DiskAnnConfig::default()
        };

        let index = DiskAnnIndex::try_new(config.clone()).expect("valid config"); // expect
        let vectors = vec![vec![1.0f32; dimension]];
        let ids = vec![DocId::from(1)];

        index.build(&vectors, &ids).await.expect("Build failed"); // expect

        // Mutate neighbor_count of node 0 in the binary index file to be > max_degree
        let mut data = tokio::fs::read(&index_path).await.expect("read file"); // expect

        // Offset layout: sector_size (4096) + dimension * 4 bytes (64)
        let neighbor_count_offset = config.sector_size + (dimension * 4);
        let corrupt_count: u32 = (max_degree + 5) as u32;
        data[neighbor_count_offset..neighbor_count_offset + 4]
            .copy_from_slice(&corrupt_count.to_le_bytes());

        tokio::fs::write(&index_path, &data)
            .await
            .expect("write corrupt file"); // expect

        let reloaded_index = DiskAnnIndex::try_new(config).expect("valid config"); // expect
        reloaded_index.load().await.expect("Load header & mmap"); // expect

        let result = reloaded_index.load_node(0);
        assert!(result.is_err());
        let err_msg = result.err().unwrap().to_string(); // unwrap allowed (AGENT:03)
        assert!(
            err_msg.contains("Corrupt DiskANN node: neighbor_count 13 > max_degree 8"),
            "Unexpected error message: {}",
            err_msg
        );
    }

    #[tokio::test]
    async fn test_load_rejects_bad_magic() {
        let temp_dir = tempfile::tempdir().unwrap(); // unwrap
        let index_path = temp_dir.path().join("bad_magic_test.idx");

        let config = DiskAnnConfig {
            index_path: index_path.clone(),
            dimension: 8,
            max_degree: 4,
            distance_metric: DistanceMetric::Euclidean,
            ..DiskAnnConfig::default()
        };

        let index = DiskAnnIndex::try_new(config.clone()).expect("valid config"); // expect
        let vectors = vec![vec![1.0; 8]];
        let ids = vec![DocId::from(1)];
        index.build(&vectors, &ids).await.expect("build"); // expect

        // Mutate magic bytes
        let mut data = tokio::fs::read(&index_path).await.expect("read file"); // expect
        data[0..4].copy_from_slice(b"BADM");
        tokio::fs::write(&index_path, &data)
            .await
            .expect("write bad magic file"); // expect

        let reloaded_index = DiskAnnIndex::try_new(config).expect("valid config"); // expect
        let load_res = reloaded_index.load().await;
        assert!(load_res.is_err());
        let err_msg = load_res.err().unwrap().to_string(); // unwrap allowed (AGENT:03)
        assert!(
            err_msg.contains("Invalid DiskANN file: bad magic"),
            "Unexpected error message: {}",
            err_msg
        );
    }

    #[tokio::test]
    async fn test_load_rejects_version_mismatch() {
        let temp_dir = tempfile::tempdir().unwrap(); // unwrap
        let index_path = temp_dir.path().join("version_mismatch_test.idx");

        let config = DiskAnnConfig {
            index_path: index_path.clone(),
            dimension: 8,
            max_degree: 4,
            distance_metric: DistanceMetric::Euclidean,
            ..DiskAnnConfig::default()
        };

        let index = DiskAnnIndex::try_new(config.clone()).expect("valid config"); // expect
        let vectors = vec![vec![1.0; 8]];
        let ids = vec![DocId::from(1)];
        index.build(&vectors, &ids).await.expect("build"); // expect

        // Mutate version to 99
        let mut data = tokio::fs::read(&index_path).await.expect("read file"); // expect
        data[4..6].copy_from_slice(&99u16.to_le_bytes());
        tokio::fs::write(&index_path, &data)
            .await
            .expect("write bad version file"); // expect

        let reloaded_index = DiskAnnIndex::try_new(config).expect("valid config"); // expect
        let load_res = reloaded_index.load().await;
        assert!(load_res.is_err());
        let err_msg = load_res.err().unwrap().to_string(); // unwrap allowed (AGENT:03)
        assert!(
            err_msg.contains("DiskANN version mismatch"),
            "Unexpected error message: {}",
            err_msg
        );
    }

    #[tokio::test]
    async fn test_write_to_file_uses_tmp_and_atomic_rename() {
        let temp_dir = tempfile::tempdir().unwrap(); // unwrap
        let index_path = temp_dir.path().join("atomic_save_test.idx");
        let tmp_path = index_path.with_extension("idx.tmp");

        let config = DiskAnnConfig {
            index_path: index_path.clone(),
            dimension: 8,
            max_degree: 4,
            sector_size: 4096,
            distance_metric: DistanceMetric::Euclidean,
            ..DiskAnnConfig::default()
        };

        let index = DiskAnnIndex::try_new(config).expect("valid config"); // expect
        let vectors = vec![vec![1.0; 8]];
        let ids = vec![DocId::from(1)];
        index.build(&vectors, &ids).await.expect("build"); // expect

        // After build completes, index_path must exist and .tmp must NOT exist
        assert!(
            index_path.exists(),
            "Final index file must exist after atomic rename"
        );
        assert!(
            !tmp_path.exists(),
            "Temporary file .tmp must be cleaned up / renamed"
        );
    }

    #[tokio::test]
    async fn test_load_rejects_sector_size_mismatch() {
        let temp_dir = tempfile::tempdir().unwrap(); // unwrap allowed (AGENT:03)
        let index_path = temp_dir.path().join("sector_mismatch_test.idx");

        let build_config = DiskAnnConfig {
            index_path: index_path.clone(),
            dimension: 8,
            max_degree: 4,
            sector_size: 4096,
            distance_metric: DistanceMetric::Euclidean,
            ..DiskAnnConfig::default()
        };

        let index = DiskAnnIndex::try_new(build_config).expect("valid config"); // expect
        let vectors = vec![vec![1.0; 8]];
        let ids = vec![DocId::from(1)];
        index.build(&vectors, &ids).await.expect("build"); // expect

        let load_config = DiskAnnConfig {
            index_path,
            dimension: 8,
            max_degree: 4,
            sector_size: 2048,
            distance_metric: DistanceMetric::Euclidean,
            ..DiskAnnConfig::default()
        };

        let reloaded_index = DiskAnnIndex::try_new(load_config).expect("valid config"); // expect
        let load_res = reloaded_index.load().await;
        assert!(load_res.is_err());
        let err_msg = load_res.err().unwrap().to_string(); // unwrap allowed (AGENT:03)
        assert!(
            err_msg.contains("DiskANN-Index inkompatibel: Config-sector_size=2048 stimmt nicht mit Header-sector_size=4096 überein"),
            "Unexpected error message: {}",
            err_msg
        );
    }

    #[tokio::test]
    async fn test_diskann_build_search_roundtrip() -> Result<()> {
        let dir = tempfile::tempdir().map_err(MemFuseError::Io)?;
        let config = DiskAnnConfig {
            index_path: dir.path().join("smoke.diskann"),
            dimension: 4,
            max_degree: 8,
            beam_width: 8,
            distance_metric: DistanceMetric::Euclidean,
            quantize: true,
            ..DiskAnnConfig::default()
        };
        let index = DiskAnnIndex::try_new(config)?;
        let vectors: Vec<Vec<f32>> = (0..10).map(|i| vec![i as f32, 0.0, 0.0, 0.0]).collect();
        let ids: Vec<DocId> = (0..10).map(DocId::from).collect();
        index.build(&vectors, &ids).await?;
        let query = vec![9.0f32, 0.0, 0.0, 0.0];
        let results = index.search(&query, 1).await?;
        assert!(
            !results.is_empty(),
            "Smoke-Test: Suchergebnis darf nicht leer sein"
        );
        assert_eq!(
            results[0].doc_id,
            DocId::from(9u64),
            "Nächster Nachbar zu [9,0,0,0] muss id=9 sein"
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_diskann_exact_file_sizes_no_panic() {
        // BEFUND 3: Dateigrößen 0, 1, 10, 39, 40 Bytes
        for size in [0, 1, 10, 39, 40] {
            let temp_dir = tempfile::tempdir().unwrap();
            let index_path = temp_dir.path().join(format!("diskann_{}.idx", size));
            let dummy_data = vec![0x55u8; size];
            tokio::fs::write(&index_path, &dummy_data).await.unwrap();

            let config = DiskAnnConfig {
                index_path,
                dimension: 8,
                max_degree: 4,
                sector_size: 4096,
                ..DiskAnnConfig::default()
            };
            let index = DiskAnnIndex::try_new(config).unwrap();

            let spawn_res = tokio::spawn(async move { index.load().await }).await;
            assert!(
                spawn_res.is_ok(),
                "DiskAnnIndex::load panicked on file size {}!",
                size
            );

            let load_res = spawn_res.unwrap();
            if size < DiskAnnHeader::SIZE {
                assert!(
                    load_res.is_err(),
                    "Expected Result::Err for DiskANN file size {} < {}",
                    size,
                    DiskAnnHeader::SIZE
                );
            }
        }
    }

    #[tokio::test]
    async fn test_diskann_corrupt_offset_out_of_bounds_no_panic() {
        // BEFUND 3: Valid Header (size >= 40) but corrupt node_count / offset causing offset + 8 out of bounds
        let temp_dir = tempfile::tempdir().unwrap();
        let index_path = temp_dir.path().join("corrupt_offset.idx");

        let config = DiskAnnConfig {
            index_path: index_path.clone(),
            dimension: 8,
            max_degree: 4,
            sector_size: 4096,
            distance_metric: DistanceMetric::Euclidean,
            ..DiskAnnConfig::default()
        };

        let index = DiskAnnIndex::try_new(config.clone()).unwrap();
        let vectors = vec![vec![1.0; 8]];
        let ids = vec![DocId::from(1)];
        index.build(&vectors, &ids).await.unwrap();

        // Mutate node_count in header to 10,000 without expanding file size -> offset out of bounds
        let mut data = tokio::fs::read(&index_path).await.unwrap();
        let corrupt_count: u64 = 10_000;
        data[6..14].copy_from_slice(&corrupt_count.to_le_bytes());
        tokio::fs::write(&index_path, &data).await.unwrap();

        let reloaded = DiskAnnIndex::try_new(config).unwrap();

        let reloaded_clone = reloaded.clone();
        let spawn_res = tokio::spawn(async move { reloaded_clone.load().await }).await;
        assert!(
            spawn_res.is_ok(),
            "DiskAnnIndex::load panicked on corrupt node_count/offset!"
        );

        let load_res = spawn_res.unwrap();
        assert!(
            load_res.is_err(),
            "Expected Result::Err on corrupt offset beyond file size!"
        );

        // Also test load_node directly on reloaded index with truncated/corrupt file
        let catch_node =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| reloaded.load_node(9999)));
        assert!(
            catch_node.is_ok(),
            "load_node panicked on out of bounds node index!"
        );
        assert!(catch_node.unwrap().is_err());
    }

    #[tokio::test]
    async fn test_diskann_loaded_doc_ids_match_original() -> Result<()> {
        let dir = tempfile::tempdir().map_err(MemFuseError::Io)?;
        let config = DiskAnnConfig {
            index_path: dir.path().join("doc_ids.diskann"),
            dimension: 4,
            max_degree: 4,
            sector_size: 4096,
            distance_metric: DistanceMetric::Euclidean,
            quantize: false,
            ..DiskAnnConfig::default()
        };
        let index = DiskAnnIndex::try_new(config.clone())?;
        let vectors: Vec<Vec<f32>> = (0..5).map(|i| vec![i as f32, 1.0, 2.0, 3.0]).collect();
        let expected_ids: Vec<DocId> = vec![
            DocId::from(1001u64),
            DocId::from(1002u64),
            DocId::from(1003u64),
            DocId::from(1004u64),
            DocId::from(1005u64),
        ];
        index.build(&vectors, &expected_ids).await?;

        // Verify loaded doc_ids in fresh index instance match original IDs
        let reloaded = DiskAnnIndex::try_new(config)?;
        reloaded.load().await?;
        let loaded_ids = reloaded.inner.doc_ids.read().clone();
        assert_eq!(
            loaded_ids, expected_ids,
            "Loaded doc_ids must exactly match original doc_ids (regression for D-2.1 offset bug)"
        );
        Ok(())
    }
}
