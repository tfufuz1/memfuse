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

use memfuse_core::MemFuseErrorDto;

pub fn validate_collection_name(name: &str) -> Result<(), MemFuseErrorDto> {
    if name.is_empty() {
        return Err(MemFuseErrorDto::new(
            "InvalidInput",
            "Collection name cannot be empty",
        ));
    }
    if name.len() > 256 {
        return Err(MemFuseErrorDto::new(
            "InvalidInput",
            "Collection name exceeds maximum length of 256 characters",
        ));
    }
    if name.starts_with("__") {
        return Err(MemFuseErrorDto::new(
            "PolicyViolation",
            "Collection name cannot start with '__' (reserved for internal use)",
        ));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(MemFuseErrorDto::new(
            "InvalidInput",
            "Collection name must only contain alphanumeric characters, hyphens, or underscores",
        ));
    }
    Ok(())
}

/// Öffnet oder erstellt eine lokale MemFuse-Datenbank am gegebenen Pfad.
#[tauri::command]
pub async fn open_database(
    state: State<'_, AppState>,
    path: String,
) -> Result<(), MemFuseErrorDto> {
    let path_buf = PathBuf::from(&path);
    // Erstelle Verzeichnis falls es noch nicht existiert, um canonicalize zu ermöglichen
    if !path_buf.exists() {
        if let Err(e) = std::fs::create_dir_all(&path_buf) {
            return Err(MemFuseErrorDto::new(
                "InvalidInput",
                format!("Failed to create database directory {:?}: {e}", path_buf),
            ));
        }
    }

    let canonical_path = std::fs::canonicalize(&path_buf).map_err(|e| {
        MemFuseErrorDto::new(
            "InvalidInput",
            format!("Invalid database path ({path}): {e}"),
        )
    })?;

    let db = MemFuse::open(&canonical_path)
        .await
        .map_err(|e| MemFuseErrorDto::from(&e))?;

    *state.db.write() = Some(std::sync::Arc::new(db));
    *state.db_path.write() = Some(canonical_path);
    Ok(())
}

#[tauri::command]
pub async fn list_collections(
    state: State<'_, AppState>,
) -> Result<Vec<CollectionInfo>, MemFuseErrorDto> {
    let db = {
        let db_guard = state.db.read();
        db_guard.as_ref().cloned().ok_or_else(|| {
            MemFuseErrorDto::new(
                "NotFound",
                "No database is open. Please open or create a database first.",
            )
        })?
    };

    let names = db
        .list_collections()
        .await
        .map_err(|e| MemFuseErrorDto::from(&e))?;
    let mut infos = Vec::new();
    for name in names {
        let col = db
            .collection(&name)
            .await
            .map_err(|e| MemFuseErrorDto::from(&e))?;
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
) -> Result<(), MemFuseErrorDto> {
    validate_collection_name(&name)?;
    let db = {
        let db_guard = state.db.read();
        db_guard.as_ref().cloned().ok_or_else(|| {
            MemFuseErrorDto::new(
                "NotFound",
                "No database is open. Please open or create a database first.",
            )
        })?
    };
    db.collection(&name)
        .await
        .map_err(|e| MemFuseErrorDto::from(&e))?;
    Ok(())
}

#[tauri::command]
pub async fn drop_collection(
    state: State<'_, AppState>,
    name: String,
) -> Result<(), MemFuseErrorDto> {
    validate_collection_name(&name)?;
    let db = {
        let db_guard = state.db.read();
        db_guard.as_ref().cloned().ok_or_else(|| {
            MemFuseErrorDto::new(
                "NotFound",
                "No database is open. Please open or create a database first.",
            )
        })?
    };
    db.drop_collection(&name)
        .await
        .map_err(|e| MemFuseErrorDto::from(&e))?;
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
        let exact_limit = "a".repeat(256);
        assert!(validate_collection_name(&exact_limit).is_ok());

        let long_name = "a".repeat(257);
        assert!(validate_collection_name(&long_name).is_err());
    }

    #[test]
    fn test_validate_collection_name_reserved_prefix() {
        assert!(validate_collection_name("_single_underscore_allowed").is_ok());
        assert!(validate_collection_name("__internal").is_err());
    }

    #[test]
    fn test_validate_collection_name_unicode_rejected() {
        assert!(validate_collection_name("sammlung_öäü").is_err());
    }

    #[test]
    fn test_validate_collection_name_invalid_chars() {
        assert!(validate_collection_name("invalid/name").is_err());
        assert!(validate_collection_name("invalid\\name").is_err());
        assert!(validate_collection_name("invalid name").is_err());
        assert!(validate_collection_name("invalid.name").is_err());
    }
}
