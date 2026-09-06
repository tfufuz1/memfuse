use memfuse_core::{
    BoxFuture, DistanceMetric, FilterExpr, HybridQuery};
use memfuse_db::{Collection, Language};
use memfuse_graph::CsrGraph;
use memfuse_index::{HnswConfig, HnswIndex};
use memfuse_store::{LsmConfig, LsmStorage};
use serde_json::json;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use tempfile::TempDir;

async fn create_large_test_collection(
    name: &str,
    total_docs: usize,
    archived_ratio: f64,
) -> (Collection<LsmStorage, HnswIndex>, TempDir) {
    let dir = TempDir::new().expect("tempdir");
    let lsm_config = LsmConfig {
        path: dir.path().to_path_buf(),
        ..Default::default()
    };
    let storage = Arc::new(LsmStorage::new(lsm_config).await.expect("storage"));
    let hnsw_config = HnswConfig {
        dimension: 4,
        max_elements: total_docs + 1000,
        m: 16,
        ef_construction: 64,
        ef_search: 64,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };
    let index = Arc::new(HnswIndex::try_new(hnsw_config).expect("hnsw index"));
    let graph = Arc::new(CsrGraph::new());
    let next_tx = Arc::new(AtomicU64::new(1));
    let col = Collection::new(
        name.to_string(),
        storage,
        index,
        graph,
        next_tx,
        4,
        Language::English,
    );

    let archived_count = (total_docs as f64 * archived_ratio) as usize;

    let chunk_size = 1000;
    for chunk_start in (0..total_docs).step_by(chunk_size) {
        let chunk_end = (chunk_start + chunk_size).min(total_docs);
        let mut batch = Vec::with_capacity(chunk_end - chunk_start);

        for i in chunk_start..chunk_end {
            let id = format!("doc-{}", i);
            let is_archived = i < archived_count;
            let status = if is_archived { "archived" } else { "active" };

            let embedding = if is_archived {
                vec![1.0, 0.0, 0.0, 0.0]
            } else {
                vec![0.0, 1.0, 0.0, 0.0]
            };

            let text = if is_archived {
                format!("archived secret content {}", i)
            } else {
                format!("active standard document {}", i)
            };

            batch.push((
                id,
                embedding,
                Some(json!({
                    "status": status,
                    "text": text,
                    "index": i
                })),
            ));
        }

        col.insert_many(&batch)
            .await
            .expect("insert_many chunk failed");
    }

    (col, dir)
}

#[tokio::test]
async fn test_search_with_filter_expr_recall_10k_selective() {
    let total_docs = 10_000;
    let archived_ratio = 0.02; // 2% selective filter (200 docs out of 10,000)
    let (col, _dir) =
        create_large_test_collection("test_recall_vec", total_docs, archived_ratio).await;

    let filter = FilterExpr::Eq {
        field: "status".to_string(),
        value: json!("archived"),
    };

    let query_vector = vec![1.0, 0.0, 0.0, 0.0];
    let k = 10;

    #[allow(deprecated)]
    let results = col
        .search_with_filter_expr(&query_vector, k, Some(filter.clone()))
        .await
        .expect("search_with_filter_expr should succeed");

    assert_eq!(
        results.len(),
        k,
        "search_with_filter_expr must return exactly 10 results for 2% selective filter over 10k docs, got {}",
        results.len()
    );

    for res in &results {
        let meta = res.metadata.as_ref().expect("metadata present");
        assert_eq!(
            meta["status"].as_str().unwrap(),
            "archived",
            "Every result must match status 'archived'"
        );
    }
}

#[tokio::test]
async fn test_hybrid_search_with_query_recall_10k_selective() {
    let total_docs = 10_000;
    let archived_ratio = 0.02; // 2% selective filter (200 docs out of 10,000)
    let (col, _dir) =
        create_large_test_collection("test_recall_hybrid", total_docs, archived_ratio).await;

    let filter = FilterExpr::Eq {
        field: "status".to_string(),
        value: json!("archived"),
    };

    let query_vector = vec![1.0, 0.0, 0.0, 0.0];
    let k = 10;

    let hybrid_query = HybridQuery::builder()
        .with_text_query("secret".to_string())
        .with_vector_query(query_vector.clone())
        .with_filter(filter.clone())
        .with_k(k)
        .build()
        .expect("hybrid query");

    #[allow(deprecated)]
    let results_query = col
        .hybrid_search_with_query(&hybrid_query)
        .await
        .expect("hybrid_search_with_query should succeed");

    assert_eq!(
        results_query.len(),
        k,
        "hybrid_search_with_query must return exactly 10 results for 2% selective filter over 10k docs, got {}",
        results_query.len()
    );

    let builder_results = col
        .query()
        .text("secret")
        .vector(&query_vector)
        .filter(filter)
        .k(k)
        .execute()
        .await
        .expect("query builder execute should succeed");

    assert_eq!(
        builder_results.len(),
        k,
        "col.query() builder must return exactly 10 results for 2% selective filter over 10k docs, got {}",
        builder_results.len()
    );

    for res in &builder_results {
        let meta = res.metadata.as_ref().expect("metadata present");
        assert_eq!(
            meta["status"].as_str().unwrap(),
            "archived",
            "Every builder result must match status 'archived'"
        );
    }
}

#[tokio::test]
async fn test_unselective_filter_performance_regression() {
    let total_docs = 10_000;
    let archived_ratio = 0.50; // 50% unselective filter (5000 docs match)
    let (col, _dir) =
        create_large_test_collection("test_unselective", total_docs, archived_ratio).await;

    let filter = FilterExpr::Eq {
        field: "status".to_string(),
        value: json!("archived"),
    };

    let query_vector = vec![1.0, 0.0, 0.0, 0.0];
    let k = 10;

    #[allow(deprecated)]
    let results = col
        .search_with_filter_expr(&query_vector, k, Some(filter.clone()))
        .await
        .expect("search_with_filter_expr should succeed for unselective filter");

    assert_eq!(
        results.len(),
        k,
        "Unselective filter must return k results, got {}",
        results.len()
    );

    let builder_results = col
        .query()
        .vector(&query_vector)
        .filter(filter)
        .k(k)
        .execute()
        .await
        .expect("col.query() should succeed for unselective filter");

    assert_eq!(
        builder_results.len(),
        k,
        "Unselective builder query must return k results, got {}",
        builder_results.len()
    );
}
