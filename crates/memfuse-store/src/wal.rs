// ANCHOR:DOC:DOC-WAL-001 — Missing module documentation
// WP:WP-0.0 PRIO:3 NEEDS:NONE
// AGENT:02 DATE:2026-05-09 STATUS:REVIEW
// CREATED:2026-05-09 DEADLINE:NONE
// ANCHOR:ARCH:WAL-001 — Write-Ahead Log für Crash Recovery.
// WP:WP-0.0 PRIO:1 NEEDS:NONE
// AGENT:01 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-05 DEADLINE:NONE
// FORMAT: [u32 len][u64 seq_no][u32 crc32][u8 op_type][payload...]
// INVARIANTE: Jeder Eintrag wird ERST in WAL geschrieben, DANN in MemTable übernommen.
// REPLAY: Bei Neustart wird WAL komplett in MemTable replayed (lsm.rs::new()).
// ROTATION: Beim Flush wird alte WAL archiviert, neue geöffnet.
//
// ANCHOR:SPEC:WP-3.2-HMAC-001 — HMAC-Integrity statt CRC32 für Encryption-at-Rest.
// WP:WP-3.2 PRIO:3 NEEDS:NONE
// AGENT:10 DATE:2026-05-09 STATUS:READY
// CREATED:2026-05-09 DEADLINE:NONE
//! Write-Ahead Log (WAL) for durability and crash recovery.
//!
//! ## Workflow
//! 1. Every write operation (Put/Delete) is first appended to the WAL.
//! 2. `sync_all()` is called to ensure the entry is persisted to physical disk.
//! 3. The operation is then applied to the in-memory MemTable.
//!
//! ## Crash Recovery
//! Upon restart, the `LsmStorage` engine replays the WAL from start to end,
//! reconstructing the state of the MemTable as it was before the crash.
//! Entries with invalid CRC32 checksums are ignored, and replay stops
//! at the first point of corruption.
//!
//! ## Invariants
//! - **Durability**: Every committed transaction is guaranteed to be in the WAL.
//! - **Integrity**: Entries are protected by CRC32 checksums to detect data corruption.
//! - **Async I/O**: Operations use `tokio::fs` for non-blocking disk access.
//
// ANCHOR:PERF:LATENCY-001 — WAL-Write-Path Hotspot
// WP:WP-0.0 PRIO:2 NEEDS:NONE
// AGENT:08-perf DATE:2026-05-09 STATUS:READY
// CREATED:2026-05-09 DEADLINE:NONE
// TARGET: < 2ms bei Peak-Load
// AKTUELL: Unbekannt (Sync Flush)
// BOTTLENECK: I/O (File::sync_all blockiert)
// OPTIMIERUNGSIDEE: Group Commit oder fsync-Offloading

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
        let op_size = match &self.op {
            WalOp::Put { key, value, .. } => 1 + 8 + 4 + key.len() + 4 + value.len(),
            WalOp::Delete { key, .. } => 1 + 8 + 4 + key.len(),
        };
        // Verify op_size matches what is actually written in the match block below.
        // Put: type(1) + tx_id(8) + k_len(4) + key + v_len(4) + value
        // Delete: type(1) + tx_id(8) + k_len(4) + key

        let payload_size = 8 + 4 + op_size; // seq_no(8) + checksum(4) + op
        let total_size = 4 + payload_size; // length prefix + payload

        let mut buf = Vec::with_capacity(total_size);
        buf.extend_from_slice(&(payload_size as u32).to_le_bytes());
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
        buf
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
        // ANCHOR:ALG-FIX:D1-001 — fsync für WAL-Durability (INV-LSM-5)
        // WP:WP-0.0 PRIO:1 NEEDS:NONE
        // AGENT:13 DATE:2026-05-08 STATUS:DONE
        // CREATED:2026-05-08 DEADLINE:NONE
        // flush() schreibt nur in den OS-Page-Cache. sync_all() erzwingt
        // Physical Write auf Disk — ohne das ist WAL bei Stromausfall wertlos.
        file.sync_all()
            .await
            .map_err(|e| MemFuseError::Storage(format!("WAL fsync failed: {}", e)))?;
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
            let len = u32::from_le_bytes(
                data.get(pos..pos + 4)
                    .ok_or_else(|| MemFuseError::WalCorruption {
                        offset: pos as u64,
                        reason: "Buffer underrun reading length".into(),
                    })?
                    .try_into()
                    .map_err(|_| MemFuseError::WalCorruption {
                        offset: pos as u64,
                        reason: "Invalid length slice".into(),
                    })?,
            ) as usize;
            pos += 4;

            if pos + len > data.len() {
                tracing::warn!("WAL truncated at offset {}", pos);
                break;
            }

            let entry_data =
                data.get(pos..pos + len)
                    .ok_or_else(|| MemFuseError::WalCorruption {
                        offset: pos as u64,
                        reason: "Buffer underrun reading entry data".into(),
                    })?;
            pos += len;

            if entry_data.len() < 13 {
                continue;
            }

            let seq_no = u64::from_le_bytes(
                entry_data
                    .get(0..8)
                    .ok_or_else(|| MemFuseError::WalCorruption {
                        offset: pos as u64,
                        reason: "Invalid seq_no buffer".into(),
                    })?
                    .try_into()
                    .map_err(|_| MemFuseError::WalCorruption {
                        offset: pos as u64,
                        reason: "Invalid seq_no".into(),
                    })?,
            );
            let stored_checksum = u32::from_le_bytes(
                entry_data
                    .get(8..12)
                    .ok_or_else(|| MemFuseError::WalCorruption {
                        offset: pos as u64,
                        reason: "Invalid checksum buffer".into(),
                    })?
                    .try_into()
                    .map_err(|_| MemFuseError::WalCorruption {
                        offset: pos as u64,
                        reason: "Invalid checksum".into(),
                    })?,
            );
            let op_type = *entry_data
                .get(12)
                .ok_or_else(|| MemFuseError::WalCorruption {
                    offset: pos as u64,
                    reason: "Invalid op_type".into(),
                })?;

            let remaining = entry_data
                .get(13..)
                .ok_or_else(|| MemFuseError::WalCorruption {
                    offset: pos as u64,
                    reason: "Invalid remaining data".into(),
                })?;
            let op = match op_type {
                0 => {
                    // Put
                    if remaining.len() < 12 {
                        continue;
                    }
                    let tx_id = TxId::new(u64::from_le_bytes(
                        remaining
                            .get(0..8)
                            .ok_or_else(|| MemFuseError::WalCorruption {
                                offset: pos as u64,
                                reason: "Invalid tx_id buffer".into(),
                            })?
                            .try_into()
                            .map_err(|_| MemFuseError::WalCorruption {
                                offset: pos as u64,
                                reason: "Invalid tx_id".into(),
                            })?,
                    ));
                    let key_len = u32::from_le_bytes(
                        remaining
                            .get(8..12)
                            .ok_or_else(|| MemFuseError::WalCorruption {
                                offset: pos as u64,
                                reason: "Invalid key_len buffer".into(),
                            })?
                            .try_into()
                            .map_err(|_| MemFuseError::WalCorruption {
                                offset: pos as u64,
                                reason: "Invalid key_len".into(),
                            })?,
                    ) as usize;
                    if remaining.len() < 12 + key_len + 4 {
                        continue;
                    }
                    let key = remaining
                        .get(12..12 + key_len)
                        .ok_or_else(|| MemFuseError::WalCorruption {
                            offset: pos as u64,
                            reason: "Invalid key buffer".into(),
                        })?
                        .to_vec();
                    let val_start = 12 + key_len;
                    let val_len = u32::from_le_bytes(
                        remaining
                            .get(val_start..val_start + 4)
                            .ok_or_else(|| MemFuseError::WalCorruption {
                                offset: pos as u64,
                                reason: "Invalid val_len buffer".into(),
                            })?
                            .try_into()
                            .map_err(|_| MemFuseError::WalCorruption {
                                offset: pos as u64,
                                reason: "Invalid val_len".into(),
                            })?,
                    ) as usize;
                    let value = remaining
                        .get(val_start + 4..val_start + 4 + val_len)
                        .ok_or_else(|| MemFuseError::WalCorruption {
                            offset: pos as u64,
                            reason: "Invalid value buffer".into(),
                        })?
                        .to_vec();
                    WalOp::Put { tx_id, key, value }
                }
                1 => {
                    // Delete
                    if remaining.len() < 12 {
                        continue;
                    }
                    let tx_id = TxId::new(u64::from_le_bytes(
                        remaining
                            .get(0..8)
                            .ok_or_else(|| MemFuseError::WalCorruption {
                                offset: pos as u64,
                                reason: "Invalid tx_id buffer".into(),
                            })?
                            .try_into()
                            .map_err(|_| MemFuseError::WalCorruption {
                                offset: pos as u64,
                                reason: "Invalid tx_id".into(),
                            })?,
                    ));
                    let key_len = u32::from_le_bytes(
                        remaining
                            .get(8..12)
                            .ok_or_else(|| MemFuseError::WalCorruption {
                                offset: pos as u64,
                                reason: "Invalid key_len buffer".into(),
                            })?
                            .try_into()
                            .map_err(|_| MemFuseError::WalCorruption {
                                offset: pos as u64,
                                reason: "Invalid key_len".into(),
                            })?,
                    ) as usize;
                    let key = remaining
                        .get(12..12 + key_len)
                        .ok_or_else(|| MemFuseError::WalCorruption {
                            offset: pos as u64,
                            reason: "Invalid key buffer".into(),
                        })?
                        .to_vec();
                    WalOp::Delete { tx_id, key }
                }
                _ => continue,
            };

            // ANCHOR:ALG-FIX:D1-007 — CRC32-Verifikation bei WAL Replay
            // WP:WP-0.0 PRIO:1 NEEDS:NONE
            // AGENT:13 DATE:2026-05-08 STATUS:DONE
            // CREATED:2026-05-08 DEADLINE:NONE
            // Ohne Verifikation werden korrupte Entries (Bit-Flip, Partial Write)
            // blind in die MemTable replayed → stille Datenkorrumpierung.
            let recomputed_checksum = WalEntry::compute_checksum(&op, seq_no);
            if recomputed_checksum != stored_checksum {
                tracing::warn!(
                    "WAL entry at offset {} has invalid checksum (stored={}, computed={}), truncating replay",
                    pos, stored_checksum, recomputed_checksum
                );
                break;
            }

            entries.push((
                seq_no,
                WalEntry {
                    op,
                    seq_no,
                    checksum: stored_checksum,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wal_entry_serialization_roundtrip() {
        let op = WalOp::Put {
            tx_id: TxId::new(42),
            key: b"key".to_vec(),
            value: b"value".to_vec(),
        };
        let entry = WalEntry::new(op, 100);
        let bytes = entry.to_bytes();

        // Manual verification of length
        // total_len(4) + seq_no(8) + checksum(4) + op_type(1) + tx_id(8) + k_len(4) + key(3) + v_len(4) + val(5)
        // 4 + 8 + 4 + 1 + 8 + 4 + 3 + 4 + 5 = 41
        assert_eq!(bytes.len(), 41);

        let payload_len = u32::from_le_bytes(bytes[0..4].try_into().expect("valid slice"));
        assert_eq!(payload_len, 37);

        // Test with Delete
        let op2 = WalOp::Delete {
            tx_id: TxId::new(43),
            key: b"key2".to_vec(),
        };
        let entry2 = WalEntry::new(op2, 101);
        let bytes2 = entry2.to_bytes();
        // 4 + 8 + 4 + 1 + 8 + 4 + key(4) = 33
        assert_eq!(bytes2.len(), 33);
    }
}
