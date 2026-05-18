//! DiskANN Out-of-Core Search implementation.
// ANCHOR:ARCH:DISKANN-001 — SSD-optimierte ANN Suche (WP-4.3).
// WP:WP-4.3 PRIO:4 NEEDS:WP-2.2,WP-4.1
// AGENT:03 DATE:2026-05-18 STATUS:WIP
// CREATED:2026-05-18 DEADLINE:NONE
// KERNIDEE: HNSW-Struktur auf Disk/SSD via mmap. Beam-Search statt Greedy.
// LAYOUT: [HEADER] [NODE 0] [NODE 1] ...
// NODE-LAYOUT: [DOCID:8b] [PADDING] [VECTOR] [M_COUNT:4b] [NEIGHBORS:M*4b]

use crate::hnsw::{HnswIndex, VectorData};
use memfuse_core::Result;
use std::fs::File;
use std::io::Write;
use std::path::Path;

/// DiskANN-based index for datasets larger than RAM.
#[allow(dead_code)]
pub struct DiskAnnIndex {
    path: std::path::PathBuf,
    header: DiskAnnHeader,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[allow(dead_code)]
struct DiskAnnHeader {
    magic: [u8; 8],
    version: u32,
    num_nodes: u32,
    dimension: u32,
    m: u32,
    entry_point: i32,
    is_quantized: bool,
}

impl DiskAnnIndex {
    /// Creates a new DiskAnnIndex by serializing an existing HNSW index.
    pub fn build_from_hnsw<P: AsRef<Path>>(hnsw: &HnswIndex, path: P) -> Result<Self> {
        let nodes = hnsw.get_nodes_for_diskann();
        let ep = hnsw.get_entry_point_for_diskann();
        let mut file = File::create(&path)?;

        let header = DiskAnnHeader {
            magic: *b"MEMFUSE\0",
            version: 1,
            num_nodes: nodes.len() as u32,
            dimension: hnsw.config.dimension as u32,
            m: hnsw.config.m as u32,
            entry_point: ep.map(|x| x as i32).unwrap_or(-1),
            is_quantized: hnsw.config.quantize,
        };

        // Write header
        let header_bytes = bincode::serde::encode_to_vec(&header, bincode::config::standard())
            .map_err(|e| {
                memfuse_core::MemFuseError::Storage(format!("Header serialization failed: {}", e))
            })?;

        let mut header_len_buf = [0u8; 4];
        header_len_buf.copy_from_slice(&(header_bytes.len() as u32).to_le_bytes());
        file.write_all(&header_len_buf)?;
        file.write_all(&header_bytes)?;

        // Write nodes
        for node in nodes {
            // [DocId (8b)]
            file.write_all(&node.doc_id.inner().to_le_bytes())?;

            // [Vector Data]
            match &node.vector {
                VectorData::F32(v) => {
                    for &val in v {
                        file.write_all(&val.to_le_bytes())?;
                    }
                }
                VectorData::U8(v) => {
                    // SQ8 nodes need 8-byte alignment for the vector start if we want to use mmap efficiently.
                    // DocId is 8 bytes, so we are aligned.
                    // BUT: We need a padding BEFORE vector if we want ALIGNMENT within the file.
                    // Assuming we want the entire node to be nicely packed.
                    // [DocId (8)] [Vector (dim * 1)] [NeighborCount (4)] [Neighbors (M * 4)]
                    // If is_quantized, we add 8 bytes of padding after DocId to align vector start to 16-byte boundary
                    // OR we just write it and rely on unaligned loads which are fast on modern CPUs.
                    // Given WP-4.1 (mmap), alignment helps.
                    let padding_size = 8;
                    file.write_all(&[0u8; 8][..padding_size])?;
                    file.write_all(v)?;
                }
            }

            // [Neighbor Count (4b)]
            // Only Layer 0 is stored for DiskANN.
            let neighbors = node.connections.first().cloned().unwrap_or_default();
            file.write_all(&(neighbors.len() as u32).to_le_bytes())?;

            // [Neighbor Indices (max_m * 4b)]
            // We write exactly max_m indices for fixed-size node records.
            for i in 0..hnsw.config.m {
                let neighbor_idx = if i < neighbors.len() {
                    neighbors[i] as i32
                } else {
                    -1
                };
                file.write_all(&neighbor_idx.to_le_bytes())?;
            }
        }

        file.sync_all()?;
        Ok(Self {
            path: path.as_ref().to_path_buf(),
            header,
        })
    }
}
