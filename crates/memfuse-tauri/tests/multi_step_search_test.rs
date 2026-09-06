//! Integration Test for Multi-Step Query Rewriting Search in memfuse-tauri / memfuse-db.
//!
//! Asserts that `MultiStepEngine` executes search against an in-memory/tempdir collection
//! with multiple documents, applies iterative rewriting via `QueryRewriter` when quality
//! thresholds trigger, and returns the expected result structure.
//!
//! Note: Qualitative evaluation of whether multi-step search yields better precision/recall
//! for real-world agent queries is a follow-up evaluation task, not part of this integration test.

use memfuse_core::Result;
use memfuse_db::{MultiStepConfig, MultiStepEngine, QueryRewriter, SearchResult};
use tempfile::tempdir;

struct DummyTestRewriter {
    sub_queries: Vec<String>,
}

impl QueryRewriter for DummyTestRewriter {
    fn rewrite<'a>(
        &'a self,
        _original_query: &'a str,
        _current_results: &'a [SearchResult],
    ) -> memfuse_core::BoxFuture<'a, Result<Vec<String>>> {
        Box::pin(async move {
            Ok(self.sub_queries.clone())
        })
    }
}

#[tokio::test]
async fn test_multi_step_search_execution_and_dto_shape() -> Result<()> {
    let dir = tempdir()?;
    let config = memfuse_db::MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = memfuse_db::MemFuse::open_with_config(dir.path(), config).await?;
    let collection = db.collection("multi_step_test_col").await?;

    // Insert multiple documents into the tempdir collection
    collection
        .insert(
            "doc-rust-1",
            &[1.0, 0.0, 0.0, 0.0],
            Some(serde_json::json!({
                "text": "Rust concurrency and async runtime architecture",
                "source": "docs/rust_async.md"
            })),
        )
        .await?;

    collection
        .insert(
            "doc-rust-2",
            &[0.8, 0.2, 0.0, 0.0],
            Some(serde_json::json!({
                "text": "Tokio task spawning and thread pool management",
                "source": "docs/tokio.md"
            })),
        )
        .await?;

    collection
        .insert(
            "doc-memfuse-3",
            &[0.0, 1.0, 0.0, 0.0],
            Some(serde_json::json!({
                "text": "Sovereign AI Memory Operating System architecture and LSM storage",
                "source": "docs/architecture.md"
            })),
        )
        .await?;

    // 1. Single-round execution (quality threshold low, round 1 sufficient)
    let single_config = MultiStepConfig {
        max_rounds: 3,
        quality_threshold: 0.001,
        min_quality_hits: 1,
    };
    let single_engine = MultiStepEngine::new(collection.clone(), single_config);
    let rewriter = DummyTestRewriter {
        sub_queries: vec!["Tokio task".to_string()],
    };

    let query_vector = vec![1.0, 0.0, 0.0, 0.0];
    let single_result = single_engine
        .search("Rust concurrency", &query_vector, 5, Some(&rewriter))
        .await?;

    assert_eq!(single_result.rounds_executed, 1);
    assert!(single_result.sub_queries.is_empty());
    assert!(!single_result.results.is_empty());

    // 2. Multi-round execution (quality threshold high, triggers query rewriting)
    let multi_config = MultiStepConfig {
        max_rounds: 3,
        quality_threshold: 0.99, // round 1 hits won't satisfy threshold
        min_quality_hits: 2,
    };
    let multi_engine = MultiStepEngine::new(collection.clone(), multi_config);

    let multi_result = multi_engine
        .search("Rust concurrency", &query_vector, 5, Some(&rewriter))
        .await?;

    assert_eq!(multi_result.rounds_executed, 2);
    assert_eq!(multi_result.sub_queries, vec!["Tokio task"]);
    assert!(!multi_result.results.is_empty());

    Ok(())
}
