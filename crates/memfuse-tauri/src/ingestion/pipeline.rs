use memfuse_core::{DocId, Result};
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

/// Trait für eine Embedding-Funktion — abstrahiert vom konkreten
/// Backend (Ollama, OpenAI, lokales ONNX-Modell etc.).
#[async_trait::async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;
}

pub struct IngestionPipeline {
    embedder: Arc<dyn EmbeddingProvider>,
}

impl IngestionPipeline {
    pub fn new(embedder: Arc<dyn EmbeddingProvider>) -> Self {
        Self { embedder }
    }

    /// Liest eine Datei, erkennt das Format anhand der Endung, chunked den
    /// Text mit dem bestehenden MarkdownChunker und speichert die Chunks
    /// mit Embeddings in der übergebenen Collection.
    pub async fn ingest_file(
        &self,
        path: &Path,
        collection: &Collection,
    ) -> Result<IngestReport> {
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let raw_text = match extension.as_str() {
            "pdf" => crate::ingestion::pdf::extract_pdf_text(path)?,
            "docx" => crate::ingestion::docx::extract_docx_text(path)?,
            "md" | "markdown" | "txt" => std::fs::read_to_string(path).map_err(|e| {
                memfuse_core::MemFuseError::Internal(format!("Datei lesen fehlgeschlagen: {e}"))
            })?,
            "eml" => {
                let email = crate::ingestion::email::extract_email(path)?;
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
            .unwrap_or_default()
            .to_string_lossy();

        // Nutzt den bereits bestehenden MarkdownChunker aus memfuse-db
        let chunker = MarkdownChunker::with_defaults();

        // DocId generator uses key hashing, we can pass a dummy base DocId or hash
        let base_doc_id = DocId::from_key(&file_name)?;
        let chunks = chunker.chunk(base_doc_id, &raw_text);

        let mut created = 0;
        let mut errors = Vec::new();

        for (idx, chunk) in chunks.into_iter().enumerate() {
            match self.embedder.embed(&chunk.content).await {
                Ok(embedding) => {
                    let doc_id = format!("{}#{}", file_name, idx);
                    let mut metadata = chunk.metadata.unwrap_or_else(|| serde_json::json!({}));
                    if let Some(obj) = metadata.as_object_mut() {
                        obj.insert(
                            "text".to_string(),
                            serde_json::Value::String(chunk.content),
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
                    }
                }
                Err(e) => errors.push(format!("Embedding fehlgeschlagen: {e}")),
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
