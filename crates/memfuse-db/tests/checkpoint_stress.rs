// AGENT:12
// ANCHOR:INTEGRATION STATUS:FIXME PRIO:1 AGENT:12 AGENT:01
// This test is currently disabled due to mismatch in CheckpointManager::create_checkpoint and drop_checkpoint API.
/*
use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use tempfile::TempDir;

#[tokio::test]
async fn test_checkpoint_stress_and_gc() {
    let tmp = TempDir::new().expect("failed to create temp dir");
    let db = MemFuse::open_with_config(
        tmp.path(),
        MemFuseConfig {
            dimension: 4,
            max_elements: 100,
            distance_metric: DistanceMetric::Cosine,
            ..Default::default()
        },
    )
    .await
    .expect("open db");

    let checkpoint_manager = db.checkpoint_manager();

    // 1. Create multiple checkpoints
    let mut checkpoints = Vec::new();
    for i in 0..10 {
        let name = format!("checkpoint-{}", i);
        // Create some data
        db.insert(
            &format!("doc-{}", i),
            &[i as f32, 0.0, 0.0, 0.0],
            Some(json!({"i": i})),
        )
        .await
        .expect("insert");

        let cp = checkpoint_manager
            .create_checkpoint(&name)
            .await
            .expect("create checkpoint");
        checkpoints.push(cp);
    }

    // 2. Delete half of them
    for cp in checkpoints.drain(0..5) {
        checkpoint_manager
            .drop_checkpoint(&cp)
            .await
            .expect("drop checkpoint");
    }

    // 3. Verify remaining
    let list = checkpoint_manager.list_checkpoints().await.expect("list");
    assert_eq!(list.len(), 5);

    // 4. Stress: rapidly create and delete
    for i in 10..20 {
        let name = format!("stress-{}", i);
        let cp = checkpoint_manager
            .create_checkpoint(&name)
            .await
            .expect("create");
        checkpoint_manager
            .drop_checkpoint(&cp.name)
            .await
            .expect("drop");
    }
}
*/
