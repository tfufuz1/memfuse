// AGENT:05
// ANCHOR:ARCH:DAG-001 STATUS:FIXME PRIO:1 AGENT:05 AGENT:13
// This test is currently disabled due to DAG violation (memfuse-text -> memfuse-store).
/*
use memfuse_text::{InvertedIndex, Tokenizer, DefaultTokenizer};
use memfuse_store::{LsmConfig, LsmStorage}; // This is a DAG violation
use tempfile::TempDir;
use std::sync::Arc;

#[tokio::test]
async fn test_inverted_index_with_lsm_storage() {
    let tmp = TempDir::new().unwrap();
    let store = Arc::new(LsmStorage::open(tmp.path(), LsmConfig::default()).await.unwrap());
    let mut index = InvertedIndex::new(store);

    index.upsert_document("doc1", "The quick brown fox").await.unwrap();
    index.upsert_document("doc2", "Jumped over the lazy dog").await.unwrap();

    let results = index.search_bm25("quick fox", 10).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "doc1");
}
*/
