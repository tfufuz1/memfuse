// FILE-CONTEXT
// STAND: 2026-08-30T14:38:30Z (SESSION: 45595f71)
// ZWECK: PDF document text extraction.
// INVARIANTEN: Zero panic policy on corrupted or malformed PDF byte inputs (uses catch_unwind).
// NICHT-OFFENSICHTLICH: Empty input returns Ok(empty string) without calling pdf_extract.
// SIEHE AUCH: crates/memfuse-tauri/src/ingestion/pipeline.rs

use memfuse_core::{MemFuseError, Result};
use std::path::Path;

/// Extrahiert reinen Text aus PDF-Bytes (panikgeschützt).
pub fn extract_pdf_bytes(bytes: &[u8]) -> Result<String> {
    if bytes.is_empty() {
        return Ok(String::new());
    }
    std::panic::catch_unwind(|| pdf_extract::extract_text_from_mem(bytes))
        .map_err(|_| MemFuseError::Internal("PDF extraction panicked on malformed file".into()))?
        .map_err(|e| MemFuseError::Internal(format!("PDF extraction failed: {e}")))
}

/// Extrahiert reinen Text aus einer PDF-Datei (panikgeschützt via spawn_blocking + catch_unwind).
pub async fn extract_pdf_text(path: &Path) -> Result<String> {
    let path_buf = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        std::panic::catch_unwind(|| {
            let bytes = std::fs::read(&path_buf).map_err(|e| {
                MemFuseError::Internal(format!("Failed to read PDF file {:?}: {e}", path_buf))
            })?;
            extract_pdf_bytes(&bytes)
        })
    })
    .await
    .map_err(|e| MemFuseError::Internal(format!("PDF extraction task panicked: {e:?}")))?
    .map_err(|_| MemFuseError::Internal("PDF extraction panicked on malformed file".into()))?
}
