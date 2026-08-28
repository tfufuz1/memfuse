//! Label Propagation Community Detection algorithm for CsrGraph.
//!
//! Provides deterministic, offline graph clustering to assign entities to
//! semantic communities for GraphRAG retrieval.

use crate::CsrGraph;
use memfuse_core::{EntityId, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for Label Propagation Community Detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunityDetectionConfig {
    /// Maximum number of propagation iterations.
    pub max_iterations: u32,
    /// Seed for deterministic node traversal shuffling.
    pub seed: u64,
}

impl Default for CommunityDetectionConfig {
    fn default() -> Self {
        Self {
            max_iterations: 100,
            seed: 42,
        }
    }
}

/// Represents the assignment of an entity to a detected community.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommunityAssignment {
    /// Entity ID.
    pub entity_id: EntityId,
    /// Community ID (represented as a 64-bit integer, initialized from the seed EntityId).
    pub community_id: u64,
}

/// Minimal deterministic PRNG for shuffling node order.
struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 {
                0xda3e_39cb_94b9_5bdb
            } else {
                seed
            },
        }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    fn gen_range(&mut self, bound: usize) -> usize {
        if bound <= 1 {
            return 0;
        }
        (self.next_u64() as usize) % bound
    }

    fn shuffle<T>(&mut self, slice: &mut [T]) {
        for i in (1..slice.len()).rev() {
            let j = self.gen_range(i + 1);
            slice.swap(i, j);
        }
    }
}

/// Detects semantic communities in the given CSR graph using deterministic Label Propagation.
///
/// Tie-breaking rule when multiple community labels have equal aggregate weight:
/// The label with the smallest `u64` numerical value (smallest EntityId) wins.
pub async fn detect_communities(
    graph: &CsrGraph,
    config: &CommunityDetectionConfig,
) -> Result<Vec<CommunityAssignment>> {
    graph.compact();

    // Acquire read lock to access CSR arrays
    let (valid_nodes, reverse_map, adj) = {
        let inner = graph.inner_read();
        let num_nodes = inner.reverse_map.len();

        let mut valid_nodes = Vec::new();
        for idx in 0..num_nodes {
            if inner.entities.get(idx).is_some_and(|e| e.is_some()) {
                valid_nodes.push(idx);
            }
        }

        if valid_nodes.is_empty() {
            return Ok(Vec::new());
        }

        // Build undirected adjacency list
        let mut adj: HashMap<usize, Vec<(usize, f32)>> = HashMap::new();
        for &u in &valid_nodes {
            if u < inner.offsets.len() - 1 {
                let start = inner.offsets[u];
                let end = inner.offsets[u + 1];
                for edge_idx in start..end {
                    let v = inner.targets[edge_idx];
                    if !inner.tombstoned_edges.contains(&(u, v))
                        && inner.entities.get(v).is_some_and(|e| e.is_some())
                    {
                        let w = inner.weights[edge_idx];
                        adj.entry(u).or_default().push((v, w));
                        adj.entry(v).or_default().push((u, w));
                    }
                }
            }
        }

        (valid_nodes, inner.reverse_map.clone(), adj)
    };

    // Sort valid node indices by EntityId for deterministic initial ordering
    let mut node_indices = valid_nodes;
    node_indices.sort_by_key(|&idx| reverse_map[idx]);

    // Initialize labels: labels[node_idx] = reverse_map[node_idx].inner()
    let mut labels: HashMap<usize, u64> = HashMap::new();
    for &idx in &node_indices {
        labels.insert(idx, reverse_map[idx].inner());
    }

    // Run Label Propagation iterations
    for iter in 0..config.max_iterations {
        let mut rng = SimpleRng::new(config.seed.wrapping_add(iter as u64));
        let mut order = node_indices.clone();
        rng.shuffle(&mut order);

        let mut changed = false;

        for &u in &order {
            let neighbors = match adj.get(&u) {
                Some(n) if !n.is_empty() => n,
                _ => continue,
            };

            // Calculate aggregate weight per community label
            let mut label_weights: HashMap<u64, f32> = HashMap::new();
            for &(v, w) in neighbors {
                if let Some(&label_v) = labels.get(&v) {
                    let weight = if w > 0.0 { w } else { 1.0 };
                    *label_weights.entry(label_v).or_default() += weight;
                }
            }

            if label_weights.is_empty() {
                continue;
            }

            // Find max weight
            let mut max_weight = -1.0f32;
            for &w in label_weights.values() {
                if w > max_weight {
                    max_weight = w;
                }
            }

            // Collect labels matching max_weight (within floating point epsilon)
            let mut candidate_labels = Vec::new();
            for (&label, &w) in &label_weights {
                if (w - max_weight).abs() < 1e-6 {
                    candidate_labels.push(label);
                }
            }

            // Tie-breaking: smallest label (u64 / EntityId) wins
            if let Some(&best_label) = candidate_labels.iter().min() {
                if labels.get(&u) != Some(&best_label) {
                    labels.insert(u, best_label);
                    changed = true;
                }
            }
        }

        if !changed {
            break;
        }
    }

    // Build final result list sorted by EntityId
    let mut assignments = Vec::with_capacity(node_indices.len());
    for idx in node_indices {
        let entity_id = reverse_map[idx];
        let community_id = labels
            .get(&idx)
            .copied()
            .unwrap_or_else(|| entity_id.inner());
        assignments.push(CommunityAssignment {
            entity_id,
            community_id,
        });
    }

    Ok(assignments)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::csr::CsrGraph;
    use memfuse_core::{Edge, Entity, EntityId, GraphIndex, TxId};

    #[tokio::test]
    async fn test_community_detection_determinism() {
        let graph = CsrGraph::new();
        let tx = TxId::new(1);

        for i in 1..=10 {
            graph
                .add_entity(tx, Entity::new(EntityId::new(i), format!("E{i}"), "Node"))
                .await
                .unwrap();
        }

        // Add some edges
        graph
            .add_edge(tx, Edge::new(EntityId::new(1), EntityId::new(2), "knows"))
            .await
            .unwrap();
        graph
            .add_edge(tx, Edge::new(EntityId::new(2), EntityId::new(3), "knows"))
            .await
            .unwrap();
        graph
            .add_edge(tx, Edge::new(EntityId::new(3), EntityId::new(1), "knows"))
            .await
            .unwrap();
        graph
            .add_edge(tx, Edge::new(EntityId::new(4), EntityId::new(5), "knows"))
            .await
            .unwrap();
        graph.commit(tx).await.unwrap();

        let config = CommunityDetectionConfig {
            max_iterations: 50,
            seed: 12345,
        };

        let run1 = detect_communities(&graph, &config).await.unwrap();
        let run2 = detect_communities(&graph, &config).await.unwrap();

        assert_eq!(run1, run2, "Twice execution with identical graph and seed must yield identical CommunityAssignments");
    }

    #[tokio::test]
    async fn test_community_detection_disconnected_clusters() {
        let graph = CsrGraph::new();
        let tx = TxId::new(1);

        // Cluster 1: Nodes 1, 2, 3 tightly connected
        for id in 1..=3 {
            graph
                .add_entity(
                    tx,
                    Entity::new(EntityId::new(id), format!("C1_{id}"), "Node"),
                )
                .await
                .unwrap();
        }
        graph
            .add_edge(tx, Edge::new(EntityId::new(1), EntityId::new(2), "link"))
            .await
            .unwrap();
        graph
            .add_edge(tx, Edge::new(EntityId::new(2), EntityId::new(3), "link"))
            .await
            .unwrap();
        graph
            .add_edge(tx, Edge::new(EntityId::new(3), EntityId::new(1), "link"))
            .await
            .unwrap();

        // Cluster 2: Nodes 100, 101, 102 tightly connected (no path to Cluster 1)
        for id in [100, 101, 102] {
            graph
                .add_entity(
                    tx,
                    Entity::new(EntityId::new(id), format!("C2_{id}"), "Node"),
                )
                .await
                .unwrap();
        }
        graph
            .add_edge(
                tx,
                Edge::new(EntityId::new(100), EntityId::new(101), "link"),
            )
            .await
            .unwrap();
        graph
            .add_edge(
                tx,
                Edge::new(EntityId::new(101), EntityId::new(102), "link"),
            )
            .await
            .unwrap();
        graph
            .add_edge(
                tx,
                Edge::new(EntityId::new(102), EntityId::new(100), "link"),
            )
            .await
            .unwrap();

        graph.commit(tx).await.unwrap();

        let config = CommunityDetectionConfig::default();
        let assignments = detect_communities(&graph, &config).await.unwrap();

        let map: HashMap<u64, u64> = assignments
            .into_iter()
            .map(|a| (a.entity_id.inner(), a.community_id))
            .collect();

        // Nodes in Cluster 1 must share the same community ID
        let c1_community = map[&1];
        assert_eq!(map[&2], c1_community);
        assert_eq!(map[&3], c1_community);

        // Nodes in Cluster 2 must share the same community ID
        let c2_community = map[&100];
        assert_eq!(map[&101], c2_community);
        assert_eq!(map[&102], c2_community);

        // Cluster 1 and Cluster 2 must be assigned DIFFERENT communities
        assert_ne!(
            c1_community, c2_community,
            "Disconnected clusters MUST be assigned to different communities"
        );
    }
}
