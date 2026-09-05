//! Persistence structures for HNSW Index (Zero-Copy compatible via rkyv).

use crate::hnsw::{HNSWConfig, HNSWIndex, HNSWIndexCore, HNSWNode};
use ahash::AHashMap;
use async_trait::async_trait;
use chimera_core::{DistanceMetric, DocId, NamespaceId, Persist, Result, TxBuffer};
use parking_lot::RwLock;
use rkyv::{Archive, Deserialize, Serialize};
use roaring::RoaringTreemap;
use std::path::Path;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use tokio::fs;

#[derive(Archive, Deserialize, Serialize, Debug)]
#[archive(check_bytes)]
pub struct PersistedHNSWConfig {
    pub dimension: usize,
    pub max_elements: usize,
    pub m: usize,
    pub ef_construction: usize,
    pub ef_search: usize,
    pub distance_metric: u8,
    pub rebuild_threshold: f64,
}

impl From<&HNSWConfig> for PersistedHNSWConfig {
    fn from(config: &HNSWConfig) -> Self {
        Self {
            dimension: config.dimension,
            max_elements: config.max_elements,
            m: config.m,
            ef_construction: config.ef_construction,
            ef_search: config.ef_search,
            distance_metric: match config.distance_metric {
                DistanceMetric::Cosine => 0,
                DistanceMetric::Euclidean => 1,
                DistanceMetric::DotProduct => 2,
            },
            rebuild_threshold: config.rebuild_threshold,
        }
    }
}

impl From<PersistedHNSWConfig> for HNSWConfig {
    fn from(val: PersistedHNSWConfig) -> Self {
        HNSWConfig {
            dimension: val.dimension,
            max_elements: val.max_elements,
            m: val.m,
            ef_construction: val.ef_construction,
            ef_search: val.ef_search,
            distance_metric: match val.distance_metric {
                0 => DistanceMetric::Cosine,
                1 => DistanceMetric::Euclidean,
                _ => DistanceMetric::DotProduct, // fallback
            },
            rebuild_threshold: val.rebuild_threshold,
            health_check_interval_secs: 30, // Default for restored indices
            tx_buffer_shard_count: 64,
            tx_timeout_secs: 60,
        }
    }
}

#[derive(Archive, Deserialize, Serialize, Debug)]
#[archive(check_bytes)]
pub struct PersistedHNSWNode {
    pub namespace_id: String,
    pub doc_id: u64,
    pub vector: Vec<f32>,
    pub connections: Vec<Vec<usize>>,
    pub max_layer: usize,
}

impl From<&HNSWNode> for PersistedHNSWNode {
    fn from(node: &HNSWNode) -> Self {
        Self {
            namespace_id: node.namespace_id.as_str().to_string(),
            doc_id: node.doc_id.0,
            vector: node.vector.clone(),
            connections: node.connections.clone(),
            max_layer: node.max_layer,
        }
    }
}

impl From<PersistedHNSWNode> for HNSWNode {
    fn from(val: PersistedHNSWNode) -> Self {
        HNSWNode {
            namespace_id: NamespaceId::new(val.namespace_id),
            doc_id: DocId(val.doc_id),
            vector: val.vector,
            connections: val.connections,
            max_layer: val.max_layer,
        }
    }
}

#[derive(Archive, Deserialize, Serialize, Debug)]
#[archive(check_bytes)]
pub struct HNSWIndexState {
    pub config: PersistedHNSWConfig,
    pub nodes: Vec<PersistedHNSWNode>,
    pub doc_to_node: Vec<(Vec<u8>, usize)>,
    pub entry_points: Vec<(String, usize)>,
    pub max_layers: Vec<(String, u64)>,
    pub deleted_nodes: Vec<u8>,
}

#[async_trait]
impl Persist for HNSWIndex {
    async fn save(&self, path: &Path) -> Result<()> {
        let inner = self.inner.clone();
        let bytes = tokio::task::spawn_blocking(move || -> Result<rkyv::util::AlignedVec> {
            let state = {
                let nodes_read = inner.nodes.read();
                let doc_to_node_read = inner.doc_to_node.read();
                let entry_points_read = inner.entry_points.read();
                let max_layers_read = inner.max_layers.read();
                let deleted_nodes = inner.deleted_nodes.read();
                let mut deleted_bytes = Vec::new();
                deleted_nodes
                    .serialize_into(&mut deleted_bytes)
                    .unwrap_or_default();

                HNSWIndexState {
                    config: (&inner.config).into(),
                    nodes: nodes_read.iter().map(|n| n.into()).collect(),
                    doc_to_node: doc_to_node_read
                        .iter()
                        .map(|(k, v)| (k.clone(), *v))
                        .collect(),
                    entry_points: entry_points_read
                        .iter()
                        .map(|(k, v)| (k.as_str().to_string(), *v))
                        .collect(),
                    max_layers: max_layers_read
                        .iter()
                        .map(|(k, v)| (k.as_str().to_string(), *v))
                        .collect(),
                    deleted_nodes: deleted_bytes,
                }
            };

            rkyv::to_bytes::<_, 1024>(&state).map_err(|e| {
                chimera_core::ChimeraError::Internal(format!(
                    "Failed to serialize HNSW index: {}",
                    e
                ))
            })
        })
        .await
        .map_err(|e| {
            chimera_core::ChimeraError::Internal(format!("Blocking task failed: {}", e))
        })??;

        fs::write(path, bytes).await.map_err(|e| {
            chimera_core::ChimeraError::Serialization(format!(
                "Failed to write HNSW index file: {}",
                e
            ))
        })?;

        Ok(())
    }

    async fn load(path: &Path) -> Result<Self> {
        let bytes = fs::read(path).await.map_err(|e| {
            chimera_core::ChimeraError::Serialization(format!(
                "Failed to read HNSW index file: {}",
                e
            ))
        })?;

        let state = tokio::task::spawn_blocking(move || -> Result<HNSWIndexState> {
            let archived = rkyv::check_archived_root::<HNSWIndexState>(&bytes).map_err(|e| {
                chimera_core::ChimeraError::Internal(format!(
                    "Failed to validate HNSW index archive: {}",
                    e
                ))
            })?;

            let state: HNSWIndexState =
                archived.deserialize(&mut rkyv::Infallible).map_err(|e| {
                    chimera_core::ChimeraError::Internal(format!(
                        "HNSW index deserialization error: {:?}",
                        e
                    ))
                })?;
            Ok(state)
        })
        .await
        .map_err(|e| {
            chimera_core::ChimeraError::Internal(format!("Blocking task failed: {}", e))
        })??;

        let config: HNSWConfig = state.config.into();
        let ml = 1.0 / (config.m as f64).ln();

        let mut doc_to_node = AHashMap::new();
        for (k, v) in state.doc_to_node {
            doc_to_node.insert(k, v);
        }

        let mut entry_points = AHashMap::new();
        for (k, v) in state.entry_points {
            entry_points.insert(NamespaceId::new(k), v);
        }

        let mut max_layers = AHashMap::new();
        for (k, v) in state.max_layers {
            max_layers.insert(NamespaceId::new(k), v);
        }

        let deleted_nodes = if state.deleted_nodes.is_empty() {
            RoaringTreemap::new()
        } else {
            RoaringTreemap::deserialize_from(&state.deleted_nodes[..]).unwrap_or_default()
        };

        Ok(HNSWIndex {
            inner: Arc::new(HNSWIndexCore {
                nodes: RwLock::new(state.nodes.into_iter().map(|n| n.into()).collect()),
                doc_to_node: RwLock::new(doc_to_node),
                entry_points: RwLock::new(entry_points),
                max_layers: RwLock::new(max_layers),
                ml,
                tx_buffer: TxBuffer::new_with_config(
                    config.tx_buffer_shard_count,
                    std::time::Duration::from_secs(config.tx_timeout_secs),
                ),
                config,
                deleted_nodes: RwLock::new(deleted_nodes),
                deleted_count: AtomicU64::new(0), // Reset count on load
                rebuilding: std::sync::atomic::AtomicBool::new(false),
                write_mutex: tokio::sync::Mutex::new(()),
            }),
        })
    }
}
