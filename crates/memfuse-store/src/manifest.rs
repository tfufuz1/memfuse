//! SSTable Manifest for append-only tracking of active SSTable sets.
// FILE-CONTEXT
// STAND: 2026-09-11
// ZWECK: Append-Only Manifest-Protokolldatei zur Verfolgung gültiger SSTables für Crash-Safety.
// INVARIANTEN: fsync NACH jedem Manifest-Eintrag; Add erst nach fsync der SSTable-Datei.

use memfuse_core::{MemFuseError, Result};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Maximum allowed payload size for a single manifest entry (1 MB).
pub const MAX_MANIFEST_ENTRY_SIZE: u32 = 1024 * 1024;

/// An entry in the SSTable manifest log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestEntry {
    /// A new SSTable file was created and fully fsynced.
    Add { path: PathBuf, max_tx: u64 },
    /// An SSTable file was removed or superseded.
    Remove { path: PathBuf },
    /// A transaction rollback operation completed.
    RollbackComplete { target_tx: u64 },
}

impl ManifestEntry {
    /// Serializes the entry into binary format with a 4-byte total size prefix and a 4-byte CRC32.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut payload = Vec::new();
        match self {
            ManifestEntry::Add { path, max_tx } => {
                payload.push(0u8); // op_tag = 0
                payload.extend_from_slice(&max_tx.to_le_bytes());
                let path_str = path.to_string_lossy();
                let path_bytes = path_str.as_bytes();
                payload.extend_from_slice(&(path_bytes.len() as u32).to_le_bytes());
                payload.extend_from_slice(path_bytes);
            }
            ManifestEntry::Remove { path } => {
                payload.push(1u8); // op_tag = 1
                let path_str = path.to_string_lossy();
                let path_bytes = path_str.as_bytes();
                payload.extend_from_slice(&(path_bytes.len() as u32).to_le_bytes());
                payload.extend_from_slice(path_bytes);
            }
            ManifestEntry::RollbackComplete { target_tx } => {
                payload.push(2u8); // op_tag = 2
                payload.extend_from_slice(&target_tx.to_le_bytes());
            }
        }

        let total_payload_size = (4 + payload.len()) as u32;
        if total_payload_size > MAX_MANIFEST_ENTRY_SIZE {
            return Err(MemFuseError::Serialization(format!(
                "Manifest entry exceeds max size: {} bytes",
                total_payload_size
            )));
        }

        let crc = crc32fast::hash(&payload);

        let mut buf = Vec::with_capacity(4 + total_payload_size as usize);
        buf.extend_from_slice(&total_payload_size.to_le_bytes());
        buf.extend_from_slice(&crc.to_le_bytes());
        buf.extend_from_slice(&payload);

        Ok(buf)
    }

    /// Deserializes a manifest entry from a slice containing `[crc32: u32 LE] [payload...]`.
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < 5 {
            return Err(MemFuseError::Serialization(
                "Manifest entry too short".into(),
            ));
        }

        let stored_crc = u32::from_le_bytes(
            data[0..4]
                .try_into()
                .map_err(|_| MemFuseError::Serialization("Invalid CRC format".into()))?,
        );
        let payload = &data[4..];
        let computed_crc = crc32fast::hash(payload);

        if stored_crc != computed_crc {
            return Err(MemFuseError::Serialization(format!(
                "CRC mismatch: stored={:#010x}, computed={:#010x}",
                stored_crc, computed_crc
            )));
        }

        let op_tag = payload[0];
        let remaining = &payload[1..];

        match op_tag {
            0 => {
                // Add
                if remaining.len() < 12 {
                    return Err(MemFuseError::Serialization("Add payload too short".into()));
                }
                let max_tx = u64::from_le_bytes(
                    remaining[0..8]
                        .try_into()
                        .map_err(|_| MemFuseError::Serialization("Invalid max_tx format".into()))?,
                );
                let path_len = u32::from_le_bytes(
                    remaining[8..12]
                        .try_into()
                        .map_err(|_| MemFuseError::Serialization("Invalid path_len format".into()))?,
                ) as usize;
                if remaining.len() < 12 + path_len {
                    return Err(MemFuseError::Serialization("Add path data truncated".into()));
                }
                let path_str = std::str::from_utf8(&remaining[12..12 + path_len])
                    .map_err(|e| MemFuseError::Serialization(format!("Invalid path UTF-8: {}", e)))?;
                Ok(ManifestEntry::Add {
                    path: PathBuf::from(path_str),
                    max_tx,
                })
            }
            1 => {
                // Remove
                if remaining.len() < 4 {
                    return Err(MemFuseError::Serialization("Remove payload too short".into()));
                }
                let path_len = u32::from_le_bytes(
                    remaining[0..4]
                        .try_into()
                        .map_err(|_| MemFuseError::Serialization("Invalid path_len format".into()))?,
                ) as usize;
                if remaining.len() < 4 + path_len {
                    return Err(MemFuseError::Serialization("Remove path data truncated".into()));
                }
                let path_str = std::str::from_utf8(&remaining[4..4 + path_len])
                    .map_err(|e| MemFuseError::Serialization(format!("Invalid path UTF-8: {}", e)))?;
                Ok(ManifestEntry::Remove {
                    path: PathBuf::from(path_str),
                })
            }
            2 => {
                // RollbackComplete
                if remaining.len() < 8 {
                    return Err(MemFuseError::Serialization(
                        "RollbackComplete payload too short".into(),
                    ));
                }
                let target_tx = u64::from_le_bytes(
                    remaining[0..8]
                        .try_into()
                        .map_err(|_| MemFuseError::Serialization("Invalid target_tx format".into()))?,
                );
                Ok(ManifestEntry::RollbackComplete { target_tx })
            }
            _ => Err(MemFuseError::Serialization(format!(
                "Unknown Manifest op tag: {}",
                op_tag
            ))),
        }
    }
}

/// Append-only Manifest file handle for recording active SSTable set transitions.
pub struct Manifest {
    path: PathBuf,
    file: tokio::sync::Mutex<tokio::fs::File>,
}

impl std::fmt::Debug for Manifest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Manifest")
            .field("path", &self.path)
            .finish()
    }
}

impl Manifest {
    /// Opens or creates an append-only Manifest file at `path`.
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let (file, is_new) = match tokio::fs::OpenOptions::new()
            .create_new(true)
            .append(true)
            .read(true)
            .open(&path)
            .await
        {
            Ok(file) => (file, true),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let file = tokio::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .read(true)
                    .open(&path)
                    .await
                    .map_err(|e| MemFuseError::Storage(format!("Failed to open MANIFEST: {}", e)))?;
                (file, false)
            }
            Err(e) => {
                return Err(MemFuseError::Storage(format!(
                    "Failed to create MANIFEST: {}",
                    e
                )));
            }
        };

        if is_new {
            file.sync_all().await.map_err(|e| {
                MemFuseError::Storage(format!(
                    "MANIFEST file fsync failed for {}: {}",
                    path.display(),
                    e
                ))
            })?;
            crate::util::fsync_parent_dir(&path).await?;
        }

        Ok(Self {
            path,
            file: tokio::sync::Mutex::new(file),
        })
    }

    /// Appends a new entry to the manifest and performs flush + fsync.
    pub async fn append(&self, entry: &ManifestEntry) -> Result<()> {
        let bytes = entry.to_bytes()?;
        let mut file = self.file.lock().await;
        file.write_all(&bytes).await.map_err(|e| {
            MemFuseError::Storage(format!(
                "MANIFEST write failed for {}: {}",
                self.path.display(),
                e
            ))
        })?;
        file.flush().await.map_err(|e| {
            MemFuseError::Storage(format!(
                "MANIFEST flush failed for {}: {}",
                self.path.display(),
                e
            ))
        })?;
        file.sync_all().await.map_err(|e| {
            MemFuseError::Storage(format!(
                "MANIFEST fsync failed for {}: {}",
                self.path.display(),
                e
            ))
        })?;
        Ok(())
    }

    /// Loads all valid manifest entries from `path`.
    ///
    /// If the file does not exist, returns an empty vector.
    /// If corruption or truncation occurs at the tail of the file, logs a warning and
    /// returns all valid entries read up to the point of failure (WAL chain break recovery pattern).
    pub async fn load(path: &Path) -> Result<Vec<ManifestEntry>> {
        if !tokio::fs::try_exists(path).await.unwrap_or(false) {
            return Ok(Vec::new());
        }

        let mut file = match tokio::fs::File::open(path).await {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => {
                return Err(MemFuseError::Storage(format!(
                    "Failed to open MANIFEST for reading: {}",
                    e
                )))
            }
        };

        let file_size = file
            .metadata()
            .await
            .map_err(|e| MemFuseError::Storage(e.to_string()))?
            .len();

        let mut reader = tokio::io::BufReader::new(&mut file);
        let mut entries = Vec::new();
        let mut pos = 0u64;

        loop {
            let mut len_bytes = [0u8; 4];
            match reader.read_exact(&mut len_bytes).await {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => {
                    tracing::warn!("MANIFEST read error at offset {}: {}", pos, e);
                    break;
                }
            }

            let len = u32::from_le_bytes(len_bytes) as usize;
            if len > MAX_MANIFEST_ENTRY_SIZE as usize || pos + 4 + len as u64 > file_size {
                tracing::warn!(
                    "MANIFEST truncation or corrupt entry length ({}) at offset {}",
                    len,
                    pos
                );
                break;
            }

            let mut entry_raw = vec![0u8; len];
            match reader.read_exact(&mut entry_raw).await {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    tracing::warn!("MANIFEST truncated tail payload at offset {}", pos);
                    break;
                }
                Err(e) => {
                    tracing::warn!("MANIFEST read error at offset {}: {}", pos, e);
                    break;
                }
            }

            pos += 4 + len as u64;

            match ManifestEntry::from_bytes(&entry_raw) {
                Ok(entry) => entries.push(entry),
                Err(e) => {
                    tracing::warn!(
                        "MANIFEST entry corruption at offset {}: {} — discarding tail",
                        pos,
                        e
                    );
                    break;
                }
            }
        }

        Ok(entries)
    }

    /// Reconstructs the set of currently valid SSTable file names (or path components)
    /// from a sequence of manifest entries by applying `Add` and `Remove` operations sequentially.
    pub fn reconstruct_valid_sstables(entries: &[ManifestEntry]) -> HashSet<PathBuf> {
        let mut valid = HashSet::new();
        for entry in entries {
            match entry {
                ManifestEntry::Add { path, .. } => {
                    let key = path.file_name().map(PathBuf::from).unwrap_or_else(|| path.clone());
                    valid.insert(key);
                }
                ManifestEntry::Remove { path } => {
                    let key = path.file_name().map(PathBuf::from).unwrap_or_else(|| path.clone());
                    valid.remove(&key);
                }
                ManifestEntry::RollbackComplete { .. } => {}
            }
        }
        valid
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_manifest_entry_roundtrip() {
        let entries = vec![
            ManifestEntry::Add {
                path: PathBuf::from("sst-00000000000000000001-000000.sst"),
                max_tx: 42,
            },
            ManifestEntry::Remove {
                path: PathBuf::from("sst-00000000000000000001-000000.sst"),
            },
            ManifestEntry::RollbackComplete { target_tx: 100 },
        ];

        for entry in entries {
            let bytes = entry.to_bytes().expect("serialization should succeed");
            let payload_from_bytes = &bytes[4..]; // Skip total_payload_size prefix
            let decoded = ManifestEntry::from_bytes(payload_from_bytes)
                .expect("deserialization should succeed");
            assert_eq!(entry, decoded);
        }
    }

    #[tokio::test]
    async fn test_manifest_crc_corruption_detection() {
        let dir = tempdir().expect("tempdir");
        let manifest_path = dir.path().join("MANIFEST");

        let manifest = Manifest::open(&manifest_path).await.expect("open manifest");

        let entry1 = ManifestEntry::Add {
            path: PathBuf::from("sst-1.sst"),
            max_tx: 10,
        };
        let entry2 = ManifestEntry::Add {
            path: PathBuf::from("sst-2.sst"),
            max_tx: 20,
        };

        manifest.append(&entry1).await.expect("append 1");
        manifest.append(&entry2).await.expect("append 2");
        drop(manifest);

        // Corrupt entry 2 (flip bytes near the end of the file)
        let mut file_bytes = tokio::fs::read(&manifest_path).await.expect("read file");
        let len = file_bytes.len();
        file_bytes[len - 2] ^= 0xFF;
        tokio::fs::write(&manifest_path, file_bytes)
            .await
            .expect("write corrupted file");

        let loaded = Manifest::load(&manifest_path).await.expect("load manifest");
        assert_eq!(loaded.len(), 1, "Should recover entry 1 and stop before corrupted entry 2");
        assert_eq!(loaded[0], entry1);
    }

    #[tokio::test]
    async fn test_manifest_truncated_tail_recovery() {
        let dir = tempdir().expect("tempdir");
        let manifest_path = dir.path().join("MANIFEST");

        let manifest = Manifest::open(&manifest_path).await.expect("open manifest");

        let entry1 = ManifestEntry::Add {
            path: PathBuf::from("sst-1.sst"),
            max_tx: 10,
        };
        let entry2 = ManifestEntry::Add {
            path: PathBuf::from("sst-2.sst"),
            max_tx: 20,
        };

        manifest.append(&entry1).await.expect("append 1");
        manifest.append(&entry2).await.expect("append 2");
        drop(manifest);

        // Truncate the file in the middle of entry 2
        let mut file_bytes = tokio::fs::read(&manifest_path).await.expect("read file");
        file_bytes.truncate(file_bytes.len() - 8);
        tokio::fs::write(&manifest_path, file_bytes)
            .await
            .expect("write truncated file");

        let loaded = Manifest::load(&manifest_path).await.expect("load manifest");
        assert_eq!(loaded.len(), 1, "Truncated tail entry should be safely ignored");
        assert_eq!(loaded[0], entry1);
    }

    #[test]
    fn test_reconstruct_valid_sstables() {
        let entries = vec![
            ManifestEntry::Add {
                path: PathBuf::from("/data/sst-1.sst"),
                max_tx: 10,
            },
            ManifestEntry::Add {
                path: PathBuf::from("sst-2.sst"),
                max_tx: 20,
            },
            ManifestEntry::Remove {
                path: PathBuf::from("/data/sst-1.sst"),
            },
            ManifestEntry::Add {
                path: PathBuf::from("sst-3.sst"),
                max_tx: 30,
            },
        ];

        let valid = Manifest::reconstruct_valid_sstables(&entries);
        assert!(!valid.contains(Path::new("sst-1.sst")));
        assert!(valid.contains(Path::new("sst-2.sst")));
        assert!(valid.contains(Path::new("sst-3.sst")));
        assert_eq!(valid.len(), 2);
    }
}
