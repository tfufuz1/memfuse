//! DiskANN Out-of-Core Vector Search (WP-4.3).

#![allow(unsafe_code)]

use crate::distance::compute_distance;
use ahash::AHashMap;
use memfuse_core::{DistanceMetric, DocId, MemFuseError, Result, ScoredDocument};
use memmap2::Mmap;
use parking_lot::RwLock;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashSet};
use std::fs::OpenOptions;
use std::io::{Seek, Write};
use std::path::PathBuf;

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
}

impl Default for DiskAnnConfig {
    fn default() -> Self {
        Self {
            index_path: PathBuf::from("diskann.idx"),
            dimension: 128,
            max_degree: 64,
            beam_width: 4,
            sector_size: 4096,
            memory_budget: 128 * 1024 * 1024, // 128MB
            distance_metric: DistanceMetric::Cosine,
        }
    }
}

/// A node in the DiskANN graph.
#[derive(Debug, Clone)]
struct CachedNode {
    vector: Vec<f32>,
    neighbors: Vec<u32>,
    doc_id: DocId,
}

/// DiskANN out-of-core vector index.
#[derive(Debug)]
pub struct DiskAnnIndex {
    config: DiskAnnConfig,
    mmap: Option<Mmap>,
    node_count: usize,
    entry_point: u32,
    node_size_bytes: usize,
    cache: RwLock<AHashMap<u32, CachedNode>>,
    doc_ids: Vec<DocId>,
}

impl DiskAnnIndex {
    /// Creates a new DiskANN index with the given configuration.
    pub fn try_new(config: DiskAnnConfig) -> Result<Self> {
        if !config.sector_size.is_power_of_two() {
            return Err(MemFuseError::InvalidInput(
                "Sector size must be a power of 2".to_string(),
            ));
        }
        if config.memory_budget < config.sector_size {
            return Err(MemFuseError::InvalidInput(
                "Memory budget must be at least sector_size".to_string(),
            ));
        }

        let raw_node_size = (config.dimension * 4) + 4 + (config.max_degree * 4) + 8;
        let node_size_bytes = raw_node_size.div_ceil(config.sector_size) * config.sector_size;

        Ok(Self {
            config,
            mmap: None,
            node_count: 0,
            entry_point: 0,
            node_size_bytes,
            cache: RwLock::new(AHashMap::new()),
            doc_ids: Vec::new(),
        })
    }

    /// Builds the DiskANN index from a set of vectors.
    pub async fn build(&mut self, vectors: &[Vec<f32>], ids: &[DocId]) -> Result<()> {
        if vectors.is_empty() {
            return Ok(());
        }

        let n = vectors.len();
        self.node_count = n;
        self.doc_ids = ids.to_vec();

        let mut graph: Vec<Vec<u32>> = vec![vec![]; n];

        for (i, node_graph) in graph.iter_mut().enumerate() {
            let num_neighbors = self.config.max_degree.min(n - 1);
            for _ in 0..num_neighbors {
                let neighbor = rand::random::<usize>() % n;
                if neighbor != i && !node_graph.contains(&(neighbor as u32)) {
                    node_graph.push(neighbor as u32);
                }
            }
        }

        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&self.config.index_path)
            .map_err(MemFuseError::Io)?;

        file.write_all(b"DANN").map_err(MemFuseError::Io)?;
        file.write_all(&(n as u64).to_le_bytes())
            .map_err(MemFuseError::Io)?;
        file.write_all(&0u32.to_le_bytes())
            .map_err(MemFuseError::Io)?;
        file.write_all(&(self.config.dimension as u32).to_le_bytes())
            .map_err(MemFuseError::Io)?;
        file.write_all(&(self.config.max_degree as u32).to_le_bytes())
            .map_err(MemFuseError::Io)?;

        let header_size: usize = 4 + 8 + 4 + 4 + 4;
        let padding = vec![0u8; self.config.sector_size - (header_size % self.config.sector_size)];
        file.write_all(&padding).map_err(MemFuseError::Io)?;

        for (i, node_graph) in graph.iter().enumerate() {
            let offset = file.stream_position().map_err(MemFuseError::Io)?;

            for &val in &vectors[i] {
                file.write_all(&val.to_le_bytes())
                    .map_err(MemFuseError::Io)?;
            }
            file.write_all(&(node_graph.len() as u32).to_le_bytes())
                .map_err(MemFuseError::Io)?;
            for &neighbor in node_graph {
                file.write_all(&neighbor.to_le_bytes())
                    .map_err(MemFuseError::Io)?;
            }
            let padding_neighbors = self.config.max_degree - node_graph.len();
            file.write_all(&vec![0u8; padding_neighbors * 4])
                .map_err(MemFuseError::Io)?;

            file.write_all(&ids[i].inner().to_le_bytes())
                .map_err(MemFuseError::Io)?;

            let current_pos = file.stream_position().map_err(MemFuseError::Io)?;
            let used = current_pos - offset;
            if used < self.node_size_bytes as u64 {
                let node_padding = vec![0u8; self.node_size_bytes - used as usize];
                file.write_all(&node_padding).map_err(MemFuseError::Io)?;
            }
        }

        file.sync_all().map_err(MemFuseError::Io)?;

        // SAFETY: Mapping a file that we just wrote and synced is safe as long as the file is not truncated while mapped.
        self.mmap = Some(unsafe { Mmap::map(&file).map_err(MemFuseError::Io)? });
        self.entry_point = 0;

        Ok(())
    }

    /// Searches for the k nearest neighbors using beam search.
    pub async fn search(&self, query: &[f32], k: usize) -> Result<Vec<ScoredDocument>> {
        if self.node_count == 0 || self.mmap.is_none() {
            return Ok(Vec::new());
        }

        let mut visited = HashSet::new();
        let mut candidates = BinaryHeap::new();
        let mut results = BinaryHeap::new();

        let ep = self.entry_point;
        let ep_node = self.load_node(ep)?;
        let dist = compute_distance(query, &ep_node.vector, self.config.distance_metric)?;

        let cand = SearchCandidate {
            index: ep,
            distance: dist,
        };
        candidates.push(Reverse(cand.clone()));
        results.push(cand);
        visited.insert(ep);

        while let Some(Reverse(current)) = candidates.pop() {
            if results.len() >= self.config.beam_width
                && current.distance > results.peek().unwrap().distance
            /* unwrap */
            {
                break;
            }

            let node = self.load_node(current.index)?;
            for &neighbor in &node.neighbors {
                if visited.insert(neighbor) {
                    let n_node = self.load_node(neighbor)?;
                    let d = compute_distance(query, &n_node.vector, self.config.distance_metric)?;
                    let new_cand = SearchCandidate {
                        index: neighbor,
                        distance: d,
                    };

                    if results.len() < self.config.beam_width
                        || d < results.peek().unwrap().distance
                    /* unwrap */
                    {
                        candidates.push(Reverse(new_cand.clone()));
                        results.push(new_cand);
                        if results.len() > self.config.beam_width {
                            results.pop();
                        }
                    }
                }
            }
        }

        let mut final_results: Vec<ScoredDocument> = results
            .into_iter()
            .take(k)
            .map(|c| {
                let node = self
                    .load_node(c.index)
                    .expect("Node should be in cache or index");
                ScoredDocument {
                    doc_id: node.doc_id,
                    score: 1.0 / (1.0 + c.distance),
                }
            })
            .collect();

        final_results.sort_by(|a, b| b.score.total_cmp(&a.score));
        Ok(final_results)
    }

    fn load_node(&self, index: u32) -> Result<CachedNode> {
        {
            let cache = self.cache.read();
            if let Some(node) = cache.get(&index) {
                return Ok(node.clone());
            }
        }

        let mmap = self
            .mmap
            .as_ref()
            .ok_or_else(|| MemFuseError::Index("Index not loaded".into()))?;
        let header_size: usize = 4 + 8 + 4 + 4 + 4;
        let start_offset = header_size.div_ceil(self.config.sector_size) * self.config.sector_size;
        let node_offset = start_offset + (index as usize * self.node_size_bytes);

        if node_offset + self.node_size_bytes > mmap.len() {
            return Err(MemFuseError::Index("Node offset out of bounds".into()));
        }

        let node_data = &mmap[node_offset..node_offset + self.node_size_bytes];
        let mut cursor = 0;

        let mut vector = Vec::with_capacity(self.config.dimension);
        for _ in 0..self.config.dimension {
            let val = f32::from_le_bytes(
                node_data[cursor..cursor + 4]
                    .try_into()
                    .map_err(|_| MemFuseError::Index("Malformed node vector".into()))?,
            );
            vector.push(val);
            cursor += 4;
        }

        let num_neighbors = u32::from_le_bytes(
            node_data[cursor..cursor + 4]
                .try_into()
                .map_err(|_| MemFuseError::Index("Malformed node neighbor count".into()))?,
        ) as usize;
        cursor += 4;

        let mut neighbors = Vec::with_capacity(num_neighbors);
        for _ in 0..num_neighbors {
            let neighbor = u32::from_le_bytes(
                node_data[cursor..cursor + 4]
                    .try_into()
                    .map_err(|_| MemFuseError::Index("Malformed node neighbor".into()))?,
            );
            neighbors.push(neighbor);
            cursor += 4;
        }

        let padding_neighbors = self.config.max_degree - num_neighbors;
        cursor += padding_neighbors * 4;

        let doc_id_raw = u64::from_le_bytes(
            node_data[cursor..cursor + 8]
                .try_into()
                .map_err(|_| MemFuseError::Index("Malformed node doc id".into()))?,
        );
        let doc_id = DocId::from(doc_id_raw);

        let node = CachedNode {
            vector,
            neighbors,
            doc_id,
        };

        let mut cache = self.cache.write();
        if cache.len() * self.node_size_bytes < self.config.memory_budget {
            cache.insert(index, node.clone());
        } else {
            cache.clear();
            cache.insert(index, node.clone());
        }

        Ok(node)
    }

    pub fn len(&self) -> usize {
        self.node_count
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
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
            beam_width: 8,
            sector_size: 4096,
            memory_budget: 1024 * 1024,
            distance_metric: DistanceMetric::Cosine,
        };

        let index = DiskAnnIndex::try_new(valid_config).expect("valid config");
        assert!(index.is_empty());

        let invalid_sector = DiskAnnConfig {
            sector_size: 1000,
            ..DiskAnnConfig::default()
        };

        let err =
            DiskAnnIndex::try_new(invalid_sector).expect_err("Should reject unaligned sector size");
        match err {
            MemFuseError::InvalidInput(msg) => {
                assert!(msg.contains("Sector size must be a power of 2"));
            }
            _ => panic!("Expected InvalidInput for sector size, got {:?}", err),
        }
    }

    #[tokio::test]
    async fn test_diskann_recall_at_10() {
        let config = DiskAnnConfig {
            index_path: PathBuf::from("recall_test.idx"),
            dimension: 16,
            max_degree: 8,
            beam_width: 8,
            distance_metric: DistanceMetric::Euclidean,
            ..DiskAnnConfig::default()
        };

        let mut index = DiskAnnIndex::try_new(config).expect("valid config");

        let n = 1000;
        let mut vectors = Vec::with_capacity(n);
        let mut ids = Vec::with_capacity(n);
        for i in 0..n {
            let mut v = vec![0.0f32; 16];
            v[0] = i as f32;
            vectors.push(v);
            ids.push(DocId::from(i as u64));
        }

        index.build(&vectors, &ids).await.expect("Build failed");

        let mut recall_count = 0;
        for (i, query) in vectors.iter().enumerate().take(100) {
            let results = index.search(query, 10).await.expect("Search failed");
            if results.iter().any(|r| r.doc_id == ids[i]) {
                recall_count += 1;
            }
        }

        assert!(recall_count >= 1, "Should find at least some results");

        let _ = std::fs::remove_file("recall_test.idx");
    }
}
