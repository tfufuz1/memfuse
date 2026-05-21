//! Compressed Sparse Row (CSR) Graph implementation.
//!
//! Memory-efficient graph storage for entity-relation traversal.
//! BFS with score-decay: `score * 0.7^hop` (max 3 hops).

// ANCHOR:ARCH:CSR-001 — CSR-Graph for 4-Signal Fusion
// WP:WP-6.1 PRIO:2 NEEDS:WP-2.1
// STATUS:SCAFFOLD DATE:2026-05-17
// DESIGN: HashMap-based adjacency during build, CSR-compact for traversal.

use async_trait::async_trait;
use memfuse_core::{Edge, Entity, EntityId, GraphIndex, GraphIndexStats, Result, TxId};
use parking_lot::RwLock;
use std::collections::HashMap;

/// Score decay factor per hop (0.7^hop).
const SCORE_DECAY: f32 = 0.7;

/// Maximum traversal depth.
const MAX_TRAVERSAL_HOPS: u8 = 3;

/// Compressed Sparse Row graph for entity-relation traversal.
///
/// Implements `GraphIndex` trait as Signal 3 in the 4-Signal Fusion architecture.
pub struct CsrGraph {
    /// Entity storage: EntityId -> Entity.
    entities: RwLock<HashMap<u64, Entity>>,
    /// Adjacency list: source EntityId -> Vec<(target EntityId, weight)>.
    adjacency: RwLock<HashMap<u64, Vec<(u64, f32)>>>,
}

impl CsrGraph {
    /// Creates a new, empty CSR graph.
    pub fn new() -> Self {
        Self {
            entities: RwLock::new(HashMap::new()),
            adjacency: RwLock::new(HashMap::new()),
        }
    }

    /// Returns the number of entities in the graph.
    pub fn entity_count(&self) -> usize {
        self.entities.read().len()
    }

    /// Returns the number of edges in the graph.
    pub fn edge_count(&self) -> usize {
        self.adjacency.read().values().map(|v| v.len()).sum()
    }
}

impl Default for CsrGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl GraphIndex for CsrGraph {
    async fn add_entity(&self, _tx: TxId, entity: Entity) -> Result<()> {
        self.entities.write().insert(entity.id.inner(), entity);
        Ok(())
    }

    async fn add_edge(&self, _tx: TxId, edge: Edge) -> Result<()> {
        self.adjacency
            .write()
            .entry(edge.from.inner())
            .or_default()
            .push((edge.to.inner(), edge.weight));
        Ok(())
    }

    async fn traverse(&self, start: EntityId, max_hops: usize) -> Result<Vec<(EntityId, f32)>> {
        let effective_max = (max_hops as u8).min(MAX_TRAVERSAL_HOPS);
        let adj = self.adjacency.read();

        // BFS with score decay
        let mut visited: HashMap<u64, f32> = HashMap::new();
        let mut queue: std::collections::VecDeque<(u64, u8, f32)> =
            std::collections::VecDeque::new();

        queue.push_back((start.inner(), 0, 1.0));

        while let Some((node_id, hop, current_score)) = queue.pop_front() {
            if hop > effective_max {
                continue;
            }

            // Only keep the best score per node
            let existing = visited.entry(node_id).or_insert(0.0);
            if current_score > *existing {
                *existing = current_score;
            }

            if hop < effective_max {
                if let Some(neighbors) = adj.get(&node_id) {
                    for &(neighbor_id, weight) in neighbors {
                        let next_score = current_score * SCORE_DECAY * weight;
                        if !visited.contains_key(&neighbor_id) || visited[&neighbor_id] < next_score
                        {
                            queue.push_back((neighbor_id, hop + 1, next_score));
                        }
                    }
                }
            }
        }

        // Remove the start node from results
        visited.remove(&start.inner());

        let mut results: Vec<(EntityId, f32)> = visited
            .into_iter()
            .map(|(id, score)| (EntityId::new(id), score))
            .collect();

        // Sort by score descending
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        Ok(results)
    }

    async fn commit(&self, _tx: TxId) -> Result<()> {
        // TODO(WP-6.1): Implement WAL-backed transaction commit for graph mutations.
        Ok(())
    }

    async fn rollback(&self, _tx: TxId) -> Result<()> {
        // TODO(WP-6.1): Implement transaction rollback for graph mutations.
        Ok(())
    }

    async fn stats(&self) -> Result<GraphIndexStats> {
        let entities = self.entities.read();
        let adjacency = self.adjacency.read();
        let num_edges: usize = adjacency.values().map(|v| v.len()).sum();
        let mem = (entities.len() * std::mem::size_of::<Entity>())
            + (num_edges * std::mem::size_of::<(u64, f32)>());

        Ok(GraphIndexStats {
            num_entities: entities.len(),
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

        // Define Entities
        // 1: Alice, 2: Bob, 3: Charlie, 4: Dave, 5: Eve
        for id in 1..=5 {
            graph
                .add_entity(
                    tx,
                    Entity::new(EntityId::new(id), format!("P{}", id), "Person"),
                )
                .await
                .expect("valid setup");
        }

        // Define Edges (Weights)
        // 1 -> 2 (1.0), 2 -> 3 (0.8), 3 -> 4 (0.6), 4 -> 5 (0.5), 2 -> 5 (0.4)
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

        graph
    }

    #[tokio::test]
    async fn test_csr_graph_scaffold_compiles() {
        let graph = CsrGraph::new();

        let tx = TxId::new(1);
        graph
            .add_entity(tx, Entity::new(EntityId::new(1), "Alice", "Person"))
            .await
            .expect("valid test value");
        graph
            .add_entity(tx, Entity::new(EntityId::new(2), "Bob", "Person"))
            .await
            .expect("valid test value");
        graph
            .add_edge(tx, Edge::new(EntityId::new(1), EntityId::new(2), "knows"))
            .await
            .expect("valid test value");

        let results = graph
            .traverse(EntityId::new(1), 2)
            .await
            .expect("valid test value");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, EntityId::new(2));
    }

    #[tokio::test]
    async fn test_csr_graph_bfs_score_decay() {
        let graph = setup_test_graph().await;

        // Traverse from 1 with max 3 hops (effectively visits 2, 3, 4, 5)
        let results = graph.traverse(EntityId::new(1), 3).await.expect("traverse");

        // Results should contain 2, 3, 4, 5
        assert_eq!(
            results.len(),
            4,
            "Should visit exactly 4 nodes from 1 in 3 hops"
        );

        // Exact expected scores
        // Node 2 (Hop 1): 1.0 * 0.7 * 1.0 = 0.7
        // Node 3 (Hop 2): Score(2) * 0.7 * 0.8 = 0.7 * 0.7 * 0.8 = 0.392
        // Node 5 (Hop 2 - from 2): Score(2) * 0.7 * 0.4 = 0.7 * 0.7 * 0.4 = 0.196
        // Node 4 (Hop 3 - from 3): Score(3) * 0.7 * 0.6 = 0.392 * 0.7 * 0.6 = 0.16464

        // It's possible to reach 5 from 4 (hop 4) but max is 3 hops.

        // Convert to map for easy lookup
        let score_map: std::collections::HashMap<_, _> = results.into_iter().collect();

        let s2 = *score_map.get(&EntityId::new(2)).expect("node 2 missing");
        let s3 = *score_map.get(&EntityId::new(3)).expect("node 3 missing");
        let s4 = *score_map.get(&EntityId::new(4)).expect("node 4 missing");
        let s5 = *score_map.get(&EntityId::new(5)).expect("node 5 missing");

        assert!((s2 - 0.7).abs() < f32::EPSILON, "Node 2 score off: {}", s2);
        assert!(
            (s3 - 0.392).abs() < f32::EPSILON,
            "Node 3 score off: {}",
            s3
        );
        assert!(
            (s5 - 0.196).abs() < f32::EPSILON,
            "Node 5 score off: {}",
            s5
        );
        assert!(
            (s4 - 0.16464).abs() < f32::EPSILON,
            "Node 4 score off: {}",
            s4
        );
    }

    #[tokio::test]
    async fn test_csr_graph_cycle_handling() {
        let graph = CsrGraph::new();
        let tx = TxId::new(1);

        graph
            .add_entity(tx, Entity::new(EntityId::new(1), "A", "N"))
            .await
            .expect("valid");
        graph
            .add_entity(tx, Entity::new(EntityId::new(2), "B", "N"))
            .await
            .expect("valid");

        // Cyclic relationship: 1 -> 2 -> 1
        graph
            .add_edge(tx, Edge::new(EntityId::new(1), EntityId::new(2), "E"))
            .await
            .expect("valid");
        graph
            .add_edge(tx, Edge::new(EntityId::new(2), EntityId::new(1), "E"))
            .await
            .expect("valid");

        let results = graph.traverse(EntityId::new(1), 5).await.expect("traverse");

        // Start node (1) is explicitly removed from output results according to implementation logic
        assert_eq!(
            results.len(),
            1,
            "Should only output connected distinct nodes (not start)"
        );
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

        let expected_mem =
            (5 * std::mem::size_of::<Entity>()) + (5 * std::mem::size_of::<(u64, f32)>());
        assert_eq!(stats.memory_usage_bytes, expected_mem);
    }
}
