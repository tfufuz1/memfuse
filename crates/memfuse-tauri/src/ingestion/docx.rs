use memfuse_core::Result;
use std::path::Path;

/// Extrahiert reinen Text aus einer DOCX-Datei.
pub fn extract_docx_text(path: &Path) -> Result<String> {
    use docx_rs::*;

    let bytes = std::fs::read(path).map_err(|e| {
        memfuse_core::MemFuseError::Internal(format!(
            "DOCX lesen fehlgeschlagen für {:?}: {e}",
            path
        ))
    })?;

    let docx = read_docx(&bytes).map_err(|e| {
        memfuse_core::MemFuseError::Internal(format!(
            "DOCX parsen fehlgeschlagen für {:?}: {e:?}",
            path
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
}
