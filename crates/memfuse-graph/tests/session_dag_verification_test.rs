//! Session-DAG Verification and Concurrency Audit Test Suite.
//!
//! Verifies acyclicity via Kahn's cycle detection algorithm after randomized branching stress tests,
//! active_head consistency under parallel branch switches, lock hierarchy safety under opposing
//! access patterns (deadlock detection), and persistence roundtrips.

use memfuse_core::{StorageEngine, TxId};
use memfuse_graph::{NodeIdx, SessionBranchTree};
use memfuse_store::{LsmConfig, LsmStorage};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

/// Kahn's algorithm for explicit DAG cycle detection.
/// Returns true if the graph contains NO cycles (is strictly acyclic).
fn is_acyclic(tree: &SessionBranchTree) -> bool {
    let mut in_degree: HashMap<NodeIdx, usize> = HashMap::new();
    let mut adj: HashMap<NodeIdx, Vec<NodeIdx>> = HashMap::new();
    let total_nodes = tree.node_count();

    // Collect all nodes and outgoing edges
    for id in 0..(total_nodes as u64) {
        in_degree.entry(id).or_insert(0);
        let children = tree.children_of(id);
        for &child in &children {
            *in_degree.entry(child).or_insert(0) += 1;
            adj.entry(id).or_default().push(child);
        }
    }

    // Queue of nodes with in-degree 0 (roots)
    let mut queue: VecDeque<NodeIdx> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(&id, _)| id)
        .collect();

    let mut visited_count = 0usize;

    while let Some(u) = queue.pop_front() {
        visited_count += 1;
        if let Some(neighbors) = adj.get(&u) {
            for &v in neighbors {
                if let Some(deg) = in_degree.get_mut(&v) {
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(v);
                    }
                }
            }
        }
    }

    visited_count == total_nodes
}

#[tokio::test]
async fn test_session_dag_branch_stress_and_acyclicity_proof() {
    let dag = Arc::new(SessionBranchTree::new("Root".into(), "Root Resp".into()));

    let mut state = 123456789u64;
    let mut next_rand = || {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        state
    };

    // Perform 200 random branch creation operations
    let mut created_nodes = vec![0u64];

    for i in 1..=200 {
        let parent_idx = (next_rand() as usize) % created_nodes.len();
        let parent = created_nodes[parent_idx];

        let new_id = dag
            .branch_from(
                parent,
                format!("Prompt step {i}"),
                format!("Resp step {i}"),
                Some(TxId::new(i as u64)),
                vec![format!("tool_{i}")],
                "grok_branch",
            )
            .unwrap();

        created_nodes.push(new_id);
    }

    assert_eq!(dag.node_count(), 201);

    // ABNAHMEKRITERIUM: Explizite Zyklen-Detektion beweist Azyklizität nach Stress
    let acyclic = is_acyclic(&dag);
    assert!(
        acyclic,
        "Session-DAG must remain strictly acyclic after 200 randomized branch operations"
    );
}

#[tokio::test]
async fn test_active_head_consistency_under_concurrent_branch_switches() {
    let dag = Arc::new(SessionBranchTree::new("Root".into(), "Root Resp".into()));

    // Create 10 target nodes
    let mut target_nodes = Vec::new();
    for i in 1..=10 {
        let id = dag
            .append_step(format!("P{i}"), format!("R{i}"), None, vec![], "main")
            .unwrap();
        target_nodes.push(id);
    }

    let mut handles = Vec::new();

    // Spawn 10 concurrent tasks continuously switching active head
    for task_idx in 0..10 {
        let d = dag.clone();
        let targets = target_nodes.clone();
        handles.push(tokio::spawn(async move {
            for i in 0..100 {
                let target = targets[(task_idx + i) % targets.len()];
                d.set_active_head(target).unwrap();
                let head = d.active_head();
                // Active head must always point to a valid node in targets or 0
                assert!(head <= 10, "Invalid active head index observed: {head}");
                tokio::task::yield_now().await;
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    let final_head = dag.active_head();
    assert!(
        final_head <= 10,
        "Final active head must remain valid after concurrent switches"
    );
    let path = dag.path_to_head();
    assert!(!path.is_empty(), "Path to active head must be non-empty");
}

#[tokio::test]
async fn test_session_dag_lock_hierarchy_deadlock_stress() {
    let dag = Arc::new(SessionBranchTree::new("Root".into(), "Root Resp".into()));

    // Create initial tree
    for i in 1..=50 {
        dag.append_step(format!("P{i}"), format!("R{i}"), None, vec![], "main")
            .unwrap();
    }

    // Run lock stress test under 5 second timeout to detect deadlocks
    let result = tokio::time::timeout(Duration::from_secs(5), async {
        let mut handles = Vec::new();

        // Group 1: Readers acquiring nodes -> edges -> active_head (path_to_head)
        for _ in 0..5 {
            let d = dag.clone();
            handles.push(tokio::spawn(async move {
                for _ in 0..100 {
                    let _p = d.path_to_head();
                    let _c = d.children_of(10);
                    tokio::task::yield_now().await;
                }
            }));
        }

        // Group 2: Writers appending and branching
        for writer_id in 0..5 {
            let d = dag.clone();
            handles.push(tokio::spawn(async move {
                for i in 0..50 {
                    let _ = d.append_step(
                        format!("W{writer_id}_{i}"),
                        format!("R{writer_id}_{i}"),
                        None,
                        vec![],
                        "branch",
                    );
                    tokio::task::yield_now().await;
                }
            }));
        }

        // Group 3: Head switchers
        for _ in 0..5 {
            let d = dag.clone();
            handles.push(tokio::spawn(async move {
                for i in 0..100 {
                    let target = (i % 50) as u64;
                    let _ = d.set_active_head(target);
                    tokio::task::yield_now().await;
                }
            }));
        }

        for h in handles {
            h.await.unwrap();
        }
    })
    .await;

    assert!(
        result.is_ok(),
        "Session-DAG lock hierarchy stress test completed without deadlocks (no timeout)"
    );
}

#[tokio::test]
async fn test_session_dag_persistence_save_load_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(
        LsmStorage::new(LsmConfig {
            path: dir.path().to_path_buf(),
            ..Default::default()
        })
        .await
        .unwrap(),
    );

    let dag = SessionBranchTree::new("Root Prompt".into(), "Root Response".into());
    let step1 = dag
        .append_step(
            "Step 1 Prompt".into(),
            "Step 1 Resp".into(),
            Some(TxId::new(42)),
            vec!["tool_out".into()],
            "main",
        )
        .unwrap();

    let branch = dag
        .branch_from(
            step1,
            "Branch Prompt".into(),
            "Branch Resp".into(),
            Some(TxId::new(100)),
            vec![],
            "explore",
        )
        .unwrap();

    dag.set_active_head(branch).unwrap();

    let tx = TxId::new(1);
    dag.save(storage.as_ref(), "test_session", tx)
        .await
        .unwrap();
    storage.commit(tx).await.unwrap();
    storage.flush().await.unwrap();

    let reloaded = SessionBranchTree::load(storage.as_ref(), "test_session")
        .await
        .unwrap();

    assert_eq!(reloaded.node_count(), 3);
    assert_eq!(reloaded.active_head(), branch);

    let path = reloaded.path_to_head();
    assert_eq!(path.len(), 3);
    assert_eq!(path[0].step_id, 0);
    assert_eq!(path[1].step_id, step1);
    assert_eq!(path[2].step_id, branch);
    assert_eq!(path[2].prompt, "Branch Prompt");
}
