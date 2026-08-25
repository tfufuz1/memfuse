use memfuse_core::{
    ContextChunk, DocId, Edge, Entity, EntityId, GraphIndex, Result, TextEmbeddingEngine, TxId,
};
use memfuse_db::{chunker::MarkdownChunker, Collection};
use std::path::Path;
use std::sync::Arc;

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

impl IngestionPipeline {
    pub fn new(embedder: Arc<dyn TextEmbeddingEngine>) -> Self {
        Self { embedder }
    }

    /// Liest eine Datei, erkennt das Format anhand der Endung, chunked den
    /// Text mit dem bestehenden MarkdownChunker und speichert die Chunks
    /// mit Embeddings in der übergebenen Collection.
    pub async fn ingest_file(&self, path: &Path, collection: &Collection) -> Result<IngestReport> {
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let raw_text = match extension.as_str() {
            "pdf" => crate::ingestion::pdf::extract_pdf_text(path).await?,
            "docx" => crate::ingestion::docx::extract_docx_text(path).await?,
            "md" | "markdown" | "txt" => std::fs::read_to_string(path).map_err(|e| {
                memfuse_core::MemFuseError::Internal(format!("Datei lesen fehlgeschlagen: {e}"))
            })?,
            "eml" => {
                let email = crate::ingestion::email::extract_email(path).await?;
                format!(
                    "Betreff: {}\nVon: {}\n\n{}",
                    email.subject, email.from, email.body
                )
            }
            other => {
                return Ok(IngestReport {
                    file_path: path.display().to_string(),
                    chunks_created: 0,
                    errors: vec![format!("Nicht unterstütztes Format: .{other}")],
                });
            }
        };

        let file_name = path
            .file_name()
            .ok_or_else(|| {
                memfuse_core::MemFuseError::InvalidInput(format!(
                    "Pfad hat keinen Dateinamen: {:?}",
                    path
                ))
            })?
            .to_string_lossy();

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
                        let res = embedder.embed(&chunk.content).await;
                        (idx, chunk, res)
                    }
                })
                .buffer_unordered(EMBED_CONCURRENCY)
                .collect()
                .await;

        let mut sorted = embedding_results;
        sorted.sort_by_key(|(idx, _, _)| *idx);

        for (idx, chunk, embed_res) in sorted {
            let chunk_text = chunk.content;
            match embed_res {
                Ok(embedding) => {
                    let doc_id = format!("{}#{}", file_name, idx);
                    let mut metadata = chunk.metadata.unwrap_or_else(|| serde_json::json!({}));
                    if let Some(obj) = metadata.as_object_mut() {
                        obj.insert(
                            "text".to_string(),
                            serde_json::Value::String(chunk_text.clone()),
                        );
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
                            crate::ingestion::entities::SimpleEntityExtractor::extract(&chunk_text);
                        if !extracted_entities.is_empty() {
                            let graph = collection.graph_index();
                            let tx = collection.allocate_tx();

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
                let report = self.ingest_file(entry.path(), collection).await?;
                reports.push(report);
            }
        }

        Ok(reports)
    }
}
