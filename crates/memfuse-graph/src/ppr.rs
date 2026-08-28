//! Personalized PageRank (PPR) power iteration implementation for `CsrGraph`.

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
/// # Konvergenzverhalten
/// Erreicht die Power-Iteration `max_iterations` ohne dass die L1-Norm-Differenz
/// unter `convergence_epsilon` fällt, wird das aktuell beste Zwischenergebnis
/// (Best-Effort-Ranking) zurückgegeben. Bei Nicht-Konvergenz wird eine `tracing::warn!`-Zeile
/// mit `max_iterations`, der tatsächlichen L1-Norm-Differenz und `convergence_epsilon` geloggt.
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

            if inner.entities.get(target).is_some_and(|e| e.is_some()) && weight > 0.0 {
                sum += weight;
                edges.push(OutgoingEdge { target, weight });
            }
        }

        out_weight_sums[i] = sum;
        out_edges[i] = edges;
    }

    // Validate config parameters defensively
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
    let mut converged = false;
    let mut final_diff = 0.0f32;

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
        final_diff = diff;

        if diff < epsilon {
            converged = true;
            break;
        }
    }

    if !converged {
        tracing::warn!(
            max_iterations = max_iters,
            l1_diff = final_diff,
            convergence_epsilon = epsilon,
            "Personalized PageRank reached max_iterations without converging; returning best-effort result"
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
    async fn test_ppr_non_convergence_warning_logged() {
        use std::sync::{Arc, Mutex};
        use tracing_subscriber::layer::SubscriberExt;

        #[derive(Clone, Default)]
        struct LogCapture(Arc<Mutex<Vec<String>>>);

        struct CapturingWriter(LogCapture);
        impl std::io::Write for CapturingWriter {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                let msg = String::from_utf8_lossy(buf).to_string();
                self.0 .0.lock().unwrap().push(msg);
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        let capture = LogCapture::default();
        let capture_clone = capture.clone();

        let layer = tracing_subscriber::fmt::layer()
            .with_writer(move || CapturingWriter(capture_clone.clone()));
        let subscriber = tracing_subscriber::registry().with(layer);

        let graph = CsrGraph::new();
        let tx = TxId::new(1);

        // Long linear chain of 500 nodes to slow down PPR diffusion
        let num_nodes = 500;
        for i in 1..=num_nodes {
            graph
                .add_entity(tx, Entity::new(EntityId::new(i), format!("N{i}"), "Node"))
                .await
                .unwrap();
        }
        for i in 1..num_nodes {
            graph
                .add_edge(
                    tx,
                    Edge::new(EntityId::new(i), EntityId::new(i + 1), "next"),
                )
                .await
                .unwrap();
        }
        graph.commit(tx).await.unwrap();

        // Low max_iterations (2) and strict epsilon (1e-12) to guarantee non-convergence within max_iterations
        let config = PprConfig {
            damping_factor: 0.85,
            max_iterations: 2,
            convergence_epsilon: 1e-12,
        };

        // Call compact() before compute_ppr
        graph.compact();

        let start_time = std::time::Instant::now();
        let results = {
            let _guard = tracing::subscriber::set_default(subscriber);
            compute_ppr(&graph.inner_read(), &[EntityId::new(1)], &config)
        };
        let elapsed = start_time.elapsed();

        // 1. Terminated within max_iterations without infinite loop
        assert!(!results.is_empty());
        assert!(elapsed.as_secs() < 5);

        // 2. Result is deterministic across repeated calls
        let results_retry = compute_ppr(&graph.inner_read(), &[EntityId::new(1)], &config);
        assert_eq!(results.len(), results_retry.len());
        for ((id1, s1), (id2, s2)) in results.iter().zip(results_retry.iter()) {
            assert_eq!(id1, id2);
            assert_eq!(s1.to_bits(), s2.to_bits());
        }

        // 3. Verify warn log was emitted containing non-convergence details
        let logs = capture.0.lock().unwrap();
        let warn_log = logs
            .iter()
            .find(|l| l.contains("Personalized PageRank reached max_iterations"));
        assert!(
            warn_log.is_some(),
            "tracing::warn! log for PPR non-convergence must be emitted. Captured logs: {logs:?}"
        );
        let log_text = warn_log.unwrap();
        assert!(log_text.contains("max_iterations") || log_text.contains("20"));
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
}
