// FILE-CONTEXT
// ZWECK: Export-Funktionalität für Memory-Export-Format v1 (Schema Version "1.0").
// INVARIANTEN: Schema-Integrität; Vollständigkeit aller Memories & Relationen; Nur lesender Zugriff.
// STAND: TS:2026-09-12

use crate::collection::{extract_effective_importance, extract_text, Collection, StoredDocument};
use crate::filter::extract_memory_type;
use crate::MemFuse;
use memfuse_core::{MemoryLink, MemoryType, Result, StorageEngine, VectorIndex};
use serde::{Deserialize, Serialize};

/// Aktuelle Schema-Version für das JSON Export-Format.
pub const SCHEMA_VERSION_V1: &str = "1.0";

/// Wurzel-Dokument für den Export aller DB-Collections.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExportDocumentV1 {
    /// Pflichtfeld für Schema-Migrationen (MUSS "1.0" sein)
    pub schema_version: String,
    /// UTC-Zeitstempel der Export-Erstellung (ISO-8601 String)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exported_at: Option<String>,
    /// Liste aller exportierten Collections
    pub collections: Vec<ExportCollectionV1>,
}

/// Export-Repräsentation einer einzelnen Collection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExportCollectionV1 {
    /// Name der Collection
    pub name: String,
    /// Liste aller Memory-Einträge
    pub memories: Vec<ExportMemoryV1>,
    /// Liste aller direkten Graph-Relationen
    pub relations: Vec<ExportRelationV1>,
}

/// Export-Repräsentation eines einzelnen Memory-Eintrags.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExportMemoryV1 {
    /// Eindeutiger Memory-Schlüssel / ID
    pub id: String,
    /// Kognitiver Gedächtnistyp (episodic, semantic, procedural, working)
    #[serde(rename = "type")]
    pub memory_type: MemoryType,
    /// Textinhalt des Memory-Eintrags
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Vektor-Embedding (falls vorhanden)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
    /// Name des verwendeten Embedding-Modells
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding_model: Option<String>,
    /// Erstellungs-Transaktions-ID oder Unix-Timestamp
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<u64>,
    /// Effektiver Importance Score in [0.0, 1.0]
    pub importance_score: f32,
    /// Zusätzliche Metadaten als JSON-Objekt
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    /// Zettelkasten Memory Links zu anderen Memories
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<MemoryLink>,
}

/// Export-Repräsentation einer Graph-Beziehung.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExportRelationV1 {
    /// Quell-Memory ID
    pub from: String,
    /// Ziel-Memory ID
    pub to: String,
    /// Beziehungs-Label (z.B. "references", "depends_on")
    pub label: String,
}

impl<S: StorageEngine, V: VectorIndex> Collection<S, V> {
    /// Exportiert alle Memories und Graph-Relationen der Collection in das ExportCollectionV1 Schema.
    pub async fn export_memories(&self) -> Result<ExportCollectionV1> {
        let mut memories = Vec::new();

        // 1. Scan user documents (key_type = 0)
        let user_prefix = if self.name == "default" {
            b"".to_vec()
        } else {
            let mut p = self.prefix.clone();
            p.push(0);
            p
        };

        let mut cursor: Option<Vec<u8>> = None;
        const BATCH_SIZE: usize = 1000;

        loop {
            let (batch, next_cursor) = self
                .storage
                .scan_prefix_bounded(&user_prefix, BATCH_SIZE, cursor.as_deref())
                .await?;

            if batch.is_empty() {
                break;
            }

            for (k, v) in batch {
                if self.name == "default" && k.starts_with(b"__") {
                    continue;
                }

                let stored: StoredDocument = match serde_json::from_slice(&v) {
                    Ok(d) => d,
                    Err(_) => continue,
                };

                let memory_type = extract_memory_type(&stored.metadata);
                let content = extract_text(&stored.metadata);
                let embedding = if stored.embedding.is_empty() {
                    None
                } else {
                    Some(stored.embedding)
                };

                let embedding_model = stored.metadata.as_ref().and_then(|m| {
                    m.get("embedding_model")
                        .and_then(|val| val.as_str())
                        .map(String::from)
                });

                let created_at = stored.metadata.as_ref().and_then(|m| {
                    m.get("created_at_tx")
                        .and_then(|val| val.as_u64())
                        .or_else(|| m.get("created_at").and_then(|val| val.as_u64()))
                        .or_else(|| {
                            m.get("importance")
                                .and_then(|imp| imp.get("created_at_tx"))
                                .and_then(|val| val.as_u64())
                        })
                });

                let importance_score = extract_effective_importance(&stored.metadata, memfuse_core::TxId::new(u64::MAX));

                let links: Vec<MemoryLink> = stored
                    .metadata
                    .as_ref()
                    .and_then(|m| m.get("links"))
                    .and_then(|val| serde_json::from_value(val.clone()).ok())
                    .unwrap_or_default();

                memories.push(ExportMemoryV1 {
                    id: stored.id,
                    memory_type,
                    content,
                    embedding,
                    embedding_model,
                    created_at,
                    importance_score,
                    metadata: stored.metadata,
                    links,
                });
            }

            if let Some(next) = next_cursor {
                cursor = Some(next);
            } else {
                break;
            }
        }

        // 2. Scan graph relations (key_type = 2)
        let rel_prefix = if self.name == "default" {
            b"__rel:".to_vec()
        } else {
            let mut p = self.prefix.clone();
            p.push(2);
            p
        };

        let mut relations = Vec::new();
        let mut rel_cursor: Option<Vec<u8>> = None;

        loop {
            let (batch, next_cursor) = self
                .storage
                .scan_prefix_bounded(&rel_prefix, BATCH_SIZE, rel_cursor.as_deref())
                .await?;

            if batch.is_empty() {
                break;
            }

            for (_k, v) in batch {
                if let Ok(rel) = serde_json::from_slice::<ExportRelationV1>(&v) {
                    relations.push(rel);
                }
            }

            if let Some(next) = next_cursor {
                rel_cursor = Some(next);
            } else {
                break;
            }
        }

        Ok(ExportCollectionV1 {
            name: self.name.clone(),
            memories,
            relations,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MemFuseConfig;
    use memfuse_core::{LinkRelation, MemoryType};
    use serde_json::json;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_export_import_roundtrip() {
        let tmp1 = TempDir::new().unwrap();
        let config = MemFuseConfig {
            dimension: 4,
            ..Default::default()
        };

        let db1 = MemFuse::open_with_config(tmp1.path(), config.clone())
            .await
            .unwrap();

        let col_default = db1.collection("default").await.unwrap();
        col_default
            .insert_typed(
                "mem-1",
                &[1.0, 0.0, 0.0, 0.0],
                MemoryType::Episodic,
                Some(json!({
                    "text": "Visited Berlin in summer 2025",
                    "category": "travel"
                })),
            )
            .await
            .unwrap();

        col_default
            .insert_typed(
                "mem-2",
                &[0.0, 1.0, 0.0, 0.0],
                MemoryType::Semantic,
                Some(json!({
                    "content": "Capital of Germany is Berlin"
                })),
            )
            .await
            .unwrap();

        col_default
            .link_memories(
                memfuse_core::DocId::from_key("mem-1").unwrap(),
                memfuse_core::DocId::from_key("mem-2").unwrap(),
                LinkRelation::References,
            )
            .await
            .unwrap();

        db1.relate("mem-1", "mem-2", "related_to").await.unwrap();

        let custom_col = db1.collection("work").await.unwrap();
        custom_col
            .insert_typed(
                "work-1",
                &[0.0, 0.0, 1.0, 0.0],
                MemoryType::Procedural,
                Some(json!({
                    "content": "Deploying release to k8s"
                })),
            )
            .await
            .unwrap();

        // 1. Export
        let export_doc = db1.export_memories().await.unwrap();
        assert_eq!(export_doc.schema_version, SCHEMA_VERSION_V1);
        assert_eq!(export_doc.collections.len(), 2);

        // 2. Import into a fresh DB
        let tmp2 = TempDir::new().unwrap();
        let db2 = MemFuse::open_with_config(tmp2.path(), config)
            .await
            .unwrap();

        let summary = db2.import_memories(export_doc.clone()).await.unwrap();
        assert_eq!(summary.imported_memories, 3);
        assert_eq!(summary.skipped_memories, 0);
        assert_eq!(summary.imported_relations, 2); // bidirectional graph relations

        // 3. Verify content in DB2
        let col2_default = db2.collection("default").await.unwrap();
        assert_eq!(col2_default.len().await, 2);

        let doc1 = col2_default.get("mem-1").await.unwrap().unwrap();
        let meta1 = doc1.metadata.unwrap();
        assert_eq!(meta1["memory_type"], "episodic");
        assert_eq!(meta1["text"], "Visited Berlin in summer 2025");

        let doc2 = col2_default.get("mem-2").await.unwrap().unwrap();
        let meta2 = doc2.metadata.unwrap();
        assert_eq!(meta2["memory_type"], "semantic");

        let links1 = col2_default
            .get_links(memfuse_core::DocId::from_key("mem-1").unwrap())
            .await
            .unwrap();
        assert_eq!(links1.len(), 1);
        assert_eq!(
            links1[0].target,
            memfuse_core::DocId::from_key("mem-2").unwrap()
        );
        assert_eq!(links1[0].relation, LinkRelation::References);

        let col2_work = db2.collection("work").await.unwrap();
        assert_eq!(col2_work.len().await, 1);
        let work_doc = col2_work.get("work-1").await.unwrap().unwrap();
        assert_eq!(work_doc.metadata.unwrap()["memory_type"], "procedural");

        // 4. Re-export from DB2 and compare
        let export_doc2 = db2.export_memories().await.unwrap();
        assert_eq!(export_doc2.collections.len(), export_doc.collections.len());

        // 5. Verify Idempotency of Import
        let summary_reimport = db2.import_memories(export_doc.clone()).await.unwrap();
        assert_eq!(summary_reimport.imported_memories, 3);
        assert_eq!(col2_default.len().await, 2);
    }

    #[tokio::test]
    async fn test_import_schema_version_mismatch_fails() {
        let tmp = TempDir::new().unwrap();
        let db = MemFuse::open_with_config(
            tmp.path(),
            MemFuseConfig {
                dimension: 4,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let invalid_doc = ExportDocumentV1 {
            schema_version: "2.0".to_string(),
            exported_at: None,
            collections: vec![],
        };

        let err = db.import_memories(invalid_doc).await;
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("Incompatible export schema version"));
    }
}

impl MemFuse {
    /// Exportiert alle Collections der MemFuse-Datenbank in das ExportDocumentV1 Schema.
    pub async fn export_memories(&self) -> Result<ExportDocumentV1> {
        let collection_names = self.list_collections().await?;
        let mut collections = Vec::new();

        for name in collection_names {
            let col = self.collection(&name).await?;
            let export_col = col.export_memories().await?;
            collections.push(export_col);
        }

        Ok(ExportDocumentV1 {
            schema_version: SCHEMA_VERSION_V1.to_string(),
            exported_at: None,
            collections,
        })
    }
}
