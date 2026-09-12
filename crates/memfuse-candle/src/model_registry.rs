// FILE-CONTEXT
// STAND: 2026-09-09T15:45:22Z (SESSION: 6cae458a)
// ZWECK: Model fingerprinting and CandleQuantization definitions.
// INVARIANTEN: SHA-256 over weight blob concatenated with quantization string; distinct quant tiers yield distinct fingerprints.

use memfuse_core::MemFuseError;
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::str::FromStr;

/// Supported Candle quantization formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandleQuantization {
    /// 4-bit medium K-quantization
    Q4KM,
    /// 8-bit quantization
    Q8_0,
    /// 16-bit floating point
    F16,
}

impl fmt::Display for CandleQuantization {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Q4KM => write!(f, "Q4_K_M"),
            Self::Q8_0 => write!(f, "Q8_0"),
            Self::F16 => write!(f, "F16"),
        }
    }
}

impl FromStr for CandleQuantization {
    type Err = MemFuseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_uppercase().as_str() {
            "Q4KM" | "Q4_K_M" | "Q4" => Ok(Self::Q4KM),
            "Q8_0" | "Q8" => Ok(Self::Q8_0),
            "F16" | "FP16" => Ok(Self::F16),
            _ => Err(MemFuseError::InvalidInput(format!(
                "Unsupported Candle quantization grade: {s}. Expected Q4KM, Q8_0, or F16"
            ))),
        }
    }
}

pub use memfuse_core::ModelFingerprint;

/// Computes a unique fingerprint for a model file and quantization variant.
///
/// Calculates SHA-256 over the weight blob concatenated with the quantization grade string
/// so that different quantization tiers for the same underlying weights file yield distinct hashes.
pub fn compute_fingerprint(
    model_path: &Path,
    quantization: &CandleQuantization,
) -> Result<ModelFingerprint, MemFuseError> {
    let file = File::open(model_path).map_err(|e| {
        MemFuseError::Io(std::io::Error::new(
            e.kind(),
            format!("Failed to open model file at {}: {e}", model_path.display()),
        ))
    })?;

    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536];

    loop {
        let count = reader.read(&mut buffer).map_err(MemFuseError::Io)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }

    let quant_str = quantization.to_string();
    hasher.update(quant_str.as_bytes());

    let result = hasher.finalize();
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&result);

    let model_id = model_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    Ok(ModelFingerprint {
        hash,
        model_id,
        quantization: quant_str,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_compute_fingerprint_differs_by_quantization() -> Result<(), Box<dyn std::error::Error>>
    {
        let mut tmp_file = NamedTempFile::new()?;
        tmp_file.write_all(b"mock weight blob data 1234567890")?;

        let fp_q4 = compute_fingerprint(tmp_file.path(), &CandleQuantization::Q4KM)?;
        let fp_q8 = compute_fingerprint(tmp_file.path(), &CandleQuantization::Q8_0)?;
        let fp_f16 = compute_fingerprint(tmp_file.path(), &CandleQuantization::F16)?;

        assert_ne!(
            fp_q4.hash, fp_q8.hash,
            "Q4KM and Q8_0 must produce different hashes"
        );
        assert_ne!(
            fp_q8.hash, fp_f16.hash,
            "Q8_0 and F16 must produce different hashes"
        );
        assert_ne!(
            fp_q4.hash, fp_f16.hash,
            "Q4KM and F16 must produce different hashes"
        );

        assert_eq!(fp_q4.quantization, "Q4_K_M");
        assert_eq!(fp_q8.quantization, "Q8_0");
        assert_eq!(fp_f16.quantization, "F16");

        Ok(())
    }

    #[test]
    fn test_compute_fingerprint_nonexistent_file_returns_err() {
        let path = Path::new("/nonexistent/file/path.gguf");
        let res = compute_fingerprint(path, &CandleQuantization::Q4KM);
        assert!(res.is_err(), "Expected error for nonexistent file");
    }
}
