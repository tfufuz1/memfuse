//! Comprehensive CSR Graph Correctness Test Suite.
//!
//! Verifies BFS traversal, score decay, delta buffer compaction,
//! edge cases, and multi-threaded concurrency against reference implementations.

use memfuse_core::{Edge, Entity, EntityId, GraphIndex, TxId};
use memfuse_graph::csr::{CsrGraph, CsrGraphConfig};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

/// Score decay factor per hop (0.7^hop).
const SCORE_DECAY: f32 = 0.7;

/// Independent Reference BFS Implementation for Verification (Anti-Mirroring).
struct ReferenceGraph {
    nodes: HashSet<u64>,
    edges: HashMap<u64, Vec<(u64, f32)>>,
}

impl ReferenceGraph {
    fn new() -> Self {
        Self {
            nodes: HashSet::new(),
            edges: HashMap::new(),
        }
    }

    fn add_node(&mut self, id: u64) {
        self.nodes.insert(id);
    }

    fn add_edge(&mut self, from: u64, to: u64, weight: f32) {
        self.nodes.insert(from);
        self.nodes.insert(to);
        self.edges.entry(from).or_default().push((to, weight));
    }

    /// Performs reference BFS traversal with score decay from start node up to max_hops (capped at 3).
    fn bfs_traverse(&self, start: u64, max_hops: usize) -> Vec<(u64, f32)> {
        if !self.nodes.contains(&start) {
            return Vec::new();
        }

        let effective_max = max_hops.min(3) as u8;
        let mut visited: HashMap<u64, f32> = HashMap::new();
        let mut queue: VecDeque<(u64, u8, f32)> = VecDeque::new();

        queue.push_back((start, 0, 1.0));

        while let Some((node, hop, current_score)) = queue.pop_front() {
            if hop > effective_max {
                continue;
            }

            let entry = visited.entry(node).or_insert(0.0);
            if current_score > *entry {
                *entry = current_score;
            }

            if hop < effective_max {
                if let Some(neighbors) = self.edges.get(&node) {
                    for &(neighbor, weight) in neighbors {
                        let next_score = current_score * SCORE_DECAY * weight;
                        if !visited.contains_key(&neighbor) || visited[&neighbor] < next_score {
                            if self.nodes.contains(&neighbor) {
                                queue.push_back((neighbor, hop + 1, next_score));
                            }
                        }
                    }
                }
            }
        }

        visited.remove(&start);

        let mut results: Vec<(u64, f32)> = visited.into_iter().collect();
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results
    }
}

// Helper: Populates both CsrGraph and ReferenceGraph with synthetic Star topology
async fn create_star_graph(n_leaves: usize) -> (CsrGraph, ReferenceGraph) {
    let csr = CsrGraph::new();
    let mut ref_g = ReferenceGraph::new();
    let tx = TxId::new(1);

    csr.add_entity(tx, Entity::new(EntityId::new(0), "Center", "Center"))
        .await
        .unwrap();
    ref_g.add_node(0);

    for i in 1..=n_leaves {
        let id = i as u64;
        csr.add_entity(tx, Entity::new(EntityId::new(id), format!("Leaf_{i}"), "Leaf"))
            .await
            .unwrap();
        let weight = 0.5 + (i as f32 * 0.01);
        csr.add_edge(
            tx,
            Edge::new(EntityId::new(0), EntityId::new(id), "leaf_rel").with_weight(weight),
        )
        .await
        .unwrap();
        ref_g.add_edge(0, id, weight);
    }

    csr.commit(tx).await.unwrap();
    (csr, ref_g)
}

// Helper: Populates both CsrGraph and ReferenceGraph with synthetic Chain topology
async fn create_chain_graph(len: usize, weight_pattern: fn(usize) -> f32) -> (CsrGraph, ReferenceGraph) {
    let csr = CsrGraph::new();
    let mut ref_g = ReferenceGraph::new();
    let tx = TxId::new(1);

    for i in 0..len {
        let id = i as u64;
        csr.add_entity(tx, Entity::new(EntityId::new(id), format!("N{i}"), "Node"))
            .await
            .unwrap();
        ref_g.add_node(id);
    }

    for i in 0..len - 1 {
        let u = i as u64;
        let v = (i + 1) as u64;
        let w = weight_pattern(i);
        csr.add_edge(
            tx,
            Edge::new(EntityId::new(u), EntityId::new(v), "link").with_weight(w),
        )
        .await
        .unwrap();
        ref_g.add_edge(u, v, w);
    }

    csr.commit(tx).await.unwrap();
    (csr, ref_g)
}

// Helper: Populates Complete graph K_n
async fn create_complete_graph(n: usize) -> (CsrGraph, ReferenceGraph) {
    let csr = CsrGraph::new();
    let mut ref_g = ReferenceGraph::new();
    let tx = TxId::new(1);

    for i in 0..n {
        let id = i as u64;
        csr.add_entity(tx, Entity::new(EntityId::new(id), format!("N{i}"), "Node"))
            .await
            .unwrap();
        ref_g.add_node(id);
    }

    for i in 0..n {
        for j in 0..n {
            if i != j {
                let u = i as u64;
                let v = j as u64;
                let w = 0.8;
                csr.add_edge(
                    tx,
                    Edge::new(EntityId::new(u), EntityId::new(v), "link").with_weight(w),
                )
                .await
                .unwrap();
                ref_g.add_edge(u, v, w);
            }
        }
    }

    csr.commit(tx).await.unwrap();
    (csr, ref_g)
}

// Helper: Erdős–Rényi random graph generator with fixed seed
async fn create_erdos_renyi_graph(n: usize, p: f64, seed: u64) -> (CsrGraph, ReferenceGraph) {
    let csr = CsrGraph::new();
    let mut ref_g = ReferenceGraph::new();
    let tx = TxId::new(1);

    for i in 0..n {
        let id = i as u64;
        csr.add_entity(tx, Entity::new(EntityId::new(id), format!("N{i}"), "Node"))
            .await
            .unwrap();
        ref_g.add_node(id);
    }

    let mut state = seed;
    let mut next_float = || {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (state as f64) / (u64::MAX as f64)
    };

    for i in 0..n {
        for j in 0..n {
            if i != j && next_float() < p {
                let u = i as u64;
                let v = j as u64;
                let w = (0.1 + 0.8 * next_float()) as f32;
                csr.add_edge(
                    tx,
                    Edge::new(EntityId::new(u), EntityId::new(v), "link").with_weight(w),
                )
                .await
                .unwrap();
                ref_g.add_edge(u, v, w);
            }
        }
    }

    csr.commit(tx).await.unwrap();
    (csr, ref_g)
}

#[tokio::test]
async fn test_star_topology_bfs_and_scores() {
    let (csr, ref_g) = create_star_graph(20).await;

    let csr_res = csr.traverse(EntityId::new(0), 1).await.unwrap();
    let ref_res = ref_g.bfs_traverse(0, 1);

    assert_eq!(csr_res.len(), ref_res.len());
    for ((csr_id, csr_score), (ref_id, ref_score)) in csr_res.iter().zip(ref_res.iter()) {
        assert_eq!(csr_id.inner(), *ref_id);
        assert!(
            (csr_score - ref_score).abs() < 1e-5,
            "Score mismatch for node {ref_id}: {csr_score} vs {ref_score}"
        );
    }
}

#[tokio::test]
async fn test_chain_topology_exact_score_decay() {
    // Chain: 0 -> 1 -> 2 -> 3 with weights 1.0, 0.8, 0.5
    let (csr, ref_g) = create_chain_graph(4, |i| match i {
        0 => 1.0,
        1 => 0.8,
        2 => 0.5,
        _ => 1.0,
    })
    .await;

    let csr_res = csr.traverse(EntityId::new(0), 3).await.unwrap();
    let ref_res = ref_g.bfs_traverse(0, 3);

    assert_eq!(csr_res.len(), 3);
    assert_eq!(ref_res.len(), 3);

    // Analytical scores:
    // Hop 1 (node 1): 1.0 * 0.7 * 1.0 = 0.7
    // Hop 2 (node 2): 0.7 * 0.7 * 0.8 = 0.392
    // Hop 3 (node 3): 0.392 * 0.7 * 0.5 = 0.1372
    let map: HashMap<u64, f32> = csr_res.into_iter().map(|(id, s)| (id.inner(), s)).collect();

    let s1 = map[&1];
    let s2 = map[&2];
    let s3 = map[&3];

    assert!((s1 - 0.7).abs() < 1e-5, "Expected 0.7, got {s1}");
    assert!((s2 - 0.392).abs() < 1e-5, "Expected 0.392, got {s2}");
    assert!((s3 - 0.1372).abs() < 1e-5, "Expected 0.1372, got {s3}");
}

#[tokio::test]
async fn test_complete_graph_topology() {
    let (csr, ref_g) = create_complete_graph(10).await;

    let csr_res = csr.traverse(EntityId::new(0), 1).await.unwrap();
    let ref_res = ref_g.bfs_traverse(0, 1);

    assert_eq!(csr_res.len(), 9, "In K_10, node 0 has 9 neighbors");
    assert_eq!(csr_res.len(), ref_res.len());
}

#[tokio::test]
async fn test_erdos_renyi_random_graph_equivalence() {
    let (csr, ref_g) = create_erdos_renyi_graph(30, 0.25, 424242).await;

    for start in 0..5 {
        let csr_res = csr.traverse(EntityId::new(start), 2).await.unwrap();
        let ref_res = ref_g.bfs_traverse(start, 2);

        let csr_map: HashMap<u64, f32> = csr_res.into_iter().map(|(id, s)| (id.inner(), s)).collect();
        let ref_map: HashMap<u64, f32> = ref_res.into_iter().collect();

        assert_eq!(
            csr_map.len(),
            ref_map.len(),
            "Traversal reachability count mismatch from start node {start}"
        );

        for (node, ref_score) in ref_map {
            let csr_score = csr_map.get(&node).copied().expect("Node missing in CsrGraph");
            assert!(
                (csr_score - ref_score).abs() < 1e-5,
                "Score mismatch at node {node} from start {start}: {csr_score} vs {ref_score}"
            );
        }
    }
}

#[tokio::test]
async fn test_pending_edges_compaction_cycles() {
    let csr = CsrGraph::with_config(CsrGraphConfig {
        rebuild_threshold: 1000,
    });
    let tx1 = TxId::new(1);

    // Initial setup
    csr.add_entity(tx1, Entity::new(EntityId::new(1), "N1", "T")).await.unwrap();
    csr.add_entity(tx1, Entity::new(EntityId::new(2), "N2", "T")).await.unwrap();
    csr.add_edge(tx1, Edge::new(EntityId::new(1), EntityId::new(2), "link")).await.unwrap();
    csr.commit(tx1).await.unwrap();

    // Verify traversal before compaction
    let res1 = csr.traverse(EntityId::new(1), 1).await.unwrap();
    assert_eq!(res1.len(), 1);

    // First compaction
    csr.compact();
    let res2 = csr.traverse(EntityId::new(1), 1).await.unwrap();
    assert_eq!(res2.len(), 1);
    assert_eq!(res1, res2);

    // Repeat 5 incremental add + compact cycles
    for cycle in 2..=6 {
        let tx = TxId::new(cycle as u64);
        let next_id = cycle as u64 + 1;
        csr.add_entity(tx, Entity::new(EntityId::new(next_id), format!("N{next_id}"), "T")).await.unwrap();
        csr.add_edge(
            tx,
            Edge::new(EntityId::new(cycle as u64), EntityId::new(next_id), "link"),
        )
        .await
        .unwrap();
        csr.commit(tx).await.unwrap();

        // Traversal before cycle compact
        let pre_compact = csr.traverse(EntityId::new(1), 3).await.unwrap();
        csr.compact();
        // Traversal after cycle compact
        let post_compact = csr.traverse(EntityId::new(1), 3).await.unwrap();

        assert_eq!(
            pre_compact, post_compact,
            "Traversal results must be identical before and after compaction in cycle {cycle}"
        );
    }
}

#[tokio::test]
async fn test_edge_cases_empty_single_self_loop_multiedges() {
    // 1. Empty graph
    let empty_csr = CsrGraph::new();
    let empty_res = empty_csr.traverse(EntityId::new(1), 2).await.unwrap();
    assert!(empty_res.is_empty());

    // 2. Single isolated node
    let iso_csr = CsrGraph::new();
    let tx1 = TxId::new(1);
    iso_csr.add_entity(tx1, Entity::new(EntityId::new(10), "Iso", "T")).await.unwrap();
    iso_csr.commit(tx1).await.unwrap();
    let iso_res = iso_csr.traverse(EntityId::new(10), 2).await.unwrap();
    assert!(iso_res.is_empty());

    // 3. Self-loop
    let self_csr = CsrGraph::new();
    self_csr.add_entity(tx1, Entity::new(EntityId::new(1), "Self", "T")).await.unwrap();
    self_csr.add_edge(tx1, Edge::new(EntityId::new(1), EntityId::new(1), "loop")).await.unwrap();
    self_csr.commit(tx1).await.unwrap();
    let self_res = self_csr.traverse(EntityId::new(1), 2).await.unwrap();
    assert!(self_res.is_empty(), "Self-loop traversal must not include start node in results");

    // 4. Parallel edges (multi-edges)
    let multi_csr = CsrGraph::new();
    multi_csr.add_entity(tx1, Entity::new(EntityId::new(1), "A", "T")).await.unwrap();
    multi_csr.add_entity(tx1, Entity::new(EntityId::new(2), "B", "T")).await.unwrap();
    multi_csr.add_edge(
        tx1,
        Edge::new(EntityId::new(1), EntityId::new(2), "r1").with_weight(0.4),
    ).await.unwrap();
    multi_csr.add_edge(
        tx1,
        Edge::new(EntityId::new(1), EntityId::new(2), "r2").with_weight(0.9),
    ).await.unwrap();
    multi_csr.commit(tx1).await.unwrap();

    let multi_res = multi_csr.traverse(EntityId::new(1), 1).await.unwrap();
    assert_eq!(multi_res.len(), 1);
    // BFS keeps highest score encountered
    let score = multi_res[0].1;
    assert!(
        (score - 0.63).abs() < 1e-4,
        "Traversal across multi-edges must preserve best score (0.9 * 0.7 = 0.63), got {score}"
    );
}

#[tokio::test]
async fn test_concurrency_stress_parallel_inserts_and_traversals() {
    let graph = Arc::new(CsrGraph::new());
    let tx_setup = TxId::new(1);

    // Pre-create 100 entities
    for i in 1..=100 {
        graph
            .add_entity(
                tx_setup,
                Entity::new(EntityId::new(i), format!("N{i}"), "Type"),
            )
            .await
            .unwrap();
    }
    graph.commit(tx_setup).await.unwrap();

    let mut handles = Vec::new();

    // 10 Writer tasks: each task inserts 20 edges
    for writer_idx in 0..10 {
        let g = graph.clone();
        handles.push(tokio::spawn(async move {
            for edge_idx in 0..20 {
                let tx = TxId::new(100 + writer_idx * 100 + edge_idx + 1);
                let src = (writer_idx * 10 + edge_idx % 10 + 1) as u64;
                let dst = ((writer_idx * 10 + edge_idx % 10 + 5) % 100 + 1) as u64;
                if src != dst {
                    g.add_edge(tx, Edge::new(EntityId::new(src), EntityId::new(dst), "rel"))
                        .await
                        .unwrap();
                    g.commit(tx).await.unwrap();
                }
            }
        }));
    }

    // 10 Reader tasks: concurrently traverse graph while writers are active
    for reader_idx in 0..10 {
        let g = graph.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..50 {
                let start = (reader_idx * 10 + 1) as u64;
                let _res = g.traverse(EntityId::new(start), 2).await;
                tokio::task::yield_now().await;
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    // Compact graph and verify consistency
    graph.compact();
    let stats = graph.stats().await.unwrap();
    assert_eq!(stats.num_entities, 100);
    assert!(
        stats.num_edges > 0,
        "Total edges after concurrent stress test must be positive"
    );
}
