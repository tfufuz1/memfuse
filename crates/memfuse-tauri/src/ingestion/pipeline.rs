use memfuse_core::{
    ContextChunk, DocId, Edge, Entity, EntityId, GraphIndex, MemFuseError, Result,
    TextEmbeddingEngine,
};
use memfuse_db::{chunker::MarkdownChunker, Collection};
use std::path::Path;
use std::sync::Arc;

/// Maximum allowed file size for ingestion (100 MB).
pub const MAX_INGEST_FILE_SIZE_BYTES: u64 = 100 * 1024 * 1024;

/// Ergebnis eines Ingestion-Vorgangs.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct IngestReport {
    pub file_path: String,
    pub chunks_created: usize,
    pub errors: Vec<String>,
}

pub struct IngestionPipeline {
    embedder: Arc<dyn TextEmbeddingEngine>,
}

/// Extrahiert reinen Text aus übergebenen Datei-Bytes basierend auf der Dateiendung.
pub fn extract_text_from_bytes(bytes: &[u8], extension: &str) -> Result<String> {
    if bytes.len() as u64 > MAX_INGEST_FILE_SIZE_BYTES {
        tracing::warn!(
            file_size_bytes = bytes.len(),
            max_size_bytes = MAX_INGEST_FILE_SIZE_BYTES,
            "File size exceeds maximum allowed size during byte extraction"
        );
        return Err(MemFuseError::InvalidInput(
            "File size exceeds maximum allowed size of 100 MB".to_string(),
        ));
    }

    if bytes.is_empty() {
        return Ok(String::new());
    }

    let ext = extension.trim_start_matches('.').to_lowercase();
    match ext.as_str() {
        "pdf" => crate::ingestion::pdf::extract_pdf_bytes(bytes),
        "docx" => crate::ingestion::docx::extract_docx_bytes(bytes),
        "md" | "markdown" | "txt" => String::from_utf8(bytes.to_vec()).map_err(|e| {
            MemFuseError::InvalidInput(format!("Datei enthält ungültiges UTF-8: {e}"))
        }),
        "eml" => {
            let email = crate::ingestion::email::extract_email_bytes(bytes)?;
            Ok(format!(
                "Betreff: {}\nVon: {}\n\n{}",
                email.subject, email.from, email.body
            ))
        }
        "" => Err(MemFuseError::InvalidInput("File has no extension".into())),
        ext => Err(MemFuseError::InvalidInput(format!(
            "Unsupported file type: .{ext}"
        ))),
    }
}

/// Helper zur Ingestion/Chunking von Raw-Bytes ohne Datenbank-Anbindung (für Standalone-Tests).
pub fn ingest_bytes(bytes: &[u8], filename_or_ext: &str) -> Result<Vec<String>> {
    let ext = Path::new(filename_or_ext)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or(if filename_or_ext.contains('.') {
            ""
        } else {
            filename_or_ext
        });

    let raw_text = extract_text_from_bytes(bytes, ext)?;
    if raw_text.trim().is_empty() {
        return Ok(Vec::new());
    }

    let chunker = MarkdownChunker::with_defaults();
    let base_doc_id = DocId::from_key(filename_or_ext)?;
    let chunks = chunker.chunk(base_doc_id, &raw_text);
    Ok(chunks.into_iter().map(|c| c.content).collect())
}

impl IngestionPipeline {
    pub fn new(embedder: Arc<dyn TextEmbeddingEngine>) -> Self {
        Self { embedder }
    }

    /// Liest eine Datei, erkennt das Format anhand der Endung, chunked den
    /// Text mit dem bestehenden MarkdownChunker und speichert die Chunks
    /// mit Embeddings in der übergebenen Collection.
    pub async fn ingest_file(&self, path: &Path, collection: &Collection) -> Result<IngestReport> {
        let metadata = std::fs::metadata(path).map_err(|e| {
            MemFuseError::Internal(format!(
                "Datei-Metadaten lesen fehlgeschlagen für {:?}: {e}",
                path
            ))
        })?;

        if metadata.len() > MAX_INGEST_FILE_SIZE_BYTES {
            tracing::warn!(
                file_path = %path.display(),
                file_size_bytes = metadata.len(),
                max_size_bytes = MAX_INGEST_FILE_SIZE_BYTES,
                "File size exceeds maximum allowed size of 100 MB"
            );
            return Err(MemFuseError::InvalidInput(format!(
                "File size ({:.2} MB) exceeds maximum allowed size of 100 MB",
                metadata.len() as f64 / (1024.0 * 1024.0)
            )));
        }

        let path_buf = path.to_path_buf();
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let extract_res = tokio::task::spawn_blocking(move || {
            std::panic::catch_unwind(|| {
                let bytes = std::fs::read(&path_buf).map_err(|e| {
                    MemFuseError::Internal(format!(
                        "Datei lesen fehlgeschlagen für {:?}: {e}",
                        path_buf
                    ))
                })?;
                extract_text_from_bytes(&bytes, &extension)
            })
        })
        .await;

        let raw_text = match extract_res {
            Ok(Ok(Ok(text))) => text,
            Ok(Ok(Err(e))) => {
                return Ok(IngestReport {
                    file_path: path.display().to_string(),
                    chunks_created: 0,
                    errors: vec![e.to_string()],
                });
            }
            Ok(Err(_panic_payload)) => {
                return Ok(IngestReport {
                    file_path: path.display().to_string(),
                    chunks_created: 0,
                    errors: vec!["Extraction panicked on malformed file".to_string()],
                });
            }
            Err(join_err) => {
                return Ok(IngestReport {
                    file_path: path.display().to_string(),
                    chunks_created: 0,
                    errors: vec![format!("Extraction task failed: {join_err:?}")],
                });
            }
        };

        let file_name = path
            .file_name()
            .ok_or_else(|| {
                MemFuseError::InvalidInput(format!("Pfad hat keinen Dateinamen: {:?}", path))
            })?
            .to_string_lossy();

        if raw_text.trim().is_empty() {
            return Ok(IngestReport {
                file_path: path.display().to_string(),
                chunks_created: 0,
                errors: Vec::new(),
            });
        }

        // Nutzt den bereits bestehenden MarkdownChunker aus memfuse-db
        let chunker = MarkdownChunker::with_defaults();

        // DocId generator uses key hashing, we can pass a dummy base DocId or hash
        let base_doc_id = DocId::from_key(&file_name)?;
        let chunks = chunker.chunk(base_doc_id, &raw_text);

        let mut created = 0;
        let mut errors = Vec::new();

        use futures_util::stream::{self, StreamExt};
        const EMBED_CONCURRENCY: usize = 8;

        let embedding_results: Vec<(usize, ContextChunk, Result<Vec<f32>>)> =
            stream::iter(chunks.into_iter().enumerate())
                .map(|(idx, chunk)| {
                    let embedder = Arc::clone(&self.embedder);
                    async move {
                        let text_to_embed = chunk.combined_text_owned();
                        let res = embedder.embed(&text_to_embed).await;
                        (idx, chunk, res)
                    }
                })
                .buffer_unordered(EMBED_CONCURRENCY)
                .collect()
                .await;

        let mut sorted = embedding_results;
        sorted.sort_by_key(|(idx, _, _)| *idx);

        for (idx, chunk, embed_res) in sorted {
            let index_text = chunk.combined_text_owned();
            let raw_content = chunk.content;
            match embed_res {
                Ok(embedding) => {
                    let doc_id = format!("{}#{}", file_name, idx);
                    let mut metadata = chunk.metadata.unwrap_or_else(|| serde_json::json!({}));
                    if let Some(obj) = metadata.as_object_mut() {
                        obj.insert("text".to_string(), serde_json::Value::String(index_text));
                        if let Some(prefix) = &chunk.contextual_prefix {
                            obj.insert(
                                "contextual_prefix".to_string(),
                                serde_json::Value::String(prefix.clone()),
                            );
                        }
                        obj.insert(
                            "source".to_string(),
                            serde_json::Value::String(path.display().to_string()),
                        );
                    }

                    if let Err(e) = collection.insert(&doc_id, &embedding, Some(metadata)).await {
                        errors.push(format!("Insert fehlgeschlagen: {e}"));
                    } else {
                        created += 1;

                        // Extrahiere Entitäten aus dem Chunk-Text
                        let extracted_entities =
                            crate::ingestion::entities::SimpleEntityExtractor::extract(
                                &raw_content,
                            );

                        if !extracted_entities.is_empty() {
                            let graph = collection.graph_index();
                            let tx = collection.allocate_tx()?;

                            for entity_id in &extracted_entities {
                                let entity =
                                    Entity::new(*entity_id, "ExtractedTerm", "ExtractedTerm");
                                if let Err(e) = graph.add_entity(tx, entity).await {
                                    tracing::warn!(
                                        "Entity-Insert fehlgeschlagen für {:?}: {e}",
                                        entity_id
                                    );
                                }
                            }

                            for i in 0..extracted_entities.len() {
                                for j in (i + 1)..extracted_entities.len() {
                                    if let Err(e) = graph
                                        .add_edge(
                                            tx,
                                            Edge::new(
                                                extracted_entities[i],
                                                extracted_entities[j],
                                                "co_occurrence",
                                            )
                                            .with_weight(0.5),
                                        )
                                        .await
                                    {
                                        tracing::warn!(
                                            "Edge-Insert (co_occurrence ->) fehlgeschlagen: {e}"
                                        );
                                    }
                                    if let Err(e) = graph
                                        .add_edge(
                                            tx,
                                            Edge::new(
                                                extracted_entities[j],
                                                extracted_entities[i],
                                                "co_occurrence",
                                            )
                                            .with_weight(0.5),
                                        )
                                        .await
                                    {
                                        tracing::warn!(
                                            "Edge-Insert (co_occurrence <-) fehlgeschlagen: {e}"
                                        );
                                    }
                                }
                            }

                            let doc_entity_id = EntityId::from(doc_id.as_str());
                            let doc_entity = Entity::new(doc_entity_id, doc_id.clone(), "Document");
                            if let Err(e) = graph.add_entity(tx, doc_entity).await {
                                tracing::warn!(
                                    "Doc Entity-Insert fehlgeschlagen für {doc_id}: {e}"
                                );
                            }

                            for term_id in &extracted_entities {
                                if let Err(e) = graph
                                    .add_edge(
                                        tx,
                                        Edge::new(doc_entity_id, *term_id, "contains")
                                            .with_weight(0.8),
                                    )
                                    .await
                                {
                                    tracing::warn!("Edge-Insert (contains) fehlgeschlagen: {e}");
                                }
                                if let Err(e) = graph
                                    .add_edge(
                                        tx,
                                        Edge::new(*term_id, doc_entity_id, "mentioned_in")
                                            .with_weight(0.8),
                                    )
                                    .await
                                {
                                    tracing::warn!(
                                        "Edge-Insert (mentioned_in) fehlgeschlagen: {e}"
                                    );
                                }
                            }

                            if let Err(e) = graph.commit(tx).await {
                                tracing::warn!("Graph tx commit failed: {e}");
                            }
                        }
                    }
                }
                Err(e) => errors.push(format!("Chunk {idx}: Embedding fehlgeschlagen: {e}")),
            }
        }

        Ok(IngestReport {
            file_path: path.display().to_string(),
            chunks_created: created,
            errors,
        })
    }

    /// Indiziert alle unterstützten Dateien in einem Ordner (rekursiv).
    pub async fn ingest_folder(
        &self,
        folder: &Path,
        collection: &Collection,
    ) -> Result<Vec<IngestReport>> {
        let mut reports = Vec::new();
        let supported = ["pdf", "docx", "md", "markdown", "txt", "eml"];

        for entry in walkdir::WalkDir::new(folder)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let ext = entry
                .path()
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            if supported.contains(&ext.as_str()) {
                let report = match self.ingest_file(entry.path(), collection).await {
                    Ok(rep) => rep,
                    Err(e) => IngestReport {
                        file_path: entry.path().display().to_string(),
                        chunks_created: 0,
                        errors: vec![e.to_string()],
                    },
                };
                reports.push(report);
            }
        }

        Ok(reports)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipeline_empty_pdf_no_panic() {
        let result = ingest_bytes(b"", "test.pdf");
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty()); // unwrap
    }

    #[test]
    fn test_command_ingest_unsupported_format() {
        let result = ingest_bytes(b"some content", "document.xyz");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Unsupported file type: .xyz"),
            "Error message was: {err_msg}"
        );
    }

    #[test]
    fn test_command_ingest_no_extension() {
        let result = extract_text_from_bytes(b"some content", "");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("File has no extension"),
            "Error message was: {err_msg}"
        );
    }

    #[test]
    fn test_ingest_corrupted_pdf_returns_error() {
        let result = extract_text_from_bytes(b"not a real pdf content", "pdf");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("PDF extraction failed") || err_msg.contains("panicked"),
            "Error message was: {err_msg}"
        );
    }

    #[test]
    fn test_ingest_oversized_file_returns_error() {
        let oversized = vec![0u8; MAX_INGEST_FILE_SIZE_BYTES as usize + 1];
        let result = extract_text_from_bytes(&oversized, "txt");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("File size exceeds maximum allowed size of 100 MB"),
            "Error message was: {err_msg}"
        );
    }

    struct MockEmbedder {
        fail_on_even: bool,
    }

    #[async_trait::async_trait]
    impl TextEmbeddingEngine for MockEmbedder {
        async fn embed(&self, text: &str) -> Result<Vec<f32>> {
            if self.fail_on_even && text.contains("Even") {
                Err(MemFuseError::Internal("Mock embedder failure".into()))
            } else {
                Ok(vec![0.1; 768])
            }
        }
    }

    #[tokio::test]
    async fn test_ingest_panic_recovery() {
        let temp_dir = tempfile::tempdir().unwrap(); // unwrap
        let file_path = temp_dir.path().join("malformed.pdf");
        std::fs::write(&file_path, b"not a real pdf content").unwrap(); // unwrap

        let embedder = Arc::new(MockEmbedder {
            fail_on_even: false,
        });
        let pipeline = IngestionPipeline::new(embedder);

        // Standard MemFuse DB setup in memory or temp dir
        let db = memfuse_db::MemFuse::open(temp_dir.path()).await.unwrap(); // unwrap
        let collection = db.collection("test_col").await.unwrap(); // unwrap

        let report = pipeline.ingest_file(&file_path, &collection).await.unwrap(); // unwrap
        assert_eq!(report.chunks_created, 0);
        assert!(!report.errors.is_empty());
        assert!(
            report.errors[0].contains("PDF extraction failed")
                || report.errors[0].contains("panicked")
        );
    }

    #[tokio::test]
    async fn test_partial_failure_error_aggregation() {
        let temp_dir = tempfile::tempdir().unwrap(); // unwrap
        let file_path = temp_dir.path().join("test_partial.md");
        // Create 3 long sections with distinct headings so MarkdownChunker creates 3 separate chunks (>50 tokens each)
        let chunk1 = format!("# Section 1\n{}\nChunk Odd 1\n\n", "word ".repeat(60));
        let chunk2 = format!("# Section 2\n{}\nChunk Even 2\n\n", "word ".repeat(60));
        let chunk3 = format!("# Section 3\n{}\nChunk Odd 3\n\n", "word ".repeat(60));
        let content = format!("{chunk1}{chunk2}{chunk3}");
        std::fs::write(&file_path, content).unwrap(); // unwrap

        let embedder = Arc::new(MockEmbedder { fail_on_even: true });
        let pipeline = IngestionPipeline::new(embedder);

        let db = memfuse_db::MemFuse::open(temp_dir.path()).await.unwrap(); // unwrap
        let collection = db.collection("test_col").await.unwrap(); // unwrap

        let report = pipeline.ingest_file(&file_path, &collection).await.unwrap(); // unwrap
        assert_eq!(report.chunks_created, 2);
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].contains("Embedding fehlgeschlagen"));
    }
}
