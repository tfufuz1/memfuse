//! Write-Ahead Log (WAL) for durability and crash recovery with HMAC chaining.

use crate::crypto::KeyManager;
use hmac::{Hmac, Mac};
use memfuse_core::{MemFuseError, Result, TxId};
use sha2::Sha256;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

type HmacSha256 = Hmac<Sha256>;

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
        let mut mac = HmacSha256::new_from_slice(integrity_key)
            .map_err(|e| MemFuseError::Storage(format!("HMAC key error: {}", e)))?;

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
        Ok(mac.finalize().into_bytes().into())
    }

    /// Serializes the entry to bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let op_size = match &self.op {
            WalOp::Put { key, value, .. } => 1 + 8 + 4 + key.len() + 4 + value.len(),
            WalOp::Delete { key, .. } => 1 + 8 + 4 + key.len(),
        };

        // payload = seq_no(8) + checksum(32) + prev_hmac(32) + op
        let payload_size = 8 + 32 + 32 + op_size;
        let total_size = 4 + payload_size; // length prefix + payload

        let mut buf = Vec::with_capacity(total_size);
        buf.extend_from_slice(&(payload_size as u32).to_le_bytes());
        buf.extend_from_slice(&self.seq_no.to_le_bytes());
        buf.extend_from_slice(&self.checksum);
        buf.extend_from_slice(&self.prev_hmac);

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
    /// Last HMAC written to the log, used for hash-chaining.
    last_hmac: tokio::sync::Mutex<[u8; 32]>,
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
            let entries = wal.replay().await?;
            if let Some((_, last_entry)) = entries.last() {
                let mut guard = wal.last_hmac.lock().await;
                *guard = last_entry.checksum;
            }
        }

        Ok(wal)
    }

    /// Appends an entry to the WAL.
    pub async fn append(&self, entry: &WalEntry) -> Result<()> {
        let mut bytes = entry.to_bytes();

        if let Some(km) = &self.key_manager {
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
        file.sync_data()
            .await
            .map_err(|e| MemFuseError::Storage(format!("WAL fsync failed: {}", e)))?;

        self.size
            .fetch_add(bytes.len() as u64, std::sync::atomic::Ordering::Relaxed);

        let mut last_hmac = self.last_hmac.lock().await;
        *last_hmac = entry.checksum;

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

    /// Replays the WAL, returning all valid entries.
    pub async fn replay(&self) -> Result<Vec<(u64, WalEntry)>> {
        let file = tokio::fs::File::open(&self.path)
            .await
            .map_err(|e| MemFuseError::Storage(format!("WAL replay open failed: {}", e)))?;
        let mut reader = tokio::io::BufReader::new(file);

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
                tracing::warn!("WAL entry too large at offset {}", pos);
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

            pos += (4 + len) as u64;

            let decrypted_data;
            let entry_data = if let Some(km) = &self.key_manager {
                // The offset used for encryption was the file size before writing the 4-byte length prefix.
                let offset = pos - len as u64 - 4;
                decrypted_data = km.decrypt(&entry_data_raw, offset)?;
                &decrypted_data
            } else {
                &entry_data_raw
            };

            if entry_data.len() < 73 {
                continue;
            }

            let seq_no = u64::from_le_bytes(
                entry_data
                    .get(0..8)
                    .ok_or(MemFuseError::WalCorruption {
                        offset: pos,
                        reason: "Invalid seq_no".into(),
                    })?
                    .try_into()
                    .map_err(|_| MemFuseError::WalCorruption {
                        offset: pos,
                        reason: "Invalid seq_no format".into(),
                    })?,
            );
            let stored_checksum: [u8; 32] = entry_data
                .get(8..40)
                .ok_or(MemFuseError::WalCorruption {
                    offset: pos,
                    reason: "Invalid checksum".into(),
                })?
                .try_into()
                .map_err(|_| MemFuseError::WalCorruption {
                    offset: pos,
                    reason: "Invalid checksum format".into(),
                })?;
            let prev_hmac: [u8; 32] = entry_data
                .get(40..72)
                .ok_or(MemFuseError::WalCorruption {
                    offset: pos,
                    reason: "Invalid prev_hmac".into(),
                })?
                .try_into()
                .map_err(|_| MemFuseError::WalCorruption {
                    offset: pos,
                    reason: "Invalid prev_hmac format".into(),
                })?;
            let op_type = *entry_data.get(72).ok_or(MemFuseError::WalCorruption {
                offset: pos,
                reason: "Invalid op_type".into(),
            })?;

            let remaining = entry_data.get(73..).ok_or(MemFuseError::WalCorruption {
                offset: pos,
                reason: "Unexpected end of entry".into(),
            })?;
            let op = match op_type {
                0 => {
                    if remaining.len() < 12 {
                        continue;
                    }
                    let tx_id = TxId::new(u64::from_le_bytes(
                        remaining
                            .get(0..8)
                            .ok_or(MemFuseError::WalCorruption {
                                offset: pos,
                                reason: "Invalid tx_id".into(),
                            })?
                            .try_into()
                            .map_err(|_| MemFuseError::WalCorruption {
                                offset: pos,
                                reason: "Invalid tx_id format".into(),
                            })?,
                    ));
                    let key_len = u32::from_le_bytes(
                        remaining
                            .get(8..12)
                            .ok_or(MemFuseError::WalCorruption {
                                offset: pos,
                                reason: "Invalid key_len".into(),
                            })?
                            .try_into()
                            .map_err(|_| MemFuseError::WalCorruption {
                                offset: pos,
                                reason: "Invalid key_len format".into(),
                            })?,
                    ) as usize;
                    if remaining.len() < 12 + key_len + 4 {
                        continue;
                    }
                    let key = remaining
                        .get(12..12 + key_len)
                        .ok_or(MemFuseError::WalCorruption {
                            offset: pos,
                            reason: "Invalid key data".into(),
                        })?
                        .to_vec();
                    let val_start = 12 + key_len;
                    let val_len = u32::from_le_bytes(
                        remaining
                            .get(val_start..val_start + 4)
                            .ok_or(MemFuseError::WalCorruption {
                                offset: pos,
                                reason: "Invalid val_len".into(),
                            })?
                            .try_into()
                            .map_err(|_| MemFuseError::WalCorruption {
                                offset: pos,
                                reason: "Invalid val_len format".into(),
                            })?,
                    ) as usize;
                    if remaining.len() < val_start + 4 + val_len {
                        continue;
                    }
                    let value = remaining
                        .get(val_start + 4..val_start + 4 + val_len)
                        .ok_or(MemFuseError::WalCorruption {
                            offset: pos,
                            reason: "Invalid value data".into(),
                        })?
                        .to_vec();
                    WalOp::Put { tx_id, key, value }
                }
                1 => {
                    if remaining.len() < 12 {
                        continue;
                    }
                    let tx_id = TxId::new(u64::from_le_bytes(
                        remaining
                            .get(0..8)
                            .ok_or(MemFuseError::WalCorruption {
                                offset: pos,
                                reason: "Invalid tx_id".into(),
                            })?
                            .try_into()
                            .map_err(|_| MemFuseError::WalCorruption {
                                offset: pos,
                                reason: "Invalid tx_id format".into(),
                            })?,
                    ));
                    let key_len = u32::from_le_bytes(
                        remaining
                            .get(8..12)
                            .ok_or(MemFuseError::WalCorruption {
                                offset: pos,
                                reason: "Invalid key_len".into(),
                            })?
                            .try_into()
                            .map_err(|_| MemFuseError::WalCorruption {
                                offset: pos,
                                reason: "Invalid key_len format".into(),
                            })?,
                    ) as usize;
                    if remaining.len() < 12 + key_len {
                        continue;
                    }
                    let key = remaining
                        .get(12..12 + key_len)
                        .ok_or(MemFuseError::WalCorruption {
                            offset: pos,
                            reason: "Invalid key data".into(),
                        })?
                        .to_vec();
                    WalOp::Delete { tx_id, key }
                }
                _ => continue,
            };

            let recomputed_checksum =
                WalEntry::compute_checksum(&op, seq_no, &integrity_key, prev_hmac)?;
            if recomputed_checksum != stored_checksum || prev_hmac != current_chain_hmac {
                tracing::warn!(
                    "WAL entry at offset {} has invalid checksum or broken chain, truncating replay",
                    pos
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
        .expect("try_new"); // expect #[cfg(test)]
        let bytes = entry.to_bytes();

        assert_eq!(bytes.len(), 101);
        let payload_len = u32::from_le_bytes(bytes[0..4].try_into().expect("valid slice")); // expect #[cfg(test)]
        assert_eq!(payload_len, 97);
    }

    #[tokio::test]
    async fn test_wal_append_and_replay_valid() {
        let dir = tempdir().expect("tempdir"); // expect #[cfg(test)]
        let wal_path = dir.path().join("test_wal.log");

        {
            let wal = Wal::open(&wal_path).await.expect("open WAL"); // expect #[cfg(test)]
            let op1 = WalOp::Put {
                tx_id: TxId::new(1),
                key: b"user:1".to_vec(),
                value: b"Alice".to_vec(),
            };
            let entry1 = wal.create_entry(op1, 10).await.expect("valid"); // expect #[cfg(test)]
            wal.append(&entry1).await.expect("append 1"); // expect #[cfg(test)]

            let op2 = WalOp::Delete {
                tx_id: TxId::new(2),
                key: b"user:1".to_vec(),
            };
            let entry2 = wal.create_entry(op2, 11).await.expect("valid"); // expect #[cfg(test)]
            wal.append(&entry2).await.expect("append 2"); // expect #[cfg(test)]
        }

        let wal2 = Wal::open(&wal_path).await.expect("reopen WAL"); // expect #[cfg(test)]
        let entries = wal2.replay().await.expect("replay"); // expect #[cfg(test)]

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].1.prev_hmac, entries[0].1.checksum);
    }

    #[tokio::test]
    async fn test_wal_hash_chain_verification() {
        let dir = tempdir().expect("tempdir"); // expect #[cfg(test)]
        let wal_path = dir.path().join("chain_wal.log");

        {
            let wal = Wal::open(&wal_path).await.expect("open"); // expect #[cfg(test)]
            let op1 = WalOp::Put {
                tx_id: TxId::new(1),
                key: b"k1".to_vec(),
                value: b"v1".to_vec(),
            };
            let entry1 = wal.create_entry(op1, 1).await.expect("entry1"); // expect #[cfg(test)]
            wal.append(&entry1).await.expect("append1"); // expect #[cfg(test)]

            let op2 = WalOp::Put {
                tx_id: TxId::new(2),
                key: b"k2".to_vec(),
                value: b"v2".to_vec(),
            };
            let entry2 = wal.create_entry(op2, 2).await.expect("entry2"); // expect #[cfg(test)]
            wal.append(&entry2).await.expect("append2"); // expect #[cfg(test)]
        }

        {
            let mut data = fs::read(&wal_path).await.expect("read"); // expect #[cfg(test)]
            data[12] ^= 0xFF;
            fs::write(&wal_path, data).await.expect("write"); // expect #[cfg(test)]
        }

        let wal2 = Wal::open(&wal_path).await.expect("open"); // expect #[cfg(test)]
        let entries = wal2.replay().await.expect("replay"); // expect #[cfg(test)]
        assert_eq!(entries.len(), 0);
    }
}
