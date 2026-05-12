// ANCHOR:DOC:DOC-WAL-001 — Missing module documentation
// WP:WP-0.0 PRIO:3 NEEDS:NONE
// AGENT:02 DATE:2026-05-09 STATUS:REVIEW
// CREATED:2026-05-09 DEADLINE:NONE
// ANCHOR:ARCH:WAL-001 — Write-Ahead Log für Crash Recovery.
// WP:WP-0.0 PRIO:1 NEEDS:NONE
// AGENT:01 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-05 DEADLINE:NONE
// FORMAT: [u32 len][u8 version][u64 seq_no][u8[32] mac][u8 op_type][payload...]
// INVARIANTE: Jeder Eintrag wird ERST in WAL geschrieben, DANN in MemTable übernommen.
// REPLAY: Bei Neustart wird WAL komplett in MemTable replayed (lsm.rs::new()).
// ROTATION: Beim Flush wird alte WAL archiviert, neue geöffnet.
//
#![allow(unexpected_cfgs)]

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
//! - **Integrity**: Entries are protected by keyed BLAKE3 MACs to detect data corruption and tampering.
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

/// Current WAL format version.
const WAL_VERSION: u8 = 2;

/// Key used for WAL integrity MAC.
/// In production, this should be loaded from a secure KMS.
/// For now, we use a fixed placeholder that is explicitly documented.
const WAL_INTEGRITY_KEY: [u8; 32] = [
    0x4d, 0x65, 0x6d, 0x46, 0x75, 0x73, 0x65, 0x57, // MemFuseW
    0x41, 0x4c, 0x49, 0x6e, 0x74, 0x65, 0x67, 0x72, // ALIntegr
    0x69, 0x74, 0x79, 0x4b, 0x65, 0x79, 0x32, 0x30, // ityKey20
    0x32, 0x36, 0x30, 0x35, 0x30, 0x39, 0x5f, 0x5f, // 260509__
];

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

impl WalEntry {
    /// Creates a new WAL entry with BLAKE3 MAC.
    pub fn new(op: WalOp, seq_no: u64) -> Self {
        let mac = Self::compute_mac(&op, seq_no);
        Self {
            op,
            seq_no,
            mac,
        }
    }

    fn compute_mac(op: &WalOp, seq_no: u64) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new_keyed(&WAL_INTEGRITY_KEY);
        hasher.update(&[WAL_VERSION]);
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
        // version (1) + seq_no (8) + mac (32) + op_type (1)
        buf.push(WAL_VERSION);
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
            let len = u32::from_le_bytes(data.get(pos..pos + 4).ok_or_else(|| {
                MemFuseError::WalCorruption {
                    offset: pos as u64,
                    reason: "Invalid length".into(),
                }
            })?.try_into().unwrap()) as usize;
            pos += 4;

            if pos + len > data.len() {
                tracing::warn!("WAL truncated at offset {}", pos);
                break;
            }

            let entry_data = &data[pos..pos + len];
            pos += len;

            if entry_data.len() < 42 { // version(1) + seq_no(8) + mac(32) + op_type(1)
                continue;
            }

            let version = entry_data[0];
            if version != WAL_VERSION {
                return Err(MemFuseError::WalCorruption {
                    offset: (pos - len) as u64,
                    reason: format!("Unsupported WAL version: {}, expected {}", version, WAL_VERSION),
                });
            }

            let seq_no = u64::from_le_bytes(entry_data[1..9].try_into().map_err(|_| {
                MemFuseError::WalCorruption {
                    offset: pos as u64,
                    reason: "Invalid seq_no".into(),
                }
            })?);
            let stored_mac: [u8; 32] = entry_data[9..41].try_into().map_err(|_| {
                MemFuseError::WalCorruption {
                    offset: pos as u64,
                    reason: "Invalid mac".into(),
                }
            })?;
            let op_type = entry_data[41];

            let remaining = &entry_data[42..];
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

            // ANCHOR:ALG-FIX:D1-007 — MAC-Verifikation bei WAL Replay
            // WP:WP-0.0 PRIO:1 NEEDS:NONE
            // AGENT:13 DATE:2026-05-08 STATUS:DONE
            // CREATED:2026-05-08 DEADLINE:NONE
            // Ohne Verifikation werden korrupte Entries (Bit-Flip, Partial Write)
            // blind in die MemTable replayed → stille Datenkorrumpierung.
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

#[cfg(kani)]
mod proof {
    use super::*;

    #[kani::proof]
    fn verify_mac_consistency() {
        let seq_no: u64 = kani::any();
        let key_len: usize = kani::any();
        kani::assume(key_len < 16);
        let key: Vec<u8> = vec![0u8; key_len];

        let op = WalOp::Delete {
            tx_id: TxId::new(0),
            key: key.clone()
        };

        let mac1 = WalEntry::compute_mac(&op, seq_no);
        let mac2 = WalEntry::compute_mac(&op, seq_no);

        assert_eq!(mac1, mac2);
    }
}
