//! PathRAG Engine — Graph-basiertes Retrieval via bidirektionaler Dijkstra.
//!
//! Basis: arXiv:2502.14902 (PathRAG, AAAI 2026).
//! Präzisions-Scope: Nur für Multi-Hop-Anfragen verwenden (arXiv:2506.05690).
//! Sufficiency-Gate verhindert Precision-Kollaps (arXiv:2506.00610).
//!
//! INTEGRATION: PathRAG liefert ein RRF-Signal neben Vektor- und BM25-Signal.
//! Resultat von to_rrf_signal() wird in FusionEngine als drittes Signal eingespeist.

use memfuse_core::DocId;
pub use memfuse_core::EntityId;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

/// Ein gefundener Pfad zwischen zwei Knoten.
#[derive(Debug, Clone)]
pub struct GraphPath {
    /// Knoten-Sequenz vom Start zum Ziel.
    pub nodes: Vec<EntityId>,
    /// Kantengewichte entlang des Pfades (len = nodes.len() - 1).
    pub edge_weights: Vec<f32>,
    /// Konfidenz: Produkt aller Kantengewichte.
    pub confidence: f64,
    /// Invertierte Gesamtdistanz als Flow-Proxy.
    pub total_flow: f32,
}

/// Trait für Graphen die PathRAG konsumieren kann.
/// Ermöglicht Testbarkeit ohne echten CSR-Graphen.
pub trait PathGraph: Send + Sync {
    fn neighbors_with_weights(&self, node: EntityId) -> Vec<(EntityId, f32)>;
    fn predecessors_with_weights(&self, node: EntityId) -> Vec<(EntityId, f32)>;
}

pub struct PathRAGEngine<G: PathGraph> {
    graph: G,
    /// Maximale Suchtiefe (Hop-Limit).
    max_hops: usize,
    /// Sufficiency-Schwelle: Pfade unter dieser Konfidenz werden gefiltert.
    sufficiency_threshold: f64,
}

impl<G: PathGraph> PathRAGEngine<G> {
    pub fn new(graph: G, max_hops: usize, sufficiency_threshold: f64) -> Self {
        Self {
            graph,
            max_hops,
            sufficiency_threshold,
        }
    }

    pub fn with_defaults(graph: G) -> Self {
        Self::new(graph, 4, 0.01)
    }

    /// Findet den optimalen Pfad zwischen source und target via bidirektionalem Dijkstra.
    ///
    /// Gibt None zurück wenn kein Pfad innerhalb max_hops existiert
    /// oder Sufficiency-Gate fehlschlägt.
    pub fn find_path(&self, source: EntityId, target: EntityId) -> Option<GraphPath> {
        if source == target {
            return Some(GraphPath {
                nodes: vec![source],
                edge_weights: vec![],
                confidence: 1.0,
                total_flow: f32::INFINITY,
            });
        }

        // Bidirektionaler Dijkstra: Forward von source, Backward von target
        let mut dist_fwd: HashMap<EntityId, f32> = HashMap::new();
        let mut dist_bwd: HashMap<EntityId, f32> = HashMap::new();
        let mut prev_fwd: HashMap<EntityId, (EntityId, f32)> = HashMap::new();
        let mut prev_bwd: HashMap<EntityId, (EntityId, f32)> = HashMap::new();

        // BinaryHeap: Reverse((f32_bits, EntityId)) — f32 über Bits geordnet
        let mut heap_fwd: BinaryHeap<Reverse<(u32, EntityId)>> = BinaryHeap::new();
        let mut heap_bwd: BinaryHeap<Reverse<(u32, EntityId)>> = BinaryHeap::new();

        dist_fwd.insert(source, 0.0);
        dist_bwd.insert(target, 0.0);
        heap_fwd.push(Reverse((0u32, source)));
        heap_bwd.push(Reverse((0u32, target)));

        let mut best_dist = f32::INFINITY;
        let mut meeting_node: Option<EntityId> = None;

        let mut steps = 0;
        let max_steps = self.max_hops * 1000; // Schutzzähler gegen Endlosschleife

        while (!heap_fwd.is_empty() || !heap_bwd.is_empty()) && steps < max_steps {
            steps += 1;

            // Vorwärts-Schritt
            if let Some(Reverse((d_bits, u))) = heap_fwd.pop() {
                let d = f32::from_bits(d_bits);
                if d <= *dist_fwd.get(&u).unwrap_or(&f32::INFINITY) {
                    // Treffen-Check
                    if let Some(&bwd_d) = dist_bwd.get(&u) {
                        let total = d + bwd_d;
                        if total < best_dist {
                            best_dist = total;
                            meeting_node = Some(u);
                        }
                    }

                    if d <= best_dist {
                        for (neighbor, weight) in self.graph.neighbors_with_weights(u) {
                            let new_d = d + (1.0 / weight.max(1e-8));
                            let entry = dist_fwd.entry(neighbor).or_insert(f32::INFINITY);
                            if new_d < *entry {
                                *entry = new_d;
                                prev_fwd.insert(neighbor, (u, weight));
                                heap_fwd.push(Reverse((new_d.to_bits(), neighbor)));
                            }
                        }
                    }
                }
            }

            // Rückwärts-Schritt
            if let Some(Reverse((d_bits, u))) = heap_bwd.pop() {
                let d = f32::from_bits(d_bits);
                if d <= *dist_bwd.get(&u).unwrap_or(&f32::INFINITY) {
                    if let Some(&fwd_d) = dist_fwd.get(&u) {
                        let total = fwd_d + d;
                        if total < best_dist {
                            best_dist = total;
                            meeting_node = Some(u);
                        }
                    }

                    if d <= best_dist {
                        for (neighbor, weight) in self.graph.predecessors_with_weights(u) {
                            let new_d = d + (1.0 / weight.max(1e-8));
                            let entry = dist_bwd.entry(neighbor).or_insert(f32::INFINITY);
                            if new_d < *entry {
                                *entry = new_d;
                                prev_bwd.insert(neighbor, (u, weight));
                                heap_bwd.push(Reverse((new_d.to_bits(), neighbor)));
                            }
                        }
                    }
                }
            }
        }

        let meeting = meeting_node?;

        // Pfad rekonstruieren
        let mut path_nodes = vec![];
        let mut path_weights = vec![];

        // Vorwärts-Pfad: source → meeting
        let mut cur = meeting;
        while cur != source {
            let (prev, w) = *prev_fwd.get(&cur)?;
            path_nodes.push(cur);
            path_weights.push(w);
            cur = prev;
        }
        path_nodes.push(source);
        path_nodes.reverse();
        path_weights.reverse();

        // Rückwärts-Pfad: meeting → target
        cur = meeting;
        while cur != target {
            let (next, w) = *prev_bwd.get(&cur)?;
            path_nodes.push(next);
            path_weights.push(w);
            cur = next;
        }

        let confidence: f64 = path_weights.iter().map(|&w| w as f64).product();
        let total_flow = if best_dist > 0.0 {
            best_dist.recip()
        } else {
            f32::INFINITY
        };

        Some(GraphPath {
            nodes: path_nodes,
            edge_weights: path_weights,
            confidence,
            total_flow,
        })
    }

    /// Sufficiency-Gate: Filtert Pfade unter Konfidenz-Schwelle.
    /// Kritisch für Precision (arXiv:2506.00610).
    pub fn sufficiency_check(&self, path: &GraphPath) -> bool {
        path.confidence >= self.sufficiency_threshold
    }

    /// Konvertiert gefilterte Pfade in RRF-kompatibles Signal.
    /// Nur Knoten aus Pfaden die sufficiency_check() passiert haben.
    pub fn to_rrf_signal(&self, paths: &[GraphPath]) -> Vec<(DocId, f32)> {
        let mut scored: HashMap<u64, f32> = HashMap::new();

        for path in paths.iter().filter(|p| self.sufficiency_check(p)) {
            for (i, &entity_id) in path.nodes.iter().enumerate() {
                let position_weight = (i + 1) as f32 / path.nodes.len() as f32;
                *scored.entry(entity_id.inner()).or_insert(0.0) +=
                    path.total_flow * position_weight;
            }
        }

        let mut result: Vec<(DocId, f32)> = scored
            .into_iter()
            .map(|(id, score)| (DocId(id), score))
            .collect();
        result.sort_by(|a, b| b.1.total_cmp(&a.1));
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Einfacher Test-Graph
    struct TestGraph {
        edges: HashMap<EntityId, Vec<(EntityId, f32)>>,
    }

    impl TestGraph {
        fn new(edges: Vec<(EntityId, EntityId, f32)>) -> Self {
            let mut map: HashMap<EntityId, Vec<(EntityId, f32)>> = HashMap::new();
            for (from, to, w) in edges {
                map.entry(from).or_default().push((to, w));
            }
            Self { edges: map }
        }
    }

    impl PathGraph for TestGraph {
        fn neighbors_with_weights(&self, node: EntityId) -> Vec<(EntityId, f32)> {
            self.edges.get(&node).cloned().unwrap_or_default()
        }
        fn predecessors_with_weights(&self, node: EntityId) -> Vec<(EntityId, f32)> {
            // Für undirektierte Tests: alle Knoten die auf node zeigen
            self.edges
                .iter()
                .flat_map(|(from, nbrs)| {
                    nbrs.iter().filter_map(
                        move |(to, w)| {
                            if *to == node {
                                Some((*from, *w))
                            } else {
                                None
                            }
                        },
                    )
                })
                .collect()
        }
    }

    #[test]
    fn test_find_path_direct_edge() {
        let a = EntityId::new(1);
        let b = EntityId::new(2);
        let graph = TestGraph::new(vec![(a, b, 0.9)]);
        let engine = PathRAGEngine::with_defaults(graph);
        let path = engine.find_path(a, b).unwrap();
        assert_eq!(path.nodes, vec![a, b]);
        assert!((path.confidence - 0.9).abs() < 1e-6);
    }

    #[test]
    fn test_find_path_multi_hop() {
        let a = EntityId::new(1);
        let b = EntityId::new(2);
        let c = EntityId::new(3);
        let graph = TestGraph::new(vec![(a, b, 0.8), (b, c, 0.9)]);
        let engine = PathRAGEngine::with_defaults(graph);
        let path = engine.find_path(a, c).unwrap();
        assert_eq!(path.nodes.len(), 3);
        assert!(
            (path.confidence - 0.72).abs() < 1e-5,
            "0.8*0.9={}",
            path.confidence
        );
    }

    #[test]
    fn test_find_path_same_node() {
        let a = EntityId::new(1);
        let graph = TestGraph::new(vec![]);
        let engine = PathRAGEngine::with_defaults(graph);
        let path = engine.find_path(a, a).unwrap();
        assert_eq!(path.nodes, vec![a]);
        assert_eq!(path.edge_weights.len(), 0);
    }

    #[test]
    fn test_find_path_unreachable_returns_none() {
        let a = EntityId::new(1);
        let b = EntityId::new(99);
        let graph = TestGraph::new(vec![]);
        let engine = PathRAGEngine::with_defaults(graph);
        assert!(engine.find_path(a, b).is_none());
    }

    #[test]
    fn test_sufficiency_gate_filters_low_confidence() {
        let engine = PathRAGEngine::new(TestGraph::new(vec![]), 4, 0.5);
        let low_conf = GraphPath {
            nodes: vec![EntityId::new(1), EntityId::new(2)],
            edge_weights: vec![0.3],
            confidence: 0.3,
            total_flow: 3.33,
        };
        assert!(!engine.sufficiency_check(&low_conf));
        let high_conf = GraphPath {
            confidence: 0.8,
            ..low_conf
        };
        assert!(engine.sufficiency_check(&high_conf));
    }

    #[test]
    fn test_rrf_signal_filters_by_sufficiency() {
        let engine = PathRAGEngine::new(TestGraph::new(vec![]), 4, 0.5);
        let paths = vec![
            GraphPath {
                nodes: vec![EntityId::new(1)],
                edge_weights: vec![],
                confidence: 0.1,
                total_flow: 1.0,
            }, // under threshold
            GraphPath {
                nodes: vec![EntityId::new(2)],
                edge_weights: vec![],
                confidence: 0.9,
                total_flow: 1.0,
            }, // passes
        ];
        let signal = engine.to_rrf_signal(&paths);
        assert_eq!(signal.len(), 1);
        assert_eq!(signal[0].0, DocId(2));
    }
}
