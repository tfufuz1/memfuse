//! Property-Based Testing for CsrGraph.
//!
//! Generates random sequence mutations (AddEntity, AddEdge, RemoveEdge, Commit, Rollback, Compact)
//! and asserts CSR structural invariants after every single operation and explicitly after compaction:
//! - Offsets array strictly monotonic non-decreasing
//! - Parallel array lengths (targets, weights, valid_froms, valid_tos) strictly equal
//! - After compaction (`graph.compact()`), offsets length == reverse_map length + 1
//! - After compaction, final offset == targets length

use memfuse_core::{Edge, Entity, EntityId, GraphIndex, TxId};
use memfuse_graph::csr::{CsrGraph, CsrGraphConfig};
use proptest::prelude::*;

#[derive(Debug, Clone)]
enum GraphMutation {
    AddEntity(u64),
    AddEdge(u64, u64, f32),
    RemoveEdge(u64, u64),
    Commit,
    Rollback,
    Compact,
}

fn graph_mutation_strategy() -> impl Strategy<Value = Vec<(u64, GraphMutation)>> {
    let single_mutation = prop_oneof![
        (1u64..30).prop_map(GraphMutation::AddEntity),
        (1u64..30, 1u64..30, 0.1f32..5.0f32).prop_map(|(u, v, w)| GraphMutation::AddEdge(u, v, w)),
        (1u64..30, 1u64..30).prop_map(|(u, v)| GraphMutation::RemoveEdge(u, v)),
        Just(GraphMutation::Commit),
        Just(GraphMutation::Rollback),
        Just(GraphMutation::Compact),
    ];

    proptest::collection::vec((1u64..10, single_mutation), 10..100)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn prop_csr_graph_random_sequence_structural_invariants(mutations in graph_mutation_strategy()) {
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();

        rt.block_on(async {
            let graph = CsrGraph::with_config(CsrGraphConfig {
                rebuild_threshold: 50,
            });

            for (tx_id_raw, mut_op) in mutations {
                let tx = TxId::new(tx_id_raw);

                match mut_op {
                    GraphMutation::AddEntity(id) => {
                        let _ = graph.add_entity(tx, Entity::new(EntityId::new(id), "E", "T")).await;
                    }
                    GraphMutation::AddEdge(u, v, w) => {
                        let _ = graph.add_edge(tx, Edge::new(EntityId::new(u), EntityId::new(v), "rel").with_weight(w)).await;
                    }
                    GraphMutation::RemoveEdge(u, v) => {
                        let _ = graph.remove_edge(tx, EntityId::new(u), EntityId::new(v)).await;
                    }
                    GraphMutation::Commit => {
                        let _ = graph.commit(tx).await;
                    }
                    GraphMutation::Rollback => {
                        let _ = graph.rollback(tx).await;
                    }
                    GraphMutation::Compact => {
                        graph.compact();
                    }
                }

                // Invariant Checks after every operation
                let inner = graph.inner_read();

                // 1. Offsets must ALWAYS be strictly non-decreasing
                for window in inner.offsets.windows(2) {
                    prop_assert!(window[0] <= window[1], "Offsets array must be monotonic non-decreasing");
                }

                // 2. Parallel CSR arrays must ALWAYS have equal lengths
                prop_assert_eq!(inner.targets.len(), inner.weights.len());
                prop_assert_eq!(inner.targets.len(), inner.valid_froms.len());
                prop_assert_eq!(inner.targets.len(), inner.valid_tos.len());
            }

            // Force full compact and check structural CSR invariants on compacted state
            graph.compact();
            let inner = graph.inner_read();
            prop_assert_eq!(inner.offsets.len(), inner.reverse_map.len() + 1, "Offsets length must equal reverse_map.len() + 1 after compact");
            prop_assert_eq!(*inner.offsets.last().unwrap(), inner.targets.len(), "Final offset must equal targets length");

            Ok(())
        }).unwrap();
    }
}
