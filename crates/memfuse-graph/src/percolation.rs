//! F-06: Perkolations-Gesundheitsmonitor.
//!
//! FEATURE-FLAG: `physio-percolation`
//! KEIN neuer Index — nutzt bestehende CSR-Struktur und Embedding-Infrastruktur.
//! Re-Bonding ist eine seltene, asynchrone Background-Operation.

use memfuse_core::EntityId;
use std::collections::HashMap;

/// Konfiguration für den Perkolations-Gesundheitsmonitor.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PercolationConfig {
    /// Kritischer Perkolationsschwellenwert θ_perc. Default: 0.7.
    pub critical_threshold: f32,
    /// Similarity-Schwellenwert für Re-Bonding θ_bond. Default: 0.85.
    pub rebonding_similarity: f32,
    /// Maximale neue Kanten pro Re-Bonding-Pass. Default: 100.
    pub max_new_edges_per_pass: usize,
}

impl Default for PercolationConfig {
    fn default() -> Self {
        Self {
            critical_threshold: 0.7,
            rebonding_similarity: 0.85,
            max_new_edges_per_pass: 100,
        }
    }
}

/// Berechnet aktuelle Perkolations-Gesundheit φ(t).
///
/// Gibt `None` zurück wenn zu wenige Knoten vorhanden (< 10) — Perkolation nicht sinnvoll.
pub fn compute_percolation_health(
    active_node_count: usize,
    active_edge_count: usize, // nach Tombstone-Abzug
) -> Option<f32> {
    if active_node_count < 10 {
        return None;
    }
    let expected = (active_node_count as f32) * (active_node_count as f32).ln().max(1.0);
    Some((active_edge_count as f32) / expected)
}

/// Prüft ob Re-Bonding-Pass ausgelöst werden soll.
pub fn should_trigger_rebonding(health: f32, config: &PercolationConfig) -> bool {
    health < config.critical_threshold
}

/// Berechnet Cosine Similarity zwischen zwei Vektoren.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;
    for (&x, &y) in a.iter().zip(b.iter()) {
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }
    if norm_a <= 0.0 || norm_b <= 0.0 {
        return 0.0;
    }
    dot / (norm_a.sqrt() * norm_b.sqrt())
}

/// Findet Kandidaten-Paare für Re-Bonding.
/// Gibt `(EntityId, EntityId, similarity)`-Tupel zurück, sortiert absteigend nach Similarity.
pub async fn find_rebonding_candidates<G: memfuse_core::GraphIndex>(
    graph: &G,
    embeddings: &HashMap<EntityId, Vec<f32>>,
    config: &PercolationConfig,
) -> Vec<(EntityId, EntityId, f32)> {
    let mut candidates = Vec::new();
    let nodes: Vec<EntityId> = embeddings.keys().copied().collect();
    let n = nodes.len();

    if n < 2 {
        return candidates;
    }

    // Cache neighbors for visited nodes to avoid repeated async calls
    let mut neighbors_cache: HashMap<EntityId, std::collections::HashSet<EntityId>> =
        HashMap::new();

    for i in 0..n {
        for j in (i + 1)..n {
            let id1 = nodes[i];
            let id2 = nodes[j];

            let emb1 = &embeddings[&id1];
            let emb2 = &embeddings[&id2];

            let sim = cosine_similarity(emb1, emb2);
            if sim <= config.rebonding_similarity {
                continue;
            }

            // Check if a direct edge already exists in either direction
            if !neighbors_cache.contains_key(&id1) {
                if let Ok(nbrs) = graph.neighbors(id1).await {
                    neighbors_cache.insert(id1, nbrs.into_iter().collect());
                } else {
                    neighbors_cache.insert(id1, std::collections::HashSet::new());
                }
            }

            if neighbors_cache[&id1].contains(&id2) {
                continue;
            }

            if !neighbors_cache.contains_key(&id2) {
                if let Ok(nbrs) = graph.neighbors(id2).await {
                    neighbors_cache.insert(id2, nbrs.into_iter().collect());
                } else {
                    neighbors_cache.insert(id2, std::collections::HashSet::new());
                }
            }

            if neighbors_cache[&id2].contains(&id1) {
                continue;
            }

            candidates.push((id1, id2, sim));
        }
    }

    candidates.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

    if candidates.len() > config.max_new_edges_per_pass {
        candidates.truncate(config.max_new_edges_per_pass);
    }

    candidates
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsrGraph;
    use memfuse_core::{Entity, GraphIndex, TxId};
    use std::sync::Arc;

    #[test]
    fn test_percolation_health_full_graph() {
        // Vollständig verbundener Graph mit N=10 Knoten hat N*(N-1) = 90 gerichtete Kanten
        let n_nodes = 10;
        let n_edges = 90;
        let health = compute_percolation_health(n_nodes, n_edges);
        assert!(health.is_some());
        let phi = health.unwrap();
        // expected = 10 * ln(10) ≈ 23.02585
        // phi = 90 / 23.02585 ≈ 3.908 >> 1.0
        assert!(phi > 1.0, "Expected health >> 1.0, got {phi}");
    }

    #[test]
    fn test_percolation_health_empty_graph() {
        let n_nodes = 10;
        let n_edges = 0;
        let health = compute_percolation_health(n_nodes, n_edges);
        assert!(health.is_some());
        let phi = health.unwrap();
        assert_eq!(phi, 0.0);
    }

    #[test]
    fn test_percolation_should_trigger_below_threshold() {
        let config = PercolationConfig::default(); // critical_threshold = 0.7
        assert!(should_trigger_rebonding(0.5, &config));
        assert!(should_trigger_rebonding(0.69, &config));
    }

    #[test]
    fn test_percolation_no_trigger_above_threshold() {
        let config = PercolationConfig::default(); // critical_threshold = 0.7
        assert!(!should_trigger_rebonding(0.7, &config));
        assert!(!should_trigger_rebonding(0.85, &config));
    }

    #[test]
    fn test_percolation_skips_too_small_graphs() {
        assert!(compute_percolation_health(0, 0).is_none());
        assert!(compute_percolation_health(9, 50).is_none());
        assert!(compute_percolation_health(10, 0).is_some());
    }

    #[tokio::test]
    async fn test_find_rebonding_candidates() {
        let graph = Arc::new(CsrGraph::new());
        let tx = TxId::new(1);

        let id1 = EntityId::new(1);
        let id2 = EntityId::new(2);
        let id3 = EntityId::new(3);

        graph
            .add_entity(tx, Entity::new(id1, "N1", "Type"))
            .await
            .unwrap();
        graph
            .add_entity(tx, Entity::new(id2, "N2", "Type"))
            .await
            .unwrap();
        graph
            .add_entity(tx, Entity::new(id3, "N3", "Type"))
            .await
            .unwrap();

        // Edge 1 -> 2 exists
        graph
            .insert_edge_direct(id1, id2, 1.0)
            .await
            .unwrap();
        graph.commit(tx).await.unwrap();

        let mut embeddings = HashMap::new();
        // 1 & 2: sim 0.8 (> 0.75), but connected via edge
        embeddings.insert(id1, vec![1.0, 0.0, 0.0]);
        embeddings.insert(id2, vec![0.8, 0.6, 0.0]);
        // 1 & 3: sim 0.8 (> 0.75), unconnected -> CANDIDATE!
        // 2 & 3: sim 0.28 (< 0.75) -> ignored
        embeddings.insert(id3, vec![0.8, -0.6, 0.0]);

        let config = PercolationConfig {
            critical_threshold: 0.7,
            rebonding_similarity: 0.75,
            max_new_edges_per_pass: 100,
        };

        let candidates = find_rebonding_candidates(graph.as_ref(), &embeddings, &config).await;

        // 1 & 2 ignored (connected). 1 & 3 should be candidate.
        assert_eq!(candidates.len(), 1);
        let (cand_from, cand_to, sim) = &candidates[0];
        assert!(
            (*cand_from == id1 && *cand_to == id3) || (*cand_from == id3 && *cand_to == id1)
        );
        assert!(*sim > config.rebonding_similarity);
    }
}
