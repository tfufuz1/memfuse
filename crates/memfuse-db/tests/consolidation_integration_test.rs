use memfuse_core::traits::LlmTextGenerator;
use memfuse_core::BoxFuture;
use memfuse_core::DocId;
use memfuse_db::{
    execute_nrem_cycle, execute_sleep_cycle, CommunityStabilityTracker, ConsolidationConfig,
    MaintenanceConfig, MaintenanceScheduler, MemFuse, MemFuseConfig, NremConfig, RemConfig,
};
use std::time::Duration;
use tempfile::tempdir;
use tokio::time::sleep;

#[tokio::test]
async fn test_nrem_cycle_tombstones_duplicates() {
    let dir = tempdir().unwrap();
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(dir.path(), config).await.unwrap();
    let collection = db.collection("nrem_test").await.unwrap();

    let duplicate_emb = vec![1.0, 0.0, 0.0, 0.0];

    // Insert 10 identical chunks into the collection
    let mut turns = Vec::new();
    for i in 1..=10 {
        let doc_id_str = format!("chunk_{}", i);
        collection
            .insert(&doc_id_str, &duplicate_emb, None)
            .await
            .unwrap();
        let doc_id = DocId::from_key(&doc_id_str).unwrap();
        turns.push((doc_id, duplicate_emb.clone()));
    }

    assert_eq!(collection.len().await, 10);

    let nrem_config = NremConfig {
        min_turns_per_segment: 3,
        max_turns_per_segment: 20,
        segment_cohesion_threshold: 0.70,
        near_duplicate_cosine_threshold: 0.95,
        ..Default::default()
    };

    let result = execute_nrem_cycle(&collection, &turns, &nrem_config)
        .await
        .unwrap();

    // 10 identical turns produce 9 tombstoned duplicates (the older 9 turns)
    assert_eq!(result.duplicates_tombstoned.len(), 9);

    // Only 1 document should remain in the collection
    assert_eq!(collection.len().await, 1);
    assert!(collection.get("chunk_10").await.unwrap().is_some());
}

#[tokio::test]
async fn test_consolidation_scheduler_periodic_execution_and_cancellation() {
    let dir = tempdir().unwrap();
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(dir.path(), config).await.unwrap();
    let collection = db.collection("consolidation_scheduler_test").await.unwrap();

    let duplicate_emb = vec![0.0, 1.0, 0.0, 0.0];

    // Insert 10 identical chunks
    for i in 1..=10 {
        let doc_id_str = format!("reaper_chunk_{}", i);
        collection
            .insert(&doc_id_str, &duplicate_emb, None)
            .await
            .unwrap();
    }

    assert_eq!(collection.len().await, 10);

    let cancel_token = tokio_util::sync::CancellationToken::new();
    let scheduler = std::sync::Arc::new(MaintenanceScheduler::new(
        MaintenanceConfig {
            tick_interval_secs: 1,
            thermostat_enabled: false,
            sleep_cycle_enabled: true,
            sleep_episode_threshold: 1,
            ..Default::default()
        },
        collection.clone(),
        ConsolidationConfig::default(),
    ));
    let handle = scheduler.start(cancel_token.clone());

    // Wait for the scheduler ticker to execute consolidation
    let mut consolidated = false;
    for _ in 0..50 {
        sleep(Duration::from_millis(20)).await;
        if collection.len().await == 1 {
            consolidated = true;
            break;
        }
    }

    cancel_token.cancel();
    let handle_res = handle.await;

    assert!(handle_res.is_ok(), "Scheduler task should exit cleanly");
    assert!(
        consolidated,
        "Consolidation scheduler should consolidate duplicate chunks down to 1"
    );
}

struct TestLlmGenerator;

impl LlmTextGenerator for TestLlmGenerator {
    fn generate<'a>(&'a self, prompt: &'a str) -> BoxFuture<'a, memfuse_core::Result<String>> {
        Box::pin(async move {
            Ok(format!(
                "Synthesized summary from prompt of length {}",
                prompt.len()
            ))
        })
    }
}

#[tokio::test]
async fn test_execute_sleep_cycle_with_rem_phase() {
    let dir = tempdir().unwrap();
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(dir.path(), config).await.unwrap();
    let collection = db.collection("rem_test").await.unwrap();

    let emb_a = vec![1.0, 0.0, 0.0, 0.0];
    let mut turns = Vec::new();

    // Insert 5 turns into collection and build a graph cluster among them
    for i in 1..=5 {
        let doc_id_str = format!("turn_{}", i);
        collection
            .insert(
                &doc_id_str,
                &emb_a,
                Some(serde_json::json!({ "text": format!("Memory content {}", i) })),
            )
            .await
            .unwrap();
        let doc_id = DocId::from_key(&doc_id_str).unwrap();
        turns.push((doc_id, emb_a.clone()));
    }

    // Connect nodes into a graph cluster
    for i in 1..5 {
        collection
            .relate(
                &format!("turn_{}", i),
                &format!("turn_{}", i + 1),
                "connected",
            )
            .await
            .unwrap();
    }

    let nrem_config = NremConfig {
        min_turns_per_segment: 3,
        max_turns_per_segment: 20,
        segment_cohesion_threshold: 0.70,
        near_duplicate_cosine_threshold: 0.99, // high so turns aren't tombstoned in NREM
    };

    let rem_config = RemConfig {
        min_community_size: 3,
        stability_cycles_required: 2,
        max_llm_calls_per_cycle: 5,
    };

    let llm = TestLlmGenerator;
    let mut tracker = CommunityStabilityTracker::new();

    // First sleep cycle: stability count = 1 (< required 2)
    let (nrem_res_1, rem_res_1) = execute_sleep_cycle(
        &collection,
        &turns,
        &nrem_config,
        Some(&rem_config),
        Some(&llm),
        Some(&mut tracker),
    )
    .await
    .unwrap();

    assert_eq!(nrem_res_1.segments_created, 1);
    let rem_1 = rem_res_1.expect("REM phase result should be present");
    assert_eq!(
        rem_1.synthesized.len(),
        0,
        "Community observed once < stability_cycles_required=2, so 0 synthesized"
    );

    // Second sleep cycle: stability count = 2 (>= required 2)
    let (_nrem_res_2, rem_res_2) = execute_sleep_cycle(
        &collection,
        &turns,
        &nrem_config,
        Some(&rem_config),
        Some(&llm),
        Some(&mut tracker),
    )
    .await
    .unwrap();

    let rem_2 = rem_res_2.expect("REM phase result should be present");
    assert_eq!(
        rem_2.synthesized.len(),
        1,
        "Stable community should now be synthesized"
    );

    let meta_chunk = &rem_2.synthesized[0];
    assert!(meta_chunk.content.starts_with("[SYNTHESIZED FROM"));
    assert_eq!(meta_chunk.abstracts_from.len(), 5);

    // Verify synthesized chunk was inserted into collection via get_kv
    let synth_id = format!("rem_synth_{}_0", meta_chunk.source_community_hash);
    let kv_val = collection.get_kv(&synth_id).await.unwrap();
    assert!(kv_val.is_some());
    let doc_meta = kv_val.unwrap();
    assert_eq!(doc_meta["rem_synthesized"], true);
    assert_eq!(
        doc_meta["source_community_hash"],
        meta_chunk.source_community_hash
    );
}
