use memfuse_core::MemFuseError;
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

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

/// Fingerprint uniquely identifying a model weight file and its quantization tier.
///
/// Note: If a central `ModelFingerprint` is added to Layer 0 (`memfuse-core`), this type
/// can be migrated there in a future refactoring.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelFingerprint {
    /// SHA-256 hash digest over the model weights concatenated with the quantization string.
    pub hash: [u8; 32],
    /// Identifier or filename of the model.
    pub model_id: String,
    /// String representation of quantization.
    pub quantization: String,
}

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
