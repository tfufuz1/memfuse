#![allow(unsafe_code)]
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

        let magic = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) // unwrap allowed (AGENT:05); // unwrap allowed (AGENT:05)
        if magic != HNSW_MAGIC {
            return Err(MemFuseError::Storage("Invalid HNSW magic".into()));
        }

        Ok(Self {
            magic,
            version: u16::from_le_bytes(bytes[4..6].try_into().unwrap()) // unwrap allowed (AGENT:05), // unwrap allowed (AGENT:05)
            dimension: u32::from_le_bytes(bytes[6..10].try_into().unwrap()) // unwrap allowed (AGENT:05), // unwrap allowed (AGENT:05)
            m: u32::from_le_bytes(bytes[10..14].try_into().unwrap()) // unwrap allowed (AGENT:05), // unwrap allowed (AGENT:05)
            metric: bytes[14],
            quantized: bytes[15],
            q_min: f32::from_le_bytes(bytes[16..20].try_into().unwrap()) // unwrap allowed (AGENT:05), // unwrap allowed (AGENT:05)
            q_max: f32::from_le_bytes(bytes[20..24].try_into().unwrap()) // unwrap allowed (AGENT:05), // unwrap allowed (AGENT:05)
            node_count: u64::from_le_bytes(bytes[24..32].try_into().unwrap()) // unwrap allowed (AGENT:05), // unwrap allowed (AGENT:05)
            entry_point: i64::from_le_bytes(bytes[32..40].try_into().unwrap()) // unwrap allowed (AGENT:05), // unwrap allowed (AGENT:05)
            nodes_offset: u64::from_le_bytes(bytes[40..48].try_into().unwrap()) // unwrap allowed (AGENT:05), // unwrap allowed (AGENT:05)
            connections_offset: u64::from_le_bytes(bytes[48..56].try_into().unwrap()) // unwrap allowed (AGENT:05), // unwrap allowed (AGENT:05)
            last_tx_id: u64::from_le_bytes(bytes[56..64].try_into().unwrap()) // unwrap allowed (AGENT:05), // unwrap allowed (AGENT:05)
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

    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self {
            doc_id: u64::from_le_bytes(bytes[0..8].try_into().unwrap()) // unwrap allowed (AGENT:05), // unwrap allowed (AGENT:05)
            max_layer: bytes[8],
            vector_offset: u64::from_le_bytes(bytes[9..17].try_into().unwrap()) // unwrap allowed (AGENT:05), // unwrap allowed (AGENT:05)
            connections_offset: u64::from_le_bytes(bytes[17..25].try_into().unwrap()) // unwrap allowed (AGENT:05), // unwrap allowed (AGENT:05)
        }
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
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let file = std::fs::File::open(path)
            .map_err(|e| MemFuseError::Storage(format!("Failed to open HNSW file: {}", e)))?;
        let mmap = unsafe { memmap2::Mmap::map(&file) }
            .map_err(|e| MemFuseError::Storage(format!("Failed to mmap HNSW: {}", e)))?;

        let header = HnswHeader::try_from_bytes(&mmap[0..HnswHeader::SIZE])?;
        Ok(Self {
            mmap: std::sync::Arc::new(mmap),
            header,
        })
    }

    pub fn get_node_record(&self, index: usize) -> NodeRecord {
        let offset = self.header.nodes_offset as usize + index * NodeRecord::SIZE;
        NodeRecord::from_bytes(&self.mmap[offset..offset + NodeRecord::SIZE])
    }

    pub fn get_vector(&self, record: &NodeRecord) -> &[u8] {
        let dim = self.header.dimension as usize;
        let size = if self.header.quantized != 0 {
            dim
        } else {
            dim * 4
        };
        let offset = record.vector_offset as usize;
        &self.mmap[offset..offset + size]
    }

    pub fn get_connections(&self, record: &NodeRecord, layer: usize) -> Vec<u32> {
        let offset = record.connections_offset as usize;
        if offset >= self.mmap.len() {
            return Vec::new();
        }

        let num_layers = self.mmap[offset] as usize;
        if layer >= num_layers {
            return Vec::new();
        }

        let mut current_pos = offset + 1;
        for _ in 0..layer {
            let len =
                u32::from_le_bytes(self.mmap[current_pos..current_pos + 4].try_into().unwrap()) // unwrap allowed (AGENT:05)
                    as usize;
            current_pos += 4 + len * 4;
        }

        let len = u32::from_le_bytes(self.mmap[current_pos..current_pos + 4].try_into().unwrap()) // unwrap allowed (AGENT:05)
            as usize;
        let start = current_pos + 4;
        let end = start + len * 4;

        let raw = &self.mmap[start..end];
        let mut connections = Vec::with_capacity(len);
        for i in 0..len {
            let val = u32::from_le_bytes(raw[i * 4..(i + 1) * 4].try_into().unwrap()) // unwrap allowed (AGENT:05); // unwrap allowed (AGENT:05)
            connections.push(val);
        }

        // This is a temporary copy. For long-term performance, we should ensure alignment
        // in the file format or use a safe abstraction.
        // But for 8GB RAM remediation, this loop is acceptable.
        connections
    }
}
