// AGENT:12 STATUS:DONE
// ANCHOR:INTEGRATION:E2E-002 AGENT:12 DATE:2026-05-22
use memfuse_core::{DocId, Result, TextIndex, TxId};
use memfuse_store::LsmStorage;
use memfuse_text::inverted::InvertedIndex;
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_inverted_index_persistence_and_reload() -> Result<()> {
    let tmp = TempDir::new().map_err(|e| memfuse_core::MemFuseError::Storage(e.to_string()))?;
    let path = tmp.path().to_path_buf();

    // 1. Create index and insert data
    {
        let storage = Arc::new(
            LsmStorage::new(memfuse_store::LsmConfig {
                path: path.clone(),
                ..Default::default()
            })
            .await?,
        );
        let index = InvertedIndex::new(storage.clone(), "persistent");

        let tx = TxId::new(1);
        index.insert(tx, DocId::new(1), "Rust is awesome").await?;
        index.insert(tx, DocId::new(2), "Python is great").await?;
        index.commit(tx).await?;

        // Force flush and close
        storage.force_flush().await?;
    }

    // 2. Reload and verify
    {
        let storage = Arc::new(
            LsmStorage::new(memfuse_store::LsmConfig {
                path,
                ..Default::default()
            })
            .await?,
        );
        let index = InvertedIndex::new(storage, "persistent");

        let results = index.search("rust", 10).await?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].doc_id, DocId::new(1));

        let results2 = index.search("python", 10).await?;
        assert_eq!(results2.len(), 1);
        assert_eq!(results2[0].doc_id, DocId::new(2));
    }

    Ok(())
}
