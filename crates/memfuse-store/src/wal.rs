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

    /// Deserializes a WAL entry from bytes, verifying CRC32.
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < 4 {
            return Err(MemFuseError::Serialization(
                "WAL entry too short for CRC header".into(),
            ));
        }

        let stored_crc = u32::from_le_bytes(data[0..4].try_into().map_err(|_| {
            MemFuseError::Serialization("Invalid CRC format".into())
        })?);
        let payload = &data[4..];
        let computed_crc = crc32fast::hash(payload);

        if stored_crc != computed_crc {
            return Err(MemFuseError::Serialization(format!(
                "WAL CRC mismatch: stored={:#010x}, computed={:#010x}. \
                 WAL-Datei ist möglicherweise korrupt.",
                stored_crc, computed_crc
            )));
        }

        if payload.len() < 73 {
            // 8(seq) + 32(checksum) + 32(prev_hmac) + 1(op_type)
            return Err(MemFuseError::Serialization("WAL payload too short".into()));
        }

        let seq_no = u64::from_le_bytes(payload[0..8].try_into().map_err(|_| {
            MemFuseError::Serialization("Invalid seq_no format".into())
        })?);
        let checksum: [u8; 32] = payload[8..40].try_into().map_err(|_| {
            MemFuseError::Serialization("Invalid checksum format".into())
        })?;
        let prev_hmac: [u8; 32] = payload[40..72].try_into().map_err(|_| {
            MemFuseError::Serialization("Invalid prev_hmac format".into())
        })?;
        let op_type = payload[72];
        let remaining = &payload[73..];

        let op = match op_type {
            0 => {
                // Put
                if remaining.len() < 12 {
                    return Err(MemFuseError::Serialization("Put op too short".into()));
                }
                let tx_id = TxId::new(u64::from_le_bytes(remaining[0..8].try_into().map_err(
                    |_| MemFuseError::Serialization("Invalid tx_id format".into()),
                )?));
                let key_len = u32::from_le_bytes(remaining[8..12].try_into().map_err(|_| {
                    MemFuseError::Serialization("Invalid key_len format".into())
                })?) as usize;
                if remaining.len() < 12 + key_len + 4 {
                    return Err(MemFuseError::Serialization("Put op missing key/val_len".into()));
                }
                let key = remaining[12..12 + key_len].to_vec();
                let val_start = 12 + key_len;
                let val_len = u32::from_le_bytes(
                    remaining[val_start..val_start + 4]
                        .try_into()
                        .map_err(|_| MemFuseError::Serialization("Invalid val_len format".into()))?,
                ) as usize;
                if remaining.len() < val_start + 4 + val_len {
                    return Err(MemFuseError::Serialization("Put op missing value data".into()));
                }
                let value = remaining[val_start + 4..val_start + 4 + val_len].to_vec();
                WalOp::Put { tx_id, key, value }
            }
            1 => {
                // Delete
                if remaining.len() < 12 {
                    return Err(MemFuseError::Serialization("Delete op too short".into()));
                }
                let tx_id = TxId::new(u64::from_le_bytes(remaining[0..8].try_into().map_err(
                    |_| MemFuseError::Serialization("Invalid tx_id format".into()),
                )?));
                let key_len = u32::from_le_bytes(remaining[8..12].try_into().map_err(|_| {
                    MemFuseError::Serialization("Invalid key_len format".into())
                })?) as usize;
                if remaining.len() < 12 + key_len {
                    return Err(MemFuseError::Serialization("Delete op missing key data".into()));
                }
                let key = remaining[12..12 + key_len].to_vec();
                WalOp::Delete { tx_id, key }
            }
            _ => {
                return Err(MemFuseError::Serialization(format!(
                    "Unknown WAL op type: {}",
                    op_type
                )))
            }
        };

        Ok(Self {
            op,
            seq_no,
            checksum,
            prev_hmac,
        })
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

        // Derive sub-key for this specific file to prevent nonce reuse (FIND-CRY-002)
        let derived_key_manager = if let Some(km) = key_manager {
            let file_id = path
                .file_name()
                .map(|n| n.to_string_lossy())
                .unwrap_or_default();
            Some(Arc::new(km.derive_file_key(file_id.as_bytes())?))
        } else {
            None
        };

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
            key_manager: derived_key_manager,
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
        let integrity_key = self.get_integrity_key()?;
        WalEntry::try_new(op, seq_no, &integrity_key, *last_hmac)
    }

    /// Prepares a batch of entries, ensuring correct HMAC chaining between them.
    pub async fn prepare_batch(&self, ops: Vec<(WalOp, u64)>) -> Result<Vec<WalEntry>> {
        let last_hmac = self.last_hmac.lock().await;
        let integrity_key = self.get_integrity_key()?;

        let mut entries = Vec::with_capacity(ops.len());
        let mut current_chain = *last_hmac;

        for (op, seq_no) in ops {
            let entry = WalEntry::try_new(op, seq_no, &integrity_key, current_chain)?;
            current_chain = entry.checksum;
            entries.push(entry);
        }

        Ok(entries)
    }

    fn get_integrity_key(&self) -> Result<[u8; 32]> {
        if let Some(km) = &self.key_manager {
            km.integrity_key()
        } else {
            Ok(*b"memfuse-integrity-key-v1\0\0\0\0\0\0\0\0")
        }
    }

    /// Replays the WAL, returning all valid entries with their sequence numbers and end offsets.
    pub async fn replay(&self) -> Result<Vec<(u64, WalEntry, u64)>> {
        let metadata = tokio::fs::metadata(&self.path)
            .await
            .map_err(|e| MemFuseError::Storage(e.to_string()))?;
        self.replay_with_size(metadata.len()).await
    }

    /// Replays the WAL and returns all entries with seq_no > since_seq_no.
    pub async fn replay_from(&self, since_seq_no: u64) -> Result<Vec<WalEntry>> {
        let all = self.replay().await?;
        Ok(all
            .into_iter()
            .filter(|(seq, _, _)| *seq > since_seq_no)
            .map(|(_, entry, _)| entry)
            .collect())
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

            let entry = match WalEntry::from_bytes(entry_data) {
                Ok(e) => e,
                Err(e) => {
                    if pos >= file_size {
                        tracing::warn!(
                            "WAL corruption at tail (offset {}), truncating: {}",
                            entry_start_pos,
                            e
                        );
                        break;
                    } else {
                        return Err(MemFuseError::WalCorruption {
                            offset: entry_start_pos,
                            reason: format!("Deserialization failed: {}", e),
                        });
                    }
                }
            };

            let recomputed_checksum = WalEntry::compute_checksum(
                &entry.op,
                entry.seq_no,
                &integrity_key,
                entry.prev_hmac,
            )?;
            if recomputed_checksum != entry.checksum || entry.prev_hmac != current_chain_hmac {
                if pos >= file_size {
                    tracing::warn!(
                        "WAL entry at offset {} has invalid checksum or broken chain at tail, truncating",
                        entry_start_pos
                    );
                    break;
                } else {
                    return Err(MemFuseError::WalCorruption {
                        offset: entry_start_pos,
                        reason: format!(
                            "HMAC/Chain failure in middle of WAL: recomputed={:?}, stored={:?}, prev_hmac={:?}, current_chain={:?}",
                            recomputed_checksum, entry.checksum, entry.prev_hmac, current_chain_hmac
                        ),
                    });
                }
            }
            current_chain_hmac = entry.checksum;

            entries.push((entry.seq_no, entry, pos));
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

    /// Finds the offset and the previous HMAC for the given TxId.
    /// Returns the offset AFTER which the TxId's commits start (effectively the rollback point).
    pub async fn find_tx_offset(&self, target_tx_id: TxId) -> Result<(u64, [u8; 32])> {
        let entries = self.replay().await?;
        let mut last_offset = 0;
        let mut last_hmac = [0u8; 32];

        for (_, entry, offset) in entries {
            if entry.tx_id().inner() > target_tx_id.inner() {
                // If this entry strictly exceeds target_tx_id,
                // the rollback point is the end of the PREVIOUS entry.
                return Ok((last_offset, last_hmac));
            }
            last_offset = offset;
            last_hmac = entry.checksum;
        }

        // If target_tx_id is not found or is beyond the last entry,
        // no rollback is possible or needed at this point.
        Ok((last_offset, last_hmac))
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
        .expect("try_new");
        let bytes = entry.to_bytes();

        assert_eq!(bytes.len(), 105);
        let payload_len = u32::from_le_bytes(bytes[0..4].try_into().expect("valid slice"));
        assert_eq!(payload_len, 101);
    }

    #[tokio::test]
    async fn test_wal_append_and_replay_valid() {
        let dir = tempdir().expect("tempdir");
        let wal_path = dir.path().join("test_wal.log");

        {
            let wal = Wal::open(&wal_path).await.expect("open WAL");
            let op1 = WalOp::Put {
                tx_id: TxId::new(1),
                key: b"user:1".to_vec(),
                value: b"Alice".to_vec(),
            };
            let entry1 = wal.create_entry(op1, 10).await.expect("valid");
            wal.append(&entry1).await.expect("append 1");

            let op2 = WalOp::Delete {
                tx_id: TxId::new(2),
                key: b"user:1".to_vec(),
            };
            let entry2 = wal.create_entry(op2, 11).await.expect("valid");
            wal.append(&entry2).await.expect("append 2");
        }

        let wal2 = Wal::open(&wal_path).await.expect("reopen WAL");
        let entries = wal2.replay().await.expect("replay");

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].1.prev_hmac, entries[0].1.checksum);
    }

    #[tokio::test]
    async fn test_wal_hash_chain_verification() {
        let dir = tempdir().expect("tempdir");
        let wal_path = dir.path().join("chain_wal.log");

        {
            let wal = Wal::open(&wal_path).await.expect("open");
            let op1 = WalOp::Put {
                tx_id: TxId::new(1),
                key: b"k1".to_vec(),
                value: b"v1".to_vec(),
            };
            let entry1 = wal.create_entry(op1, 1).await.expect("entry1");
            wal.append(&entry1).await.expect("append1");

            let op2 = WalOp::Put {
                tx_id: TxId::new(2),
                key: b"k2".to_vec(),
                value: b"v2".to_vec(),
            };
            let entry2 = wal.create_entry(op2, 2).await.expect("entry2");
            wal.append(&entry2).await.expect("append2");
        }

        {
            let mut data = fs::read(&wal_path).await.expect("read");
            data[12] ^= 0xFF;
            fs::write(&wal_path, data).await.expect("write");
        }

        let result = Wal::open(&wal_path).await;
        assert!(matches!(result, Err(MemFuseError::WalCorruption { .. })));
    }
    #[tokio::test]
    async fn test_wal_replay_truncation() {
        let dir = tempdir().expect("tempdir");
        let wal_path = dir.path().join("trunc_wal.log");

        {
            let wal = Wal::open(&wal_path).await.expect("open");
            for i in 0..5 {
                let op = WalOp::Put {
                    tx_id: TxId::new(i),
                    key: b"key".to_vec(),
                    value: b"val".to_vec(),
                };
                let entry = wal.create_entry(op, i).await.expect("entry");
                wal.append(&entry).await.expect("append");
            }
        }

        // Truncate the file in the middle of the last entry
        let mut data = fs::read(&wal_path).await.expect("read");
        let new_size = data.len() - 10; // Chop off 10 bytes from the last entry
        data.truncate(new_size);
        fs::write(&wal_path, data).await.expect("write");

        let wal2 = Wal::open(&wal_path).await.expect("open");
        let entries = wal2.replay().await.expect("replay");
        assert_eq!(entries.len(), 4);
    }

    #[tokio::test]
    async fn test_wal_crc_middle_corruption() {
        let dir = tempdir().expect("tempdir");
        let wal_path = dir.path().join("middle_corrupt.log");

        {
            let wal = Wal::open(&wal_path).await.expect("open");
            for i in 0..3 {
                let op = WalOp::Put {
                    tx_id: TxId::new(i),
                    key: format!("k{}", i).into_bytes(),
                    value: format!("v{}", i).into_bytes(),
                };
                let entry = wal.create_entry(op, i).await.expect("entry");
                wal.append(&entry).await.expect("append");
            }
        }

        {
            let mut data = fs::read(&wal_path).await.expect("read");
            // Corrupt the second entry (somewhere in the middle of the file)
            // Each entry is ~100 bytes. Let's flip a bit around offset 150.
            if data.len() > 150 {
                data[150] ^= 0xFF;
                fs::write(&wal_path, data).await.expect("write");
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
        let dir = tempdir().expect("tempdir");
        let wal_path = dir.path().join("tail_corrupt.log");

        {
            let wal = Wal::open(&wal_path).await.expect("open");
            for i in 0..2 {
                let op = WalOp::Put {
                    tx_id: TxId::new(i),
                    key: format!("k{}", i).into_bytes(),
                    value: format!("v{}", i).into_bytes(),
                };
                let entry = wal.create_entry(op, i).await.expect("entry");
                wal.append(&entry).await.expect("append");
            }
        }

        {
            let mut file = fs::OpenOptions::new()
                .append(true)
                .open(&wal_path)
                .await
                .expect("open");
            use tokio::io::AsyncWriteExt;
            // Append some garbage that doesn't form a valid entry
            file.write_all(b"SOME GARBAGE DATA AT THE END")
                .await
                .expect("write");
        }

        let wal2 = Wal::open(&wal_path).await.expect("open");
        let entries = wal2.replay().await.expect("replay");

        // Should succeed and return only the 2 valid entries
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_wal_entry_crc_corruption_detected() {
        let op = WalOp::Put {
            tx_id: TxId::new(1),
            key: b"key".to_vec(),
            value: b"value".to_vec(),
        };
        let entry = WalEntry::try_new(
            op,
            1,
            b"memfuse-integrity-key-v1\0\0\0\0\0\0\0\0",
            [0u8; 32],
        )
        .expect("try_new");
        
        let mut bytes = entry.to_bytes();
        
        // Let's corrupt the payload which is after the length prefix(4) and CRC(4)
        if bytes.len() > 10 {
            bytes[10] ^= 0xFF;
        }
        
        // Check using from_bytes (skipping the length prefix at the start)
        let result = WalEntry::from_bytes(&bytes[4..]);
        assert!(result.is_err(), "Corruption must be detected by CRC check");
        let err = result.unwrap_err();
        assert!(format!("{}", err).contains("CRC mismatch"));
        assert!(format!("{}", err).contains("korrupt"));
    }

    #[test]
    fn test_wal_entry_crc_roundtrip() {
        let op = WalOp::Put {
            tx_id: TxId::new(42),
            key: b"test_key".to_vec(),
            value: b"test_value".to_vec(),
        };
        let entry = WalEntry::try_new(
            op,
            100,
            b"memfuse-integrity-key-v1\0\0\0\0\0\0\0\0",
            [0u8; 32],
        )
        .expect("try_new");
        
        let bytes = entry.to_bytes();
        let decoded = WalEntry::from_bytes(&bytes[4..]).expect("Roundtrip must work");
        
        assert_eq!(decoded.seq_no, 100);
        if let WalOp::Put { key, value, .. } = decoded.op {
            assert_eq!(key, b"test_key");
            assert_eq!(value, b"test_value");
        } else {
            panic!("Wrong op type");
        }
    }
}
