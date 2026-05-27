//! Compressed Sparse Row (CSR) Graph implementation.
//!
//! Memory-efficient graph storage for entity-relation traversal.
//! BFS with score-decay: `score * 0.7^hop` (max 3 hops).

// ANCHOR:ARCH:CSR-001 — CSR-Graph for 4-Signal Fusion
// WP:WP-6.1 PRIO:2 NEEDS:WP-2.1
// STATUS:IMPLEMENTED DATE:2026-05-27
// DESIGN: CSR structure with contiguous arrays for traversal efficiency.

use memfuse_core::{Edge, Entity, EntityId, GraphIndex, GraphIndexStats, Result, TxId};
use parking_lot::RwLock;
use std::collections::{HashMap, VecDeque};

/// Score decay factor per hop (0.7^hop).
const SCORE_DECAY: f32 = 0.7;

/// Maximum traversal depth.
const MAX_TRAVERSAL_HOPS: u8 = 3;

/// Internal contiguous index for CSR arrays.
type InternalIndex = usize;

/// Inner state of the CsrGraph to manage contiguous storage.
struct GraphInner {
    /// Mapping from public EntityId to internal contiguous index.
    id_map: HashMap<EntityId, InternalIndex>,
    /// Mapping from internal index back to EntityId.
    reverse_map: Vec<EntityId>,
    /// Entity metadata stored contiguously.
    entities: Vec<Entity>,

    /// CSR offsets array: offsets[i] is the start index in `targets` for node `i`.
    /// Length is nodes + 1.
    offsets: Vec<usize>,
    /// CSR targets array: contiguous list of neighbor internal indices.
    targets: Vec<InternalIndex>,
    /// CSR weights array: contiguous list of edge weights.
    weights: Vec<f32>,

    /// Staging for edges not yet compacted into CSR arrays.
    staged_edges: HashMap<InternalIndex, Vec<(InternalIndex, f32)>>,
    /// Flag indicating if the CSR arrays are up to date with staged edges.
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
            staged_edges: HashMap::new(),
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

    /// Compacts staged edges into the CSR arrays.
    fn compact(&mut self) {
        if !self.is_dirty {
            return;
        }

        let num_nodes = self.reverse_map.len();
        let mut new_offsets = Vec::with_capacity(num_nodes + 1);
        let mut new_targets = Vec::new();
        let mut new_weights = Vec::new();

        let mut current_offset = 0;
        new_offsets.push(current_offset);

        for i in 0..num_nodes {
            // Combine existing CSR edges (if any) and staged edges
            // Note: In this simple implementation, we just rebuild from scratch
            // for simplicity, or we could merge.
            // For now, let's assume we rebuild from the staged + old CSR.

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

            // 2. Get neighbors from staged
            if let Some(staged) = self.staged_edges.get(&i) {
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
        self.staged_edges.clear();
        self.is_dirty = false;
    }
}

/// Compressed Sparse Row graph for entity-relation traversal.
///
/// Implements `GraphIndex` trait as Signal 3 in the 4-Signal Fusion architecture.
pub struct CsrGraph {
    inner: RwLock<GraphInner>,
}

impl CsrGraph {
    /// Creates a new, empty CSR graph.
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(GraphInner::new()),
        }
    }

    /// Compacts the graph to optimize for traversal.
    // TODO(FIND-GRA-001): Transaction Isolation Leak
    // Compaction serialisiert fälschlicherweise auch uncommitted Kanten,
    // was die LSM MVCC Isolationsgarantien verletzt.
    pub fn compact(&self) {
        self.inner.write().compact();
    }

    /// Returns the number of entities in the graph.
    pub fn entity_count(&self) -> usize {
        self.inner.read().reverse_map.len()
    }

    /// Returns the number of edges in the graph.
    pub fn edge_count(&self) -> usize {
        let inner = self.inner.read();
        inner.targets.len() + inner.staged_edges.values().map(|v| v.len()).sum::<usize>()
    }
}

impl Default for CsrGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphIndex for CsrGraph {
    async fn add_entity(&self, _tx: TxId, entity: Entity) -> Result<()> {
        let mut inner = self.inner.write();
        let idx = inner.get_or_create_index(entity.id);

        if idx >= inner.entities.len() {
            inner
                .entities
                .resize(idx + 1, Entity::new(EntityId::new(0), "", ""));
        }
        inner.entities[idx] = entity;
        Ok(())
    }

    async fn add_edge(&self, _tx: TxId, edge: Edge) -> Result<()> {
        let mut inner = self.inner.write();
        let from_idx = inner.get_or_create_index(edge.from);
        let to_idx = inner.get_or_create_index(edge.to);

        inner
            .staged_edges
            .entry(from_idx)
            .or_default()
            .push((to_idx, edge.weight));
        inner.is_dirty = true;
        Ok(())
    }

    async fn traverse(&self, start: EntityId, max_hops: usize) -> Result<Vec<(EntityId, f32)>> {
        // Ensure graph is compacted for traversal
        self.compact();

        let inner = self.inner.read();
        let start_idx = match inner.id_map.get(&start) {
            Some(&idx) => idx,
            None => return Ok(Vec::new()), // Start node not in graph
        };

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
                // CSR traversal
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

    async fn commit(&self, _tx: TxId) -> Result<()> {
        self.compact();
        Ok(())
    }

    async fn rollback(&self, _tx: TxId) -> Result<()> {
        let mut inner = self.inner.write();
        inner.staged_edges.clear();
        inner.is_dirty = false;
        Ok(())
    }

    async fn stats(&self) -> Result<GraphIndexStats> {
        let inner = self.inner.read();
        let num_entities = inner.reverse_map.len();
        let num_edges =
            inner.targets.len() + inner.staged_edges.values().map(|v| v.len()).sum::<usize>();

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

        {
            let inner = graph.inner.read();
            assert!(inner.is_dirty);
            assert_eq!(inner.staged_edges.len(), 1);
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
            + (inner.entities.len() * std::mem::size_of::<Entity>())
            + (inner.offsets.len() * std::mem::size_of::<usize>())
            + (inner.targets.len() * std::mem::size_of::<usize>())
            + (inner.weights.len() * std::mem::size_of::<f32>());

        assert_eq!(stats.memory_usage_bytes, expected_mem);
    }
}
