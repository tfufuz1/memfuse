//! DiskANN Out-of-Core Vector Search (WP-4.3).

#![allow(unsafe_code)]

use crate::distance::compute_distance;
use ahash::AHashMap;
use memfuse_core::{
    DistanceMetric, DocId, MemFuseError, Result, ScoredDocument, TxId, VectorIndex,
    VectorIndexStats,
};
use memmap2::Mmap;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashSet};
use std::path::PathBuf;
use tokio::fs::OpenOptions;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::RwLock;

const DISKANN_MAGIC: &[u8; 4] = b"DANN";
const DISKANN_VERSION: u16 = 1;

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
        if &bytes[0..4] != DISKANN_MAGIC {
            return Err(MemFuseError::Index("Invalid DiskANN magic".into()));
        }
        Ok(Self {
            magic: *DISKANN_MAGIC,
            version: u16::from_le_bytes(
                bytes[4..6]
                    .try_into()
                    .map_err(|_| MemFuseError::Index("Invalid version".into()))?,
            ),
            node_count: u64::from_le_bytes(
                bytes[6..14]
                    .try_into()
                    .map_err(|_| MemFuseError::Index("Invalid node_count".into()))?,
            ),
            dimension: u32::from_le_bytes(
                bytes[14..18]
                    .try_into()
                    .map_err(|_| MemFuseError::Index("Invalid dimension".into()))?,
            ),
            max_degree: u32::from_le_bytes(
                bytes[18..22]
                    .try_into()
                    .map_err(|_| MemFuseError::Index("Invalid max_degree".into()))?,
            ),
            sector_size: u32::from_le_bytes(
                bytes[22..26]
                    .try_into()
                    .map_err(|_| MemFuseError::Index("Invalid sector_size".into()))?,
            ),
            entry_point: u32::from_le_bytes(
                bytes[26..30]
                    .try_into()
                    .map_err(|_| MemFuseError::Index("Invalid entry_point".into()))?,
            ),
            metric: bytes[30],
            quantized: bytes[31],
            q_min: f32::from_le_bytes(
                bytes[32..36]
                    .try_into()
                    .map_err(|_| MemFuseError::Index("Invalid q_min".into()))?,
            ),
            q_max: f32::from_le_bytes(
                bytes[36..40]
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
    config: DiskAnnConfig,
    header: Option<DiskAnnHeader>,
    mmap: Option<Mmap>,
    node_size_bytes: usize,
    cache: RwLock<AHashMap<u32, CachedNode>>,
    doc_ids: RwLock<Vec<DocId>>,
    quantizer: RwLock<Option<crate::quantize::ScalarQuantizer>>,
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
            config,
            header: None,
            mmap: None,
            node_size_bytes,
            cache: RwLock::new(AHashMap::new()),
            doc_ids: RwLock::new(Vec::new()),
            quantizer: RwLock::new(None),
        })
    }

    async fn compute_dist_between_nodes(&self, idx_a: u32, idx_b: u32) -> Result<f32> {
        let node_a = self.load_node(idx_a).await?;
        let node_b = self.load_node(idx_b).await?;

        match (&node_a.vector, &node_b.vector) {
            (VectorData::F32(v_a), VectorData::F32(v_b)) => {
                compute_distance(v_a, v_b, self.config.distance_metric)
            }
            (VectorData::U8(v_a), VectorData::U8(v_b)) => {
                let q_guard = self.quantizer.read().await;
                let q = q_guard
                    .as_ref()
                    .ok_or_else(|| MemFuseError::Index("Quantizer missing".into()))?;
                q.symmetric_dist(v_a, v_b, self.config.distance_metric)
            }
            _ => Err(MemFuseError::Index("Mixed vector types".into())),
        }
    }

    async fn get_dist_to_query(&self, query: &[f32], node_idx: u32) -> Result<f32> {
        let node = self.load_node(node_idx).await?;
        match &node.vector {
            VectorData::F32(v) => compute_distance(query, v, self.config.distance_metric),
            VectorData::U8(v) => {
                let q_guard = self.quantizer.read().await;
                let q = q_guard
                    .as_ref()
                    .ok_or_else(|| MemFuseError::Index("Quantizer missing".into()))?;
                q.asymmetric_dist(query, v, self.config.distance_metric)
            }
        }
    }
    async fn prune(
        &self,
        candidates: &mut [SearchCandidate],
        max_degree: usize,
        alpha: f32,
    ) -> Vec<u32> {
        if candidates.is_empty() {
            return Vec::new();
        }
        candidates.sort_by(|a, b| a.distance.total_cmp(&b.distance));

        let mut pruned = Vec::with_capacity(max_degree);
        for cand in candidates.iter() {
            if pruned.len() >= max_degree {
                break;
            }

            let mut keep = true;
            for &p_idx in &pruned {
                let dist_p_cand = self
                    .compute_dist_between_nodes(cand.index, p_idx)
                    .await
                    .unwrap_or(f32::MAX);
                if alpha * dist_p_cand < cand.distance {
                    keep = false;
                    break;
                }
            }

            if keep {
                pruned.push(cand.index);
            }
        }
        pruned
    }

    /// Builds the index from a set of vectors.
    pub async fn build(&mut self, vectors: &[Vec<f32>], ids: &[DocId]) -> Result<()> {
        if vectors.is_empty() {
            return Ok(());
        }

        let n = vectors.len();

        // 0. SQ8 Training if needed
        if self.config.quantize {
            let refs: Vec<&[f32]> = vectors.iter().map(|v| v.as_slice()).collect();
            let q = crate::quantize::ScalarQuantizer::train(&refs, self.config.dimension);
            *self.quantizer.write().await = Some(q);
        }

        // 1. Initial State
        let mut graph: Vec<Vec<u32>> = vec![vec![]; n];
        for (i, node_graph) in graph.iter_mut().enumerate() {
            let neighbor = (i + 1) % n;
            node_graph.push(neighbor as u32);
        }

        // 2. Initial Write & Mmap
        self.write_to_file(&graph, vectors, ids).await?;
        self.load().await?; // Load mmap and doc_ids

        // 3. Vamana Build Pass
        let alpha = 1.2;
        for (i, vector) in vectors.iter().enumerate() {
            let mut results = self
                .search_to_candidates(vector, self.config.beam_width)
                .await?;
            let pruned = self
                .prune(&mut results, self.config.max_degree, alpha)
                .await;

            for &neighbor in &pruned {
                let neighbor_idx = neighbor as usize;
                if !graph[neighbor_idx].contains(&(i as u32))
                    && graph[neighbor_idx].len() < self.config.max_degree
                {
                    graph[neighbor_idx].push(i as u32);
                }
            }
            graph[i] = pruned;
        }

        // 4. Final Write
        self.write_to_file(&graph, vectors, ids).await?;
        self.load().await?;

        Ok(())
    }

    async fn write_to_file(
        &self,
        graph: &[Vec<u32>],
        vectors: &[Vec<f32>],
        ids: &[DocId],
    ) -> Result<()> {
        let n = vectors.len();
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&self.config.index_path)
            .await
            .map_err(MemFuseError::Io)?;

        let quantizer_opt = {
            let q_guard = self.quantizer.read().await;
            q_guard.clone()
        };
        let (q_min, q_max, quantized) = if let Some(ref q) = quantizer_opt {
            (q.min, q.max, 1)
        } else {
            (0.0, 0.0, 0)
        };

        let header = DiskAnnHeader {
            magic: *DISKANN_MAGIC,
            version: DISKANN_VERSION,
            node_count: n as u64,
            dimension: self.config.dimension as u32,
            max_degree: self.config.max_degree as u32,
            sector_size: self.config.sector_size as u32,
            entry_point: 0,
            metric: self.config.distance_metric as u8,
            quantized,
            q_min,
            q_max,
        };

        file.write_all(&header.to_bytes())
            .await
            .map_err(MemFuseError::Io)?;
        let padding =
            vec![0u8; self.config.sector_size - (DiskAnnHeader::SIZE % self.config.sector_size)];
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
            let padding_count = self.config.max_degree - neighbors.len();
            file.write_all(&vec![0u8; padding_count * 4])
                .await
                .map_err(MemFuseError::Io)?;
            file.write_all(&ids[i].inner().to_le_bytes())
                .await
                .map_err(MemFuseError::Io)?;

            let end_pos = file.stream_position().await.map_err(MemFuseError::Io)?;
            let used = (end_pos - start_pos) as usize;
            if used < self.node_size_bytes {
                file.write_all(&vec![0u8; self.node_size_bytes - used])
                    .await
                    .map_err(MemFuseError::Io)?;
            }
        }
        file.sync_all().await.map_err(MemFuseError::Io)?;
        Ok(())
    }

    async fn search_to_candidates(
        &self,
        query: &[f32],
        beam_width: usize,
    ) -> Result<Vec<SearchCandidate>> {
        let header = self
            .header
            .as_ref()
            .ok_or_else(|| MemFuseError::Index("Index not loaded".into()))?;
        let mut visited = HashSet::new();
        let mut candidates = BinaryHeap::new();
        let mut results = BinaryHeap::new();

        let ep = header.entry_point;
        let ep_dist = self.get_dist_to_query(query, ep).await?;

        let initial = SearchCandidate {
            index: ep,
            distance: ep_dist,
        };
        candidates.push(Reverse(initial.clone()));
        results.push(initial);
        visited.insert(ep);

        while let Some(Reverse(current)) = candidates.pop() {
            if let Some(worst) = results.peek() {
                if current.distance > worst.distance && results.len() >= beam_width {
                    break;
                }
            }

            let node = self.load_node(current.index).await?;
            for &neighbor in &node.neighbors {
                if !visited.insert(neighbor) {
                    continue;
                }
                let dist = self.get_dist_to_query(query, neighbor).await?;
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

    /// Loads the index from the configured path.
    pub async fn load(&mut self) -> Result<()> {
        let file = std::fs::File::open(&self.config.index_path).map_err(MemFuseError::Io)?;
        // ANCHOR:SAFETY:MMAP-002 — Memory-mapping of a DiskANN index file.
        // BEGRÜNDUNG: Mapping einer Datei ist sicher, solange sie während des Mappings nicht gekürzt
        // oder extern modifiziert wird. MemFuse garantiert dies durch File-Level-Locks.
        // SAFETY: Mapping is safe as long as the file is not modified externally.
        let mmap = unsafe { Mmap::map(&file).map_err(MemFuseError::Io)? };

        let header = DiskAnnHeader::try_from_bytes(&mmap[0..DiskAnnHeader::SIZE])?;

        if header.quantized != 0 {
            *self.quantizer.write().await = Some(crate::quantize::ScalarQuantizer {
                min: header.q_min,
                max: header.q_max,
                scale: 255.0 / (header.q_max - header.q_min).max(1e-6),
                inv_scale: (header.q_max - header.q_min) / 255.0,
                dimension: header.dimension as usize,
            });
        }

        self.header = Some(header);
        let vector_size = if header.quantized != 0 {
            header.dimension as usize
        } else {
            header.dimension as usize * 4
        };
        let neighbors_size = 4 + (header.max_degree as usize * 4);
        let doc_id_size = 8;
        let raw_node_size = vector_size + neighbors_size + doc_id_size;
        self.node_size_bytes =
            raw_node_size.div_ceil(header.sector_size as usize) * header.sector_size as usize;

        self.mmap = Some(mmap);

        // Load DocIds into RAM (Scan all nodes once)
        let mut ids = Vec::with_capacity(header.node_count as usize);
        for i in 0..header.node_count as u32 {
            let node = self.load_node(i).await?;
            ids.push(node.doc_id);
        }
        *self.doc_ids.write().await = ids;

        Ok(())
    }

    async fn load_node(&self, index: u32) -> Result<CachedNode> {
        // 1. Check Cache
        if let Some(node) = self.cache.read().await.get(&index) {
            return Ok(node.clone());
        }

        // 2. Load from Mmap
        let mmap = self
            .mmap
            .as_ref()
            .ok_or_else(|| MemFuseError::Index("Index not loaded".into()))?;
        let header = self
            .header
            .as_ref()
            .ok_or_else(|| MemFuseError::Index("Header missing".into()))?;

        let start_offset =
            DiskAnnHeader::SIZE.div_ceil(header.sector_size as usize) * header.sector_size as usize;
        let node_offset = start_offset + (index as usize * self.node_size_bytes);

        if node_offset + self.node_size_bytes > mmap.len() {
            return Err(MemFuseError::Index("Node offset out of bounds".into()));
        }

        let node_data = &mmap[node_offset..node_offset + self.node_size_bytes];
        let mut cursor = 0;

        let vector = if header.quantized != 0 {
            let v = node_data[cursor..cursor + header.dimension as usize].to_vec();
            cursor += header.dimension as usize;
            VectorData::U8(v)
        } else {
            let mut v = Vec::with_capacity(header.dimension as usize);
            for _ in 0..header.dimension {
                // SAFETY: In-bounds check performed above.
                v.push(f32::from_le_bytes(
                    node_data[cursor..cursor + 4]
                        .try_into()
                        .map_err(|_| MemFuseError::Index("Invalid vector data".into()))?,
                ));
                cursor += 4;
            }
            VectorData::F32(v)
        };

        // Neighbors
        let neighbor_count = u32::from_le_bytes(
            node_data[cursor..cursor + 4]
                .try_into()
                .map_err(|_| MemFuseError::Index("Invalid neighbor count".into()))?,
        ) as usize;
        cursor += 4;
        let mut neighbors = Vec::with_capacity(neighbor_count);
        for _ in 0..neighbor_count {
            neighbors.push(u32::from_le_bytes(
                node_data[cursor..cursor + 4]
                    .try_into()
                    .map_err(|_| MemFuseError::Index("Invalid neighbor ID".into()))?,
            ));
            cursor += 4;
        }
        // Skip padding neighbors
        cursor += (header.max_degree as usize - neighbor_count) * 4;

        // DocId
        let doc_id = DocId::from(u64::from_le_bytes(
            node_data[cursor..cursor + 8]
                .try_into()
                .map_err(|_| MemFuseError::Index("Invalid DocId".into()))?,
        ));

        let node = CachedNode {
            vector,
            neighbors,
            doc_id,
        };

        // 3. Update Cache (Naive Clear-on-Budget)
        let mut cache = self.cache.write().await;
        if cache.len() * self.node_size_bytes >= self.config.memory_budget {
            cache.clear();
        }
        cache.insert(index, node.clone());

        Ok(node)
    }

    /// Optimized Beam Search
    // TODO(WP-8.2): DiskANN/HNSW Async I/O (S3-B)
    // Synchronous Mmap/File operations in tokio threads can stall the reactor.
    // Migrate blocking search operations to native tokio AIO.
    pub async fn search_internal(&self, query: &[f32], k: usize) -> Result<Vec<ScoredDocument>> {
        let header = self
            .header
            .as_ref()
            .ok_or_else(|| MemFuseError::Index("Index not loaded".into()))?;
        if header.node_count == 0 {
            return Ok(Vec::new());
        }

        let mut visited = HashSet::new();
        let mut candidates = BinaryHeap::new();
        let mut results = BinaryHeap::new();

        let ep = header.entry_point;
        let ep_dist = self.get_dist_to_query(query, ep).await?;

        let initial_cand = SearchCandidate {
            index: ep,
            distance: ep_dist,
        };
        candidates.push(Reverse(initial_cand.clone()));
        results.push(initial_cand);
        visited.insert(ep);

        while let Some(Reverse(current)) = candidates.pop() {
            if let Some(worst) = results.peek() {
                if current.distance > worst.distance && results.len() >= self.config.beam_width {
                    break;
                }
            }

            let node = self.load_node(current.index).await?;
            for &neighbor in &node.neighbors {
                if !visited.insert(neighbor) {
                    continue;
                }

                let dist = self.get_dist_to_query(query, neighbor).await?;
                let cand = SearchCandidate {
                    index: neighbor,
                    distance: dist,
                };

                let is_better = if results.len() < self.config.beam_width {
                    true
                } else if let Some(worst) = results.peek() {
                    dist < worst.distance
                } else {
                    false
                };

                if is_better {
                    candidates.push(Reverse(cand.clone()));
                    results.push(cand);
                    if results.len() > self.config.beam_width {
                        results.pop();
                    }
                }
            }
        }

        let mut final_results = Vec::with_capacity(k);
        let mut sorted_results: Vec<SearchCandidate> = results.into_vec();
        sorted_results.sort_by(|a, b| a.distance.total_cmp(&b.distance));

        for c in sorted_results.into_iter().take(k) {
            let node = self.load_node(c.index).await?;
            let score = match self.config.distance_metric {
                DistanceMetric::Cosine => 1.0 - c.distance,
                DistanceMetric::Euclidean => 1.0 / (1.0 + c.distance),
                DistanceMetric::DotProduct => -c.distance,
            };
            final_results.push(ScoredDocument::new(node.doc_id, score));
        }

        Ok(final_results)
    }
}

#[async_trait::async_trait]
impl VectorIndex for DiskAnnIndex {
    async fn insert(&self, _tx: TxId, _id: DocId, _embedding: &[f32]) -> Result<()> {
        Err(MemFuseError::InvalidInput(
            "DiskAnn is a read-only out-of-core index. Use build() for batch creation.".to_string(),
        ))
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

    async fn last_tx_id(&self) -> Result<u64> {
        Ok(0)
    }

    async fn len(&self) -> usize {
        self.header.map(|h| h.node_count as usize).unwrap_or(0)
    }

    async fn stats(&self) -> Result<VectorIndexStats> {
        let count = self.len().await;
        let cache_usage = self.cache.read().await.len() * self.node_size_bytes;
        Ok(VectorIndexStats {
            num_vectors: count,
            memory_usage_bytes: cache_usage,
            num_layers: 1,
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
    async fn test_diskann_config_validation() {
        let valid_config = DiskAnnConfig {
            index_path: PathBuf::from("dummy.idx"),
            dimension: 128,
            max_degree: 64,
            sector_size: 4096,
            ..DiskAnnConfig::default()
        };

        let index = DiskAnnIndex::try_new(valid_config).expect("valid config");
        assert_eq!(index.len().await, 0);
    }

    #[tokio::test]
    async fn test_diskann_header_persistence() {
        let temp_dir = tempfile::tempdir().expect("test environment");
        let index_path = temp_dir.path().join("header_test.idx");

        let config = DiskAnnConfig {
            index_path: index_path.clone(),
            dimension: 8,
            max_degree: 4,
            distance_metric: DistanceMetric::Euclidean,
            ..DiskAnnConfig::default()
        };

        let mut index = DiskAnnIndex::try_new(config).expect("valid config");
        let vectors = vec![vec![1.0; 8]];
        let ids = vec![DocId::from(42)];
        index.build(&vectors, &ids).await.expect("build");

        let data = tokio::fs::read(&index_path).await.expect("read file");
        assert!(data.starts_with(b"DANN"));

        let header =
            DiskAnnHeader::try_from_bytes(&data[0..DiskAnnHeader::SIZE]).expect("try_from_bytes");
        assert_eq!(header.version, DISKANN_VERSION);
        assert_eq!(header.node_count, 1);
        assert_eq!(header.sector_size, 4096);
    }

    #[tokio::test]
    async fn test_diskann_recall_basic() {
        let temp_dir = tempfile::tempdir().expect("test environment");
        let index_path = temp_dir.path().join("recall_test.idx");

        let config = DiskAnnConfig {
            index_path: index_path.clone(),
            dimension: 16,
            max_degree: 8,
            beam_width: 8,
            distance_metric: DistanceMetric::Euclidean,
            ..DiskAnnConfig::default()
        };

        let mut index = DiskAnnIndex::try_new(config).expect("valid config");

        let n = 100;
        let mut vectors = Vec::with_capacity(n);
        let mut ids = Vec::with_capacity(n);
        for i in 0..n {
            let mut v = vec![0.0f32; 16];
            v[0] = i as f32;
            vectors.push(v);
            ids.push(DocId::from(i as u64));
        }

        index.build(&vectors, &ids).await.expect("Build failed");

        let query = &vectors[50];
        let results = index.search(query, 1).await.expect("Search failed");
        assert!(!results.is_empty());
        assert_eq!(results[0].doc_id, ids[50]);
    }

    #[tokio::test]
    async fn test_diskann_sq8_recall() {
        let temp_dir = tempfile::tempdir().expect("test environment");
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

        let mut index = DiskAnnIndex::try_new(config).expect("valid config");

        let n = 200;
        let mut vectors = Vec::with_capacity(n);
        let mut ids = Vec::with_capacity(n);
        for i in 0..n {
            let mut v = vec![0.0f32; 16];
            v[0] = i as f32;
            vectors.push(v);
            ids.push(DocId::from(i as u64));
        }

        index.build(&vectors, &ids).await.expect("Build failed");

        let query = &vectors[150];
        let results = index.search(query, 1).await.expect("Search failed");
        assert!(!results.is_empty());
        assert_eq!(results[0].doc_id, ids[150]);
    }
}
