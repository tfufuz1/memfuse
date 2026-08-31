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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_pdf_bytes_empty() {
        let res = extract_pdf_bytes(&[]);
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), "");
    }

    #[test]
    fn test_extract_pdf_bytes_invalid_format() {
        let invalid_bytes = b"This is not a PDF file.";
        let res = extract_pdf_bytes(invalid_bytes);
        assert!(res.is_err());
    }

    #[test]
    fn test_extract_pdf_bytes_corrupted_header() {
        let corrupted_bytes = b"%PDF-1.4\n1 0 obj\n<< /Type /Catalog >>\nendobj\ntrailer\n<< >>";
        let res = extract_pdf_bytes(corrupted_bytes);
        assert!(res.is_err());
    }
}
