use crate::commands::collections::validate_collection_name;
use crate::ingestion::pipeline::{IngestReport, IngestionPipeline};
use crate::ollama::OllamaBridge;
use crate::state::AppState;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub async fn ingest_file(
    state: State<'_, AppState>,
    file_path: String,
    collection_name: String,
) -> Result<IngestReport, String> {
    validate_collection_name(&collection_name)?;
    let db = {
        let db_guard = state.db.read();
        db_guard
            .as_ref()
            .cloned()
            .ok_or_else(|| "No database is open. Please open or create a database first.".to_string())?
    };
    let collection = db
        .collection(&collection_name)
        .await
        .map_err(|e| e.to_string())?;

    let embedder = Arc::new(OllamaBridge::localhost());
    let pipeline = IngestionPipeline::new(embedder);

    pipeline
        .ingest_file(std::path::Path::new(&file_path), &collection)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ingest_folder(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    folder_path: String,
    collection_name: String,
) -> Result<Vec<IngestReport>, String> {
    validate_collection_name(&collection_name)?;
    let db = {
        let db_guard = state.db.read();
        db_guard
            .as_ref()
            .cloned()
            .ok_or_else(|| "No database is open. Please open or create a database first.".to_string())?
    };
    let collection = db
        .collection(&collection_name)
        .await
        .map_err(|e| e.to_string())?;

    let embedder = Arc::new(OllamaBridge::localhost());
    let pipeline = IngestionPipeline::new(embedder);

    let folder = std::path::Path::new(&folder_path);
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
            let report = pipeline
                .ingest_file(entry.path(), &collection)
                .await
                .map_err(|e| e.to_string())?;
            use tauri::Emitter;
            let _ = app.emit("ingest-progress", &report);
            reports.push(report);
        }
    }

    Ok(reports)
}
