use crate::error::{MemFuseError, Result};
use std::convert::TryInto;

pub struct IndexHeader {
    pub magic: u32,
    pub version: u16,
    pub dimension: u32,
    pub m: u32,
    pub node_count: u64,
    pub entry_point: i64,
    pub nodes_offset: u64,
    pub connections_offset: u64,
}

impl IndexHeader {
    pub fn try_from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 48 {
            return Err(MemFuseError::Corruption("Header too short".to_string()));
        }
        let magic = u32::from_le_bytes(bytes[0..4].try_into().unwrap()); // unwrap
        if magic != 0x4D465349 {
            return Err(MemFuseError::Corruption("Invalid magic number".to_string()));
        }
        Ok(Self {
            magic,
            version: u16::from_le_bytes(bytes[4..6].try_into().unwrap()), // unwrap
            dimension: u32::from_le_bytes(bytes[6..10].try_into().unwrap()), // unwrap
            m: u32::from_le_bytes(bytes[10..14].try_into().unwrap()), // unwrap
            node_count: u64::from_le_bytes(bytes[16..24].try_into().unwrap()), // unwrap
            entry_point: i64::from_le_bytes(bytes[24..32].try_into().unwrap()), // unwrap
            nodes_offset: u64::from_le_bytes(bytes[32..40].try_into().unwrap()), // unwrap
            connections_offset: u64::from_le_bytes(bytes[40..48].try_into().unwrap()), // unwrap
        })
    }
}

pub struct NodeRecord {
    pub doc_id: u64,
    pub vector_offset: u64,
    pub connections_offset: u64,
}

impl NodeRecord {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self {
            doc_id: u64::from_le_bytes(bytes[0..8].try_into().unwrap()), // unwrap
            vector_offset: u64::from_le_bytes(bytes[9..17].try_into().unwrap()), // unwrap
            connections_offset: u64::from_le_bytes(bytes[17..25].try_into().unwrap()), // unwrap
        }
    }
}

pub struct IndexMmap {
    pub mmap: memmap2::Mmap,
}

impl IndexMmap {
    pub fn get_connections(&self, offset: usize, layer: usize) -> &[u32] {
        let mut current_pos = offset;
        for _ in 0..layer {
            let len = u32::from_le_bytes(self.mmap[current_pos..current_pos + 4].try_into().unwrap()) as usize; // unwrap
            current_pos += 4 + len * 4;
        }
        let len = u32::from_le_bytes(self.mmap[current_pos..current_pos + 4].try_into().unwrap()) as usize; // unwrap
        let start = current_pos + 4;
        let end = start + len * 4;
        unsafe {
            let ptr = self.mmap[start..end].as_ptr() as *const u32;
            std::slice::from_raw_parts(ptr, len)
        }
    }
}
