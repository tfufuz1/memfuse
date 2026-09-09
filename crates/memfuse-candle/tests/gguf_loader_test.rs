// FILE-CONTEXT
// STAND: 2026-09-09T12:44:49Z (SESSION: c74a1828)
// ZWECK: Unit tests for GGUF metadata parsing error paths.

use memfuse_candle::gguf_loader::parse_gguf_metadata;
use std::io::Write;
use std::path::Path;
use tempfile::NamedTempFile;

#[test]
fn test_parse_gguf_metadata_nonexistent_file() {
    let res = parse_gguf_metadata(Path::new("/nonexistent/model.gguf"));
    assert!(res.is_err());
    let err_str = res.unwrap_err().to_string();
    assert!(err_str.contains("Failed to open GGUF model file"));
}

#[test]
fn test_parse_gguf_metadata_corrupt_file() {
    let mut tmp = NamedTempFile::new().unwrap();
    tmp.write_all(b"not a valid gguf container header").unwrap();

    let res = parse_gguf_metadata(tmp.path());
    assert!(res.is_err());
    let err_str = res.unwrap_err().to_string();
    assert!(err_str.contains("Failed to parse GGUF container header"));
}
