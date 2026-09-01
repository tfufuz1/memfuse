//! Personalized PageRank (PPR) power iteration implementation for `CsrGraph`.

// FILE-CONTEXT
// STAND: 2026-08-30T18:53:58Z (SESSION: b1234567)
// ZWECK: Personalized PageRank Power Iteration über CSR Graph
// INVARIANTEN: inner MUSS vor Aufruf kompaktiert sein; bitidentischer Determinismus.
// HOTSPOTS: L30-L90 (Power Iteration Matrix-Vector Vector Multiplication)
// SIEHE AUCH: crates/memfuse-graph/src/csr.rs

use crate::csr::GraphInner;
use memfuse_core::{EntityId, PprConfig};
use std::collections::HashSet;

/// Calculates Personalized PageRank (PPR) over a compacted `GraphInner` state.
///
/// # Invarianten
/// - `inner` MUSS vor dem Aufruf kompaktiert sein (`inner.compact()`).
/// - Determinismus: Bitidentische Sortierung bei identischem Graph & Inputs.
/// - Zero-Hang: Bounded execution by `config.max_iterations`.
///
/// # Non-Convergence Behavior
/// Power iteration terminates early if the L1 norm diff drops below `config.convergence_epsilon`.
/// If `config.max_iterations` is reached without convergence, the intermediate best-effort rank
/// vector is returned (without returning an error) and a `tracing::warn!` message is logged with
/// `max_iterations`, `last_diff`, and `convergence_epsilon`.
pub(crate) fn compute_ppr(
    inner: &GraphInner,
    seed_nodes: &[EntityId],
    config: &PprConfig,
) -> Vec<(EntityId, f32)> {
    let n = inner.reverse_map.len();
    if n == 0 || seed_nodes.is_empty() {
        return Vec::new();
    }

    // 1. Identify valid seed internal indices (must exist and have committed entity)
    let mut valid_seeds = Vec::new();
    let mut seen_seeds = HashSet::new();

    for &seed in seed_nodes {
        if let Some(&idx) = inner.id_map.get(&seed) {
            if idx < n
                && inner.entities.get(idx).is_some_and(|e| e.is_some())
                && seen_seeds.insert(idx)
            {
                valid_seeds.push(idx);
            }
        }
    }

    if valid_seeds.is_empty() {
        return Vec::new();
    }

    // 2. Build restart / teleport vector p (uniform over valid seed nodes)
    let seed_count = valid_seeds.len() as f32;
    let restart_prob = 1.0 / seed_count;
    let mut p = vec![0.0f32; n];
    for &seed_idx in &valid_seeds {
        p[seed_idx] = restart_prob;
    }

    // 3. Precompute active outgoing edges & total weight per node
    //
    // ARCHITECTURE DECISION (MAJOR-05): Option B (Guaranteed Compact State)
    // Option B was chosen over Option A because Option A would require modifying `csr.rs` to make
    // `pending_edges` and `EdgePayload` `pub(crate)`, which is strictly prohibited (Sperrzone P08).
    // Option B ensures `compute_ppr()` operates on `GraphInner` after `compact()` has consolidated
    // all `pending_edges` into the CSR arrays (`offsets`, `targets`, `weights`), structurally
    // eliminating the window for uncompacted pending edges without modifying `csr.rs`.
    #[derive(Clone)]
    struct OutgoingEdge {
        target: usize,
        weight: f32,
    }

    let mut out_edges: Vec<Vec<OutgoingEdge>> = vec![Vec::new(); n];
    let mut out_weight_sums = vec![0.0f32; n];

    for i in 0..n {
        if !inner.entities.get(i).is_some_and(|e| e.is_some()) {
            continue;
        }

        let start = if i < inner.offsets.len() - 1 {
            inner.offsets[i]
        } else {
            0
        };
        let end = if i < inner.offsets.len() - 1 {
            inner.offsets[i + 1]
        } else {
            0
        };

        let mut sum = 0.0f32;
        let mut edges = Vec::with_capacity(end.saturating_sub(start));

        for edge_idx in start..end {
            let target = inner.targets[edge_idx];
            let weight = inner.weights[edge_idx];

            if !inner.tombstoned_edges.contains(&(i, target))
                && inner.entities.get(target).is_some_and(|e| e.is_some())
                && weight > 0.0
            {
                sum += weight;
                edges.push(OutgoingEdge { target, weight });
            }
        }

        out_weight_sums[i] = sum;
        out_edges[i] = edges;
    }

    // Validate config parameters defensively
    let _ = config.validate();

    let damping = if config.damping_factor.is_nan()
        || config.damping_factor <= 0.0
        || config.damping_factor >= 1.0
    {
        0.85
    } else {
        config.damping_factor
    };

    let max_iters = config.max_iterations.min(1000); // hard ceiling
    let epsilon = if config.convergence_epsilon.is_nan() || config.convergence_epsilon <= 0.0 {
        1e-6
    } else {
        config.convergence_epsilon
    };

    // 4. Power Iteration
    let mut ranks = p.clone();
    let mut last_diff = 0.0f32;
    let mut converged = false;

    for _iter in 0..max_iters {
        let mut next_ranks = vec![0.0f32; n];

        // Rank mass accumulated at dead-end (dangling) nodes
        let mut dangling_sum = 0.0f32;
        for i in 0..n {
            if inner.entities.get(i).is_some_and(|e| e.is_some()) && out_weight_sums[i] == 0.0 {
                dangling_sum += ranks[i];
            }
        }

        // Teleport / restart contribution (including redistributed dangling rank mass)
        let teleport_factor = (1.0 - damping) + damping * dangling_sum;
        for &seed_idx in &valid_seeds {
            next_ranks[seed_idx] += teleport_factor * restart_prob;
        }

        // Rank distribution across outgoing edges
        for i in 0..n {
            let sum_w = out_weight_sums[i];
            if sum_w > 0.0 && ranks[i] > 0.0 {
                let share = damping * ranks[i] / sum_w;
                for edge in &out_edges[i] {
                    next_ranks[edge.target] += share * edge.weight;
                }
            }
        }

        // Convergence check via L1 norm diff
        let diff: f32 = ranks
            .iter()
            .zip(next_ranks.iter())
            .map(|(a, b)| (a - b).abs())
            .sum();

        ranks = next_ranks;
        last_diff = diff;

        if diff < epsilon {
            converged = true;
            break;
        }
    }

    if !converged && config.warn_on_non_convergence {
        tracing::warn!(
            max_iterations = max_iters,
            last_diff = last_diff,
            convergence_epsilon = epsilon,
            "Personalized PageRank power iteration reached maximum iterations without reaching convergence; returning best-effort rank allocation"
        );
    }

    // 5. Build and sort result vector
    let mut results = Vec::new();
    for (idx, &rank) in ranks.iter().enumerate() {
        if rank > 0.0 && inner.entities.get(idx).is_some_and(|e| e.is_some()) {
            if let Some(&id) = inner.reverse_map.get(idx) {
                results.push((id, rank));
            }
        }
    }

    // Deterministic sort: score descending, tie-break by EntityId ascending
    results.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });

    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::csr::CsrGraph;
    use memfuse_core::{Edge, Entity, EntityId, GraphIndex, TxId};

    #[tokio::test]
    async fn test_ppr_analytical_5_node_ring() {
        let graph = CsrGraph::new();
        let tx = TxId::new(1);

        for i in 1..=5 {
            graph
                .add_entity(tx, Entity::new(EntityId::new(i), format!("N{i}"), "Node"))
                .await
                .unwrap(); // unwrap allowed
        }

        // 1 -> 2 -> 3 -> 4 -> 5 -> 1
        graph
            .add_edge(tx, Edge::new(EntityId::new(1), EntityId::new(2), "next"))
            .await
            .unwrap(); // unwrap allowed
        graph
            .add_edge(tx, Edge::new(EntityId::new(2), EntityId::new(3), "next"))
            .await
            .unwrap(); // unwrap allowed
        graph
            .add_edge(tx, Edge::new(EntityId::new(3), EntityId::new(4), "next"))
            .await
            .unwrap(); // unwrap allowed
        graph
            .add_edge(tx, Edge::new(EntityId::new(4), EntityId::new(5), "next"))
            .await
            .unwrap(); // unwrap allowed
        graph
            .add_edge(tx, Edge::new(EntityId::new(5), EntityId::new(1), "next"))
            .await
            .unwrap(); // unwrap allowed
        graph.commit(tx).await.unwrap(); // unwrap allowed

        let config = PprConfig {
            damping_factor: 0.85,
            max_iterations: 100,
            convergence_epsilon: 1e-6,
            warn_on_non_convergence: true,
        };

        let seed = EntityId::new(1);
        let results = graph
            .personalized_page_rank(&[seed], &config)
            .await
            .unwrap(); // unwrap allowed

        assert_eq!(results.len(), 5);

        // Analytical solution for 5-ring with seed = Node 1 and d = 0.85:
        // r_i = (1 - d) * d^(i-1) / (1 - d^5)
        let d = 0.85f32;
        let denom = 1.0 - d.powi(5);
        let expected_r1 = (1.0 - d) * 1.0 / denom; // ~0.26964
        let expected_r2 = (1.0 - d) * d / denom; // ~0.22920
        let expected_r3 = (1.0 - d) * d.powi(2) / denom; // ~0.19482
        let expected_r4 = (1.0 - d) * d.powi(3) / denom; // ~0.16559
        let expected_r5 = (1.0 - d) * d.powi(4) / denom; // ~0.14075

        let rank_map: std::collections::HashMap<EntityId, f32> = results.into_iter().collect();

        assert!((rank_map[&EntityId::new(1)] - expected_r1).abs() < 1e-3);
        assert!((rank_map[&EntityId::new(2)] - expected_r2).abs() < 1e-3);
        assert!((rank_map[&EntityId::new(3)] - expected_r3).abs() < 1e-3);
        assert!((rank_map[&EntityId::new(4)] - expected_r4).abs() < 1e-3);
        assert!((rank_map[&EntityId::new(5)] - expected_r5).abs() < 1e-3);

        // Monotonic order check
        assert!(rank_map[&EntityId::new(1)] > rank_map[&EntityId::new(2)]);
        assert!(rank_map[&EntityId::new(2)] > rank_map[&EntityId::new(3)]);
        assert!(rank_map[&EntityId::new(3)] > rank_map[&EntityId::new(4)]);
        assert!(rank_map[&EntityId::new(4)] > rank_map[&EntityId::new(5)]);
    }

    #[tokio::test]
    async fn test_ppr_handles_sink_node_correctly() {
        // Graph: A (1) -> B (2), B has NO outgoing edge (Sink), C (3) -> A (1)
        let graph = CsrGraph::new();
        let tx = TxId::new(1);

        let id_a = EntityId::new(1);
        let id_b = EntityId::new(2);
        let id_c = EntityId::new(3);

        graph
            .add_entity(tx, Entity::new(id_a, "Node A", "Node"))
            .await
            .unwrap();
        graph
            .add_entity(tx, Entity::new(id_b, "Node B (Sink)", "Node"))
            .await
            .unwrap();
        graph
            .add_entity(tx, Entity::new(id_c, "Node C", "Node"))
            .await
            .unwrap();

        // A -> B
        graph
            .add_edge(tx, Edge::new(id_a, id_b, "link"))
            .await
            .unwrap();
        // C -> A
        graph
            .add_edge(tx, Edge::new(id_c, id_a, "link"))
            .await
            .unwrap();
        graph.commit(tx).await.unwrap();

        let config = PprConfig::default();
        let results = graph
            .personalized_page_rank(&[id_a], &config)
            .await
            .unwrap();

        let total_mass: f32 = results.iter().map(|(_, score)| score).sum();
        assert!(
            (total_mass - 1.0).abs() < 1e-4,
            "PPR mass must conserve to 1.0 when graph contains sink node B, got {total_mass}"
        );

        let rank_map: std::collections::HashMap<EntityId, f32> = results.into_iter().collect();
        assert!(
            rank_map.contains_key(&id_b),
            "Sink node B must receive rank mass from A"
        );
        assert!(rank_map[&id_b] > 0.0, "Sink node B score must be positive");
    }

    #[tokio::test]
    async fn test_ppr_dangling_node_mass_conservation() {
        let graph = CsrGraph::new();
        let tx = TxId::new(1);

        for i in 1..=3 {
            graph
                .add_entity(tx, Entity::new(EntityId::new(i), format!("N{i}"), "Node"))
                .await
                .unwrap(); // unwrap allowed
        }

        // 1 -> 2 -> 3 (Node 3 has out-degree 0, dead-end/dangling)
        graph
            .add_edge(tx, Edge::new(EntityId::new(1), EntityId::new(2), "edge"))
            .await
            .unwrap(); // unwrap allowed
        graph
            .add_edge(tx, Edge::new(EntityId::new(2), EntityId::new(3), "edge"))
            .await
            .unwrap(); // unwrap allowed
        graph.commit(tx).await.unwrap(); // unwrap allowed

        let config = PprConfig::default();
        let results = graph
            .personalized_page_rank(&[EntityId::new(1)], &config)
            .await
            .unwrap(); // unwrap allowed

        let sum: f32 = results.iter().map(|(_, score)| score).sum();
        assert!(
            (sum - 1.0).abs() < 1e-4,
            "PPR rank mass must conserve to 1.0 despite dangling nodes, got {sum}"
        );
    }

    #[tokio::test]
    async fn test_ppr_bit_identical_determinism() {
        let graph = CsrGraph::new();
        let tx = TxId::new(1);

        for i in 1..=6 {
            graph
                .add_entity(tx, Entity::new(EntityId::new(i), format!("N{i}"), "Node"))
                .await
                .unwrap(); // unwrap allowed
        }

        graph
            .add_edge(tx, Edge::new(EntityId::new(1), EntityId::new(2), "rel"))
            .await
            .unwrap(); // unwrap allowed
        graph
            .add_edge(tx, Edge::new(EntityId::new(1), EntityId::new(3), "rel"))
            .await
            .unwrap(); // unwrap allowed
        graph
            .add_edge(tx, Edge::new(EntityId::new(2), EntityId::new(4), "rel"))
            .await
            .unwrap(); // unwrap allowed
        graph
            .add_edge(tx, Edge::new(EntityId::new(3), EntityId::new(4), "rel"))
            .await
            .unwrap(); // unwrap allowed
        graph
            .add_edge(tx, Edge::new(EntityId::new(4), EntityId::new(5), "rel"))
            .await
            .unwrap(); // unwrap allowed
        graph
            .add_edge(tx, Edge::new(EntityId::new(5), EntityId::new(6), "rel"))
            .await
            .unwrap(); // unwrap allowed
        graph.commit(tx).await.unwrap(); // unwrap allowed

        let config = PprConfig::default();
        let run1 = graph
            .personalized_page_rank(&[EntityId::new(1)], &config)
            .await
            .unwrap(); // unwrap allowed
        let run2 = graph
            .personalized_page_rank(&[EntityId::new(1)], &config)
            .await
            .unwrap(); // unwrap allowed

        assert_eq!(run1.len(), run2.len());
        for (a, b) in run1.iter().zip(run2.iter()) {
            assert_eq!(a.0, b.0);
            assert_eq!(
                a.1.to_bits(),
                b.1.to_bits(),
                "Float scores must be bit-identical across runs"
            );
        }
    }

    #[tokio::test]
    async fn test_ppr_single_node_no_edges() {
        let graph = CsrGraph::new();
        let tx = TxId::new(1);
        let seed = EntityId::new(42);

        graph
            .add_entity(tx, Entity::new(seed, "SoleNode", "Node"))
            .await
            .unwrap();
        graph.commit(tx).await.unwrap();

        let config = PprConfig::default();
        let results = graph
            .personalized_page_rank(&[seed], &config)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, seed);
        assert!((results[0].1 - 1.0).abs() < 1e-4);
    }

    #[tokio::test]
    async fn test_ppr_isolated_nodes_handling() {
        let graph = CsrGraph::new();
        let tx = TxId::new(1);

        for i in 1..=4 {
            graph
                .add_entity(tx, Entity::new(EntityId::new(i), format!("N{i}"), "Node"))
                .await
                .unwrap();
        }

        // Only 1 -> 2 edge. 3 and 4 are completely isolated.
        graph
            .add_edge(tx, Edge::new(EntityId::new(1), EntityId::new(2), "link"))
            .await
            .unwrap();
        graph.commit(tx).await.unwrap();

        let config = PprConfig::default();
        let results = graph
            .personalized_page_rank(&[EntityId::new(1)], &config)
            .await
            .unwrap();

        assert!(!results.is_empty());
        for (_, rank) in &results {
            assert!(!rank.is_nan(), "Rank must not be NaN");
            assert!(!rank.is_infinite(), "Rank must not be infinite");
        }

        let total_mass: f32 = results.iter().map(|(_, r)| r).sum();
        assert!(
            (total_mass - 1.0).abs() < 1e-4,
            "Total rank mass must conserve to 1.0 even with isolated nodes, got {total_mass}"
        );
    }

    #[tokio::test]
    async fn test_ppr_self_loop_handling() {
        let graph = CsrGraph::new();
        let tx = TxId::new(1);

        graph
            .add_entity(tx, Entity::new(EntityId::new(1), "N1", "Node"))
            .await
            .unwrap();
        graph
            .add_entity(tx, Entity::new(EntityId::new(2), "N2", "Node"))
            .await
            .unwrap();

        // 1 -> 1 (self-loop) and 1 -> 2
        graph
            .add_edge(tx, Edge::new(EntityId::new(1), EntityId::new(1), "self"))
            .await
            .unwrap();
        graph
            .add_edge(tx, Edge::new(EntityId::new(1), EntityId::new(2), "link"))
            .await
            .unwrap();
        graph.commit(tx).await.unwrap();

        let config = PprConfig::default();
        let results = graph
            .personalized_page_rank(&[EntityId::new(1)], &config)
            .await
            .unwrap();

        assert_eq!(results.len(), 2);
        for (_, rank) in &results {
            assert!(!rank.is_nan());
        }
        let total_mass: f32 = results.iter().map(|(_, r)| r).sum();
        assert!((total_mass - 1.0).abs() < 1e-4);
    }

    #[tokio::test]
    async fn test_ppr_duplicate_multi_edges_deterministic() {
        let graph = CsrGraph::new();
        let tx = TxId::new(1);

        graph
            .add_entity(tx, Entity::new(EntityId::new(1), "N1", "Node"))
            .await
            .unwrap();
        graph
            .add_entity(tx, Entity::new(EntityId::new(2), "N2", "Node"))
            .await
            .unwrap();

        // Add 3 duplicate edges 1 -> 2 with weights 1.0, 2.0, 3.0
        graph
            .add_edge(
                tx,
                Edge::new(EntityId::new(1), EntityId::new(2), "edge1").with_weight(1.0),
            )
            .await
            .unwrap();
        graph
            .add_edge(
                tx,
                Edge::new(EntityId::new(1), EntityId::new(2), "edge2").with_weight(2.0),
            )
            .await
            .unwrap();
        graph
            .add_edge(
                tx,
                Edge::new(EntityId::new(1), EntityId::new(2), "edge3").with_weight(3.0),
            )
            .await
            .unwrap();
        graph.commit(tx).await.unwrap();

        let config = PprConfig::default();
        let res1 = graph
            .personalized_page_rank(&[EntityId::new(1)], &config)
            .await
            .unwrap();
        let res2 = graph
            .personalized_page_rank(&[EntityId::new(1)], &config)
            .await
            .unwrap();

        assert_eq!(res1.len(), res2.len());
        for (a, b) in res1.iter().zip(res2.iter()) {
            assert_eq!(a.0, b.0);
            assert_eq!(a.1.to_bits(), b.1.to_bits());
        }
    }

    #[tokio::test]
    async fn test_ppr_exact_score_tie_breaking_by_entity_id() {
        let graph = CsrGraph::new();
        let tx = TxId::new(1);

        // Seed node 1, and symmetric nodes 10, 20 connected identically
        graph
            .add_entity(tx, Entity::new(EntityId::new(1), "Center", "Node"))
            .await
            .unwrap();
        graph
            .add_entity(tx, Entity::new(EntityId::new(20), "B", "Node"))
            .await
            .unwrap();
        graph
            .add_entity(tx, Entity::new(EntityId::new(10), "A", "Node"))
            .await
            .unwrap();

        // 1 -> 10 and 1 -> 20 with identical weight 1.0
        graph
            .add_edge(
                tx,
                Edge::new(EntityId::new(1), EntityId::new(10), "link").with_weight(1.0),
            )
            .await
            .unwrap();
        graph
            .add_edge(
                tx,
                Edge::new(EntityId::new(1), EntityId::new(20), "link").with_weight(1.0),
            )
            .await
            .unwrap();
        graph.commit(tx).await.unwrap();

        let config = PprConfig::default();
        let results = graph
            .personalized_page_rank(&[EntityId::new(1)], &config)
            .await
            .unwrap();

        let rank_10 = results
            .iter()
            .find(|(id, _)| *id == EntityId::new(10))
            .map(|(_, r)| *r)
            .unwrap();
        let rank_20 = results
            .iter()
            .find(|(id, _)| *id == EntityId::new(20))
            .map(|(_, r)| *r)
            .unwrap();

        assert_eq!(
            rank_10, rank_20,
            "Symmetric nodes must have identical PPR scores"
        );

        // Results order must sort tie by EntityId ascending (10 before 20)
        let idx_10 = results
            .iter()
            .position(|(id, _)| *id == EntityId::new(10))
            .unwrap();
        let idx_20 = results
            .iter()
            .position(|(id, _)| *id == EntityId::new(20))
            .unwrap();
        assert!(
            idx_10 < idx_20,
            "Tie-breaking must place EntityId(10) before EntityId(20)"
        );
    }

    proptest::proptest! {
        #[test]
        fn prop_ppr_rank_mass_conservation(
            node_count in 1..=20usize,
            edge_specs in proptest::collection::vec((0..20usize, 0..20usize, 0.1f32..5.0f32), 0..40),
            seed_idx in 0..20usize,
            damping in 0.1f32..0.99f32,
            max_iters in 1..=50u32,
        ) {
            let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
            let res: Result<(), proptest::test_runner::TestCaseError> = rt.block_on(async {
                let graph = CsrGraph::new();
                let tx = TxId::new(1);

                for i in 0..node_count {
                    graph
                        .add_entity(tx, Entity::new(EntityId::new(i as u64 + 1), format!("N{i}"), "Node"))
                        .await
                        .unwrap();
                }

                for (src, dst, w) in edge_specs {
                    let src_id = EntityId::new((src % node_count) as u64 + 1);
                    let dst_id = EntityId::new((dst % node_count) as u64 + 1);
                    graph
                        .add_edge(tx, Edge::new(src_id, dst_id, "link").with_weight(w))
                        .await
                        .unwrap();
                }
                graph.commit(tx).await.unwrap();

                let actual_seed = EntityId::new((seed_idx % node_count) as u64 + 1);
                let config = PprConfig {
                    damping_factor: damping,
                    max_iterations: max_iters,
                    convergence_epsilon: 1e-7,
                    warn_on_non_convergence: true,
                };

                let results = graph
                    .personalized_page_rank(&[actual_seed], &config)
                    .await
                    .unwrap();

                let total_mass: f32 = results.iter().map(|(_, r)| r).sum();
                proptest::prop_assert!(
                    (total_mass - 1.0).abs() < 1e-4,
                    "Rank mass conservation failed: total mass {} != 1.0 for node_count={}, seed={:?}",
                    total_mass,
                    node_count,
                    actual_seed
                );
                Ok(())
            });
            res?;
        }
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
            self.0.lock().unwrap().push(visitor.0); // unwrap allowed
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
    async fn test_ppr_warn_on_non_convergence_suppressible() {
        let graph = CsrGraph::new();
        let tx = TxId::new(1);

        for i in 1..=5 {
            graph
                .add_entity(tx, Entity::new(EntityId::new(i), format!("N{i}"), "Node"))
                .await
                .unwrap();
        }

        for i in 1..=5 {
            let next = if i == 5 { 1 } else { i + 1 };
            graph
                .add_edge(tx, Edge::new(EntityId::new(i), EntityId::new(next), "next"))
                .await
                .unwrap();
        }
        graph.commit(tx).await.unwrap();

        // Config with max_iterations: 1 and warn_on_non_convergence: false
        let config = PprConfig {
            damping_factor: 0.85,
            max_iterations: 1,
            convergence_epsilon: 1e-12,
            warn_on_non_convergence: false,
        };

        let seed = EntityId::new(1);
        let results = graph
            .personalized_page_rank(&[seed], &config)
            .await
            .unwrap();

        assert!(
            !results.is_empty(),
            "Calculation must return best-effort result without error or panic when warn_on_non_convergence is false"
        );
    }

    #[tokio::test]
    async fn test_ppr_non_convergence_logs_warning_and_returns_best_effort() {
        use tracing_subscriber::layer::SubscriberExt;

        let logs = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let capture_layer = LogCaptureLayer(logs.clone());
        let subscriber = tracing_subscriber::registry().with(capture_layer);
        let _guard = tracing::subscriber::set_default(subscriber);

        let graph = CsrGraph::new();
        let tx = TxId::new(1);

        for i in 1..=5 {
            graph
                .add_entity(tx, Entity::new(EntityId::new(i), format!("N{i}"), "Node"))
                .await
                .unwrap(); // unwrap allowed
        }

        // Ring 1 -> 2 -> 3 -> 4 -> 5 -> 1
        for i in 1..=5 {
            let next = if i == 5 { 1 } else { i + 1 };
            graph
                .add_edge(tx, Edge::new(EntityId::new(i), EntityId::new(next), "next"))
                .await
                .unwrap(); // unwrap allowed
        }
        graph.commit(tx).await.unwrap(); // unwrap allowed

        // Set max_iterations very low (2) and convergence_epsilon very small (1e-12)
        // so that power iteration cannot reach convergence in 2 iterations
        let config = PprConfig {
            damping_factor: 0.85,
            max_iterations: 2,
            convergence_epsilon: 1e-12,
            warn_on_non_convergence: true,
        };

        let seed = EntityId::new(1);
        let results = graph
            .personalized_page_rank(&[seed], &config)
            .await
            .unwrap(); // unwrap allowed

        assert!(
            !results.is_empty(),
            "Best-effort results must be returned on non-convergence"
        );

        let captured = logs.lock().unwrap(); // unwrap allowed
        let warning_found = captured.iter().any(|msg| {
            msg.contains("Personalized PageRank power iteration reached maximum iterations without reaching convergence")
                && msg.contains("max_iterations=2")
                && msg.contains("convergence_epsilon=")
        });

        assert!(
            warning_found,
            "Expected structured warning log on PPR non-convergence, got logs: {:?}",
            *captured
        );
    }

    #[tokio::test]
    async fn test_ppr_pathological_max_iterations_ceiling() {
        let graph = CsrGraph::new();
        let tx = TxId::new(1);

        for i in 1..=4 {
            graph
                .add_entity(tx, Entity::new(EntityId::new(i), format!("N{i}"), "Node"))
                .await
                .unwrap(); // unwrap allowed
        }

        // Cycle 1 <-> 2 <-> 3 <-> 4
        graph
            .add_bidirectional(tx, EntityId::new(1), EntityId::new(2), "edge")
            .await
            .unwrap(); // unwrap allowed
        graph
            .add_bidirectional(tx, EntityId::new(2), EntityId::new(3), "edge")
            .await
            .unwrap(); // unwrap allowed
        graph
            .add_bidirectional(tx, EntityId::new(3), EntityId::new(4), "edge")
            .await
            .unwrap(); // unwrap allowed
        graph.commit(tx).await.unwrap(); // unwrap allowed

        let config = PprConfig {
            damping_factor: 0.85,
            max_iterations: 5,          // Capped to 5 iterations
            convergence_epsilon: 1e-15, // Unreachable tolerance forces iter cap
            warn_on_non_convergence: true,
        };

        let start_time = std::time::Instant::now();
        let results = graph
            .personalized_page_rank(&[EntityId::new(1)], &config)
            .await
            .unwrap(); // unwrap allowed
        let elapsed = start_time.elapsed();

        assert!(!results.is_empty());
        assert!(
            elapsed.as_millis() < 500,
            "Max iterations ceiling must terminate execution promptly without hanging"
        );
    }

    #[tokio::test]
    async fn test_ppr_includes_uncompacted_pending_edges() {
        let graph = CsrGraph::new();
        let tx = TxId::new(1);

        // Nodes 1, 2, 3
        let id1 = EntityId::new(1);
        let id2 = EntityId::new(2);
        let id3 = EntityId::new(3);

        graph.add_entity(tx, Entity::new(id1, "N1", "Node")).await.unwrap();
        graph.add_entity(tx, Entity::new(id2, "N2", "Node")).await.unwrap();
        graph.add_entity(tx, Entity::new(id3, "N3", "Node")).await.unwrap();

        // Initially only 1 -> 2 edge
        graph.add_edge(tx, Edge::new(id1, id2, "link")).await.unwrap();
        graph.commit(tx).await.unwrap();

        // Force compaction so 1 -> 2 is in CSR arrays
        graph.compact();

        // Add a NEW edge 1 -> 3 in tx2 and commit without compacting!
        // This edge resides in pending_edges (uncompacted delta buffer).
        let tx2 = TxId::new(2);
        graph.add_edge(tx2, Edge::new(id1, id3, "link")).await.unwrap();
        graph.commit(tx2).await.unwrap();

        // Calling personalized_page_rank compacts pending_edges under lock before PPR computation,
        // ensuring the uncompacted pending edge 1 -> 3 is consolidated into CSR arrays and evaluated.
        let config = PprConfig::default();
        let results = graph.personalized_page_rank(&[id1], &config).await.unwrap();

        let rank_map: std::collections::HashMap<EntityId, f32> = results.into_iter().collect();

        assert!(
            rank_map.contains_key(&id3),
            "Uncompacted pending edge 1 -> 3 must propagate rank mass to Node 3"
        );
        assert!(
            rank_map[&id3] > 0.0,
            "PPR score for Node 3 must be strictly positive due to pending edge, got {}",
            rank_map[&id3]
        );
    }

    #[tokio::test]
    async fn test_ppr_max_iterations_warning_log_above_1000() {
        use tracing_subscriber::layer::SubscriberExt;

        let logs = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let capture_layer = LogCaptureLayer(logs.clone());
        let subscriber = tracing_subscriber::registry().with(capture_layer);
        let _guard = tracing::subscriber::set_default(subscriber);

        let graph = CsrGraph::new();
        let tx = TxId::new(1);

        for i in 1..=3 {
            graph
                .add_entity(tx, Entity::new(EntityId::new(i), format!("N{i}"), "Node"))
                .await
                .unwrap();
        }
        graph
            .add_edge(tx, Edge::new(EntityId::new(1), EntityId::new(2), "link"))
            .await
            .unwrap();
        graph.commit(tx).await.unwrap();

        let config = PprConfig {
            damping_factor: 0.85,
            max_iterations: 5000,
            convergence_epsilon: 1e-6,
            warn_on_non_convergence: true,
        };

        let results = graph
            .personalized_page_rank(&[EntityId::new(1)], &config)
            .await
            .unwrap();

        assert!(!results.is_empty());

        let captured = logs.lock().unwrap();
        let warning_found = captured.iter().any(|msg| {
            msg.contains("PprConfig max_iterations exceeds hard cap of 1000")
                && msg.contains("max_iterations=5000")
                && msg.contains("capped_iterations=1000")
        });

        assert!(
            warning_found,
            "Expected warning log when max_iterations > 1000, got logs: {:?}",
            *captured
        );
    }
}
