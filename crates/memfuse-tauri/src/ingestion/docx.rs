use memfuse_core::{MemFuseError, Result};
use std::io::Read;
use std::path::Path;

/// Konfiguration für DOCX-Ingestion-Sicherheitslimits (Zip-Bomb-Schutz).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DocxConfig {
    /// Maximale erlaubte Kompressionsrate pro ZIP-Eintrag (z.B. 100.0 = Faktor 100).
    pub max_compression_ratio: f64,
    /// Maximale unkomprimierte Gesamtgröße aller Einträge in Bytes (Default: 500 MB).
    pub max_uncompressed_size_bytes: u64,
    /// Maximale Anzahl von Einträgen im ZIP-Container (Default: 1.000).
    pub max_entries: usize,
}

impl Default for DocxConfig {
    fn default() -> Self {
        Self {
            max_compression_ratio: 100.0,
            max_uncompressed_size_bytes: 500 * 1024 * 1024, // 500 MB
            max_entries: 1000,
        }
    }
}

impl DocxConfig {
    /// Erstellt eine neue `DocxConfig` mit benutzerdefinierten Limits.
    pub fn new(
        max_compression_ratio: f64,
        max_uncompressed_size_bytes: u64,
        max_entries: usize,
    ) -> Self {
        Self {
            max_compression_ratio,
            max_uncompressed_size_bytes,
            max_entries,
        }
    }
}

/// Spezifischer Fehlertyp für DOCX-Ingestion und Dekomprimierungsbomben-Erkennung.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum IngestionError {
    #[error("Decompression bomb suspected in entry '{entry_name}': compression ratio {ratio:.1} exceeds limit")]
    DecompressionBombSuspected { ratio: f64, entry_name: String },
    #[error("Total uncompressed size ({total_bytes} bytes) exceeds maximum limit of {limit_bytes} bytes")]
    TotalSizeExceeded { total_bytes: u64, limit_bytes: u64 },
    #[error("ZIP entry count ({entries}) exceeds maximum limit of {max_entries}")]
    EntryCountExceeded { entries: usize, max_entries: usize },
    #[error("Invalid or corrupted DOCX archive: {0}")]
    InvalidArchive(String),
}

impl From<IngestionError> for MemFuseError {
    fn from(err: IngestionError) -> Self {
        MemFuseError::PolicyViolation(err.to_string())
    }
}

/// Prüft den ZIP-Container einer DOCX-Datei vor dem Parsen auf Zip-Bomben,
/// exzessive Eintragsanzahl und Gesamtgrößenüberschreitungen.
pub fn validate_docx_zip(
    bytes: &[u8],
    config: &DocxConfig,
) -> std::result::Result<(), IngestionError> {
    if bytes.is_empty() {
        return Ok(());
    }

    let cursor = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|e| IngestionError::InvalidArchive(format!("Failed to open ZIP archive: {e}")))?;

    // 1. Eintragsanzahl-Prüfung
    let entry_count = archive.len();
    if entry_count > config.max_entries {
        return Err(IngestionError::EntryCountExceeded {
            entries: entry_count,
            max_entries: config.max_entries,
        });
    }

    let mut accumulated_uncompressed_bytes: u64 = 0;

    // 2. Pre-Check der ZIP-Header & Streaming-Validierung aller Einträge
    for i in 0..entry_count {
        let mut entry = archive.by_index(i).map_err(|e| {
            IngestionError::InvalidArchive(format!("Failed to read ZIP entry at index {i}: {e}"))
        })?;

        let entry_name = entry.name().to_string();
        let compressed_size = entry.compressed_size();
        let header_uncompressed_size = entry.size();

        // Header-Ratio-Prüfung
        let header_ratio = if compressed_size == 0 {
            if header_uncompressed_size > 0 {
                f64::INFINITY
            } else {
                0.0
            }
        } else {
            (header_uncompressed_size as f64) / (compressed_size as f64)
        };

        if header_ratio > config.max_compression_ratio {
            return Err(IngestionError::DecompressionBombSuspected {
                ratio: header_ratio,
                entry_name,
            });
        }

        // Streaming-Dekomprimierungs-Prüfung mit hard limit (Defense-in-Depth)
        // Schützt vor präparierten ZIPs mit gefälschten Header-Größenangaben.
        let max_allowed_for_entry = if compressed_size == 0 {
            0
        } else {
            ((compressed_size as f64) * config.max_compression_ratio).ceil() as u64
        };

        let remaining_archive_budget = config
            .max_uncompressed_size_bytes
            .saturating_sub(accumulated_uncompressed_bytes);

        let read_cap = max_allowed_for_entry
            .min(remaining_archive_budget)
            .saturating_add(1);

        let mut limited_reader = entry.by_ref().take(read_cap);
        let mut sink = std::io::sink();
        let bytes_read = std::io::copy(&mut limited_reader, &mut sink).map_err(|e| {
            IngestionError::InvalidArchive(format!(
                "Failed to decompress ZIP entry '{entry_name}': {e}"
            ))
        })?;

        accumulated_uncompressed_bytes = accumulated_uncompressed_bytes.saturating_add(bytes_read);

        // Streaming-Ratio-Prüfung
        let streaming_ratio = if compressed_size == 0 {
            if bytes_read > 0 {
                f64::INFINITY
            } else {
                0.0
            }
        } else {
            (bytes_read as f64) / (compressed_size as f64)
        };

        if streaming_ratio > config.max_compression_ratio {
            return Err(IngestionError::DecompressionBombSuspected {
                ratio: streaming_ratio,
                entry_name,
            });
        }

        // Gesamtgrößen-Prüfung
        if accumulated_uncompressed_bytes > config.max_uncompressed_size_bytes {
            return Err(IngestionError::TotalSizeExceeded {
                total_bytes: accumulated_uncompressed_bytes,
                limit_bytes: config.max_uncompressed_size_bytes,
            });
        }
    }

    Ok(())
}

/// Extrahiert reinen Text aus DOCX-Bytes unter Berücksichtigung konfigurierter Sicherheitslimits.
pub fn extract_docx_bytes_with_config(bytes: &[u8], config: &DocxConfig) -> Result<String> {
    if bytes.is_empty() {
        return Ok(String::new());
    }

    // 1. Vorprüfung des ZIP-Containers vor Aufruf von read_docx
    validate_docx_zip(bytes, config)?;

    // 2. Eigentliche Extraktion mit docx_rs
    let res = std::panic::catch_unwind(|| {
        use docx_rs::*;

        let docx = read_docx(bytes)
            .map_err(|e| MemFuseError::Internal(format!("DOCX parsing failed: {e:?}")))?;

        let mut text_buf = String::new();
        let max_text_bytes = config.max_uncompressed_size_bytes as usize;

        for child in docx.document.children {
            if let DocumentChild::Paragraph(p) = child {
                let mut paragraph_text = String::new();
                for p_child in p.children {
                    if let ParagraphChild::Run(r) = p_child {
                        for r_child in r.children {
                            if let RunChild::Text(t) = r_child {
                                paragraph_text.push_str(&t.text);
                                if text_buf.len() + paragraph_text.len() > max_text_bytes {
                                    return Err(MemFuseError::PolicyViolation(format!(
                                        "Extracted DOCX text size exceeded limit of {max_text_bytes} bytes"
                                    )));
                                }
                            }
                        }
                    }
                }
                if !paragraph_text.is_empty() {
                    text_buf.push_str(&paragraph_text);
                    text_buf.push('\n');
                    if text_buf.len() > max_text_bytes {
                        return Err(MemFuseError::PolicyViolation(format!(
                            "Extracted DOCX text size exceeded limit of {max_text_bytes} bytes"
                        )));
                    }
                }
            }
        }

        Ok(text_buf)
    });

    match res {
        Ok(inner_res) => inner_res,
        Err(_) => Err(MemFuseError::Internal(
            "DOCX extraction panicked on malformed file".into(),
        )),
    }
}

/// Extrahiert reinen Text aus DOCX-Bytes (panikgeschützt, Default-Limits).
pub fn extract_docx_bytes(bytes: &[u8]) -> Result<String> {
    extract_docx_bytes_with_config(bytes, &DocxConfig::default())
}

/// Extrahiert reinen Text aus einer DOCX-Datei mit Konfiguration (panikgeschützt via spawn_blocking + catch_unwind).
pub async fn extract_docx_text_with_config(path: &Path, config: &DocxConfig) -> Result<String> {
    let path_buf = path.to_path_buf();
    let config = config.clone();
    tokio::task::spawn_blocking(move || {
        let res = std::panic::catch_unwind(|| {
            let bytes = std::fs::read(&path_buf).map_err(|e| {
                MemFuseError::Internal(format!("Failed to read DOCX file {:?}: {e}", path_buf))
            })?;
            extract_docx_bytes_with_config(&bytes, &config)
        });
        match res {
            Ok(inner_res) => inner_res,
            Err(_) => Err(MemFuseError::Internal(
                "DOCX extraction panicked on malformed file".into(),
            )),
        }
    })
    .await
    .map_err(|e| MemFuseError::Internal(format!("DOCX extraction task panicked: {e:?}")))?
}

/// Extrahiert reinen Text aus einer DOCX-Datei (panikgeschützt, Default-Limits).
pub async fn extract_docx_text(path: &Path) -> Result<String> {
    extract_docx_text_with_config(path, &DocxConfig::default()).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    #[test]
    fn test_valid_docx_processing_succeeds() {
        use docx_rs::*;
        let mut buf = Vec::new();
        let docx = Docx::new().add_paragraph(
            Paragraph::new().add_run(Run::new().add_text("Valid DOCX Paragraph Content")),
        );
        docx.pack(std::io::Cursor::new(&mut buf)).unwrap();

        let text = extract_docx_bytes(&buf).expect("Valid DOCX extraction should succeed");
        assert!(
            text.contains("Valid DOCX Paragraph Content"),
            "Extracted text was: '{text}'"
        );
    }

    #[test]
    fn test_zip_bomb_detection_ratio_exceeded() {
        let mut buffer = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buffer));
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            zip.start_file("word/document.xml", options).unwrap();
            let highly_compressible = vec![b'0'; 100_000];
            zip.write_all(&highly_compressible).unwrap();
            zip.finish().unwrap();
        }

        let config = DocxConfig::new(10.0, 500 * 1024 * 1024, 1000);
        let res = validate_docx_zip(&buffer, &config);

        assert!(res.is_err());
        match res.unwrap_err() {
            IngestionError::DecompressionBombSuspected { ratio, entry_name } => {
                assert_eq!(entry_name, "word/document.xml");
                assert!(ratio > 10.0, "Expected ratio > 10, got {ratio}");
            }
            other => panic!("Expected DecompressionBombSuspected, got {other:?}"),
        }
    }

    #[test]
    fn test_zip_bomb_entry_count_exceeded() {
        let mut buffer = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buffer));
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            for i in 0..10 {
                zip.start_file(format!("file_{i}.txt"), options).unwrap();
                zip.write_all(b"a").unwrap();
            }
            zip.finish().unwrap();
        }

        let config = DocxConfig::new(100.0, 500 * 1024 * 1024, 5);
        let res = validate_docx_zip(&buffer, &config);

        assert!(res.is_err());
        match res.unwrap_err() {
            IngestionError::EntryCountExceeded {
                entries,
                max_entries,
            } => {
                assert_eq!(entries, 10);
                assert_eq!(max_entries, 5);
            }
            other => panic!("Expected EntryCountExceeded, got {other:?}"),
        }
    }

    #[test]
    fn test_zip_bomb_total_size_exceeded() {
        let mut buffer = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buffer));
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            zip.start_file("large.txt", options).unwrap();
            zip.write_all(&[b'x'; 2000]).unwrap();
            zip.finish().unwrap();
        }

        let config = DocxConfig::new(100.0, 1000, 100);
        let res = validate_docx_zip(&buffer, &config);

        assert!(res.is_err());
        match res.unwrap_err() {
            IngestionError::TotalSizeExceeded {
                total_bytes,
                limit_bytes,
            } => {
                assert!(total_bytes > 1000);
                assert_eq!(limit_bytes, 1000);
            }
            other => panic!("Expected TotalSizeExceeded, got {other:?}"),
        }
    }

    #[test]
    fn test_extract_docx_malformed_returns_error_no_panic() {
        let malformed_docx = b"not a zip or docx file at all";
        let res = extract_docx_bytes(malformed_docx);
        assert!(res.is_err());
    }
}
