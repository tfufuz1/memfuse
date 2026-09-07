use candle_core::quantized::gguf_file;
use memfuse_core::MemFuseError;
use std::fs::File;
use std::path::Path;

/// Metadata extracted from a GGUF model container.
#[derive(Debug, Clone)]
pub struct GgufMetadata {
    /// Identified architecture (e.g., "llama", "mistral").
    pub architecture: String,
    /// Number of tensors present in the container.
    pub tensor_count: usize,
    /// Metadata keys present in the GGUF header.
    pub metadata_keys: Vec<String>,
}

/// Inspects a GGUF model file and extracts its architecture and metadata header keys.
///
/// Uses `candle_core::quantized::gguf_file` to parse container headers without reading tensor payloads into memory.
pub fn parse_gguf_metadata(model_path: &Path) -> Result<GgufMetadata, MemFuseError> {
    let mut file = File::open(model_path).map_err(|e| {
        MemFuseError::Io(std::io::Error::new(
            e.kind(),
            format!(
                "Failed to open GGUF model file {}: {e}",
                model_path.display()
            ),
        ))
    })?;

    let content = gguf_file::Content::read(&mut file).map_err(|e| {
        MemFuseError::Internal(format!(
            "Failed to parse GGUF container header for {}: {e}",
            model_path.display()
        ))
    })?;

    let architecture = content
        .metadata
        .get("general.architecture")
        .and_then(|v| v.to_string().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let metadata_keys = content.metadata.keys().cloned().collect();
    let tensor_count = content.tensor_infos.len();

    Ok(GgufMetadata {
        architecture,
        tensor_count,
        metadata_keys,
    })
}
