use memfuse_core::{MemFuseError, Result};
use std::path::Path;

/// Extrahiert reinen Text aus einer PDF-Datei (pantiggeschützt via spawn_blocking + catch_unwind).
pub async fn extract_pdf_text(path: &Path) -> Result<String> {
    let path_buf = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        std::panic::catch_unwind(|| pdf_extract::extract_text(&path_buf))
    })
    .await
    .map_err(|e| MemFuseError::Internal(format!("PDF extraction task panicked: {e:?}")))?
    .map_err(|_| MemFuseError::Internal("PDF extraction panicked on malformed file".into()))?
    .map_err(|e| {
        MemFuseError::Internal(format!("PDF-Extraktion fehlgeschlagen für {:?}: {e}", path))
    })
}
