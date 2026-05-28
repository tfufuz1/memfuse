//! Write-Ahead Log (WAL) for durability and crash recovery with HMAC chaining.

use memfuse_core::{MemFuseError, Result, TxId};
use memfuse_crypto::crypto::KeyManager;
use memfuse_crypto::wal_crypto::WalHmac;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// WAL entry operation.
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

impl WalOp {
    pub fn tx_id(&self) -> TxId {
        match self {
            WalOp::Put { tx_id, .. } => *tx_id,
            WalOp::Delete { tx_id, .. } => *tx_id,
        }
    }
}

/// A single entry in the Write-Ahead Log.
#[derive(Debug, Clone)]
pub struct WalEntry {
    /// The operation performed.
    pub op: WalOp,
    /// Sequence number assigned to the operation.
    pub seq_no: u64,
    /// HMAC of the current entry (includes previous HMAC).
    pub checksum: [u8; 32],
    /// HMAC of the previous entry (the chain link).
    pub prev_hmac: [u8; 32],
}

impl WalEntry {
    pub fn tx_id(&self) -> TxId {
        self.op.tx_id()
    }
}

impl WalEntry {
    /// Creates a new WAL entry with HMAC-SHA256 checksum and chaining.
    pub fn try_new(
        op: WalOp,
        seq_no: u64,
        integrity_key: &[u8],
        prev_hmac: [u8; 32],
    ) -> Result<Self> {
        let checksum = Self::compute_checksum(&op, seq_no, integrity_key, prev_hmac)?;
        Ok(Self {
            op,
            seq_no,
            checksum,
            prev_hmac,
        })
    }

    pub fn compute_checksum(
        op: &WalOp,
        seq_no: u64,
        integrity_key: &[u8],
        prev_hmac: [u8; 32],
    ) -> Result<[u8; 32]> {
        let mut mac = WalHmac::new(integrity_key)?;

        // Hash Chaining: binding to the previous entry
        mac.update(&prev_hmac);

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
        Ok(mac.finalize())
    }

    /// Serializes the entry to bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let op_size = match &self.op {
            WalOp::Put { key, value, .. } => 1 + 8 + 4 + key.len() + 4 + value.len(),
            WalOp::Delete { key, .. } => 1 + 8 + 4 + key.len(),
        };

        // total_payload = CRC32(4) + seq_no(8) + checksum(32) + prev_hmac(32) + op
        let payload_size = 8 + 32 + 32 + op_size;
        let total_payload_size = 4 + payload_size; // CRC32 + payload
        let total_size = 4 + total_payload_size; // length prefix + total_payload

        let mut buf = Vec::with_capacity(total_size);
        buf.extend_from_slice(&(total_payload_size as u32).to_le_bytes());

        // Prepare payload to compute CRC
        let mut payload = Vec::with_capacity(payload_size);
        payload.extend_from_slice(&self.seq_no.to_le_bytes());
        payload.extend_from_slice(&self.checksum);
        payload.extend_from_slice(&self.prev_hmac);

        match &self.op {
            WalOp::Put { tx_id, key, value } => {
                payload.push(0u8);
                payload.extend_from_slice(&tx_id.inner().to_le_bytes());
                payload.extend_from_slice(&(key.len() as u32).to_le_bytes());
                payload.extend_from_slice(key);
                payload.extend_from_slice(&(value.len() as u32).to_le_bytes());
                payload.extend_from_slice(value);
            }
            WalOp::Delete { tx_id, key } => {
                payload.push(1u8);
                payload.extend_from_slice(&tx_id.inner().to_le_bytes());
                payload.extend_from_slice(&(key.len() as u32).to_le_bytes());
                payload.extend_from_slice(key);
            }
        }

        let crc = crc32fast::hash(&payload);
        buf.extend_from_slice(&crc.to_le_bytes());
        buf.extend_from_slice(&payload);
        buf
    }
}

/// Write-Ahead Log for crash recovery.
pub struct Wal {
    path: PathBuf,
    file: tokio::sync::Mutex<tokio::fs::File>,
    size: std::sync::atomic::AtomicU64,
    key_manager: Option<Arc<KeyManager>>,
    /// Last HMAC written to the log, used for hash-chaining.
    last_hmac: tokio::sync::Mutex<[u8; 32]>,
}

impl std::fmt::Debug for Wal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Wal")
            .field("path", &self.path)
            .field("size", &self.size())
            .finish()
    }
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

        let wal = Self {
            path: path.clone(),
            size: std::sync::atomic::AtomicU64::new(metadata.len()),
            file: tokio::sync::Mutex::new(file),
            key_manager,
            last_hmac: tokio::sync::Mutex::new([0u8; 32]),
        };

        // If file is not empty, find the last valid HMAC to continue the chain
        if metadata.len() > 0 {
            let entries = wal.replay_with_size(metadata.len()).await?;
            if let Some((_, last_entry, _)) = entries.last() {
                let mut guard = wal.last_hmac.lock().await;
                *guard = last_entry.checksum;
            }
        }

        Ok(wal)
    }

    /// Appends an entry to the WAL.
    // TODO(FIND-STO-001): WAL CRC fehlend. (WP-1.1)
    // Add CRC32c or HMAC tagging to prevent stealth corruption during crash recoveries.
    pub async fn append(&self, entry: &WalEntry) -> Result<()> {
        self.append_batch(std::slice::from_ref(entry)).await
    }

    /// Appends a batch of entries to the WAL and performs a single fsync.
    pub async fn append_batch(&self, entries: &[WalEntry]) -> Result<()> {
        if entries.is_empty() {
            return Ok(());
        }

        let mut total_bytes = Vec::new();
        let mut last_hmac_val = [0u8; 32];

        for entry in entries {
            let mut bytes = entry.to_bytes();

            if let Some(km) = &self.key_manager {
                if bytes.len() > 4 {
                    let payload = &bytes[4..];
                    let offset = self.size() + total_bytes.len() as u64;
                    let encrypted = km.encrypt(payload, offset)?;
                    let mut new_bytes = Vec::with_capacity(4 + encrypted.len());
                    new_bytes.extend_from_slice(&(encrypted.len() as u32).to_le_bytes());
                    new_bytes.extend_from_slice(&encrypted);
                    bytes = new_bytes;
                }
            }
            total_bytes.extend_from_slice(&bytes);
            last_hmac_val = entry.checksum;
        }

        let mut file = self.file.lock().await;
        file.write_all(&total_bytes)
            .await
            .map_err(|e| MemFuseError::Storage(format!("WAL batch write failed: {}", e)))?;
        file.flush()
            .await
            .map_err(|e| MemFuseError::Storage(format!("WAL batch flush failed: {}", e)))?;
        file.sync_data()
            .await
            .map_err(|e| MemFuseError::Storage(format!("WAL batch fsync failed: {}", e)))?;

        self.size.fetch_add(
            total_bytes.len() as u64,
            std::sync::atomic::Ordering::SeqCst,
        );

        let mut last_hmac = self.last_hmac.lock().await;
        *last_hmac = last_hmac_val;

        Ok(())
    }

    /// Helper for creating entries bound to this WAL's current chain.
    pub async fn create_entry(&self, op: WalOp, seq_no: u64) -> Result<WalEntry> {
        let last_hmac = self.last_hmac.lock().await;
        let integrity_key = if let Some(km) = &self.key_manager {
            km.integrity_key()?
        } else {
            *b"memfuse-integrity-key-v1\0\0\0\0\0\0\0\0"
        };
        WalEntry::try_new(op, seq_no, &integrity_key, *last_hmac)
    }

    /// Replays the WAL, returning all valid entries with their sequence numbers and end offsets.
    pub async fn replay(&self) -> Result<Vec<(u64, WalEntry, u64)>> {
        let metadata = tokio::fs::metadata(&self.path)
            .await
            .map_err(|e| MemFuseError::Storage(e.to_string()))?;
        self.replay_with_size(metadata.len()).await
    }

    async fn replay_with_size(&self, file_size: u64) -> Result<Vec<(u64, WalEntry, u64)>> {
        let mut file = self.file.lock().await;
        use tokio::io::AsyncSeekExt;
        file.seek(std::io::SeekFrom::Start(0))
            .await
            .map_err(|e| MemFuseError::Storage(format!("WAL replay seek failed: {}", e)))?;

        let mut reader = tokio::io::BufReader::new(&mut *file);

        let mut entries = Vec::new();
        let mut pos = 0u64;
        let mut current_chain_hmac = [0u8; 32];

        let integrity_key = if let Some(km) = &self.key_manager {
            km.integrity_key()?
        } else {
            *b"memfuse-integrity-key-v1\0\0\0\0\0\0\0\0"
        };

        loop {
            let mut len_bytes = [0u8; 4];
            match reader.read_exact(&mut len_bytes).await {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(MemFuseError::Storage(format!("WAL read failed: {}", e))),
            };
            let len = u32::from_le_bytes(len_bytes) as usize;

            if len > 128 * 1024 * 1024 {
                tracing::warn!("WAL entry too large ({} bytes) at offset {}", len, pos);
                break;
            }

            let mut entry_data_raw = vec![0u8; len];
            match reader.read_exact(&mut entry_data_raw).await {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    tracing::warn!("WAL truncated at offset {}", pos);
                    break;
                }
                Err(e) => return Err(MemFuseError::Storage(format!("WAL read failed: {}", e))),
            };

            let entry_start_pos = pos;
            pos += (4 + len) as u64;

            let decrypted_data;
            let entry_data = if let Some(km) = &self.key_manager {
                decrypted_data = km.decrypt(&entry_data_raw, entry_start_pos)?;
                &decrypted_data
            } else {
                &entry_data_raw
            };

            if entry_data.len() < 4 + 73 {
                tracing::warn!("WAL entry too small at offset {}", entry_start_pos);
                if pos >= file_size {
                    break;
                } else {
                    return Err(MemFuseError::WalCorruption {
                        offset: entry_start_pos,
                        reason: "Entry too small".into(),
                    });
                }
            }

            let stored_crc = u32::from_le_bytes(entry_data[0..4].try_into().map_err(|_| {
                MemFuseError::WalCorruption {
                    offset: entry_start_pos,
                    reason: "Invalid CRC format".into(),
                }
            })?);
            let payload = &entry_data[4..];
            let computed_crc = crc32fast::hash(payload);

            if stored_crc != computed_crc {
                if pos >= file_size {
                    tracing::warn!(
                        "WAL CRC mismatch at tail (offset {}), truncating",
                        entry_start_pos
                    );
                    break;
                } else {
                    return Err(MemFuseError::WalCorruption {
                        offset: entry_start_pos,
                        reason: format!(
                            "CRC mismatch in middle of WAL: stored={}, computed={}",
                            stored_crc, computed_crc
                        ),
                    });
                }
            }

            let seq_no = u64::from_le_bytes(
                payload
                    .get(0..8)
                    .ok_or(MemFuseError::WalCorruption {
                        offset: entry_start_pos,
                        reason: "Invalid seq_no".into(),
                    })?
                    .try_into()
                    .map_err(|_| MemFuseError::WalCorruption {
                        offset: entry_start_pos,
                        reason: "Invalid seq_no format".into(),
                    })?,
            );
            let stored_checksum: [u8; 32] = payload
                .get(8..40)
                .ok_or(MemFuseError::WalCorruption {
                    offset: entry_start_pos,
                    reason: "Invalid checksum".into(),
                })?
                .try_into()
                .map_err(|_| MemFuseError::WalCorruption {
                    offset: entry_start_pos,
                    reason: "Invalid checksum format".into(),
                })?;
            let prev_hmac: [u8; 32] = payload
                .get(40..72)
                .ok_or(MemFuseError::WalCorruption {
                    offset: entry_start_pos,
                    reason: "Invalid prev_hmac".into(),
                })?
                .try_into()
                .map_err(|_| MemFuseError::WalCorruption {
                    offset: entry_start_pos,
                    reason: "Invalid prev_hmac format".into(),
                })?;
            let op_type = *payload.get(72).ok_or(MemFuseError::WalCorruption {
                offset: entry_start_pos,
                reason: "Invalid op_type".into(),
            })?;

            let remaining = payload.get(73..).ok_or(MemFuseError::WalCorruption {
                offset: entry_start_pos,
                reason: "Unexpected end of entry".into(),
            })?;
            let op = match op_type {
                0 => {
                    if remaining.len() < 12 {
                        if pos >= file_size {
                            break;
                        } else {
                            continue;
                        }
                    }
                    let tx_id = TxId::new(u64::from_le_bytes(
                        remaining
                            .get(0..8)
                            .ok_or(MemFuseError::WalCorruption {
                                offset: entry_start_pos,
                                reason: "Invalid tx_id".into(),
                            })?
                            .try_into()
                            .map_err(|_| MemFuseError::WalCorruption {
                                offset: entry_start_pos,
                                reason: "Invalid tx_id format".into(),
                            })?,
                    ));
                    let key_len = u32::from_le_bytes(
                        remaining
                            .get(8..12)
                            .ok_or(MemFuseError::WalCorruption {
                                offset: entry_start_pos,
                                reason: "Invalid key_len".into(),
                            })?
                            .try_into()
                            .map_err(|_| MemFuseError::WalCorruption {
                                offset: entry_start_pos,
                                reason: "Invalid key_len format".into(),
                            })?,
                    ) as usize;
                    if remaining.len() < 12 + key_len + 4 {
                        if pos >= file_size {
                            break;
                        } else {
                            continue;
                        }
                    }
                    let key = remaining
                        .get(12..12 + key_len)
                        .ok_or(MemFuseError::WalCorruption {
                            offset: entry_start_pos,
                            reason: "Invalid key data".into(),
                        })?
                        .to_vec();
                    let val_start = 12 + key_len;
                    let val_len = u32::from_le_bytes(
                        remaining
                            .get(val_start..val_start + 4)
                            .ok_or(MemFuseError::WalCorruption {
                                offset: entry_start_pos,
                                reason: "Invalid val_len".into(),
                            })?
                            .try_into()
                            .map_err(|_| MemFuseError::WalCorruption {
                                offset: entry_start_pos,
                                reason: "Invalid val_len format".into(),
                            })?,
                    ) as usize;
                    if remaining.len() < val_start + 4 + val_len {
                        if pos >= file_size {
                            break;
                        } else {
                            continue;
                        }
                    }
                    let value = remaining
                        .get(val_start + 4..val_start + 4 + val_len)
                        .ok_or(MemFuseError::WalCorruption {
                            offset: entry_start_pos,
                            reason: "Invalid value data".into(),
                        })?
                        .to_vec();
                    WalOp::Put { tx_id, key, value }
                }
                1 => {
                    if remaining.len() < 12 {
                        if pos >= file_size {
                            break;
                        } else {
                            continue;
                        }
                    }
                    let tx_id = TxId::new(u64::from_le_bytes(
                        remaining
                            .get(0..8)
                            .ok_or(MemFuseError::WalCorruption {
                                offset: entry_start_pos,
                                reason: "Invalid tx_id".into(),
                            })?
                            .try_into()
                            .map_err(|_| MemFuseError::WalCorruption {
                                offset: entry_start_pos,
                                reason: "Invalid tx_id format".into(),
                            })?,
                    ));
                    let key_len = u32::from_le_bytes(
                        remaining
                            .get(8..12)
                            .ok_or(MemFuseError::WalCorruption {
                                offset: entry_start_pos,
                                reason: "Invalid key_len".into(),
                            })?
                            .try_into()
                            .map_err(|_| MemFuseError::WalCorruption {
                                offset: entry_start_pos,
                                reason: "Invalid key_len format".into(),
                            })?,
                    ) as usize;
                    if remaining.len() < 12 + key_len {
                        if pos >= file_size {
                            break;
                        } else {
                            continue;
                        }
                    }
                    let key = remaining
                        .get(12..12 + key_len)
                        .ok_or(MemFuseError::WalCorruption {
                            offset: entry_start_pos,
                            reason: "Invalid key data".into(),
                        })?
                        .to_vec();
                    WalOp::Delete { tx_id, key }
                }
                _ => {
                    if pos >= file_size {
                        break;
                    } else {
                        continue;
                    }
                }
            };

            let recomputed_checksum =
                WalEntry::compute_checksum(&op, seq_no, &integrity_key, prev_hmac)?;
            if recomputed_checksum != stored_checksum || prev_hmac != current_chain_hmac {
                tracing::warn!(
                    "WAL entry at offset {} has invalid checksum or broken chain, truncating replay",
                    entry_start_pos
                );
                break;
            }
            current_chain_hmac = stored_checksum;

            entries.push((
                seq_no,
                WalEntry {
                    op,
                    seq_no,
                    checksum: stored_checksum,
                    prev_hmac,
                },
                pos,
            ));
        }

        Ok(entries)
    }

    pub fn size(&self) -> u64 {
        self.size.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Physically truncates the WAL file to the specified offset.
    /// This also updates the in-memory size and the HMAC chain link.
    pub async fn truncate(&self, offset: u64, new_last_hmac: [u8; 32]) -> Result<()> {
        let mut file = self.file.lock().await;

        // 1. Physically truncate the file
        file.set_len(offset)
            .await
            .map_err(|e| MemFuseError::Storage(format!("WAL truncate failed: {}", e)))?;

        // 2. Ensure we seek to the new end
        use tokio::io::AsyncSeekExt;
        file.seek(std::io::SeekFrom::Start(offset))
            .await
            .map_err(|e| MemFuseError::Storage(format!("WAL seek after truncate failed: {}", e)))?;

        // 3. Update in-memory size
        self.size.store(offset, std::sync::atomic::Ordering::SeqCst);

        // 4. Update last_hmac
        let mut last_hmac_guard = self.last_hmac.lock().await;
        *last_hmac_guard = new_last_hmac;

        Ok(())
    }

    /// Returns a snapshot of the last HMAC written to the log.
    pub async fn last_hmac_snapshot(&self) -> [u8; 32] {
        *self.last_hmac.lock().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use tokio::fs;

    #[test]
    fn test_wal_entry_serialization_roundtrip() {
        let op = WalOp::Put {
            tx_id: TxId::new(42),
            key: b"key".to_vec(),
            value: b"value".to_vec(),
        };
        let entry = WalEntry::try_new(
            op,
            100,
            b"memfuse-integrity-key-v1\0\0\0\0\0\0\0\0",
            [0u8; 32],
        )
        .expect("try_new"); // unwrap allowed (AGENT:08)
        let bytes = entry.to_bytes();

        assert_eq!(bytes.len(), 105);
        let payload_len = u32::from_le_bytes(bytes[0..4].try_into().expect("valid slice")); // unwrap allowed (AGENT:08)
        assert_eq!(payload_len, 101);
    }

    #[tokio::test]
    async fn test_wal_append_and_replay_valid() {
        let dir = tempdir().expect("tempdir"); // unwrap allowed (AGENT:08)
        let wal_path = dir.path().join("test_wal.log");

        {
            let wal = Wal::open(&wal_path).await.expect("open WAL"); // unwrap allowed (AGENT:08)
            let op1 = WalOp::Put {
                tx_id: TxId::new(1),
                key: b"user:1".to_vec(),
                value: b"Alice".to_vec(),
            };
            let entry1 = wal.create_entry(op1, 10).await.expect("valid"); // unwrap allowed (AGENT:08)
            wal.append(&entry1).await.expect("append 1"); // unwrap allowed (AGENT:08)

            let op2 = WalOp::Delete {
                tx_id: TxId::new(2),
                key: b"user:1".to_vec(),
            };
            let entry2 = wal.create_entry(op2, 11).await.expect("valid"); // unwrap allowed (AGENT:08)
            wal.append(&entry2).await.expect("append 2"); // unwrap allowed (AGENT:08)
        }

        let wal2 = Wal::open(&wal_path).await.expect("reopen WAL"); // unwrap allowed (AGENT:08)
        let entries = wal2.replay().await.expect("replay"); // unwrap allowed (AGENT:08)

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].1.prev_hmac, entries[0].1.checksum);
    }

    #[tokio::test]
    async fn test_wal_hash_chain_verification() {
        let dir = tempdir().expect("tempdir"); // unwrap allowed (AGENT:08)
        let wal_path = dir.path().join("chain_wal.log");

        {
            let wal = Wal::open(&wal_path).await.expect("open"); // unwrap allowed (AGENT:08)
            let op1 = WalOp::Put {
                tx_id: TxId::new(1),
                key: b"k1".to_vec(),
                value: b"v1".to_vec(),
            };
            let entry1 = wal.create_entry(op1, 1).await.expect("entry1"); // unwrap allowed (AGENT:08)
            wal.append(&entry1).await.expect("append1"); // unwrap allowed (AGENT:08)

            let op2 = WalOp::Put {
                tx_id: TxId::new(2),
                key: b"k2".to_vec(),
                value: b"v2".to_vec(),
            };
            let entry2 = wal.create_entry(op2, 2).await.expect("entry2"); // unwrap allowed (AGENT:08)
            wal.append(&entry2).await.expect("append2"); // unwrap allowed (AGENT:08)
        }

        {
            let mut data = fs::read(&wal_path).await.expect("read"); // unwrap allowed (AGENT:08)
            data[12] ^= 0xFF;
            fs::write(&wal_path, data).await.expect("write"); // unwrap allowed (AGENT:08)
        }

        let result = Wal::open(&wal_path).await;
        assert!(matches!(result, Err(MemFuseError::WalCorruption { .. })));
    }
    #[tokio::test]
    async fn test_wal_replay_truncation() {
        let dir = tempdir().expect("tempdir"); // unwrap allowed (AGENT:08)
        let wal_path = dir.path().join("trunc_wal.log");

        {
            let wal = Wal::open(&wal_path).await.expect("open"); // unwrap allowed (AGENT:08)
            for i in 0..5 {
                let op = WalOp::Put {
                    tx_id: TxId::new(i),
                    key: b"key".to_vec(),
                    value: b"val".to_vec(),
                };
                let entry = wal.create_entry(op, i).await.expect("entry"); // unwrap allowed (AGENT:08)
                wal.append(&entry).await.expect("append"); // unwrap allowed (AGENT:08)
            }
        }

        // Truncate the file in the middle of the last entry
        let mut data = fs::read(&wal_path).await.expect("read"); // unwrap allowed (AGENT:08)
        let new_size = data.len() - 10; // Chop off 10 bytes from the last entry
        data.truncate(new_size);
        fs::write(&wal_path, data).await.expect("write"); // unwrap allowed (AGENT:08)

        let wal2 = Wal::open(&wal_path).await.expect("open"); // unwrap allowed (AGENT:08)
        let entries = wal2.replay().await.expect("replay"); // unwrap allowed (AGENT:08)
        assert_eq!(entries.len(), 4);
    }

    #[tokio::test]
    async fn test_wal_crc_middle_corruption() {
        let dir = tempdir().expect("tempdir"); // unwrap allowed (AGENT:08)
        let wal_path = dir.path().join("middle_corrupt.log");

        {
            let wal = Wal::open(&wal_path).await.expect("open"); // unwrap allowed (AGENT:08)
            for i in 0..3 {
                let op = WalOp::Put {
                    tx_id: TxId::new(i),
                    key: format!("k{}", i).into_bytes(),
                    value: format!("v{}", i).into_bytes(),
                };
                let entry = wal.create_entry(op, i).await.expect("entry"); // unwrap allowed (AGENT:08)
                wal.append(&entry).await.expect("append"); // unwrap allowed (AGENT:08)
            }
        }

        {
            let mut data = fs::read(&wal_path).await.expect("read"); // unwrap allowed (AGENT:08)
                                                                     // Corrupt the second entry (somewhere in the middle of the file)
                                                                     // Each entry is ~100 bytes. Let's flip a bit around offset 150.
            if data.len() > 150 {
                data[150] ^= 0xFF;
                fs::write(&wal_path, data).await.expect("write"); // unwrap allowed (AGENT:08)
            }
        }

        let result = Wal::open(&wal_path).await;

        // Should fail because corruption is in the middle (before the last entry)
        assert!(
            matches!(result, Err(MemFuseError::WalCorruption { .. })),
            "Expected WalCorruption error, got {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_wal_crc_tail_corruption() {
        let dir = tempdir().expect("tempdir"); // unwrap allowed (AGENT:08)
        let wal_path = dir.path().join("tail_corrupt.log");

        {
            let wal = Wal::open(&wal_path).await.expect("open"); // unwrap allowed (AGENT:08)
            for i in 0..2 {
                let op = WalOp::Put {
                    tx_id: TxId::new(i),
                    key: format!("k{}", i).into_bytes(),
                    value: format!("v{}", i).into_bytes(),
                };
                let entry = wal.create_entry(op, i).await.expect("entry"); // unwrap allowed (AGENT:08)
                wal.append(&entry).await.expect("append"); // unwrap allowed (AGENT:08)
            }
        }

        {
            let mut file = fs::OpenOptions::new()
                .append(true)
                .open(&wal_path)
                .await
                .expect("open"); // unwrap allowed (AGENT:08)
            use tokio::io::AsyncWriteExt;
            // Append some garbage that doesn't form a valid entry
            file.write_all(b"SOME GARBAGE DATA AT THE END")
                .await
                .expect("write"); // unwrap allowed (AGENT:08)
        }

        let wal2 = Wal::open(&wal_path).await.expect("open"); // unwrap allowed (AGENT:08)
        let entries = wal2.replay().await.expect("replay"); // unwrap allowed (AGENT:08)

        // Should succeed and return only the 2 valid entries
        assert_eq!(entries.len(), 2);
    }
}
