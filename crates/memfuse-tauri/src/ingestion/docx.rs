use memfuse_core::{MemFuseError, Result};
use std::path::Path;

/// Extrahiert reinen Text aus einer DOCX-Datei (panikgeschützt via spawn_blocking + catch_unwind).
pub async fn extract_docx_text(path: &Path) -> Result<String> {
    let path_buf = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        std::panic::catch_unwind(|| {
            use docx_rs::*;

            let bytes = std::fs::read(&path_buf).map_err(|e| {
                MemFuseError::Internal(format!(
                    "DOCX lesen fehlgeschlagen für {:?}: {e}",
                    path_buf
                ))
            })?;

            let docx = read_docx(&bytes).map_err(|e| {
                MemFuseError::Internal(format!(
                    "DOCX parsen fehlgeschlagen für {:?}: {e:?}",
                    path_buf
                ))
            })?;

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
    })
    .await
    .map_err(|e| MemFuseError::Internal(format!("DOCX extraction task panicked: {e:?}")))?
    .map_err(|_| MemFuseError::Internal("DOCX extraction panicked on malformed file".into()))?
}
