use memfuse_core::DocId;
use memfuse_core::BoxFuture;
use memfuse_db::{
    execute_nrem_cycle, execute_sleep_cycle, start_nrem_reaper, MemFuse, MemFuseConfig,
    NremConfig, SegmentSynthesizer,
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
    let db = MemFuse::open_with_config(dir.path(), config)
        .await
        .unwrap();
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
async fn test_nrem_reaper_periodic_execution_and_cancellation() {
    let dir = tempdir().unwrap();
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(dir.path(), config)
        .await
        .unwrap();
    let collection = db.collection("nrem_reaper_test").await.unwrap();

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
    let handle = start_nrem_reaper(
        collection.clone(),
        NremConfig::default(),
        Duration::from_millis(20),
        cancel_token.clone(),
    );

    // Wait for the reaper ticker to execute NREM consolidation
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
        "NREM reaper should consolidate duplicate chunks down to 1"
    );
}

struct TestSynthesizer;

impl SegmentSynthesizer for TestSynthesizer {
    fn synthesize_segment<'a>(
        &'a self,
        segment_texts: &'a [&'a str],
    ) -> BoxFuture<'a, memfuse_core::Result<String>> {
        Box::pin(async move { Ok(format!("Synthesized: {}", segment_texts.join(" + "))) })
    }

    fn model_id(&self) -> &str {
        "test-synth"
    }
}

#[tokio::test]
async fn test_execute_sleep_cycle_with_rem_phase() {
    let dir = tempdir().unwrap();
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(dir.path(), config)
        .await
        .unwrap();
    let collection = db.collection("rem_test").await.unwrap();

    let emb_a = vec![1.0, 0.0, 0.0, 0.0];
    let mut turns = Vec::new();

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

    let nrem_config = NremConfig {
        min_turns_per_segment: 3,
        max_turns_per_segment: 20,
        segment_cohesion_threshold: 0.70,
        near_duplicate_cosine_threshold: 0.99, // high so turns aren't tombstoned in NREM
    };

    let synthesizer = TestSynthesizer;
    let (nrem_res, rem_res) = execute_sleep_cycle(&collection, &turns, &nrem_config, Some(&synthesizer))
        .await
        .unwrap();

    assert_eq!(nrem_res.segments_created, 1);
    let rem = rem_res.expect("REM phase result should be present");
    assert_eq!(rem.synthesized_chunks.len(), 1);
    assert_eq!(rem.skipped_segments, 0);

    let synth_chunk = &rem.synthesized_chunks[0];
    assert!(synth_chunk.content.contains("Synthesized:"));
    assert_eq!(synth_chunk.source_turn_ids.len(), 5);

    // Verify synthesized chunk was inserted into collection (via get or get_kv)
    let synth_id = format!("rem_synth_{}_0", synthesizer.model_id());
    let kv_val = collection.get_kv(&synth_id).await.unwrap();
    assert!(kv_val.is_some());
    let doc_meta = kv_val.unwrap();
    assert_eq!(doc_meta["rem_synthesized"], true);
    assert_eq!(doc_meta["source_turn_count"], 5);
}
