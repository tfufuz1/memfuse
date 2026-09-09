use memfuse_core::traits::LlmTextGenerator;
use memfuse_core::BoxFuture;
use memfuse_core::DocId;
use memfuse_db::{
    execute_consolidation_pass, execute_sleep_cycle, start_consolidation_reaper,
    CommunityStabilityTracker, ConsolidationConfig, MemFuse, MemFuseConfig, SynthesisConfig,
};
use std::time::Duration;
use tempfile::tempdir;
use tokio::time::sleep;

#[tokio::test]
async fn test_consolidation_pass_tombstones_duplicates() {
    let dir = tempdir().unwrap();
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(dir.path(), config).await.unwrap();
    let collection = db.collection("consolidation_test").await.unwrap();

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

    let consolidation_config = ConsolidationConfig {
        min_turns_per_segment: 3,
        max_turns_per_segment: 20,
        segment_cohesion_threshold: 0.70,
        near_duplicate_cosine_threshold: 0.95,
        ..Default::default()
    };

    let result = execute_consolidation_pass(&collection, &turns, &consolidation_config)
        .await
        .unwrap();

    // 10 identical turns produce 9 tombstoned duplicates (the older 9 turns)
    assert_eq!(result.duplicates_tombstoned.len(), 9);

    // Only 1 document should remain in the collection
    assert_eq!(collection.len().await, 1);
    assert!(collection.get("chunk_10").await.unwrap().is_some());
}

#[tokio::test]
async fn test_consolidation_reaper_periodic_execution_and_cancellation() {
    let dir = tempdir().unwrap();
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(dir.path(), config).await.unwrap();
    let collection = db.collection("consolidation_reaper_test").await.unwrap();

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
    let handle = start_consolidation_reaper(
        collection.clone(),
        ConsolidationConfig::default(),
        Duration::from_millis(20),
        cancel_token.clone(),
    );

    // Wait for the reaper ticker to execute consolidation
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

    assert!(handle_res.is_ok(), "Reaper task should exit cleanly");
    assert!(
        consolidated,
        "Consolidation reaper should consolidate duplicate chunks down to 1"
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
async fn test_execute_sleep_cycle_with_synthesis_pass() {
    let dir = tempdir().unwrap();
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(dir.path(), config).await.unwrap();
    let collection = db.collection("synthesis_test").await.unwrap();

    let mut turns = Vec::new();

    // Insert 5 turns into collection and build a graph cluster among them
    for i in 1..=5 {
        let doc_id_str = format!("turn_{}", i);
        let mut emb = vec![1.0, 0.5 * (i as f32), 0.0, 0.0];
        let norm = (emb[0] * emb[0] + emb[1] * emb[1]).sqrt();
        emb[0] /= norm;
        emb[1] /= norm;
        collection
            .insert(
                &doc_id_str,
                &emb,
                Some(serde_json::json!({ "text": format!("Memory content {}", i) })),
            )
            .await
            .unwrap();
        let doc_id = DocId::from_key(&doc_id_str).unwrap();
        turns.push((doc_id, emb));
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

    let consolidation_config = ConsolidationConfig {
        min_turns_per_segment: 3,
        max_turns_per_segment: 20,
        segment_cohesion_threshold: 0.70,
        near_duplicate_cosine_threshold: 1.0, // high so turns aren't tombstoned in consolidation pass
    };

    let synthesis_config = SynthesisConfig {
        min_community_size: 3,
        stability_cycles_required: 2,
        max_llm_calls_per_cycle: 5,
    };

    let llm = TestLlmGenerator;
    let mut tracker = CommunityStabilityTracker::new();

    // First cycle: stability count = 1 (< required 2)
    let (consolidation_res_1, synthesis_res_1) = execute_sleep_cycle(
        &collection,
        &turns,
        &consolidation_config,
        Some(&synthesis_config),
        Some(&llm),
        Some(&mut tracker),
    )
    .await
    .unwrap();

    assert_eq!(consolidation_res_1.segments_created, 1);
    let synth_1 = synthesis_res_1.expect("Synthesis result should be present");
    assert_eq!(
        synth_1.synthesized.len(),
        0,
        "Community observed once < stability_cycles_required=2, so 0 synthesized"
    );

    // Second cycle: stability count = 2 (>= required 2)
    let (_consolidation_res_2, synthesis_res_2) = execute_sleep_cycle(
        &collection,
        &turns,
        &consolidation_config,
        Some(&synthesis_config),
        Some(&llm),
        Some(&mut tracker),
    )
    .await
    .unwrap();

    let synth_2 = synthesis_res_2.expect("Synthesis result should be present");
    assert_eq!(
        synth_2.synthesized.len(),
        1,
        "Stable community should now be synthesized"
    );

    let meta_chunk = &synth_2.synthesized[0];
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
