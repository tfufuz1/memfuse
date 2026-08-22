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
    let db = {
        let db_guard = state.db.read();
        db_guard.as_ref().cloned().ok_or("Keine Datenbank geöffnet")?
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
    state: State<'_, AppState>,
    folder_path: String,
    collection_name: String,
) -> Result<Vec<IngestReport>, String> {
    let db = {
        let db_guard = state.db.read();
        db_guard.as_ref().cloned().ok_or("Keine Datenbank geöffnet")?
    };
    let collection = db
        .collection(&collection_name)
        .await
        .map_err(|e| e.to_string())?;

    let embedder = Arc::new(OllamaBridge::localhost());
    let pipeline = IngestionPipeline::new(embedder);

    pipeline
        .ingest_folder(std::path::Path::new(&folder_path), &collection)
        .await
        .map_err(|e| e.to_string())
}
