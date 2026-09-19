//! CSR-Graph-Implementierung für Entity-Relation-Traversal.
//!
//! Implementiert [`memfuse_core::GraphIndex`] via Compressed Sparse Row (CSR)
//! Datenstruktur für cache-effizienten Graph-Traversal.

// FILE-CONTEXT
// STAND: 2026-08-30T18:53:58Z (SESSION: b1234567)
// ZWECK: CSR-Graph für Entity-Relation-Traversal (Signal 3 in 4-Signal-Fusion)
// INVARIANTEN: Graph-Zustand wird in LSM-Store persistiert unter Präfixen
//              `__graph:entity:` und `__graph:edge:`. Änderungen müssen
//              BEIDE Strukturen konsistent halten (In-Memory CSR + LSM).
// HOTSPOTS: L500-L620 (GraphInner compact & pending edges buffer merge), L830-L980 (BFS Traversal)
// NICHT-OFFENSICHTLICH: KEINE petgraph-Abhängigkeit (Pure-Rust CSR, ADR-004).
//                       `relate()` MUSS sowohl LSM-Write als auch graph_index.add_edge()
//                       aufrufen — nur eines zu tun bricht Graph-Traversal (crates/memfuse-db/AGENTS.md).
// SIEHE AUCH: DECISIONS.md ADR-004, crates/memfuse-db/AGENTS.md §relate()

use crate::consistency_enforcement::{ConsistencyEnforcer, EdgeAssertion};
use crate::GraphIndexExt;
use arc_swap::ArcSwap;
use memfuse_core::{
    BoxFuture, DocId, Entity, EntityId, GraphIndex, GraphIndexStats, MemFuseError, Result,
    StorageEngine, TxId,
};
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};

/// Edge type representation for CSR edges.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum EdgeType {
    #[default]
    Default,
}

/// Edge structure in CSR graph representation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Edge {
    pub target: EntityId,
    pub weight: f32,
    pub edge_type: EdgeType,
    #[cfg(feature = "edge-reinforcement-learning")]
    pub cooccurrence_weight: f32, // w_ij, initialisiert mit 0.0
    #[cfg(feature = "edge-reinforcement-learning")]
    pub traversal_weight: f32, // τ_ij, initialisiert mit 0.0
}

impl Edge {
    pub fn new(target: EntityId, weight: f32) -> Self {
        Self {
            target,
            weight,
            edge_type: EdgeType::Default,
            #[cfg(feature = "edge-reinforcement-learning")]
            cooccurrence_weight: 0.0,
            #[cfg(feature = "edge-reinforcement-learning")]
            traversal_weight: 0.0,
        }
    }
}
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Persisted edge payload format for storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedEdgePayload {
    pub weight: f32,
    #[serde(default, alias = "valid_from")]
    pub tx_valid_from: Option<TxId>,
    #[serde(default, alias = "valid_to")]
    pub tx_valid_to: Option<TxId>,
    #[serde(default)]
    pub business_valid_from: Option<i64>,
    #[serde(default)]
    pub business_valid_to: Option<i64>,
    #[serde(default)]
    pub source_doc_id: Option<DocId>,
}

/// Score decay factor per hop (0.7^hop).
const SCORE_DECAY: f32 = 0.7;

/// Maximum traversal depth.
const MAX_TRAVERSAL_HOPS: u8 = 3;

/// Maximum visited nodes limit during BFS traversal to prevent intermediate hub-node memory explosion.
pub const MAX_VISITED_NODES: usize = 10_000;

/// LSM-Key-Prefix für alle Graph-Entities.
const GRAPH_ENTITY_PREFIX: &[u8] = b"__graph:entity:";
/// LSM-Key-Prefix für gelöschte Graph-Entities (Tombstones).
const GRAPH_ENTITY_DELETED_PREFIX: &[u8] = b"graph:entity:deleted:";
/// LSM-Key-Prefix für alle Graph-Edges.
const GRAPH_EDGE_PREFIX: &[u8] = b"__graph:edge:";
/// LSM-Key-Prefix für alle Community-Assignments.
const GRAPH_COMMUNITY_PREFIX: &[u8] = b"__graph:community:";

/// Untere Schranke für Wall-Clock-abgeleitete TxId-Heuristik.
///
/// Unix-Nanosekunden seit Epoch lagen am 01-01-2014 bei ca. 1.39×10¹⁸.
/// TxIds, die in diesen Bereich fallen **und** unterhalb von `INTERNAL_BASE`
/// liegen, sind höchstwahrscheinlich wall-clock-abgeleitet und verletzen das
/// TxId-Origin-Invariant (AGT-GRAPH-001).
///
/// Wert gewählt als `1_400_000_000 * 1_000_000_000` (1. Jan 2014 UTC in ns).
const WALLCLOCK_TX_HEURISTIC_MIN: u64 = 1_400_000_000_000_000_000;

/// Prüft, ob `tx` aus einem verdächtigen (wall-clock-ähnlichen) Bereich stammt.
///
/// Gibt `true` zurück, wenn `tx` zwischen [`WALLCLOCK_TX_HEURISTIC_MIN`] und
/// `TxId::INTERNAL_BASE` liegt — ein Bereich, in dem keine kanonische
/// Collection-Sequenz operiert, aber Unix-Nanosekunden-Werte liegen würden.
///
/// # Invarianten-Kontext (AGT-GRAPH-001)
/// Die Invariante AGT-GRAPH-001 ist graph-spezifisch: Graph-Traversal, Personalized PageRank (PPR),
/// bi-temporale Gültigkeitsfenster (`valid_from`, `valid_to`) und kausale `rollback_to_tx()`-Operationen
/// hängen strikt von geordneten Transaktions-Sequenzen ab. VectorIndex (HNSW) und TextIndex verwalten
/// Dokumenten-Updates hingegen über `TxBuffer` und atomare Snapshots/Tombstones.
///
/// # AI-NOTE[BOUNDARY-MISSING][MAJOR]
/// KONTEXT: AGT-GRAPH-001 — add_entity/add_edge/commit akzeptieren TxIds ohne
///   Laufzeit-Fehlerablehnung, erzwingen jedoch in Debug-Builds `debug_assert!(tx.is_valid_origin())`.
/// ANWEISUNG: Bei verdächtigen TxIds => tracing::warn! loggen. In Debug-Builds schlägt debug_assert! fehl.
/// ID: AGT-GRAPH-001
#[inline]
fn is_suspicious_tx_id(tx: TxId) -> bool {
    let v = tx.inner();
    v == 0 || (WALLCLOCK_TX_HEURISTIC_MIN..TxId::INTERNAL_BASE).contains(&v)
}

/// Prüft ob eine Kante bezogen auf die Transaktionszeit (MVCC / Systemzeit) zum Zeitpunkt `as_of` sichtbar ist.
#[inline]
pub(crate) fn is_edge_visible(
    tx_valid_from: Option<TxId>,
    tx_valid_to: Option<TxId>,
    as_of: TxId,
) -> bool {
    tx_valid_from.is_none_or(|vf| vf <= as_of) && tx_valid_to.is_none_or(|vt| as_of < vt)
}

/// Prüft ob eine Kante bezogen auf die Businesszeit zum Zeitpunkt `business_as_of` gültig ist.
#[inline]
pub(crate) fn is_edge_visible_business(
    business_valid_from: Option<i64>,
    business_valid_to: Option<i64>,
    business_as_of: i64,
) -> bool {
    business_valid_from.is_none_or(|vf| vf <= business_as_of)
        && business_valid_to.is_none_or(|vt| business_as_of < vt)
}

/// Prüft bi-temporale Sichtbarkeit einer Kante (unabhängige Auswertung von System- und Businesszeit).
///
/// Business-Zeit wird nur ausgewertet, wenn `business_as_of` angegeben ist UND mindestens
/// ein Business-Zeit-Feld (`business_valid_from` oder `business_valid_to`) auf der Kante gesetzt ist.
#[inline]
pub(crate) fn is_edge_visible_bitemporal(
    tx_valid_from: Option<TxId>,
    tx_valid_to: Option<TxId>,
    as_of_tx: TxId,
    business_valid_from: Option<i64>,
    business_valid_to: Option<i64>,
    as_of_business: Option<i64>,
) -> bool {
    if !is_edge_visible(tx_valid_from, tx_valid_to, as_of_tx) {
        return false;
    }
    if let Some(b_as_of) = as_of_business {
        if business_valid_from.is_some() || business_valid_to.is_some() {
            return is_edge_visible_business(business_valid_from, business_valid_to, b_as_of);
        }
    }
    true
}

/// Configuration parameters for [`CsrGraph`].
#[derive(Debug, Clone)]
pub struct CsrGraphConfig {
    /// Rebuild threshold: max number of uncompacted pending edges in delta buffer before triggering an automatic full CSR rebuild.
    pub rebuild_threshold: usize,
    /// Max compaction peak memory limit in MB (IP-08). Compaction will be deferred if current graph memory + rebuild allocation exceeds this threshold.
    pub max_compaction_peak_memory_mb: Option<usize>,
}

impl Default for CsrGraphConfig {
    fn default() -> Self {
        Self {
            rebuild_threshold: 1000,
            max_compaction_peak_memory_mb: Some(1024),
        }
    }
}

/// Internal contiguous index for CSR arrays.
type InternalIndex = usize;

/// Internal representation of an edge payload.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EdgePayload {
    pub(crate) target: InternalIndex,
    pub(crate) weight: f32,
    pub(crate) tx_valid_from: Option<TxId>,
    pub(crate) tx_valid_to: Option<TxId>,
    pub(crate) business_valid_from: Option<i64>,
    pub(crate) business_valid_to: Option<i64>,
    pub(crate) source_doc_id: Option<DocId>,
}

/// Staging representation of an edge before index allocation at commit time.
#[derive(Debug, Clone, PartialEq)]
struct StagedEdgePayload {
    target: EntityId,
    weight: f32,
    tx_valid_from: Option<TxId>,
    tx_valid_to: Option<TxId>,
    business_valid_from: Option<i64>,
    business_valid_to: Option<i64>,
    source_doc_id: Option<DocId>,
}

/// Inner state of the CsrGraph to manage contiguous storage.
#[derive(Clone)]
pub(crate) struct GraphInner {
    /// Mapping from public EntityId to internal contiguous index.
    pub(crate) id_map: HashMap<EntityId, InternalIndex>,
    /// Mapping from internal index back to EntityId.
    pub(crate) reverse_map: Vec<EntityId>,
    /// Entity metadata stored contiguously (Sentinel Entity with id EntityId::new(0) represents None).
    pub(crate) entities: Vec<Entity>,
    /// Community assignments mapping EntityId -> community_id.
    pub(crate) communities: HashMap<EntityId, u64>,
    /// Flag indicating whether communities have been loaded from storage or set in memory.
    pub(crate) communities_loaded: bool,

    /// CSR offsets array: offsets[i] is the start index in `targets` for node `i`.
    /// Length is nodes + 1.
    pub(crate) offsets: Vec<usize>,
    /// CSR targets array: contiguous list of neighbor internal indices.
    pub(crate) targets: Vec<InternalIndex>,
    /// CSR weights array: contiguous list of edge weights.
    pub(crate) weights: Vec<f32>,
    /// CSR valid_from array: contiguous list of bi-temporal tx_valid_from TxIds (TxId::INVALID represents None).
    pub(crate) tx_valid_froms: Vec<TxId>,
    /// CSR valid_to array: contiguous list of bi-temporal tx_valid_to TxIds (TxId::INVALID represents None).
    pub(crate) tx_valid_tos: Vec<TxId>,
    /// CSR business_valid_from array: contiguous list of business_valid_from timestamps (ms) (i64::MIN represents None).
    pub(crate) business_valid_froms: Vec<i64>,
    /// CSR business_valid_to array: contiguous list of business_valid_to timestamps (ms) (i64::MIN represents None).
    pub(crate) business_valid_tos: Vec<i64>,
    /// CSR source_doc_id array: contiguous list of optional source document IDs (DocId with inner() == 0 represents None).
    pub(crate) source_doc_ids: Vec<DocId>,
    /// Precomputed outgoing weight sums per internal node index.
    pub(crate) out_weight_sums: Vec<f32>,

    /// Reverse lookup index mapping DocId to Set of EdgeId (EntityId, EntityId)
    pub(crate) doc_to_edges: ahash::AHashMap<DocId, HashSet<(EntityId, EntityId)>>,

    /// Staging for entities not yet committed, indexed by (TxId, EntityId).
    staged_entities: ahash::AHashMap<(TxId, EntityId), Entity>,
    /// Staging for edges not yet committed, indexed by (TxId, EntityId).
    staged_edges: ahash::AHashMap<(TxId, EntityId), Vec<StagedEdgePayload>>,
    /// Staging for edge removals not yet committed, grouped by TxId.
    staged_removals: ahash::AHashMap<TxId, Vec<(EntityId, EntityId)>>,
    /// Edges that have been committed but not yet compacted into CSR arrays (delta buffer).
    pub(crate) pending_edges: HashMap<InternalIndex, Vec<EdgePayload>>,
    /// Tombstoned edges that have been removed and should be excluded during compaction and traversal.
    pub(crate) tombstoned_edges: HashSet<(InternalIndex, InternalIndex)>,
    /// Total number of uncompacted edges currently in `pending_edges`.
    pending_edge_count: usize,
    /// Flag indicating if there are uncompacted pending edges or modifications.
    is_dirty: bool,
    /// On-demand adjacency list cache for edge reinforcement learning.
    /// INVALIDATION STRATEGY: Cleared completely during `compact()` to prevent stale or removed edges
    /// from being served after pending/tombstone edge compaction.
    #[cfg(feature = "edge-reinforcement-learning")]
    pub(crate) edge_store: HashMap<EntityId, Vec<Edge>>,

    /// Hyperedge storage mapping HyperEdgeId -> HyperEdge.
    pub(crate) hyperedges: HashMap<crate::hyperedge::HyperEdgeId, crate::hyperedge::HyperEdge>,
    /// Index mapping DocId -> Set of HyperEdgeIds.
    pub(crate) doc_to_hyperedges: ahash::AHashMap<DocId, HashSet<crate::hyperedge::HyperEdgeId>>,
    /// Index mapping EntityId -> Set of HyperEdgeIds.
    pub(crate) hyperedge_index: ahash::AHashMap<EntityId, HashSet<crate::hyperedge::HyperEdgeId>>,
}

#[inline]
pub(crate) fn sentinel_entity() -> Entity {
    Entity::new(EntityId::new(0), "", "")
}

impl GraphInner {
    fn new() -> Self {
        Self {
            id_map: HashMap::new(),
            reverse_map: Vec::new(),
            entities: Vec::new(),
            communities: HashMap::new(),
            communities_loaded: false,
            offsets: vec![0],
            targets: Vec::new(),
            weights: Vec::new(),
            tx_valid_froms: Vec::new(),
            tx_valid_tos: Vec::new(),
            business_valid_froms: Vec::new(),
            business_valid_tos: Vec::new(),
            source_doc_ids: Vec::new(),
            out_weight_sums: Vec::new(),
            doc_to_edges: ahash::AHashMap::new(),
            staged_entities: ahash::AHashMap::default(),
            staged_edges: ahash::AHashMap::default(),
            staged_removals: ahash::AHashMap::default(),
            pending_edges: HashMap::new(),
            tombstoned_edges: HashSet::new(),
            pending_edge_count: 0,
            is_dirty: false,
            #[cfg(feature = "edge-reinforcement-learning")]
            edge_store: HashMap::new(),
            hyperedges: HashMap::new(),
            doc_to_hyperedges: ahash::AHashMap::new(),
            hyperedge_index: ahash::AHashMap::new(),
        }
    }

    #[inline]
    pub(crate) fn entity_at(&self, idx: usize) -> Option<&Entity> {
        self.entities.get(idx).filter(|e| e.id != EntityId::new(0))
    }

    #[inline]
    pub(crate) fn tx_valid_from_at(&self, idx: usize) -> Option<TxId> {
        self.tx_valid_froms
            .get(idx)
            .copied()
            .filter(|&tx| tx != TxId::INVALID)
    }

    #[inline]
    pub(crate) fn tx_valid_to_at(&self, idx: usize) -> Option<TxId> {
        self.tx_valid_tos
            .get(idx)
            .copied()
            .filter(|&tx| tx != TxId::INVALID)
    }

    #[inline]
    pub(crate) fn business_valid_from_at(&self, idx: usize) -> Option<i64> {
        self.business_valid_froms
            .get(idx)
            .copied()
            .filter(|&ts| ts != i64::MIN)
    }

    #[inline]
    pub(crate) fn business_valid_to_at(&self, idx: usize) -> Option<i64> {
        self.business_valid_tos
            .get(idx)
            .copied()
            .filter(|&ts| ts != i64::MIN)
    }

    #[inline]
    pub(crate) fn source_doc_id_at(&self, idx: usize) -> Option<DocId> {
        self.source_doc_ids
            .get(idx)
            .copied()
            .filter(|d| d.inner() != 0)
    }

    pub(crate) fn estimate_memory_bytes(&self) -> usize {
        (self.reverse_map.len() * std::mem::size_of::<EntityId>())
            + (self.entities.len() * std::mem::size_of::<Entity>())
            + (self.offsets.len() * std::mem::size_of::<usize>())
            + (self.targets.len() * std::mem::size_of::<usize>())
            + (self.weights.len() * std::mem::size_of::<f32>())
            + (self.tx_valid_froms.len() * std::mem::size_of::<TxId>())
            + (self.tx_valid_tos.len() * std::mem::size_of::<TxId>())
            + (self.business_valid_froms.len() * std::mem::size_of::<i64>())
            + (self.business_valid_tos.len() * std::mem::size_of::<i64>())
            + (self.source_doc_ids.len() * std::mem::size_of::<DocId>())
            + (self.out_weight_sums.len() * std::mem::size_of::<f32>())
            + (self.pending_edge_count * std::mem::size_of::<EdgePayload>())
            + (self.hyperedges.len() * std::mem::size_of::<crate::hyperedge::HyperEdge>())
            + self
                .hyperedges
                .values()
                .map(|e| {
                    e.participants.len() * std::mem::size_of::<crate::hyperedge::RoleBinding>()
                })
                .sum::<usize>()
            + (self.hyperedge_index.len()
                * (std::mem::size_of::<EntityId>()
                    + std::mem::size_of::<HashSet<crate::hyperedge::HyperEdgeId>>()))
            + (self
                .hyperedge_index
                .values()
                .map(|v| v.len())
                .sum::<usize>()
                * std::mem::size_of::<crate::hyperedge::HyperEdgeId>())
            + (self.doc_to_hyperedges.len()
                * (std::mem::size_of::<DocId>()
                    + std::mem::size_of::<HashSet<crate::hyperedge::HyperEdgeId>>()))
            + (self
                .doc_to_hyperedges
                .values()
                .map(|v| v.len())
                .sum::<usize>()
                * std::mem::size_of::<crate::hyperedge::HyperEdgeId>())
    }

    #[expect(
        dead_code,
        reason = "Internal GraphInner hyperedge helper method retained for planned GraphInner API symmetry"
    )]
    pub(crate) fn hyperedges_for_entity(&self, id: EntityId) -> Vec<crate::hyperedge::HyperEdgeId> {
        self.hyperedge_index
            .get(&id)
            .map(|set| set.iter().copied().collect())
            .unwrap_or_default()
    }

    #[expect(
        dead_code,
        reason = "Internal GraphInner hyperedge helper method retained for planned GraphInner API symmetry"
    )]
    pub(crate) fn insert_hyperedge(&mut self, edge: crate::hyperedge::HyperEdge) {
        let edge_id = edge.id;
        for participant in &edge.participants {
            self.hyperedge_index
                .entry(participant.entity)
                .or_default()
                .insert(edge_id);
        }
        self.hyperedges.insert(edge_id, edge);
    }

    fn get_or_create_index(&mut self, id: EntityId) -> InternalIndex {
        if let Some(&idx) = self.id_map.get(&id) {
            idx
        } else {
            let idx = self.reverse_map.len();
            self.id_map.insert(id, idx);
            self.reverse_map.push(id);
            self.out_weight_sums.push(0.0);
            // entities vector should be kept in sync by add_entity,
            // but we might add an edge to an entity not yet added via add_entity.
            // In that case, we'll have a "shadow" entity.
            idx
        }
    }

    #[inline]
    pub(crate) fn add_to_out_weight_sum(&mut self, node_idx: usize, weight: f32) {
        if weight > 0.0 {
            let num_nodes = self.reverse_map.len();
            if self.out_weight_sums.len() < num_nodes {
                self.out_weight_sums.resize(num_nodes, 0.0);
            }
            if node_idx < self.out_weight_sums.len() {
                self.out_weight_sums[node_idx] += weight;
            }
        }
    }

    pub(crate) fn recompute_node_out_weight_sum(&mut self, node_idx: usize) {
        let num_nodes = self.reverse_map.len();
        if node_idx >= num_nodes {
            return;
        }
        if self.out_weight_sums.len() < num_nodes {
            self.out_weight_sums.resize(num_nodes, 0.0);
        }
        if self.entity_at(node_idx).is_none() {
            self.out_weight_sums[node_idx] = 0.0;
            return;
        }

        let mut sum = 0.0f32;

        if node_idx < self.offsets.len() - 1 {
            let start = self.offsets[node_idx];
            let end = self.offsets[node_idx + 1];
            for j in start..end {
                let target = self.targets[j];
                let w = self.weights[j];
                if !self.tombstoned_edges.contains(&(node_idx, target))
                    && self.entity_at(target).is_some()
                    && w > 0.0
                {
                    sum += w;
                }
            }
        }

        if let Some(pending) = self.pending_edges.get(&node_idx) {
            for edge in pending {
                let target = edge.target;
                if !self.tombstoned_edges.contains(&(node_idx, target))
                    && self.entity_at(target).is_some()
                    && edge.weight > 0.0
                {
                    sum += edge.weight;
                }
            }
        }

        self.out_weight_sums[node_idx] = sum;
    }

    /// Compacts pending edges in the delta buffer into the main CSR arrays.
    fn compact(&mut self) {
        let num_nodes = self.reverse_map.len();

        if self.pending_edges.is_empty() && self.tombstoned_edges.is_empty() {
            while self.offsets.len() < num_nodes + 1 {
                let last = *self.offsets.last().unwrap_or(&0);
                self.offsets.push(last);
            }
            self.pending_edge_count = 0;
            self.is_dirty = false;
            #[cfg(feature = "edge-reinforcement-learning")]
            self.edge_store.clear();
            return;
        }

        let mut new_offsets = Vec::with_capacity(num_nodes + 1);
        let mut new_targets = Vec::with_capacity(self.targets.len() + self.pending_edge_count);
        let mut new_weights = Vec::with_capacity(self.weights.len() + self.pending_edge_count);
        let mut new_tx_valid_froms =
            Vec::with_capacity(self.tx_valid_froms.len() + self.pending_edge_count);
        let mut new_tx_valid_tos =
            Vec::with_capacity(self.tx_valid_tos.len() + self.pending_edge_count);
        let mut new_business_valid_froms =
            Vec::with_capacity(self.business_valid_froms.len() + self.pending_edge_count);
        let mut new_business_valid_tos =
            Vec::with_capacity(self.business_valid_tos.len() + self.pending_edge_count);
        let mut new_source_doc_ids =
            Vec::with_capacity(self.source_doc_ids.len() + self.pending_edge_count);

        let mut current_offset = 0;
        new_offsets.push(current_offset);

        for i in 0..num_nodes {
            let mut node_edges = Vec::new();

            // 1. Get neighbors from old CSR
            let old_start = if i < self.offsets.len() - 1 {
                self.offsets[i]
            } else {
                0
            };
            let old_end = if i < self.offsets.len() - 1 {
                self.offsets[i + 1]
            } else {
                0
            };

            for j in old_start..old_end {
                let target = self.targets[j];
                if !self.tombstoned_edges.contains(&(i, target)) {
                    node_edges.push(EdgePayload {
                        target,
                        weight: self.weights[j],
                        tx_valid_from: self.tx_valid_from_at(j),
                        tx_valid_to: self.tx_valid_to_at(j),
                        business_valid_from: self.business_valid_from_at(j),
                        business_valid_to: self.business_valid_to_at(j),
                        source_doc_id: self.source_doc_id_at(j),
                    });
                }
            }

            // 2. Get neighbors from pending_edges (FIND-GRA-001)
            if let Some(staged) = self.pending_edges.get(&i) {
                for edge in staged {
                    if !self.tombstoned_edges.contains(&(i, edge.target)) {
                        node_edges.push(edge.clone());
                    }
                }
            }

            // Stable sort target indices for deterministic CSR layout
            node_edges.sort_by_key(|e| e.target);

            for edge in node_edges {
                new_targets.push(edge.target);
                new_weights.push(edge.weight);
                new_tx_valid_froms.push(edge.tx_valid_from.unwrap_or(TxId::INVALID));
                new_tx_valid_tos.push(edge.tx_valid_to.unwrap_or(TxId::INVALID));
                new_business_valid_froms.push(edge.business_valid_from.unwrap_or(i64::MIN));
                new_business_valid_tos.push(edge.business_valid_to.unwrap_or(i64::MIN));
                new_source_doc_ids.push(edge.source_doc_id.unwrap_or(DocId::new(0)));
                current_offset += 1;
            }

            new_offsets.push(current_offset);
        }

        while new_offsets.len() < num_nodes + 1 {
            let last = *new_offsets.last().unwrap_or(&0);
            new_offsets.push(last);
        }

        self.offsets = new_offsets;
        self.targets = new_targets;
        self.weights = new_weights;
        self.tx_valid_froms = new_tx_valid_froms;
        self.tx_valid_tos = new_tx_valid_tos;
        self.business_valid_froms = new_business_valid_froms;
        self.business_valid_tos = new_business_valid_tos;
        self.source_doc_ids = new_source_doc_ids;
        self.pending_edges.clear();
        self.tombstoned_edges.clear();
        self.pending_edge_count = 0;
        self.is_dirty = false;
        #[cfg(feature = "edge-reinforcement-learning")]
        self.edge_store.clear();

        // Recompute out_weight_sums for all nodes during compaction
        self.out_weight_sums.resize(num_nodes, 0.0);
        for i in 0..num_nodes {
            if self.entity_at(i).is_none() {
                self.out_weight_sums[i] = 0.0;
                continue;
            }
            let start = self.offsets[i];
            let end = self.offsets[i + 1];
            let mut sum = 0.0f32;
            for j in start..end {
                let target = self.targets[j];
                let w = self.weights[j];
                if self.entity_at(target).is_some() && w > 0.0 {
                    sum += w;
                }
            }
            self.out_weight_sums[i] = sum;
        }
    }
}

#[cfg(feature = "edge-reinforcement-learning")]
impl GraphInner {
    pub(crate) fn find_edge_mut_by_entities(
        &mut self,
        from: EntityId,
        to: EntityId,
    ) -> Option<&mut Edge> {
        let from_idx = *self.id_map.get(&from)?;
        let to_idx = *self.id_map.get(&to)?;

        let exists_pending = self
            .pending_edges
            .get(&from_idx)
            .is_some_and(|edges| edges.iter().any(|e| e.target == to_idx));

        let exists_csr = if from_idx < self.offsets.len() - 1 {
            let start = self.offsets[from_idx];
            let end = self.offsets[from_idx + 1];
            self.targets[start..end].contains(&to_idx)
        } else {
            false
        };

        if !exists_pending && !exists_csr {
            return None;
        }

        let vec = self.edge_store.entry(from).or_default();
        if !vec.iter().any(|e| e.target == to) {
            let current_weight = if let Some(pending) = self.pending_edges.get(&from_idx) {
                pending
                    .iter()
                    .find(|e| e.target == to_idx)
                    .map(|e| e.weight)
            } else {
                None
            }
            .unwrap_or_else(|| {
                if from_idx < self.offsets.len() - 1 {
                    let start = self.offsets[from_idx];
                    let end = self.offsets[from_idx + 1];
                    for j in start..end {
                        if self.targets[j] == to_idx {
                            return self.weights[j];
                        }
                    }
                }
                1.0
            });

            vec.push(Edge::new(to, current_weight));
        }

        self.edge_store
            .get_mut(&from)?
            .iter_mut()
            .find(|e| e.target == to)
    }

    pub(crate) fn all_entity_ids(&self) -> Vec<EntityId> {
        self.reverse_map.clone()
    }

    pub(crate) fn outgoing_edges_mut(&mut self, entity_id: EntityId) -> &mut [Edge] {
        if let Some(from_idx) = self.id_map.get(&entity_id).copied() {
            let mut target_ids = Vec::new();
            if let Some(pending) = self.pending_edges.get(&from_idx) {
                for p in pending {
                    if let Some(&t_id) = self.reverse_map.get(p.target) {
                        target_ids.push((t_id, p.weight));
                    }
                }
            }
            if from_idx < self.offsets.len() - 1 {
                let start = self.offsets[from_idx];
                let end = self.offsets[from_idx + 1];
                for j in start..end {
                    let t_idx = self.targets[j];
                    if let Some(&t_id) = self.reverse_map.get(t_idx) {
                        target_ids.push((t_id, self.weights[j]));
                    }
                }
            }

            let vec = self.edge_store.entry(entity_id).or_default();
            for (t_id, w) in target_ids {
                if !vec.iter().any(|e| e.target == t_id) {
                    vec.push(Edge::new(t_id, w));
                }
            }
        }

        self.edge_store
            .get_mut(&entity_id)
            .map(|v| v.as_mut_slice())
            .unwrap_or(&mut [])
    }

    pub(crate) fn sync_edge_reinforcement_weights(
        &mut self,
        config: &crate::edge_reinforcement::EdgeReinforcementConfig,
    ) {
        use crate::edge_reinforcement::compute_edge_weight;

        for (&from_id, edges) in &self.edge_store {
            let Some(&from_idx) = self.id_map.get(&from_id) else {
                continue;
            };

            for edge in edges {
                let new_weight = compute_edge_weight(
                    edge.cooccurrence_weight,
                    edge.traversal_weight,
                    config.alpha,
                );

                if let Some(&to_idx) = self.id_map.get(&edge.target) {
                    // Update in pending_edges
                    if let Some(pending) = self.pending_edges.get_mut(&from_idx) {
                        for p_edge in pending.iter_mut() {
                            if p_edge.target == to_idx {
                                p_edge.weight = new_weight;
                            }
                        }
                    }

                    // Update in CSR arrays
                    if from_idx < self.offsets.len() - 1 {
                        let start = self.offsets[from_idx];
                        let end = self.offsets[from_idx + 1];
                        for j in start..end {
                            if self.targets[j] == to_idx {
                                self.weights[j] = new_weight;
                            }
                        }
                    }
                }
            }
        }
    }
}

/// RAII guard wrapping the writer mutex guard, automatically publishing updated `Arc<GraphInner>` snapshots to `ArcSwap` on drop.
pub(crate) struct InnerWriteGuard<'a> {
    guard: parking_lot::MutexGuard<'a, GraphInner>,
    arc_swap: &'a ArcSwap<GraphInner>,
}

impl<'a> std::ops::Deref for InnerWriteGuard<'a> {
    type Target = GraphInner;
    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

impl<'a> std::ops::DerefMut for InnerWriteGuard<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.guard
    }
}

impl<'a> Drop for InnerWriteGuard<'a> {
    fn drop(&mut self) {
        self.arc_swap.store(Arc::new((*self.guard).clone()));
    }
}

/// Compressed Sparse Row graph for entity-relation traversal.
///
/// Implements `GraphIndex` trait as Signal 3 in the 4-Signal Fusion architecture.
pub struct CsrGraph {
    config: CsrGraphConfig,
    inner: Arc<ArcSwap<GraphInner>>,
    write_state: Arc<Mutex<GraphInner>>,
    /// Optionaler Persistenz-Handle. None = reiner In-Memory-Modus (z.B. Tests).
    storage: Option<Arc<dyn StorageEngine>>,
    last_tx_id: AtomicU64,
    /// Optionales Register für Widerspruchsprävention/Consistency-Enforcement (F-04/ADR-073).
    /// None = disabled (default, P1-safe).
    consistency_enforcer: Option<RwLock<ConsistencyEnforcer>>,
    /// Rückverfolgung DocId -> betroffene Kanten, für Cascading-Invalidation (INV-GRAPH-PROV-1).
    pub doc_edge_index: crate::provenance::DocEdgeIndex,
}

impl CsrGraph {
    /// Creates a new, empty CSR graph with default configuration.
    pub fn new() -> Self {
        Self::with_config(CsrGraphConfig::default())
    }

    /// Creates a new, empty CSR graph with specified configuration.
    pub fn with_config(config: CsrGraphConfig) -> Self {
        let initial = Arc::new(GraphInner::new());
        Self {
            config,
            inner: Arc::new(ArcSwap::from(initial.clone())),
            write_state: Arc::new(Mutex::new((*initial).clone())),
            storage: None,
            last_tx_id: AtomicU64::new(0),
            consistency_enforcer: None,
            doc_edge_index: crate::provenance::DocEdgeIndex::new(),
        }
    }

    /// Creates a new CSR graph with persistent storage and default config.
    pub fn with_storage(storage: Arc<dyn StorageEngine>) -> Self {
        Self::with_config_and_storage(CsrGraphConfig::default(), storage)
    }

    /// Creates a new CSR graph with configuration and persistent storage.
    pub fn with_config_and_storage(
        config: CsrGraphConfig,
        storage: Arc<dyn StorageEngine>,
    ) -> Self {
        let initial = Arc::new(GraphInner::new());
        Self {
            config,
            inner: Arc::new(ArcSwap::from(initial.clone())),
            write_state: Arc::new(Mutex::new((*initial).clone())),
            storage: Some(storage),
            last_tx_id: AtomicU64::new(0),
            consistency_enforcer: None,
            doc_edge_index: crate::provenance::DocEdgeIndex::new(),
        }
    }

    /// Erstellt CsrGraph mit aktiviertem ConsistencyEnforcer für Widerspruchsprävention (F-04/ADR-073).
    pub fn with_consistency_enforcer(suppression_threshold: u32) -> Self {
        let initial = Arc::new(GraphInner::new());
        Self {
            config: CsrGraphConfig::default(),
            inner: Arc::new(ArcSwap::from(initial.clone())),
            write_state: Arc::new(Mutex::new((*initial).clone())),
            storage: None,
            last_tx_id: AtomicU64::new(0),
            consistency_enforcer: Some(RwLock::new(ConsistencyEnforcer::new(
                suppression_threshold,
            ))),
            doc_edge_index: crate::provenance::DocEdgeIndex::new(),
        }
    }

    /// Tombstoniert eine Kante direkt für eine Transaktions-ID via Cascading-Invalidation (INV-GRAPH-PROV-1).
    pub async fn tombstone_edge(
        &self,
        edge_id: crate::consistency_enforcement::EdgeId,
        tx: TxId,
    ) -> Result<()> {
        let (from, to) = edge_id;
        GraphIndex::remove_edge(self, tx, from, to).await?;
        GraphIndex::commit(self, tx).await?;
        Ok(())
    }

    /// Read path contract for RCU snapshot isolation (IP-08):
    /// Returns a lock-free reference `arc_swap::Guard<Arc<GraphInner>>` to the current `GraphInner` snapshot.
    /// Readers never block on compaction or writer locks.
    ///
    /// # PPR Integration Contract
    /// Downstream PPR algorithms (e.g. `ppr.rs`) consume this snapshot directly via `inner_read()`.
    /// The returned `Arc<GraphInner>` snapshot is point-in-time immutable and guaranteed not to be mutated in-place.
    pub(crate) fn inner_read(&self) -> arc_swap::Guard<Arc<GraphInner>> {
        self.inner.load()
    }

    pub(crate) fn inner_write(&self) -> InnerWriteGuard<'_> {
        InnerWriteGuard {
            guard: self.write_state.lock(),
            arc_swap: &self.inner,
        }
    }

    /// Sets or replaces the persistent storage handle.
    pub fn set_storage(&mut self, storage: Arc<dyn StorageEngine>) {
        self.storage = Some(storage);
    }

    /// Returns a reference to the optional persistent storage handle.
    pub fn storage(&self) -> Option<Arc<dyn StorageEngine>> {
        self.storage.clone()
    }

    /// Returns the optional source document ID stored at the given edge index in `source_doc_ids`.
    pub fn get_source_doc_id(&self, index: usize) -> Option<DocId> {
        let inner = self.inner_read();
        inner.source_doc_id_at(index)
    }

    /// Returns the optional source document ID from which the edge (from, to) was derived.
    pub fn source_doc_id_at(&self, from: EntityId, to: EntityId) -> Option<DocId> {
        let inner = self.inner_read();

        for ((_, staged_from), staged_vec) in inner.staged_edges.iter() {
            if *staged_from == from {
                if let Some(staged) = staged_vec.iter().find(|e| e.target == to) {
                    if staged.source_doc_id.is_some() {
                        return staged.source_doc_id;
                    }
                }
            }
        }

        let from_idx = *inner.id_map.get(&from)?;
        let to_idx = *inner.id_map.get(&to)?;

        if let Some(pending) = inner.pending_edges.get(&from_idx) {
            if let Some(edge) = pending.iter().find(|e| e.target == to_idx) {
                return edge.source_doc_id;
            }
        }

        if from_idx < inner.offsets.len() - 1 {
            let start = inner.offsets[from_idx];
            let end = inner.offsets[from_idx + 1];
            for j in start..end {
                if inner.targets.get(j) == Some(&to_idx) {
                    return inner.source_doc_id_at(j);
                }
            }
        }

        None
    }

    /// Atomically tombstones a list of edges with WAL sequence provenance (INV-GRAPH-PROV-1),
    /// returning newly tombstoned edges and affected node IDs.
    #[allow(clippy::type_complexity)]
    pub(crate) fn tombstone_edges_direct(
        &self,
        edges: &[(EntityId, EntityId)],
        wal_tx: TxId,
    ) -> Result<(usize, Vec<(EntityId, EntityId)>, Vec<EntityId>)> {
        let mut inner = self.inner_write();
        let mut newly_tombstoned = Vec::new();
        let mut affected_nodes_set = HashSet::new();

        for &(from_id, to_id) in edges {
            let from_idx = inner.id_map.get(&from_id).copied();
            let to_idx = inner.id_map.get(&to_id).copied();

            if let (Some(f_idx), Some(t_idx)) = (from_idx, to_idx) {
                if inner.tombstoned_edges.insert((f_idx, t_idx)) {
                    // Record WAL transaction invalidation provenance on pending edge payloads
                    if let Some(pending) = inner.pending_edges.get_mut(&f_idx) {
                        for edge in pending.iter_mut() {
                            if edge.target == t_idx {
                                edge.tx_valid_to = Some(wal_tx);
                            }
                        }
                    }
                    // Record WAL transaction invalidation provenance on compacted CSR arrays
                    if f_idx < inner.offsets.len() - 1 {
                        let start = inner.offsets[f_idx];
                        let end = inner.offsets[f_idx + 1];
                        for j in start..end {
                            if inner.targets.get(j) == Some(&t_idx) {
                                if let Some(tx_to) = inner.tx_valid_tos.get_mut(j) {
                                    *tx_to = wal_tx;
                                }
                            }
                        }
                    }
                    inner.is_dirty = true;
                    newly_tombstoned.push((from_id, to_id));
                    affected_nodes_set.insert(from_id);
                    affected_nodes_set.insert(to_id);
                }
            }
        }

        let mut affected_node_ids: Vec<EntityId> = affected_nodes_set.into_iter().collect();
        affected_node_ids.sort();

        Ok((newly_tombstoned.len(), newly_tombstoned, affected_node_ids))
    }

    /// Directly inserts a hyperedge into memory.
    pub fn insert_hyperedge_direct(&self, hyperedge: crate::hyperedge::HyperEdge) {
        let mut inner = self.inner_write();
        let hyperedge_id = hyperedge.id;
        if let Some(doc_id) = hyperedge.source_doc_id {
            inner
                .doc_to_hyperedges
                .entry(doc_id)
                .or_default()
                .insert(hyperedge_id);
        }
        for participant in &hyperedge.participants {
            inner
                .hyperedge_index
                .entry(participant.entity)
                .or_default()
                .insert(hyperedge_id);
        }
        inner.hyperedges.insert(hyperedge_id, hyperedge);
    }

    /// Returns all non-tombstoned hyperedge IDs derived from `doc_id`.
    pub fn hyperedges_for_doc(&self, doc_id: DocId) -> Vec<crate::hyperedge::HyperEdgeId> {
        let inner = self.inner_read();
        inner
            .doc_to_hyperedges
            .get(&doc_id)
            .map(|set| {
                set.iter()
                    .copied()
                    .filter(|id| {
                        inner
                            .hyperedges
                            .get(id)
                            .is_some_and(|edge| edge.tx_valid_to.is_none())
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Retrieves a non-tombstoned hyperedge by its ID if present.
    pub fn get_hyperedge(
        &self,
        id: crate::hyperedge::HyperEdgeId,
    ) -> Option<crate::hyperedge::HyperEdge> {
        let inner = self.inner_read();
        inner
            .hyperedges
            .get(&id)
            .filter(|edge| edge.tx_valid_to.is_none())
            .cloned()
    }

    /// Returns all non-tombstoned hyperedge IDs associated with `entity_id`.
    pub fn hyperedges_for_entity(&self, entity_id: EntityId) -> Vec<crate::hyperedge::HyperEdgeId> {
        let inner = self.inner_read();
        inner
            .hyperedge_index
            .get(&entity_id)
            .map(|set| {
                set.iter()
                    .copied()
                    .filter(|id| {
                        inner
                            .hyperedges
                            .get(id)
                            .is_some_and(|edge| edge.tx_valid_to.is_none())
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Atomically tombstones a hyperedge by setting `tx_valid_to = Some(wal_tx)` and removing
    /// it from `hyperedge_index` and `doc_to_hyperedges` in a single write lock.
    ///
    /// NOTE: Callers in `cascade.rs` will adjust to pass `wal_tx: memfuse_core::TxId` as part of
    /// parallel wave updates.
    ///
    /// Returns `true` if the hyperedge was found and newly tombstoned, or `false` if
    /// it was already tombstoned or does not exist.
    pub fn tombstone_hyperedge(&self, id: crate::hyperedge::HyperEdgeId, wal_tx: TxId) -> bool {
        let mut inner = self.inner_write();
        let inner_ptr = &mut *inner;
        let edge = match inner_ptr.hyperedges.get_mut(&id) {
            Some(edge) => {
                if edge.tx_valid_to.is_some() {
                    return false;
                }
                edge.tx_valid_to = Some(wal_tx);
                edge
            }
            None => return false,
        };

        if let Some(doc_id) = edge.source_doc_id {
            if let Some(set) = inner_ptr.doc_to_hyperedges.get_mut(&doc_id) {
                set.remove(&id);
                if set.is_empty() {
                    inner_ptr.doc_to_hyperedges.remove(&doc_id);
                }
            }
        }

        for participant in &edge.participants {
            if let Some(set) = inner_ptr.hyperedge_index.get_mut(&participant.entity) {
                set.remove(&id);
                if set.is_empty() {
                    inner_ptr.hyperedge_index.remove(&participant.entity);
                }
            }
        }

        true
    }

    /// Returns all edge IDs derived from the given source `DocId`.
    pub fn edges_for_doc(&self, doc_id: DocId) -> Vec<(EntityId, EntityId)> {
        let inner = self.inner_read();
        let mut edges: HashSet<(EntityId, EntityId)> =
            inner.doc_to_edges.get(&doc_id).cloned().unwrap_or_default();
        for edge in self.doc_edge_index.edges_for_doc(doc_id) {
            edges.insert(edge);
        }
        edges.into_iter().collect()
    }

    /// Atomically inserts a hyperedge into the graph and updates the entity secondary index.
    pub fn insert_hyperedge(&self, edge: crate::hyperedge::HyperEdge) {
        self.insert_hyperedge_direct(edge);
    }

    /// Directly inserts an entity into the CSR graph without staging.
    pub fn insert_entity_direct(&self, entity: Entity) -> Result<()> {
        let mut inner = self.inner_write();
        let idx = inner.get_or_create_index(entity.id);
        if idx >= inner.entities.len() {
            inner.entities.resize(idx + 1, sentinel_entity());
        }
        inner.entities[idx] = entity;
        Ok(())
    }

    /// Inserts an edge directly into the CSR graph with bi-temporal validity, offloading compaction asynchronously if needed.
    #[allow(clippy::too_many_arguments)]
    pub async fn add_edge(
        self: &Arc<Self>,
        from: EntityId,
        to: EntityId,
        weight: f32,
        tx_valid_from: Option<TxId>,
        tx_valid_to: Option<TxId>,
        business_valid_from: Option<i64>,
        business_valid_to: Option<i64>,
        source_doc_id: Option<DocId>,
        predicate_hash: Option<[u8; 32]>,
        object_repr: Option<Vec<u8>>,
    ) -> Result<()> {
        if !weight.is_finite() || weight < 0.0 {
            return Err(MemFuseError::InvalidInput(format!(
                "Invalid edge weight {weight}: weight must be finite and non-negative"
            )));
        }

        // Consistency-Check wenn aktiviert
        if let Some(ref enforcer_lock) = self.consistency_enforcer {
            if let (Some(pred_hash), Some(obj)) = (predicate_hash, object_repr.as_ref()) {
                let assertion = EdgeAssertion {
                    subject: from.inner(),
                    predicate_hash: pred_hash,
                    object_repr: obj.clone(),
                };
                let mut enforcer = enforcer_lock.write();
                if let Some(pattern) = enforcer.check_before_insert(&assertion) {
                    if pattern.suppressed {
                        // Widerspruch unterdrückt — Einfügen blockiert
                        tracing::warn!(
                            suppression_count = pattern.contradiction_count,
                            "ConsistencyEnforcer: edge insertion suppressed by conflict pattern"
                        );
                        return Err(MemFuseError::PolicyViolation(
                            "Contradictory edge suppressed by consistency enforcer".to_string(),
                        ));
                    }
                    // Widerspruch erkannt aber noch nicht suppressed — loggen, trotzdem einfügen
                    tracing::warn!(
                        "ConsistencyEnforcer: contradictory edge detected (not yet suppressed)"
                    );
                }
            }
        }

        // Phase 1: Edge einfügen (Write-Lock kurz halten, kein I/O)
        let needs_compact = {
            let mut inner = self.inner_write();
            let from_idx = inner.get_or_create_index(from);
            let to_idx = inner.get_or_create_index(to);
            if let Some(doc_id) = source_doc_id {
                inner
                    .doc_to_edges
                    .entry(doc_id)
                    .or_default()
                    .insert((from, to));
            }
            inner
                .pending_edges
                .entry(from_idx)
                .or_default()
                .push(EdgePayload {
                    target: to_idx,
                    weight,
                    tx_valid_from,
                    tx_valid_to,
                    business_valid_from,
                    business_valid_to,
                    source_doc_id,
                });
            inner.pending_edge_count += 1;
            inner.is_dirty = true;
            inner.add_to_out_weight_sum(from_idx, weight);
            inner.pending_edge_count >= self.config.rebuild_threshold
        }; // Write-Lock freigegeben

        if let Some(doc_id) = source_doc_id {
            self.doc_edge_index.record(doc_id, (from, to));
        }

        // Phase 2: Compact außerhalb des Write-Locks (falls nötig)
        if needs_compact {
            // compact_async holt sich intern den Write-Lock in spawn_blocking
            self.compact_async().await?;
        }

        Ok(())
    }

    /// Directly inserts an edge into the CSR graph without staging.
    pub async fn insert_edge_direct(
        self: &Arc<Self>,
        from: EntityId,
        to: EntityId,
        weight: f32,
    ) -> Result<()> {
        self.add_edge(from, to, weight, None, None, None, None, None, None, None)
            .await
    }

    /// Directly inserts an edge with validity into the CSR graph without staging.
    ///
    /// # Weight Validation & Policy
    /// Edge weights represent relationship strengths or score-decay factors in graph traversal and PPR.
    /// Negative, infinite, or NaN weights can cause pruning failure in BFS traversal or invalid PageRank calculations.
    /// Therefore, edge weights MUST be finite and non-negative (`0.0 <= weight`).
    pub async fn insert_edge_direct_with_validity(
        self: &Arc<Self>,
        from: EntityId,
        to: EntityId,
        weight: f32,
        tx_valid_from: Option<TxId>,
        tx_valid_to: Option<TxId>,
    ) -> Result<()> {
        self.add_edge(
            from,
            to,
            weight,
            tx_valid_from,
            tx_valid_to,
            None,
            None,
            None,
            None,
            None,
        )
        .await
    }

    /// Directly inserts an edge with full bi-temporal validity into the CSR graph without staging.
    #[allow(clippy::too_many_arguments)]
    pub async fn insert_edge_direct_with_bitemporal_validity(
        self: &Arc<Self>,
        from: EntityId,
        to: EntityId,
        weight: f32,
        tx_valid_from: Option<TxId>,
        tx_valid_to: Option<TxId>,
        business_valid_from: Option<i64>,
        business_valid_to: Option<i64>,
        source_doc_id: Option<DocId>,
    ) -> Result<()> {
        self.add_edge(
            from,
            to,
            weight,
            tx_valid_from,
            tx_valid_to,
            business_valid_from,
            business_valid_to,
            source_doc_id,
            None,
            None,
        )
        .await
    }

    /// Fügt eine Entity direkt ein (für load_from_storage).
    fn load_entity_direct(&self, entity: Entity) -> Result<()> {
        let mut inner = self.inner_write();
        let idx = inner.get_or_create_index(entity.id);
        while inner.entities.len() <= idx {
            inner.entities.push(sentinel_entity());
        }
        inner.entities[idx] = entity;
        Ok(())
    }

    /// Fügt eine Edge direkt in committed_staged / pending_edges ein (für load_from_storage).
    /// Umgeht das TX-Staging, da beim Laden alle Daten bereits committed sind.
    #[allow(clippy::too_many_arguments)]
    fn load_edge_direct(
        &self,
        from: EntityId,
        to: EntityId,
        weight: f32,
        tx_valid_from: Option<TxId>,
        tx_valid_to: Option<TxId>,
        business_valid_from: Option<i64>,
        business_valid_to: Option<i64>,
        source_doc_id: Option<DocId>,
    ) -> Result<()> {
        if !weight.is_finite() || weight < 0.0 {
            return Err(MemFuseError::InvalidInput(format!(
                "Invalid edge weight {weight}: weight must be finite and non-negative"
            )));
        }
        let mut inner = self.inner_write();
        let from_idx = inner.get_or_create_index(from);
        let to_idx = inner.get_or_create_index(to);
        if let Some(doc_id) = source_doc_id {
            inner
                .doc_to_edges
                .entry(doc_id)
                .or_default()
                .insert((from, to));
        }
        inner
            .pending_edges
            .entry(from_idx)
            .or_default()
            .push(EdgePayload {
                target: to_idx,
                weight,
                tx_valid_from,
                tx_valid_to,
                business_valid_from,
                business_valid_to,
                source_doc_id,
            });
        inner.pending_edge_count += 1;
        inner.is_dirty = true;
        inner.add_to_out_weight_sum(from_idx, weight);
        Ok(())
    }

    /// Persistiert eine einzelne Entity in den übergebenen Storage.
    pub async fn persist_entity<S: StorageEngine + ?Sized>(
        &self,
        storage: &S,
        tx: TxId,
        entity: &Entity,
    ) -> Result<()> {
        let key = [GRAPH_ENTITY_PREFIX, entity.id.as_bytes().as_slice()].concat();
        let value = bincode::serialize(entity)
            .map_err(|e| MemFuseError::Internal(format!("graph entity serialize: {e}")))?;
        storage.put(tx, &key, &value).await
    }

    /// Persistiert eine einzelne Edge in den übergebenen Storage.
    pub async fn persist_edge<S: StorageEngine + ?Sized>(
        &self,
        storage: &S,
        tx: TxId,
        from: &EntityId,
        to: &EntityId,
        payload: &PersistedEdgePayload,
    ) -> Result<()> {
        let key = [
            GRAPH_EDGE_PREFIX,
            from.as_bytes().as_slice(),
            b":",
            to.as_bytes().as_slice(),
        ]
        .concat();
        let value = bincode::serialize(payload)
            .map_err(|e| MemFuseError::Internal(format!("graph edge serialize: {e}")))?;
        storage.put(tx, &key, &value).await
    }

    /// Löscht eine einzelne Edge aus dem übergebenen Storage.
    pub async fn delete_edge_persistence<S: StorageEngine + ?Sized>(
        &self,
        storage: &S,
        tx: TxId,
        from: &EntityId,
        to: &EntityId,
    ) -> Result<()> {
        let key = [
            GRAPH_EDGE_PREFIX,
            from.as_bytes().as_slice(),
            b":",
            to.as_bytes().as_slice(),
        ]
        .concat();
        storage.delete(tx, &key).await
    }

    /// Lädt den kompletten Graph-Zustand aus dem Storage (beim Startup).
    pub async fn load_from_storage<S: StorageEngine + ?Sized>(storage: &S) -> Result<Self> {
        let graph = Self::new();

        // 0. Deleted Entities (Tombstones) laden
        let deleted_entries = storage.scan_prefix(GRAPH_ENTITY_DELETED_PREFIX).await?;
        let mut deleted_entity_ids = HashSet::new();
        for (raw_key, _) in deleted_entries {
            if let Some(key_payload) = raw_key.get(GRAPH_ENTITY_DELETED_PREFIX.len()..) {
                if let Ok(key_str) = std::str::from_utf8(key_payload) {
                    deleted_entity_ids.insert(EntityId::from(key_str));
                }
            }
        }

        // 1. Entities laden
        let entity_entries = storage.scan_prefix(GRAPH_ENTITY_PREFIX).await?;
        let mut entity_count = 0usize;
        for (_, raw_value) in entity_entries {
            let entity: Entity = bincode::deserialize(&raw_value)
                .map_err(|e| MemFuseError::Internal(format!("graph entity deserialize: {e}")))?;
            if deleted_entity_ids.contains(&entity.id) {
                continue;
            }
            graph.load_entity_direct(entity)?;
            entity_count += 1;
        }

        // 2. Edges laden
        let edge_entries = storage.scan_prefix(GRAPH_EDGE_PREFIX).await?;
        let mut edge_count = 0usize;
        for (raw_key, raw_value) in edge_entries {
            let (
                weight,
                tx_valid_from,
                tx_valid_to,
                business_valid_from,
                business_valid_to,
                source_doc_id,
            ) = if let Ok(p) = bincode::deserialize::<PersistedEdgePayload>(&raw_value) {
                (
                    p.weight,
                    p.tx_valid_from,
                    p.tx_valid_to,
                    p.business_valid_from,
                    p.business_valid_to,
                    p.source_doc_id,
                )
            } else {
                // Backward compatibility fallback for legacy 5-field PersistedEdgePayload
                #[derive(Deserialize)]
                struct LegacyPersistedEdgePayloadV2 {
                    weight: f32,
                    valid_from: Option<TxId>,
                    valid_to: Option<TxId>,
                    business_valid_from: Option<i64>,
                    business_valid_to: Option<i64>,
                }

                if let Ok(legacy2) =
                    bincode::deserialize::<LegacyPersistedEdgePayloadV2>(&raw_value)
                {
                    (
                        legacy2.weight,
                        legacy2.valid_from,
                        legacy2.valid_to,
                        legacy2.business_valid_from,
                        legacy2.business_valid_to,
                        None,
                    )
                } else {
                    // Backward compatibility fallback for legacy 3-field PersistedEdgePayload
                    #[derive(Deserialize)]
                    struct LegacyPersistedEdgePayloadV1 {
                        weight: f32,
                        valid_from: Option<TxId>,
                        valid_to: Option<TxId>,
                    }

                    if let Ok(legacy) =
                        bincode::deserialize::<LegacyPersistedEdgePayloadV1>(&raw_value)
                    {
                        (
                            legacy.weight,
                            legacy.valid_from,
                            legacy.valid_to,
                            None,
                            None,
                            None,
                        )
                    } else {
                        // Backward compatibility fallback for legacy raw f32 weight values
                        let w: f32 = bincode::deserialize(&raw_value).map_err(|e| {
                            MemFuseError::Internal(format!("graph edge deserialize: {e}"))
                        })?;
                        (w, None, None, None, None, None)
                    }
                }
            };

            // Key-Format: "__graph:edge:{from_id}:{to_id}"
            let key_payload = raw_key
                .get(GRAPH_EDGE_PREFIX.len()..)
                .ok_or_else(|| MemFuseError::Internal("graph edge key zu kurz".into()))?;

            let key_str = std::str::from_utf8(key_payload)
                .map_err(|e| MemFuseError::Internal(format!("graph edge key UTF-8: {e}")))?;

            if let Some((from_str, to_str)) = key_str.split_once(':') {
                let from_id = EntityId::from(from_str);
                let to_id = EntityId::from(to_str);
                graph.load_edge_direct(
                    from_id,
                    to_id,
                    weight,
                    tx_valid_from,
                    tx_valid_to,
                    business_valid_from,
                    business_valid_to,
                    source_doc_id,
                )?;
                edge_count += 1;
            } else {
                tracing::warn!(key = key_str, "Ungültiger graph edge key, übersprungen");
            }
        }

        // 3. CSR kompaktieren — MUSS nach allen Edges aufgerufen werden
        graph.compact();

        if let Ok(last_tx) = storage.last_tx_id().await {
            graph
                .last_tx_id
                .fetch_max(last_tx.inner(), Ordering::SeqCst);
        }

        // CSR rebuilt from LSM on startup; Supersedes relations are re-evaluated to re-apply edge tombstones.
        let doc_entries = storage.scan_prefix(b"").await?;
        let wal_seq = graph.last_tx_id.load(Ordering::SeqCst);
        for (_raw_key, raw_value) in doc_entries {
            if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&raw_value) {
                let meta_obj = val
                    .get("metadata")
                    .and_then(|m| m.as_object())
                    .or_else(|| val.as_object());
                if let Some(obj) = meta_obj {
                    if let Some(links_val) = obj.get("links") {
                        if let Ok(links) = serde_json::from_value::<
                            Vec<memfuse_core::types::domain::MemoryLink>,
                        >(links_val.clone())
                        {
                            for link in links {
                                if link.relation
                                    == memfuse_core::types::domain::LinkRelation::Supersedes
                                {
                                    let superseded_doc = link.target;
                                    let edge_ids = graph.edges_for_doc(superseded_doc);
                                    if !edge_ids.is_empty() {
                                        let _ = graph
                                            .tombstone_edges_direct(&edge_ids, TxId::new(wal_seq));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // 3. Communities laden
        let community_entries = storage.scan_prefix(GRAPH_COMMUNITY_PREFIX).await?;
        let mut community_count = 0usize;
        {
            let mut inner = graph.inner_write();
            inner.communities_loaded = true;
            for (raw_key, raw_value) in community_entries {
                if let Some(key_payload) = raw_key.get(GRAPH_COMMUNITY_PREFIX.len()..) {
                    if let Ok(key_str) = std::str::from_utf8(key_payload) {
                        let eid = EntityId::from(key_str);
                        if let Ok(comm_id) = serde_json::from_slice::<u64>(&raw_value) {
                            inner.communities.insert(eid, comm_id);
                            community_count += 1;
                        }
                    }
                }
            }
        }

        tracing::info!(
            entities = entity_count,
            edges = edge_count,
            communities = community_count,
            last_tx = graph.last_tx_id.load(Ordering::SeqCst),
            "Graph aus Storage geladen und kompaktiert"
        );
        Ok(graph)
    }

    /// Force compacts the graph delta buffer into the main CSR arrays to optimize traversal layout.
    pub fn compact(&self) {
        // Double-checked locking to avoid unnecessary write locks (FIND-GRA-002)
        let snapshot = self.inner.load();
        let num_nodes = snapshot.reverse_map.len();
        if !snapshot.is_dirty
            && snapshot.pending_edges.is_empty()
            && snapshot.tombstoned_edges.is_empty()
            && snapshot.offsets.len() == num_nodes + 1
        {
            return;
        }
        drop(snapshot);

        let mut inner = self.inner_write();
        let num_nodes = inner.reverse_map.len();
        if inner.is_dirty
            || !inner.pending_edges.is_empty()
            || !inner.tombstoned_edges.is_empty()
            || inner.offsets.len() != num_nodes + 1
        {
            inner.compact();
        }
    }

    /// Asynchronously compacts the graph delta buffer, offloading heavy CPU rebuild work to `spawn_blocking` if necessary.
    pub async fn compact_async(&self) -> Result<()> {
        let snapshot = self.inner.load();
        let num_nodes = snapshot.reverse_map.len();
        let is_needed = snapshot.is_dirty
            || !snapshot.pending_edges.is_empty()
            || !snapshot.tombstoned_edges.is_empty()
            || snapshot.offsets.len() != num_nodes + 1;

        if !is_needed {
            return Ok(());
        }

        if let Some(max_mb) = self.config.max_compaction_peak_memory_mb {
            let estimated_bytes = snapshot.estimate_memory_bytes();
            let estimated_peak_bytes = estimated_bytes * 2;
            let max_bytes = max_mb * 1024 * 1024;
            if estimated_peak_bytes > max_bytes {
                tracing::warn!(
                    estimated_peak_mb = estimated_peak_bytes / (1024 * 1024),
                    max_compaction_peak_memory_mb = max_mb,
                    "compact_async deferred due to compaction memory budget constraint"
                );
                // AI-TAG[TODO][IP-08-BUDGET-COUPLING](TS:2026-09-18T12:00:00Z)(SESSION:e095d708): Connect to global ResourceTracker when cross-crate tracker handle is integrated.
                // NOTE(IP-20): Hyperedge memory contributions are included in estimate_memory_bytes() for accurate local budget checks.
                return Ok(());
            }
        }

        let write_state = self.write_state.clone();
        let arc_swap = self.inner.clone();

        tokio::task::spawn_blocking(move || {
            let mut inner_writer = write_state.lock();
            let num_nodes = inner_writer.reverse_map.len();
            if inner_writer.is_dirty
                || !inner_writer.pending_edges.is_empty()
                || !inner_writer.tombstoned_edges.is_empty()
                || inner_writer.offsets.len() != num_nodes + 1
            {
                inner_writer.compact();
                arc_swap.store(Arc::new(inner_writer.clone()));
            }
        })
        .await
        .map_err(|e| MemFuseError::Internal(format!("compact_async spawn_blocking error: {e}")))?;

        Ok(())
    }

    /// Sets community assignments in batch for in-memory graph index lookups.
    pub fn set_communities_batch(&self, assignments: &[crate::CommunityAssignment]) {
        let mut inner = self.inner_write();
        for a in assignments {
            inner.communities.insert(a.entity_id, a.community_id);
        }
        inner.communities_loaded = true;
    }

    /// Retrieves community assignments for a batch of entity IDs in a single operation.
    pub async fn get_communities_batch(
        &self,
        entity_ids: &[EntityId],
    ) -> Result<HashMap<EntityId, u64>> {
        let (map, done) = {
            let inner = self.inner_read();
            let mut map = HashMap::with_capacity(entity_ids.len());
            for &eid in entity_ids {
                if let Some(&comm_id) = inner.communities.get(&eid) {
                    map.insert(eid, comm_id);
                }
            }
            let done = inner.communities_loaded || self.storage.is_none() || entity_ids.is_empty();
            (map, done)
        };

        if done {
            return Ok(map);
        }

        if let Some(ref storage) = self.storage {
            let entries = storage.scan_prefix(GRAPH_COMMUNITY_PREFIX).await?;
            let mut inner = self.inner_write();
            inner.communities_loaded = true;
            for (raw_key, raw_val) in entries {
                if let Some(key_payload) = raw_key.get(GRAPH_COMMUNITY_PREFIX.len()..) {
                    if let Ok(key_str) = std::str::from_utf8(key_payload) {
                        let eid = EntityId::from(key_str);
                        if let Ok(comm_id) = serde_json::from_slice::<u64>(&raw_val) {
                            inner.communities.insert(eid, comm_id);
                        }
                    }
                }
            }
            let mut map = HashMap::with_capacity(entity_ids.len());
            for &eid in entity_ids {
                if let Some(&comm_id) = inner.communities.get(&eid) {
                    map.insert(eid, comm_id);
                }
            }
            Ok(map)
        } else {
            Ok(HashMap::new())
        }
    }

    /// Returns direct 1-hop outgoing neighbors of `start`.
    pub async fn neighbors(&self, start: EntityId) -> Result<Vec<EntityId>> {
        let inner = self.inner_read();
        let start_idx = match inner.id_map.get(&start) {
            Some(&idx) => idx,
            None => return Ok(Vec::new()),
        };
        if inner.entity_at(start_idx).is_none() {
            return Ok(Vec::new());
        }

        // HashSet für O(1)-Dedup statt O(k) Vec::contains
        let mut seen: std::collections::HashSet<EntityId> = std::collections::HashSet::new();
        let mut neighbors: Vec<EntityId> = Vec::new();

        // Helper-Closure für deduped Insert:
        let mut push_if_new = |id: EntityId| {
            if seen.insert(id) {
                neighbors.push(id);
            }
        };

        // 1. CSR targets
        if start_idx < inner.offsets.len() - 1 {
            for edge_idx in inner.offsets[start_idx]..inner.offsets[start_idx + 1] {
                let neighbor_idx = inner.targets[edge_idx];
                if !inner.tombstoned_edges.contains(&(start_idx, neighbor_idx))
                    && inner.entity_at(neighbor_idx).is_some()
                {
                    if let Some(&id) = inner.reverse_map.get(neighbor_idx) {
                        push_if_new(id);
                    }
                }
            }
        }
        // 2. Pending edges
        if let Some(pending) = inner.pending_edges.get(&start_idx) {
            for edge in pending {
                let neighbor_idx = edge.target;
                if !inner.tombstoned_edges.contains(&(start_idx, neighbor_idx))
                    && inner.entity_at(neighbor_idx).is_some()
                {
                    if let Some(&id) = inner.reverse_map.get(neighbor_idx) {
                        push_if_new(id);
                    }
                }
            }
        }
        Ok(neighbors)
    }

    /// Calculates PageRank for all entities in the graph using the CSR layout.
    pub async fn pagerank(
        &self,
        damping_factor: f32,
        max_iterations: usize,
        tolerance: f32,
    ) -> HashMap<EntityId, f32> {
        // Befund 2.1: compact_async() offloads CPU work if compaction is needed.
        // NOTE: This is an intermediate mitigation preventing Tokio runtime stalls during O(V+E) rebuilds;
        // exclusive write-lock scoping remains open for IP-08.
        if let Err(err) = self.compact_async().await {
            tracing::warn!(error = %err, "compact_async failed during pagerank compaction");
        }
        let inner = self.inner_read();
        let n = inner.reverse_map.len();
        if n == 0 {
            return HashMap::new();
        }

        let mut ranks = vec![1.0 / (n as f32); n];
        let d = damping_factor;

        // Out-degree per node
        let mut out_degree = vec![0usize; n];
        for (i, deg) in out_degree.iter_mut().enumerate().take(n) {
            if i < inner.offsets.len() - 1 {
                *deg = inner.offsets[i + 1] - inner.offsets[i];
            }
        }

        for _iter in 0..max_iterations {
            let mut next_ranks = vec![(1.0 - d) / (n as f32); n];

            // Account for dangling nodes (out_degree == 0)
            let dangling_sum: f32 = (0..n)
                .filter(|&i| out_degree[i] == 0)
                .map(|i| ranks[i])
                .sum();
            let dangling_contrib = d * dangling_sum / (n as f32);
            for r in &mut next_ranks {
                *r += dangling_contrib;
            }

            // Distribute rank across outgoing edges
            for i in 0..n {
                let deg = out_degree[i];
                if deg > 0 {
                    let share = d * ranks[i] / (deg as f32);
                    let start = inner.offsets[i];
                    let end = inner.offsets[i + 1];
                    for edge_idx in start..end {
                        let target = inner.targets[edge_idx];
                        next_ranks[target] += share;
                    }
                }
            }

            // Check convergence
            let diff: f32 = ranks
                .iter()
                .zip(next_ranks.iter())
                .map(|(a, b)| (a - b).abs())
                .sum();

            ranks = next_ranks;

            if diff < tolerance {
                break;
            }
        }

        let mut result = HashMap::new();
        for (idx, &rank) in ranks.iter().enumerate() {
            if inner.entity_at(idx).is_some() {
                if let Some(&id) = inner.reverse_map.get(idx) {
                    result.insert(id, rank);
                }
            }
        }
        result
    }

    /// Collects internal node indices of entities that are marked as deleted in storage.
    pub async fn get_deleted_node_indices(&self) -> HashSet<usize> {
        if self.storage.is_none() {
            return HashSet::new();
        }
        let entity_ids: Vec<(usize, EntityId)> = {
            let inner = self.inner_read();
            inner.reverse_map.iter().copied().enumerate().collect()
        };
        let mut deleted_indices = HashSet::new();
        for (idx, entity_id) in entity_ids {
            if self.is_entity_deleted(entity_id).await {
                deleted_indices.insert(idx);
            }
        }
        deleted_indices
    }

    /// Loads tombstone status of all graph nodes and constructs an authoritative [`crate::DeletedView`].
    pub async fn deleted_view(&self) -> crate::DeletedView {
        crate::DeletedView::from_nodes(self.get_deleted_node_indices().await)
    }

    /// Calculates Personalized PageRank (PPR) using a reusable [`crate::PprContext`] buffer to avoid allocations.
    pub async fn personalized_page_rank_with_context_async(
        &self,
        seed_nodes: &[EntityId],
        config: &memfuse_core::PprConfig,
        ctx: &mut crate::PprContext,
    ) -> Vec<(EntityId, f32)> {
        let deleted_view = self.deleted_view().await;
        // Befund 2.1: compact_async() offloads CPU work if compaction is needed.
        // NOTE: This is an intermediate mitigation preventing Tokio runtime stalls during O(V+E) rebuilds;
        // exclusive write-lock scoping remains open for IP-08.
        if let Err(err) = self.compact_async().await {
            tracing::warn!(
                error = %err,
                "compact_async failed during personalized_page_rank_with_context_async compaction"
            );
        }
        let inner = self.inner_read();
        crate::ppr::compute_ppr_with_context(&inner, seed_nodes, config, &deleted_view, ctx)
    }

    /// Returns the number of committed entities in the graph.
    pub fn entity_count(&self) -> usize {
        self.inner_read()
            .entities
            .iter()
            .filter(|e| e.id != EntityId::new(0))
            .count()
    }

    /// Checks if a committed entity exists in the graph.
    pub fn entity_exists(&self, id: EntityId) -> bool {
        let inner = self.inner_read();
        if let Some(&idx) = inner.id_map.get(&id) {
            inner.entity_at(idx).is_some()
        } else {
            false
        }
    }

    /// Prüft ob eine Entity per Tombstone als gelöscht markiert wurde.
    /// Nutzt den Key "graph:entity:deleted:{entity_id}" im LSM-Storage (wenn vorhanden).
    pub async fn is_entity_deleted(&self, entity: EntityId) -> bool {
        if let Some(storage) = &self.storage {
            let key = format!("graph:entity:deleted:{}", entity.0);
            return storage.get(key.as_bytes()).await.ok().flatten().is_some();
        }
        false
    }

    /// Returns the number of edges in the graph.
    pub fn edge_count(&self) -> usize {
        let inner = self.inner_read();
        inner.targets.len()
            + inner.pending_edge_count
            + inner.staged_edges.values().map(|v| v.len()).sum::<usize>()
    }

    /// Removes an entity node and all its incident (outgoing and incoming) edges from the graph.
    pub async fn remove_entity(&self, tx: TxId, entity: EntityId) -> Result<()> {
        GraphIndexExt::remove_entity(self, tx, entity).await
    }
}

impl GraphIndexExt for CsrGraph {
    fn remove_entity<'a>(&'a self, tx: TxId, entity: EntityId) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let (target_idx, outgoing_targets, incoming_sources) = {
                let inner = self.inner_read();
                let idx = match inner.id_map.get(&entity) {
                    Some(&i) => i,
                    None => return Ok(()),
                };

                let mut outgoing = Vec::new();
                let mut incoming = Vec::new();

                // a. Outgoing edges (entity -> *)
                if idx < inner.offsets.len() - 1 {
                    for j in inner.offsets[idx]..inner.offsets[idx + 1] {
                        let t_idx = inner.targets[j];
                        if !inner.tombstoned_edges.contains(&(idx, t_idx)) {
                            if let Some(&t_id) = inner.reverse_map.get(t_idx) {
                                outgoing.push(t_id);
                            }
                        }
                    }
                }
                if let Some(pending) = inner.pending_edges.get(&idx) {
                    for edge in pending {
                        if !inner.tombstoned_edges.contains(&(idx, edge.target)) {
                            if let Some(&t_id) = inner.reverse_map.get(edge.target) {
                                outgoing.push(t_id);
                            }
                        }
                    }
                }

                // b. Incoming edges (* -> entity)
                let num_nodes = inner.reverse_map.len();
                for node_idx in 0..num_nodes {
                    let source_id = match inner.reverse_map.get(node_idx) {
                        Some(&id) => id,
                        None => continue,
                    };

                    if node_idx < inner.offsets.len() - 1 {
                        for j in inner.offsets[node_idx]..inner.offsets[node_idx + 1] {
                            if inner.targets[j] == idx
                                && !inner.tombstoned_edges.contains(&(node_idx, idx))
                            {
                                incoming.push(source_id);
                            }
                        }
                    }
                    if let Some(pending) = inner.pending_edges.get(&node_idx) {
                        for edge in pending {
                            if edge.target == idx
                                && !inner.tombstoned_edges.contains(&(node_idx, idx))
                            {
                                incoming.push(source_id);
                            }
                        }
                    }
                }

                (idx, outgoing, incoming)
            };

            // Tombstone outgoing edges (entity -> to)
            for to_id in outgoing_targets {
                GraphIndex::remove_edge(self, tx, entity, to_id).await?;
            }

            // Tombstone incoming edges (from -> entity)
            for from_id in incoming_sources {
                GraphIndex::remove_edge(self, tx, from_id, entity).await?;
            }

            // c. Den Knoten selbst aus inner.id_map entfernen
            {
                let mut inner = self.inner_write();
                inner.id_map.remove(&entity);
                if target_idx < inner.entities.len() {
                    inner.entities[target_idx] = sentinel_entity();
                }
                inner.communities.remove(&entity);
                inner.is_dirty = true;
            }

            // LSM-Storage Marker / Deletion schreiben
            if let Some(ref storage) = self.storage {
                let entity_key = [GRAPH_ENTITY_PREFIX, entity.as_bytes().as_slice()].concat();
                storage.delete(tx, &entity_key).await?;

                let deleted_key =
                    [GRAPH_ENTITY_DELETED_PREFIX, entity.as_bytes().as_slice()].concat();
                storage
                    .put(tx, &deleted_key, &tx.inner().to_le_bytes())
                    .await?;
            }

            // d. Committe die Änderungen
            GraphIndex::commit(self, tx).await?;

            Ok(())
        })
    }
}

impl Default for CsrGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphIndex for CsrGraph {
    fn add_entity<'a>(&'a self, tx: TxId, entity: Entity) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            debug_assert!(
            tx != TxId::INVALID && tx.is_valid_origin(),
            "TxId {} verletzt AGT-GRAPH-001 Origin-Invariante — Sentinel TxId(0) oder Wall-Clock-abgeleitete IDs korrumpieren rollback_to_tx()-Kausalordnung",
            tx
        );
            // AGT-GRAPH-001: Heuristik — wall-clock-abgeleitete oder unallozierte TxIds warnen.
            if is_suspicious_tx_id(tx) {
                tracing::warn!(
                tx_id = tx.inner(),
                hint = if tx == TxId::INVALID { "Sentinel TxId(0)" } else { "Wall-Clock-ns-Bereich" },
                "AGT-GRAPH-001: Verdächtiger oder unallozierter TxId in add_entity (weder im plausiblen next_tx-Bereich noch im INTERNAL_BASE-Bereich [u64::MAX - 1_000_000]) — \
                 möglicherweise unalloziert oder aus Wall-Clock-Nanosekunden abgeleitet. \
                 Rollback-Korrelation kann verletzt sein."
            );
            }
            // Lazy index allocation: Entity indices are assigned in commit(),
            // avoiding premature mutation of id_map/reverse_map on rollback.
            {
                let mut inner = self.inner_write();
                inner
                    .staged_entities
                    .insert((tx, entity.id), entity.clone());
            }

            if let Some(ref storage) = self.storage {
                self.persist_entity(storage.as_ref(), tx, &entity).await?;
            }
            Ok(())
        })
    }

    fn add_edge<'a>(&'a self, tx: TxId, edge: memfuse_core::Edge) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            debug_assert!(
            tx != TxId::INVALID && tx.is_valid_origin(),
            "TxId {} verletzt AGT-GRAPH-001 Origin-Invariante — Sentinel TxId(0) oder Wall-Clock-abgeleitete IDs korrumpieren rollback_to_tx()-Kausalordnung",
            tx
        );
            // AGT-GRAPH-001: Heuristik — wall-clock-abgeleitete oder unallozierte TxIds warnen.
            if is_suspicious_tx_id(tx) {
                tracing::warn!(
                tx_id = tx.inner(),
                hint = if tx == TxId::INVALID { "Sentinel TxId(0)" } else { "Wall-Clock-ns-Bereich" },
                "AGT-GRAPH-001: Verdächtiger oder unallozierter TxId in add_edge (weder im plausiblen next_tx-Bereich noch im INTERNAL_BASE-Bereich [u64::MAX - 1_000_000]) — \
                 möglicherweise unalloziert oder aus Wall-Clock-Nanosekunden abgeleitet. \
                 Rollback-Korrelation kann verletzt sein."
            );
            }
            if !edge.weight.is_finite() || edge.weight < 0.0 {
                return Err(MemFuseError::InvalidInput(format!(
                    "Invalid edge weight {}: weight must be finite and non-negative",
                    edge.weight
                )));
            }

            // Consistency-Check wenn aktiviert
            if let Some(ref enforcer_lock) = self.consistency_enforcer {
                let pred_hash = *blake3::hash(edge.label.as_bytes()).as_bytes();
                let assertion = EdgeAssertion {
                    subject: edge.from.inner(),
                    predicate_hash: pred_hash,
                    object_repr: edge.to.as_bytes(),
                };
                let mut enforcer = enforcer_lock.write();
                if let Some(pattern) = enforcer.check_before_insert(&assertion) {
                    if pattern.suppressed {
                        tracing::warn!(
                            suppression_count = pattern.contradiction_count,
                            "ConsistencyEnforcer: edge insertion suppressed by conflict pattern"
                        );
                        return Err(MemFuseError::PolicyViolation(
                            "Contradictory edge suppressed by consistency enforcer".to_string(),
                        ));
                    }
                    tracing::warn!(
                        "ConsistencyEnforcer: contradictory edge detected (not yet suppressed)"
                    );
                }
            }

            let tx_valid_from = edge.tx_valid_from.or(Some(tx));

            // Register source document provenance for cascading invalidation
            if let Some(doc_id) = edge.source_doc_id {
                self.doc_edge_index.record(doc_id, (edge.from, edge.to));
            }

            // Lazy index allocation: Store EntityIds directly in staged_edges.
            // Internal indices via get_or_create_index are allocated only during commit(),
            // ensuring rollback does not leak entity indices into id_map/reverse_map.
            {
                let mut inner = self.inner_write();
                inner
                    .staged_edges
                    .entry((tx, edge.from))
                    .or_default()
                    .push(StagedEdgePayload {
                        target: edge.to,
                        weight: edge.weight,
                        tx_valid_from,
                        tx_valid_to: edge.tx_valid_to,
                        business_valid_from: edge.business_valid_from,
                        business_valid_to: edge.business_valid_to,
                        source_doc_id: edge.source_doc_id,
                    });
            }

            if let Some(ref storage) = self.storage {
                let payload = PersistedEdgePayload {
                    weight: edge.weight,
                    tx_valid_from,
                    tx_valid_to: edge.tx_valid_to,
                    business_valid_from: edge.business_valid_from,
                    business_valid_to: edge.business_valid_to,
                    source_doc_id: edge.source_doc_id,
                };
                self.persist_edge(storage.as_ref(), tx, &edge.from, &edge.to, &payload)
                    .await?;
            }
            Ok(())
        })
    }

    fn personalized_page_rank<'a>(
        &'a self,
        seed_nodes: &'a [EntityId],
        config: &'a memfuse_core::PprConfig,
    ) -> BoxFuture<'a, Result<Vec<(EntityId, f32)>>> {
        Box::pin(async move {
            let deleted_view = self.deleted_view().await;
            // Befund 2.1: Call compact_async() to prevent Tokio runtime stalls during O(V+E) rebuilds.
            // NOTE: Intermediate mitigation; exclusive write-lock scoping remains open for IP-08.
            if let Err(err) = self.compact_async().await {
                tracing::warn!(
                    error = %err,
                    "compact_async failed during personalized_page_rank compaction"
                );
            }
            let inner = self.inner_read();
            Ok(crate::ppr::compute_ppr(
                &inner,
                seed_nodes,
                config,
                &deleted_view,
            ))
        })
    }

    fn traverse_at<'a>(
        &'a self,
        start_node: EntityId,
        max_hops: usize,
        seq_no: u64,
    ) -> BoxFuture<'a, Result<Vec<(EntityId, f32)>>> {
        Box::pin(async move {
            self.traverse_at_time(start_node, max_hops, TxId::new(seq_no))
                .await
        })
    }

    fn traverse_at_time<'a>(
        &'a self,
        start: EntityId,
        max_hops: usize,
        as_of: TxId,
    ) -> BoxFuture<'a, Result<Vec<(EntityId, f32)>>> {
        Box::pin(async move {
            self.traverse_at_bitemporal(start, max_hops, as_of, None)
                .await
        })
    }

    fn traverse_at_bitemporal<'a>(
        &'a self,
        start: EntityId,
        max_hops: usize,
        as_of_tx: TxId,
        as_of_business: Option<i64>,
    ) -> BoxFuture<'a, Result<Vec<(EntityId, f32)>>> {
        Box::pin(async move {
            if max_hops > 100 {
                return Err(MemFuseError::InvalidInput(format!(
                    "max_hops {max_hops} exceeds upper safety limit of 100"
                )));
            }
            if max_hops > MAX_TRAVERSAL_HOPS as usize {
                tracing::warn!(
                requested_max_hops = max_hops,
                effective_max_hops = MAX_TRAVERSAL_HOPS,
                "traverse_at_bitemporal requested max_hops ({max_hops}) exceeds internal cap MAX_TRAVERSAL_HOPS ({MAX_TRAVERSAL_HOPS}); capping traversal depth"
            );
            }
            let inner = self.inner_read();
            let start_idx = match inner.id_map.get(&start) {
                Some(&idx) => idx,
                None => return Ok(Vec::new()),
            };

            if inner.entity_at(start_idx).is_none() {
                return Ok(Vec::new());
            }

            let effective_max = (max_hops as u8).min(MAX_TRAVERSAL_HOPS);

            let mut visited: HashMap<InternalIndex, f32> = HashMap::new();
            let mut queue: VecDeque<(InternalIndex, u8, f32)> = VecDeque::new();

            queue.push_back((start_idx, 0, 1.0));

            while let Some((node_idx, hop, current_score)) = queue.pop_front() {
                if hop > effective_max {
                    continue;
                }

                let existing = visited.entry(node_idx).or_insert(0.0);
                if current_score > *existing {
                    *existing = current_score;
                }

                if hop < effective_max {
                    if visited.len() >= MAX_VISITED_NODES {
                        tracing::warn!(
                        visited_count = visited.len(),
                        max_visited = MAX_VISITED_NODES,
                        "traverse_at_bitemporal visited node limit reached ({MAX_VISITED_NODES}); halting graph expansion"
                    );
                        break;
                    }

                    // 1. CSR traversal (compacted edges)
                    if node_idx < inner.offsets.len() - 1 {
                        let start_edge = inner.offsets[node_idx];
                        let end_edge = inner.offsets[node_idx + 1];

                        for edge_idx in start_edge..end_edge {
                            let neighbor_idx = inner.targets[edge_idx];
                            if inner.tombstoned_edges.contains(&(node_idx, neighbor_idx)) {
                                continue;
                            }
                            let tx_valid_from = inner.tx_valid_from_at(edge_idx);
                            let tx_valid_to = inner.tx_valid_to_at(edge_idx);
                            let business_valid_from = inner.business_valid_from_at(edge_idx);
                            let business_valid_to = inner.business_valid_to_at(edge_idx);

                            if !is_edge_visible_bitemporal(
                                tx_valid_from,
                                tx_valid_to,
                                as_of_tx,
                                business_valid_from,
                                business_valid_to,
                                as_of_business,
                            ) {
                                continue;
                            }
                            let weight = inner.weights[edge_idx];
                            let next_score = current_score * SCORE_DECAY * weight;

                            if (!visited.contains_key(&neighbor_idx)
                                || visited[&neighbor_idx] < next_score)
                                && inner.entity_at(neighbor_idx).is_some()
                            {
                                if !visited.contains_key(&neighbor_idx)
                                    && visited.len() + queue.len() >= MAX_VISITED_NODES
                                {
                                    tracing::warn!(
                                    visited_and_queued = visited.len() + queue.len(),
                                    max_visited = MAX_VISITED_NODES,
                                    "traverse_at_bitemporal visited node limit reached ({MAX_VISITED_NODES}); halting neighbor expansion"
                                );
                                    break;
                                }
                                queue.push_back((neighbor_idx, hop + 1, next_score));
                            }
                        }
                    }

                    // 2. Delta buffer traversal (uncompacted committed edges)
                    if let Some(pending) = inner.pending_edges.get(&node_idx) {
                        for edge in pending {
                            let neighbor_idx = edge.target;
                            if inner.tombstoned_edges.contains(&(node_idx, neighbor_idx)) {
                                continue;
                            }
                            if !is_edge_visible_bitemporal(
                                edge.tx_valid_from,
                                edge.tx_valid_to,
                                as_of_tx,
                                edge.business_valid_from,
                                edge.business_valid_to,
                                as_of_business,
                            ) {
                                continue;
                            }
                            let next_score = current_score * SCORE_DECAY * edge.weight;

                            if (!visited.contains_key(&neighbor_idx)
                                || visited[&neighbor_idx] < next_score)
                                && inner.entity_at(neighbor_idx).is_some()
                            {
                                if !visited.contains_key(&neighbor_idx)
                                    && visited.len() + queue.len() >= MAX_VISITED_NODES
                                {
                                    tracing::warn!(
                                    visited_and_queued = visited.len() + queue.len(),
                                    max_visited = MAX_VISITED_NODES,
                                    "traverse_at_bitemporal visited node limit reached ({MAX_VISITED_NODES}); halting neighbor expansion"
                                );
                                    break;
                                }
                                queue.push_back((neighbor_idx, hop + 1, next_score));
                            }
                        }
                    }
                }
            }

            visited.remove(&start_idx);

            let mut results: Vec<(EntityId, f32)> = visited
                .into_iter()
                .filter_map(|(idx, score)| inner.reverse_map.get(idx).map(|&id| (id, score)))
                .collect();

            results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

            Ok(results)
        })
    }

    fn traverse<'a>(
        &'a self,
        start: EntityId,
        max_hops: usize,
    ) -> BoxFuture<'a, Result<Vec<(EntityId, f32)>>> {
        Box::pin(async move {
            if max_hops > 100 {
                return Err(MemFuseError::InvalidInput(format!(
                    "max_hops {max_hops} exceeds upper safety limit of 100"
                )));
            }
            if max_hops > MAX_TRAVERSAL_HOPS as usize {
                tracing::warn!(
                requested_max_hops = max_hops,
                effective_max_hops = MAX_TRAVERSAL_HOPS,
                "traverse requested max_hops ({max_hops}) exceeds internal cap MAX_TRAVERSAL_HOPS ({MAX_TRAVERSAL_HOPS}); capping traversal depth"
            );
            }

            // Merge-read: read directly from both compacted CSR arrays AND uncompacted pending_edges delta buffer.
            // No full compact() call is required before traversal.
            let inner = self.inner_read();
            let start_idx = match inner.id_map.get(&start) {
                Some(&idx) => idx,
                None => return Ok(Vec::new()), // Start node not in graph
            };

            // If the start node itself is not committed, we shouldn't start traversal from it
            if inner.entity_at(start_idx).is_none() {
                return Ok(Vec::new());
            }

            let effective_max = (max_hops as u8).min(MAX_TRAVERSAL_HOPS);

            // BFS with score decay
            let mut visited: HashMap<InternalIndex, f32> = HashMap::new();
            let mut queue: VecDeque<(InternalIndex, u8, f32)> = VecDeque::new();

            queue.push_back((start_idx, 0, 1.0));

            while let Some((node_idx, hop, current_score)) = queue.pop_front() {
                if hop > effective_max {
                    continue;
                }

                // Only keep the best score per node
                let existing = visited.entry(node_idx).or_insert(0.0);
                if current_score > *existing {
                    *existing = current_score;
                }

                if hop < effective_max {
                    if visited.len() >= MAX_VISITED_NODES {
                        tracing::warn!(
                        visited_count = visited.len(),
                        max_visited = MAX_VISITED_NODES,
                        "traverse visited node limit reached ({MAX_VISITED_NODES}); halting graph expansion"
                    );
                        break;
                    }

                    // 1. CSR traversal (compacted edges)
                    if node_idx < inner.offsets.len() - 1 {
                        let start_edge = inner.offsets[node_idx];
                        let end_edge = inner.offsets[node_idx + 1];

                        for edge_idx in start_edge..end_edge {
                            let neighbor_idx = inner.targets[edge_idx];
                            if inner.tombstoned_edges.contains(&(node_idx, neighbor_idx)) {
                                continue;
                            }
                            let weight = inner.weights[edge_idx];
                            let next_score = current_score * SCORE_DECAY * weight;

                            if !visited.contains_key(&neighbor_idx)
                                || visited[&neighbor_idx] < next_score
                            {
                                // Only visit nodes that have a committed entity (FIND-GRA-001)
                                if inner.entity_at(neighbor_idx).is_some() {
                                    if !visited.contains_key(&neighbor_idx)
                                        && visited.len() + queue.len() >= MAX_VISITED_NODES
                                    {
                                        tracing::warn!(
                                        visited_and_queued = visited.len() + queue.len(),
                                        max_visited = MAX_VISITED_NODES,
                                        "traverse visited node limit reached ({MAX_VISITED_NODES}); halting neighbor expansion"
                                    );
                                        break;
                                    }
                                    queue.push_back((neighbor_idx, hop + 1, next_score));
                                }
                            }
                        }
                    }

                    // 2. Delta buffer traversal (uncompacted committed edges)
                    if let Some(pending) = inner.pending_edges.get(&node_idx) {
                        for edge in pending {
                            let neighbor_idx = edge.target;
                            if inner.tombstoned_edges.contains(&(node_idx, neighbor_idx)) {
                                continue;
                            }
                            let next_score = current_score * SCORE_DECAY * edge.weight;

                            if (!visited.contains_key(&neighbor_idx)
                                || visited[&neighbor_idx] < next_score)
                                && inner.entity_at(neighbor_idx).is_some()
                            {
                                if !visited.contains_key(&neighbor_idx)
                                    && visited.len() + queue.len() >= MAX_VISITED_NODES
                                {
                                    tracing::warn!(
                                    visited_and_queued = visited.len() + queue.len(),
                                    max_visited = MAX_VISITED_NODES,
                                    "traverse visited node limit reached ({MAX_VISITED_NODES}); halting neighbor expansion"
                                );
                                    break;
                                }
                                queue.push_back((neighbor_idx, hop + 1, next_score));
                            }
                        }
                    }
                }
            }

            // Remove the start node from results
            visited.remove(&start_idx);

            let mut results: Vec<(EntityId, f32)> = visited
                .into_iter()
                .filter_map(|(idx, score)| inner.reverse_map.get(idx).map(|&id| (id, score)))
                .collect();

            // Sort by score descending
            results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

            Ok(results)
        })
    }

    fn commit<'a>(&'a self, tx: TxId) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            debug_assert!(
            tx != TxId::INVALID && tx.is_valid_origin(),
            "TxId {} verletzt AGT-GRAPH-001 Origin-Invariante — Sentinel TxId(0) oder Wall-Clock-abgeleitete IDs korrumpieren rollback_to_tx()-Kausalordnung",
            tx
        );
            // AGT-GRAPH-001: Heuristik — wall-clock-abgeleitete oder unallozierte TxIds warnen.
            if is_suspicious_tx_id(tx) {
                tracing::warn!(
                tx_id = tx.inner(),
                hint = if tx == TxId::INVALID { "Sentinel TxId(0)" } else { "Wall-Clock-ns-Bereich" },
                "AGT-GRAPH-001: Verdächtiger oder unallozierter TxId in commit (weder im plausiblen next_tx-Bereich noch im INTERNAL_BASE-Bereich [u64::MAX - 1_000_000]) — \
                 möglicherweise unalloziert oder aus Wall-Clock-Nanosekunden abgeleitet. \
                 Rollback-Korrelation kann verletzt sein."
            );
            }

            let mut inner = self.inner_write();

            // 1. Commit entities
            let mut tx_entities = Vec::new();
            inner.staged_entities.retain(|&(t, id), entity| {
                if t == tx {
                    tx_entities.push((id, entity.clone()));
                    false
                } else {
                    true
                }
            });
            if !tx_entities.is_empty() {
                tx_entities.sort_by_key(|(id, _)| *id);
                for (id, entity) in tx_entities {
                    let idx = inner.get_or_create_index(id);
                    if idx >= inner.entities.len() {
                        inner.entities.resize(idx + 1, sentinel_entity());
                    }
                    inner.entities[idx] = entity;
                }
                inner.is_dirty = true;
            }

            // 2. Commit edges (lazy index resolution occurs here)
            let mut tx_edges = Vec::new();
            inner.staged_edges.retain(|&(t, from_id), edges| {
                if t == tx {
                    tx_edges.push((from_id, std::mem::take(edges)));
                    false
                } else {
                    true
                }
            });
            if !tx_edges.is_empty() {
                tx_edges.sort_by_key(|(from_id, _)| *from_id);
                for (from_id, edges) in tx_edges {
                    let from_idx = inner.get_or_create_index(from_id);
                    let mut converted_edges = Vec::with_capacity(edges.len());
                    for edge in edges {
                        let to_idx = inner.get_or_create_index(edge.target);
                        if let Some(doc_id) = edge.source_doc_id {
                            inner
                                .doc_to_edges
                                .entry(doc_id)
                                .or_default()
                                .insert((from_id, edge.target));
                        }
                        converted_edges.push(EdgePayload {
                            target: to_idx,
                            weight: edge.weight,
                            tx_valid_from: edge.tx_valid_from,
                            tx_valid_to: edge.tx_valid_to,
                            business_valid_from: edge.business_valid_from,
                            business_valid_to: edge.business_valid_to,
                            source_doc_id: edge.source_doc_id,
                        });
                    }
                    for edge in &converted_edges {
                        inner.add_to_out_weight_sum(from_idx, edge.weight);
                    }
                    let count = converted_edges.len();
                    inner
                        .pending_edges
                        .entry(from_idx)
                        .or_default()
                        .extend(converted_edges);
                    inner.pending_edge_count += count;
                }
                inner.is_dirty = true;
            }

            // 3. Commit removals
            if let Some(tx_removals) = inner.staged_removals.remove(&tx) {
                for (from_id, to_id) in tx_removals {
                    let from_idx = inner.id_map.get(&from_id).copied();
                    let to_idx = inner.id_map.get(&to_id).copied();
                    if let (Some(f_idx), Some(t_idx)) = (from_idx, to_idx) {
                        if let Some(pending) = inner.pending_edges.get_mut(&f_idx) {
                            pending.retain(|edge| edge.target != t_idx);
                        }
                        inner.tombstoned_edges.insert((f_idx, t_idx));
                        inner.is_dirty = true;
                        inner.recompute_node_out_weight_sum(f_idx);
                    }
                }
            }

            // Auto-rebuild CSR arrays if pending delta buffer reaches or exceeds threshold
            if inner.pending_edge_count >= self.config.rebuild_threshold {
                inner.compact();
            }

            self.last_tx_id.fetch_max(tx.inner(), Ordering::SeqCst);

            Ok(())
        })
    }

    fn remove_edge<'a>(
        &'a self,
        tx: TxId,
        from: EntityId,
        to: EntityId,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            debug_assert!(
            tx != TxId::INVALID && tx.is_valid_origin(),
            "TxId {} verletzt AGT-GRAPH-001 Origin-Invariante — Sentinel TxId(0) oder Wall-Clock-abgeleitete IDs korrumpieren rollback_to_tx()-Kausalordnung",
            tx
        );
            if is_suspicious_tx_id(tx) {
                tracing::warn!(
                tx_id = tx.inner(),
                hint = if tx == TxId::INVALID { "Sentinel TxId(0)" } else { "Wall-Clock-ns-Bereich" },
                "AGT-GRAPH-001: Verdächtiger oder unallozierter TxId in remove_edge (weder im plausiblen next_tx-Bereich noch im INTERNAL_BASE-Bereich [u64::MAX - 1_000_000]) — \
                 möglicherweise unalloziert oder aus Wall-Clock-Nanosekunden abgeleitet."
            );
            }
            {
                let mut inner = self.inner_write();
                inner
                    .staged_removals
                    .entry(tx)
                    .or_default()
                    .push((from, to));
            }
            if let Some(ref storage) = self.storage {
                self.delete_edge_persistence(storage.as_ref(), tx, &from, &to)
                    .await?;
            }
            Ok(())
        })
    }

    fn add_bidirectional<'a>(
        &'a self,
        tx: TxId,
        from: EntityId,
        to: EntityId,
        label: &'a str,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            self.add_edge(tx, memfuse_core::Edge::new(from, to, label))
                .await?;
            self.add_edge(tx, memfuse_core::Edge::new(to, from, label))
                .await?;
            Ok(())
        })
    }

    fn neighbors<'a>(&'a self, start: EntityId) -> BoxFuture<'a, Result<Vec<EntityId>>> {
        Box::pin(async move { self.neighbors(start).await })
    }

    fn rollback<'a>(&'a self, tx: TxId) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let mut inner = self.inner_write();
            inner.staged_entities.retain(|&(t, _), _| t != tx);
            inner.staged_edges.retain(|&(t, _), _| t != tx);
            inner.staged_removals.remove(&tx);
            Ok(())
        })
    }

    fn rollback_to_tx<'a>(&'a self, _tx_id: TxId) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            // Physical rollback for CSR graph is driven by WAL replay or reloading state from storage.
            // In-memory staged transactions are handled by rollback().
            Ok(())
        })
    }

    fn last_tx_id<'a>(&'a self) -> BoxFuture<'a, Result<TxId>> {
        Box::pin(async move { Ok(TxId::new(self.last_tx_id.load(Ordering::SeqCst))) })
    }

    fn len<'a>(&'a self) -> BoxFuture<'a, usize> {
        Box::pin(async move { self.entity_count() })
    }

    fn stats<'a>(&'a self) -> BoxFuture<'a, Result<GraphIndexStats>> {
        Box::pin(async move {
            let inner = self.inner_read();
            let num_entities = inner
                .entities
                .iter()
                .filter(|e| e.id != EntityId::new(0))
                .count();
            let num_edges = inner.targets.len()
                + inner.pending_edge_count
                + inner.staged_edges.values().map(|v| v.len()).sum::<usize>();

            let mem = (inner.reverse_map.len() * std::mem::size_of::<EntityId>())
                + (inner.entities.len() * std::mem::size_of::<Entity>())
                + (inner.offsets.len() * std::mem::size_of::<usize>())
                + (inner.targets.len() * std::mem::size_of::<usize>())
                + (inner.weights.len() * std::mem::size_of::<f32>());

            Ok(GraphIndexStats {
                num_entities,
                num_edges,
                memory_usage_bytes: mem,
            })
        })
    }
}

impl crate::path_rag::PathGraph for CsrGraph {
    fn neighbors_with_weights(&self, node: EntityId) -> Vec<(EntityId, f32)> {
        let inner = self.inner_read();
        let node_idx = match inner.id_map.get(&node) {
            Some(&idx) => idx,
            None => return Vec::new(),
        };
        if inner.entity_at(node_idx).is_none() {
            return Vec::new();
        }

        let mut result = Vec::new();
        let mut seen = std::collections::HashSet::new();

        if node_idx < inner.offsets.len() - 1 {
            let start_edge = inner.offsets[node_idx];
            let end_edge = inner.offsets[node_idx + 1];
            for edge_idx in start_edge..end_edge {
                let neighbor_idx = inner.targets[edge_idx];
                if !inner.tombstoned_edges.contains(&(node_idx, neighbor_idx))
                    && inner.entity_at(neighbor_idx).is_some()
                {
                    if let Some(&id) = inner.reverse_map.get(neighbor_idx) {
                        if seen.insert(id) {
                            result.push((id, inner.weights[edge_idx]));
                        }
                    }
                }
            }
        }

        if let Some(pending) = inner.pending_edges.get(&node_idx) {
            for edge in pending {
                let neighbor_idx = edge.target;
                if !inner.tombstoned_edges.contains(&(node_idx, neighbor_idx))
                    && inner.entity_at(neighbor_idx).is_some()
                {
                    if let Some(&id) = inner.reverse_map.get(neighbor_idx) {
                        if seen.insert(id) {
                            result.push((id, edge.weight));
                        }
                    }
                }
            }
        }

        result
    }

    fn predecessors_with_weights(&self, node: EntityId) -> Vec<(EntityId, f32)> {
        let inner = self.inner_read();
        let target_idx = match inner.id_map.get(&node) {
            Some(&idx) => idx,
            None => return Vec::new(),
        };
        if inner.entity_at(target_idx).is_none() {
            return Vec::new();
        }

        let mut result = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let num_nodes = inner.reverse_map.len();

        for u_idx in 0..num_nodes {
            if inner.entity_at(u_idx).is_none() {
                continue;
            }
            let u_id = match inner.reverse_map.get(u_idx) {
                Some(&id) => id,
                None => continue,
            };

            if u_idx < inner.offsets.len() - 1 {
                let start_edge = inner.offsets[u_idx];
                let end_edge = inner.offsets[u_idx + 1];
                for edge_idx in start_edge..end_edge {
                    if inner.targets[edge_idx] == target_idx
                        && !inner.tombstoned_edges.contains(&(u_idx, target_idx))
                        && seen.insert(u_id)
                    {
                        result.push((u_id, inner.weights[edge_idx]));
                    }
                }
            }

            if let Some(pending) = inner.pending_edges.get(&u_idx) {
                for edge in pending {
                    if edge.target == target_idx
                        && !inner.tombstoned_edges.contains(&(u_idx, target_idx))
                        && seen.insert(u_id)
                    {
                        result.push((u_id, edge.weight));
                    }
                }
            }
        }

        result
    }

    fn hyperedges_for_entity(&self, node: EntityId) -> Vec<crate::hyperedge::HyperEdgeId> {
        self.hyperedges_for_entity(node)
    }

    fn get_hyperedge(
        &self,
        id: crate::hyperedge::HyperEdgeId,
    ) -> Option<crate::hyperedge::HyperEdge> {
        self.get_hyperedge(id)
    }
}

impl crate::path_rag::PathGraph for &CsrGraph {
    fn neighbors_with_weights(&self, node: EntityId) -> Vec<(EntityId, f32)> {
        (*self).neighbors_with_weights(node)
    }
    fn predecessors_with_weights(&self, node: EntityId) -> Vec<(EntityId, f32)> {
        (*self).predecessors_with_weights(node)
    }
    fn hyperedges_for_entity(&self, node: EntityId) -> Vec<crate::hyperedge::HyperEdgeId> {
        (*self).hyperedges_for_entity(node)
    }
    fn get_hyperedge(
        &self,
        id: crate::hyperedge::HyperEdgeId,
    ) -> Option<crate::hyperedge::HyperEdge> {
        (*self).get_hyperedge(id)
    }
}

impl crate::path_rag::PathGraph for Arc<CsrGraph> {
    fn neighbors_with_weights(&self, node: EntityId) -> Vec<(EntityId, f32)> {
        self.as_ref().neighbors_with_weights(node)
    }
    fn predecessors_with_weights(&self, node: EntityId) -> Vec<(EntityId, f32)> {
        self.as_ref().predecessors_with_weights(node)
    }
    fn hyperedges_for_entity(&self, node: EntityId) -> Vec<crate::hyperedge::HyperEdgeId> {
        self.as_ref().hyperedges_for_entity(node)
    }
    fn get_hyperedge(
        &self,
        id: crate::hyperedge::HyperEdgeId,
    ) -> Option<crate::hyperedge::HyperEdge> {
        self.as_ref().get_hyperedge(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use memfuse_core::Edge;

    async fn setup_test_graph() -> CsrGraph {
        let graph = CsrGraph::new();
        let tx = TxId::new(1);

        for id in 1..=5 {
            graph
                .add_entity(
                    tx,
                    Entity::new(EntityId::new(id), format!("P{}", id), "Person"),
                )
                .await
                .expect("valid setup"); // expect
        }

        graph
            .add_edge(
                tx,
                Edge::new(EntityId::new(1), EntityId::new(2), "knows").with_weight(1.0),
            )
            .await
            .expect("valid edge"); // expect
        graph
            .add_edge(
                tx,
                Edge::new(EntityId::new(2), EntityId::new(3), "knows").with_weight(0.8),
            )
            .await
            .expect("valid edge"); // expect
        graph
            .add_edge(
                tx,
                Edge::new(EntityId::new(3), EntityId::new(4), "knows").with_weight(0.6),
            )
            .await
            .expect("valid edge"); // expect
        graph
            .add_edge(
                tx,
                Edge::new(EntityId::new(4), EntityId::new(5), "knows").with_weight(0.5),
            )
            .await
            .expect("valid edge"); // expect
        graph
            .add_edge(
                tx,
                Edge::new(EntityId::new(2), EntityId::new(5), "knows").with_weight(0.4),
            )
            .await
            .expect("valid edge"); // expect

        graph.commit(tx).await.expect("commit"); // expect
        graph.compact();
        graph
    }

    #[tokio::test]
    async fn test_sentinel_representation_no_value_cases() {
        let graph = Arc::new(CsrGraph::new());
        let tx = TxId::new(1);
        let id1 = EntityId::new(10);
        let id2 = EntityId::new(20);

        graph
            .add_entity(tx, Entity::new(id1, "Node10", "Type"))
            .await
            .expect("add entity");
        graph
            .add_entity(tx, Entity::new(id2, "Node20", "Type"))
            .await
            .expect("add entity");

        // Edge with no optional validities or doc_ids inserted directly with None validities
        graph
            .add_edge(id1, id2, 1.0, None, None, None, None, None, None, None)
            .await
            .expect("add edge");
        graph.commit(tx).await.expect("commit");
        graph.compact();

        let inner = graph.inner_read();
        assert_eq!(inner.tx_valid_froms[0], TxId::INVALID);
        assert_eq!(inner.tx_valid_tos[0], TxId::INVALID);
        assert_eq!(inner.business_valid_froms[0], i64::MIN);
        assert_eq!(inner.business_valid_tos[0], i64::MIN);
        assert_eq!(inner.source_doc_ids[0], DocId::new(0));

        // Helper getters must return None for sentinel values
        assert_eq!(inner.tx_valid_from_at(0), None);
        assert_eq!(inner.tx_valid_to_at(0), None);
        assert_eq!(inner.business_valid_from_at(0), None);
        assert_eq!(inner.business_valid_to_at(0), None);
        assert_eq!(inner.source_doc_id_at(0), None);
    }

    #[tokio::test]
    async fn test_invalid_edge_weights_rejected() {
        let graph = Arc::new(CsrGraph::new());
        let tx = TxId::new(1);
        let id1 = EntityId::new(1);
        let id2 = EntityId::new(2);

        // NaN weight
        let err_nan = graph
            .insert_edge_direct(id1, id2, f32::NAN)
            .await
            .unwrap_err();
        assert!(matches!(err_nan, MemFuseError::InvalidInput(_)));

        // Infinity weight
        let err_inf = graph
            .insert_edge_direct(id1, id2, f32::INFINITY)
            .await
            .unwrap_err();
        assert!(matches!(err_inf, MemFuseError::InvalidInput(_)));

        // Neg Infinity weight
        let err_neginf = graph
            .insert_edge_direct(id1, id2, f32::NEG_INFINITY)
            .await
            .unwrap_err();
        assert!(matches!(err_neginf, MemFuseError::InvalidInput(_)));

        // Negative weight
        let err_neg = graph.insert_edge_direct(id1, id2, -1.0).await.unwrap_err();
        assert!(matches!(err_neg, MemFuseError::InvalidInput(_)));

        // add_edge with NaN
        let edge_nan = Edge::new(id1, id2, "rel").with_weight(f32::NAN);
        let err_add = GraphIndex::add_edge(graph.as_ref(), tx, edge_nan)
            .await
            .unwrap_err();
        assert!(matches!(err_add, MemFuseError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_compact_entities_without_edges_syncs_offsets() {
        let graph = CsrGraph::new();
        let tx = TxId::new(1);

        // Add 5 entities without adding any edges
        for id in 1..=5 {
            graph
                .add_entity(
                    tx,
                    Entity::new(EntityId::new(id), format!("E{id}"), "Entity"),
                )
                .await
                .unwrap(); // unwrap
        }

        graph.commit(tx).await.unwrap(); // unwrap

        // Before compact(), reverse_map has 5 entities
        {
            let inner = graph.inner_read();
            assert_eq!(inner.reverse_map.len(), 5);
        }

        graph.compact();

        // After compact(), offsets length MUST equal reverse_map.len() + 1 = 6
        {
            let inner = graph.inner_read();
            assert_eq!(inner.reverse_map.len(), 5);
            assert_eq!(inner.offsets.len(), 6);
            assert_eq!(inner.offsets, vec![0, 0, 0, 0, 0, 0]);
        }
    }

    #[tokio::test]
    async fn test_csr_graph_compact_layout() {
        let graph = CsrGraph::new();
        let tx = TxId::new(1);

        graph
            .add_entity(tx, Entity::new(EntityId::new(1), "A", "T"))
            .await
            .unwrap(); // unwrap
        graph
            .add_entity(tx, Entity::new(EntityId::new(2), "B", "T"))
            .await
            .unwrap(); // unwrap
        graph
            .add_edge(tx, Edge::new(EntityId::new(1), EntityId::new(2), "E"))
            .await
            .unwrap(); // unwrap

        graph.commit(tx).await.unwrap(); // unwrap

        {
            let inner = graph.inner_read();
            assert!(inner.is_dirty);
            assert_eq!(inner.staged_edges.len(), 0);
            assert_eq!(inner.targets.len(), 0);
        }

        graph.compact();

        {
            let inner = graph.inner_read();
            assert!(!inner.is_dirty);
            assert_eq!(inner.staged_edges.len(), 0);
            assert_eq!(inner.targets.len(), 1);
            assert_eq!(inner.offsets.len(), 3);
            assert_eq!(inner.offsets[0], 0);
            assert_eq!(inner.offsets[1] + (inner.offsets[2] - inner.offsets[1]), 1);
        }
    }

    #[tokio::test]
    async fn test_csr_delta_buffer_uncompacted_traversal() {
        // Test that committed edges in the pending_edges delta buffer (uncompacted)
        // are correctly traversed without needing compact() call.
        let graph = CsrGraph::with_config(CsrGraphConfig {
            rebuild_threshold: 1000,
            ..Default::default()
        });
        let tx = TxId::new(1);

        graph
            .add_entity(tx, Entity::new(EntityId::new(1), "A", "Node"))
            .await
            .unwrap(); // unwrap
        graph
            .add_entity(tx, Entity::new(EntityId::new(2), "B", "Node"))
            .await
            .unwrap(); // unwrap
        graph
            .add_entity(tx, Entity::new(EntityId::new(3), "C", "Node"))
            .await
            .unwrap(); // unwrap

        graph
            .add_edge(
                tx,
                Edge::new(EntityId::new(1), EntityId::new(2), "knows").with_weight(1.0),
            )
            .await
            .unwrap(); // unwrap
        graph.commit(tx).await.unwrap(); // unwrap

        // Edge 1->2 is committed in pending_edges (uncompacted)
        {
            let inner = graph.inner_read();
            assert!(inner.is_dirty);
            assert_eq!(inner.pending_edge_count, 1);
            assert_eq!(inner.targets.len(), 0); // Not in CSR targets yet
        }

        // Traversal MUST find Entity 2 directly from pending_edges delta buffer
        let results = graph.traverse(EntityId::new(1), 1).await.unwrap(); // unwrap
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, EntityId::new(2));

        // Add second edge 2->3 in next transaction
        let tx2 = TxId::new(2);
        graph
            .add_edge(
                tx2,
                Edge::new(EntityId::new(2), EntityId::new(3), "knows").with_weight(0.8),
            )
            .await
            .unwrap(); // unwrap
        graph.commit(tx2).await.unwrap(); // unwrap

        // Traversal from 1 (max 2 hops) MUST find both 2 and 3 through delta buffer
        let results_2hop = graph.traverse(EntityId::new(1), 2).await.unwrap(); // unwrap
        assert_eq!(results_2hop.len(), 2);
        let ids: Vec<_> = results_2hop.iter().map(|(id, _)| id.inner()).collect();
        assert!(ids.contains(&2));
        assert!(ids.contains(&3));
    }

    #[tokio::test]
    async fn test_graph_transaction_isolation() {
        let graph = CsrGraph::new();
        let tx1 = TxId::new(1);

        // 1. Tx1 fügt Entity und Edge hinzu
        graph
            .add_entity(tx1, Entity::new(EntityId::new(1), "A", "T"))
            .await
            .unwrap(); // unwrap
        graph
            .add_entity(tx1, Entity::new(EntityId::new(2), "B", "T"))
            .await
            .unwrap(); // unwrap
        graph
            .add_edge(
                tx1,
                Edge::new(EntityId::new(1), EntityId::new(2), "E").with_weight(1.0),
            )
            .await
            .unwrap(); // unwrap

        // 2. Traverse (ohne Tx) darf Edge NICHT sehen
        let results = graph.traverse(EntityId::new(1), 1).await.unwrap(); // unwrap
        assert_eq!(results.len(), 0, "Uncommitted edge should not be visible");

        // 3. Tx1 committet
        graph.commit(tx1).await.unwrap(); // unwrap

        // 4. Traverse MUSS Edge sehen
        let results = graph.traverse(EntityId::new(1), 1).await.unwrap(); // unwrap
        assert_eq!(results.len(), 1, "Committed edge should be visible");
        assert_eq!(results[0].0, EntityId::new(2));
    }

    #[tokio::test]
    async fn test_graph_rollback_isolation() {
        let graph = CsrGraph::new();
        let tx1 = TxId::new(1);
        let tx2 = TxId::new(2);

        // 1. Tx1 und Tx2 fügen Edges hinzu
        graph
            .add_entity(tx1, Entity::new(EntityId::new(1), "A", "T"))
            .await
            .unwrap(); // unwrap
        graph
            .add_entity(tx1, Entity::new(EntityId::new(2), "B", "T"))
            .await
            .unwrap(); // unwrap
        graph
            .add_edge(
                tx1,
                Edge::new(EntityId::new(1), EntityId::new(2), "E1").with_weight(1.0),
            )
            .await
            .unwrap(); // unwrap

        graph
            .add_entity(tx2, Entity::new(EntityId::new(1), "A", "T"))
            .await
            .unwrap(); // unwrap
        graph
            .add_entity(tx2, Entity::new(EntityId::new(3), "C", "T"))
            .await
            .unwrap(); // unwrap
        graph
            .add_edge(
                tx2,
                Edge::new(EntityId::new(1), EntityId::new(3), "E2").with_weight(1.0),
            )
            .await
            .unwrap(); // unwrap

        // 2. Tx1 rollt back
        graph.rollback(tx1).await.unwrap(); // unwrap

        // 3. Tx2 committet
        graph.commit(tx2).await.unwrap(); // unwrap

        // 4. Nur Edges von Tx2 dürfen existieren
        let results = graph.traverse(EntityId::new(1), 1).await.unwrap(); // unwrap
        assert_eq!(results.len(), 1, "Only Tx2 edge should be visible");
        assert_eq!(results[0].0, EntityId::new(3));

        let stats = graph.stats().await.unwrap(); // unwrap
                                                  // With lazy index allocation, Tx1 rollback discards staged entities and edges,
                                                  // so Entity 2 is never registered in id_map/reverse_map.
        assert_eq!(
            stats.num_entities, 2,
            "Only entities from Tx2 and common ones should exist"
        );
    }

    #[tokio::test]
    async fn test_csr_graph_bfs_score_decay() {
        let graph = setup_test_graph().await;
        let results = graph.traverse(EntityId::new(1), 3).await.expect("traverse"); // expect

        assert_eq!(results.len(), 4);

        let score_map: std::collections::HashMap<_, _> = results.into_iter().collect();

        let s2 = *score_map.get(&EntityId::new(2)).expect("node 2 missing"); // expect
        let s3 = *score_map.get(&EntityId::new(3)).expect("node 3 missing"); // expect
        let s4 = *score_map.get(&EntityId::new(4)).expect("node 4 missing"); // expect
        let s5 = *score_map.get(&EntityId::new(5)).expect("node 5 missing"); // expect

        assert!((s2 - 0.7).abs() < f32::EPSILON);
        assert!((s3 - 0.392).abs() < f32::EPSILON);
        assert!((s5 - 0.196).abs() < f32::EPSILON);
        assert!((s4 - 0.16464).abs() < f32::EPSILON);
    }

    #[tokio::test]
    async fn test_csr_graph_cycle_handling() {
        let graph = CsrGraph::new();
        let tx = TxId::new(1);
        graph
            .add_entity(tx, Entity::new(EntityId::new(1), "A", "N"))
            .await
            .unwrap(); // unwrap
        graph
            .add_entity(tx, Entity::new(EntityId::new(2), "B", "N"))
            .await
            .unwrap(); // unwrap
        graph
            .add_edge(tx, Edge::new(EntityId::new(1), EntityId::new(2), "E"))
            .await
            .unwrap(); // unwrap
        graph
            .add_edge(tx, Edge::new(EntityId::new(2), EntityId::new(1), "E"))
            .await
            .unwrap(); // unwrap

        graph.commit(tx).await.unwrap(); // unwrap

        let results = graph.traverse(EntityId::new(1), 5).await.expect("traverse"); // expect
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, EntityId::new(2));
    }

    #[tokio::test]
    async fn test_csr_graph_max_hop_enforcement() {
        let graph = setup_test_graph().await;

        // Traverse from 1, max hops 1 -> Should only find Node 2
        let results_hop1 = graph
            .traverse(EntityId::new(1), 1)
            .await
            .expect("traverse 1 hop"); // expect
        assert_eq!(results_hop1.len(), 1);
        assert_eq!(results_hop1[0].0, EntityId::new(2));

        // Traverse from 3, max hops 1 -> Should only find Node 4
        let results_hop1_n3 = graph
            .traverse(EntityId::new(3), 1)
            .await
            .expect("traverse 1 hop"); // expect
        assert_eq!(results_hop1_n3.len(), 1);
        assert_eq!(results_hop1_n3[0].0, EntityId::new(4));
    }

    #[tokio::test]
    async fn test_csr_graph_stats_accuracy() {
        let graph = setup_test_graph().await;
        let stats = graph.stats().await.expect("valid stats"); // expect

        assert_eq!(stats.num_entities, 5);
        assert_eq!(stats.num_edges, 5);

        // Calculate expected memory based on implementation
        let inner = graph.inner_read();
        let expected_mem = (inner.reverse_map.len() * std::mem::size_of::<EntityId>())
            + (inner.entities.len() * std::mem::size_of::<Option<Entity>>())
            + (inner.offsets.len() * std::mem::size_of::<usize>())
            + (inner.targets.len() * std::mem::size_of::<usize>())
            + (inner.weights.len() * std::mem::size_of::<f32>());

        assert_eq!(stats.memory_usage_bytes, expected_mem);
    }

    #[tokio::test]
    async fn test_add_edge_rollback_no_index_growth() {
        let graph = CsrGraph::new();

        {
            let inner = graph.inner_read();
            assert_eq!(inner.id_map.len(), 0);
            assert_eq!(inner.reverse_map.len(), 0);
        }

        for i in 1..=100 {
            let tx = TxId::new(i);
            let from = EntityId::new(i * 10);
            let to = EntityId::new(i * 10 + 1);

            graph
                .add_edge(tx, Edge::new(from, to, "test_rel"))
                .await
                .unwrap(); // unwrap

            graph.rollback(tx).await.unwrap(); // unwrap

            let inner = graph.inner_read();
            assert_eq!(
                inner.id_map.len(),
                0,
                "id_map must remain empty after rollback at iteration {i}"
            );
            assert_eq!(
                inner.reverse_map.len(),
                0,
                "reverse_map must remain empty after rollback at iteration {i}"
            );
        }
    }

    #[tokio::test]
    async fn test_compaction_excludes_uncommitted_edges() {
        let graph = setup_test_graph().await;

        // 1. Add uncommitted edges
        let tx_uncommitted = TxId::new(999);
        let edge1 = Edge::new(EntityId::new(1), EntityId::new(5), "").with_weight(0.5);
        graph.add_edge(tx_uncommitted, edge1).await.unwrap(); // unwrap

        // 2. Add committed edges
        let tx_committed = TxId::new(100);
        let edge2 = Edge::new(EntityId::new(1), EntityId::new(2), "").with_weight(0.9);
        graph.add_edge(tx_committed, edge2).await.unwrap(); // unwrap
        graph.commit(tx_committed).await.unwrap(); // unwrap

        // 3. Compact
        graph.compact();

        // 4. Verify traversal
        let results = graph.traverse(EntityId::new(1), 1).await.unwrap(); // unwrap
        let targets: Vec<_> = results.iter().map(|(id, _)| id.inner()).collect();

        // Should find committed edge (2) but NOT uncommitted edge (5)
        assert!(
            targets.contains(&2),
            "Expected Entity 2 in results, got {:?}",
            targets
        );
    }

    #[tokio::test]
    async fn test_suspicious_txid_does_not_silently_overwrite() {
        let graph = CsrGraph::new();

        // Simulated Quelle A: Kanonische TxId (z.B. 42)
        let tx_source_a = TxId::new(42);
        // Simulated Quelle B: Kollidierende TxId mit demselben Wert 42 aus anderer Herkunft
        let tx_source_b = TxId::new(42);

        graph
            .add_entity(
                tx_source_a,
                Entity::new(EntityId::new(10), "EntityFromA", "TypeA"),
            )
            .await
            .unwrap(); // unwrap

        // Staging unter gleicher TxId ueberschreibt staged entity fuer EntityId(10) in der staged HashMap
        graph
            .add_entity(
                tx_source_b,
                Entity::new(EntityId::new(10), "EntityFromB", "TypeB"),
            )
            .await
            .unwrap(); // unwrap

        graph.commit(tx_source_a).await.unwrap(); // unwrap

        // Nach Commit ist der Zustand deterministisch (letzte staged Entity gewinnt)
        let inner = graph.inner_read();
        let idx = inner.id_map.get(&EntityId::new(10)).unwrap(); // unwrap
        let entity = inner.entity_at(*idx).unwrap(); // unwrap
        assert_eq!(&*entity.name, "EntityFromB");
    }

    #[tokio::test]
    #[should_panic(expected = "AGT-GRAPH-001")]
    async fn test_wallclock_txid_debug_assert_panics() {
        let graph = CsrGraph::new();
        // Wall-clock-artiger TxId (~1.7e18 ns) in der verbotenen Gap-Zone zwischen 10^12 und INTERNAL_BASE
        let wallclock_tx = TxId::new(1_700_000_000_000_000_000);

        assert!(super::is_suspicious_tx_id(wallclock_tx));

        // In Debug-Builds MUSS debug_assert!(tx.is_valid_origin()) greifen und mit "AGT-GRAPH-001" paniquen
        let _ = graph
            .add_entity(
                wallclock_tx,
                Entity::new(EntityId::new(100), "WallClockEntity", "Type"),
            )
            .await;
    }

    #[tokio::test]
    #[should_panic(expected = "AGT-GRAPH-001")]
    async fn test_sentinel_txid_zero_debug_assert_panics() {
        let graph = CsrGraph::new();
        let invalid_tx = TxId::INVALID;

        assert!(super::is_suspicious_tx_id(invalid_tx));

        let _ = graph
            .add_entity(
                invalid_tx,
                Entity::new(EntityId::new(101), "SentinelEntity", "Type"),
            )
            .await;
    }

    #[tokio::test]
    async fn test_graph_operation_sequence_txid_determinism() {
        // Test executing identical operation sequences with canonical TxIds yields identical state
        let run_sequence = || async {
            let graph = CsrGraph::new();
            let mut committed_txs = Vec::new();

            for i in 1..=5u64 {
                let tx = TxId::new(i);
                let id1 = EntityId::new(i * 10);
                let id2 = EntityId::new(i * 10 + 1);

                graph
                    .add_entity(tx, Entity::new(id1, format!("E{}", id1.inner()), "Type"))
                    .await
                    .unwrap();
                graph
                    .add_entity(tx, Entity::new(id2, format!("E{}", id2.inner()), "Type"))
                    .await
                    .unwrap();
                graph
                    .add_edge(tx, Edge::new(id1, id2, "connects"))
                    .await
                    .unwrap();

                graph.commit(tx).await.unwrap();
                committed_txs.push(graph.last_tx_id().await.unwrap());
            }

            let stats = graph.stats().await.unwrap();
            (committed_txs, stats.num_entities, stats.num_edges)
        };

        let (txs1, ent1, edge1) = run_sequence().await;
        let (txs2, ent2, edge2) = run_sequence().await;

        assert_eq!(txs1, vec![TxId(1), TxId(2), TxId(3), TxId(4), TxId(5)]);
        assert_eq!(
            txs1, txs2,
            "TxId sequence must be deterministically identical across runs"
        );
        assert_eq!(ent1, ent2);
        assert_eq!(edge1, edge2);
    }

    #[tokio::test]
    async fn test_get_communities_batch_api() {
        let graph = CsrGraph::new();
        let assignments = vec![
            crate::CommunityAssignment {
                entity_id: EntityId::new(1),
                community_id: 100,
                hyperedges_included: false,
            },
            crate::CommunityAssignment {
                entity_id: EntityId::new(2),
                community_id: 200,
                hyperedges_included: false,
            },
        ];

        graph.set_communities_batch(&assignments);

        let map = graph
            .get_communities_batch(&[EntityId::new(1), EntityId::new(2), EntityId::new(3)])
            .await
            .unwrap(); // unwrap

        assert_eq!(map.len(), 2);
        assert_eq!(map.get(&EntityId::new(1)), Some(&100));
        assert_eq!(map.get(&EntityId::new(2)), Some(&200));
        assert_eq!(map.get(&EntityId::new(3)), None);
    }

    #[tokio::test]
    async fn test_neighbors_api() {
        let graph = CsrGraph::new();
        let tx = TxId::new(1);
        let id_a = EntityId::new(1);
        let id_b = EntityId::new(2);
        let id_c = EntityId::new(3);

        graph
            .add_entity(tx, Entity::new(id_a, "A", "T"))
            .await
            .unwrap(); // unwrap
        graph
            .add_entity(tx, Entity::new(id_b, "B", "T"))
            .await
            .unwrap(); // unwrap
        graph
            .add_entity(tx, Entity::new(id_c, "C", "T"))
            .await
            .unwrap(); // unwrap
        graph
            .add_edge(tx, Edge::new(id_a, id_b, "rel"))
            .await
            .unwrap(); // unwrap
        graph
            .add_edge(tx, Edge::new(id_a, id_c, "rel"))
            .await
            .unwrap(); // unwrap
        graph.commit(tx).await.unwrap(); // unwrap

        let n = graph.neighbors(id_a).await.unwrap(); // unwrap
        assert_eq!(n.len(), 2);
        assert!(n.contains(&id_b));
        assert!(n.contains(&id_c));
    }

    #[tokio::test]
    async fn test_neighbors_hub_node_dedup() {
        let graph = CsrGraph::new();
        let tx = TxId::new(1);
        let hub_id = EntityId::new(1);

        graph
            .add_entity(tx, Entity::new(hub_id, "Hub", "Type"))
            .await
            .unwrap();

        // Create 100 leaf nodes and 100 outgoing edges, including duplicate edges
        for i in 2..=101 {
            let leaf_id = EntityId::new(i);
            graph
                .add_entity(tx, Entity::new(leaf_id, format!("Leaf_{i}"), "Type"))
                .await
                .unwrap();
            graph
                .add_edge(tx, Edge::new(hub_id, leaf_id, "rel"))
                .await
                .unwrap();
            // Duplicate edge to same target
            graph
                .add_edge(tx, Edge::new(hub_id, leaf_id, "rel_dup"))
                .await
                .unwrap();
        }
        graph.commit(tx).await.unwrap();

        let neighbors = graph.neighbors(hub_id).await.unwrap();
        assert!(
            neighbors.len() <= 100,
            "Hub node neighbors count must be <= 100 unique entities, got {}",
            neighbors.len()
        );
        assert_eq!(
            neighbors.len(),
            100,
            "Hub node with 100 leaves must return exactly 100 unique neighbors"
        );

        let unique_neighbors: std::collections::HashSet<_> = neighbors.iter().copied().collect();
        assert_eq!(
            unique_neighbors.len(),
            neighbors.len(),
            "Neighbors returned must not contain duplicate EntityIds"
        );
    }

    #[tokio::test]
    async fn test_remove_edge_uncompacted_and_compacted() {
        let graph = CsrGraph::new();
        let tx1 = TxId::new(1);
        let id_a = EntityId::new(1);
        let id_b = EntityId::new(2);

        graph
            .add_entity(tx1, Entity::new(id_a, "A", "T"))
            .await
            .unwrap(); // unwrap
        graph
            .add_entity(tx1, Entity::new(id_b, "B", "T"))
            .await
            .unwrap(); // unwrap
        graph
            .add_edge(tx1, Edge::new(id_a, id_b, "rel"))
            .await
            .unwrap(); // unwrap
        graph.commit(tx1).await.unwrap(); // unwrap

        assert!(graph.neighbors(id_a).await.unwrap().contains(&id_b)); // unwrap

        // Remove edge in tx2
        let tx2 = TxId::new(2);
        graph.remove_edge(tx2, id_a, id_b).await.unwrap(); // unwrap
        graph.commit(tx2).await.unwrap(); // unwrap

        assert!(
            !graph.neighbors(id_a).await.unwrap().contains(&id_b), // unwrap
            "Edge A->B should not exist after remove_edge commit"
        );

        // Compact graph and verify edge remains removed
        graph.compact();
        assert!(
            !graph.neighbors(id_a).await.unwrap().contains(&id_b), // unwrap
            "Edge A->B should remain removed after compact"
        );
    }

    #[tokio::test]
    async fn test_add_bidirectional() {
        let graph = CsrGraph::new();
        let tx = TxId::new(1);
        let id_a = EntityId::new(1);
        let id_b = EntityId::new(2);

        graph
            .add_entity(tx, Entity::new(id_a, "A", "T"))
            .await
            .unwrap(); // unwrap
        graph
            .add_entity(tx, Entity::new(id_b, "B", "T"))
            .await
            .unwrap(); // unwrap
        graph
            .add_bidirectional(tx, id_a, id_b, "knows")
            .await
            .unwrap(); // unwrap
        graph.commit(tx).await.unwrap(); // unwrap

        let n_a = graph.neighbors(id_a).await.unwrap(); // unwrap
        let n_b = graph.neighbors(id_b).await.unwrap(); // unwrap

        assert!(n_a.contains(&id_b), "neighbors(A) must contain B");
        assert!(n_b.contains(&id_a), "neighbors(B) must contain A");
    }

    #[tokio::test]
    async fn test_pagerank_linear_chain() {
        let graph = CsrGraph::new();
        let tx = TxId::new(1);

        for i in 1..=3 {
            graph
                .add_entity(tx, Entity::new(EntityId::new(i), format!("N{i}"), "Type"))
                .await
                .unwrap(); // unwrap
        }

        // 1 -> 2 -> 3
        graph
            .add_edge(tx, Edge::new(EntityId::new(1), EntityId::new(2), "edge"))
            .await
            .unwrap(); // unwrap
        graph
            .add_edge(tx, Edge::new(EntityId::new(2), EntityId::new(3), "edge"))
            .await
            .unwrap(); // unwrap
        graph.commit(tx).await.unwrap(); // unwrap

        let ranks = graph.pagerank(0.85, 100, 1e-6).await;
        assert_eq!(ranks.len(), 3);

        let r1 = ranks[&EntityId::new(1)];
        let r2 = ranks[&EntityId::new(2)];
        let r3 = ranks[&EntityId::new(3)];

        // Downstream nodes in linear chain receive PageRank flow
        assert!(
            r2 > r1,
            "Node 2 rank ({r2}) should be higher than Node 1 ({r1})"
        );
        assert!(
            r3 > r2,
            "Node 3 rank ({r3}) should be higher than Node 2 ({r2})"
        );
    }

    #[tokio::test]
    async fn traverse_handles_cycles_without_infinite_loop() {
        let graph = CsrGraph::new();
        let tx = TxId::new(1);
        let id_a = EntityId::from_key("node_a").expect("test: non-empty key must succeed"); // expect
        let id_b = EntityId::from_key("node_b").expect("test: non-empty key must succeed"); // expect

        graph
            .add_entity(tx, Entity::new(id_a, "Node A", "Type"))
            .await
            .unwrap(); // unwrap
        graph
            .add_entity(tx, Entity::new(id_b, "Node B", "Type"))
            .await
            .unwrap(); // unwrap

        // A -> B and B -> A cycle
        graph
            .add_edge(tx, Edge::new(id_a, id_b, "relates"))
            .await
            .unwrap(); // unwrap
        graph
            .add_edge(tx, Edge::new(id_b, id_a, "relates"))
            .await
            .unwrap(); // unwrap
        graph.commit(tx).await.unwrap(); // unwrap

        // traverse with max_hops=10 (capped by MAX_TRAVERSAL_HOPS internal logic)
        let results = graph.traverse(id_a, 10).await.unwrap(); // unwrap

        // Must return finite results without duplicates
        let ids: Vec<_> = results.iter().map(|(id, _)| *id).collect();
        let unique_ids: std::collections::HashSet<_> = ids.iter().copied().collect();
        assert_eq!(
            ids.len(),
            unique_ids.len(),
            "Results must not contain duplicates"
        );
        assert!(ids.contains(&id_b), "Must contain node B");
        assert!(!ids.contains(&id_a), "Must not contain start node A");
    }

    #[tokio::test]
    async fn multi_traverse_keeps_highest_score_per_entity() {
        let graph = CsrGraph::new();
        let tx = TxId::new(1);
        let id_a = EntityId::from_key("node_a").expect("test: non-empty key must succeed"); // expect
        let id_b = EntityId::from_key("node_b").expect("test: non-empty key must succeed"); // expect
        let id_c = EntityId::from_key("node_c").expect("test: non-empty key must succeed"); // expect

        graph
            .add_entity(tx, Entity::new(id_a, "Node A", "Type"))
            .await
            .unwrap(); // unwrap
        graph
            .add_entity(tx, Entity::new(id_b, "Node B", "Type"))
            .await
            .unwrap(); // unwrap
        graph
            .add_entity(tx, Entity::new(id_c, "Node C", "Type"))
            .await
            .unwrap(); // unwrap

        // A -> C (weight 1.0) => hop score = 1.0 * 0.7 = 0.7
        graph
            .add_edge(tx, Edge::new(id_a, id_c, "relates").with_weight(1.0))
            .await
            .unwrap(); // unwrap
                       // B -> C (weight 0.7) => hop score = 1.0 * 0.7 * 0.7 = 0.49
        graph
            .add_edge(tx, Edge::new(id_b, id_c, "relates").with_weight(0.7))
            .await
            .unwrap(); // unwrap
        graph.commit(tx).await.unwrap(); // unwrap

        let results = graph.multi_traverse(&[id_a, id_b], 1).await.unwrap(); // unwrap
        let c_score = results.iter().find(|(id, _)| *id == id_c).map(|(_, s)| *s);

        assert!(c_score.is_some(), "Node C must be in traversal results");
        let score = c_score.unwrap(); // unwrap
        assert!(
            (score - 0.7).abs() < 1e-4,
            "Multi-traverse must keep max score 0.7, got {score}"
        );
    }

    #[tokio::test]
    async fn test_concurrent_add_edge() {
        let graph = Arc::new(CsrGraph::new());
        let tx0 = TxId::new(1);

        // Pre-create center entity
        let center_id = EntityId::new(999);
        graph
            .add_entity(tx0, Entity::new(center_id, "Center", "Type"))
            .await
            .unwrap(); // unwrap

        for i in 1..=20 {
            graph
                .add_entity(
                    tx0,
                    Entity::new(EntityId::new(i), format!("Node{i}"), "Type"),
                )
                .await
                .unwrap(); // unwrap
        }
        graph.commit(tx0).await.unwrap(); // unwrap

        let mut handles = Vec::new();

        for i in 1..=20 {
            let g = graph.clone();
            let handle = tokio::spawn(async move {
                let tx = TxId::new(100 + i);
                let target = EntityId::new(i);
                GraphIndex::add_edge(g.as_ref(), tx, Edge::new(center_id, target, "connect"))
                    .await
                    .unwrap(); // unwrap
                g.commit(tx).await.unwrap(); // unwrap
            });
            handles.push(handle);
        }

        for h in handles {
            h.await.unwrap(); // unwrap
        }

        let neighbors = graph.neighbors(center_id).await.unwrap(); // unwrap
        assert_eq!(
            neighbors.len(),
            20,
            "All 20 concurrent edges must be committed without lost updates"
        );
    }

    #[tokio::test]
    async fn test_staged_edges_invisible_to_concurrent_readers() {
        let graph = CsrGraph::new();
        let tx_a = TxId::new(10);
        let id_1 = EntityId::new(1);
        let id_2 = EntityId::new(2);

        // Stage entity 1 & 2, and edge 1->2 in Tx A
        graph
            .add_entity(tx_a, Entity::new(id_1, "Node 1", "Type"))
            .await
            .unwrap(); // unwrap
        graph
            .add_entity(tx_a, Entity::new(id_2, "Node 2", "Type"))
            .await
            .unwrap(); // unwrap
        graph
            .add_edge(tx_a, Edge::new(id_1, id_2, "staged_edge"))
            .await
            .unwrap(); // unwrap

        // Concurrent read (no TxId context): neighbors(1) must NOT include node 2
        let n_before = graph.neighbors(id_1).await.unwrap(); // unwrap
        assert!(
            !n_before.contains(&id_2),
            "Uncommitted staged edge must not be visible to readers"
        );

        // Tx A commits
        graph.commit(tx_a).await.unwrap(); // unwrap

        // Second read: neighbors(1) MUST include node 2
        let n_after = graph.neighbors(id_1).await.unwrap(); // unwrap
        assert!(
            n_after.contains(&id_2),
            "Committed edge must be visible to readers"
        );
    }

    #[tokio::test]
    async fn graph_edges_survive_storage_roundtrip() {
        use memfuse_store::{LsmConfig, LsmStorage};

        let dir = tempfile::tempdir().unwrap(); // unwrap allowed
        let storage = Arc::new(
            LsmStorage::new(LsmConfig {
                path: dir.path().to_path_buf(),
                ..Default::default()
            })
            .await
            .unwrap(), // unwrap allowed
        );
        let graph = CsrGraph::with_config_and_storage(CsrGraphConfig::default(), storage.clone());
        let tx = TxId::new(1);
        let id_a = EntityId::from_key("alice").unwrap(); // unwrap allowed
        let id_b = EntityId::from_key("bob").unwrap(); // unwrap allowed
        graph
            .add_entity(tx, Entity::new(id_a, "alice", "Person"))
            .await
            .unwrap(); // unwrap allowed
        graph
            .add_entity(tx, Entity::new(id_b, "bob", "Person"))
            .await
            .unwrap(); // unwrap allowed
        graph
            .add_edge(tx, Edge::new(id_a, id_b, "knows"))
            .await
            .unwrap(); // unwrap allowed
        graph.commit(tx).await.unwrap(); // unwrap allowed
        storage.commit(tx).await.unwrap(); // unwrap allowed
        storage.flush().await.unwrap(); // unwrap allowed
        drop(graph);

        let graph2 = CsrGraph::load_from_storage(storage.as_ref()).await.unwrap(); // unwrap allowed
        let neighbors = graph2.traverse(id_a, 1).await.unwrap(); // unwrap allowed
        assert!(
            !neighbors.is_empty(),
            "Kante muss storage-roundtrip überleben"
        );
        assert!(neighbors.iter().any(|(id, _)| *id == id_b));
    }

    #[tokio::test]
    async fn test_csr_graph_traverse_at_filtering() {
        let graph = CsrGraph::new();
        let id1 = EntityId::new(1);
        let id2 = EntityId::new(2);
        let id3 = EntityId::new(3);

        // Tx 1: Add nodes 1, 2, 3 and edge 1->2
        let tx1 = TxId::new(1);
        graph
            .add_entity(tx1, Entity::new(id1, "N1", "T"))
            .await
            .unwrap(); // unwrap
        graph
            .add_entity(tx1, Entity::new(id2, "N2", "T"))
            .await
            .unwrap(); // unwrap
        graph
            .add_entity(tx1, Entity::new(id3, "N3", "T"))
            .await
            .unwrap(); // unwrap
        graph
            .add_edge(tx1, Edge::new(id1, id2, "rel1"))
            .await
            .unwrap(); // unwrap
        graph.commit(tx1).await.unwrap(); // unwrap

        // Tx 2: Add edge 2->3
        let tx2 = TxId::new(2);
        graph
            .add_edge(tx2, Edge::new(id2, id3, "rel2"))
            .await
            .unwrap(); // unwrap
        graph.commit(tx2).await.unwrap(); // unwrap

        // traverse_at seq 1: 1->2 visible, but edge 2->3 (tx2) NOT visible
        let res_seq1 = graph.traverse_at(id1, 2, 1).await.unwrap(); // unwrap
        let ids_seq1: Vec<_> = res_seq1.iter().map(|(id, _)| id.inner()).collect();
        assert!(ids_seq1.contains(&2), "seq 1 traverse must include node 2");
        assert!(
            !ids_seq1.contains(&3),
            "seq 1 traverse must NOT include node 3"
        );

        // traverse_at seq 2: both 1->2 and 2->3 visible
        let res_seq2 = graph.traverse_at(id1, 2, 2).await.unwrap(); // unwrap
        let ids_seq2: Vec<_> = res_seq2.iter().map(|(id, _)| id.inner()).collect();
        assert!(ids_seq2.contains(&2), "seq 2 traverse must include node 2");
        assert!(ids_seq2.contains(&3), "seq 2 traverse must include node 3");
    }

    #[tokio::test]
    async fn test_last_tx_id_tracking() {
        let graph = CsrGraph::new();
        assert_eq!(graph.last_tx_id().await.unwrap(), TxId(0)); // unwrap

        let tx1 = TxId::new(5);
        graph
            .add_entity(tx1, Entity::new(EntityId::new(1), "E1", "T"))
            .await
            .unwrap(); // unwrap
        graph.commit(tx1).await.unwrap(); // unwrap

        assert_eq!(
            graph.last_tx_id().await.unwrap(), // unwrap
            TxId(5),
            "last_tx_id should be updated to 5 after committing Tx 5"
        );

        let tx2 = TxId::new(12);
        graph
            .add_entity(tx2, Entity::new(EntityId::new(2), "E2", "T"))
            .await
            .unwrap(); // unwrap
        graph.commit(tx2).await.unwrap(); // unwrap

        assert_eq!(
            graph.last_tx_id().await.unwrap(), // unwrap
            TxId(12),
            "last_tx_id should be updated to 12 after committing Tx 12"
        );
    }

    #[tokio::test]
    async fn test_traverse_at_time_exact_boundary_off_by_one() {
        let graph = CsrGraph::new();
        let tx_setup = TxId::new(1);
        let id1 = EntityId::new(1);
        let id2 = EntityId::new(2);

        graph
            .add_entity(tx_setup, Entity::new(id1, "Node1", "Type"))
            .await
            .unwrap(); // unwrap
        graph
            .add_entity(tx_setup, Entity::new(id2, "Node2", "Type"))
            .await
            .unwrap(); // unwrap

        let valid_until = TxId::new(100);
        let edge =
            Edge::new(id1, id2, "valid_rel").with_validity(Some(TxId::new(10)), Some(valid_until));

        graph.add_edge(tx_setup, edge).await.unwrap(); // unwrap
        graph.commit(tx_setup).await.unwrap(); // unwrap

        // 1. Before valid_from (< 10) -> Should NOT return edge
        let res_before = graph.traverse_at_time(id1, 1, TxId::new(9)).await.unwrap(); // unwrap
        assert!(
            res_before.is_empty(),
            "Edge must not be valid before valid_from (9 < 10)"
        );

        // 2. Exactly at valid_from (10) -> MUST return edge
        let res_from = graph.traverse_at_time(id1, 1, TxId::new(10)).await.unwrap(); // unwrap
        assert_eq!(res_from.len(), 1, "Edge must be valid at valid_from (10)");

        // 3. One step before valid_to (valid_until - 1 = 99) -> MUST return edge
        let res_before_to = graph.traverse_at_time(id1, 1, TxId::new(99)).await.unwrap(); // unwrap
        assert_eq!(
            res_before_to.len(),
            1,
            "Edge must be valid at valid_to - 1 (99)"
        );

        // 4. Exactly at valid_to (valid_until = 100) -> MUST NOT return edge
        let res_at_to = graph.traverse_at_time(id1, 1, valid_until).await.unwrap(); // unwrap
        assert!(
            res_at_to.is_empty(),
            "Edge must NOT be valid at exact valid_to boundary (100)"
        );

        // 5. After valid_to (101) -> MUST NOT return edge
        let res_after_to = graph
            .traverse_at_time(id1, 1, TxId::new(101))
            .await
            .unwrap(); // unwrap
        assert!(
            res_after_to.is_empty(),
            "Edge must NOT be valid after valid_to (101)"
        );

        // 6. Test compacted CSR path boundary behavior
        graph.compact();

        let res_compact_valid = graph.traverse_at_time(id1, 1, TxId::new(99)).await.unwrap(); // unwrap
        assert_eq!(
            res_compact_valid.len(),
            1,
            "Compacted edge must be valid at 99"
        );

        let res_compact_invalid = graph.traverse_at_time(id1, 1, valid_until).await.unwrap(); // unwrap
        assert!(
            res_compact_invalid.is_empty(),
            "Compacted edge must NOT be valid at 100"
        );
    }

    #[tokio::test]
    async fn test_traverse_at_time_unbounded_validity() {
        let graph = CsrGraph::new();
        let tx = TxId::new(1);
        let id1 = EntityId::new(1);
        let id2 = EntityId::new(2);

        graph
            .add_entity(tx, Entity::new(id1, "N1", "T"))
            .await
            .unwrap(); // unwrap
        graph
            .add_entity(tx, Entity::new(id2, "N2", "T"))
            .await
            .unwrap(); // unwrap

        let edge = Edge::new(id1, id2, "always_valid");
        graph.add_edge(tx, edge).await.unwrap(); // unwrap
        graph.commit(tx).await.unwrap(); // unwrap

        let res = graph
            .traverse_at_time(id1, 1, TxId::new(500))
            .await
            .unwrap(); // unwrap
        assert_eq!(
            res.len(),
            1,
            "Unbounded edge must be valid at any point in time"
        );
    }

    proptest::proptest! {
        #[test]
        fn prop_add_edge_rollback_no_index_growth(
            edge_specs in proptest::collection::vec((1u64..1000, 1001u64..2000), 10..100)
        ) {
            let rt = tokio::runtime::Builder::new_current_thread().build().unwrap(); // unwrap
            let res: std::result::Result<(), proptest::test_runner::TestCaseError> = rt.block_on(async {
                let graph = CsrGraph::new();
                let initial_id_len = graph.inner_read().id_map.len();
                let initial_rev_len = graph.inner_read().reverse_map.len();

                for (i, (from_val, to_val)) in edge_specs.into_iter().enumerate() {
                    let tx = TxId::new(i as u64 + 1);
                    let edge = Edge::new(EntityId::new(from_val), EntityId::new(to_val), "rel");
                    let _ = graph.add_edge(tx, edge).await;
                    let _ = graph.rollback(tx).await;

                    let inner = graph.inner_read();
                    proptest::prop_assert_eq!(inner.id_map.len(), initial_id_len);
                    proptest::prop_assert_eq!(inner.reverse_map.len(), initial_rev_len);
                }
                Ok(())
            });
            res?;
        }

        #[test]
        fn prop_edge_visible_monotone(
            vf in 0u64..100,
            vt in 100u64..200,
            as_of in 0u64..200,
        ) {
            let valid_from = Some(TxId::new(vf));
            let valid_to = Some(TxId::new(vt));
            let visible = is_edge_visible(valid_from, valid_to, TxId::new(as_of));
            let expected = vf <= as_of && as_of < vt;
            proptest::prop_assert_eq!(visible, expected);
        }

        #[test]
        fn prop_traverse_at_time_never_panics(
            node_count in 1..=15usize,
            edge_specs in proptest::collection::vec((0..15usize, 0..15usize, 0u64..50u64, 50u64..100u64), 0..30),
            start_idx in 0..15usize,
            hops in 0usize..5,
            as_of in 0u64..150u64,
        ) {
            let rt = tokio::runtime::Builder::new_current_thread().build().unwrap(); // unwrap
            let res: std::result::Result<(), proptest::test_runner::TestCaseError> = rt.block_on(async {
                let graph = CsrGraph::new();
                let tx = TxId::new(1);

                for i in 0..node_count {
                    let _ = graph
                        .add_entity(tx, Entity::new(EntityId::new(i as u64 + 1), format!("N{i}"), "Node"))
                        .await;
                }

                for (src, dst, vf, vt) in edge_specs {
                    let src_id = EntityId::new((src % node_count) as u64 + 1);
                    let dst_id = EntityId::new((dst % node_count) as u64 + 1);
                    let edge = Edge::new(src_id, dst_id, "link")
                        .with_validity(Some(TxId::new(vf)), Some(TxId::new(vt)));
                    let _ = graph.add_edge(tx, edge).await;
                }
                let _ = graph.commit(tx).await;

                let start = EntityId::new((start_idx % node_count) as u64 + 1);
                let _res = graph.traverse_at_time(start, hops, TxId::new(as_of)).await;
                Ok(())
            });
            res?;
        }
    }

    #[test]
    fn prop_csr_graph_traverse_at_consistency() {
        use proptest::prelude::*;

        #[derive(Debug, Clone)]
        enum Op {
            AddEntity(u64),
            AddEdge(u64, u64),
            RemoveEdge(u64, u64),
        }

        let op_strategy = proptest::collection::vec(
            prop_oneof![
                (1u64..20).prop_map(Op::AddEntity),
                (1u64..20, 1u64..20).prop_map(|(f, t)| Op::AddEdge(f, t)),
                (1u64..20, 1u64..20).prop_map(|(f, t)| Op::RemoveEdge(f, t)),
            ],
            10..80,
        );

        proptest!(ProptestConfig::with_cases(20), |(ops in op_strategy)| {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap(); // unwrap

            rt.block_on(async {
                let graph = CsrGraph::new();
                let mut current_tx = 1u64;
                let mut tx_checkpoints = Vec::new();

                for op in ops {
                    let tx = TxId::new(current_tx);
                    match op {
                        Op::AddEntity(id) => {
                            let _ = graph.add_entity(tx, Entity::new(EntityId::new(id), "N", "T")).await;
                        }
                        Op::AddEdge(from, to) => {
                            if from != to {
                                let _ = graph.add_edge(tx, Edge::new(EntityId::new(from), EntityId::new(to), "E")).await;
                            }
                        }
                        Op::RemoveEdge(from, to) => {
                            let _ = graph.remove_edge(tx, EntityId::new(from), EntityId::new(to)).await;
                        }
                    }
                    if graph.commit(tx).await.is_ok() {
                        tx_checkpoints.push(current_tx);
                        current_tx += 1;
                    }
                }

                // Verify traverse_at at each target sequence against reference replay model
                for &target_seq in &tx_checkpoints {
                    // Reconstruct expected edges and entities up to target_seq
                    let (entities, active_edges) = {
                        let mut entities = std::collections::HashSet::new();
                        let mut active_edges = std::collections::HashSet::new();

                        let inner = graph.inner_read();
                        let num_nodes = inner.reverse_map.len();
                        for i in 0..num_nodes {
                            if let Some(&id) = inner.reverse_map.get(i) {
                                if inner.entity_at(i).is_some() {
                                    entities.insert(id.inner());
                                }
                            }

                            let old_start = if i < inner.offsets.len() - 1 { inner.offsets[i] } else { 0 };
                            let old_end = if i < inner.offsets.len() - 1 { inner.offsets[i + 1] } else { 0 };
                            for j in old_start..old_end {
                                let target_idx = inner.targets[j];
                                let vf = inner.tx_valid_from_at(j).unwrap_or(TxId::new(0)).inner();
                                let vt = inner.tx_valid_to_at(j).map(|t| t.inner());

                                if vf <= target_seq && vt.is_none_or(|t| target_seq < t) {
                                    if let (Some(&f), Some(&t)) = (inner.reverse_map.get(i), inner.reverse_map.get(target_idx)) {
                                        if !inner.tombstoned_edges.contains(&(i, target_idx)) {
                                            active_edges.insert((f.inner(), t.inner()));
                                        }
                                    }
                                }
                            }

                            if let Some(pending) = inner.pending_edges.get(&i) {
                                for edge in pending {
                                    let vf = edge.tx_valid_from.unwrap_or(TxId::new(0)).inner();
                                    let vt = edge.tx_valid_to.map(|t| t.inner());
                                    if vf <= target_seq && vt.is_none_or(|t| target_seq < t) {
                                        if let (Some(&f), Some(&t)) = (inner.reverse_map.get(i), inner.reverse_map.get(edge.target)) {
                                            if !inner.tombstoned_edges.contains(&(i, edge.target)) {
                                                active_edges.insert((f.inner(), t.inner()));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        (entities, active_edges)
                    };

                    // Check traverse_at for each active node
                    for &start in &entities {
                        let res = graph.traverse_at(EntityId::new(start), 1, target_seq).await.unwrap(); // unwrap
                        let actual_neighbors: std::collections::HashSet<_> = res.into_iter().map(|(id, _)| id.inner()).collect();

                        let expected_neighbors: std::collections::HashSet<_> = active_edges
                            .iter()
                            .filter(|(f, t)| *f == start && entities.contains(t))
                            .map(|(_, t)| *t)
                            .collect();

                        prop_assert_eq!(actual_neighbors, expected_neighbors, "Neighbors at seq {} from node {} must match reference model", target_seq, start);
                    }
                }
                Ok(())
            }).unwrap(); // unwrap
        });
    }

    #[tokio::test]
    #[allow(non_snake_case)]
    async fn compact_async_CASE_dirty_and_clean_states() {
        let graph = Arc::new(CsrGraph::new());
        let tx = TxId::new(1);

        // Stage and commit an edge to mark graph dirty
        graph
            .add_entity(tx, Entity::new(EntityId::new(1), "A", "T"))
            .await
            .unwrap(); // unwrap allowed
        graph
            .add_entity(tx, Entity::new(EntityId::new(2), "B", "T"))
            .await
            .unwrap(); // unwrap allowed
        GraphIndex::add_edge(
            graph.as_ref(),
            tx,
            Edge::new(EntityId::new(1), EntityId::new(2), "rel"),
        )
        .await
        .unwrap(); // unwrap allowed
        graph.commit(tx).await.unwrap(); // unwrap allowed

        assert!(graph.inner_read().is_dirty);

        // compact_async on dirty graph
        graph.compact_async().await.unwrap(); // unwrap allowed

        assert!(!graph.inner_read().is_dirty);
        assert_eq!(graph.inner_read().targets.len(), 1);

        // compact_async no-op on clean graph
        graph.compact_async().await.unwrap(); // unwrap allowed
        assert!(!graph.inner_read().is_dirty);
    }

    #[tokio::test]
    #[allow(non_snake_case)]
    async fn set_storage_and_with_config_and_storage_CASE_initialization() {
        use memfuse_store::{LsmConfig, LsmStorage};

        let dir = tempfile::tempdir().unwrap(); // unwrap allowed
        let storage: Arc<dyn StorageEngine> = Arc::new(
            LsmStorage::new(LsmConfig {
                path: dir.path().to_path_buf(),
                ..Default::default()
            })
            .await
            .unwrap(), // unwrap allowed
        );

        let config = CsrGraphConfig {
            rebuild_threshold: 50,
            ..Default::default()
        };
        let mut graph = CsrGraph::with_config_and_storage(config, storage.clone());
        assert!(graph.storage.is_some());

        // Replace storage via set_storage
        let dir2 = tempfile::tempdir().unwrap(); // unwrap allowed
        let storage2: Arc<dyn StorageEngine> = Arc::new(
            LsmStorage::new(LsmConfig {
                path: dir2.path().to_path_buf(),
                ..Default::default()
            })
            .await
            .unwrap(), // unwrap allowed
        );

        graph.set_storage(storage2);
        assert!(graph.storage.is_some());
    }

    #[tokio::test]
    #[allow(non_snake_case)]
    async fn insert_entity_direct_and_edge_direct_CASE_and_boundaries() {
        let graph = Arc::new(CsrGraph::new());

        let id1 = EntityId::new(10);
        let id2 = EntityId::new(20);

        graph
            .insert_entity_direct(Entity::new(id1, "Direct1", "Type"))
            .unwrap(); // unwrap allowed
        graph
            .insert_entity_direct(Entity::new(id2, "Direct2", "Type"))
            .unwrap(); // unwrap allowed

        assert_eq!(graph.entity_count(), 2);
        assert!(graph.entity_exists(id1));
        assert!(graph.entity_exists(id2));
        assert!(!graph.entity_exists(EntityId::new(999)));

        graph
            .insert_edge_direct_with_validity(
                id1,
                id2,
                1.5,
                Some(TxId::new(5)),
                Some(TxId::new(50)),
            )
            .await
            .unwrap(); // unwrap allowed

        assert_eq!(graph.edge_count(), 1);
    }

    #[tokio::test]
    #[allow(non_snake_case)]
    async fn persist_entity_edge_delete_persistence_CASE_direct_calls() {
        use memfuse_store::{LsmConfig, LsmStorage};

        let dir = tempfile::tempdir().unwrap(); // unwrap allowed
        let storage = LsmStorage::new(LsmConfig {
            path: dir.path().to_path_buf(),
            ..Default::default()
        })
        .await
        .unwrap(); // unwrap allowed

        let graph = CsrGraph::new();
        let tx = TxId::new(10);
        let id1 = EntityId::new(1);
        let id2 = EntityId::new(2);
        let entity = Entity::new(id1, "P1", "Person");
        let payload = PersistedEdgePayload {
            weight: 0.9,
            tx_valid_from: Some(TxId::new(1)),
            tx_valid_to: None,
            business_valid_from: None,
            business_valid_to: None,
            source_doc_id: None,
        };

        // Persist entity & edge directly
        graph.persist_entity(&storage, tx, &entity).await.unwrap(); // unwrap allowed
        graph
            .persist_edge(&storage, tx, &id1, &id2, &payload)
            .await
            .unwrap(); // unwrap allowed
        storage.commit(tx).await.unwrap(); // unwrap allowed

        // Delete edge persistence
        let tx2 = TxId::new(11);
        graph
            .delete_edge_persistence(&storage, tx2, &id1, &id2)
            .await
            .unwrap(); // unwrap allowed
        storage.commit(tx2).await.unwrap(); // unwrap allowed
    }

    #[tokio::test]
    #[allow(non_snake_case)]
    async fn load_from_storage_CASE_legacy_f32_weight_and_invalid_key() {
        use memfuse_store::{LsmConfig, LsmStorage};

        let dir = tempfile::tempdir().unwrap(); // unwrap allowed
        let storage = LsmStorage::new(LsmConfig {
            path: dir.path().to_path_buf(),
            ..Default::default()
        })
        .await
        .unwrap(); // unwrap allowed

        let tx = TxId::new(1);

        // Put legacy f32 edge payload
        let legacy_key = b"__graph:edge:1:2";
        let legacy_val = bincode::serialize(&0.75f32).unwrap(); // unwrap allowed
        storage.put(tx, legacy_key, &legacy_val).await.unwrap(); // unwrap allowed

        // Put invalid key (missing colon delimiter)
        let invalid_key = b"__graph:edge:invalidkeywithoutcolon";
        let payload = PersistedEdgePayload {
            weight: 1.0,
            tx_valid_from: None,
            tx_valid_to: None,
            business_valid_from: None,
            business_valid_to: None,
            source_doc_id: None,
        };
        let payload_val = bincode::serialize(&payload).unwrap(); // unwrap allowed
        storage.put(tx, invalid_key, &payload_val).await.unwrap(); // unwrap allowed

        storage.commit(tx).await.unwrap(); // unwrap allowed

        let loaded_graph = CsrGraph::load_from_storage(&storage).await.unwrap(); // unwrap allowed
        assert_eq!(loaded_graph.edge_count(), 1);
    }

    #[derive(Clone)]
    struct LogCaptureLayer(std::sync::Arc<std::sync::Mutex<Vec<String>>>);

    impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for LogCaptureLayer {
        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            _ctx: tracing_subscriber::layer::Context<'_, S>,
        ) {
            let mut visitor = StringVisitor(String::new());
            event.record(&mut visitor);
            self.0.lock().unwrap().push(visitor.0); // unwrap
        }
    }

    struct StringVisitor(String);
    impl tracing::field::Visit for StringVisitor {
        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            use std::fmt::Write;
            write!(self.0, "{}={:?} ", field.name(), value).ok();
        }
    }

    #[tokio::test]
    async fn test_traverse_max_hops_exceeded_emits_warning() {
        use tracing_subscriber::layer::SubscriberExt;

        let logs = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let capture_layer = LogCaptureLayer(logs.clone());
        let subscriber = tracing_subscriber::registry().with(capture_layer);
        let _guard = tracing::subscriber::set_default(subscriber);

        let graph = setup_test_graph().await;

        let _ = graph.traverse(EntityId::new(1), 5).await.unwrap(); // unwrap

        let captured = logs.lock().unwrap(); // unwrap
        let warning_found = captured.iter().any(|msg| {
            msg.contains("exceeds internal cap MAX_TRAVERSAL_HOPS")
                && msg.contains("requested_max_hops=5")
        });

        assert!(
            warning_found,
            "Expected warning log when max_hops > MAX_TRAVERSAL_HOPS, got: {:?}",
            *captured
        );
    }

    #[tokio::test]
    #[allow(non_snake_case)]
    async fn traverse_at_time_CASE_saturating_max_hops() {
        let graph = setup_test_graph().await;

        // Traverse with max_hops 100 (should saturate safely to MAX_TRAVERSAL_HOPS=3 without panic/OOM)
        let results = graph
            .traverse_at_time(EntityId::new(1), 100, TxId::new(100))
            .await
            .unwrap(); // unwrap allowed

        assert!(!results.is_empty());
        assert!(results.len() <= 4);
    }

    #[tokio::test]
    #[allow(non_snake_case)]
    async fn traverse_CASE_exceeds_max_hops_returns_invalid_input() {
        let graph = setup_test_graph().await;

        let err = graph.traverse(EntityId::new(1), 101).await.unwrap_err();
        assert!(matches!(err, MemFuseError::InvalidInput(_)));

        let err_time = graph
            .traverse_at_time(EntityId::new(1), 101, TxId::new(100))
            .await
            .unwrap_err();
        assert!(matches!(err_time, MemFuseError::InvalidInput(_)));
    }

    #[tokio::test]
    #[allow(non_snake_case)]
    async fn pagerank_CASE_empty_graph_and_isolated_node() {
        let empty_graph = CsrGraph::new();
        let ranks_empty = empty_graph.pagerank(0.85, 100, 1e-6).await;
        assert!(ranks_empty.is_empty());

        let iso_graph = CsrGraph::new();
        iso_graph
            .insert_entity_direct(Entity::new(EntityId::new(1), "Iso", "Type"))
            .unwrap(); // unwrap allowed

        let ranks_iso = iso_graph.pagerank(0.85, 100, 1e-6).await;
        assert_eq!(ranks_iso.len(), 1);
        let rank = ranks_iso[&EntityId::new(1)];
        assert!((rank - 1.0).abs() < 1e-4);
    }

    #[test]
    #[allow(non_snake_case)]
    fn serialization_roundtrip_CASE_persisted_edge_payload() {
        let payload = PersistedEdgePayload {
            weight: 0.825,
            tx_valid_from: Some(TxId::new(10)),
            tx_valid_to: Some(TxId::new(20)),
            business_valid_from: Some(1000),
            business_valid_to: Some(2000),
            source_doc_id: None,
        };

        let serialized = bincode::serialize(&payload).unwrap(); // unwrap allowed
        let deserialized: PersistedEdgePayload = bincode::deserialize(&serialized).unwrap(); // unwrap allowed

        assert!((payload.weight - deserialized.weight).abs() < f32::EPSILON);
        assert_eq!(payload.tx_valid_from, deserialized.tx_valid_from);
        assert_eq!(payload.tx_valid_to, deserialized.tx_valid_to);
        assert_eq!(
            payload.business_valid_from,
            deserialized.business_valid_from
        );
        assert_eq!(payload.business_valid_to, deserialized.business_valid_to);
    }

    proptest::proptest! {
        #[test]
        fn prop_csr_offset_array_structural_consistency(
            node_count in 1..=30usize,
            edge_pairs in proptest::collection::vec((0..30usize, 0..30usize, 0.1f32..2.0f32), 1..100)
        ) {
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
            let res: std::result::Result<(), proptest::test_runner::TestCaseError> = rt.block_on(async {
                let graph = Arc::new(CsrGraph::new());
                for i in 0..node_count {
                    graph.insert_entity_direct(Entity::new(EntityId::new(i as u64 + 1), format!("N{i}"), "Type")).unwrap(); // unwrap
                }

                for (src, dst, w) in edge_pairs {
                    let src_id = EntityId::new((src % node_count) as u64 + 1);
                    let dst_id = EntityId::new((dst % node_count) as u64 + 1);
                    graph.insert_edge_direct(src_id, dst_id, w).await.unwrap(); // unwrap
                }

                graph.compact();

                let inner = graph.inner_read();

                // Invariant 1: offsets length must equal reverse_map length + 1 after compaction
                proptest::prop_assert_eq!(inner.offsets.len(), inner.reverse_map.len() + 1);

                // Invariant 2: offsets must be monotonically non-decreasing
                for window in inner.offsets.windows(2) {
                    proptest::prop_assert!(window[0] <= window[1]);
                }

                // Invariant 3: final offset must match targets length
                proptest::prop_assert_eq!(*inner.offsets.last().unwrap(), inner.targets.len()); // unwrap

                // Invariant 4: parallel arrays (targets, weights, tx_valid_froms, tx_valid_tos, business_valid_froms, business_valid_tos, source_doc_ids) must have equal lengths
                proptest::prop_assert_eq!(inner.targets.len(), inner.weights.len());
                proptest::prop_assert_eq!(inner.targets.len(), inner.tx_valid_froms.len());
                proptest::prop_assert_eq!(inner.targets.len(), inner.tx_valid_tos.len());
                proptest::prop_assert_eq!(inner.targets.len(), inner.business_valid_froms.len());
                proptest::prop_assert_eq!(inner.targets.len(), inner.business_valid_tos.len());
                proptest::prop_assert_eq!(inner.targets.len(), inner.source_doc_ids.len());
                Ok(())
            });
            res?;
        }
    }

    #[tokio::test]
    async fn test_bitemporal_independent_axis_evaluation() {
        let graph = CsrGraph::new();
        let tx1 = TxId::new(1);
        let id1 = EntityId::new(1);
        let id2 = EntityId::new(2);

        graph
            .add_entity(tx1, Entity::new(id1, "ContractNode1", "Company"))
            .await
            .unwrap(); // unwrap
        graph
            .add_entity(tx1, Entity::new(id2, "ContractNode2", "Vendor"))
            .await
            .unwrap(); // unwrap

        let bitemporal_edge = Edge::new(id1, id2, "contract_valid")
            .with_tx_validity(Some(TxId::new(10)), Some(TxId::new(100)))
            .with_business_validity(Some(1000), Some(2000));

        graph.add_edge(tx1, bitemporal_edge).await.unwrap(); // unwrap
        graph.commit(tx1).await.unwrap(); // unwrap

        async fn verify_bitemporal_assertions(
            g: &CsrGraph,
            id1: EntityId,
            id2: EntityId,
            label: &str,
        ) {
            // Case 1: System valid (tx=50), Business invalid (500 < 1000) -> NOT visible
            let res1 = g
                .traverse_at_bitemporal(id1, 1, TxId::new(50), Some(500))
                .await
                .unwrap(); // unwrap
            assert!(
                res1.is_empty(),
                "[{label}] Edge must NOT be visible before business validity start (500 < 1000)"
            );

            // Case 2: System valid (tx=50), Business valid (1000 <= 1500 < 2000) -> VISIBLE
            let res2 = g
                .traverse_at_bitemporal(id1, 1, TxId::new(50), Some(1500))
                .await
                .unwrap(); // unwrap
            assert_eq!(
                res2.len(),
                1,
                "[{label}] Edge MUST be visible when both system and business axes match"
            );
            assert_eq!(res2[0].0, id2);

            // Case 3: System valid (tx=50), Business expired (2500 >= 2000) -> NOT visible
            let res3 = g
                .traverse_at_bitemporal(id1, 1, TxId::new(50), Some(2500))
                .await
                .unwrap(); // unwrap
            assert!(
                res3.is_empty(),
                "[{label}] Edge must NOT be visible after business validity end (2500 >= 2000)"
            );

            // Case 4: Business valid (1500), System before valid (5 < 10) -> NOT visible
            let res4 = g
                .traverse_at_bitemporal(id1, 1, TxId::new(5), Some(1500))
                .await
                .unwrap(); // unwrap
            assert!(
                res4.is_empty(),
                "[{label}] Edge must NOT be visible before system validity start (5 < 10)"
            );

            // Case 5: Business valid (1500), System expired (150 >= 100) -> NOT visible
            let res5 = g
                .traverse_at_bitemporal(id1, 1, TxId::new(150), Some(1500))
                .await
                .unwrap(); // unwrap
            assert!(
                res5.is_empty(),
                "[{label}] Edge must NOT be visible after system validity end (150 >= 100)"
            );

            // Case 6: System valid (tx=50), Business filter omitted (None) -> VISIBLE
            let res6 = g
                .traverse_at_bitemporal(id1, 1, TxId::new(50), None)
                .await
                .unwrap(); // unwrap
            assert_eq!(
                res6.len(),
                1,
                "[{label}] Edge MUST be visible when business filter is None"
            );
        }

        // 1. Verify uncompacted delta buffer path
        verify_bitemporal_assertions(&graph, id1, id2, "delta_buffer").await;

        // 2. Compact graph and verify CSR array path
        graph.compact();
        verify_bitemporal_assertions(&graph, id1, id2, "compacted_csr").await;
    }

    #[tokio::test]
    async fn test_bitemporal_regression_pure_tx_time_unchanged() {
        let graph = CsrGraph::new();
        let tx = TxId::new(1);
        let id1 = EntityId::new(1);
        let id2 = EntityId::new(2);

        graph
            .add_entity(tx, Entity::new(id1, "N1", "T"))
            .await
            .unwrap(); // unwrap
        graph
            .add_entity(tx, Entity::new(id2, "N2", "T"))
            .await
            .unwrap(); // unwrap

        // Pure transaction-time edge (no business time set)
        let pure_tx_edge = Edge::new(id1, id2, "pure_tx_rel")
            .with_tx_validity(Some(TxId::new(10)), Some(TxId::new(100)));

        graph.add_edge(tx, pure_tx_edge).await.unwrap(); // unwrap
        graph.commit(tx).await.unwrap(); // unwrap

        // Before tx_valid_from (9 < 10) -> NOT visible regardless of business_as_of
        assert!(graph
            .traverse_at_bitemporal(id1, 1, TxId::new(9), Some(999999))
            .await
            .unwrap()
            .is_empty());

        // At tx_valid_from (10) -> VISIBLE regardless of business_as_of
        let res_tx = graph
            .traverse_at_bitemporal(id1, 1, TxId::new(10), Some(999999))
            .await
            .unwrap();
        assert_eq!(res_tx.len(), 1);

        // Standard traverse_at_time call
        let res_time = graph.traverse_at_time(id1, 1, TxId::new(50)).await.unwrap();
        assert_eq!(res_time.len(), 1);

        // Compact and re-verify
        graph.compact();
        assert_eq!(
            graph
                .traverse_at_bitemporal(id1, 1, TxId::new(50), Some(12345))
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn test_hub_node_1m_neighbors_bfs_capped() -> memfuse_core::Result<()> {
        // Use a high rebuild_threshold to avoid repeated O(N) CSR compactions during setup
        let graph = Arc::new(CsrGraph::with_config(CsrGraphConfig {
            rebuild_threshold: 2_000_000,
            ..Default::default()
        }));
        let start = EntityId::new(1);
        let hub = EntityId::new(2);

        // Build 1,000,000 outgoing edges directly in CSR layout
        graph.insert_entity_direct(Entity::new(start, "StartNode", "Type"))?;
        graph.insert_entity_direct(Entity::new(hub, "HubNode", "Supernode"))?;
        graph.insert_edge_direct(start, hub, 1.0).await?;

        // Build 15,000 outgoing edges directly in CSR layout (exceeds MAX_VISITED_NODES = 10,000)
        let num_neighbors = 15_000usize;
        for i in 0..num_neighbors {
            let leaf_id = EntityId::new(3 + i as u64);
            graph.insert_entity_direct(Entity::new(leaf_id, "Leaf", "Type"))?;
            graph.insert_edge_direct(hub, leaf_id, 0.9).await?;
        }
        graph.compact();

        let start_time = std::time::Instant::now();
        let results = graph.traverse(start, 2).await?;
        let elapsed = start_time.elapsed();

        assert!(
            elapsed.as_millis() < 1000,
            "1M neighbor hub node BFS traversal must terminate in < 1 second, took {:?}",
            elapsed
        );
        assert!(
            results.len() <= MAX_VISITED_NODES,
            "Traversal result count must be capped by MAX_VISITED_NODES ({MAX_VISITED_NODES}), got {}",
            results.len()
        );
        assert_eq!(
            results.len(),
            MAX_VISITED_NODES - 1, // Start node excluded from result list
            "Visited cap includes hub and leaves, total returned results equals MAX_VISITED_NODES - 1"
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_consistency_enforcer_contradiction_suppression() {
        let graph = Arc::new(CsrGraph::with_consistency_enforcer(3));
        let from = EntityId::new(10);
        let to = EntityId::new(20);
        let pred_hash = [7u8; 32];
        let object_repr = b"ContradictoryValue".to_vec();

        // 1st insertion: recorded, not yet suppressed (count = 1)
        let res1 = graph
            .add_edge(
                from,
                to,
                1.0,
                None,
                None,
                None,
                None,
                None,
                Some(pred_hash),
                Some(object_repr.clone()),
            )
            .await;
        assert!(res1.is_ok(), "First insertion should succeed");

        // 2nd insertion: recorded, not yet suppressed (count = 2)
        let res2 = graph
            .add_edge(
                from,
                to,
                1.0,
                None,
                None,
                None,
                None,
                None,
                Some(pred_hash),
                Some(object_repr.clone()),
            )
            .await;
        assert!(res2.is_ok(), "Second insertion should succeed");

        // 3rd insertion: recorded, reaches suppression threshold (count = 3) -> suppressed!
        let res3 = graph
            .add_edge(
                from,
                to,
                1.0,
                None,
                None,
                None,
                None,
                None,
                Some(pred_hash),
                Some(object_repr),
            )
            .await;
        assert!(
            res3.is_err(),
            "Third insertion should fail due to consistency enforcer contradiction suppression"
        );
        let err = res3.unwrap_err();
        assert!(
            matches!(err, MemFuseError::PolicyViolation(ref msg) if msg.contains("Contradictory edge suppressed by consistency enforcer")),
            "Expected policy violation error, got: {:?}",
            err
        );
    }

    #[tokio::test]
    async fn test_default_csr_graph_no_consistency_check() {
        let graph = Arc::new(CsrGraph::new());
        let from = EntityId::new(100);
        let to = EntityId::new(200);
        let pred_hash = [9u8; 32];
        let object_repr = b"SomeValue".to_vec();

        // Standard CsrGraph::new() has consistency_enforcer = None, so insertion never blocks
        for i in 1..=5 {
            let res = graph
                .add_edge(
                    from,
                    to,
                    1.0,
                    None,
                    None,
                    None,
                    None,
                    None,
                    Some(pred_hash),
                    Some(object_repr.clone()),
                )
                .await;
            assert!(
                res.is_ok(),
                "Insertion {i} on default CsrGraph without consistency enforcer should always succeed"
            );
        }
    }

    #[tokio::test]
    async fn test_csr_is_entity_deleted_returns_false_for_live_entity() {
        use memfuse_store::{LsmConfig, LsmStorage};

        let dir = tempfile::tempdir().unwrap();
        let storage = Arc::new(
            LsmStorage::new(LsmConfig {
                path: dir.path().to_path_buf(),
                ..Default::default()
            })
            .await
            .unwrap(),
        );
        let graph = CsrGraph::with_storage(storage.clone());
        let entity_live = EntityId::new(10);
        let entity_deleted = EntityId::new(20);

        // No storage marker exists for entity_live -> returns false
        assert!(!graph.is_entity_deleted(entity_live).await);

        // Put deletion tombstone key in LSM storage for entity_deleted
        let tx = TxId::new(1);
        let tombstone_key = format!("graph:entity:deleted:{}", entity_deleted.0);
        storage
            .put(tx, tombstone_key.as_bytes(), b"deleted")
            .await
            .unwrap();
        storage.commit(tx).await.unwrap();

        // Marker exists -> returns true
        assert!(graph.is_entity_deleted(entity_deleted).await);
        // Live entity still returns false
        assert!(!graph.is_entity_deleted(entity_live).await);
    }

    #[test]
    #[cfg(feature = "edge-reinforcement-learning")]
    fn test_edge_store_invalidated_after_compact() {
        let mut inner = GraphInner::new();
        let entity_a = EntityId::new(1);
        let entity_b = EntityId::new(2);

        let idx_a = inner.get_or_create_index(entity_a);
        let idx_b = inner.get_or_create_index(entity_b);

        // (1) Anlegen einer Kante A -> B in pending_edges
        inner
            .pending_edges
            .entry(idx_a)
            .or_default()
            .push(EdgePayload {
                target: idx_b,
                weight: 1.0,
                tx_valid_from: None,
                tx_valid_to: None,
                business_valid_from: None,
                business_valid_to: None,
                source_doc_id: None,
            });
        inner.pending_edge_count += 1;

        // (2) `outgoing_edges_mut(A)` aufrufen um den edge_store-Cache zu befüllen
        let cached = inner.outgoing_edges_mut(entity_a);
        assert_eq!(cached.len(), 1);
        assert_eq!(cached[0].target, entity_b);

        // (3) Kante über Tombstone-Mechanismus entfernen
        inner.tombstoned_edges.insert((idx_a, idx_b));

        // (4) `compact()` aufrufen
        inner.compact();

        // (5) Beweisen, dass `outgoing_edges_mut(A)` danach die Kante NICHT mehr zurückgibt
        let cached_after_compact = inner.outgoing_edges_mut(entity_a);
        assert_eq!(
            cached_after_compact.len(),
            0,
            "edge_store must be cleared by compact() so deleted/tombstoned edges are no longer returned"
        );
    }

    #[tokio::test]
    async fn test_rcu_snapshot_isolation_no_torn_reads() {
        let graph = Arc::new(CsrGraph::new());
        let tx = TxId::new(1);

        for i in 1..=500 {
            graph
                .add_entity(tx, Entity::new(EntityId::new(i), format!("N{i}"), "Type"))
                .await
                .unwrap();
        }
        for i in 1..500 {
            GraphIndex::add_edge(
                graph.as_ref(),
                tx,
                Edge::new(EntityId::new(i), EntityId::new(i + 1), "rel").with_weight(1.0),
            )
            .await
            .unwrap();
        }
        graph.commit(tx).await.unwrap();

        // Reader acquires point-in-time RCU snapshot BEFORE compaction
        let snapshot_v1 = graph.inner_read();
        assert_eq!(
            snapshot_v1
                .entities
                .iter()
                .filter(|e| e.id != EntityId::new(0))
                .count(),
            500
        );

        // Mutate graph with additional transaction
        let tx2 = TxId::new(2);
        for i in 501..=1000 {
            graph
                .add_entity(tx2, Entity::new(EntityId::new(i), format!("N{i}"), "Type"))
                .await
                .unwrap();
        }
        for i in 500..1000 {
            GraphIndex::add_edge(
                graph.as_ref(),
                tx2,
                Edge::new(EntityId::new(i), EntityId::new(i + 1), "rel").with_weight(1.0),
            )
            .await
            .unwrap();
        }
        graph.commit(tx2).await.unwrap();
        graph.compact(); // Trigger full CSR compaction

        // Snapshot held by v1 MUST remain isolated on v1 state without torn reads
        assert_eq!(
            snapshot_v1
                .entities
                .iter()
                .filter(|e| e.id != EntityId::new(0))
                .count(),
            500
        );

        // New reader loads published post-compaction v2 snapshot
        let snapshot_v2 = graph.inner_read();
        assert_eq!(
            snapshot_v2
                .entities
                .iter()
                .filter(|e| e.id != EntityId::new(0))
                .count(),
            1000
        );
    }

    #[tokio::test]
    async fn test_rcu_concurrent_readers_never_block_during_compaction() {
        let graph = Arc::new(CsrGraph::with_config(CsrGraphConfig {
            rebuild_threshold: 10,
            ..Default::default()
        }));

        let setup_tx = TxId::new(1);
        for i in 1..=200 {
            graph
                .add_entity(
                    setup_tx,
                    Entity::new(EntityId::new(i), format!("N{i}"), "Node"),
                )
                .await
                .unwrap();
        }
        graph.commit(setup_tx).await.unwrap();

        let stop_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let max_reader_load_nanos = Arc::new(AtomicU64::new(0));

        // Spawn 8 parallel reader tasks
        let mut reader_handles = Vec::new();
        for _ in 0..8 {
            let g = graph.clone();
            let stop = stop_flag.clone();
            let max_load = max_reader_load_nanos.clone();
            reader_handles.push(tokio::spawn(async move {
                while !stop.load(Ordering::Relaxed) {
                    let start = std::time::Instant::now();
                    let snapshot = g.inner_read();
                    let load_duration_nanos = start.elapsed().as_nanos() as u64;
                    max_load.fetch_max(load_duration_nanos, Ordering::Relaxed);

                    // Traversal runs lock-free on snapshot
                    let _ = g.traverse(EntityId::new(1), 2).await;
                    drop(snapshot);
                    tokio::task::yield_now().await;
                }
            }));
        }

        // Writer task continuously adds edges and triggers compact_async()
        let g_writer = graph.clone();
        let stop_writer = stop_flag.clone();
        let writer_handle = tokio::spawn(async move {
            let mut iter = 0u64;
            while !stop_writer.load(Ordering::Relaxed) {
                iter += 1;
                let tx = TxId::new(10 + iter);
                let src = EntityId::new((iter % 150) + 1);
                let dst = EntityId::new(((iter * 3) % 150) + 1);
                let _ = GraphIndex::add_edge(
                    g_writer.as_ref(),
                    tx,
                    Edge::new(src, dst, "link").with_weight(0.9),
                )
                .await;
                g_writer.commit(tx).await.ok();
                g_writer.compact_async().await.ok();
                tokio::task::yield_now().await;
            }
        });

        // Run concurrent test loop for 2 seconds
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        stop_flag.store(true, Ordering::Relaxed);

        for h in reader_handles {
            h.await.unwrap();
        }
        writer_handle.await.unwrap();

        let max_ns = max_reader_load_nanos.load(Ordering::Relaxed);
        let max_ms = max_ns as f64 / 1_000_000.0;
        println!(
            "RCU ArcSwap::load max reader latency: {:.4} ms ({} ns)",
            max_ms, max_ns
        );

        assert!(
            max_ms < 5.0,
            "RCU ArcSwap::load must remain lock-free (< 5.0 ms), got {:.4} ms",
            max_ms
        );
    }

    /// CONTRACT STUB: Demonstrates downstream PPR / Graph search access contract.
    /// Follow-up agents updating `ppr.rs` or `search.rs` consume `graph.inner_read()`.
    /// Read-your-own-write consistency requires inspecting both snapshot and uncompacted pending buffers.
    #[tokio::test]
    async fn test_ppr_read_contract_stub() {
        let graph = CsrGraph::new();
        let tx = TxId::new(1);
        let id1 = EntityId::new(1);
        let id2 = EntityId::new(2);

        graph
            .add_entity(tx, Entity::new(id1, "P1", "Person"))
            .await
            .unwrap();
        graph
            .add_entity(tx, Entity::new(id2, "P2", "Person"))
            .await
            .unwrap();
        GraphIndex::add_edge(&graph, tx, Edge::new(id1, id2, "knows"))
            .await
            .unwrap();
        graph.commit(tx).await.unwrap();

        // PPR contract: inner_read() returns point-in-time immutable Guard<Arc<GraphInner>>
        let snapshot = graph.inner_read();
        assert_eq!(
            snapshot
                .entities
                .iter()
                .filter(|e| e.id != EntityId::new(0))
                .count(),
            2
        );
        assert!(!snapshot.reverse_map.is_empty());

        // Snapshot coerces to &GraphInner for PPR
        fn consume_ppr_inner(inner: &GraphInner) -> usize {
            inner.reverse_map.len()
        }
        assert_eq!(consume_ppr_inner(&snapshot), 2);
    }

    #[tokio::test]
    async fn test_source_doc_id_provenance_and_compact_isolation() {
        let graph = Arc::new(CsrGraph::new());
        let tx = TxId::new(1);

        let id1 = EntityId::new(10);
        let id2 = EntityId::new(20);
        let id3 = EntityId::new(30);

        let doc100 = DocId(100);
        let doc200 = DocId(200);

        graph
            .insert_entity_direct(Entity::new(id1, "N1", "Type"))
            .unwrap();
        graph
            .insert_entity_direct(Entity::new(id2, "N2", "Type"))
            .unwrap();
        graph
            .insert_entity_direct(Entity::new(id3, "N3", "Type"))
            .unwrap();

        // Edge 10 -> 20 from doc100
        graph
            .add_edge(
                id1,
                id2,
                1.0,
                Some(tx),
                None,
                None,
                None,
                Some(doc100),
                None,
                None,
            )
            .await
            .unwrap();

        // Edge 20 -> 30 from doc200
        graph
            .add_edge(
                id2,
                id3,
                1.0,
                Some(tx),
                None,
                None,
                None,
                Some(doc200),
                None,
                None,
            )
            .await
            .unwrap();

        // Before compact
        assert_eq!(graph.source_doc_id_at(id1, id2), Some(doc100));
        assert_eq!(graph.source_doc_id_at(id2, id3), Some(doc200));

        let edges_doc100_pre = graph.edges_for_doc(doc100);
        assert!(edges_doc100_pre.contains(&(id1, id2)));
        assert!(!edges_doc100_pre.contains(&(id2, id3)));

        // Ensure querying DocId(10) (which equals EntityId 10) does NOT contain foreign edge (id1, id2)
        let edges_bogus_doc = graph.edges_for_doc(DocId(10));
        assert!(!edges_bogus_doc.contains(&(id1, id2)));

        // Compact CSR graph
        graph.compact();

        // After compact
        assert_eq!(graph.source_doc_id_at(id1, id2), Some(doc100));
        assert_eq!(graph.source_doc_id_at(id2, id3), Some(doc200));

        let edges_doc100_post = graph.edges_for_doc(doc100);
        assert!(edges_doc100_post.contains(&(id1, id2)));
        assert!(!edges_doc100_post.contains(&(id2, id3)));

        let edges_doc200_post = graph.edges_for_doc(doc200);
        assert!(edges_doc200_post.contains(&(id2, id3)));
        assert!(!edges_doc200_post.contains(&(id1, id2)));

        // Verify parallel arrays source_doc_ids in GraphInner
        let inner = graph.inner_read();
        assert_eq!(inner.targets.len(), inner.source_doc_ids.len());
        for (idx, target_idx) in inner.targets.iter().enumerate() {
            let target_id = inner.reverse_map[*target_idx];
            let source_doc = inner.source_doc_ids[idx];
            if target_id == id2 {
                assert_eq!(source_doc, doc100);
            } else if target_id == id3 {
                assert_eq!(source_doc, doc200);
            }
        }
    }

    #[test]
    fn test_hyperedge_insertion_and_lookup() {
        use crate::hyperedge::{HyperEdge, HyperEdgeId, RoleBinding, RoleId};

        let graph = CsrGraph::new();
        let e1 = EntityId::from("entity_a");
        let e2 = EntityId::from("entity_b");

        let rb1 = RoleBinding::new(RoleId::new(1), e1);
        let rb2 = RoleBinding::new(RoleId::new(2), e2);
        let he_id = HyperEdgeId(101);
        let edge = HyperEdge::new(he_id, EdgeType::Default, vec![rb1, rb2], 0.85);

        graph.insert_hyperedge(edge.clone());

        let retrieved = graph.get_hyperedge(he_id);
        assert_eq!(retrieved, Some(edge));

        let e1_hes = graph.hyperedges_for_entity(e1);
        assert_eq!(e1_hes, vec![he_id]);

        let e2_hes = graph.hyperedges_for_entity(e2);
        assert_eq!(e2_hes, vec![he_id]);
    }

    #[test]
    fn test_hyperedge_multiple_participants() {
        use crate::hyperedge::{HyperEdge, HyperEdgeId, RoleBinding, RoleId};

        let graph = CsrGraph::new();
        let e1 = EntityId::from("p1");
        let e2 = EntityId::from("p2");
        let e3 = EntityId::from("p3");

        let edge = HyperEdge::new(
            HyperEdgeId(202),
            EdgeType::Default,
            vec![
                RoleBinding::new(RoleId::new(1), e1),
                RoleBinding::new(RoleId::new(2), e2),
                RoleBinding::new(RoleId::new(3), e3),
            ],
            1.0,
        );

        graph.insert_hyperedge(edge);

        for entity in &[e1, e2, e3] {
            let hes = graph.hyperedges_for_entity(*entity);
            assert_eq!(hes, vec![HyperEdgeId(202)]);
        }
    }

    #[test]
    fn test_entity_multiple_hyperedges() {
        use crate::hyperedge::{HyperEdge, HyperEdgeId, RoleBinding, RoleId};

        let graph = CsrGraph::new();
        let e1 = EntityId::from("shared_entity");
        let e2 = EntityId::from("other_entity");

        let he1 = HyperEdge::new(
            HyperEdgeId(1),
            EdgeType::Default,
            vec![
                RoleBinding::new(RoleId::new(1), e1),
                RoleBinding::new(RoleId::new(2), e2),
            ],
            0.5,
        );
        let he2 = HyperEdge::new(
            HyperEdgeId(2),
            EdgeType::Default,
            vec![
                RoleBinding::new(RoleId::new(2), e1),
                RoleBinding::new(RoleId::new(1), e2),
            ],
            0.7,
        );

        graph.insert_hyperedge(he1);
        graph.insert_hyperedge(he2);

        let hes = graph.hyperedges_for_entity(e1);
        assert_eq!(hes.len(), 2);
        assert!(hes.contains(&HyperEdgeId(1)));
        assert!(hes.contains(&HyperEdgeId(2)));
    }

    #[test]
    fn test_hyperedge_nonexistent_entity() {
        use crate::hyperedge::HyperEdgeId;

        let graph = CsrGraph::new();
        let nonexistent = EntityId::from("missing_entity");

        let hes = graph.hyperedges_for_entity(nonexistent);
        assert!(hes.is_empty());

        let he = graph.get_hyperedge(HyperEdgeId(9999));
        assert!(he.is_none());
    }

    #[test]
    fn test_hyperedge_memory_estimation() {
        use crate::hyperedge::{HyperEdge, HyperEdgeId, RoleBinding, RoleId};

        let graph = CsrGraph::new();
        let initial_bytes = graph.inner_read().estimate_memory_bytes();

        let e1 = EntityId::from("m1");
        let e2 = EntityId::from("m2");

        let edge = HyperEdge::new(
            HyperEdgeId(500),
            EdgeType::Default,
            vec![
                RoleBinding::new(RoleId::new(1), e1),
                RoleBinding::new(RoleId::new(2), e2),
            ],
            0.9,
        );

        graph.insert_hyperedge(edge);

        let new_bytes = graph.inner_read().estimate_memory_bytes();
        assert!(
            new_bytes > initial_bytes,
            "Expected memory estimation to increase from {initial_bytes} but got {new_bytes}"
        );
    }

    #[tokio::test]
    async fn test_hyperedge_rcu_concurrent_compaction_consistency() {
        use crate::hyperedge::{HyperEdge, HyperEdgeId, RoleBinding, RoleId};

        let graph = std::sync::Arc::new(CsrGraph::new());
        let iterations = 50;

        let g_writer = graph.clone();
        let writer_handle = tokio::spawn(async move {
            for i in 0..iterations {
                let e1 = EntityId::from(format!("entity_{i}"));
                let e2 = EntityId::from(format!("entity_{}", i + 1));
                let he_id = HyperEdgeId(i as u64);

                let edge = HyperEdge::new(
                    he_id,
                    EdgeType::Default,
                    vec![
                        RoleBinding::new(RoleId::new(1), e1),
                        RoleBinding::new(RoleId::new(2), e2),
                    ],
                    1.0,
                );

                g_writer.insert_hyperedge(edge);
                g_writer.insert_edge_direct(e1, e2, 1.0).await.ok();

                if i % 5 == 0 {
                    g_writer.compact_async().await.ok();
                }
            }
        });

        let g_reader = graph.clone();
        let reader_handle = tokio::spawn(async move {
            for i in 0..iterations {
                let e1 = EntityId::from(format!("entity_{i}"));
                let hes = g_reader.hyperedges_for_entity(e1);
                for he_id in hes {
                    let he = g_reader.get_hyperedge(he_id);
                    assert!(
                        he.is_some(),
                        "RCU inconsistency: hyperedge {he_id:?} found in index but missing in snapshot"
                    );
                    let found_he = he.expect("checked above");
                    assert_eq!(found_he.id, he_id);
                }
                tokio::task::yield_now().await;
            }
        });

        let (r1, r2) = tokio::join!(writer_handle, reader_handle);
        assert!(r1.is_ok());
        assert!(r2.is_ok());
    }

    #[tokio::test]
    async fn test_hyperedge_doc_and_entity_rcu_atomicity() {
        use crate::hyperedge::{HyperEdge, HyperEdgeId, RoleBinding, RoleId};

        let graph = std::sync::Arc::new(CsrGraph::new());
        let doc_id = DocId::new(42);
        let e1 = EntityId::from("doc_entity_1");
        let e2 = EntityId::from("doc_entity_2");
        let he_id = HyperEdgeId(999);

        // Before insertion, both lookups return empty
        assert!(graph.hyperedges_for_doc(doc_id).is_empty());
        assert!(graph.hyperedges_for_entity(e1).is_empty());
        assert!(graph.get_hyperedge(he_id).is_none());

        // Insert hyperedge with source_doc_id
        let edge = HyperEdge::new(
            he_id,
            EdgeType::Default,
            vec![
                RoleBinding::new(RoleId::new(1), e1),
                RoleBinding::new(RoleId::new(2), e2),
            ],
            1.0,
        )
        .with_source_doc_id(Some(doc_id));

        graph.insert_hyperedge(edge);

        // Acquire snapshot via inner_read() and verify all 3 indices are atomically populated
        let snapshot = graph.inner_read();
        assert!(snapshot.hyperedges.contains_key(&he_id));
        assert!(snapshot
            .doc_to_hyperedges
            .get(&doc_id)
            .is_some_and(|set| set.contains(&he_id)));
        assert!(snapshot
            .hyperedge_index
            .get(&e1)
            .is_some_and(|set| set.contains(&he_id)));
        assert!(snapshot
            .hyperedge_index
            .get(&e2)
            .is_some_and(|set| set.contains(&he_id)));
        drop(snapshot);

        // Atomically tombstone hyperedge
        let success = graph.tombstone_hyperedge(he_id, TxId::new(10));
        assert!(success);

        // After tombstone, helper getters filter tombstoned hyperedges
        assert!(graph.hyperedges_for_doc(doc_id).is_empty());
        assert!(graph.hyperedges_for_entity(e1).is_empty());
        assert!(graph.get_hyperedge(he_id).is_none());
    }
}
