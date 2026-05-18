use crate::hnsw::{HnswIndex, VectorData};
use memfuse_core::Result;
use std::fs::File;
use std::io::Write;
use std::path::Path;
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
        let hb =
            bincode::serde::encode_to_vec(&header, bincode::config::standard()).map_err(|e| {
                memfuse_core::MemFuseError::Storage(format!("Header serialization failed: {}", e))
            })?;
        file.write_all(&(hb.len() as u32).to_le_bytes())?;
        file.write_all(&hb)?;
        for node in nodes {
            file.write_all(&node.doc_id.inner().to_le_bytes())?;
            match &node.vector {
                VectorData::F32(v) => {
                    for &val in v {
                        file.write_all(&val.to_le_bytes())?;
                    }
                }
                VectorData::U8(v) => {
                    file.write_all(&[0u8; 8])?;
                    file.write_all(v)?;
                }
            }
            let ns = node.connections.first().cloned().unwrap_or_default();
            file.write_all(&(ns.len() as u32).to_le_bytes())?;
            for i in 0..hnsw.config.m {
                file.write_all(&(if i < ns.len() { ns[i] as i32 } else { -1 }).to_le_bytes())?;
            }
        }
        file.sync_all()?;
        Ok(Self {
            path: path.as_ref().to_path_buf(),
            header,
        })
    }
}
