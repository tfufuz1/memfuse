//! Write-Ahead Log (WAL) for durability and crash recovery.
// ANCHOR:DOC:DOC-WAL-001 — Missing module documentation
// WP:WP-0.0 PRIO:3 NEEDS:NONE
// AGENT:02 DATE:2026-05-16 STATUS:REVIEW
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
// AGENT:10 DATE:2026-05-09 STATUS:REVIEW
// CREATED:2026-05-09 DEADLINE:NONE
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
//!
//! ## Performance
//! The current WAL implementation uses `sync_all()` (fsync) after every append to ensure
//! strict durability. This can be a performance bottleneck for high-throughput write
//! workloads. Future optimizations may include group commit or asynchronous fsync offloading.
//
// ANCHOR:PERF:LATENCY-001 — WAL-Write-Path Hotspot
// WP:WP-0.0 PRIO:2 NEEDS:NONE
// AGENT:08-perf DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-09 DEADLINE:NONE
// TARGET: < 2ms bei Peak-Load
// AKTUELL: ~71ns (Memory-only path verified)
// BOTTLENECK: I/O (File::sync_all blockiert)
// OPTIMIERUNGSIDEE: Group Commit oder fsync-Offloading

use crate::crypto::KeyManager;
use hmac::{Hmac, Mac};
use memfuse_core::{MemFuseError, Result, TxId};
use sha2::Sha256;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

type HmacSha256 = Hmac<Sha256>;

/// WAL entry operation.
/// Represents an operation logged in the WAL.
#[derive(Debug, Clone)]
pub enum WalOp {
    /// Inserts or updates a key-value pair.
    Put {
        tx_id: TxId,
        key: Vec<u8>,
        value: Vec<u8>,
    },
    /// Deletes a key.
    Delete { tx_id: TxId, key: Vec<u8> },
}

/// A single entry in the Write-Ahead Log.
#[derive(Debug, Clone)]
pub struct WalEntry {
    /// The operation performed.
    pub op: WalOp,
    /// Sequence number assigned to the operation.
    pub seq_no: u64,
    pub checksum: [u8; 32],
}

impl WalEntry {
    /// Creates a new WAL entry with HMAC-SHA256 checksum.
    pub fn try_new(op: WalOp, seq_no: u64, integrity_key: &[u8]) -> Result<Self> {
        let checksum = Self::compute_checksum(&op, seq_no, integrity_key)?;
        Ok(Self {
            op,
            seq_no,
            checksum,
        })
    }

    pub fn compute_checksum(op: &WalOp, seq_no: u64, integrity_key: &[u8]) -> Result<[u8; 32]> {
        let mut mac = HmacSha256::new_from_slice(integrity_key)
            .map_err(|e| MemFuseError::Storage(format!("HMAC key error: {}", e)))?;
        mac.update(&seq_no.to_le_bytes());
        match op {
            WalOp::Put { key, value, .. } => {
                mac.update(&[0u8]); // op type
                mac.update(key);
                mac.update(value);
            }
            WalOp::Delete { key, .. } => {
                mac.update(&[1u8]); // op type
                mac.update(key);
            }
        }
        Ok(mac.finalize().into_bytes().into())
    }

    /// Serializes the entry to bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let op_size = match &self.op {
            WalOp::Put { key, value, .. } => 1 + 8 + 4 + key.len() + 4 + value.len(),
            WalOp::Delete { key, .. } => 1 + 8 + 4 + key.len(),
        };

        let payload_size = 8 + 32 + op_size; // seq_no(8) + checksum(32) + op
        let total_size = 4 + payload_size; // length prefix + payload

        let mut buf = Vec::with_capacity(total_size);
        buf.extend_from_slice(&(payload_size as u32).to_le_bytes());
        buf.extend_from_slice(&self.seq_no.to_le_bytes());
        buf.extend_from_slice(&self.checksum);

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
    key_manager: Option<Arc<KeyManager>>,
}

/// Maximum WAL size before triggering a flush (128MB).
pub const MAX_WAL_SIZE: u64 = 128 * 1024 * 1024;

impl Wal {
    /// Opens or creates a WAL file.
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_with_key_manager(path, None).await
    }

    /// Opens or creates a WAL file with an optional KeyManager.
    pub async fn open_with_key_manager(
        path: impl AsRef<Path>,
        key_manager: Option<Arc<KeyManager>>,
    ) -> Result<Self> {
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
            key_manager,
        })
    }

    /// Appends an entry to the WAL.
    pub async fn append(&self, entry: &WalEntry) -> Result<()> {
        let mut bytes = entry.to_bytes();

        if let Some(km) = &self.key_manager {
            // Encrypt the payload (everything after the length prefix)
            if bytes.len() > 4 {
                let payload = &bytes[4..];
                let offset = self.size();
                let encrypted = km.encrypt(payload, offset)?;
                let mut new_bytes = Vec::with_capacity(4 + encrypted.len());
                new_bytes.extend_from_slice(&(encrypted.len() as u32).to_le_bytes());
                new_bytes.extend_from_slice(&encrypted);
                bytes = new_bytes;
            }
        }

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

        let integrity_key = if let Some(km) = &self.key_manager {
            km.integrity_key()?
        } else {
            *b"memfuse-integrity-key-v1\0\0\0\0\0\0\0\0"
        };

        while pos + 4 <= data.len() {
            let len_bytes = data.get(pos..pos + 4).ok_or(MemFuseError::WalCorruption {
                offset: pos as u64,
                reason: "Unexpected end of file while reading length".into(),
            })?;
            let len = u32::from_le_bytes(len_bytes.try_into().map_err(|_| {
                MemFuseError::WalCorruption {
                    offset: pos as u64,
                    reason: "Invalid length format".into(),
                }
            })?) as usize;
            pos += 4;

            if pos + len > data.len() {
                tracing::warn!("WAL truncated at offset {}", pos);
                break;
            }

            let entry_data_raw = data
                .get(pos..pos + len)
                .ok_or(MemFuseError::WalCorruption {
                    offset: pos as u64,
                    reason: "Unexpected end of file while reading entry data".into(),
                })?;
            pos += len;

            let decrypted_data;
            let entry_data = if let Some(km) = &self.key_manager {
                let offset = (pos - len) as u64;
                decrypted_data = km.decrypt(entry_data_raw, offset)?;
                &decrypted_data
            } else {
                entry_data_raw
            };

            if entry_data.len() < 41 {
                // seq_no(8) + checksum(32) + op_type(1)
                continue;
            }

            let seq_no = u64::from_le_bytes(
                entry_data
                    .get(0..8)
                    .ok_or(MemFuseError::WalCorruption {
                        offset: pos as u64,
                        reason: "Invalid seq_no".into(),
                    })?
                    .try_into()
                    .map_err(|_| MemFuseError::WalCorruption {
                        offset: pos as u64,
                        reason: "Invalid seq_no format".into(),
                    })?,
            );
            let stored_checksum: [u8; 32] = entry_data
                .get(8..40)
                .ok_or(MemFuseError::WalCorruption {
                    offset: pos as u64,
                    reason: "Invalid checksum".into(),
                })?
                .try_into()
                .map_err(|_| MemFuseError::WalCorruption {
                    offset: pos as u64,
                    reason: "Invalid checksum format".into(),
                })?;
            let op_type = *entry_data.get(40).ok_or(MemFuseError::WalCorruption {
                offset: pos as u64,
                reason: "Invalid op_type".into(),
            })?;

            let remaining = entry_data.get(41..).ok_or(MemFuseError::WalCorruption {
                offset: pos as u64,
                reason: "Unexpected end of entry".into(),
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
                            .ok_or(MemFuseError::WalCorruption {
                                offset: pos as u64,
                                reason: "Invalid tx_id".into(),
                            })?
                            .try_into()
                            .map_err(|_| MemFuseError::WalCorruption {
                                offset: pos as u64,
                                reason: "Invalid tx_id format".into(),
                            })?,
                    ));
                    let key_len = u32::from_le_bytes(
                        remaining
                            .get(8..12)
                            .ok_or(MemFuseError::WalCorruption {
                                offset: pos as u64,
                                reason: "Invalid key_len".into(),
                            })?
                            .try_into()
                            .map_err(|_| MemFuseError::WalCorruption {
                                offset: pos as u64,
                                reason: "Invalid key_len format".into(),
                            })?,
                    ) as usize;
                    if remaining.len() < 12 + key_len + 4 {
                        continue;
                    }
                    let key = remaining
                        .get(12..12 + key_len)
                        .ok_or(MemFuseError::WalCorruption {
                            offset: pos as u64,
                            reason: "Invalid key data".into(),
                        })?
                        .to_vec();
                    let val_start = 12 + key_len;
                    let val_len = u32::from_le_bytes(
                        remaining
                            .get(val_start..val_start + 4)
                            .ok_or(MemFuseError::WalCorruption {
                                offset: pos as u64,
                                reason: "Invalid val_len".into(),
                            })?
                            .try_into()
                            .map_err(|_| MemFuseError::WalCorruption {
                                offset: pos as u64,
                                reason: "Invalid val_len format".into(),
                            })?,
                    ) as usize;
                    if remaining.len() < val_start + 4 + val_len {
                        continue;
                    }
                    let value = remaining
                        .get(val_start + 4..val_start + 4 + val_len)
                        .ok_or(MemFuseError::WalCorruption {
                            offset: pos as u64,
                            reason: "Invalid value data".into(),
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
                            .ok_or(MemFuseError::WalCorruption {
                                offset: pos as u64,
                                reason: "Invalid tx_id".into(),
                            })?
                            .try_into()
                            .map_err(|_| MemFuseError::WalCorruption {
                                offset: pos as u64,
                                reason: "Invalid tx_id format".into(),
                            })?,
                    ));
                    let key_len = u32::from_le_bytes(
                        remaining
                            .get(8..12)
                            .ok_or(MemFuseError::WalCorruption {
                                offset: pos as u64,
                                reason: "Invalid key_len".into(),
                            })?
                            .try_into()
                            .map_err(|_| MemFuseError::WalCorruption {
                                offset: pos as u64,
                                reason: "Invalid key_len format".into(),
                            })?,
                    ) as usize;
                    if remaining.len() < 12 + key_len {
                        continue;
                    }
                    let key = remaining
                        .get(12..12 + key_len)
                        .ok_or(MemFuseError::WalCorruption {
                            offset: pos as u64,
                            reason: "Invalid key data".into(),
                        })?
                        .to_vec();
                    WalOp::Delete { tx_id, key }
                }
                _ => continue,
            };

            // ANCHOR:ALG-FIX:D1-007 — HMAC-Verifikation bei WAL Replay
            // WP:WP-3.2 PRIO:1 NEEDS:NONE
            // AGENT:10 DATE:2026-05-15 STATUS:REVIEW
            // Ohne Verifikation werden korrupte Entries (Bit-Flip, Partial Write)
            // blind in die MemTable replayed → stille Datenkorrumpierung.
            let recomputed_checksum = WalEntry::compute_checksum(&op, seq_no, &integrity_key)?;
            if recomputed_checksum != stored_checksum {
                tracing::warn!(
                    "WAL entry at offset {} has invalid checksum, truncating replay",
                    pos
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
        let entry = WalEntry::try_new(op, 100, b"memfuse-integrity-key-v1\0\0\0\0\0\0\0\0")
            .expect("try_new");
        let bytes = entry.to_bytes();

        // Manual verification of length
        // total_len(4) + seq_no(8) + checksum(32) + op_type(1) + tx_id(8) + k_len(4) + key(3) + v_len(4) + val(5)
        // 4 + 8 + 32 + 1 + 8 + 4 + 3 + 4 + 5 = 69
        assert_eq!(bytes.len(), 69);

        let payload_len = u32::from_le_bytes(bytes[0..4].try_into().expect("valid slice"));
        assert_eq!(payload_len, 65);

        // Test with Delete
        let op2 = WalOp::Delete {
            tx_id: TxId::new(43),
            key: b"key2".to_vec(),
        };
        let entry2 = WalEntry::try_new(op2, 101, b"memfuse-integrity-key-v1\0\0\0\0\0\0\0\0")
            .expect("try_new");
        let bytes2 = entry2.to_bytes();
        // 4 + 8 + 32 + 1 + 8 + 4 + key(4) = 61
        assert_eq!(bytes2.len(), 61);
    }
}
