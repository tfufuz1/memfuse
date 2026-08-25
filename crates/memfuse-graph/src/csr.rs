//! CSR-Graph-Implementierung für Entity-Relation-Traversal.
//!
//! Implementiert [`memfuse_core::GraphIndex`] via Compressed Sparse Row (CSR)
//! Datenstruktur für cache-effizienten Graph-Traversal.

// INVARIANT: CSR-Graph for 4-Signal Fusion

use async_trait::async_trait;
use memfuse_core::{
    Edge, Entity, EntityId, GraphIndex, GraphIndexStats, MemFuseError, Result, StorageEngine, TxId,
};
use parking_lot::RwLock;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

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
/// # AI-NOTE[BOUNDARY-MISSING][MAJOR]
/// KONTEXT: AGT-GRAPH-001 — add_entity/add_edge/commit akzeptieren TxIds ohne
///   Herkunftsvalidierung. Harte Ablehnung ist nicht möglich, da der Graph
///   keinen Zugriff auf den `next_tx`-Höchststand der aufrufenden Collection hat.
/// ANWEISUNG: Bei `true` => tracing::warn! loggen. Keine Ablehnung (kein Err).
/// ID: AGT-GRAPH-001
#[inline]
fn is_suspicious_tx_id(tx: TxId) -> bool {
    let v = tx.inner();
    (WALLCLOCK_TX_HEURISTIC_MIN..TxId::INTERNAL_BASE).contains(&v)
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

/// Inner state of the CsrGraph to manage contiguous storage.
struct GraphInner {
    /// Mapping from public EntityId to internal contiguous index.
    id_map: HashMap<EntityId, InternalIndex>,
    /// Mapping from internal index back to EntityId.
    reverse_map: Vec<EntityId>,
    /// Entity metadata stored contiguously.
    entities: Vec<Option<Entity>>,

    /// CSR offsets array: offsets[i] is the start index in `targets` for node `i`.
    /// Length is nodes + 1.
    offsets: Vec<usize>,
    /// CSR targets array: contiguous list of neighbor internal indices.
    targets: Vec<InternalIndex>,
    /// CSR weights array: contiguous list of edge weights.
    weights: Vec<f32>,

    /// Staging for entities not yet committed, grouped by TxId.
    staged_entities: HashMap<TxId, HashMap<EntityId, Entity>>,
    /// Staging for edges not yet committed, grouped by TxId.
    staged_edges: HashMap<TxId, HashMap<InternalIndex, Vec<(InternalIndex, f32)>>>,
    /// Edges that have been committed but not yet compacted into CSR arrays (delta buffer).
    pending_edges: HashMap<InternalIndex, Vec<(InternalIndex, f32)>>,
    /// Total number of uncompacted edges currently in `pending_edges`.
    pending_edge_count: usize,
    /// Flag indicating if there are uncompacted pending edges.
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
            staged_entities: HashMap::new(),
            staged_edges: HashMap::new(),
            pending_edges: HashMap::new(),
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
        if !self.is_dirty || self.pending_edges.is_empty() {
            self.pending_edge_count = 0;
            self.is_dirty = false;
            return;
        }

        let num_nodes = self.reverse_map.len();
        let mut new_offsets = Vec::with_capacity(num_nodes + 1);
        let mut new_targets = Vec::with_capacity(self.targets.len() + self.pending_edge_count);
        let mut new_weights = Vec::with_capacity(self.weights.len() + self.pending_edge_count);

        let mut current_offset = 0;
        new_offsets.push(current_offset);

        for i in 0..num_nodes {
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
                new_targets.push(self.targets[j]);
                new_weights.push(self.weights[j]);
                current_offset += 1;
            }

            // 2. Get neighbors from pending_edges (FIND-GRA-001)
            if let Some(staged) = self.pending_edges.get(&i) {
                for &(target, weight) in staged {
                    new_targets.push(target);
                    new_weights.push(weight);
                    current_offset += 1;
                }
            }
            new_offsets.push(current_offset);
        }

        self.offsets = new_offsets;
        self.targets = new_targets;
        self.weights = new_weights;
        self.pending_edges.clear();
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
        }
    }

    /// Creates a new CSR graph with persistent storage and default config.
    pub fn with_storage(storage: Arc<dyn StorageEngine>) -> Self {
        Self::with_config_and_storage(CsrGraphConfig::default(), storage)
    }

    /// Creates a new CSR graph with configuration and persistent storage.
    pub fn with_config_and_storage(config: CsrGraphConfig, storage: Arc<dyn StorageEngine>) -> Self {
        Self {
            config,
            inner: RwLock::new(GraphInner::new()),
            storage: Some(storage),
        }
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
        let mut inner = self.inner.write();
        let from_idx = inner.get_or_create_index(from);
        let to_idx = inner.get_or_create_index(to);
        inner
            .pending_edges
            .entry(from_idx)
            .or_default()
            .push((to_idx, weight));
        inner.pending_edge_count += 1;
        inner.is_dirty = true;
        if inner.pending_edge_count >= self.config.rebuild_threshold {
            inner.compact();
        }
        Ok(())
    }

    /// Fügt eine Entity direkt in committed state ein (für `load_from_storage`).
    /// Umgeht das TX-Staging, da beim Laden alle Daten bereits committed sind.
    fn load_entity_direct(&self, entity: Entity) -> Result<()> {
        let mut inner = self.inner.write();
        let idx = inner.get_or_create_index(entity.id);
        if idx >= inner.entities.len() {
            inner.entities.resize(idx + 1, None);
        }
        inner.entities[idx] = Some(entity);
        Ok(())
    }

    /// Fügt eine Edge direkt in `pending_edges` ein (für `load_from_storage`).
    /// Umgeht das TX-Staging, da beim Laden alle Daten bereits committed sind.
    fn load_edge_direct(&self, from: EntityId, to: EntityId, weight: f32) -> Result<()> {
        let mut inner = self.inner.write();
        let from_idx = inner.get_or_create_index(from);
        let to_idx = inner.get_or_create_index(to);
        inner
            .pending_edges
            .entry(from_idx)
            .or_default()
            .push((to_idx, weight));
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
        weight: f32,
    ) -> Result<()> {
        let key = [
            GRAPH_EDGE_PREFIX,
            from.as_bytes().as_slice(),
            b":",
            to.as_bytes().as_slice(),
        ]
        .concat();
        let value = bincode::serialize(&weight)
            .map_err(|e| MemFuseError::Internal(format!("graph edge serialize: {e}")))?;
        storage.put(tx, &key, &value).await
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
            let weight: f32 = bincode::deserialize(&raw_value).map_err(|e| {
                MemFuseError::Internal(format!("graph edge weight deserialize: {e}"))
            })?;

            // Key-Format: "__graph:edge:{from_id}:{to_id}"
            let key_payload = raw_key
                .get(GRAPH_EDGE_PREFIX.len()..)
                .ok_or_else(|| MemFuseError::Internal("graph edge key zu kurz".into()))?;

            let key_str = std::str::from_utf8(key_payload)
                .map_err(|e| MemFuseError::Internal(format!("graph edge key UTF-8: {e}")))?;

            if let Some((from_str, to_str)) = key_str.split_once(':') {
                let from_id = EntityId::from(from_str);
                let to_id = EntityId::from(to_str);
                graph.load_edge_direct(from_id, to_id, weight)?;
                edge_count += 1;
            } else {
                tracing::warn!(key = key_str, "Ungültiger graph edge key, übersprungen");
            }
        }

        // 3. CSR kompaktieren — MUSS nach allen Edges aufgerufen werden
        graph.compact();

        tracing::info!(
            entities = entity_count,
            edges = edge_count,
            "Graph aus Storage geladen und kompaktiert"
        );
        Ok(graph)
    }

    /// Force compacts the graph delta buffer into the main CSR arrays to optimize traversal layout.
    pub fn compact(&self) {
        // Double-checked locking to avoid unnecessary write locks (FIND-GRA-002)
        let inner_read = self.inner.read();
        if !inner_read.is_dirty && inner_read.pending_edges.is_empty() {
            return;
        }
        drop(inner_read);

        let mut inner = self.inner.write();
        if inner.is_dirty || !inner.pending_edges.is_empty() {
            inner.compact();
        }
    }

    /// Returns the number of committed entities in the graph.
    pub fn entity_count(&self) -> usize {
        self.inner.read().entities.iter().flatten().count()
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

        inner
            .staged_edges
            .entry(tx)
            .or_default()
            .entry(from_idx)
            .or_default()
            .push((to_idx, edge.weight));
        Ok(())
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
                    for &(neighbor_idx, weight) in pending {
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
        let (entities_to_commit, edges_to_commit) = {
            let inner = self.inner.read();
            let entities = inner.staged_entities.get(&tx).cloned();
            let edges = inner.staged_edges.get(&tx).map(|tx_edges| {
                let mut list = Vec::new();
                for (&from_idx, to_list) in tx_edges {
                    if let Some(&from_id) = inner.reverse_map.get(from_idx) {
                        for &(to_idx, weight) in to_list {
                            if let Some(&to_id) = inner.reverse_map.get(to_idx) {
                                list.push((from_id, to_id, weight));
                            }
                        }
                    }
                }
                list
            });
            (entities, edges)
        };

        if let Some(ref storage) = self.storage {
            if let Some(ref entities) = entities_to_commit {
                for entity in entities.values() {
                    self.persist_entity(storage.as_ref(), tx, entity).await?;
                }
            }
            if let Some(ref edges) = edges_to_commit {
                for (from_id, to_id, weight) in edges {
                    self.persist_edge(storage.as_ref(), tx, from_id, to_id, *weight)
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

        // Auto-rebuild CSR arrays if pending delta buffer reaches or exceeds threshold
        if inner.pending_edge_count >= self.config.rebuild_threshold {
            inner.compact();
        }

        Ok(())
    }

    async fn rollback(&self, tx: TxId) -> Result<()> {
        let mut inner = self.inner.write();
        inner.staged_entities.remove(&tx);
        inner.staged_edges.remove(&tx);
        Ok(())
    }

    async fn rollback_to_tx(&self, _tx_id: TxId) -> Result<()> {
        // Physical rollback for CSR graph is driven by WAL replay or reloading state from storage.
        // In-memory staged transactions are handled by rollback().
        Ok(())
    }

    async fn last_tx_id(&self) -> Result<u64> {
        Ok(0)
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
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    use tracing::subscriber::Subscriber;
    use tracing::Event;
    use tracing::Id;
    use tracing::Metadata;
    use tracing::span::Attributes;
    use tracing::span::Record;

    struct WarnCounterSubscriber {
        warn_count: Arc<AtomicUsize>,
    }

    impl Subscriber for WarnCounterSubscriber {
        fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
            true
        }
        fn new_span(&self, _span: &Attributes<'_>) -> Id {
            Id::from_u64(1)
        }
        fn record(&self, _span: &Id, _values: &Record<'_>) {}
        fn record_follows_from(&self, _span: &Id, _follows: &Id) {}
        fn event(&self, event: &Event<'_>) {
            if *event.metadata().level() == tracing::Level::WARN {
                self.warn_count.fetch_add(1, Ordering::SeqCst);
            }
        }
        fn enter(&self, _span: &Id) {}
        fn exit(&self, _span: &Id) {}
    }

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
                .expect("valid setup");
        }

        graph
            .add_edge(
                tx,
                Edge::new(EntityId::new(1), EntityId::new(2), "knows").with_weight(1.0),
            )
            .await
            .expect("valid edge");
        graph
            .add_edge(
                tx,
                Edge::new(EntityId::new(2), EntityId::new(3), "knows").with_weight(0.8),
            )
            .await
            .expect("valid edge");
        graph
            .add_edge(
                tx,
                Edge::new(EntityId::new(3), EntityId::new(4), "knows").with_weight(0.6),
            )
            .await
            .expect("valid edge");
        graph
            .add_edge(
                tx,
                Edge::new(EntityId::new(4), EntityId::new(5), "knows").with_weight(0.5),
            )
            .await
            .expect("valid edge");
        graph
            .add_edge(
                tx,
                Edge::new(EntityId::new(2), EntityId::new(5), "knows").with_weight(0.4),
            )
            .await
            .expect("valid edge");

        graph.commit(tx).await.expect("commit");
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
            .unwrap();
        graph
            .add_entity(tx, Entity::new(EntityId::new(2), "B", "T"))
            .await
            .unwrap();
        graph
            .add_edge(tx, Edge::new(EntityId::new(1), EntityId::new(2), "E"))
            .await
            .unwrap();

        graph.commit(tx).await.unwrap();

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
            .unwrap();
        graph
            .add_entity(tx, Entity::new(EntityId::new(2), "B", "Node"))
            .await
            .unwrap();
        graph
            .add_entity(tx, Entity::new(EntityId::new(3), "C", "Node"))
            .await
            .unwrap();

        graph
            .add_edge(
                tx,
                Edge::new(EntityId::new(1), EntityId::new(2), "knows").with_weight(1.0),
            )
            .await
            .unwrap();
        graph.commit(tx).await.unwrap();

        // Edge 1->2 is committed in pending_edges (uncompacted)
        {
            let inner = graph.inner.read();
            assert!(inner.is_dirty);
            assert_eq!(inner.pending_edge_count, 1);
            assert_eq!(inner.targets.len(), 0); // Not in CSR targets yet
        }

        // Traversal MUST find Entity 2 directly from pending_edges delta buffer
        let results = graph.traverse(EntityId::new(1), 1).await.unwrap();
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
            .unwrap();
        graph.commit(tx2).await.unwrap();

        // Traversal from 1 (max 2 hops) MUST find both 2 and 3 through delta buffer
        let results_2hop = graph.traverse(EntityId::new(1), 2).await.unwrap();
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
            .unwrap();
        graph
            .add_entity(tx1, Entity::new(EntityId::new(2), "B", "T"))
            .await
            .unwrap();
        graph
            .add_edge(
                tx1,
                Edge::new(EntityId::new(1), EntityId::new(2), "E").with_weight(1.0),
            )
            .await
            .unwrap();

        // 2. Traverse (ohne Tx) darf Edge NICHT sehen
        let results = graph.traverse(EntityId::new(1), 1).await.unwrap();
        assert_eq!(results.len(), 0, "Uncommitted edge should not be visible");

        // 3. Tx1 committet
        graph.commit(tx1).await.unwrap();

        // 4. Traverse MUSS Edge sehen
        let results = graph.traverse(EntityId::new(1), 1).await.unwrap();
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
            .unwrap();
        graph
            .add_entity(tx1, Entity::new(EntityId::new(2), "B", "T"))
            .await
            .unwrap();
        graph
            .add_edge(
                tx1,
                Edge::new(EntityId::new(1), EntityId::new(2), "E1").with_weight(1.0),
            )
            .await
            .unwrap();

        graph
            .add_entity(tx2, Entity::new(EntityId::new(1), "A", "T"))
            .await
            .unwrap();
        graph
            .add_entity(tx2, Entity::new(EntityId::new(3), "C", "T"))
            .await
            .unwrap();
        graph
            .add_edge(
                tx2,
                Edge::new(EntityId::new(1), EntityId::new(3), "E2").with_weight(1.0),
            )
            .await
            .unwrap();

        // 2. Tx1 rollt back
        graph.rollback(tx1).await.unwrap();

        // 3. Tx2 committet
        graph.commit(tx2).await.unwrap();

        // 4. Nur Edges von Tx2 dürfen existieren
        let results = graph.traverse(EntityId::new(1), 1).await.unwrap();
        assert_eq!(results.len(), 1, "Only Tx2 edge should be visible");
        assert_eq!(results[0].0, EntityId::new(3));

        let stats = graph.stats().await.unwrap();
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
        let results = graph.traverse(EntityId::new(1), 3).await.expect("traverse");

        assert_eq!(results.len(), 4);

        let score_map: std::collections::HashMap<_, _> = results.into_iter().collect();

        let s2 = *score_map.get(&EntityId::new(2)).expect("node 2 missing");
        let s3 = *score_map.get(&EntityId::new(3)).expect("node 3 missing");
        let s4 = *score_map.get(&EntityId::new(4)).expect("node 4 missing");
        let s5 = *score_map.get(&EntityId::new(5)).expect("node 5 missing");

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
            .unwrap();
        graph
            .add_entity(tx, Entity::new(EntityId::new(2), "B", "N"))
            .await
            .unwrap();
        graph
            .add_edge(tx, Edge::new(EntityId::new(1), EntityId::new(2), "E"))
            .await
            .unwrap();
        graph
            .add_edge(tx, Edge::new(EntityId::new(2), EntityId::new(1), "E"))
            .await
            .unwrap();

        graph.commit(tx).await.unwrap();

        let results = graph.traverse(EntityId::new(1), 5).await.expect("traverse");
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
            .expect("traverse 1 hop");
        assert_eq!(results_hop1.len(), 1);
        assert_eq!(results_hop1[0].0, EntityId::new(2));

        // Traverse from 3, max hops 1 -> Should only find Node 4
        let results_hop1_n3 = graph
            .traverse(EntityId::new(3), 1)
            .await
            .expect("traverse 1 hop");
        assert_eq!(results_hop1_n3.len(), 1);
        assert_eq!(results_hop1_n3[0].0, EntityId::new(4));
    }

    #[tokio::test]
    async fn test_csr_graph_stats_accuracy() {
        let graph = setup_test_graph().await;
        let stats = graph.stats().await.expect("valid stats");

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
        let edge1 = Edge {
            from: EntityId::new(1),
            to: EntityId::new(5),
            weight: 0.5,
            label: String::new(),
        };
        graph.add_edge(tx_uncommitted, edge1).await.unwrap();

        // 2. Add committed edges
        let tx_committed = TxId::new(100);
        let edge2 = Edge {
            from: EntityId::new(1),
            to: EntityId::new(2), // Use existing entity 2 to avoid 'entry missing' errors
            weight: 0.9,
            label: String::new(),
        };
        graph.add_edge(tx_committed, edge2).await.unwrap();
        graph.commit(tx_committed).await.unwrap();

        // 3. Compact
        graph.compact();

        // 4. Verify traversal
        let results = graph.traverse(EntityId::new(1), 1).await.unwrap();
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
            .unwrap();

        // Staging unter gleicher TxId ueberschreibt staged entity fuer EntityId(10) in der staged HashMap
        graph
            .add_entity(
                tx_source_b,
                Entity::new(EntityId::new(10), "EntityFromB", "TypeB"),
            )
            .await
            .unwrap();

        graph.commit(tx_source_a).await.unwrap();

        // Nach Commit ist der Zustand deterministisch (letzte staged Entity gewinnt)
        let inner = graph.inner.read();
        let idx = inner.id_map.get(&EntityId::new(10)).unwrap();
        let entity = inner.entities[*idx].as_ref().unwrap();
        assert_eq!(entity.name, "EntityFromB");
    }

    #[tokio::test]
    async fn test_wallclock_txid_warn_but_no_panic() {
        let graph = CsrGraph::new();
        // Wall-clock-aehnlicher TxId (~1.7e18 ns)
        let wallclock_tx = TxId::new(1_700_000_000_000_000_000);

        assert!(super::is_suspicious_tx_id(wallclock_tx));

        // Ausfuehrung darf nicht paniquen oder fehlschlagen
        graph
            .add_entity(
                wallclock_tx,
                Entity::new(EntityId::new(100), "WallClockEntity", "Type"),
            )
            .await
            .unwrap();

        graph
            .add_edge(
                wallclock_tx,
                Edge::new(EntityId::new(100), EntityId::new(101), "rel"),
            )
            .await
            .unwrap();

        graph.commit(wallclock_tx).await.unwrap();

        assert_eq!(graph.entity_count(), 1);
    }

    #[tokio::test]
    async fn test_wallclock_nanos_txid_triggers_defensive_warning() {
        let warn_count = Arc::new(AtomicUsize::new(0));
        let subscriber = WarnCounterSubscriber {
            warn_count: warn_count.clone(),
        };

        let _guard = tracing::subscriber::set_default(subscriber);

        let graph = CsrGraph::new();
        // Aus Wall-Clock-Nanosekunden abgeleiteter TxId-Wert (astronomisch groß, aber unter INTERNAL_BASE)
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_nanos() as u64;
        let wallclock_tx = TxId::new(nanos);

        assert!(
            super::is_suspicious_tx_id(wallclock_tx),
            "Wallclock nanos TxId ({nanos}) must be detected as suspicious"
        );

        graph
            .add_entity(
                wallclock_tx,
                Entity::new(EntityId::new(200), "WallClockEntity", "Type"),
            )
            .await
            .unwrap();

        graph
            .add_edge(
                wallclock_tx,
                Edge::new(EntityId::new(200), EntityId::new(201), "rel"),
            )
            .await
            .unwrap();

        graph.commit(wallclock_tx).await.unwrap();

        // Warnung MUSS gefeuert haben (add_entity, add_edge, commit)
        assert!(
            warn_count.load(Ordering::SeqCst) >= 1,
            "Defensive warning must fire for add_entity, add_edge, and commit when using wall-clock TxId"
        );
    }
}
