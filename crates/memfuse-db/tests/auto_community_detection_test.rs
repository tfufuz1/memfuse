use memfuse_core::EntityId;
use memfuse_db::{CommunityDetectionConfig, MemFuse, MemFuseConfig};
use tempfile::TempDir;

#[tokio::test]
async fn test_auto_community_detection_after_150_inserts() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = TempDir::new()?;
    let config = MemFuseConfig {
        dimension: 4,
        community_detection: CommunityDetectionConfig {
            auto_trigger_threshold: 100,
        },
        ..Default::default()
    };

    let db = MemFuse::open_with_config(tmp.path(), config).await?;
    let col = db.collection("auto_comm_test").await?;

    assert_eq!(col.community_detection_trigger_threshold(), 100);
    assert_eq!(col.mutations_since_community_detection(), 0);

    // Prepare 150 documents
    let docs: Vec<(String, Vec<f32>, Option<serde_json::Value>)> = (0..150)
        .map(|i| {
            (
                format!("doc_{i}"),
                vec![i as f32, 1.0, 0.0, 0.0],
                Some(serde_json::json!({ "idx": i })),
            )
        })
        .collect();

    // First insert batch of 100 documents -> reaches threshold 100 -> resets counter to 0 & triggers auto community detection
    col.insert_many(&docs[0..100]).await?;
    assert_eq!(col.mutations_since_community_detection(), 0);

    // Second insert batch of 50 documents -> 50 mutations since reset -> counter is 50
    col.insert_many(&docs[100..150]).await?;
    assert_eq!(col.mutations_since_community_detection(), 50);

    // Yield briefly to allow background tokio task to finish community detection
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Verify community assignments exist in graph / storage for inserted entities
    let eid = EntityId::from_key("doc_0")?;
    let comm_id = col.get_community(eid).await?;
    assert!(
        comm_id.is_some(),
        "Auto-triggered community detection must have persisted community assignment for doc_0"
    );

    Ok(())
}

#[tokio::test]
async fn test_auto_community_detection_disabled_when_threshold_is_zero() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = TempDir::new()?;
    let config = MemFuseConfig {
        dimension: 4,
        community_detection: CommunityDetectionConfig {
            auto_trigger_threshold: 0,
        },
        ..Default::default()
    };

    let db = MemFuse::open_with_config(tmp.path(), config).await?;
    let col = db.collection("disabled_comm_test").await?;

    assert_eq!(col.community_detection_trigger_threshold(), 0);

    let docs: Vec<(String, Vec<f32>, Option<serde_json::Value>)> = (0..150)
        .map(|i| {
            (
                format!("doc_{i}"),
                vec![i as f32, 1.0, 0.0, 0.0],
                Some(serde_json::json!({ "idx": i })),
            )
        })
        .collect();

    col.insert_many(&docs).await?;

    // Mutations counter should remain 0 when threshold is 0
    assert_eq!(col.mutations_since_community_detection(), 0);

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let eid = EntityId::from_key("doc_0")?;
    let comm_id = col.get_community(eid).await?;
    assert!(
        comm_id.is_none(),
        "Community detection must NOT be auto-triggered when threshold is 0"
    );

    Ok(())
}

#[tokio::test]
async fn test_auto_community_detection_custom_low_threshold() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = TempDir::new()?;
    let config = MemFuseConfig {
        dimension: 4,
        community_detection: CommunityDetectionConfig {
            auto_trigger_threshold: 10,
        },
        ..Default::default()
    };

    let db = MemFuse::open_with_config(tmp.path(), config).await?;
    let col = db.collection("low_thresh_test").await?;

    for i in 0..15 {
        col.insert(
            &format!("item_{i}"),
            &[i as f32, 0.5, 0.0, 0.0],
            Some(serde_json::json!({ "item": i })),
        )
        .await?;
    }

    // 15 single inserts with threshold 10: 10 inserts reset to 0, 5 remaining -> count is 5
    assert_eq!(col.mutations_since_community_detection(), 5);

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let eid = EntityId::from_key("item_0")?;
    let comm = col.get_community(eid).await?;
    assert!(comm.is_some());

    Ok(())
}
