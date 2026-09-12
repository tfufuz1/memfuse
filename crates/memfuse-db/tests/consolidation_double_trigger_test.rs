// FILE-CONTEXT
// ZWECK: Integrierter Test zur Verifizierung des Double-Trigger-Schutzes zwischen ConsolidationEngine und MaintenanceScheduler.
// INVARIANTEN: P14-Compliance (Zuständigkeiten dokumentiert & koordiniert); M-1 double-trigger protection.
// STAND: TS:2026-09-12T00:00:00Z

use memfuse_db::consolidation_executor::{ConsolidationEngine, ConsolidationLockGuard};
use memfuse_db::maintenance_config::MaintenanceConfig;
use memfuse_db::maintenance_scheduler::MaintenanceScheduler;
use memfuse_db::memory_consolidation::{ConsolidationConfig, SynthesisConfig};
use memfuse_db::Collection;
use memfuse_graph::CsrGraph;
use memfuse_index::HnswIndex;
use memfuse_store::LsmStorage;
use serde_json::json;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;

async fn create_test_collection() -> (Arc<Collection<LsmStorage, HnswIndex>>, tempfile::TempDir) {
    let dir = tempdir().expect("tempdir");
    let storage = Arc::new(
        LsmStorage::new(memfuse_store::LsmConfig {
            path: dir.path().to_path_buf(),
            ..Default::default()
        })
        .await
        .expect("LsmStorage"),
    );
    let index = Arc::new(
        HnswIndex::try_new(memfuse_index::HnswConfig {
            dimension: 4,
            ..Default::default()
        })
        .expect("HnswIndex"),
    );
    let graph = Arc::new(CsrGraph::new());
    let next_tx = Arc::new(AtomicU64::new(1));

    let col = Arc::new(Collection::new(
        "default".to_string(),
        storage,
        index,
        graph,
        next_tx,
        4,
        memfuse_text::Language::English,
    ));
    (col, dir)
}

#[tokio::test]
async fn test_consolidation_double_trigger_mutual_exclusion() {
    let (col, _dir) = create_test_collection().await;
    let cancel_token = tokio_util::sync::CancellationToken::new();

    // Populate collection with duplicate turns
    let vec = vec![1.0, 0.0, 0.0, 0.0];
    col.insert("turn_1", &vec, Some(json!({"text": "Duplicate content"})))
        .await
        .expect("insert turn 1");
    col.insert("turn_2", &vec, Some(json!({"text": "Duplicate content"})))
        .await
        .expect("insert turn 2");

    let engine = ConsolidationEngine::new(
        col.clone(),
        ConsolidationConfig {
            near_duplicate_cosine_threshold: 0.9,
            ..Default::default()
        },
        SynthesisConfig::default(),
        Duration::from_secs(60),
        cancel_token.clone(),
    );

    let scheduler_config = MaintenanceConfig {
        tick_interval_secs: 1,
        background_consolidation_enabled: true,
        background_consolidation_episode_threshold: 1,
        decay_enabled: false,
        percolation_enabled: false,
        replicator_enabled: false,
        ..Default::default()
    };
    let scheduler = MaintenanceScheduler::new(
        scheduler_config,
        col.clone(),
        ConsolidationConfig {
            near_duplicate_cosine_threshold: 0.9,
            ..Default::default()
        },
    );

    // Case 1: Acquire ConsolidationLockGuard manually
    let guard = ConsolidationLockGuard::try_acquire(&col.consolidation_in_progress())
        .expect("lock acquisition should succeed");

    // While lock is held, ConsolidationEngine::run_cycle should skip
    let engine_res = engine.run_cycle().await.expect("engine run_cycle");
    assert_eq!(
        engine_res.0.duplicates_tombstoned.len(),
        0,
        "Engine cycle should skip when lock is held"
    );

    // While lock is held, MaintenanceScheduler::run_tick should skip consolidation pass step
    scheduler.run_tick().await;

    // Verify turn_2 is NOT tombstoned because lock was held by guard
    let turn_2_doc = col.get("turn_2").await.expect("get turn_2");
    assert!(
        turn_2_doc.is_some(),
        "turn_2 must still exist since both triggers skipped"
    );

    // Case 2: Drop manual lock guard
    drop(guard);

    // ConsolidationEngine::run_cycle should now acquire lock and execute
    let engine_res_2 = engine.run_cycle().await.expect("engine run_cycle 2");
    assert_eq!(
        engine_res_2.0.duplicates_tombstoned.len(),
        1,
        "Engine cycle should consolidate duplicate turn after lock release"
    );

    cancel_token.cancel();
}

#[tokio::test]
async fn test_consolidation_lock_guard_panic_safety() {
    let (col, _dir) = create_test_collection().await;

    let flag = col.consolidation_in_progress();
    assert!(!flag.load(std::sync::atomic::Ordering::Relaxed));

    // Demonstrate guard drop on panic / unwind
    let result = std::panic::catch_unwind(|| {
        let _guard = ConsolidationLockGuard::try_acquire(&flag)
            .expect("acquire in closure");
        assert!(flag.load(std::sync::atomic::Ordering::Relaxed));
        panic!("Simulated panic inside consolidation block");
    });

    assert!(result.is_err(), "closure should have panicked");

    // Guard should reset flag back to false on drop during panic unwind
    assert!(
        !flag.load(std::sync::atomic::Ordering::Relaxed),
        "Flag must be reset to false after panic unwind"
    );
}
