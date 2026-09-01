use memfuse_core::{Edge, Entity, EntityId, GraphIndex, PprConfig, TxId};
use memfuse_graph::CsrGraph;
use std::collections::HashMap;

/// Helper function to create a graph entity with standard naming
async fn add_node(graph: &CsrGraph, tx: TxId, id: u64, name: &str) {
    graph
        .add_entity(tx, Entity::new(EntityId::new(id), name, "Node"))
        .await
        .expect("Failed to add entity");
}

/// Helper function to add a directed edge with weight 1.0
async fn add_edge(graph: &CsrGraph, tx: TxId, src: u64, dst: u64) {
    graph
        .add_edge(
            tx,
            Edge::new(EntityId::new(src), EntityId::new(dst), "link").with_weight(1.0),
        )
        .await
        .expect("Failed to add edge");
}

#[tokio::test]
async fn test_ppr_graph_1_single_dangling_node() {
    let graph = CsrGraph::new();
    let tx = TxId::new(1);

    // 10 nodes total
    for i in 1..=10 {
        add_node(&graph, tx, i, &format!("Node_{i}")).await;
    }

    // Well-connected 9-node core (1..=9): cycle + cross edges
    for i in 1..=9 {
        let next = if i == 9 { 1 } else { i + 1 };
        add_edge(&graph, tx, i, next).await;
    }
    // Cross edges in core
    add_edge(&graph, tx, 1, 3).await;
    add_edge(&graph, tx, 3, 7).await;
    add_edge(&graph, tx, 5, 2).await;
    add_edge(&graph, tx, 8, 4).await;

    // Edges pointing INTO Node 10 (Single Dangling Node, out-degree = 0)
    add_edge(&graph, tx, 1, 10).await;
    add_edge(&graph, tx, 5, 10).await;
    add_edge(&graph, tx, 9, 10).await;

    graph.commit(tx).await.expect("Commit failed");

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
        .expect("PPR execution failed");

    println!("=== GRAPH 1: SINGLE DANGLING NODE (10 Nodes, Node 10 Dangling) ===");
    let mut total_mass = 0.0f32;
    let score_map: HashMap<EntityId, f32> = results.into_iter().collect();

    for i in 1..=10 {
        let score = score_map.get(&EntityId::new(i)).copied().unwrap_or(0.0);
        total_mass += score;
        let is_dangling = if i == 10 { " [DANGLING]" } else { "" };
        println!("Node {:2}: {:.6}{}", i, score, is_dangling);
    }

    println!("Total Rank Mass Sum: {:.6}", total_mass);

    // Assertions
    // (a) Mass Conservation
    assert!(
        (total_mass - 1.0).abs() < 1e-4,
        "Graph 1 rank mass must conserve to 1.0, got {:.6}",
        total_mass
    );

    // (b) Non-dangling nodes must retain non-zero, differentiated scores
    for i in 1..=9 {
        let score = score_map[&EntityId::new(i)];
        assert!(
            score > 0.01,
            "Non-dangling Node {i} must retain meaningful score, got {score}"
        );
    }

    // Check score differentiation (max / min among non-dangling nodes)
    let min_core_score = (1..=9)
        .map(|i| score_map[&EntityId::new(i)])
        .fold(f32::INFINITY, f32::min);
    let max_core_score = (1..=9)
        .map(|i| score_map[&EntityId::new(i)])
        .fold(f32::NEG_INFINITY, f32::max);

    println!(
        "Core Score Range: min={:.6}, max={:.6}, ratio={:.2}",
        min_core_score,
        max_core_score,
        max_core_score / min_core_score
    );
    assert!(
        max_core_score / min_core_score > 1.2,
        "Core nodes must have differentiated scores"
    );
}

#[tokio::test]
async fn test_ppr_graph_2_extreme_90_percent_dangling_nodes() {
    let graph = CsrGraph::new();
    let tx = TxId::new(1);

    // 20 nodes total: 18 dangling nodes (3..=20, 90%), only 2 nodes (1, 2) have outgoing edges
    for i in 1..=20 {
        add_node(&graph, tx, i, &format!("Node_{i}")).await;
    }

    // Nodes 1 and 2 form connected core
    add_edge(&graph, tx, 1, 2).await;
    add_edge(&graph, tx, 2, 1).await;

    // Node 1 points to dangling nodes 3..=11
    for i in 3..=11 {
        add_edge(&graph, tx, 1, i).await;
    }

    // Node 2 points to dangling nodes 12..=20
    for i in 12..=20 {
        add_edge(&graph, tx, 2, i).await;
    }

    graph.commit(tx).await.expect("Commit failed");

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
        .expect("PPR execution failed");

    println!("\n=== GRAPH 2: EXTREME 90% DANGLING NODES (20 Nodes, 18 Dangling) ===");
    let mut total_mass = 0.0f32;
    let score_map: HashMap<EntityId, f32> = results.into_iter().collect();

    for i in 1..=20 {
        let score = score_map.get(&EntityId::new(i)).copied().unwrap_or(0.0);
        total_mass += score;
        let status = if i <= 2 { " [CORE]" } else { " [DANGLING]" };
        println!("Node {:2}: {:.6}{}", i, score, status);
    }

    println!("Total Rank Mass Sum: {:.6}", total_mass);

    // Assertions
    // (a) Mass Conservation
    assert!(
        (total_mass - 1.0).abs() < 1e-4,
        "Graph 2 rank mass must conserve to 1.0, got {:.6}",
        total_mass
    );

    // (b) Core non-dangling nodes 1 and 2 must retain distinct positive scores (no score collapse)
    let s1 = score_map[&EntityId::new(1)];
    let s2 = score_map[&EntityId::new(2)];
    assert!(
        s1 > 0.05,
        "Seed Node 1 must retain significant score despite 90% dangling graph, got {s1}"
    );
    assert!(
        s2 > 0.01,
        "Core Node 2 must retain significant score, got {s2}"
    );

    // Dangling nodes should also have positive scores redistributed from teleportation
    for i in 3..=20 {
        let score = score_map[&EntityId::new(i)];
        assert!(
            score > 0.0,
            "Dangling Node {i} must receive positive score, got {score}"
        );
    }
}

#[tokio::test]
async fn test_ppr_graph_3_group_of_dangling_nodes() {
    let graph = CsrGraph::new();
    let tx = TxId::new(1);

    // 12 nodes total: 9 core nodes (1..=9), 3 connected group dangling nodes (10, 11, 12)
    for i in 1..=12 {
        add_node(&graph, tx, i, &format!("Node_{i}")).await;
    }

    // Core 1..=9 cycle
    for i in 1..=9 {
        let next = if i == 9 { 1 } else { i + 1 };
        add_edge(&graph, tx, i, next).await;
    }

    // Group of dangling nodes: Nodes 10, 11, 12 have NO outgoing edges.
    // Core feeds into group
    add_edge(&graph, tx, 3, 10).await;
    add_edge(&graph, tx, 6, 11).await;
    add_edge(&graph, tx, 9, 12).await;

    graph.commit(tx).await.expect("Commit failed");

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
        .expect("PPR execution failed");

    println!(
        "\n=== GRAPH 3: GROUP OF DANGLING NODES (12 Nodes, Nodes 10,11,12 Dangling Group) ==="
    );
    let mut total_mass = 0.0f32;
    let score_map: HashMap<EntityId, f32> = results.into_iter().collect();

    for i in 1..=12 {
        let score = score_map.get(&EntityId::new(i)).copied().unwrap_or(0.0);
        total_mass += score;
        let status = if i >= 10 {
            " [DANGLING GROUP]"
        } else {
            " [CORE]"
        };
        println!("Node {:2}: {:.6}{}", i, score, status);
    }

    println!("Total Rank Mass Sum: {:.6}", total_mass);

    // Assertions
    // (a) Mass Conservation
    assert!(
        (total_mass - 1.0).abs() < 1e-4,
        "Graph 3 rank mass must conserve to 1.0, got {:.6}",
        total_mass
    );

    // (b) Score collapse check
    for i in 1..=9 {
        let score = score_map[&EntityId::new(i)];
        assert!(
            score > 0.01,
            "Core Node {i} must maintain positive score, got {score}"
        );
    }

    for i in 10..=12 {
        let score = score_map[&EntityId::new(i)];
        assert!(
            score > 0.005,
            "Dangling Group Node {i} must receive rank mass, got {score}"
        );
    }
}
