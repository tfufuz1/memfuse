mod chat;
mod collections;
mod ingest;
mod search;
mod transform;

pub use chat::*;
pub use collections::*;
pub use ingest::*;
pub use search::*;
pub use transform::*;

use memfuse_core::{MemFuseError, Result};
use std::path::{Path, PathBuf};

/// Validiert, dass ein gegebenes Zielpfad-Argument existiert und nach Kanonisierung
/// innerhalb des zugelassenen Basisverzeichnisses liegt.
/// Verhindert Path-Traversal-Angriffe durch relative Pfade wie `../../etc/passwd`.
pub fn validate_path_within_base(path: &Path, base: &Path) -> Result<PathBuf> {
    let canonical_base = std::fs::canonicalize(base).map_err(|e| {
        MemFuseError::InvalidInput(format!(
            "Base path canonicalization failed ({:?}): {e}",
            base
        ))
    })?;

    let canonical_path = std::fs::canonicalize(path).map_err(|e| {
        MemFuseError::InvalidInput(format!(
            "Target path canonicalization failed ({:?}): {e}",
            path
        ))
    })?;

    if !canonical_path.starts_with(&canonical_base) {
        tracing::warn!(
            target_path = %canonical_path.display(),
            base_path = %canonical_base.display(),
            "Path traversal violation detected"
        );
        return Err(MemFuseError::PolicyViolation(
            "Path traversal detected: target path is outside allowed base directory".into(),
        ));
    }

    Ok(canonical_path)
}

#[cfg(test)]
mod path_validation_tests {
    use super::*;

    #[test]
    fn test_validate_path_within_base_valid() {
        let temp_dir = tempfile::tempdir().unwrap();
        let base = temp_dir.path();
        let file_path = base.join("subfolder").join("file.txt");
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        std::fs::write(&file_path, "data").unwrap();

        let validated = validate_path_within_base(&file_path, base);
        assert!(validated.is_ok());
    }

    #[test]
    fn test_validate_path_within_base_traversal_rejected() {
        let temp_dir = tempfile::tempdir().unwrap();
        let base = temp_dir.path().join("allowed_base");
        std::fs::create_dir_all(&base).unwrap();

        let outside_file = temp_dir.path().join("outside.txt");
        std::fs::write(&outside_file, "secret").unwrap();

        // Path traversal using relative path components
        let traversal_path = base.join("../outside.txt");

        let res = validate_path_within_base(&traversal_path, &base);
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(matches!(err, MemFuseError::PolicyViolation(_)));
        assert!(err.to_string().contains("Path traversal detected"));
    }
}
