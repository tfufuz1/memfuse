use memfuse_core::{MemFuseError, Result};
use std::path::Path;

/// Extrahiert reinen Text aus PDF-Bytes (panikgeschützt).
///
/// **Sicherheitsgarantie**: `pdf-extract` führt ausschließlich die Extraktion
/// von Text-Streams im PDF-Format durch. Es verarbeitet/interpretiert keinerlei
/// PDF-Aktionen, `OpenAction`-JavaScript-Trigger, Formularskripte oder
/// sonstige ausführbare Chunks.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_pdf_malformed_returns_error_no_panic() {
        let malformed_pdf = b"%PDF-1.4 truncated malformed header content";
        let res = extract_pdf_bytes(malformed_pdf);
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(
            err.to_string().contains("PDF extraction failed")
                || err.to_string().contains("panicked"),
            "Unexpected error message: {err}"
        );
    }
}
