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

pub fn validate_collection_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Collection name cannot be empty".to_string());
    }
    if name.len() > 256 {
        return Err("Collection name exceeds maximum length of 256 characters".to_string());
    }
    if name.starts_with("__") {
        return Err("Collection name cannot start with '__' (reserved for internal use)".to_string());
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(
            "Collection name must only contain alphanumeric characters, hyphens, or underscores"
                .to_string(),
        );
    }
    Ok(())
}

/// Öffnet oder erstellt eine lokale MemFuse-Datenbank am gegebenen Pfad.
#[tauri::command]
pub async fn open_database(state: State<'_, AppState>, path: String) -> Result<(), String> {
    let path_buf = PathBuf::from(&path);
    let db = MemFuse::open(&path_buf)
        .await
        .map_err(|e| format!("Failed to open database: {e}"))?;

    *state.db.write() = Some(std::sync::Arc::new(db));
    *state.db_path.write() = Some(path_buf);
    Ok(())
}

#[tauri::command]
pub async fn list_collections(state: State<'_, AppState>) -> Result<Vec<CollectionInfo>, String> {
    let db = {
        let db_guard = state.db.read();
        db_guard
            .as_ref()
            .cloned()
            .ok_or_else(|| "No database is open. Please open or create a database first.".to_string())?
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
pub async fn create_collection(state: State<'_, AppState>, name: String) -> Result<(), String> {
    validate_collection_name(&name)?;
    let db = {
        let db_guard = state.db.read();
        db_guard
            .as_ref()
            .cloned()
            .ok_or_else(|| "No database is open. Please open or create a database first.".to_string())?
    };
    db.collection(&name).await.map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn drop_collection(state: State<'_, AppState>, name: String) -> Result<(), String> {
    validate_collection_name(&name)?;
    let db = {
        let db_guard = state.db.read();
        db_guard
            .as_ref()
            .cloned()
            .ok_or_else(|| "No database is open. Please open or create a database first.".to_string())?
    };
    db.drop_collection(&name).await.map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_collection_name_valid() {
        assert!(validate_collection_name("valid-name_123").is_ok());
        assert!(validate_collection_name("a").is_ok());
    }

    #[test]
    fn test_validate_collection_name_empty() {
        assert!(validate_collection_name("").is_err());
    }

    #[test]
    fn test_validate_collection_name_too_long() {
        let long_name = "a".repeat(257);
        assert!(validate_collection_name(&long_name).is_err());
    }

    #[test]
    fn test_validate_collection_name_reserved_prefix() {
        assert!(validate_collection_name("__internal").is_err());
    }

    #[test]
    fn test_validate_collection_name_invalid_chars() {
        assert!(validate_collection_name("invalid/name").is_err());
        assert!(validate_collection_name("invalid\\name").is_err());
        assert!(validate_collection_name("invalid name").is_err());
        assert!(validate_collection_name("invalid.name").is_err());
    }
}
