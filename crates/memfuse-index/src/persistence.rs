// FILE-CONTEXT
// ZWECK: Persistenz-Schicht für HNSW-Dateiserialisierung (`.hnsw`) und mmap-basiertes Lesen.
// INVARIANTEN: Zero-Panic bei Deserialisierung; mmap-Reads überleben POSIX file replace.
// NICHT-OFFENSICHTLICH: MmapIndex hält read-only FD; Schreibvorgänge laufen atomar über .tmp und Rename.
// HOTSPOTS: persistence.rs (HnswHeader::try_from_bytes, MmapIndex::open)
// STAND: TS:2026-08-30T18:53:53Z (SESSION: 37b1d991)

// ANCHOR[DEBT:WP-0.0-ZEROPANIC] STATUS:DONE (TS:2026-06-01T00:00:00Z) — Eradicate .unwrap() in persistence.rs
// TEST: grep -c ".unwrap()" crates/memfuse-index/src/persistence.rs
// DONE: Alle .unwrap() Aufrufe auf try_into() sind durch ? ersetzt.
//! HNSW Persistence Layer — Serialisierung und mmap-Mapping für Vektor-Indizes.
//!
//! Dieses Modul implementiert das `.hnsw` Dateiformat, das für das Offloading von
//! Vektoren auf die Festplatte optimiert ist, um den RAM-Verbrauch auf 8GB-Systemen zu minimieren.

use memfuse_core::{MemFuseError, Result};

/// Magic number for HNSW files (0x484E5357 = "HNSW").
pub const HNSW_MAGIC: u32 = 0x484E5357;
/// Current file format version.
pub const HNSW_VERSION: u16 = 1;

/// The header of an HNSW persistent file.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HnswHeader {
    pub magic: u32,
    pub version: u16,
    pub dimension: u32,
    pub m: u32,
    pub metric: u8,
    pub quantized: u8,
    pub q_min: f32, // Added for ScalarQuantizer
    pub q_max: f32, // Added for ScalarQuantizer
    pub node_count: u64,
    pub entry_point: i64,
    pub nodes_offset: u64,
    pub connections_offset: u64,
    pub last_tx_id: u64, // Added for Repair-on-Open
}

impl HnswHeader {
    pub const SIZE: usize = 64;

    pub fn try_from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < Self::SIZE {
            return Err(MemFuseError::Storage("Header too small".into()));
        }

        let magic = u32::from_le_bytes(
            bytes[0..4]
                .try_into()
                .map_err(|_| MemFuseError::Storage("Invalid magic offset".into()))?,
        );
        if magic != HNSW_MAGIC {
            return Err(MemFuseError::Storage(
                "Not a valid HNSW file: bad magic".into(),
            ));
        }

        Ok(Self {
            magic,
            version: u16::from_le_bytes(
                bytes[4..6]
                    .try_into()
                    .map_err(|_| MemFuseError::Storage("Invalid version offset".into()))?,
            ),
            dimension: u32::from_le_bytes(
                bytes[6..10]
                    .try_into()
                    .map_err(|_| MemFuseError::Storage("Invalid dimension offset".into()))?,
            ),
            m: u32::from_le_bytes(
                bytes[10..14]
                    .try_into()
                    .map_err(|_| MemFuseError::Storage("Invalid m offset".into()))?,
            ),
            metric: bytes[14],
            quantized: bytes[15],
            q_min: f32::from_le_bytes(
                bytes[16..20]
                    .try_into()
                    .map_err(|_| MemFuseError::Storage("Invalid q_min offset".into()))?,
            ),
            q_max: f32::from_le_bytes(
                bytes[20..24]
                    .try_into()
                    .map_err(|_| MemFuseError::Storage("Invalid q_max offset".into()))?,
            ),
            node_count: u64::from_le_bytes(
                bytes[24..32]
                    .try_into()
                    .map_err(|_| MemFuseError::Storage("Invalid node_count offset".into()))?,
            ),
            entry_point: i64::from_le_bytes(
                bytes[32..40]
                    .try_into()
                    .map_err(|_| MemFuseError::Storage("Invalid entry_point offset".into()))?,
            ),
            nodes_offset: u64::from_le_bytes(
                bytes[40..48]
                    .try_into()
                    .map_err(|_| MemFuseError::Storage("Invalid nodes_offset offset".into()))?,
            ),
            connections_offset: u64::from_le_bytes(
                bytes[48..56].try_into().map_err(|_| {
                    MemFuseError::Storage("Invalid connections_offset offset".into())
                })?,
            ),
            last_tx_id: u64::from_le_bytes(
                bytes[56..64]
                    .try_into()
                    .map_err(|_| MemFuseError::Storage("Invalid last_tx_id offset".into()))?,
            ),
        })
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        buf[0..4].copy_from_slice(&self.magic.to_le_bytes());
        buf[4..6].copy_from_slice(&self.version.to_le_bytes());
        buf[6..10].copy_from_slice(&self.dimension.to_le_bytes());
        buf[10..14].copy_from_slice(&self.m.to_le_bytes());
        buf[14] = self.metric;
        buf[15] = self.quantized;
        buf[16..20].copy_from_slice(&self.q_min.to_le_bytes());
        buf[20..24].copy_from_slice(&self.q_max.to_le_bytes());
        buf[24..32].copy_from_slice(&self.node_count.to_le_bytes());
        buf[32..40].copy_from_slice(&self.entry_point.to_le_bytes());
        buf[40..48].copy_from_slice(&self.nodes_offset.to_le_bytes());
        buf[48..56].copy_from_slice(&self.connections_offset.to_le_bytes());
        buf[56..64].copy_from_slice(&self.last_tx_id.to_le_bytes());
        buf
    }
}

/// Represents a node's metadata in the flat file.
#[derive(Debug, Clone, Copy)]
pub struct NodeRecord {
    pub doc_id: u64,
    pub max_layer: u8,
    pub vector_offset: u64,
    pub connections_offset: u64,
}

impl NodeRecord {
    pub const SIZE: usize = 8 + 1 + 8 + 8; // 25 bytes

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < Self::SIZE {
            return Err(MemFuseError::Storage("NodeRecord bytes too small".into()));
        }
        Ok(Self {
            doc_id: u64::from_le_bytes(
                bytes[0..8]
                    .try_into()
                    .map_err(|_| MemFuseError::Storage("Invalid doc_id offset".into()))?,
            ),
            max_layer: bytes[8],
            vector_offset: u64::from_le_bytes(
                bytes[9..17]
                    .try_into()
                    .map_err(|_| MemFuseError::Storage("Invalid vector_offset offset".into()))?,
            ),
            connections_offset: u64::from_le_bytes(
                bytes[17..25].try_into().map_err(|_| {
                    MemFuseError::Storage("Invalid connections_offset offset".into())
                })?,
            ),
        })
    }

    pub fn to_bytes(&self) -> [u8; 25] {
        let mut buf = [0u8; 25];
        buf[0..8].copy_from_slice(&self.doc_id.to_le_bytes());
        buf[8] = self.max_layer;
        buf[9..17].copy_from_slice(&self.vector_offset.to_le_bytes());
        buf[17..25].copy_from_slice(&self.connections_offset.to_le_bytes());
        buf
    }
}

#[derive(Clone)]
/// A reader for memory-mapped HNSW indices.
pub struct MmapIndex {
    pub mmap: std::sync::Arc<memmap2::Mmap>,
    pub header: HnswHeader,
}

impl MmapIndex {
    #[allow(unsafe_code)]
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let file = std::fs::File::open(path)
            .map_err(|e| MemFuseError::Storage(format!("Failed to open HNSW file: {}", e)))?;

        // SAFETY: Invariant: `file` is a valid open file descriptor to a read-only persisted HNSW file, and the mapping remains valid for the entire duration of `MmapIndex`.
        //         Guarantor: `std::fs::File::open` successfully returned a valid file handle above.
        //         Lifetime: Wrapping `memmap2::Mmap` inside `Arc<Mmap>` ensures the memory mapping stays alive as long as `MmapIndex` or any clone exists.
        //         POSIX Unlink/Truncation & SIGBUS Defense: atomic rename on `save()` creates a new temp file and replaces the path, never truncating the file in place. On POSIX, deleting or replacing an open file retains the active file descriptor and mmap region intact until dropped.
        //         UB Prevention: Opening as read-only mapping (`Mmap::map`) prevents data races or undefined behavior from writes.
        //         ADR-017: Memory mapping permitted in `persistence.rs`.
        let mmap = unsafe { memmap2::Mmap::map(&file) } // SAFETY: 1. Invariant: Valid file descriptor and immutable mapping. 2. Guarantor: std::fs::File & atomic rename. 3. Call-site verified. 4. ADR-017 mmap.
            .map_err(|e| MemFuseError::Storage(format!("Failed to mmap HNSW: {}", e)))?;

        let header = HnswHeader::try_from_bytes(&mmap[0..HnswHeader::SIZE])?;
        Ok(Self {
            mmap: std::sync::Arc::new(mmap),
            header,
        })
    }

    /// Asynchronously opens an HNSW file using `spawn_blocking`.
    pub async fn open_async(path: impl AsRef<std::path::Path> + Send) -> Result<Self> {
        let path_buf = path.as_ref().to_path_buf();
        tokio::task::spawn_blocking(move || Self::open(path_buf))
            .await
            .map_err(|e| MemFuseError::Storage(format!("Join error: {}", e)))?
    }

    pub fn get_node_record(&self, index: usize) -> Result<NodeRecord> {
        let offset = self.header.nodes_offset as usize + index * NodeRecord::SIZE;
        if offset + NodeRecord::SIZE > self.mmap.len() {
            return Err(MemFuseError::Storage("Node record out of bounds".into()));
        }
        NodeRecord::from_bytes(&self.mmap[offset..offset + NodeRecord::SIZE])
    }

    pub fn get_vector(&self, record: &NodeRecord) -> Result<&[u8]> {
        let dim = self.header.dimension as usize;
        let size = if self.header.quantized != 0 {
            dim
        } else {
            dim * 4
        };
        let offset = record.vector_offset as usize;
        if offset + size > self.mmap.len() {
            return Err(MemFuseError::Storage("Vector data out of bounds".into()));
        }
        Ok(&self.mmap[offset..offset + size])
    }

    pub fn get_connections(&self, record: &NodeRecord, layer: usize) -> Result<Vec<u32>> {
        let offset = record.connections_offset as usize;
        if offset >= self.mmap.len() {
            return Ok(Vec::new());
        }

        let num_layers = self.mmap[offset] as usize;
        if layer >= num_layers {
            return Ok(Vec::new());
        }

        let mut current_pos = offset + 1;
        for _ in 0..layer {
            if current_pos + 4 > self.mmap.len() {
                return Ok(Vec::new());
            }
            let len = u32::from_le_bytes(
                self.mmap[current_pos..current_pos + 4]
                    .try_into()
                    .map_err(|_| MemFuseError::Storage("Corrupt connection length".into()))?,
            ) as usize;
            current_pos += 4 + len * 4;
        }

        if current_pos + 4 > self.mmap.len() {
            return Ok(Vec::new());
        }

        let len = u32::from_le_bytes(
            self.mmap[current_pos..current_pos + 4]
                .try_into()
                .map_err(|_| MemFuseError::Storage("Corrupt connection length".into()))?,
        ) as usize;
        let start = current_pos + 4;
        let end = start + len * 4;

        if end > self.mmap.len() {
            return Err(MemFuseError::Storage(
                "Connection data out of bounds".into(),
            ));
        }

        let raw = &self.mmap[start..end];
        let mut connections = Vec::with_capacity(len);
        for i in 0..len {
            let val = u32::from_le_bytes(
                raw[i * 4..(i + 1) * 4]
                    .try_into()
                    .map_err(|_| MemFuseError::Storage("Corrupt connection value".into()))?,
            );
            connections.push(val);
        }

        Ok(connections)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mmap_open_async() -> memfuse_core::Result<()> {
        use crate::hnsw::{HnswConfig, HnswIndex};
        use memfuse_core::{DistanceMetric, DocId, TxId, VectorIndex};

        let temp_dir = tempfile::tempdir().map_err(|e| MemFuseError::Storage(e.to_string()))?;
        let path = temp_dir.path().join("test_async.hnsw");

        // 1. Write an HNSW index with 100 vectors of dimension 4
        let config = HnswConfig {
            dimension: 4,
            m: 16,
            ef_construction: 200,
            ef_search: 64,
            distance_metric: DistanceMetric::Euclidean,
            ..Default::default()
        };
        let index = HnswIndex::try_new(config.clone())?;
        let tx = TxId::new(1);

        let mut vectors = Vec::with_capacity(100);
        for i in 1..=100u64 {
            let vec = vec![i as f32, (i * 2) as f32, (i * 3) as f32, (i * 4) as f32];
            index.insert(tx, DocId::new(i), &vec).await?;
            vectors.push((DocId::new(i), vec));
        }
        index.commit(tx).await?;

        // 2. Save to disk
        index.save(&path).await?;

        // 3. Open via mmap
        let mmap_index = HnswIndex::try_new(config)?;
        mmap_index.load_mmap(&path).await?;

        // 4. Search for 5 nearest neighbors of a query vector
        let query = vec![50.1, 100.2, 150.3, 200.4];
        let search_results = mmap_index.search(&query, 5).await?;
        assert_eq!(search_results.len(), 5);

        // 5. Verify the returned doc_ids match expected nearest neighbors by brute force
        let mut brute_force: Vec<(DocId, f32)> = vectors
            .iter()
            .map(|(doc_id, v)| {
                let dist: f32 = v
                    .iter()
                    .zip(query.iter())
                    .map(|(a, b)| (a - b).powi(2))
                    .sum::<f32>()
                    .sqrt();
                (*doc_id, dist)
            })
            .collect();
        brute_force.sort_by(|a, b| a.1.total_cmp(&b.1));

        let expected_doc_ids: Vec<DocId> = brute_force.iter().take(5).map(|(id, _)| *id).collect();
        let returned_doc_ids: Vec<DocId> = search_results.iter().map(|r| r.doc_id).collect();

        assert_eq!(returned_doc_ids, expected_doc_ids);
        Ok(())
    }
}
