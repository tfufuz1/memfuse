use memfuse_core::{BoxFuture, GraphIndex, Result, TextEmbeddingEngine};
use memfuse_db::{MemFuse, MemFuseConfig};
use memfuse_tauri_lib::ingestion::{IngestionPipeline, MAX_COOCCURRENCE_ENTITIES_PER_CHUNK};
use std::sync::Arc;
use tempfile::TempDir;

struct DummyEmbedder {
    dim: usize,
}

impl TextEmbeddingEngine for DummyEmbedder {
    fn embed<'a>(&'a self, _text: &'a str) -> BoxFuture<'a, Result<Vec<f32>>> {
        Box::pin(async move {
            Ok(vec![0.1; self.dim])
        })
    }

    fn embed_batch<'a>(&'a self, texts: &'a [&'a str]) -> BoxFuture<'a, Result<Vec<Vec<f32>>>> {
        Box::pin(async move {
            Ok(vec![vec![0.1; self.dim]; texts.len()])
        })
    }
}

#[tokio::test]
async fn test_ingestion_pipeline_markdown() {
    let tmp = TempDir::new().expect("temp dir");
    let db_path = tmp.path().join("db");
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };

    let db = MemFuse::open_with_config(&db_path, config)
        .await
        .expect("open db");

    let collection = db.collection("test-col").await.expect("collection");

    let embedder = Arc::new(DummyEmbedder { dim: 4 });
    let pipeline = IngestionPipeline::new(embedder);

    let doc_path = tmp.path().join("test.md");
    let content = "# Title\n\nThis is paragraph one.\n\n## Section 2\n\nThis is paragraph two.";
    std::fs::write(&doc_path, content).expect("write md");

    let report = pipeline
        .ingest_file(&doc_path, &collection)
        .await
        .expect("ingest_file");

    assert_eq!(report.chunks_created, 2);
    assert!(report.errors.is_empty());

    let results = collection
        .query()
        .embedding([0.1, 0.1, 0.1, 0.1])
        .k(10)
        .execute()
        .await
        .expect("search");

    assert_eq!(results.len(), 2);
}

#[tokio::test]
async fn test_ingestion_pipeline_txt() {
    let tmp = TempDir::new().expect("temp dir");
    let db_path = tmp.path().join("db");
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };

    let db = MemFuse::open_with_config(&db_path, config)
        .await
        .expect("open db");

    let collection = db.collection("test-col-txt").await.expect("collection");

    let embedder = Arc::new(DummyEmbedder { dim: 4 });
    let pipeline = IngestionPipeline::new(embedder);

    let doc_path = tmp.path().join("test.txt");
    let content = "First block of plain text.\n\nSecond block of plain text.";
    std::fs::write(&doc_path, content).expect("write txt");

    let report = pipeline
        .ingest_file(&doc_path, &collection)
        .await
        .expect("ingest_file");

    assert_eq!(report.chunks_created, 2);
    assert!(report.errors.is_empty());

    let results = collection
        .query()
        .embedding([0.1, 0.1, 0.1, 0.1])
        .k(10)
        .execute()
        .await
        .expect("search");

    assert_eq!(results.len(), 2);
}

#[tokio::test]
async fn test_ingestion_pipeline_directory() {
    let tmp = TempDir::new().expect("temp dir");
    let db_path = tmp.path().join("db");
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };

    let db = MemFuse::open_with_config(&db_path, config)
        .await
        .expect("open db");

    let collection = db.collection("test-col-dir").await.expect("collection");

    let embedder = Arc::new(DummyEmbedder { dim: 4 });
    let pipeline = IngestionPipeline::new(embedder);

    let doc1_path = tmp.path().join("file1.md");
    let doc2_path = tmp.path().join("file2.txt");
    let ignored_path = tmp.path().join("image.png");

    std::fs::write(&doc1_path, "# Doc 1\nContent 1").expect("write doc1");
    std::fs::write(&doc2_path, "Doc 2 content").expect("write doc2");
    std::fs::write(&ignored_path, "binary data").expect("write ignored");

    let reports = pipeline
        .ingest_folder(tmp.path(), &collection)
        .await
        .expect("ingest_folder");

    let total_chunks: usize = reports.iter().map(|r| r.chunks_created).sum();
    assert_eq!(reports.len(), 2);
    assert_eq!(total_chunks, 2);
    assert!(reports.iter().all(|r| r.errors.is_empty()));

    let results = collection
        .query()
        .embedding([0.1, 0.1, 0.1, 0.1])
        .k(10)
        .execute()
        .await
        .expect("search");

    assert_eq!(results.len(), 2);
}

#[tokio::test]
async fn test_ingestion_pipeline_unsupported_format() {
    let tmp = TempDir::new().expect("temp dir");
    let db_path = tmp.path().join("db");
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };

    let db = MemFuse::open_with_config(&db_path, config)
        .await
        .expect("open db");

    let collection = db.collection("test-col-err").await.expect("collection");

    let embedder = Arc::new(DummyEmbedder { dim: 4 });
    let pipeline = IngestionPipeline::new(embedder);

    let doc_path = tmp.path().join("archive.zip");
    std::fs::write(&doc_path, "fake zip").expect("write zip");

    let report = pipeline
        .ingest_file(&doc_path, &collection)
        .await
        .expect("ingest_file");

    assert_eq!(report.chunks_created, 0);
    assert_eq!(report.errors.len(), 1);
    assert!(report.errors[0].contains("Nicht unterstütztes Dateiformat"));
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

    let collection = db.collection("test-col-graph").await.expect("collection");

    let embedder = Arc::new(DummyEmbedder { dim: 4 });
    let pipeline = IngestionPipeline::new(embedder);

    let doc_path = tmp.path().join("anfrage.md");
    let content = "# Kundenanfrage\n\nKunde Müller GmbH hat ein Angebot angefordert.";
    std::fs::write(&doc_path, content).expect("write md");

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

    let collection = db.collection("cap-edges-test").await.expect("collection");

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

    let max_cooccurrence_directional_edges =
        MAX_COOCCURRENCE_ENTITIES_PER_CHUNK * (MAX_COOCCURRENCE_ENTITIES_PER_CHUNK - 1);

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

    assert!(
        !results.is_empty(),
        "Search should still return vector/text results"
    );
}

#[tokio::test]
async fn test_reimport_identical_content_is_skipped() {
    let tmp = TempDir::new().expect("temp dir");
    let db_path = tmp.path().join("db");
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };

    let db = MemFuse::open_with_config(&db_path, config)
        .await
        .expect("open db");

    let collection = db.collection("duplicate-test").await.expect("collection");

    let embedder = Arc::new(DummyEmbedder { dim: 4 });
    let pipeline = IngestionPipeline::new(embedder);

    let doc_path = tmp.path().join("invoice.md");
    let content = "# Invoice\nAmount: 100 EUR\nCustomer: ACME";
    std::fs::write(&doc_path, content).expect("write invoice");

    let first_report = pipeline
        .ingest_file(&doc_path, &collection)
        .await
        .expect("first ingest");

    assert!(first_report.chunks_created > 0);
    assert!(!first_report.skipped_as_duplicate);

    let second_report = pipeline
        .ingest_file(&doc_path, &collection)
        .await
        .expect("second ingest");

    assert_eq!(second_report.chunks_created, 0);
    assert!(second_report.skipped_as_duplicate);
    assert!(second_report
        .errors
        .iter()
        .any(|e| e.contains("Re-Import übersprungen")));

    let results = collection
        .query()
        .embedding([0.1, 0.1, 0.1, 0.1])
        .k(10)
        .execute()
        .await
        .expect("search");

    assert_eq!(results.len(), first_report.chunks_created);
}

#[tokio::test]
async fn test_reimport_modified_content_is_not_skipped() {
    let tmp = TempDir::new().expect("temp dir");
    let db_path = tmp.path().join("db");
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };

    let db = MemFuse::open_with_config(&db_path, config)
        .await
        .expect("open db");

    let collection = db.collection("modified-test").await.expect("collection");

    let embedder = Arc::new(DummyEmbedder { dim: 4 });
    let pipeline = IngestionPipeline::new(embedder);

    let doc_path = tmp.path().join("invoice.md");
    let initial_content = "# Invoice v1\nAmount: 100 EUR\nCustomer: ACME";
    std::fs::write(&doc_path, initial_content).expect("write invoice v1");

    let first_report = pipeline
        .ingest_file(&doc_path, &collection)
        .await
        .expect("first ingest");

    assert!(first_report.chunks_created > 0);
    assert!(!first_report.skipped_as_duplicate);

    let modified_content = "# Invoice v2\nAmount: 150 EUR\nCustomer: ACME";
    std::fs::write(&doc_path, modified_content).expect("write invoice v2");

    let second_report = pipeline
        .ingest_file(&doc_path, &collection)
        .await
        .expect("second ingest");

    assert!(second_report.chunks_created > 0);
    assert!(!second_report.skipped_as_duplicate);
}
