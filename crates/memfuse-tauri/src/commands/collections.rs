use crate::state::AppState;
use memfuse_db::MemFuse;
use serde::Serialize;
use std::path::PathBuf;
use tauri::State;

#[derive(Serialize)]
pub struct CollectionInfo {
    pub name: String,
    pub document_count: usize,
}

/// Öffnet oder erstellt eine lokale MemFuse-Datenbank am gegebenen Pfad.
#[tauri::command]
pub async fn open_database(
    state: State<'_, AppState>,
    path: String,
) -> Result<(), String> {
    let path_buf = PathBuf::from(&path);
    let db = MemFuse::open(&path_buf)
        .await
        .map_err(|e| format!("Datenbank konnte nicht geöffnet werden: {e}"))?;

    *state.db.write() = Some(std::sync::Arc::new(db));
    *state.db_path.write() = Some(path_buf);
    Ok(())
}

#[tauri::command]
pub async fn list_collections(state: State<'_, AppState>) -> Result<Vec<CollectionInfo>, String> {
    let db = {
        let db_guard = state.db.read();
        db_guard.as_ref().cloned().ok_or("Keine Datenbank geöffnet")?
    };

    let names = db.list_collections().await.map_err(|e| e.to_string())?;
    let mut infos = Vec::new();
    for name in names {
        let col = db.collection(&name).await.map_err(|e| e.to_string())?;
        let count = col.len().await;
        infos.push(CollectionInfo {
            name,
            document_count: count,
        });
    }
    Ok(infos)
}

#[tauri::command]
pub async fn create_collection(
    state: State<'_, AppState>,
    name: String,
) -> Result<(), String> {
    let db = {
        let db_guard = state.db.read();
        db_guard.as_ref().cloned().ok_or("Keine Datenbank geöffnet")?
    };
    db.collection(&name).await.map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn drop_collection(
    state: State<'_, AppState>,
    name: String,
) -> Result<(), String> {
    let db = {
        let db_guard = state.db.read();
        db_guard.as_ref().cloned().ok_or("Keine Datenbank geöffnet")?
    };
    db.drop_collection(&name).await.map_err(|e| e.to_string())?;
    Ok(())
}
