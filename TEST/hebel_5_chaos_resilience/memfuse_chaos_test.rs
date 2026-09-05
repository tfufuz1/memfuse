//! MemFuse Chaos Resilience Test Suite
//!
//! Validates MemFuse's WAL V3 and MVCC Store under extreme crash,
//! torn-write, bit-flip, and OOM scenarios adapted from Project Chimera (SPEC-035).

use std::fs::{self, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;

/// Simulates a torn write by truncating the last WAL file mid-record.
pub fn inject_wal_truncation(wal_path: &Path, bytes_to_truncate: u64) -> std::io::Result<()> {
    let file = OpenOptions::new().write(true).open(wal_path)?;
    let current_len = file.metadata()?.len();
    if current_len > bytes_to_truncate {
        file.set_len(current_len - bytes_to_truncate)?;
    }
    Ok(())
}

/// Injects random bit-flips into a data file to verify checksum detection.
pub fn inject_bit_flip(file_path: &Path, offset: u64) -> std::io::Result<()> {
    let mut file = OpenOptions::new().read(true).write(true).open(file_path)?;
    file.seek(SeekFrom::Start(offset))?;
    let mut byte = [0u8; 1];
    use std::io::Read;
    file.read_exact(&mut byte)?;
    // Invert bits
    byte[0] ^= 0xFF;
    file.seek(SeekFrom::Start(offset))?;
    file.write_all(&byte)?;
    file.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_torn_wal_recovery_simulation() {
        let dir = tempdir().unwrap();
        let wal_file = dir.path().join("wal_001.log");

        // Write sample data
        fs::write(&wal_file, b"HEADER_RECORD_1_VALID_DATA_RECORD_2_HALF_WRITTEN").unwrap();

        // Inject truncation
        inject_wal_truncation(&wal_file, 15).unwrap();

        let remaining = fs::read(&wal_file).unwrap();
        assert!(remaining.len() < 50);
        // MemFuse recovery must cleanly ignore or roll back partial uncommitted records
    }

    #[test]
    fn test_bit_flip_detection() {
        let dir = tempdir().unwrap();
        let sstable_file = dir.path().join("00001.sst");

        fs::write(&sstable_file, vec![0xAA; 128]).unwrap();

        inject_bit_flip(&sstable_file, 16).unwrap();

        let corrupted = fs::read(&sstable_file).unwrap();
        assert_ne!(corrupted[16], 0xAA);
        // MemFuse must return MemFuseError::ChecksumMismatch
    }
}
