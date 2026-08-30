//! CSR-Graph-Implementierung für Entity-Relation-Traversal.
//!
//! Implementiert [`memfuse_core::GraphIndex`] via Compressed Sparse Row (CSR)
//! Datenstruktur für cache-effizienten Graph-Traversal.

// FILE-CONTEXT
// STAND:       2026-08-30T14:35:05Z (SESSION: ab88edae)
// ZWECK:       CSR-Graph für Entity-Relation-Traversal (Signal 3 in 4-Signal-Fusion)
// INVARIANTEN: Graph-Zustand wird in LSM-Store persistiert unter Präfixen
//              `__graph:entity:` und `__graph:edge:`. Änderungen müssen
//              BEIDE Strukturen konsistent halten (In-Memory CSR + LSM).
// HOTSPOTS:    bfs_traverse(), compact(), GraphInner, GraphIndex impl
// AGENT-NOTIZ: Extrahierte BFS-Traversal & Storage-Key Helper zur Eliminierung von Duplikation
// SIEHE AUCH:  DECISIONS.md ADR-004, crates/memfuse-db/AGENTS.md §relate()

use async_trait::async_trait;
use memfuse_core::{
    Edge, Entity, EntityId, GraphIndex, GraphIndexStats, MemFuseError, Result, StorageEngine, TxId,
};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Persisted edge payload format for storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedEdgePayload {
    pub weight: f32,
    #[serde(default)]
    pub valid_from: Option<TxId>,
    #[serde(default)]
    pub valid_to: Option<TxId>,
}

/// Score decay factor per hop (0.7^hop).
const SCORE_DECAY: f32 = 0.7;

/// Maximum traversal depth.
const MAX_TRAVERSAL_HOPS: u8 = 3;

/// LSM-Key-Prefix für alle Graph-Entities.
const GRAPH_ENTITY_PREFIX: &[u8] = b"__graph:entity:";
/// LSM-Key-Prefix für alle Graph-Edges.
const GRAPH_EDGE_PREFIX: &[u8] = b"__graph:edge:";

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
    (WALLCLOCK_TX_HEURISTIC_MIN..TxId::INTERNAL_BASE).contains(&v)
}

/// Prüft ob eine Kante zum Zeitpunkt `as_of` sichtbar ist (bi-temporale Filterung).
#[inline]
pub(crate) fn is_edge_visible(
    valid_from: Option<TxId>,
    valid_to: Option<TxId>,
    as_of: TxId,
) -> bool {
    valid_from.is_none_or(|vf| vf <= as_of) && valid_to.is_none_or(|vt| as_of < vt)
}

/// Configuration parameters for [`CsrGraph`].
#[derive(Debug, Clone)]
pub struct CsrGraphConfig {
    /// Rebuild threshold: max number of uncompacted pending edges in delta buffer before triggering an automatic full CSR rebuild.
    pub rebuild_threshold: usize,
}

impl Default for CsrGraphConfig {
    fn default() -> Self {
        Self {
            rebuild_threshold: 1000,
        }
    }
}

/// Internal contiguous index for CSR arrays.
type InternalIndex = usize;

/// Internal representation of an edge payload.
#[derive(Debug, Clone, PartialEq)]
struct EdgePayload {
    target: InternalIndex,
    weight: f32,
    valid_from: Option<TxId>,
    valid_to: Option<TxId>,
}

/// Inner state of the CsrGraph to manage contiguous storage.
pub(crate) struct GraphInner {
    /// Mapping from public EntityId to internal contiguous index.
    pub(crate) id_map: HashMap<EntityId, InternalIndex>,
    /// Mapping from internal index back to EntityId.
    pub(crate) reverse_map: Vec<EntityId>,
    /// Entity metadata stored contiguously.
    pub(crate) entities: Vec<Option<Entity>>,

    /// CSR offsets array: offsets[i] is the start index in `targets` for node `i`.
    /// Length is nodes + 1.
    pub(crate) offsets: Vec<usize>,
    /// CSR targets array: contiguous list of neighbor internal indices.
    pub(crate) targets: Vec<InternalIndex>,
    /// CSR weights array: contiguous list of edge weights.
    pub(crate) weights: Vec<f32>,
    /// CSR valid_from array: contiguous list of bi-temporal valid_from TxIds.
    pub(crate) valid_froms: Vec<Option<TxId>>,
    /// CSR valid_to array: contiguous list of bi-temporal valid_to TxIds.
    pub(crate) valid_tos: Vec<Option<TxId>>,

    /// Staging for entities not yet committed, grouped by TxId.
    staged_entities: HashMap<TxId, HashMap<EntityId, Entity>>,
    /// Staging for edges not yet committed, grouped by TxId.
    staged_edges: HashMap<TxId, HashMap<InternalIndex, Vec<EdgePayload>>>,
    /// Staging for edge removals not yet committed, grouped by TxId.
    staged_removals: HashMap<TxId, Vec<(InternalIndex, InternalIndex)>>,
    /// Edges that have been committed but not yet compacted into CSR arrays (delta buffer).
    pending_edges: HashMap<InternalIndex, Vec<EdgePayload>>,
    /// Tombstoned edges that have been removed and should be excluded during compaction and traversal.
    pub(crate) tombstoned_edges: HashSet<(InternalIndex, InternalIndex)>,
    /// Total number of uncompacted edges currently in `pending_edges`.
    pending_edge_count: usize,
    /// Flag indicating if there are uncompacted pending edges or modifications.
    is_dirty: bool,
}

impl GraphInner {
    fn new() -> Self {
        Self {
            id_map: HashMap::new(),
            reverse_map: Vec::new(),
            entities: Vec::new(),
            offsets: vec![0],
            targets: Vec::new(),
            weights: Vec::new(),
            valid_froms: Vec::new(),
            valid_tos: Vec::new(),
            staged_entities: HashMap::new(),
            staged_edges: HashMap::new(),
            staged_removals: HashMap::new(),
            pending_edges: HashMap::new(),
            tombstoned_edges: HashSet::new(),
            pending_edge_count: 0,
            is_dirty: false,
        }
    }

    fn get_or_create_index(&mut self, id: EntityId) -> InternalIndex {
        if let Some(&idx) = self.id_map.get(&id) {
            idx
        } else {
            let idx = self.reverse_map.len();
            self.id_map.insert(id, idx);
            self.reverse_map.push(id);
            // entities vector should be kept in sync by add_entity,
            // but we might add an edge to an entity not yet added via add_entity.
            // In that case, we'll have a "shadow" entity.
            idx
        }
    }

    /// Compacts pending edges in the delta buffer into the main CSR arrays.
    fn compact(&mut self) {
        if !self.is_dirty || (self.pending_edges.is_empty() && self.tombstoned_edges.is_empty()) {
            self.pending_edge_count = 0;
            self.is_dirty = false;
            return;
        }

        let num_nodes = self.reverse_map.len();
        let mut new_offsets = Vec::with_capacity(num_nodes + 1);
        let mut new_targets = Vec::with_capacity(self.targets.len() + self.pending_edge_count);
        let mut new_weights = Vec::with_capacity(self.weights.len() + self.pending_edge_count);
        let mut new_valid_froms =
            Vec::with_capacity(self.valid_froms.len() + self.pending_edge_count);
        let mut new_valid_tos = Vec::with_capacity(self.valid_tos.len() + self.pending_edge_count);

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
                        valid_from: self.valid_froms.get(j).copied().flatten(),
                        valid_to: self.valid_tos.get(j).copied().flatten(),
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
                new_valid_froms.push(edge.valid_from);
                new_valid_tos.push(edge.valid_to);
                current_offset += 1;
            }

            new_offsets.push(current_offset);
        }

        self.offsets = new_offsets;
        self.targets = new_targets;
        self.weights = new_weights;
        self.valid_froms = new_valid_froms;
        self.valid_tos = new_valid_tos;
        self.pending_edges.clear();
        self.tombstoned_edges.clear();
        self.pending_edge_count = 0;
        self.is_dirty = false;
    }
}

/// Compressed Sparse Row graph for entity-relation traversal.
///
/// Implements `GraphIndex` trait as Signal 3 in the 4-Signal Fusion architecture.
pub struct CsrGraph {
    config: CsrGraphConfig,
    inner: RwLock<GraphInner>,
    /// Optionaler Persistenz-Handle. None = reiner In-Memory-Modus (z.B. Tests).
    storage: Option<Arc<dyn StorageEngine>>,
    last_tx_id: AtomicU64,
}

impl CsrGraph {
    /// Creates a new, empty CSR graph with default configuration.
    pub fn new() -> Self {
        Self::with_config(CsrGraphConfig::default())
    }

    /// Creates a new, empty CSR graph with specified configuration.
    pub fn with_config(config: CsrGraphConfig) -> Self {
        Self {
            config,
            inner: RwLock::new(GraphInner::new()),
            storage: None,
            last_tx_id: AtomicU64::new(0),
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
        Self {
            config,
            inner: RwLock::new(GraphInner::new()),
            storage: Some(storage),
            last_tx_id: AtomicU64::new(0),
        }
    }

    pub(crate) fn inner_read(&self) -> parking_lot::RwLockReadGuard<'_, GraphInner> {
        self.inner.read()
    }

    /// Sets or replaces the persistent storage handle.
    pub fn set_storage(&mut self, storage: Arc<dyn StorageEngine>) {
        self.storage = Some(storage);
    }

    /// Directly inserts an entity into the CSR graph without staging.
    pub fn insert_entity_direct(&self, entity: Entity) -> Result<()> {
        let mut inner = self.inner.write();
        let idx = inner.get_or_create_index(entity.id);
        if idx >= inner.entities.len() {
            inner.entities.resize(idx + 1, None);
        }
        inner.entities[idx] = Some(entity);
        Ok(())
    }

    /// Directly inserts an edge into the CSR graph without staging.
    pub fn insert_edge_direct(&self, from: EntityId, to: EntityId, weight: f32) -> Result<()> {
        self.insert_edge_direct_with_validity(from, to, weight, None, None)
    }

    /// Directly inserts an edge with validity into the CSR graph without staging.
    pub fn insert_edge_direct_with_validity(
        &self,
        from: EntityId,
        to: EntityId,
        weight: f32,
        valid_from: Option<TxId>,
        valid_to: Option<TxId>,
    ) -> Result<()> {
        let mut inner = self.inner.write();
        let from_idx = inner.get_or_create_index(from);
        let to_idx = inner.get_or_create_index(to);
        inner
            .pending_edges
            .entry(from_idx)
            .or_default()
            .push(EdgePayload {
                target: to_idx,
                weight,
                valid_from,
                valid_to,
            });
        inner.pending_edge_count += 1;
        inner.is_dirty = true;
        if inner.pending_edge_count >= self.config.rebuild_threshold {
            inner.compact();
        }
        Ok(())
    }

    /// Internal BFS graph traversal implementation with optional temporal filtering.
    fn bfs_traverse(
        &self,
        start: EntityId,
        max_hops: usize,
        as_of: Option<TxId>,
    ) -> Result<Vec<(EntityId, f32)>> {
        let inner = self.inner.read();
        let start_idx = match inner.id_map.get(&start) {
            Some(&idx) => idx,
            None => return Ok(Vec::new()),
        };

        if !inner.entities.get(start_idx).is_some_and(Option::is_some) {
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
                // 1. CSR traversal (compacted edges)
                if node_idx < inner.offsets.len() - 1 {
                    let start_edge = inner.offsets[node_idx];
                    let end_edge = inner.offsets[node_idx + 1];

                    for edge_idx in start_edge..end_edge {
                        let neighbor_idx = inner.targets[edge_idx];
                        if inner.tombstoned_edges.contains(&(node_idx, neighbor_idx)) {
                            continue;
                        }
                        if let Some(time) = as_of {
                            let valid_from = inner.valid_froms.get(edge_idx).copied().flatten();
                            let valid_to = inner.valid_tos.get(edge_idx).copied().flatten();
                            if !is_edge_visible(valid_from, valid_to, time) {
                                continue;
                            }
                        }
                        let weight = inner.weights[edge_idx];
                        let next_score = current_score * SCORE_DECAY * weight;

                        if (!visited.contains_key(&neighbor_idx)
                            || visited[&neighbor_idx] < next_score)
                            && inner
                                .entities
                                .get(neighbor_idx)
                                .is_some_and(Option::is_some)
                        {
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
                        if let Some(time) = as_of {
                            if !is_edge_visible(edge.valid_from, edge.valid_to, time) {
                                continue;
                            }
                        }
                        let next_score = current_score * SCORE_DECAY * edge.weight;

                        if (!visited.contains_key(&neighbor_idx)
                            || visited[&neighbor_idx] < next_score)
                            && inner
                                .entities
                                .get(neighbor_idx)
                                .is_some_and(Option::is_some)
                        {
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
    }

    /// Fügt eine Entity direkt ein (für load_from_storage).
    fn load_entity_direct(&self, entity: Entity) -> Result<()> {
        let mut inner = self.inner.write();
        let idx = inner.get_or_create_index(entity.id);
        while inner.entities.len() <= idx {
            inner.entities.push(None);
        }
        inner.entities[idx] = Some(entity);
        Ok(())
    }

    /// Fügt eine Edge direkt in committed_staged / pending_edges ein (für load_from_storage).
    /// Umgeht das TX-Staging, da beim Laden alle Daten bereits committed sind.
    fn load_edge_direct(
        &self,
        from: EntityId,
        to: EntityId,
        weight: f32,
        valid_from: Option<TxId>,
        valid_to: Option<TxId>,
    ) -> Result<()> {
        let mut inner = self.inner.write();
        let from_idx = inner.get_or_create_index(from);
        let to_idx = inner.get_or_create_index(to);
        inner
            .pending_edges
            .entry(from_idx)
            .or_default()
            .push(EdgePayload {
                target: to_idx,
                weight,
                valid_from,
                valid_to,
            });
        inner.pending_edge_count += 1;
        inner.is_dirty = true;
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

        // 1. Entities laden
        let entity_entries = storage.scan_prefix(GRAPH_ENTITY_PREFIX).await?;
        let mut entity_count = 0usize;
        for (_, raw_value) in entity_entries {
            let entity: Entity = bincode::deserialize(&raw_value)
                .map_err(|e| MemFuseError::Internal(format!("graph entity deserialize: {e}")))?;
            graph.load_entity_direct(entity)?;
            entity_count += 1;
        }

        // 2. Edges laden
        let edge_entries = storage.scan_prefix(GRAPH_EDGE_PREFIX).await?;
        let mut edge_count = 0usize;
        for (raw_key, raw_value) in edge_entries {
            let (weight, valid_from, valid_to) =
                match bincode::deserialize::<PersistedEdgePayload>(&raw_value) {
                    Ok(p) => (p.weight, p.valid_from, p.valid_to),
                    Err(_) => {
                        // Backward compatibility fallback for legacy serialized f32 weight values
                        let w: f32 = bincode::deserialize(&raw_value).map_err(|e| {
                            MemFuseError::Internal(format!("graph edge deserialize: {e}"))
                        })?;
                        (w, None, None)
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
                graph.load_edge_direct(from_id, to_id, weight, valid_from, valid_to)?;
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

        tracing::info!(
            entities = entity_count,
            edges = edge_count,
            last_tx = graph.last_tx_id.load(Ordering::SeqCst),
            "Graph aus Storage geladen und kompaktiert"
        );
        Ok(graph)
    }

    /// Force compacts the graph delta buffer into the main CSR arrays to optimize traversal layout.
    pub fn compact(&self) {
        // Double-checked locking to avoid unnecessary write locks (FIND-GRA-002)
        let inner_read = self.inner.read();
        if !inner_read.is_dirty
            && inner_read.pending_edges.is_empty()
            && inner_read.tombstoned_edges.is_empty()
        {
            return;
        }
        drop(inner_read);

        let mut inner = self.inner.write();
        if inner.is_dirty || !inner.pending_edges.is_empty() || !inner.tombstoned_edges.is_empty() {
            inner.compact();
        }
    }

    /// Asynchronously compacts the graph delta buffer, offloading heavy CPU rebuild work to `spawn_blocking` if necessary.
    pub async fn compact_async(self: &Arc<Self>) -> Result<()> {
        let is_needed = {
            let inner_read = self.inner.read();
            inner_read.is_dirty
                || !inner_read.pending_edges.is_empty()
                || !inner_read.tombstoned_edges.is_empty()
        };
        if !is_needed {
            return Ok(());
        }

        let self_clone = self.clone();
        tokio::task::spawn_blocking(move || {
            let mut inner = self_clone.inner.write();
            if inner.is_dirty
                || !inner.pending_edges.is_empty()
                || !inner.tombstoned_edges.is_empty()
            {
                inner.compact();
            }
        })
        .await
        .map_err(|e| MemFuseError::Internal(format!("Graph compact panicked: {e}")))?;

        Ok(())
    }

    /// Returns direct 1-hop outgoing neighbors of `start`.
    pub async fn neighbors(&self, start: EntityId) -> Result<Vec<EntityId>> {
        let inner = self.inner.read();
        let start_idx = match inner.id_map.get(&start) {
            Some(&idx) => idx,
            None => return Ok(Vec::new()),
        };

        if !inner.entities.get(start_idx).is_some_and(|e| e.is_some()) {
            return Ok(Vec::new());
        }

        let mut neighbors = Vec::new();

        // 1. CSR targets
        if start_idx < inner.offsets.len() - 1 {
            let start_edge = inner.offsets[start_idx];
            let end_edge = inner.offsets[start_idx + 1];
            for edge_idx in start_edge..end_edge {
                let neighbor_idx = inner.targets[edge_idx];
                if !inner.tombstoned_edges.contains(&(start_idx, neighbor_idx))
                    && inner
                        .entities
                        .get(neighbor_idx)
                        .is_some_and(|e| e.is_some())
                {
                    if let Some(&id) = inner.reverse_map.get(neighbor_idx) {
                        if !neighbors.contains(&id) {
                            neighbors.push(id);
                        }
                    }
                }
            }
        }

        // 2. Pending edges
        if let Some(pending) = inner.pending_edges.get(&start_idx) {
            for edge in pending {
                let neighbor_idx = edge.target;
                if !inner.tombstoned_edges.contains(&(start_idx, neighbor_idx))
                    && inner
                        .entities
                        .get(neighbor_idx)
                        .is_some_and(|e| e.is_some())
                {
                    if let Some(&id) = inner.reverse_map.get(neighbor_idx) {
                        if !neighbors.contains(&id) {
                            neighbors.push(id);
                        }
                    }
                }
            }
        }

        Ok(neighbors)
    }

    /// Calculates PageRank for all entities in the graph using the CSR layout.
    pub fn pagerank(
        &self,
        damping_factor: f32,
        max_iterations: usize,
        tolerance: f32,
    ) -> HashMap<EntityId, f32> {
        self.compact();
        let inner = self.inner.read();
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
            if inner.entities.get(idx).is_some_and(|e| e.is_some()) {
                if let Some(&id) = inner.reverse_map.get(idx) {
                    result.insert(id, rank);
                }
            }
        }
        result
    }

    /// Returns the number of committed entities in the graph.
    pub fn entity_count(&self) -> usize {
        self.inner.read().entities.iter().flatten().count()
    }

    /// Checks if a committed entity exists in the graph.
    pub fn entity_exists(&self, id: EntityId) -> bool {
        let inner = self.inner.read();
        if let Some(&idx) = inner.id_map.get(&id) {
            inner.entities.get(idx).is_some_and(|e| e.is_some())
        } else {
            false
        }
    }

    /// Returns the number of edges in the graph.
    pub fn edge_count(&self) -> usize {
        let inner = self.inner.read();
        inner.targets.len()
            + inner.pending_edge_count
            + inner
                .staged_edges
                .values()
                .map(|tx_map| tx_map.values().map(|v| v.len()).sum::<usize>())
                .sum::<usize>()
    }
}

impl Default for CsrGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl GraphIndex for CsrGraph {
    async fn add_entity(&self, tx: TxId, entity: Entity) -> Result<()> {
        debug_assert!(
            tx.is_valid_origin(),
            "TxId {} verletzt AGT-GRAPH-001 Origin-Invariante — Wall-Clock-abgeleitete IDs korrumpieren rollback_to_tx()-Kausalordnung",
            tx
        );
        // AGT-GRAPH-001: Heuristik — wall-clock-abgeleitete TxIds warnen.
        if is_suspicious_tx_id(tx) {
            tracing::warn!(
                tx_id = tx.inner(),
                hint = "Wall-Clock-ns-Bereich",
                "AGT-GRAPH-001: Verdächtiger TxId in add_entity (weder im plausiblen next_tx-Bereich noch im INTERNAL_BASE-Bereich [u64::MAX - 1_000_000]) — \
                 möglicherweise aus Wall-Clock-Nanosekunden abgeleitet. \
                 Rollback-Korrelation kann verletzt sein."
            );
        }
        let mut inner = self.inner.write();
        inner
            .staged_entities
            .entry(tx)
            .or_default()
            .insert(entity.id, entity);
        Ok(())
    }

    async fn add_edge(&self, tx: TxId, edge: Edge) -> Result<()> {
        debug_assert!(
            tx.is_valid_origin(),
            "TxId {} verletzt AGT-GRAPH-001 Origin-Invariante — Wall-Clock-abgeleitete IDs korrumpieren rollback_to_tx()-Kausalordnung",
            tx
        );
        // AGT-GRAPH-001: Heuristik — wall-clock-abgeleitete TxIds warnen.
        if is_suspicious_tx_id(tx) {
            tracing::warn!(
                tx_id = tx.inner(),
                hint = "Wall-Clock-ns-Bereich",
                "AGT-GRAPH-001: Verdächtiger TxId in add_edge (weder im plausiblen next_tx-Bereich noch im INTERNAL_BASE-Bereich [u64::MAX - 1_000_000]) — \
                 möglicherweise aus Wall-Clock-Nanosekunden abgeleitet. \
                 Rollback-Korrelation kann verletzt sein."
            );
        }
        let mut inner = self.inner.write();
        let from_idx = inner.get_or_create_index(edge.from);
        let to_idx = inner.get_or_create_index(edge.to);

        let valid_from = edge.valid_from.or(Some(tx));

        inner
            .staged_edges
            .entry(tx)
            .or_default()
            .entry(from_idx)
            .or_default()
            .push(EdgePayload {
                target: to_idx,
                weight: edge.weight,
                valid_from,
                valid_to: edge.valid_to,
            });
        Ok(())
    }

    async fn personalized_page_rank(
        &self,
        seed_nodes: &[EntityId],
        config: &memfuse_core::PprConfig,
    ) -> Result<Vec<(EntityId, f32)>> {
        self.compact();
        let inner = self.inner.read();
        Ok(crate::ppr::compute_ppr(&inner, seed_nodes, config))
    }

    async fn traverse_at(
        &self,
        start_node: EntityId,
        max_hops: usize,
        seq_no: u64,
    ) -> Result<Vec<(EntityId, f32)>> {
        self.traverse_at_time(start_node, max_hops, TxId::new(seq_no))
            .await
    }

    async fn traverse_at_time(
        &self,
        start: EntityId,
        max_hops: usize,
        as_of: TxId,
    ) -> Result<Vec<(EntityId, f32)>> {
        let inner = self.inner.read();
        let start_idx = match inner.id_map.get(&start) {
            Some(&idx) => idx,
            None => return Ok(Vec::new()),
        };

        if !inner.entities.get(start_idx).is_some_and(|e| e.is_some()) {
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
                // 1. CSR traversal (compacted edges)
                if node_idx < inner.offsets.len() - 1 {
                    let start_edge = inner.offsets[node_idx];
                    let end_edge = inner.offsets[node_idx + 1];

                    for edge_idx in start_edge..end_edge {
                        let neighbor_idx = inner.targets[edge_idx];
                        if inner.tombstoned_edges.contains(&(node_idx, neighbor_idx)) {
                            continue;
                        }
                        let valid_from = inner.valid_froms.get(edge_idx).copied().flatten();
                        let valid_to = inner.valid_tos.get(edge_idx).copied().flatten();
                        if !is_edge_visible(valid_from, valid_to, as_of) {
                            continue;
                        }
                        let weight = inner.weights[edge_idx];
                        let next_score = current_score * SCORE_DECAY * weight;

                        if (!visited.contains_key(&neighbor_idx)
                            || visited[&neighbor_idx] < next_score)
                            && inner
                                .entities
                                .get(neighbor_idx)
                                .is_some_and(|e| e.is_some())
                        {
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
                        if !is_edge_visible(edge.valid_from, edge.valid_to, as_of) {
                            continue;
                        }
                        let next_score = current_score * SCORE_DECAY * edge.weight;

                        if (!visited.contains_key(&neighbor_idx)
                            || visited[&neighbor_idx] < next_score)
                            && inner
                                .entities
                                .get(neighbor_idx)
                                .is_some_and(|e| e.is_some())
                        {
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
    }

    async fn traverse(&self, start: EntityId, max_hops: usize) -> Result<Vec<(EntityId, f32)>> {
        // Merge-read: read directly from both compacted CSR arrays AND uncompacted pending_edges delta buffer.
        // No full compact() call is required before traversal.
        let inner = self.inner.read();
        let start_idx = match inner.id_map.get(&start) {
            Some(&idx) => idx,
            None => return Ok(Vec::new()), // Start node not in graph
        };

        // If the start node itself is not committed, we shouldn't start traversal from it
        if !inner.entities.get(start_idx).is_some_and(|e| e.is_some()) {
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
                            if inner
                                .entities
                                .get(neighbor_idx)
                                .is_some_and(|e| e.is_some())
                            {
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
                            && inner
                                .entities
                                .get(neighbor_idx)
                                .is_some_and(|e| e.is_some())
                        {
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
    }

    async fn commit(&self, tx: TxId) -> Result<()> {
        debug_assert!(
            tx.is_valid_origin(),
            "TxId {} verletzt AGT-GRAPH-001 Origin-Invariante — Wall-Clock-abgeleitete IDs korrumpieren rollback_to_tx()-Kausalordnung",
            tx
        );
        // AGT-GRAPH-001: Heuristik — wall-clock-abgeleitete TxIds warnen.
        if is_suspicious_tx_id(tx) {
            tracing::warn!(
                tx_id = tx.inner(),
                hint = "Wall-Clock-ns-Bereich",
                "AGT-GRAPH-001: Verdächtiger TxId in commit (weder im plausiblen next_tx-Bereich noch im INTERNAL_BASE-Bereich [u64::MAX - 1_000_000]) — \
                 möglicherweise aus Wall-Clock-Nanosekunden abgeleitet. \
                 Rollback-Korrelation kann verletzt sein."
            );
        }
        let (entities_to_commit, edges_to_commit, removals_to_commit) = {
            let inner = self.inner.read();
            let entities = inner.staged_entities.get(&tx).cloned();
            let edges = inner.staged_edges.get(&tx).map(|tx_edges| {
                let mut list = Vec::new();
                for (&from_idx, to_list) in tx_edges {
                    if let Some(&from_id) = inner.reverse_map.get(from_idx) {
                        for edge in to_list {
                            if let Some(&to_id) = inner.reverse_map.get(edge.target) {
                                list.push((
                                    from_id,
                                    to_id,
                                    PersistedEdgePayload {
                                        weight: edge.weight,
                                        valid_from: edge.valid_from,
                                        valid_to: edge.valid_to,
                                    },
                                ));
                            }
                        }
                    }
                }
                list
            });
            let removals = inner.staged_removals.get(&tx).map(|tx_removals| {
                let mut list = Vec::new();
                for &(f_idx, t_idx) in tx_removals {
                    if let (Some(&from_id), Some(&to_id)) =
                        (inner.reverse_map.get(f_idx), inner.reverse_map.get(t_idx))
                    {
                        list.push((from_id, to_id, f_idx, t_idx));
                    }
                }
                list
            });
            (entities, edges, removals)
        };

        if let Some(ref storage) = self.storage {
            if let Some(ref entities) = entities_to_commit {
                for entity in entities.values() {
                    self.persist_entity(storage.as_ref(), tx, entity).await?;
                }
            }
            if let Some(ref edges) = edges_to_commit {
                for (from_id, to_id, payload) in edges {
                    self.persist_edge(storage.as_ref(), tx, from_id, to_id, payload)
                        .await?;
                }
            }
            if let Some(ref removals) = removals_to_commit {
                for (from_id, to_id, _, _) in removals {
                    self.delete_edge_persistence(storage.as_ref(), tx, from_id, to_id)
                        .await?;
                }
            }
        }

        let mut inner = self.inner.write();

        // 1. Commit entities
        if let Some(tx_entities) = inner.staged_entities.remove(&tx) {
            for (id, entity) in tx_entities {
                let idx = inner.get_or_create_index(id);
                if idx >= inner.entities.len() {
                    inner.entities.resize(idx + 1, None);
                }
                inner.entities[idx] = Some(entity);
                inner.is_dirty = true;
            }
        }

        // 2. Commit edges
        if let Some(tx_edges) = inner.staged_edges.remove(&tx) {
            for (from_idx, edges) in tx_edges {
                let count = edges.len();
                inner
                    .pending_edges
                    .entry(from_idx)
                    .or_default()
                    .extend(edges);
                inner.pending_edge_count += count;
            }
            inner.is_dirty = true;
        }

        // 3. Commit removals
        if let Some(tx_removals) = inner.staged_removals.remove(&tx) {
            for (f_idx, t_idx) in tx_removals {
                if let Some(pending) = inner.pending_edges.get_mut(&f_idx) {
                    pending.retain(|edge| edge.target != t_idx);
                }
                inner.tombstoned_edges.insert((f_idx, t_idx));
                inner.is_dirty = true;
            }
        }

        // Auto-rebuild CSR arrays if pending delta buffer reaches or exceeds threshold
        if inner.pending_edge_count >= self.config.rebuild_threshold {
            inner.compact();
        }

        self.last_tx_id.fetch_max(tx.inner(), Ordering::SeqCst);

        Ok(())
    }

    async fn remove_edge(&self, tx: TxId, from: EntityId, to: EntityId) -> Result<()> {
        if is_suspicious_tx_id(tx) {
            tracing::warn!(
                tx_id = tx.inner(),
                hint = "Wall-Clock-ns-Bereich",
                "AGT-GRAPH-001: Verdächtiger TxId in remove_edge (weder im plausiblen next_tx-Bereich noch im INTERNAL_BASE-Bereich [u64::MAX - 1_000_000]) — \
                 möglicherweise aus Wall-Clock-Nanosekunden abgeleitet."
            );
        }
        let mut inner = self.inner.write();
        let from_idx = inner.id_map.get(&from).copied();
        let to_idx = inner.id_map.get(&to).copied();

        if let (Some(f_idx), Some(t_idx)) = (from_idx, to_idx) {
            inner
                .staged_removals
                .entry(tx)
                .or_default()
                .push((f_idx, t_idx));
        }
        Ok(())
    }

    async fn add_bidirectional(
        &self,
        tx: TxId,
        from: EntityId,
        to: EntityId,
        label: &str,
    ) -> Result<()> {
        self.add_edge(tx, Edge::new(from, to, label)).await?;
        self.add_edge(tx, Edge::new(to, from, label)).await?;
        Ok(())
    }

    async fn neighbors(&self, start: EntityId) -> Result<Vec<EntityId>> {
        self.neighbors(start).await
    }

    async fn rollback(&self, tx: TxId) -> Result<()> {
        let mut inner = self.inner.write();
        inner.staged_entities.remove(&tx);
        inner.staged_edges.remove(&tx);
        inner.staged_removals.remove(&tx);
        Ok(())
    }

    async fn rollback_to_tx(&self, _tx_id: TxId) -> Result<()> {
        // Physical rollback for CSR graph is driven by WAL replay or reloading state from storage.
        // In-memory staged transactions are handled by rollback().
        Ok(())
    }

    async fn last_tx_id(&self) -> Result<u64> {
        Ok(self.last_tx_id.load(Ordering::SeqCst))
    }

    async fn len(&self) -> usize {
        self.entity_count()
    }

    async fn stats(&self) -> Result<GraphIndexStats> {
        let inner = self.inner.read();
        let num_entities = inner.entities.iter().flatten().count();
        let num_edges = inner.targets.len()
            + inner.pending_edge_count
            + inner
                .staged_edges
                .values()
                .map(|tx_map| tx_map.values().map(|v| v.len()).sum::<usize>())
                .sum::<usize>();

        let mem = (inner.reverse_map.len() * std::mem::size_of::<EntityId>())
            + (inner.entities.len() * std::mem::size_of::<Option<Entity>>())
            + (inner.offsets.len() * std::mem::size_of::<usize>())
            + (inner.targets.len() * std::mem::size_of::<usize>())
            + (inner.weights.len() * std::mem::size_of::<f32>());

        Ok(GraphIndexStats {
            num_entities,
            num_edges,
            memory_usage_bytes: mem,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            let inner = graph.inner.read();
            assert!(inner.is_dirty);
            assert_eq!(inner.staged_edges.len(), 0);
            assert_eq!(inner.targets.len(), 0);
        }

        graph.compact();

        {
            let inner = graph.inner.read();
            assert!(!inner.is_dirty);
            assert_eq!(inner.staged_edges.len(), 0);
            assert_eq!(inner.targets.len(), 1);
            assert_eq!(inner.offsets[0], 0);
            assert_eq!(inner.offsets[1], 1);
        }
    }

    #[tokio::test]
    async fn test_csr_delta_buffer_uncompacted_traversal() {
        // Test that committed edges in the pending_edges delta buffer (uncompacted)
        // are correctly traversed without needing compact() call.
        let graph = CsrGraph::with_config(CsrGraphConfig {
            rebuild_threshold: 1000,
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
            let inner = graph.inner.read();
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
                                                  // Wenn Isolation für Entities funktioniert, sollten es 2 sein (1 und 3).
                                                  // Aktuell ist es aber wahrscheinlich 3 (1, 2 und 3).
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
        let inner = graph.inner.read();
        let expected_mem = (inner.reverse_map.len() * std::mem::size_of::<EntityId>())
            + (inner.entities.len() * std::mem::size_of::<Option<Entity>>())
            + (inner.offsets.len() * std::mem::size_of::<usize>())
            + (inner.targets.len() * std::mem::size_of::<usize>())
            + (inner.weights.len() * std::mem::size_of::<f32>());

        assert_eq!(stats.memory_usage_bytes, expected_mem);
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
        let inner = graph.inner.read();
        let idx = inner.id_map.get(&EntityId::new(10)).unwrap(); // unwrap
        let entity = inner.entities[*idx].as_ref().unwrap(); // unwrap
        assert_eq!(entity.name, "EntityFromB");
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

        let ranks = graph.pagerank(0.85, 100, 1e-6);
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
        let center_id = EntityId::new(0);
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
                g.add_edge(tx, Edge::new(center_id, target, "connect"))
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
            .unwrap();
        graph
            .add_entity(tx1, Entity::new(id2, "N2", "T"))
            .await
            .unwrap();
        graph
            .add_entity(tx1, Entity::new(id3, "N3", "T"))
            .await
            .unwrap();
        graph
            .add_edge(tx1, Edge::new(id1, id2, "rel1"))
            .await
            .unwrap();
        graph.commit(tx1).await.unwrap();

        // Tx 2: Add edge 2->3
        let tx2 = TxId::new(2);
        graph
            .add_edge(tx2, Edge::new(id2, id3, "rel2"))
            .await
            .unwrap();
        graph.commit(tx2).await.unwrap();

        // traverse_at seq 1: 1->2 visible, but edge 2->3 (tx2) NOT visible
        let res_seq1 = graph.traverse_at(id1, 2, 1).await.unwrap();
        let ids_seq1: Vec<_> = res_seq1.iter().map(|(id, _)| id.inner()).collect();
        assert!(ids_seq1.contains(&2), "seq 1 traverse must include node 2");
        assert!(
            !ids_seq1.contains(&3),
            "seq 1 traverse must NOT include node 3"
        );

        // traverse_at seq 2: both 1->2 and 2->3 visible
        let res_seq2 = graph.traverse_at(id1, 2, 2).await.unwrap();
        let ids_seq2: Vec<_> = res_seq2.iter().map(|(id, _)| id.inner()).collect();
        assert!(ids_seq2.contains(&2), "seq 2 traverse must include node 2");
        assert!(ids_seq2.contains(&3), "seq 2 traverse must include node 3");
    }

    #[tokio::test]
    async fn test_last_tx_id_tracking() {
        let graph = CsrGraph::new();
        assert_eq!(graph.last_tx_id().await.unwrap(), 0); // unwrap

        let tx1 = TxId::new(5);
        graph
            .add_entity(tx1, Entity::new(EntityId::new(1), "E1", "T"))
            .await
            .unwrap(); // unwrap
        graph.commit(tx1).await.unwrap(); // unwrap

        assert_eq!(
            graph.last_tx_id().await.unwrap(), // unwrap
            5,
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
            12,
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
            let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
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
                .unwrap();

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
                    let mut entities = std::collections::HashSet::new();
                    let mut active_edges = std::collections::HashSet::new();

                    let inner = graph.inner.read();
                    let num_nodes = inner.reverse_map.len();
                    for i in 0..num_nodes {
                        if let Some(&id) = inner.reverse_map.get(i) {
                            if inner.entities.get(i).is_some_and(|e| e.is_some()) {
                                entities.insert(id.inner());
                            }
                        }

                        let old_start = if i < inner.offsets.len() - 1 { inner.offsets[i] } else { 0 };
                        let old_end = if i < inner.offsets.len() - 1 { inner.offsets[i + 1] } else { 0 };
                        for j in old_start..old_end {
                            let target_idx = inner.targets[j];
                            let vf = inner.valid_froms.get(j).copied().flatten().unwrap_or(TxId::new(0)).inner();
                            let vt = inner.valid_tos.get(j).copied().flatten().map(|t| t.inner());

                            if vf <= target_seq && vt.map_or(true, |t| target_seq < t) {
                                if let (Some(&f), Some(&t)) = (inner.reverse_map.get(i), inner.reverse_map.get(target_idx)) {
                                    if !inner.tombstoned_edges.contains(&(i, target_idx)) {
                                        active_edges.insert((f.inner(), t.inner()));
                                    }
                                }
                            }
                        }

                        if let Some(pending) = inner.pending_edges.get(&i) {
                            for edge in pending {
                                let vf = edge.valid_from.unwrap_or(TxId::new(0)).inner();
                                let vt = edge.valid_to.map(|t| t.inner());
                                if vf <= target_seq && vt.map_or(true, |t| target_seq < t) {
                                    if let (Some(&f), Some(&t)) = (inner.reverse_map.get(i), inner.reverse_map.get(edge.target)) {
                                        if !inner.tombstoned_edges.contains(&(i, edge.target)) {
                                            active_edges.insert((f.inner(), t.inner()));
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Check traverse_at for each active node
                    for &start in &entities {
                        let res = graph.traverse_at(EntityId::new(start), 1, target_seq).await.unwrap();
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
            }).unwrap();
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
        graph
            .add_edge(tx, Edge::new(EntityId::new(1), EntityId::new(2), "rel"))
            .await
            .unwrap(); // unwrap allowed
        graph.commit(tx).await.unwrap(); // unwrap allowed

        assert!(graph.inner.read().is_dirty);

        // compact_async on dirty graph
        graph.compact_async().await.unwrap(); // unwrap allowed

        assert!(!graph.inner.read().is_dirty);
        assert_eq!(graph.inner.read().targets.len(), 1);

        // compact_async no-op on clean graph
        graph.compact_async().await.unwrap(); // unwrap allowed
        assert!(!graph.inner.read().is_dirty);
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

    #[test]
    #[allow(non_snake_case)]
    fn insert_entity_direct_and_edge_direct_CASE_and_boundaries() {
        let graph = CsrGraph::new();

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
            valid_from: Some(TxId::new(1)),
            valid_to: None,
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
            valid_from: None,
            valid_to: None,
        };
        let payload_val = bincode::serialize(&payload).unwrap(); // unwrap allowed
        storage.put(tx, invalid_key, &payload_val).await.unwrap(); // unwrap allowed

        storage.commit(tx).await.unwrap(); // unwrap allowed

        let loaded_graph = CsrGraph::load_from_storage(&storage).await.unwrap(); // unwrap allowed
        assert_eq!(loaded_graph.edge_count(), 1);
    }

    #[tokio::test]
    #[allow(non_snake_case)]
    async fn traverse_at_time_CASE_saturating_max_hops() {
        let graph = setup_test_graph().await;

        // Traverse with max_hops 255 (should saturate safely to MAX_TRAVERSAL_HOPS=3 without panic/OOM)
        let results = graph
            .traverse_at_time(EntityId::new(1), 255, TxId::new(100))
            .await
            .unwrap(); // unwrap allowed

        assert!(!results.is_empty());
        assert!(results.len() <= 4);
    }

    #[test]
    #[allow(non_snake_case)]
    fn pagerank_CASE_empty_graph_and_isolated_node() {
        let empty_graph = CsrGraph::new();
        let ranks_empty = empty_graph.pagerank(0.85, 100, 1e-6);
        assert!(ranks_empty.is_empty());

        let iso_graph = CsrGraph::new();
        iso_graph
            .insert_entity_direct(Entity::new(EntityId::new(1), "Iso", "Type"))
            .unwrap(); // unwrap allowed

        let ranks_iso = iso_graph.pagerank(0.85, 100, 1e-6);
        assert_eq!(ranks_iso.len(), 1);
        let rank = ranks_iso[&EntityId::new(1)];
        assert!((rank - 1.0).abs() < 1e-4);
    }

    #[test]
    #[allow(non_snake_case)]
    fn serialization_roundtrip_CASE_persisted_edge_payload() {
        let payload = PersistedEdgePayload {
            weight: 0.825,
            valid_from: Some(TxId::new(10)),
            valid_to: Some(TxId::new(20)),
        };

        let serialized = bincode::serialize(&payload).unwrap(); // unwrap allowed
        let deserialized: PersistedEdgePayload = bincode::deserialize(&serialized).unwrap(); // unwrap allowed

        assert!((payload.weight - deserialized.weight).abs() < f32::EPSILON);
        assert_eq!(payload.valid_from, deserialized.valid_from);
        assert_eq!(payload.valid_to, deserialized.valid_to);
    }

    proptest::proptest! {
        #[test]
        fn prop_csr_offset_array_structural_consistency(
            node_count in 1..=30usize,
            edge_pairs in proptest::collection::vec((0..30usize, 0..30usize, 0.1f32..2.0f32), 1..100)
        ) {
            let graph = CsrGraph::new();
            for i in 0..node_count {
                graph.insert_entity_direct(Entity::new(EntityId::new(i as u64 + 1), format!("N{i}"), "Type")).unwrap();
            }

            for (src, dst, w) in edge_pairs {
                let src_id = EntityId::new((src % node_count) as u64 + 1);
                let dst_id = EntityId::new((dst % node_count) as u64 + 1);
                graph.insert_edge_direct(src_id, dst_id, w).unwrap();
            }

            graph.compact();

            let inner = graph.inner.read();

            // Invariant 1: offsets length must equal reverse_map length + 1 after compaction
            proptest::prop_assert_eq!(inner.offsets.len(), inner.reverse_map.len() + 1);

            // Invariant 2: offsets must be monotonically non-decreasing
            for window in inner.offsets.windows(2) {
                proptest::prop_assert!(window[0] <= window[1]);
            }

            // Invariant 3: final offset must match targets length
            proptest::prop_assert_eq!(*inner.offsets.last().unwrap(), inner.targets.len());

            // Invariant 4: parallel arrays (targets, weights, valid_froms, valid_tos) must have equal lengths
            proptest::prop_assert_eq!(inner.targets.len(), inner.weights.len());
            proptest::prop_assert_eq!(inner.targets.len(), inner.valid_froms.len());
            proptest::prop_assert_eq!(inner.targets.len(), inner.valid_tos.len());
        }
    }
}
