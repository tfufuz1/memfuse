use memfuse_core::{
    BoxFuture, DocId, LlmTextGenerator, MemFuseError, Result, StorageEngine};
use memfuse_db::{
    cleanup_orphaned_consolidation_intents, ContextCompactor, MemFuse, MemFuseConfig,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

struct MockLlmGenerator {
    call_count: Arc<AtomicUsize>,
}

impl MockLlmGenerator {
    fn new() -> Self {
        Self {
            call_count: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl LlmTextGenerator for MockLlmGenerator {
    fn generate<'a>(&'a self, prompt: &'a str) -> BoxFuture<'a, Result<String>> {

        Box::pin(async move {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        Ok("Mock LLM summary of source documents".to_string())


        })

    }
}

struct MutatingLlmGenerator<S: StorageEngine, V: memfuse_core::VectorIndex> {
    collection: Arc<memfuse_db::collection::Collection<S, V>>,
    doc_to_mutate: String,
    fail_attempts: usize,
    call_count: Arc<AtomicUsize>,
}

impl<S: StorageEngine, V: memfuse_core::VectorIndex> LlmTextGenerator
    for MutatingLlmGenerator<S, V>
{
    fn generate<'a>(&'a self, _prompt: &'a str) -> BoxFuture<'a, Result<String>> {
        Box::pin(async move {
            let count = self.call_count.fetch_add(1, Ordering::SeqCst);
            if count < self.fail_attempts {
                // Mutate document to force OCC conflict
                self.collection
                    .update(
                        &self.doc_to_mutate,
                        &[0.0, 1.0, 0.0, 0.0],
                        Some(serde_json::json!({"text": format!("Mutated version {}", count)})),
                    )
                    .await?;
            }
            Ok("Summary after mutation".to_string())
        })
    }
}

fn test_config() -> MemFuseConfig {
    MemFuseConfig {
        dimension: 4,
        ..Default::default()
    }
}

#[tokio::test]
async fn test_consolidate_with_retry_succeeds_on_first_attempt() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let db = MemFuse::open_with_config(tmp.path(), test_config()).await?;
    let col = db.collection("test_col").await?;

    col.insert(
        "src_1",
        &[0.1, 0.2, 0.3, 0.4],
        Some(serde_json::json!({"text": "Fact 1 content"})),
    )
    .await?;
    col.insert(
        "src_2",
        &[0.2, 0.3, 0.4, 0.5],
        Some(serde_json::json!({"text": "Fact 2 content"})),
    )
    .await?;

    let d1 = DocId::from_key("src_1")?;
    let d2 = DocId::from_key("src_2")?;
    let target_id = DocId::from_key("target_1")?;

    let llm = MockLlmGenerator::new();

    ContextCompactor::consolidate_with_retry(&col, &[d1, d2], target_id, &llm, 3).await?;

    assert_eq!(llm.call_count.load(Ordering::SeqCst), 1);
    assert!(
        col.get("src_1").await?.is_none(),
        "Source 1 must be deleted on successful consolidation"
    );
    assert!(
        col.get("src_2").await?.is_none(),
        "Source 2 must be deleted on successful consolidation"
    );

    Ok(())
}

#[tokio::test]
async fn test_consolidate_with_retry_retries_on_occ_conflict() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let db = MemFuse::open_with_config(tmp.path(), test_config()).await?;
    let col = db.collection("retry_col").await?;

    col.insert(
        "src_1",
        &[0.1, 0.2, 0.3, 0.4],
        Some(serde_json::json!({"text": "Fact 1 content"})),
    )
    .await?;
    col.insert(
        "src_2",
        &[0.2, 0.3, 0.4, 0.5],
        Some(serde_json::json!({"text": "Fact 2 content"})),
    )
    .await?;

    let d1 = DocId::from_key("src_1")?;
    let d2 = DocId::from_key("src_2")?;
    let target_id = DocId::from_key("target_retry")?;

    let call_count = Arc::new(AtomicUsize::new(0));
    let mutating_llm = MutatingLlmGenerator {
        collection: col.clone(),
        doc_to_mutate: "src_2".to_string(),
        fail_attempts: 1, // Fail once, succeed on retry
        call_count: call_count.clone(),
    };

    ContextCompactor::consolidate_with_retry(&col, &[d1, d2], target_id, &mutating_llm, 3).await?;

    assert_eq!(call_count.load(Ordering::SeqCst), 2);

    Ok(())
}

#[tokio::test]
async fn test_consolidate_with_retry_fails_after_max_retries() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let db = MemFuse::open_with_config(tmp.path(), test_config()).await?;
    let col = db.collection("fail_col").await?;

    col.insert(
        "src_1",
        &[0.1, 0.2, 0.3, 0.4],
        Some(serde_json::json!({"text": "Fact 1 content"})),
    )
    .await?;
    col.insert(
        "src_2",
        &[0.2, 0.3, 0.4, 0.5],
        Some(serde_json::json!({"text": "Fact 2 content"})),
    )
    .await?;

    let d1 = DocId::from_key("src_1")?;
    let d2 = DocId::from_key("src_2")?;
    let target_id = DocId::from_key("target_fail")?;

    let call_count = Arc::new(AtomicUsize::new(0));
    let always_mutating_llm = MutatingLlmGenerator {
        collection: col.clone(),
        doc_to_mutate: "src_2".to_string(),
        fail_attempts: 10, // Exceeds max_retries = 2
        call_count: call_count.clone(),
    };

    let res = ContextCompactor::consolidate_with_retry(
        &col,
        &[d1, d2],
        target_id,
        &always_mutating_llm,
        2,
    )
    .await;

    assert!(res.is_err(), "Must return error after max_retries reached");
    assert!(matches!(res.unwrap_err(), MemFuseError::StaleRead(_)));
    assert_eq!(call_count.load(Ordering::SeqCst), 3); // initial attempt + 2 retries

    Ok(())
}

#[tokio::test]
async fn test_cleanup_orphaned_intents_removes_stale_keys() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let lsm_config = memfuse_store::LsmConfig {
        path: tmp.path().to_path_buf(),
        ..Default::default()
    };
    let storage = memfuse_store::LsmStorage::new(lsm_config).await?;

    let tx = memfuse_core::TxId::new(1);
    let key = b"consolidation_intent:orphaned_123";
    let intent = memfuse_db::transaction::CommitIntent::Consolidation {
        source_docs: vec![(DocId::new(1), memfuse_core::TxId::new(1))],
        target_id: DocId::new(99),
        base_tx: tx,
    };
    let intent_bytes = serde_json::to_vec(&intent)?;

    storage.put(tx, key, &intent_bytes).await?;
    storage.commit(tx).await?;

    assert!(storage.get(key).await?.is_some());

    let cleaned = cleanup_orphaned_consolidation_intents(&storage).await?;
    assert_eq!(cleaned, 1);

    assert!(storage.get(key).await?.is_none());

    Ok(())
}
