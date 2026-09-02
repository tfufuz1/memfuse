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
    fn test_validate_path_within_base_valid() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        let temp_dir = tempfile::tempdir()?;
        let base = temp_dir.path();
        let file_path = base.join("subfolder").join("file.txt");
        let parent = file_path.parent().ok_or("No parent directory")?;
        std::fs::create_dir_all(parent)?;
        std::fs::write(&file_path, "data")?;

        let validated = validate_path_within_base(&file_path, base);
        assert!(validated.is_ok());
        Ok(())
    }

    #[test]
    fn test_validate_path_within_base_traversal_rejected(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempfile::tempdir()?;
        let base = temp_dir.path().join("allowed_base");
        std::fs::create_dir_all(&base)?;

        let outside_file = temp_dir.path().join("outside.txt");
        std::fs::write(&outside_file, "secret")?;

        // Path traversal using relative path components
        let traversal_path = base.join("../outside.txt");

        let res = validate_path_within_base(&traversal_path, &base);
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(matches!(err, MemFuseError::PolicyViolation(_)));
        assert!(err.to_string().contains("Path traversal detected"));
        Ok(())
    }

    #[test]
    fn test_path_traversal_multi_nested_rejected(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempfile::tempdir()?;
        let base = temp_dir.path().join("a").join("b").join("c");
        std::fs::create_dir_all(&base)?;

        let outside_file = temp_dir.path().join("outside.txt");
        std::fs::write(&outside_file, "secret")?;

        // Multi-nested traversal ../../../
        let traversal_path = base.join("../../../outside.txt");

        let res = validate_path_within_base(&traversal_path, &base);
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(matches!(err, MemFuseError::PolicyViolation(_)));
        Ok(())
    }

    #[test]
    fn test_path_traversal_absolute_path_outside_base_rejected(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempfile::tempdir()?;
        let base = temp_dir.path().join("allowed_base");
        std::fs::create_dir_all(&base)?;

        let outside_dir = temp_dir.path().join("outside_dir");
        std::fs::create_dir_all(&outside_dir)?;
        let outside_file = outside_dir.join("secret.txt");
        std::fs::write(&outside_file, "secret")?;

        // Direct absolute path outside base
        let res = validate_path_within_base(&outside_file, &base);
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(matches!(err, MemFuseError::PolicyViolation(_)));
        Ok(())
    }

    #[test]
    fn test_path_traversal_symlink_pointing_outside_rejected(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempfile::tempdir()?;
        let base = temp_dir.path().join("allowed_base");
        std::fs::create_dir_all(&base)?;

        let outside_file = temp_dir.path().join("secret_outside.txt");
        std::fs::write(&outside_file, "secret")?;

        let symlink_path = base.join("link_to_outside.txt");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside_file, &symlink_path)?;
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&outside_file, &symlink_path)?;

        let res = validate_path_within_base(&symlink_path, &base);
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(matches!(err, MemFuseError::PolicyViolation(_)));
        assert!(err.to_string().contains("Path traversal detected"));
        Ok(())
    }
}
