// FILE-CONTEXT
// ZWECK: Import-Funktionalität & Zusammenfassung für Memory-Export-Format v1 (Schema Version "1.0").
// INVARIANTEN: Schema-Versionsprüfung (Pflicht); Idempotenz bezüglich ID (Upsert); Transaktionale Verlässlichkeit.
// STAND: TS:2026-09-12

use crate::export::{ExportCollectionV1, ExportDocumentV1, SCHEMA_VERSION_V1};
use crate::collection::Collection;
use crate::MemFuse;
use memfuse_core::{Result, StorageEngine, VectorIndex};
use serde::{Deserialize, Serialize};

/// Zusammenfassung eines Import-Vorgangs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ImportSummary {
    /// Anzahl erfolgreich importierter oder aktualisierter Memory-Einträge
    pub imported_memories: usize,
    /// Anzahl übersprungener Memory-Einträge
    pub skipped_memories: usize,
    /// Anzahl erfolgreich importierter Graph-Relationen
    pub imported_relations: usize,
    /// Anzahl übersprungener Graph-Relationen
    pub skipped_relations: usize,
}

impl<S: StorageEngine, V: VectorIndex> Collection<S, V> {
    /// Importiert alle Memories und Graph-Relationen einer ExportCollectionV1 idempotent (Upsert-Semantik).
    pub async fn import_memories(&self, col_export: ExportCollectionV1) -> Result<ImportSummary> {
        let mut summary = ImportSummary::default();

        for mem in col_export.memories {
            let mut meta = mem.metadata.unwrap_or_else(|| serde_json::json!({}));
            if let Some(obj) = meta.as_object_mut() {
                obj.insert(
                    "memory_type".to_string(),
                    serde_json::to_value(mem.memory_type)
                        .map_err(|e| memfuse_core::MemFuseError::Serialization(e.to_string()))?,
                );
                if let Some(ref text) = mem.content {
                    if !obj.contains_key("text") && !obj.contains_key("content") {
                        obj.insert("content".to_string(), serde_json::json!(text));
                    }
                }
                if let Some(model) = mem.embedding_model {
                    obj.insert("embedding_model".to_string(), serde_json::json!(model));
                }
                if let Some(created) = mem.created_at {
                    if !obj.contains_key("created_at_tx") {
                        obj.insert("created_at_tx".to_string(), serde_json::json!(created));
                    }
                }
                if !mem.links.is_empty() {
                    let links_val = serde_json::to_value(&mem.links)
                        .map_err(|e| memfuse_core::MemFuseError::Serialization(e.to_string()))?;
                    obj.insert("links".to_string(), links_val);
                }
            }

            let res = if let Some(ref emb) = mem.embedding {
                if emb.len() == self.dimension() {
                    self.upsert(&mem.id, emb, Some(meta)).await
                } else if let Some(ref text) = mem.content {
                    self.upsert_text_only(&mem.id, text, Some(meta)).await
                } else {
                    Err(memfuse_core::MemFuseError::invalid_input(format!(
                        "Dimension mismatch for memory '{}': expected {}, got {}",
                        mem.id,
                        self.dimension(),
                        emb.len()
                    )))
                }
            } else if let Some(ref text) = mem.content {
                self.upsert_text_only(&mem.id, text, Some(meta)).await
            } else {
                self.put_kv(&mem.id, &meta).await
            };

            match res {
                Ok(_) => summary.imported_memories += 1,
                Err(e) => {
                    tracing::warn!(id = %mem.id, error = %e, "Skipped memory during import");
                    summary.skipped_memories += 1;
                }
            }
        }

        for rel in col_export.relations {
            match self.relate(&rel.from, &rel.to, &rel.label).await {
                Ok(_) => summary.imported_relations += 1,
                Err(e) => {
                    tracing::warn!(from = %rel.from, to = %rel.to, label = %rel.label, error = %e, "Skipped relation during import");
                    summary.skipped_relations += 1;
                }
            }
        }

        Ok(summary)
    }
}

impl MemFuse {
    /// Importiert ein ExportDocumentV1 in die MemFuse-Datenbank mit Schema-Versionsprüfung und Idempotenz.
    pub async fn import_memories(&self, doc: ExportDocumentV1) -> Result<ImportSummary> {
        if doc.schema_version != SCHEMA_VERSION_V1 {
            return Err(memfuse_core::MemFuseError::invalid_input(format!(
                "Incompatible export schema version '{}', expected '{}'",
                doc.schema_version, SCHEMA_VERSION_V1
            )));
        }

        let mut summary = ImportSummary::default();

        for col_export in doc.collections {
            let col = self.collection(&col_export.name).await?;
            let col_summary = col.import_memories(col_export).await?;
            summary.imported_memories += col_summary.imported_memories;
            summary.skipped_memories += col_summary.skipped_memories;
            summary.imported_relations += col_summary.imported_relations;
            summary.skipped_relations += col_summary.skipped_relations;
        }

        Ok(summary)
    }
}
