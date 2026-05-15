//! Comprehensive E2E Integration Tests for MemFuse.
//!
//! ANCHOR:TEST:E2E-COMPREHENSIVE-001 STATUS:READY AGENT:12
//! This suite tests the full stack from MemFuse facade down to LSM-Store and HNSW-Index,
//! including text indexing, hybrid search, and relationship management.

use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use std::path::PathBuf;
use tempfile::TempDir;

async fn setup_db(path: PathBuf) -> MemFuse {
    let config = MemFuseConfig {
        dimension: 4,
        max_elements: 1000,
        distance_metric: DistanceMetric::Cosine,
    };
    MemFuse::open_with_config(path, config)
        .await
        .expect("Failed to open DB")
}

#[tokio::test]
async fn test_comprehensive_e2e_workflow() {
    let tmp = TempDir::new().expect("Failed to create temp dir");
    let db_path = tmp.path().to_owned();

    // 1. Initial State: Multi-Collection Creation
    {
        let db = setup_db(db_path.clone()).await;
        let col_agent = db.collection("agents").await.expect("Failed to create agents collection");
        let col_task = db.collection("tasks").await.expect("Failed to create tasks collection");

        // 2. Data Ingestion: Documents with Embeddings and Text Metadata
        col_agent.insert(
            "agent-1",
            &[1.0, 0.0, 0.0, 0.0],
            Some(json!({"name": "Alice", "text": "Senior Rust Developer specialized in systems and databases."}))
        ).await.expect("Insert agent 1 failed");

        col_agent.insert(
            "agent-2",
            &[0.0, 1.0, 0.0, 0.0],
            Some(json!({"name": "Bob", "text": "Python AI expert focused on LLM integration and orchestration."}))
        ).await.expect("Insert agent 2 failed");

        col_task.insert(
            "task-1",
            &[0.9, 0.1, 0.0, 0.0],
            Some(json!({"title": "Fix LSM Bug", "content": "Investigate and fix the write-stall issue in the LSM-tree storage engine."}))
        ).await.expect("Insert task 1 failed");

        // 3. Relationships
        db.relate("agent-1", "task-1", "assigned_to").await.expect("Relate failed");

        let relations = db.scan_prefix("__rel:agent-1:assigned_to:").await.expect("Scan relations failed");
        assert_eq!(relations.len(), 1);

        // 4. Update Verification
        col_agent.update(
            "agent-1",
            &[1.0, 0.0, 0.0, 0.0],
            Some(json!({"name": "Alice", "status": "Busy"}))
        ).await.expect("Update failed");

        let updated_doc = col_agent.get("agent-1").await.unwrap().unwrap();
        assert_eq!(updated_doc.metadata.unwrap()["status"], "Busy");

        // 5. Delete Verification
        col_agent.delete("agent-2").await.expect("Delete failed");
        assert!(col_agent.get("agent-2").await.unwrap().is_none());

        // 6. Hybrid Search
        let hybrid_results = col_task.hybrid_search("LSM storage", &[0.0, 0.0, 0.0, 0.0], 5).await.expect("Hybrid search failed");
        assert!(!hybrid_results.is_empty());
        assert_eq!(hybrid_results[0].id, "task-1");
    }

    // 7. Persistence
    {
        let db = setup_db(db_path).await;
        let col_agent = db.collection("agents").await.expect("Get agents collection failed");
        let agent = col_agent.get("agent-1").await.expect("Get agent failed").expect("Agent 1 missing");
        assert_eq!(agent.id, "agent-1");
        assert!(col_agent.get("agent-2").await.unwrap().is_none()); // Still gone
    }
}
