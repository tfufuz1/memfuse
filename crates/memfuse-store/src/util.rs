// FILE-CONTEXT: Utility functions for memfuse-store (fsync helpers, etc.) (TS: 2026-08-29T17:17:31Z) (SESSION: 8f882f1f)
//! Utility functions for storage engine operations.

use memfuse_core::{MemFuseError, Result};
use std::path::Path;

/// Performs fsync on the parent directory of `path`.
///
/// Directory fsync is required on POSIX filesystems to guarantee that newly created files or
/// directory entries are durably persisted to disk.
///
/// # Errors
/// Returns `MemFuseError::Storage` if opening or syncing the parent directory fails.
pub(crate) async fn fsync_parent_dir(path: &Path) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    let dir_path = if parent.as_os_str().is_empty() {
        Path::new(".")
    } else {
        parent
    };

    let dir = tokio::fs::File::open(dir_path).await.map_err(|e| {
        MemFuseError::Storage(format!(
            "Directory open failed for fsync on {}: {e}",
            dir_path.display()
        ))
    })?;

    dir.sync_all().await.map_err(|e| {
        MemFuseError::Storage(format!(
            "Directory fsync failed for {}: {e}",
            dir_path.display()
        ))
    })?;

    Ok(())
}
