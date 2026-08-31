//! Independent Reference PPR Implementation & Numerical Verification Test Suite.
//!
//! Implements a Matrix Power Method reference PPR algorithm to verify `compute_ppr`
//! numerical accuracy, L1 norm mass conservation, dangling node redistribution,
//! damping factor convergence rates, and disconnected component isolation.

use memfuse_core::{Edge, Entity, EntityId, GraphIndex, PprConfig, TxId};
use memfuse_graph::csr::CsrGraph;
use std::collections::HashMap;

/// Independent Power Method Reference PPR Implementation (Anti-Mirroring).
///
/// Computes Personalized PageRank over an adjacency matrix $A$ where $A_{ij}$ is
/// the weight of edge from node $i$ to node $j$.
struct PowerMethodPprReference {
    n: usize,
    seed_nodes: Vec<usize>,
    adj_weights: Vec<Vec<f32>>,
}

impl PowerMethodPprReference {
    fn new(n: usize, seed_nodes: &[usize]) -> Self {
        Self {
            n,
            seed_nodes: seed_nodes.to_vec(),
            adj_weights: vec![vec![0.0; n]; n],
        }
    }

    fn add_edge(&mut self, u: usize, v: usize, weight: f32) {
        if u < self.n && v < self.n {
            self.adj_weights[u][v] += weight;
        }
    }

    /// Solves PPR using standard Matrix Power Iteration:
    /// $\mathbf{r}^{(k+1)} = d \mathbf{r}^{(k)} P + \left((1-d) + d \sum_{i \in \mathcal{D}} r_i^{(k)}\right) \mathbf{p}$
    ///
    /// Returns (rank_vector, iterations_run).
    fn solve(&self, damping_factor: f32, max_iters: usize, tol: f32) -> (Vec<f32>, usize) {
        if self.n == 0 || self.seed_nodes.is_empty() {
            return (Vec::new(), 0);
        }

        let d = damping_factor;

        // Teleport/restart vector p
        let mut p = vec![0.0f32; self.n];
        let restart_prob = 1.0 / (self.seed_nodes.len() as f32);
        for &seed in &self.seed_nodes {
            p[seed] = restart_prob;
        }

        // Outgoing weight sums
        let mut row_sums = vec![0.0f32; self.n];
        for i in 0..self.n {
            row_sums[i] = self.adj_weights[i].iter().sum();
        }

        let mut r = p.clone();
        let mut iters_run = 0;

        for iter in 1..=max_iters {
            iters_run = iter;
            let mut next_r = vec![0.0f32; self.n];

            // 1. Accumulate dangling node rank mass
            let mut dangling_mass = 0.0f32;
            for i in 0..self.n {
                if row_sums[i] == 0.0 {
                    dangling_mass += r[i];
                }
            }

            // 2. Uniform restart teleport mass + redistributed dangling mass
            let teleport_mass = (1.0 - d) + d * dangling_mass;
            for &seed in &self.seed_nodes {
                next_r[seed] += teleport_mass * restart_prob;
            }

            // 3. Distribute rank mass across transition matrix P
            for i in 0..self.n {
                if row_sums[i] > 0.0 && r[i] > 0.0 {
                    let share = d * r[i] / row_sums[i];
                    for j in 0..self.n {
                        let w = self.adj_weights[i][j];
                        if w > 0.0 {
                            next_r[j] += share * w;
                        }
                    }
                }
            }

            // Check L1 norm convergence diff
            let diff: f32 = r.iter().zip(next_r.iter()).map(|(a, b)| (a - b).abs()).sum();
            r = next_r;

            if diff < tol {
                break;
            }
        }

        (r, iters_run)
    }
}

#[tokio::test]
async fn test_ppr_numerical_verification_against_power_method_reference() {
    // 7-node benchmark graph with loops and cross links
    let n = 7;
    let seed_nodes = vec![0];
    let mut ref_ppr = PowerMethodPprReference::new(n, &seed_nodes);

    let graph = CsrGraph::new();
    let tx = TxId::new(1);

    for i in 0..n {
        graph
            .add_entity(
                tx,
                Entity::new(EntityId::new(i as u64 + 1), format!("N{i}"), "Node"),
            )
            .await
            .unwrap();
    }

    let edges = vec![
        (0, 1, 1.0),
        (0, 2, 2.0),
        (1, 3, 1.5),
        (2, 3, 0.5),
        (3, 4, 1.0),
        (4, 0, 0.5), // loop back to seed 0
        (4, 5, 1.0),
        (5, 6, 2.0), // node 6 is dangling
    ];

    for (u, v, w) in edges {
        graph
            .add_edge(
                tx,
                Edge::new(
                    EntityId::new(u as u64 + 1),
                    EntityId::new(v as u64 + 1),
                    "rel",
                )
                .with_weight(w),
            )
            .await
            .unwrap();
        ref_ppr.add_edge(u, v, w);
    }
    graph.commit(tx).await.unwrap();

    let damping = 0.85;
    let tol = 1e-6;
    let (ref_ranks, _ref_iters) = ref_ppr.solve(damping, 500, tol);

    let config = PprConfig {
        damping_factor: damping,
        max_iterations: 500,
        convergence_epsilon: tol,
        warn_on_non_convergence: true,
    };

    let ppr_res = graph
        .personalized_page_rank(&[EntityId::new(1)], &config)
        .await
        .unwrap();

    let impl_map: HashMap<u64, f32> = ppr_res.into_iter().map(|(id, r)| (id.inner(), r)).collect();

    println!("\n=== PPR Numerical Verification Table (7-node Graph) ===");
    println!("Node | Reference PPR | Impl PPR | Absolute Diff | Relative Dev");
    println!("---------------------------------------------------------------");

    for i in 0..n {
        let node_id = i as u64 + 1;
        let ref_val = ref_ranks[i];
        let impl_val = impl_map.get(&node_id).copied().unwrap_or(0.0);
        let abs_diff = (ref_val - impl_val).abs();
        let rel_dev = if ref_val > 0.0 {
            abs_diff / ref_val
        } else {
            0.0
        };

        println!(
            "{:4} | {:13.6} | {:8.6} | {:13.8} | {:12.6e}",
            node_id, ref_val, impl_val, abs_diff, rel_dev
        );

        assert!(
            rel_dev <= 1e-4,
            "PPR numerical deviation for node {node_id} exceeds tolerance 1e-4: ref={ref_val}, impl={impl_val}, rel_dev={rel_dev}"
        );
    }
}

#[tokio::test]
async fn test_dangling_nodes_explicit_mass_redistribution() {
    let graph = CsrGraph::new();
    let tx = TxId::new(1);

    // 1 -> 2 -> 3 (Node 3 is dangling / out-degree 0)
    for i in 1..=3 {
        graph
            .add_entity(tx, Entity::new(EntityId::new(i), format!("N{i}"), "Node"))
            .await
            .unwrap();
    }
    graph
        .add_edge(tx, Edge::new(EntityId::new(1), EntityId::new(2), "edge"))
        .await
        .unwrap();
    graph
        .add_edge(tx, Edge::new(EntityId::new(2), EntityId::new(3), "edge"))
        .await
        .unwrap();
    graph.commit(tx).await.unwrap();

    let config = PprConfig::default();
    let results = graph
        .personalized_page_rank(&[EntityId::new(1)], &config)
        .await
        .unwrap();

    let sum_mass: f32 = results.iter().map(|(_, r)| r).sum();
    assert!(
        (sum_mass - 1.0).abs() < 1e-4,
        "L1 Rank mass conservation failed with dangling node: {sum_mass}"
    );
}

#[tokio::test]
async fn test_convergence_across_damping_factors() {
    let n = 10;
    let mut ref_ppr = PowerMethodPprReference::new(n, &[0]);
    let graph = CsrGraph::new();
    let tx = TxId::new(1);

    for i in 0..n {
        graph
            .add_entity(
                tx,
                Entity::new(EntityId::new(i as u64 + 1), format!("N{i}"), "Node"),
            )
            .await
            .unwrap();
    }

    // Ring topology with cross edges
    for i in 0..n {
        let next = (i + 1) % n;
        graph
            .add_edge(
                tx,
                Edge::new(
                    EntityId::new(i as u64 + 1),
                    EntityId::new(next as u64 + 1),
                    "link",
                ),
            )
            .await
            .unwrap();
        ref_ppr.add_edge(i, next, 1.0);
    }
    graph.commit(tx).await.unwrap();

    println!("\n=== PPR Damping Factor Convergence Benchmarks ===");
    println!("Damping Factor | Iterations to Convergence (tol=1e-6)");
    println!("-----------------------------------------------------");

    for &damping in &[0.15f32, 0.50f32, 0.85f32] {
        let (_ref_ranks, iters) = ref_ppr.solve(damping, 1000, 1e-6);
        println!("{:14.2} | {:31}", damping, iters);

        let config = PprConfig {
            damping_factor: damping,
            max_iterations: 1000,
            convergence_epsilon: 1e-6,
            warn_on_non_convergence: true,
        };

        let res = graph
            .personalized_page_rank(&[EntityId::new(1)], &config)
            .await
            .unwrap();

        let total: f32 = res.iter().map(|(_, r)| r).sum();
        assert!((total - 1.0).abs() < 1e-4);
    }
}

#[tokio::test]
async fn test_isolated_node_and_disconnected_components() {
    let graph = CsrGraph::new();
    let tx = TxId::new(1);

    // Component A: Nodes 1, 2, 3
    for i in 1..=3 {
        graph
            .add_entity(tx, Entity::new(EntityId::new(i), format!("A{i}"), "Node"))
            .await
            .unwrap();
    }
    graph
        .add_edge(tx, Edge::new(EntityId::new(1), EntityId::new(2), "edge"))
        .await
        .unwrap();
    graph
        .add_edge(tx, Edge::new(EntityId::new(2), EntityId::new(3), "edge"))
        .await
        .unwrap();

    // Component B (disconnected from A): Nodes 10, 11
    for i in [10, 11] {
        graph
            .add_entity(tx, Entity::new(EntityId::new(i), format!("B{i}"), "Node"))
            .await
            .unwrap();
    }
    graph
        .add_edge(tx, Edge::new(EntityId::new(10), EntityId::new(11), "edge"))
        .await
        .unwrap();

    // Isolated Node 99 (no edges)
    graph
        .add_entity(tx, Entity::new(EntityId::new(99), "Iso", "Node"))
        .await
        .unwrap();

    graph.commit(tx).await.unwrap();

    // PPR seeded from Component A (Node 1)
    let config = PprConfig::default();
    let results = graph
        .personalized_page_rank(&[EntityId::new(1)], &config)
        .await
        .unwrap();

    let map: HashMap<u64, f32> = results.into_iter().map(|(id, r)| (id.inner(), r)).collect();

    // Component A nodes must have positive rank
    assert!(map.contains_key(&1) && map[&1] > 0.0);
    assert!(map.contains_key(&2) && map[&2] > 0.0);
    assert!(map.contains_key(&3) && map[&3] > 0.0);

    // Disconnected Component B & Isolated Node 99 must have score 0.0 (not present in results)
    assert!(!map.contains_key(&10), "Unreachable node 10 must have score 0");
    assert!(!map.contains_key(&11), "Unreachable node 11 must have score 0");
    assert!(!map.contains_key(&99), "Isolated node 99 must have score 0");

    let total: f32 = map.values().sum();
    assert!(
        (total - 1.0).abs() < 1e-4,
        "Total rank mass must sum to 1.0 within reachable component"
    );
}
