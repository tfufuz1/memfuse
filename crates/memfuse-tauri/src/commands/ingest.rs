use crate::commands::collections::validate_collection_name;
use crate::commands::validate_path_within_base;
use crate::ingestion::pipeline::{IngestReport, IngestionPipeline};
use crate::ollama::OllamaBridge;
use crate::state::AppState;
use memfuse_core::MemFuseErrorDto;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub async fn ingest_file(
    state: State<'_, AppState>,
    file_path: String,
    collection_name: String,
) -> Result<IngestReport, MemFuseErrorDto> {
    validate_collection_name(&collection_name)?;
    let db = {
        let db_guard = state.db.read();
        db_guard.as_ref().cloned().ok_or_else(|| {
            MemFuseErrorDto::new(
                "NotFound",
                "No database is open. Please open or create a database first.",
            )
        })?
    };

    let base_path = {
        let db_path_guard = state.db_path.read();
        db_path_guard.as_ref().cloned().ok_or_else(|| {
            MemFuseErrorDto::new(
                "NotFound",
                "No database path set. Please open a database first.",
            )
        })?
    };

    let collection = db
        .collection(&collection_name)
        .await
        .map_err(|e| MemFuseErrorDto::from(&e))?;

    let embedder = Arc::new(OllamaBridge::localhost());
    let pipeline = IngestionPipeline::new(embedder);

    let path = std::path::Path::new(&file_path);
    let canonical_path =
        validate_path_within_base(path, &base_path).map_err(|e| MemFuseErrorDto::from(&e))?;

    if !canonical_path.is_file() {
        return Err(MemFuseErrorDto::new(
            "InvalidInput",
            format!("Path is not a regular file: {file_path}"),
        ));
    }

    pipeline
        .ingest_file(&canonical_path, &collection)
        .await
        .map_err(|e| MemFuseErrorDto::from(&e))
}

#[tauri::command]
pub async fn ingest_folder(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    folder_path: String,
    collection_name: String,
) -> Result<Vec<IngestReport>, MemFuseErrorDto> {
    validate_collection_name(&collection_name)?;
    let db = {
        let db_guard = state.db.read();
        db_guard.as_ref().cloned().ok_or_else(|| {
            MemFuseErrorDto::new(
                "NotFound",
                "No database is open. Please open or create a database first.",
            )
        })?
    };

    let base_path = {
        let db_path_guard = state.db_path.read();
        db_path_guard.as_ref().cloned().ok_or_else(|| {
            MemFuseErrorDto::new(
                "NotFound",
                "No database path set. Please open a database first.",
            )
        })?
    };

    let collection = db
        .collection(&collection_name)
        .await
        .map_err(|e| MemFuseErrorDto::from(&e))?;

    let embedder = Arc::new(OllamaBridge::localhost());
    let pipeline = IngestionPipeline::new(embedder);

    let folder = std::path::Path::new(&folder_path);
    let canonical_folder =
        validate_path_within_base(folder, &base_path).map_err(|e| MemFuseErrorDto::from(&e))?;

    if !canonical_folder.is_dir() {
        return Err(MemFuseErrorDto::new(
            "InvalidInput",
            format!("Path is not a directory: {folder_path}"),
        ));
    }

    let mut reports = Vec::new();
    let supported = ["pdf", "docx", "md", "markdown", "txt", "eml"];

    for entry in walkdir::WalkDir::new(&canonical_folder)
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
            let report = match pipeline.ingest_file(entry.path(), &collection).await {
                Ok(rep) => rep,
                Err(e) => IngestReport {
                    file_path: entry.path().display().to_string(),
                    chunks_created: 0,
                    errors: vec![e.to_string()],
                },
            };
            use tauri::Emitter;
            let _ = app.emit("ingest-progress", &report);
            reports.push(report);
        }
    }

    Ok(reports)
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_path_traversal_non_existent_path_fails() {
        let path = "../../etc/passwd_non_existent";
        let res = std::fs::canonicalize(path);
        assert!(res.is_err());
    }
}
