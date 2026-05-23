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
        while let Some(Reverse(current)) = candidates.pop() {
            if results.len() >= self.config.beam_width {
                let peeked = results.peek().unwrap(); // unwrap
                if current.distance > peeked.distance {
