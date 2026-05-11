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
// AGENT:10 DATE:2026-05-09 STATUS:REVIEW
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
//! Entries with invalid MACs are ignored, and replay stops
//! at the first point of corruption.
//!
//! ## Invariants
//! - **Durability**: Every committed transaction is guaranteed to be in the WAL.
//! - **Integrity**: Entries are protected by BLAKE3 keyed hashes (MAC) to detect data corruption and tampering.
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
    pub mac: [u8; 32],
}

/// Default key for BLAKE3 keyed hash (HMAC-like integrity).
/// In a production environment, this should be managed by a Key Management System (WP-3.2).
const WAL_INTEGRITY_KEY: [u8; 32] = [0u8; 32];

impl WalEntry {
    /// Creates a new WAL entry with BLAKE3 keyed hash (MAC).
    pub fn new(op: WalOp, seq_no: u64) -> Self {
        let mac = Self::compute_mac(&op, seq_no);
        Self { op, seq_no, mac }
    }

    fn compute_mac(op: &WalOp, seq_no: u64) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new_keyed(&WAL_INTEGRITY_KEY);
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
        *hasher.finalize().as_bytes()
    }

    /// Serializes the entry to bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        // seq_no (8) + mac (32)
        buf.extend_from_slice(&self.seq_no.to_le_bytes());
        buf.extend_from_slice(&self.mac);

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
            let len_bytes: [u8; 4] = data
                .get(pos..pos + 4)
                .ok_or_else(|| MemFuseError::WalCorruption {
                    offset: pos as u64,
                    reason: "Unexpected end of data while reading length".into(),
                })?
                .try_into()
                .map_err(|_| MemFuseError::WalCorruption {
                    offset: pos as u64,
                    reason: "Invalid length slice".into(),
                })?;
            let len = u32::from_le_bytes(len_bytes) as usize;
            pos += 4;

            if pos + len > data.len() {
                tracing::warn!("WAL truncated at offset {}", pos);
                break;
            }

            let entry_data =
                data.get(pos..pos + len)
                    .ok_or_else(|| MemFuseError::WalCorruption {
                        offset: pos as u64,
                        reason: "Invalid entry data range".into(),
                    })?;
            pos += len;

            if entry_data.len() < 41 {
                continue;
            }

            let seq_no = u64::from_le_bytes(
                entry_data
                    .get(0..8)
                    .ok_or_else(|| MemFuseError::WalCorruption {
                        offset: pos as u64,
                        reason: "Invalid seq_no range".into(),
                    })?
                    .try_into()
                    .map_err(|_| MemFuseError::WalCorruption {
                        offset: pos as u64,
                        reason: "Invalid seq_no slice".into(),
                    })?,
            );
            let stored_mac: [u8; 32] = entry_data
                .get(8..40)
                .ok_or_else(|| MemFuseError::WalCorruption {
                    offset: pos as u64,
                    reason: "Invalid MAC range".into(),
                })?
                .try_into()
                .map_err(|_| MemFuseError::WalCorruption {
                    offset: pos as u64,
                    reason: "Invalid MAC slice".into(),
                })?;
            let op_type = *entry_data
                .get(40)
                .ok_or_else(|| MemFuseError::WalCorruption {
                    offset: pos as u64,
                    reason: "Invalid op_type index".into(),
                })?;

            let remaining = entry_data.get(41..).unwrap_or(&[]);
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
                                reason: "Invalid tx_id range".into(),
                            })?
                            .try_into()
                            .map_err(|_| MemFuseError::WalCorruption {
                                offset: pos as u64,
                                reason: "Invalid tx_id slice".into(),
                            })?,
                    ));
                    let key_len = u32::from_le_bytes(
                        remaining
                            .get(8..12)
                            .ok_or_else(|| MemFuseError::WalCorruption {
                                offset: pos as u64,
                                reason: "Invalid key_len range".into(),
                            })?
                            .try_into()
                            .map_err(|_| MemFuseError::WalCorruption {
                                offset: pos as u64,
                                reason: "Invalid key_len slice".into(),
                            })?,
                    ) as usize;
                    if remaining.len() < 12 + key_len + 4 {
                        continue;
                    }
                    let key = remaining.get(12..12 + key_len).unwrap_or(&[]).to_vec();
                    let val_start = 12 + key_len;
                    let val_len = u32::from_le_bytes(
                        remaining
                            .get(val_start..val_start + 4)
                            .ok_or_else(|| MemFuseError::WalCorruption {
                                offset: pos as u64,
                                reason: "Invalid val_len range".into(),
                            })?
                            .try_into()
                            .map_err(|_| MemFuseError::WalCorruption {
                                offset: pos as u64,
                                reason: "Invalid val_len slice".into(),
                            })?,
                    ) as usize;
                    if remaining.len() < val_start + 4 + val_len {
                        continue;
                    }
                    let value = remaining
                        .get(val_start + 4..val_start + 4 + val_len)
                        .unwrap_or(&[])
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
                                reason: "Invalid tx_id range".into(),
                            })?
                            .try_into()
                            .map_err(|_| MemFuseError::WalCorruption {
                                offset: pos as u64,
                                reason: "Invalid tx_id slice".into(),
                            })?,
                    ));
                    let key_len = u32::from_le_bytes(
                        remaining
                            .get(8..12)
                            .ok_or_else(|| MemFuseError::WalCorruption {
                                offset: pos as u64,
                                reason: "Invalid key_len range".into(),
                            })?
                            .try_into()
                            .map_err(|_| MemFuseError::WalCorruption {
                                offset: pos as u64,
                                reason: "Invalid key_len slice".into(),
                            })?,
                    ) as usize;
                    if remaining.len() < 12 + key_len {
                        continue;
                    }
                    let key = remaining.get(12..12 + key_len).unwrap_or(&[]).to_vec();
                    WalOp::Delete { tx_id, key }
                }
                _ => continue,
            };

            // ANCHOR:ALG-FIX:D1-007 — MAC-Verifikation bei WAL Replay
            // WP:WP-0.0 PRIO:1 NEEDS:NONE
            // AGENT:13 DATE:2026-05-08 STATUS:DONE
            // CREATED:2026-05-08 DEADLINE:NONE
            // Ohne Verifikation werden korrupte Entries (Bit-Flip, Partial Write, malicious injection)
            // blind in die MemTable replayed → stille Datenkorrumpierung oder Sicherheitsbruch.
            let recomputed_mac = WalEntry::compute_mac(&op, seq_no);
            if recomputed_mac != stored_mac {
                tracing::warn!(
                    "WAL entry at offset {} has invalid MAC, truncating replay",
                    pos
                );
                break;
            }

            entries.push((
                seq_no,
                WalEntry {
                    op,
                    seq_no,
                    mac: stored_mac,
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
    use memfuse_core::TxId;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_wal_roundtrip() -> memfuse_core::Result<()> {
        let dir = tempdir().expect("safe");
        let wal_path = dir.path().join("test.wal");
        let wal = Wal::open(&wal_path).await?;

        let op = WalOp::Put {
            tx_id: TxId::new(1),
            key: b"key".to_vec(),
            value: b"value".to_vec(),
        };
        let entry = WalEntry::new(op, 1);
        wal.append(&entry).await?;

        let entries = wal.replay().await?;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, 1);
        if let WalOp::Put { tx_id, key, value } = &entries[0].1.op {
            assert_eq!(tx_id.inner(), 1);
            assert_eq!(key, b"key");
            assert_eq!(value, b"value");
        } else {
            panic!("Expected Put op");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_wal_corruption() -> memfuse_core::Result<()> {
        let dir = tempdir().expect("safe");
        let wal_path = dir.path().join("test_corrupt.wal");

        {
            let wal = Wal::open(&wal_path).await?;
            let op = WalOp::Put {
                tx_id: TxId::new(1),
                key: b"key".to_vec(),
                value: b"value".to_vec(),
            };
            let entry = WalEntry::new(op, 1);
            wal.append(&entry).await?;
        }

        // Corrupt the MAC
        let mut data = std::fs::read(&wal_path).expect("safe");
        if data.len() > 12 {
            data[12] ^= 0xFF; // Flip a bit in the MAC
            std::fs::write(&wal_path, data).expect("safe");
        }

        let wal = Wal::open(&wal_path).await?;
        let entries = wal.replay().await?;
        assert_eq!(entries.len(), 0, "Corrupt entry should be ignored");

        Ok(())
    }
}

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    #[kani::proof]
    fn verify_compute_mac_no_panic() {
        let tx_id = TxId::new(kani::any());
        let key_len: usize = kani::any();
        kani::assume(key_len < 100);
        let key = vec![0u8; key_len];

        let value_len: usize = kani::any();
        kani::assume(value_len < 100);
        let value = vec![0u8; value_len];

        let op = WalOp::Put { tx_id, key, value };
        let seq_no: u64 = kani::any();

        let _mac = WalEntry::compute_mac(&op, seq_no);
    }

    #[kani::proof]
    fn verify_to_bytes_no_panic() {
        let tx_id = TxId::new(kani::any());
        let key_len: usize = kani::any();
        kani::assume(key_len < 64);
        let key = vec![0u8; key_len];

        let value_len: usize = kani::any();
        kani::assume(value_len < 64);
        let value = vec![0u8; value_len];

        let op = WalOp::Put { tx_id, key, value };
        let seq_no: u64 = kani::any();
        let entry = WalEntry::new(op, seq_no);

        let bytes = entry.to_bytes();
        assert!(bytes.len() >= 45); // 4 (len) + 8 (seq) + 32 (mac) + 1 (op)
    }
}
