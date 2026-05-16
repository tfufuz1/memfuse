use memfuse_db::{MemFuse, MemFuseConfig};
use serde_json::json;
use tempfile::TempDir;

#[tokio::test]
async fn test_german_morphological_search_integration() {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(tmp.path(), config)
        .await
        .expect("open");

    // Use a namespace containing "de" to trigger GermanMorphTokenizer
    let col = db.collection("de-legal-docs").await.expect("collection");

    col.insert(
        "doc-1",
        &[0.1, 0.2, 0.3, 0.4],
        Some(json!({"text": "Das Bundesverfassungsgericht hat entschieden."})),
    )
    .await
    .expect("insert");

    // Search for "gericht" (which is a split suffix)
    // Even though "gericht" is not a standalone word in the input, it should be found
    let results = col
        .hybrid_search("gericht", &[0.0, 0.0, 0.0, 0.0], 10)
        .await
        .expect("hybrid search");

    assert!(
        !results.is_empty(),
        "Should find doc-1 via split suffix 'gericht'"
    );
    assert_eq!(results[0].id, "doc-1");

    col.insert(
        "doc-2",
        &[0.1, 0.2, 0.3, 0.4],
        Some(json!({"text": "Das Arbeitsamt ist geschlossen."})),
    )
    .await
    .expect("insert 2");

    let results2 = col
        .hybrid_search("amt", &[0.0, 0.0, 0.0, 0.0], 10)
        .await
        .expect("hybrid search amt");

    assert!(
        !results2.is_empty(),
        "Should find doc-2 via split suffix 'amt'"
    );
    assert_eq!(results2[0].id, "doc-2");
}
