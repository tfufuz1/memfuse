use memfuse_core::Result;
use std::path::Path;

/// Extrahiert reinen Text aus einer PDF-Datei.
pub fn extract_pdf_text(path: &Path) -> Result<String> {
    pdf_extract::extract_text(path).map_err(|e| {
        memfuse_core::MemFuseError::Internal(format!(
            "PDF-Extraktion fehlgeschlagen für {:?}: {e}",
            path
        ))
    })
}
