use memfuse_core::{MemFuseError, Result};
use std::path::Path;

/// Extrahiert reinen Text aus DOCX-Bytes (panikgeschützt).
pub fn extract_docx_bytes(bytes: &[u8]) -> Result<String> {
    if bytes.is_empty() {
        return Ok(String::new());
    }
    std::panic::catch_unwind(|| {
        use docx_rs::*;

        let docx = read_docx(bytes)
            .map_err(|e| MemFuseError::Internal(format!("DOCX parsing failed: {e:?}")))?;

        let mut text_buf = String::new();

        for child in docx.document.children {
            if let DocumentChild::Paragraph(p) = child {
                let mut paragraph_text = String::new();
                for p_child in p.children {
                    if let ParagraphChild::Run(r) = p_child {
                        for r_child in r.children {
                            if let RunChild::Text(t) = r_child {
                                paragraph_text.push_str(&t.text);
                            }
                        }
                    }
                }
                if !paragraph_text.is_empty() {
                    text_buf.push_str(&paragraph_text);
                    text_buf.push('\n');
                }
            }
        }

        Ok(text_buf)
    })
    .map_err(|_| MemFuseError::Internal("DOCX extraction panicked on malformed file".into()))?
}

/// Extrahiert reinen Text aus einer DOCX-Datei (panikgeschützt via spawn_blocking + catch_unwind).
pub async fn extract_docx_text(path: &Path) -> Result<String> {
    let path_buf = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let bytes = std::fs::read(&path_buf).map_err(|e| {
            MemFuseError::Internal(format!("Failed to read DOCX file {:?}: {e}", path_buf))
        })?;
        extract_docx_bytes(&bytes)
    })
    .await
    .map_err(|e| MemFuseError::Internal(format!("DOCX extraction task panicked: {e:?}")))?
}
