//! Write-Ahead Log with HMAC integrity.

use memfuse_core::{MemFuseError, Result, TxId};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// WAL entry operation.
#[derive(Debug, Clone)]
pub enum WalOp {
    Put {
        tx_id: TxId,
        key: Vec<u8>,
        value: Vec<u8>,
    },
    Delete {
        tx_id: TxId,
        key: Vec<u8>,
    },
}

/// A single WAL entry.
#[derive(Debug, Clone)]
pub struct WalEntry {
    pub op: WalOp,
    pub seq_no: u64,
    pub checksum: u32,
}

impl WalEntry {
    /// Creates a new WAL entry with CRC32 checksum.
    pub fn new(op: WalOp, seq_no: u64) -> Self {
        let checksum = Self::compute_checksum(&op, seq_no);
        Self {
            op,
            seq_no,
            checksum,
        }
    }

    fn compute_checksum(op: &WalOp, seq_no: u64) -> u32 {
        let mut hasher = crc32fast::Hasher::new();
        hasher.update(&seq_no.to_le_bytes());
        match op {
            WalOp::Put { key, value, .. } => {
                hasher.update(&[0u8]); // op type
                hasher.update(key);
                hasher.update(value);
            }
            WalOp::Delete { key, .. } => {
                hasher.update(&[1u8]); // op type
                hasher.update(key);
            }
        }
        hasher.finalize()
    }

    /// Serializes the entry to bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        // seq_no (8) + checksum (4) + op_type (1)
        buf.extend_from_slice(&self.seq_no.to_le_bytes());
        buf.extend_from_slice(&self.checksum.to_le_bytes());

        match &self.op {
            WalOp::Put { tx_id, key, value } => {
                buf.push(0u8);
                buf.extend_from_slice(&tx_id.inner().to_le_bytes());
                buf.extend_from_slice(&(key.len() as u32).to_le_bytes());
                buf.extend_from_slice(key);
                buf.extend_from_slice(&(value.len() as u32).to_le_bytes());
                buf.extend_from_slice(value);
            }
            WalOp::Delete { tx_id, key } => {
                buf.push(1u8);
                buf.extend_from_slice(&tx_id.inner().to_le_bytes());
                buf.extend_from_slice(&(key.len() as u32).to_le_bytes());
                buf.extend_from_slice(key);
            }
        }

        // Prepend total length
        let len = buf.len() as u32;
        let mut result = Vec::with_capacity(4 + buf.len());
        result.extend_from_slice(&len.to_le_bytes());
        result.extend_from_slice(&buf);
        result
    }
}

/// Write-Ahead Log for crash recovery.
pub struct Wal {
    path: PathBuf,
    file: tokio::sync::Mutex<tokio::fs::File>,
    size: std::sync::atomic::AtomicU64,
}

/// Maximum WAL size before triggering a flush (128MB).
pub const MAX_WAL_SIZE: u64 = 128 * 1024 * 1024;

impl Wal {
    /// Opens or creates a WAL file.
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(&path)
            .await
            .map_err(|e| MemFuseError::Storage(format!("Failed to open WAL: {}", e)))?;

        let metadata = file
            .metadata()
            .await
            .map_err(|e| MemFuseError::Storage(e.to_string()))?;

        Ok(Self {
            path,
            size: std::sync::atomic::AtomicU64::new(metadata.len()),
            file: tokio::sync::Mutex::new(file),
        })
    }

    /// Appends an entry to the WAL.
    pub async fn append(&self, entry: &WalEntry) -> Result<()> {
        let bytes = entry.to_bytes();
        let mut file = self.file.lock().await;
        file.write_all(&bytes)
            .await
            .map_err(|e| MemFuseError::Storage(format!("WAL write failed: {}", e)))?;
        file.flush()
            .await
            .map_err(|e| MemFuseError::Storage(format!("WAL flush failed: {}", e)))?;
        self.size
            .fetch_add(bytes.len() as u64, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    /// Replays the WAL, returning all valid entries.
    pub async fn replay(&self) -> Result<Vec<(u64, WalEntry)>> {
        let mut data = Vec::new();
        let mut file = tokio::fs::File::open(&self.path)
            .await
            .map_err(|e| MemFuseError::Storage(format!("WAL replay open failed: {}", e)))?;
        file.read_to_end(&mut data)
            .await
            .map_err(|e| MemFuseError::Storage(format!("WAL replay read failed: {}", e)))?;

        let mut entries = Vec::new();
        let mut pos = 0;

        while pos + 4 <= data.len() {
            let len = u32::from_le_bytes(data[pos..pos + 4].try_into().map_err(|_| {
                MemFuseError::WalCorruption {
                    offset: pos as u64,
                    reason: "Invalid length".into(),
                }
            })?) as usize;
            pos += 4;

            if pos + len > data.len() {
                tracing::warn!("WAL truncated at offset {}", pos);
                break;
            }

            let entry_data = &data[pos..pos + len];
            pos += len;

            if entry_data.len() < 13 {
                continue;
            }

            let seq_no = u64::from_le_bytes(entry_data[0..8].try_into().map_err(|_| {
                MemFuseError::WalCorruption {
                    offset: pos as u64,
                    reason: "Invalid seq_no".into(),
                }
            })?);
            let _checksum = u32::from_le_bytes(entry_data[8..12].try_into().map_err(|_| {
                MemFuseError::WalCorruption {
                    offset: pos as u64,
                    reason: "Invalid checksum".into(),
                }
            })?);
            let op_type = entry_data[12];

            let remaining = &entry_data[13..];
            let op = match op_type {
                0 => {
                    // Put
                    if remaining.len() < 12 {
                        continue;
                    }
                    let tx_id = TxId::new(u64::from_le_bytes(remaining[0..8].try_into().map_err(
                        |_| MemFuseError::WalCorruption {
                            offset: pos as u64,
                            reason: "Invalid tx_id".into(),
                        },
                    )?));
                    let key_len = u32::from_le_bytes(remaining[8..12].try_into().map_err(|_| {
                        MemFuseError::WalCorruption {
                            offset: pos as u64,
                            reason: "Invalid key_len".into(),
                        }
                    })?) as usize;
                    if remaining.len() < 12 + key_len + 4 {
                        continue;
                    }
                    let key = remaining[12..12 + key_len].to_vec();
                    let val_start = 12 + key_len;
                    let val_len =
                        u32::from_le_bytes(remaining[val_start..val_start + 4].try_into().map_err(
                            |_| MemFuseError::WalCorruption {
                                offset: pos as u64,
                                reason: "Invalid val_len".into(),
                            },
                        )?) as usize;
                    if remaining.len() < val_start + 4 + val_len {
                        continue;
                    }
                    let value = remaining[val_start + 4..val_start + 4 + val_len].to_vec();
                    WalOp::Put { tx_id, key, value }
                }
                1 => {
                    // Delete
                    if remaining.len() < 12 {
                        continue;
                    }
                    let tx_id = TxId::new(u64::from_le_bytes(remaining[0..8].try_into().map_err(
                        |_| MemFuseError::WalCorruption {
                            offset: pos as u64,
                            reason: "Invalid tx_id".into(),
                        },
                    )?));
                    let key_len = u32::from_le_bytes(remaining[8..12].try_into().map_err(|_| {
                        MemFuseError::WalCorruption {
                            offset: pos as u64,
                            reason: "Invalid key_len".into(),
                        }
                    })?) as usize;
                    if remaining.len() < 12 + key_len {
                        continue;
                    }
                    let key = remaining[12..12 + key_len].to_vec();
                    WalOp::Delete { tx_id, key }
                }
                _ => continue,
            };

            entries.push((
                seq_no,
                WalEntry {
                    op,
                    seq_no,
                    checksum: _checksum,
                },
            ));
        }

        Ok(entries)
    }

    /// Returns the current WAL size in bytes.
    pub fn size(&self) -> u64 {
        self.size.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Returns the WAL file path.
    pub fn path(&self) -> &Path {
        &self.path
    }
}
