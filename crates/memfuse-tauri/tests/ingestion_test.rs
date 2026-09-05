use async_trait::async_trait;
use memfuse_core::{GraphIndex, Result, TextEmbeddingEngine};
use memfuse_db::{MemFuse, MemFuseConfig};
use memfuse_tauri_lib::ingestion::{IngestionPipeline, MAX_COOCCURRENCE_ENTITIES_PER_CHUNK};
use std::sync::Arc;
use tempfile::TempDir;

struct DummyEmbedder {
    dim: usize,
}

#[async_trait]
impl TextEmbeddingEngine for DummyEmbedder {
    async fn embed(&self, _text: &str) -> Result<Vec<f32>> {
        Ok(vec![0.1f32; self.dim])
    }
}

#[tokio::test]
async fn test_ingest_markdown_file() {
    let tmp = TempDir::new().expect("temp dir");
    let db_path = tmp.path().join("db");
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };

    let db = MemFuse::open_with_config(&db_path, config)
        .await
        .expect("open db");

    let collection = db.collection("test-ingest").await.expect("collection");

    let embedder = Arc::new(DummyEmbedder { dim: 4 });
    let pipeline = IngestionPipeline::new(embedder);

    let doc_path = tmp.path().join("test_document.md");
    let markdown_content = r#"# Architecture Overview

MemFuse is an embedded hybrid-search memory engine.

## Subsystem 1: Storage
LSM-Tree provides crash-resilient key-value storage.

## Subsystem 2: Vector Index
HNSW provides fast k-NN vector search over document embeddings.
"#;

    std::fs::write(&doc_path, markdown_content).expect("write test md");

    let report = pipeline
        .ingest_file(&doc_path, &collection)
        .await
        .expect("ingest_file");

    assert_eq!(report.file_path, doc_path.display().to_string());
    assert!(report.chunks_created > 0);
    assert!(report.errors.is_empty());

    let results = collection
        .query()
        .embedding([0.1, 0.1, 0.1, 0.1])
        .k(5)
        .execute()
        .await
        .expect("search");

    assert!(!results.is_empty());
}

#[tokio::test]
async fn test_ingest_folder() {
    let tmp = TempDir::new().expect("temp dir");
    let db_path = tmp.path().join("db");
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };

    let db = MemFuse::open_with_config(&db_path, config)
        .await
        .expect("open db");

    let collection = db.collection("folder-ingest").await.expect("collection");

    let embedder = Arc::new(DummyEmbedder { dim: 4 });
    let pipeline = IngestionPipeline::new(embedder);

    let folder_path = tmp.path().join("docs");
    std::fs::create_dir(&folder_path).expect("create folder");

    let file1 = folder_path.join("doc1.txt");
    let file2 = folder_path.join("doc2.markdown");
    let file_unsupported = folder_path.join("file.xyz");

    std::fs::write(&file1, "Simple text document content.").expect("write file1");
    std::fs::write(&file2, "# Title\nMarkdown document content.").expect("write file2");
    std::fs::write(&file_unsupported, "Ignored format.").expect("write file_unsupported");

    let reports = pipeline
        .ingest_folder(&folder_path, &collection)
        .await
        .expect("ingest_folder");

    assert_eq!(reports.len(), 2);
    for r in reports {
        assert!(r.chunks_created > 0);
        assert!(r.errors.is_empty());
    }
}

#[tokio::test]
async fn test_batch_ingest_folder_best_effort_semantics() {
    let tmp = TempDir::new().expect("temp dir");
    let db_path = tmp.path().join("db");
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };

    let db = MemFuse::open_with_config(&db_path, config)
        .await
        .expect("open db");

    let collection = db
        .collection("best-effort-ingest")
        .await
        .expect("collection");

    let embedder = Arc::new(DummyEmbedder { dim: 4 });
    let pipeline = IngestionPipeline::new(embedder);

    let folder_path = tmp.path().join("batch_docs");
    std::fs::create_dir(&folder_path).expect("create folder");

    let valid_file = folder_path.join("valid.md");
    let corrupt_pdf = folder_path.join("corrupt.pdf");

    std::fs::write(&valid_file, "# Valid Markdown\nThis content is valid.").expect("write valid");
    std::fs::write(&corrupt_pdf, b"Not a valid PDF content").expect("write corrupt pdf");

    let reports = pipeline
        .ingest_folder(&folder_path, &collection)
        .await
        .expect("ingest_folder should succeed with best-effort results");

    assert_eq!(
        reports.len(),
        2,
        "Batch report should contain reports for both supported files"
    );

    let valid_report = reports
        .iter()
        .find(|r| r.file_path.contains("valid.md"))
        .unwrap();
    assert!(valid_report.chunks_created > 0);
    assert!(valid_report.errors.is_empty());

    let corrupt_report = reports
        .iter()
        .find(|r| r.file_path.contains("corrupt.pdf"))
        .unwrap();
    assert_eq!(corrupt_report.chunks_created, 0);
    assert!(!corrupt_report.errors.is_empty());
    assert!(corrupt_report.errors[0].contains("PDF extraction failed"));
}

#[tokio::test]
async fn test_ingestion_creates_graph_entities() {
    let tmp = TempDir::new().expect("temp dir");
    let db_path = tmp.path().join("db");
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };

    let db = MemFuse::open_with_config(&db_path, config)
        .await
        .expect("open db");

    let collection = db
        .collection("graph-entity-test")
        .await
        .expect("collection");

    let embedder = Arc::new(DummyEmbedder { dim: 4 });
    let pipeline = IngestionPipeline::new(embedder);

    let doc_path = tmp.path().join("anfrage.md");
    let content = "Kunde Müller GmbH hat eine Anfrage gestellt.";
    std::fs::write(&doc_path, content).expect("write anfrage md");

    let report = pipeline
        .ingest_file(&doc_path, &collection)
        .await
        .expect("ingest_file");

    assert!(report.chunks_created > 0);
    assert!(report.errors.is_empty());

    let graph = collection.graph_index();
    assert!(
        graph.entity_count() > 0,
        "Graph index should contain extracted entities"
    );

    let entity_id = memfuse_core::EntityId::from("Kunde Müller GmbH");
    let traversal = graph.traverse(entity_id, 2).await.expect("traverse graph");

    assert!(
        !traversal.is_empty(),
        "Extracted entity should exist in graph and have connections"
    );
}

#[tokio::test]
async fn test_cooccurrence_edges_capped_for_large_entity_count() {
    let tmp = TempDir::new().expect("temp dir");
    let db_path = tmp.path().join("db");
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };

    let db = MemFuse::open_with_config(&db_path, config)
        .await
        .expect("open db");

    let collection = db
        .collection("cap-edges-test")
        .await
        .expect("collection");

    let embedder = Arc::new(DummyEmbedder { dim: 4 });
    let pipeline = IngestionPipeline::new(embedder);

    let doc_path = tmp.path().join("many_entities.md");
    let content = r#"
    Partner Alpha Corp AG, Beta Corp AG, Gamma Corp AG, Delta Corp AG,
    Epsilon Corp AG, Zeta Corp AG, Eta Corp AG, Theta Corp AG, Iota Corp AG,
    Kappa Corp AG, Lambda Corp AG, Mu Corp AG, Nu Corp AG, Xi Corp AG, Omicron Corp AG
    haben eine Vereinbarung getroffen.
    "#;
    std::fs::write(&doc_path, content).expect("write md");

    let report = pipeline
        .ingest_file(&doc_path, &collection)
        .await
        .expect("ingest_file");

    assert!(report.chunks_created > 0);
    assert!(report.errors.is_empty());

    let graph = collection.graph_index();
    let total_edges = graph.stats().await.expect("graph stats").num_edges;

    let max_cooccurrence_directional_edges = MAX_COOCCURRENCE_ENTITIES_PER_CHUNK
        * (MAX_COOCCURRENCE_ENTITIES_PER_CHUNK - 1);

    // Total edges = cooccurrence_edges (capped at MAX_COOCCURRENCE * (MAX_COOCCURRENCE - 1)) + contains_edges + mentioned_in_edges
    // For 15 entities, contains + mentioned_in = 30 edges.
    // Uncapped cooccurrence would be 15 * 14 = 210 edges (+ 30 = 240 total).
    // Capped cooccurrence (12 * 11 = 132 edges) + 30 = 162 total edges.
    assert!(
        total_edges <= max_cooccurrence_directional_edges + (2 * 15),
        "Total edges ({total_edges}) exceeds capped bound ({})",
        max_cooccurrence_directional_edges + (2 * 15)
    );
}

#[tokio::test]
async fn test_ingestion_with_entity_extraction_disabled_skips_graph_writes() {
    let tmp = TempDir::new().expect("temp dir");
    let db_path = tmp.path().join("db");
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };

    let db = MemFuse::open_with_config(&db_path, config)
        .await
        .expect("open db");

    let collection = db
        .collection("disabled-graph-test")
        .await
        .expect("collection");

    let embedder = Arc::new(DummyEmbedder { dim: 4 });
    let pipeline = IngestionPipeline::new(embedder).with_extract_entities(false);

    let doc_path = tmp.path().join("anfrage_disabled.md");
    let content = "Kunde Müller GmbH hat eine Anfrage gestellt.";
    std::fs::write(&doc_path, content).expect("write md");

    let report = pipeline
        .ingest_file(&doc_path, &collection)
        .await
        .expect("ingest_file");

    assert!(report.chunks_created > 0);
    assert!(report.errors.is_empty());

    let graph = collection.graph_index();
    let extracted_term_id = memfuse_core::EntityId::from("Kunde Müller GmbH");
    assert!(
        !graph.entity_exists(extracted_term_id),
        "Extracted term entity should NOT exist in graph when entity extraction is disabled"
    );
    assert_eq!(
        graph.stats().await.expect("stats").num_edges,
        0,
        "Graph index should contain 0 edges when entity extraction is disabled"
    );

    let results = collection
        .query()
        .text("Anfrage")
        .embedding([0.1, 0.1, 0.1, 0.1])
        .k(5)
        .execute()
        .await
        .expect("search");

    assert!(!results.is_empty(), "Search should still return vector/text results");
}
