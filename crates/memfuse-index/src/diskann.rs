//! DiskANN Out-of-Core Search Implementation.
// ANCHOR:ARCH:DISKANN-001 — Disk-Resident Graph Search.
// WP:WP-4.3 PRIO:3 NEEDS:WP-2.2
// AGENT:03 DATE:2026-05-18 STATUS:WIP
// ZIEL: Recall@10 >= 90% für Datasets die RAM übersteigen.

#![allow(unsafe_code)]

use memfuse_core::{DistanceMetric, DocId, Result, ScoredDocument, MemFuseError};
use crate::distance::{compute_distance, mmap_file, cast_slice_f32, cast_slice_u32};
use crate::hnsw::{HnswIndex, VectorData};
use std::fs::File;
use std::io::{Write, BufWriter};
use memmap2::Mmap;
use std::collections::BinaryHeap;
use std::cmp::Reverse;
use ahash::AHashSet;

/// Configuration for DiskANN Out-of-Core search.
#[derive(Debug, Clone)]
pub struct DiskHnswConfig {
    pub beam_width: usize,
    pub distance_metric: DistanceMetric,
    pub dimension: usize,
    /// Whether the index is quantized (SQ8).
    pub quantize: bool,
}

/// A disk-resident HNSW index using Beam Search.
pub struct DiskHnsw {
    config: DiskHnswConfig,
    mmap: Mmap,
    _file: File,
    num_nodes: usize,
    entry_point: usize,
    max_m: usize,
    quantizer: Option<crate::quantize::ScalarQuantizer>,
}

/// Structure of a node as stored on disk.
/// Layout: [DocId (8b)] [Vector (dim * 4b OR dim * 1b)] [Num Neighbors (4b)] [Neighbor Indices (max_m * 4b)]
#[derive(Debug)]
pub struct DiskNode<'a> {
    pub doc_id: DocId,
    pub vector: DiskVector<'a>,
    pub neighbors: &'a [u32],
}

#[derive(Debug)]
pub enum DiskVector<'a> {
    F32(&'a [f32]),
    U8(&'a [u8]),
}

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
        self.distance.total_cmp(&other.distance)
    }
}

impl DiskHnsw {
    /// Loads a DiskANN index from a file using mmap.
    pub fn open(path: &str, config: DiskHnswConfig) -> Result<Self> {
        let file = File::open(path)?;
        // ANCHOR:SAFETY:SIMD-DISK-003 — Proxy to distance.rs
        // BEGRÜNDUNG: mmap wird für read-only Zugriff auf die Index-Datei verwendet.
        let mmap = unsafe { mmap_file(&file)? };

        // Header layout: [Magic (4b)] [Num Nodes (8b)] [Entry Point (8b)] [Max M (4b)] [Quantized (1b)] [Quantizer Data (if quantized)]
        if mmap.len() < 32 {
             return Err(MemFuseError::Storage("Corrupt DiskANN file: header too short".into()));
        }

        if &mmap[0..4] != b"DANN" {
            return Err(MemFuseError::Storage("Invalid DiskANN file magic".into()));
        }

        let num_nodes = u64::from_le_bytes(mmap[4..12].try_into().map_err(|_| MemFuseError::Storage("Failed to read num_nodes".into()))?) as usize;
        let entry_point = u64::from_le_bytes(mmap[12..20].try_into().map_err(|_| MemFuseError::Storage("Failed to read entry_point".into()))?) as usize;
        let max_m = u32::from_le_bytes(mmap[20..24].try_into().map_err(|_| MemFuseError::Storage("Failed to read max_m".into()))?) as usize;
        let is_quantized = mmap[24] != 0;

        let mut quantizer = None;

        if is_quantized {
            // Quantizer data: [min (4f)] [max (4f)] [scale (4f)] [inv_scale (4f)] [dimension (8b)]
            let q_offset = 32;
            if mmap.len() < q_offset + 24 {
                return Err(MemFuseError::Storage("Corrupt DiskANN file: missing quantizer data".into()));
            }
            let min = f32::from_le_bytes(mmap[q_offset..q_offset+4].try_into().unwrap());
            let max = f32::from_le_bytes(mmap[q_offset+4..q_offset+8].try_into().unwrap());
            let scale = f32::from_le_bytes(mmap[q_offset+8..q_offset+12].try_into().unwrap());
            let inv_scale = f32::from_le_bytes(mmap[q_offset+12..q_offset+16].try_into().unwrap());
            let dimension = u64::from_le_bytes(mmap[q_offset+16..q_offset+24].try_into().unwrap()) as usize;

            quantizer = Some(crate::quantize::ScalarQuantizer {
                min,
                max,
                scale,
                inv_scale,
                dimension,
            });
        }

        Ok(Self {
            config,
            mmap,
            _file: file,
            num_nodes,
            entry_point,
            max_m,
            quantizer,
        })
    }

    /// Serializes an in-memory HnswIndex to a DiskANN file.
    pub fn save_from_hnsw(index: &HnswIndex, path: &str) -> Result<()> {
        let file = File::create(path)?;
        let mut writer = BufWriter::new(file);

        let nodes = index.get_nodes_for_diskann();
        let num_nodes = nodes.len();
        let entry_point = index.get_entry_point_for_diskann().unwrap_or(0);
        let config = index.get_config_for_diskann();
        let max_m = config.m * 2; // HNSW typically uses 2*M for layer 0

        // Write Header: [Magic (4b)] [Num Nodes (8b)] [Entry Point (8b)] [Max M (4b)] [Quantized (1b)] [Padding (7b)]
        writer.write_all(b"DANN")?;
        writer.write_all(&(num_nodes as u64).to_le_bytes())?;
        writer.write_all(&(entry_point as u64).to_le_bytes())?;
        writer.write_all(&(max_m as u32).to_le_bytes())?;
        writer.write_all(&[config.quantize as u8])?;
        writer.write_all(&[0u8; 7])?; // Padding to 32 bytes

        if config.quantize {
            let q = index.get_quantizer_for_diskann().ok_or_else(|| {
                MemFuseError::Index("Quantizer missing for quantized index".into())
            })?;
            writer.write_all(&q.min.to_le_bytes())?;
            writer.write_all(&q.max.to_le_bytes())?;
            writer.write_all(&q.scale.to_le_bytes())?;
            writer.write_all(&q.inv_scale.to_le_bytes())?;
            writer.write_all(&(q.dimension as u64).to_le_bytes())?;
            writer.write_all(&[0u8; 8])?; // Padding to keep nodes aligned to 32
        }

        for node in nodes {
            // [DocId (8b)]
            writer.write_all(&node.doc_id.inner().to_le_bytes())?;

            // [Vector (dim * 4b OR dim * 1b)]
            match &node.vector {
                VectorData::F32(v) => {
                    for &val in v {
                        writer.write_all(&val.to_le_bytes())?;
                    }
                }
                VectorData::U8(v) => {
                    writer.write_all(v)?;
                }
            }

            // [Num Neighbors (4b)] [Neighbor Indices (max_m * 4b)]
            let neighbors = &node.connections[0];
            let num_neighbors = neighbors.len().min(max_m);
            writer.write_all(&(num_neighbors as u32).to_le_bytes())?;

            for (i, &neighbor) in neighbors.iter().enumerate().take(max_m) {
                writer.write_all(&(neighbor as u32).to_le_bytes())?;
                if i == max_m - 1 { break; }
            }
            for _ in num_neighbors..max_m {
                writer.write_all(&0u32.to_le_bytes())?;
            }

            // Node padding to ensure next node is aligned to 32
            let vector_size = if config.quantize { config.dimension } else { config.dimension * 4 };
            let node_bytes = 8 + vector_size + 4 + (max_m * 4);
            let padding = (32 - (node_bytes % 32)) % 32;
            if padding > 0 {
                writer.write_all(&vec![0u8; padding])?;
            }
        }

        writer.flush()?;
        Ok(())
    }

    fn get_node(&self, idx: usize) -> Result<DiskNode<'_>> {
        if idx >= self.num_nodes {
            return Err(MemFuseError::Storage("Node index out of bounds".into()));
        }

        let header_size = if self.config.quantize { 32 + 32 } else { 32 };
        let vector_size = if self.config.quantize { self.config.dimension } else { self.config.dimension * 4 };
        let node_data_size = 8 + vector_size + 4 + (self.max_m * 4);
        let padding = (32 - (node_data_size % 32)) % 32;
        let node_size = node_data_size + padding;

        let offset = header_size + (idx * node_size);

        if offset + node_data_size > self.mmap.len() {
            return Err(MemFuseError::Storage("Mmap access out of bounds".into()));
        }

        let doc_id_raw = u64::from_le_bytes(self.mmap[offset..offset+8].try_into().map_err(|_| MemFuseError::Storage("Failed to read doc_id".into()))?);
        let doc_id = DocId::new(doc_id_raw);

        let vector_start = offset + 8;
        let vector_end = vector_start + vector_size;

        let vector = if self.config.quantize {
            DiskVector::U8(&self.mmap[vector_start..vector_end])
        } else {
            // SAFETY: Casting logic moved to distance.rs
            DiskVector::F32(unsafe { cast_slice_f32(&self.mmap[vector_start..vector_end]) })
        };

        let neighbors_count_start = vector_end;
        let num_neighbors = u32::from_le_bytes(self.mmap[neighbors_count_start..neighbors_count_start+4].try_into().map_err(|_| MemFuseError::Storage("Failed to read num_neighbors".into()))?) as usize;
        let neighbors_start = neighbors_count_start + 4;
        let neighbors_end = neighbors_start + (num_neighbors.min(self.max_m) * 4);

        // SAFETY: Casting logic moved to distance.rs
        let neighbors = unsafe { cast_slice_u32(&self.mmap[neighbors_start..neighbors_end]) };

        Ok(DiskNode {
            doc_id,
            vector,
            neighbors,
        })
    }

    fn compute_dist(&self, query: &[f32], node_vector: &DiskVector) -> Result<f32> {
        match node_vector {
            DiskVector::F32(v) => compute_distance(query, v, self.config.distance_metric),
            DiskVector::U8(v) => {
                let q = self.quantizer.as_ref().ok_or_else(|| MemFuseError::Index("Quantizer missing for DiskANN".into()))?;
                q.asymmetric_dist(query, v, self.config.distance_metric)
            }
        }
    }

    /// Performs a Beam Search on the disk-resident graph.
    pub fn beam_search(&self, query: &[f32], k: usize) -> Result<Vec<ScoredDocument>> {
        if self.num_nodes == 0 {
            return Ok(Vec::new());
        }

        let mut visited = AHashSet::new();
        let mut candidates = BinaryHeap::new();
        let mut results = BinaryHeap::new();

        let beam_width = self.config.beam_width.max(k);

        let ep_node = self.get_node(self.entry_point)?;
        let ep_dist = self.compute_dist(query, &ep_node.vector)?;
        let ep_cand = Candidate { index: self.entry_point, distance: ep_dist };

        candidates.push(Reverse(ep_cand));
        results.push(ep_cand);
        visited.insert(self.entry_point);

        while let Some(Reverse(current)) = candidates.pop() {
            if let Some(worst_result) = results.peek() {
                if current.distance > worst_result.distance && results.len() >= beam_width {
                    break;
                }
            }

            let node = self.get_node(current.index)?;
            for &neighbor_idx in node.neighbors {
                let neighbor_idx = neighbor_idx as usize;
                if visited.insert(neighbor_idx) {
                    let neighbor_node = self.get_node(neighbor_idx)?;
                    let dist = self.compute_dist(query, &neighbor_node.vector)?;

                    let is_better = match results.peek() {
                        Some(worst) => dist < worst.distance,
                        None => true,
                    };

                    if results.len() < beam_width || is_better {
                        let cand = Candidate { index: neighbor_idx, distance: dist };
                        candidates.push(Reverse(cand));
                        results.push(cand);
                        if results.len() > beam_width {
                            results.pop();
                        }
                    }
                }
            }
        }

        let mut final_results = Vec::new();
        let sorted_results = results.into_sorted_vec();
        for cand in sorted_results.iter().take(k) {
            let node = self.get_node(cand.index)?;
            let score = match self.config.distance_metric {
                DistanceMetric::Cosine => 1.0 - cand.distance,
                DistanceMetric::Euclidean => 1.0 / (1.0 + cand.distance),
                DistanceMetric::DotProduct => -cand.distance,
            };
            final_results.push(ScoredDocument::new(node.doc_id, score));
        }

        Ok(final_results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hnsw::{HnswConfig, HnswIndex};
    use memfuse_core::{DocId, TxId, VectorIndex};
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_diskann_save_load_search() {
        let dim = 4;
        let config = HnswConfig {
            dimension: dim,
            m: 8,
            ef_construction: 100,
            ..Default::default()
        };
        let index = HnswIndex::new(config);
        let tx = TxId::new(1);

        index.insert(tx, DocId::new(1), &[1.0, 0.0, 0.0, 0.0]).await.unwrap();
        index.insert(tx, DocId::new(2), &[0.0, 1.0, 0.0, 0.0]).await.unwrap();
        index.insert(tx, DocId::new(3), &[0.0, 0.0, 1.0, 0.0]).await.unwrap();
        index.commit(tx).await.unwrap();

        let tmp_file = NamedTempFile::new().unwrap();
        let path = tmp_file.path().to_str().unwrap();

        DiskHnsw::save_from_hnsw(&index, path).unwrap();

        let disk_config = DiskHnswConfig {
            beam_width: 10,
            distance_metric: DistanceMetric::Cosine,
            dimension: dim,
            quantize: false,
        };

        let disk_index = DiskHnsw::open(path, disk_config).unwrap();
        assert_eq!(disk_index.num_nodes, 3);

        let results = disk_index.beam_search(&[1.0, 0.1, 0.0, 0.0], 1).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].doc_id, DocId::new(1));
    }

    #[tokio::test]
    async fn test_diskann_quantized() {
        let dim = 4;
        let config = HnswConfig {
            dimension: dim,
            m: 8,
            ef_construction: 100,
            quantize: true,
            ..Default::default()
        };
        let index = HnswIndex::new(config);
        let tx = TxId::new(1);

        // Need enough vectors to train quantizer
        for i in 1..=60u64 {
            let v = [i as f32, 0.0, 0.0, 0.0];
            index.insert(tx, DocId::new(i), &v).await.unwrap();
        }
        index.commit(tx).await.unwrap();

        let tmp_file = NamedTempFile::new().unwrap();
        let path = tmp_file.path().to_str().unwrap();

        DiskHnsw::save_from_hnsw(&index, path).unwrap();

        let disk_config = DiskHnswConfig {
            beam_width: 10,
            distance_metric: DistanceMetric::Euclidean,
            dimension: dim,
            quantize: true,
        };

        let disk_index = DiskHnsw::open(path, disk_config).unwrap();
        assert!(disk_index.quantizer.is_some());

        let results = disk_index.beam_search(&[60.0, 0.0, 0.0, 0.0], 1).unwrap();
        assert_eq!(results[0].doc_id, DocId::new(60));
    }
}
