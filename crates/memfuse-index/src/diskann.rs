//! DiskANN Out-of-Core Search implementation.
// ANCHOR:ARCH:DISKANN-001 — DiskANN Out-of-Core Search Foundation.
// WP:WP-4.3 PRIO:3 NEEDS:WP-2.2, WP-4.1
// AGENT:03 DATE:2026-05-19 STATUS:READY
// ZIEL: HNSW-Graph auf SSD für Datasets > RAM.
// DESIGN: 16-byte aligned nodes, SQ8 quantized vectors.

/// Header for a node stored on disk.
#[repr(C, align(16))]
pub struct DiskNodeHeader {
    /// Document ID.
    pub doc_id: u64,
    /// Number of neighbors.
    pub num_neighbors: u32,
    /// Vector dimension.
    pub dimension: u32,
    /// Padding to ensure 16-byte alignment and for SQ8 mode vectors.
    pub padding: [u8; 8],
}

/// Skeleton for DiskANN search.
pub struct DiskAnnSearcher {
    // To be implemented: mmap, index structure, etc.
}

impl DiskAnnSearcher {
    /// Opens a DiskANN index.
    pub fn open(_path: &str) -> memfuse_core::Result<Self> {
        Ok(Self {})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diskann_skeleton_open() {
        let searcher = DiskAnnSearcher::open("dummy_path");
        assert!(searcher.is_ok());
    }
}
