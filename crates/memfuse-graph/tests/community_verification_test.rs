//! Community Detection Verification Test Suite.
//!
//! Verifies Label Propagation Community Detection against ground-truth synthetic structures,
//! random Erdős–Rényi graphs, and fully connected graphs.

use memfuse_core::{Edge, Entity, EntityId, GraphIndex, TxId};
use memfuse_graph::{detect_communities, CommunityDetectionConfig, CsrGraph};
use std::collections::HashMap;

#[tokio::test]
async fn test_community_detection_ground_truth_two_clusters_with_bridge() {
    let graph = CsrGraph::new();
    let tx = TxId::new(1);

    // Cluster 1: Nodes 1, 2, 3, 4 (dense clique K_4)
    for id in 1..=4 {
        graph
            .add_entity(
                tx,
                Entity::new(EntityId::new(id), format!("C1_{id}"), "Type"),
            )
            .await
            .unwrap();
    }
    for i in 1..=4 {
        for j in (i + 1)..=4 {
            graph
                .add_edge(
                    tx,
                    Edge::new(EntityId::new(i), EntityId::new(j), "link").with_weight(1.0),
                )
                .await
                .unwrap();
            graph
                .add_edge(
                    tx,
                    Edge::new(EntityId::new(j), EntityId::new(i), "link").with_weight(1.0),
                )
                .await
                .unwrap();
        }
    }

    // Cluster 2: Nodes 10, 11, 12, 13 (dense clique K_4)
    for id in 10..=13 {
        graph
            .add_entity(
                tx,
                Entity::new(EntityId::new(id), format!("C2_{id}"), "Type"),
            )
            .await
            .unwrap();
    }
    for i in 10..=13 {
        for j in (i + 1)..=13 {
            graph
                .add_edge(
                    tx,
                    Edge::new(EntityId::new(i), EntityId::new(j), "link").with_weight(1.0),
                )
                .await
                .unwrap();
            graph
                .add_edge(
                    tx,
                    Edge::new(EntityId::new(j), EntityId::new(i), "link").with_weight(1.0),
                )
                .await
                .unwrap();
        }
    }

    // Weak Bridge Edge between Cluster 1 and Cluster 2 (Node 4 <-> Node 10 with low weight 0.1)
    graph
        .add_edge(
            tx,
            Edge::new(EntityId::new(4), EntityId::new(10), "bridge").with_weight(0.1),
        )
        .await
        .unwrap();
    graph
        .add_edge(
            tx,
            Edge::new(EntityId::new(10), EntityId::new(4), "bridge").with_weight(0.1),
        )
        .await
        .unwrap();

    graph.commit(tx).await.unwrap();

    let config = CommunityDetectionConfig {
        max_iterations: 100,
        seed: 42,
    };

    let assignments = detect_communities(&graph, &config).await.unwrap();
    assert_eq!(assignments.len(), 8);

    let map: HashMap<u64, u64> = assignments
        .into_iter()
        .map(|a| (a.entity_id.inner(), a.community_id))
        .collect();

    let c1_community = map[&1];
    assert_eq!(map[&2], c1_community, "Node 2 must belong to Cluster 1");
    assert_eq!(map[&3], c1_community, "Node 3 must belong to Cluster 1");
    assert_eq!(map[&4], c1_community, "Node 4 must belong to Cluster 1");

    let c2_community = map[&10];
    assert_eq!(map[&11], c2_community, "Node 11 must belong to Cluster 2");
    assert_eq!(map[&12], c2_community, "Node 12 must belong to Cluster 2");
    assert_eq!(map[&13], c2_community, "Node 13 must belong to Cluster 2");

    assert_ne!(
        c1_community, c2_community,
        "Cluster 1 and Cluster 2 separated by a weak bridge must be assigned distinct communities"
    );
}

#[tokio::test]
async fn test_community_detection_complete_graph_single_community() {
    let graph = CsrGraph::new();
    let tx = TxId::new(1);
    let n = 8;

    for i in 1..=n {
        graph
            .add_entity(
                tx,
                Entity::new(EntityId::new(i), format!("N{i}"), "Type"),
            )
            .await
            .unwrap();
    }

    // K_8 fully connected
    for i in 1..=n {
        for j in 1..=n {
            if i != j {
                graph
                    .add_edge(
                        tx,
                        Edge::new(EntityId::new(i), EntityId::new(j), "link"),
                    )
                    .await
                    .unwrap();
            }
        }
    }
    graph.commit(tx).await.unwrap();

    let config = CommunityDetectionConfig::default();
    let assignments = detect_communities(&graph, &config).await.unwrap();

    assert_eq!(assignments.len(), n as usize);

    let first_community = assignments[0].community_id;
    for a in &assignments {
        assert_eq!(
            a.community_id, first_community,
            "In a fully connected graph K_{n}, all nodes must converge to a SINGLE community"
        );
    }
}

#[tokio::test]
async fn test_community_detection_random_graph_graceful_assignment() {
    let graph = CsrGraph::new();
    let tx = TxId::new(1);
    let n = 20;

    for i in 1..=n {
        graph
            .add_entity(
                tx,
                Entity::new(EntityId::new(i), format!("N{i}"), "Type"),
            )
            .await
            .unwrap();
    }

    // Erdős–Rényi random sparse connections
    let mut state = 987654321u64;
    for i in 1..=n {
        for j in 1..=n {
            if i != j {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                let rand_val = (state as f64) / (u64::MAX as f64);
                if rand_val < 0.15 {
                    graph
                        .add_edge(
                            tx,
                            Edge::new(EntityId::new(i), EntityId::new(j), "link"),
                        )
                        .await
                        .unwrap();
                }
            }
        }
    }
    graph.commit(tx).await.unwrap();

    let config = CommunityDetectionConfig {
        max_iterations: 30,
        seed: 123,
    };

    let assignments = detect_communities(&graph, &config).await.unwrap();
    assert_eq!(
        assignments.len(),
        n as usize,
        "Every node in random graph must receive a community assignment"
    );
}
