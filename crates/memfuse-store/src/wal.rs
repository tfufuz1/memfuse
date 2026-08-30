//! Write-Ahead Log (WAL) for durability and crash recovery with HMAC chaining.
// FILE-CONTEXT
// STAND: 2026-08-27T14:32:00Z
// ZWECK: Write-Ahead-Log mit HMAC-Chaining für crash-sichere WAL-Operationen
// INVARIANTEN: fsync NACH jedem Schreibvorgang (ADR-002); WAL VOR MemTable schreiben
// NICHT-OFFENSICHTLICH: sync_all() auf dem Verzeichnis-FD nötig, nicht nur auf der Datei
// SIEHE AUCH: rules/tag_taxonomy.md, DECISIONS.md ADR-002

use memfuse_core::{MemFuseError, Result, TxId};
use memfuse_crypto::crypto::KeyManager;
use memfuse_crypto::wal_crypto::{IntegrityVerifier, WalEntrySnapshot, WalHmac};
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

/// Magic header for V2 batch-encrypted WAL files (`b"MFW2"`).
pub const WAL_V2_HEADER: [u8; 4] = *b"MFW2";

/// Magic header for V3 WAL files (`b"MFW3"`).
pub const WAL_V3_HEADER: [u8; 4] = *b"MFW3";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalVersion {
    V1, // Legacy: kein HMAC
    V2, // Current: HMAC ohne tx_id
    V3, // New: HMAC mit tx_id
}

/// Legacy static HMAC integrity key used strictly for backward-compatibility fallback during WAL replay of legacy databases.
///
/// Cryptographic Audit Guarantee (Task E):
/// 1. This key is ONLY used during replay of pre-migration WAL files when per-file key verification fails.
/// 2. It is NEVER used for new write or append operations (all new WAL writes derive an integrity key via `KeyManager`).
/// 3. After successful replay and LSM compaction into SSTables, old WAL files using `LEGACY_INTEGRITY_KEY` are superseded and truncated/removed.
///
/// ANCHOR[MIGRATION:WAL-HMAC-001] STATUS:DONE (TS:2026-06-01T00:00:00Z)
pub const LEGACY_INTEGRITY_KEY: [u8; 32] = *b"memfuse-integrity-key-v1\0\0\0\0\0\0\0\0";

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
        let checksum = Self::compute_checksum_v3(&op, seq_no, integrity_key, prev_hmac)?;
        Ok(Self {
            op,
            seq_no,
            checksum,
            prev_hmac,
        })
    }

    /// Computes V3 checksum (includes tx_id and length prefixes for key/value).
    pub fn compute_checksum_v3(
        op: &WalOp,
        seq_no: u64,
        integrity_key: &[u8],
        prev_hmac: [u8; 32],
    ) -> Result<[u8; 32]> {
        let mut mac = WalHmac::new(integrity_key)?;

        // Hash Chaining: binding to the previous entry
        mac.update(&prev_hmac);
        mac.update(&seq_no.to_le_bytes());

        // tx_id MUST come before op_type
        let tx_id_bytes = op.tx_id().inner().to_le_bytes();
        mac.update(&tx_id_bytes);

        match op {
            WalOp::Put { key, value, .. } => {
                mac.update(&[0u8]); // op type
                mac.update(&(key.len() as u32).to_le_bytes());
                mac.update(key);
                mac.update(&(value.len() as u32).to_le_bytes());
                mac.update(value);
            }
            WalOp::Delete { key, .. } => {
                mac.update(&[1u8]); // op type
                mac.update(&(key.len() as u32).to_le_bytes());
                mac.update(key);
            }
        }
        Ok(mac.finalize())
    }

    /// Legacy V2 checksum calculation (without tx_id and length-prefixes in HMAC).
    pub fn compute_checksum_v2(
        op: &WalOp,
        seq_no: u64,
        integrity_key: &[u8],
        prev_hmac: [u8; 32],
    ) -> Result<[u8; 32]> {
        let mut mac = WalHmac::new(integrity_key)?;

        mac.update(&prev_hmac);
        mac.update(&seq_no.to_le_bytes());
        match op {
            WalOp::Put { key, value, .. } => {
                mac.update(&[0u8]);
                mac.update(key);
                mac.update(value);
            }
            WalOp::Delete { key, .. } => {
                mac.update(&[1u8]);
                mac.update(key);
            }
        }
        Ok(mac.finalize())
    }

    pub fn compute_checksum(
        op: &WalOp,
        seq_no: u64,
        integrity_key: &[u8],
        prev_hmac: [u8; 32],
    ) -> Result<[u8; 32]> {
        Self::compute_checksum_v3(op, seq_no, integrity_key, prev_hmac)
    }

    /// Serializes the entry to bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let op_size = match &self.op {
            WalOp::Put { key, value, .. } => 1 + 8 + 4 + key.len() + 4 + value.len(),
            WalOp::Delete { key, .. } => 1 + 8 + 4 + key.len(),
        };

        // payload = seq_no(8) + checksum(32) + prev_hmac(32) + op
        let payload_size = 8 + 32 + 32 + op_size;
        // total_payload = CRC32(4) + payload
        let total_payload_size = 4 + payload_size;
        // total_size = length_prefix(4) + total_payload
        let total_size = 4 + total_payload_size;

        let mut buf = Vec::with_capacity(total_size);

        // 1. Length Prefix
        if total_payload_size > MAX_WAL_ENTRY_SIZE as usize {
            return Err(MemFuseError::Serialization(format!(
                "WAL entry too large: {} bytes (max {})",
                total_payload_size, MAX_WAL_ENTRY_SIZE
            )));
        }
        buf.extend_from_slice(&(total_payload_size as u32).to_le_bytes());

        // 2. CRC32 Placeholder (we'll fill this at the end)
        let crc_offset = buf.len();
        buf.extend_from_slice(&[0u8; 4]);

        // 3. Payload
        let payload_start = buf.len();
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

        // 4. Compute CRC32 over payload and fill placeholder
        let crc = crc32fast::hash(&buf[payload_start..]);
        buf[crc_offset..crc_offset + 4].copy_from_slice(&crc.to_le_bytes());

        Ok(buf)
    }

    /// Deserializes a WAL entry from bytes, verifying CRC32.
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < 4 {
            return Err(MemFuseError::Serialization(
                "WAL entry too short for CRC header".into(),
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
            // FIND-STO-001: Explicitly return a message that includes "CRC mismatch"
            // so replay can map it to WalCorruption.
            return Err(MemFuseError::Serialization(format!(
                "CRC mismatch: stored={:#010x}, computed={:#010x}",
                stored_crc, computed_crc
            )));
        }

        if payload.len() < 73 {
            // 8(seq) + 32(checksum) + 32(prev_hmac) + 1(op_type)
            return Err(MemFuseError::Serialization("WAL payload too short".into()));
        }

        let seq_no = u64::from_le_bytes(
            payload[0..8]
                .try_into()
                .map_err(|_| MemFuseError::Serialization("Invalid seq_no format".into()))?,
        );
        let checksum: [u8; 32] = payload[8..40]
            .try_into()
            .map_err(|_| MemFuseError::Serialization("Invalid checksum format".into()))?;
        let prev_hmac: [u8; 32] = payload[40..72]
            .try_into()
            .map_err(|_| MemFuseError::Serialization("Invalid prev_hmac format".into()))?;
        let op_type = payload[72];
        let remaining = &payload[73..];

        let op =
            match op_type {
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
                    if key_len > 1024 * 1024 {
                        return Err(MemFuseError::Serialization("key_len exceeds 1 MiB limit".into()));
                    }
                    if remaining.len() < 12 + key_len + 4 {
                        return Err(MemFuseError::Serialization(
                            "Put op missing key/val_len".into(),
                        ));
                    }
                    let key = remaining[12..12 + key_len].to_vec();
                    let val_start = 12 + key_len;
                    let val_len =
                        u32::from_le_bytes(remaining[val_start..val_start + 4].try_into().map_err(
                            |_| MemFuseError::Serialization("Invalid val_len format".into()),
                        )?) as usize;
                    if val_len > 128 * 1024 * 1024 {
                        return Err(MemFuseError::Serialization("val_len exceeds 128 MiB limit".into()));
                    }
                    if remaining.len() < val_start + 4 + val_len {
                        return Err(MemFuseError::Serialization(
                            "Put op missing value data".into(),
                        ));
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
                    if key_len > 1024 * 1024 {
                        return Err(MemFuseError::Serialization("key_len exceeds 1 MiB limit".into()));
                    }
                    if remaining.len() < 12 + key_len {
                        return Err(MemFuseError::Serialization(
                            "Delete op missing key data".into(),
                        ));
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
    fallback_integrity_key: Option<[u8; 32]>,
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

/// Maximum size for a single WAL entry payload (64MB).
pub const MAX_WAL_ENTRY_SIZE: u32 = 64 * 1024 * 1024;

impl Wal {
    fn handle_wal_entry_parse_error(
        e: MemFuseError,
        chunk_start_pos: u64,
        pos: u64,
        file_size: u64,
    ) -> Option<MemFuseError> {
        let err_msg = format!("{}", e);
        let is_crc_error = err_msg.contains("CRC mismatch");

        if pos >= file_size && !is_crc_error {
            tracing::warn!(
                "WAL truncation at tail (offset {}), partial entry: {}",
                chunk_start_pos,
                e
            );
            None
        } else {
            let reason = if is_crc_error {
                format!("CRC validation failed: {}", e)
            } else {
                format!("Deserialization failed: {}", e)
            };
            Some(MemFuseError::wal_corruption(chunk_start_pos, reason))
        }
    }

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

        // SD-09-CRYPTO-002: Use a persisted UUID v4 as file_id instead of the
        // filename.  This makes the WAL's cryptographic sub-key independent of
        // the filesystem path — renaming or moving the file cannot cause nonce-
        // reuse between two WAL instances sharing the same master key.
        let (derived_key_manager, fallback_integrity_key) = if let Some(km) = key_manager {
            let uuid_bytes = Self::load_or_create_wal_uuid(&path).await?;
            (Some(Arc::new(km.derive_file_key(&uuid_bytes)?)), None)
        } else {
            let key = Self::load_or_create_integrity_key(&path).await?;
            (None, Some(key))
        };

        let mut is_new = false;
        if !path.exists() {
            is_new = true;
        }

        let file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(&path)
            .await
            .map_err(|e| MemFuseError::Storage(format!("Failed to open WAL: {}", e)))?;

        // 🛡️ SICHERUNG: Directory FSync (FIND-STO-004 / Task G)
        if is_new {
            file.sync_all().await.map_err(|e| {
                MemFuseError::Storage(format!(
                    "WAL file fsync failed for {}: {}",
                    path.display(),
                    e
                ))
            })?;
            crate::util::fsync_parent_dir(&path).await?;
        }

        let metadata = file
            .metadata()
            .await
            .map_err(|e| MemFuseError::Storage(e.to_string()))?;

        let wal = Self {
            path: path.clone(),
            size: std::sync::atomic::AtomicU64::new(metadata.len()),
            file: tokio::sync::Mutex::new(file),
            key_manager: derived_key_manager,
            fallback_integrity_key,
            last_hmac: tokio::sync::Mutex::new([0u8; 32]),
        };

        // If file is not empty, find the last valid HMAC to continue the chain
        if metadata.len() > 0 {
            let (entries, version) = wal.replay_with_size_and_version(metadata.len()).await?;
            if version != WalVersion::V3 {
                tracing::info!(
                    "WAL {:?} format detected at {:?}. Will be rewritten as V3 after successful replay.",
                    version,
                    wal.path
                );
                wal.rewrite_as_v3(&entries).await?;
            } else if let Some((_, last_entry, _)) = entries.last() {
                let mut guard = wal.last_hmac.lock().await;
                *guard = last_entry.checksum;
            }
        }

        Ok(wal)
    }

    /// Helper to expose integrity key for tests
    pub fn integrity_key_for_test(&self) -> Result<[u8; 32]> {
        self.get_integrity_key()
    }

    /// Loads or generates a persistent, random 32-byte integrity key in `.wal_integrity_key`
    /// located in the same parent directory as the WAL file.
    async fn load_or_create_integrity_key(wal_path: &Path) -> Result<[u8; 32]> {
        let parent = wal_path.parent().unwrap_or_else(|| Path::new(""));
        let dir_path = if parent.as_os_str().is_empty() {
            Path::new(".")
        } else {
            parent
        };
        let key_path = if parent.as_os_str().is_empty() {
            PathBuf::from(".wal_integrity_key")
        } else {
            parent.join(".wal_integrity_key")
        };

        async fn read_key_file(path: &Path) -> Result<[u8; 32]> {
            let bytes = tokio::fs::read(path).await.map_err(|e| {
                MemFuseError::Storage(format!("Failed to read WAL integrity key: {}", e))
            })?;
            if bytes.is_empty() {
                return Err(MemFuseError::Storage(
                    "WAL integrity key file is empty — possible crash during creation. Delete and restart.".into(),
                ));
            }
            if bytes.len() != 32 {
                return Err(MemFuseError::Storage(format!(
                    "WAL integrity key has unexpected length: {} (expected 32)",
                    bytes.len()
                )));
            }
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            Ok(arr)
        }

        // Clean up any stale orphan .tmp files from previous crashed creation attempts
        if let Ok(mut entries) = tokio::fs::read_dir(dir_path).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let name = entry.file_name();
                if let Some(name_str) = name.to_str() {
                    if name_str.starts_with(".wal_integrity_key.tmp.") {
                        let _ = tokio::fs::remove_file(entry.path()).await;
                    }
                }
            }
        }

        // AI-TAG[SECURITY][CRITICAL] RESOLVED: Atomic WAL integrity key creation (TS:2026-08-29T08:06:29Z) (SESSION: a3f29c1d)
        // AGT-STORE-003 (SESSION:14348074)
        // Tests: tests/wal_key_lifecycle.rs — fault-injection, race, restart-persistence
        if key_path.exists() {
            read_key_file(&key_path).await
        } else {
            use rand::RngCore;
            use tokio::io::AsyncWriteExt;

            let mut key = [0u8; 32];
            rand::thread_rng().fill_bytes(&mut key);

            let tmp_path = dir_path.join(format!(
                ".wal_integrity_key.tmp.{}.{}",
                std::process::id(),
                rand::thread_rng().next_u64()
            ));

            let mut options = tokio::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                options.mode(0o600);
            }

            let file_res = options.open(&tmp_path).await;
            let mut file = match file_res {
                Ok(f) => f,
                Err(e) => {
                    return Err(MemFuseError::Storage(format!(
                        "Failed to create temporary WAL integrity key file at {}: {}",
                        tmp_path.display(),
                        e
                    )));
                }
            };

            if let Err(e) = file.write_all(&key).await {
                let _ = tokio::fs::remove_file(&tmp_path).await;
                return Err(MemFuseError::Storage(format!(
                    "Failed to write WAL integrity key: {}",
                    e
                )));
            }
            if let Err(e) = file.sync_all().await {
                let _ = tokio::fs::remove_file(&tmp_path).await;
                return Err(MemFuseError::Storage(format!(
                    "Failed to sync WAL integrity key file: {}",
                    e
                )));
            }
            drop(file);

            // Atomically link tmp_path to key_path. Fails if key_path already exists (O_EXCL semantics).
            let link_res = tokio::fs::hard_link(&tmp_path, &key_path).await;
            let _ = tokio::fs::remove_file(&tmp_path).await;

            match link_res {
                Ok(()) => {
                    // FSync parent directory to persist directory entry
                    crate::util::fsync_parent_dir(&key_path).await?;
                    Ok(key)
                }
                Err(_) => {
                    // AlreadyExists or race condition: another task created key_path first
                    read_key_file(&key_path).await
                }
            }
        }
    }

    /// Loads the WAL's persistent UUID from a `.uuid` sidecar file next to the
    /// WAL path, creating a new UUID v4 and persisting it if the sidecar does
    /// not yet exist.
    ///
    /// The sidecar contains exactly 16 raw bytes (UUID in native byte order).
    async fn load_or_create_wal_uuid(wal_path: &Path) -> Result<[u8; 16]> {
        let uuid_path = {
            let mut p = wal_path.as_os_str().to_os_string();
            p.push(".uuid");
            std::path::PathBuf::from(p)
        };

        if uuid_path.exists() {
            let bytes = tokio::fs::read(&uuid_path).await.map_err(|e| {
                MemFuseError::Storage(format!("Failed to read WAL UUID sidecar: {}", e))
            })?;
            if bytes.len() != 16 {
                return Err(MemFuseError::Storage(format!(
                    "WAL UUID sidecar has unexpected length: {} (expected 16)",
                    bytes.len()
                )));
            }
            let mut arr = [0u8; 16];
            arr.copy_from_slice(&bytes);
            Ok(arr)
        } else {
            // Generate and persist a fresh UUID v4.
            let uuid = uuid::Uuid::new_v4();
            let bytes = *uuid.as_bytes();
            tokio::fs::write(&uuid_path, &bytes).await.map_err(|e| {
                MemFuseError::Storage(format!("Failed to write WAL UUID sidecar: {}", e))
            })?;

            // FIND-STO-004: FSync parent directory to persist the new directory entry
            crate::util::fsync_parent_dir(&uuid_path).await?;

            Ok(bytes)
        }
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
        let current_size = self.size();

        // Write V3 header if file is currently empty (size == 0)
        if current_size == 0 {
            total_bytes.extend_from_slice(&WAL_V3_HEADER);
        }

        let mut last_hmac_val = [0u8; 32];

        if let Some(km) = &self.key_manager {
            let mut batch_plaintext = Vec::new();
            for entry in entries {
                let bytes = entry.to_bytes()?;
                batch_plaintext.extend_from_slice(&bytes);
                last_hmac_val = entry.checksum;
            }

            let (encrypted, nonce) = km.encrypt_auto_nonce(&batch_plaintext)?;
            let chunk_len = (12 + encrypted.len()) as u32;

            total_bytes.extend_from_slice(&chunk_len.to_le_bytes());
            total_bytes.extend_from_slice(&nonce);
            total_bytes.extend_from_slice(&encrypted);
        } else {
            for entry in entries {
                let bytes = entry.to_bytes()?;
                total_bytes.extend_from_slice(&bytes);
                last_hmac_val = entry.checksum;
            }
        }

        let mut file = self.file.lock().await;
        file.write_all(&total_bytes).await.map_err(|e| {
            MemFuseError::Storage(format!(
                "WAL batch write failed for {}: {}",
                self.path.display(),
                e
            ))
        })?;
        file.flush().await.map_err(|e| {
            MemFuseError::Storage(format!(
                "WAL batch flush failed for {}: {}",
                self.path.display(),
                e
            ))
        })?;
        file.sync_all().await.map_err(|e| {
            MemFuseError::Storage(format!(
                "WAL batch fsync failed for {}: {}",
                self.path.display(),
                e
            ))
        })?;

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
        } else if let Some(key) = self.fallback_integrity_key {
            Ok(key)
        } else {
            Err(MemFuseError::Storage(
                "Integrity key missing from WAL state".into(),
            ))
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
        let (entries, _) = self.replay_with_size_and_version(file_size).await?;
        Ok(entries)
    }

    async fn replay_with_size_and_version(
        &self,
        file_size: u64,
    ) -> Result<(Vec<(u64, WalEntry, u64)>, WalVersion)> {
        let mut file = self.file.lock().await;
        use tokio::io::AsyncSeekExt;
        file.seek(std::io::SeekFrom::Start(0))
            .await
            .map_err(|e| MemFuseError::Storage(format!("WAL replay seek failed: {}", e)))?;

        let mut reader = tokio::io::BufReader::new(&mut *file);

        let mut entries = Vec::new();
        let mut pos = 0u64;

        let mut version = WalVersion::V1;
        if file_size == 0 {
            return Ok((entries, version));
        }

        let integrity_key = self.get_integrity_key()?;
        let mut verifier = IntegrityVerifier::new(&integrity_key);
        let mut using_legacy_key = false;

        // Detect version from header
        if file_size >= 4 {
            let mut header_bytes = [0u8; 4];
            match reader.read_exact(&mut header_bytes).await {
                Ok(_) => {
                    if header_bytes == WAL_V3_HEADER {
                        version = WalVersion::V3;
                        pos = 4;
                    } else if header_bytes == WAL_V2_HEADER {
                        version = WalVersion::V2;
                        pos = 4;
                    } else {
                        // Rewind to 0 if not V3 or V2 header
                        reader
                            .seek(std::io::SeekFrom::Start(0))
                            .await
                            .map_err(|e| {
                                MemFuseError::Storage(format!("WAL seek failed: {}", e))
                            })?;
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    return Ok((entries, version))
                }
                Err(e) => return Err(MemFuseError::Storage(format!("WAL read failed: {}", e))),
            }
        }

        loop {
            let mut len_bytes = [0u8; 4];
            match reader.read_exact(&mut len_bytes).await {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(MemFuseError::Storage(format!("WAL read failed: {}", e))),
            };
            let len = u32::from_le_bytes(len_bytes) as usize;

            if len > MAX_WAL_ENTRY_SIZE as usize {
                if pos + 4 + len as u64 > file_size {
                    // STO-001: Massive Fehl-Länge am Anfang ist Korruption, am Ende (Tail) ignorable.
                    if entries.is_empty() && file_size > 64 {
                        return Err(MemFuseError::wal_corruption(
                            pos,
                            format!(
                                "WAL entry length ({}) exceeds hard limit and file size",
                                len
                            ),
                        ));
                    }
                    tracing::warn!("WAL tail corruption (huge len) at offset {}", pos);
                    break;
                }
                return Err(MemFuseError::wal_corruption(
                    pos,
                    format!("WAL entry too large ({} bytes)", len),
                ));
            }

            if pos + 4 + len as u64 > file_size {
                if entries.is_empty() && file_size > 64 {
                    return Err(MemFuseError::wal_corruption(
                        pos,
                        format!(
                            "WAL entry length ({}) exceeds file size ({}) at start of file",
                            len, file_size
                        ),
                    ));
                }
                tracing::warn!("WAL tail corruption (partial entry) at offset {}", pos);
                break;
            }

            let mut entry_data_raw = vec![0u8; len];
            match reader.read_exact(&mut entry_data_raw).await {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    // Falls wir trotz vorheriger Prüfung EOF erreichen, ist es eine Truncation.
                    tracing::warn!("WAL truncated during read at offset {}", pos);
                    break;
                }
                Err(e) => return Err(MemFuseError::Storage(format!("WAL read failed: {}", e))),
            };

            let chunk_start_pos = pos;
            pos += (4 + len) as u64;

            if matches!(version, WalVersion::V2 | WalVersion::V3)
                && self.key_manager.is_some()
            {
                // SAFETY: Checked self.key_manager.is_some() in the if condition above
                let km = self.key_manager.as_ref().unwrap();
                if entry_data_raw.len() < 12 {
                    if pos >= file_size {
                        tracing::warn!("WAL truncated during read at offset {}", chunk_start_pos);
                        break;
                    }
                    return Err(MemFuseError::Storage(
                        "WAL entry too short for nonce".into(),
                    ));
                }
                let mut nonce = [0u8; 12];
                nonce.copy_from_slice(&entry_data_raw[0..12]);
                let decrypted_data = match km.decrypt_auto_nonce(&entry_data_raw[12..], &nonce) {
                    Ok(data) => data,
                    Err(e) => {
                        if pos >= file_size {
                            tracing::warn!(
                                "WAL truncation at tail (offset {}), decryption failed: {}",
                                chunk_start_pos,
                                e
                            );
                            break;
                        }
                        return Err(MemFuseError::wal_corruption(
                            chunk_start_pos,
                            format!("Decryption failed: {}", e),
                        ));
                    }
                };

                let slice = decrypted_data.as_slice();
                while !slice.is_empty() {
                    if slice.len() < 4 {
                        if pos >= file_size {
                            tracing::warn!(
                                "WAL truncation at tail (offset {}), incomplete inner framing",
                                chunk_start_pos
                            );
                            break;
                        }
                        return Err(MemFuseError::wal_corruption(
                            chunk_start_pos,
                            "Truncated inner WAL entry length in batch",
                        ));
                    }
                    let inner_len_bytes: [u8; 4] = match slice[0..4].try_into() {
                        Ok(b) => b,
                        Err(_) => {
                            return Err(MemFuseError::wal_corruption(
                                chunk_start_pos,
                                "Failed to extract inner WAL entry length",
                            ));
                        }
                    };
                    let inner_len = u32::from_le_bytes(inner_len_bytes) as usize;
                    if slice.len() < 4 + inner_len {
                        if pos >= file_size {
                            tracing::warn!(
                                "WAL truncation at tail (offset {}), incomplete inner payload",
                                chunk_start_pos
                            );
                            break;
                        }
                        return Err(MemFuseError::wal_corruption(
                            chunk_start_pos,
                            "Truncated inner WAL entry in batch",
                        ));
                    }
                    let inner_entry_bytes = &slice[4..4 + inner_len];
                    slice = &slice[4 + inner_len..];

                    let entry = match WalEntry::from_bytes(inner_entry_bytes) {
                        Ok(e) => e,
                        Err(e) => {
                            let err_msg = format!("{}", e);
                            let is_crc_error = err_msg.contains("CRC mismatch");

                            if pos >= file_size && !is_crc_error {
                                tracing::warn!(
                                    "WAL truncation at tail (offset {}), partial entry: {}",
                                    chunk_start_pos,
                                    e
                                );
                                break;
                            } else {
                                let reason = if is_crc_error {
                                    format!("CRC validation failed: {}", e)
                                } else {
                                    format!("Deserialization failed: {}", e)
                                };
                                return Err(MemFuseError::wal_corruption(chunk_start_pos, reason));
                            }
                        }
                    };

                    let (op_type, key, value) = match &entry.op {
                        WalOp::Put { key, value, .. } => (0u8, key.clone(), value.clone()),
                        WalOp::Delete { key, .. } => (1u8, key.clone(), Vec::new()),
                    };

                    let snapshot = WalEntrySnapshot {
                        tx_id: entry.tx_id(),
                        seq_no: entry.seq_no,
                        op_type,
                        key,
                        value,
                        checksum: entry.checksum,
                        prev_hmac: entry.prev_hmac,
                    };

                    let verify_res = match version {
                        WalVersion::V3 => verifier.verify_and_update_v3(&snapshot, chunk_start_pos),
                        WalVersion::V2 => verifier.verify_and_update_v2(&snapshot, chunk_start_pos),
                        WalVersion::V1 => {
                            verifier.skip_hmac_verify_legacy(&snapshot);
                            Ok(())
                        }
                    };

                    if let Err(e) = verify_res {
                        if !using_legacy_key {
                            let mut legacy_verifier = IntegrityVerifier::new(&LEGACY_INTEGRITY_KEY);
                            let legacy_res =
                                match version {
                                    WalVersion::V3 => legacy_verifier
                                        .verify_and_update_v3(&snapshot, chunk_start_pos),
                                    WalVersion::V2 => legacy_verifier
                                        .verify_and_update_v2(&snapshot, chunk_start_pos),
                                    WalVersion::V1 => {
                                        legacy_verifier.skip_hmac_verify_legacy(&snapshot);
                                        Ok(())
                                    }
                                };
                            if legacy_res.is_ok() {
                                tracing::warn!(
                                    "WAL nutzt veralteten Integritätsschlüssel — Datenbank sollte neu initialisiert werden"
                                );
                                verifier = legacy_verifier;
                                using_legacy_key = true;
                            } else {
                                return Err(e);
                            }
                        } else {
                            return Err(e);
                        }
                    }

                    entries.push((entry.seq_no, entry, pos));
                }
            } else {
                let decrypted_data;
                let entry_data = if let Some(km) = &self.key_manager {
                    if entry_data_raw.len() < 12 {
                        return Err(MemFuseError::Storage(
                            "WAL entry too short for nonce".into(),
                        ));
                    }
                    let mut nonce = [0u8; 12];
                    nonce.copy_from_slice(&entry_data_raw[0..12]);
                    decrypted_data = match km.decrypt_auto_nonce(&entry_data_raw[12..], &nonce) {
                        Ok(data) => data,
                        Err(e) => {
                            if pos >= file_size {
                                tracing::warn!(
                                    "WAL truncation at tail (offset {}), decryption failed: {}",
                                    chunk_start_pos,
                                    e
                                );
                                break;
                            }
                            return Err(MemFuseError::wal_corruption(
                                chunk_start_pos,
                                format!("Decryption failed: {}", e),
                            ));
                        }
                    };
                    &decrypted_data
                } else {
                    &entry_data_raw
                };

                let entry = match WalEntry::from_bytes(entry_data) {
                    Ok(e) => e,
                    Err(e) => {
                        if let Some(err) =
                            Self::handle_wal_entry_parse_error(e, chunk_start_pos, pos, file_size)
                        {
                            return Err(err);
                        }
                        break;
                    }
                };

                let (op_type, key, value) = match &entry.op {
                    WalOp::Put { key, value, .. } => (0u8, key.clone(), value.clone()),
                    WalOp::Delete { key, .. } => (1u8, key.clone(), Vec::new()),
                };

                let snapshot = WalEntrySnapshot {
                    tx_id: entry.tx_id(),
                    seq_no: entry.seq_no,
                    op_type,
                    key,
                    value,
                    checksum: entry.checksum,
                    prev_hmac: entry.prev_hmac,
                };

                let verify_res = match version {
                    WalVersion::V3 => verifier.verify_and_update_v3(&snapshot, chunk_start_pos),
                    WalVersion::V2 => verifier.verify_and_update_v2(&snapshot, chunk_start_pos),
                    WalVersion::V1 => {
                        verifier.skip_hmac_verify_legacy(&snapshot);
                        Ok(())
                    }
                };

                if let Err(e) = verify_res {
                    if !using_legacy_key {
                        let mut legacy_verifier = IntegrityVerifier::new(&LEGACY_INTEGRITY_KEY);
                        let legacy_res = match version {
                            WalVersion::V3 => {
                                legacy_verifier.verify_and_update_v3(&snapshot, chunk_start_pos)
                            }
                            WalVersion::V2 => {
                                legacy_verifier.verify_and_update_v2(&snapshot, chunk_start_pos)
                            }
                            WalVersion::V1 => {
                                legacy_verifier.skip_hmac_verify_legacy(&snapshot);
                                Ok(())
                            }
                        };
                        if legacy_res.is_ok() {
                            tracing::warn!(
                                "WAL nutzt veralteten Integritätsschlüssel — Datenbank sollte neu initialisiert werden"
                            );
                            verifier = legacy_verifier;
                            using_legacy_key = true;
                        } else {
                            return Err(e);
                        }
                    } else {
                        return Err(e);
                    }
                }

                entries.push((entry.seq_no, entry, pos));
            }
        }

        Ok((entries, version))
    }

    /// Rewrites legacy V1 or V2 WAL files as V3.
    async fn rewrite_as_v3(&self, replayed_entries: &[(u64, WalEntry, u64)]) -> Result<()> {
        let integrity_key = self.get_integrity_key()?;
        let mut v3_entries = Vec::with_capacity(replayed_entries.len());
        let mut prev_hmac = [0u8; 32];

        for (_, entry, _) in replayed_entries {
            let v3_entry =
                WalEntry::try_new(entry.op.clone(), entry.seq_no, &integrity_key, prev_hmac)?;
            prev_hmac = v3_entry.checksum;
            v3_entries.push(v3_entry);
        }

        let mut file = self.file.lock().await;
        use tokio::io::AsyncSeekExt;
        file.seek(std::io::SeekFrom::Start(0)).await.map_err(|e| {
            MemFuseError::Storage(format!("WAL seek failed during migration: {}", e))
        })?;
        file.set_len(0).await.map_err(|e| {
            MemFuseError::Storage(format!("WAL truncate failed during migration: {}", e))
        })?;

        let mut total_bytes = Vec::new();
        total_bytes.extend_from_slice(&WAL_V3_HEADER);

        let mut last_hmac_val = [0u8; 32];
        if let Some(km) = &self.key_manager {
            let mut batch_plaintext = Vec::new();
            for entry in &v3_entries {
                let bytes = entry.to_bytes()?;
                batch_plaintext.extend_from_slice(&bytes);
                last_hmac_val = entry.checksum;
            }

            let (encrypted, nonce) = km.encrypt_auto_nonce(&batch_plaintext)?;
            let chunk_len = (12 + encrypted.len()) as u32;

            total_bytes.extend_from_slice(&chunk_len.to_le_bytes());
            total_bytes.extend_from_slice(&nonce);
            total_bytes.extend_from_slice(&encrypted);
        } else {
            for entry in &v3_entries {
                let bytes = entry.to_bytes()?;
                total_bytes.extend_from_slice(&bytes);
                last_hmac_val = entry.checksum;
            }
        }

        file.write_all(&total_bytes)
            .await
            .map_err(|e| MemFuseError::Storage(format!("WAL migration write failed: {}", e)))?;
        file.flush()
            .await
            .map_err(|e| MemFuseError::Storage(format!("WAL migration flush failed: {}", e)))?;
        file.sync_all()
            .await
            .map_err(|e| MemFuseError::Storage(format!("WAL migration fsync failed: {}", e)))?;

        self.size.store(
            total_bytes.len() as u64,
            std::sync::atomic::Ordering::SeqCst,
        );
        let mut last_hmac = self.last_hmac.lock().await;
        *last_hmac = last_hmac_val;

        Ok(())
    }

    pub fn size(&self) -> u64 {
        self.size.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Physically truncates the WAL file to the specified offset.
    /// This also updates the in-memory size and the HMAC chain link.
    ///
    /// # Errors
    /// Returns `MemFuseError::Storage` if setting file length or seeking fails.
    pub async fn truncate(&self, offset: u64, new_last_hmac: [u8; 32]) -> Result<()> {
        use tokio::io::AsyncSeekExt;

        let mut file = self.file.lock().await;

        // 1. Physically truncate the file
        file.set_len(offset)
            .await
            .map_err(|e| MemFuseError::Storage(format!("WAL truncate failed: {e}")))?;

        // 2. Ensure we seek to the new end
        file.seek(std::io::SeekFrom::Start(offset))
            .await
            .map_err(|e| MemFuseError::Storage(format!("WAL seek after truncate failed: {e}")))?;

        drop(file);

        // 3. Update in-memory size
        self.size.store(offset, std::sync::atomic::Ordering::SeqCst);

        // 4. Update last_hmac
        let mut last_hmac_guard = self.last_hmac.lock().await;
        *last_hmac_guard = new_last_hmac;
        drop(last_hmac_guard);

        Ok(())
    }

    /// Returns a snapshot of the last HMAC written to the log.
    pub async fn last_hmac_snapshot(&self) -> [u8; 32] {
        *self.last_hmac.lock().await
    }

    /// Finds the offset and the previous HMAC for the given `TxId`.
    /// Returns the offset AFTER which the `TxId`'s commits start (effectively the rollback point).
    ///
    /// # Errors
    /// Returns `MemFuseError::Storage` or `MemFuseError::WalCorruption` if reading or replaying the WAL fails.
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
        let dummy_key = b"test-integrity-key-32-bytes-long!";
        let entry = WalEntry::try_new(op, 100, dummy_key, [0u8; 32]).expect("try_new"); // expect
        let bytes = entry.to_bytes().expect("serialization failed"); // expect

        // 4 (len) + 4 (crc) + 8 (seq) + 32 (hmac) + 32 (prev) + 1 (op) + 8 (tx) + 4 (klen) + 3 (k) + 4 (vlen) + 5 (v) = 105
        assert_eq!(bytes.len(), 105);
        let total_payload_size = u32::from_le_bytes(bytes[0..4].try_into().expect("valid slice")); // expect
        assert_eq!(total_payload_size, 101); // 4 (crc) + 97 (payload)
    }

    #[tokio::test]
    async fn test_wal_append_and_replay_valid() {
        let dir = tempdir().expect("tempdir"); // expect
        let wal_path = dir.path().join("test_wal.log");

        {
            let wal = Wal::open(&wal_path).await.expect("open WAL"); // expect
            let op1 = WalOp::Put {
                tx_id: TxId::new(1),
                key: b"user:1".to_vec(),
                value: b"Alice".to_vec(),
            };
            let entry1 = wal.create_entry(op1, 10).await.expect("valid"); // expect
            wal.append(&entry1).await.expect("append 1"); // expect

            let op2 = WalOp::Delete {
                tx_id: TxId::new(2),
                key: b"user:1".to_vec(),
            };
            let entry2 = wal.create_entry(op2, 11).await.expect("valid"); // expect
            wal.append(&entry2).await.expect("append 2"); // expect
        }

        let wal2 = Wal::open(&wal_path).await.expect("reopen WAL"); // expect
        let entries = wal2.replay().await.expect("replay"); // expect

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].1.prev_hmac, entries[0].1.checksum);
    }

    #[tokio::test]
    async fn test_wal_hash_chain_verification() {
        let dir = tempdir().expect("tempdir"); // expect
        let wal_path = dir.path().join("chain_wal.log");

        {
            let wal = Wal::open(&wal_path).await.expect("open"); // expect
            let op1 = WalOp::Put {
                tx_id: TxId::new(1),
                key: b"k1".to_vec(),
                value: b"v1".to_vec(),
            };
            let entry1 = wal.create_entry(op1, 1).await.expect("entry1"); // expect
            wal.append(&entry1).await.expect("append1"); // expect

            let op2 = WalOp::Put {
                tx_id: TxId::new(2),
                key: b"k2".to_vec(),
                value: b"v2".to_vec(),
            };
            let entry2 = wal.create_entry(op2, 2).await.expect("entry2"); // expect
            wal.append(&entry2).await.expect("append2"); // expect
        }

        {
            let mut data = fs::read(&wal_path).await.expect("read"); // expect
                                                                     // Corrupt the payload of the first entry (offset 4 is CRC, payload starts at 8)
                                                                     // CRC itself is also part of validation. Let's flip a bit in the payload.
            if data.len() > 10 {
                data[12] ^= 0xFF;
                fs::write(&wal_path, data).await.expect("write"); // expect
            }
        }

        let result = Wal::open(&wal_path).await;
        // Should fail due to CRC mismatch or HMAC chain failure
        assert!(matches!(
            result,
            Err(MemFuseError::Serialization(_)) | Err(MemFuseError::WalCorruption { .. })
        ));
    }
    #[tokio::test]
    async fn test_wal_replay_truncation() {
        let dir = tempdir().expect("tempdir"); // expect
        let wal_path = dir.path().join("trunc_wal.log");

        {
            let wal = Wal::open(&wal_path).await.expect("open"); // expect
            for i in 0..5 {
                let op = WalOp::Put {
                    tx_id: TxId::new(i),
                    key: b"key".to_vec(),
                    value: b"val".to_vec(),
                };
                let entry = wal.create_entry(op, i).await.expect("entry"); // expect
                wal.append(&entry).await.expect("append"); // expect
            }
        }

        // Truncate the file in the middle of the last entry
        let mut data = fs::read(&wal_path).await.expect("read"); // expect
        let new_size = data.len() - 10; // Chop off 10 bytes from the last entry
        data.truncate(new_size);
        fs::write(&wal_path, data).await.expect("write"); // expect

        let wal2 = Wal::open(&wal_path).await.expect("open"); // expect
        let entries = wal2.replay().await.expect("replay"); // expect
                                                            // Replay should stop at the last valid entry (the 4th one)
        assert_eq!(entries.len(), 4);
    }

    #[tokio::test]
    async fn test_wal_crc_middle_corruption() {
        let dir = tempdir().expect("tempdir"); // expect
        let wal_path = dir.path().join("middle_corrupt.log");

        {
            let wal = Wal::open(&wal_path).await.expect("open"); // expect
            for i in 0..3 {
                let op = WalOp::Put {
                    tx_id: TxId::new(i),
                    key: format!("k{}", i).into_bytes(),
                    value: format!("v{}", i).into_bytes(),
                };
                let entry = wal.create_entry(op, i).await.expect("entry"); // expect
                wal.append(&entry).await.expect("append"); // expect
            }
        }

        {
            let mut data = fs::read(&wal_path).await.expect("read"); // expect
                                                                     // Corrupt the second entry (somewhere in the middle of the file)
                                                                     // Each entry is ~100 bytes. Let's flip a bit around offset 150.
            if data.len() > 150 {
                data[150] ^= 0xFF;
                fs::write(&wal_path, data).await.expect("write"); // expect
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
    async fn wal_tolerates_truncated_tail() {
        let dir = tempdir().expect("tempdir"); // expect
        let path = dir.path().join("test.wal");

        {
            let wal = Wal::open(&path).await.expect("open WAL"); // expect
            for i in 1..=4 {
                let op = WalOp::Put {
                    tx_id: TxId::new(i),
                    key: format!("key{}", i).into_bytes(),
                    value: format!("val{}", i).into_bytes(),
                };
                let entry = wal.create_entry(op, i).await.expect("create entry"); // expect
                wal.append(&entry).await.expect("append entry"); // expect
            }
        }

        // Truncate file in the middle of 4th entry
        let mut data = fs::read(&path).await.expect("read wal"); // expect
        let truncated_len = data.len() - 10;
        data.truncate(truncated_len);
        fs::write(&path, data).await.expect("write truncated wal"); // expect

        let wal2 = Wal::open(&path).await.expect("reopen WAL"); // expect
        let entries = wal2.replay().await.expect("replay WAL"); // expect

        assert_eq!(
            entries.len(),
            3,
            "Replay must return exactly 3 valid entries without error"
        );
    }

    #[tokio::test]
    async fn test_wal_crc_tail_corruption() {
        let dir = tempdir().expect("tempdir"); // expect
        let wal_path = dir.path().join("tail_corrupt.log");

        {
            let wal = Wal::open(&wal_path).await.expect("open"); // expect
            for i in 0..2 {
                let op = WalOp::Put {
                    tx_id: TxId::new(i),
                    key: format!("k{}", i).into_bytes(),
                    value: format!("v{}", i).into_bytes(),
                };
                let entry = wal.create_entry(op, i).await.expect("entry"); // expect
                wal.append(&entry).await.expect("append"); // expect
            }
        }

        {
            let mut file = fs::OpenOptions::new()
                .append(true)
                .open(&wal_path)
                .await
                .expect("open"); // expect
            use tokio::io::AsyncWriteExt;
            // Append some garbage that doesn't form a valid entry
            file.write_all(b"SOME GARBAGE DATA AT THE END")
                .await
                .expect("write"); // expect
        }

        let wal2 = Wal::open(&wal_path).await.expect("open"); // expect
        let entries = wal2.replay().await.expect("replay"); // expect

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
        let dummy_key = b"test-integrity-key-32-bytes-long!";
        let entry = WalEntry::try_new(op, 1, dummy_key, [0u8; 32]).expect("try_new"); // expect

        let mut bytes = entry.to_bytes().expect("serialization failed"); // expect

        // Let's corrupt the payload which is after the length prefix(4) and CRC(4)
        if bytes.len() > 10 {
            bytes[10] ^= 0xFF;
        }

        // Check using from_bytes (skipping the length prefix at the start)
        let result = WalEntry::from_bytes(&bytes[4..]);
        assert!(result.is_err(), "Corruption must be detected by CRC check");
        let err = result.unwrap_err();
        assert!(format!("{}", err).contains("CRC mismatch"));
    }

    #[tokio::test]
    async fn test_wal_header_systematic_fuzzing() {
        let dir = tempdir().expect("tempdir"); // expect
        let wal_path = dir.path().join("fuzz.log");

        // 1. Erstelle eine valide WAL-Datei mit einem Eintrag
        let original_data = {
            let wal = Wal::open(&wal_path).await.expect("open"); // expect
            let op = WalOp::Put {
                tx_id: TxId::new(1),
                key: b"k".to_vec(),
                value: b"v".to_vec(),
            };
            let entry = wal.create_entry(op, 1).await.expect("entry"); // expect
            wal.append(&entry).await.expect("append"); // expect
            drop(wal);
            fs::read(&wal_path).await.expect("read") // expect
        };

        // 2. Systematisch jedes Bit der ersten 12 Bytes der DATEI flippen
        // Bytes 0-3: Length Prefix
        // Bytes 4-7: CRC32
        // Bytes 8-11: Anfang von seq_no (u64)
        for byte_idx in 0..12 {
            for bit_idx in 0..8 {
                let mut corrupted_data = original_data.clone();
                corrupted_data[byte_idx] ^= 1 << bit_idx;
                fs::write(&wal_path, &corrupted_data).await.expect("write"); // expect

                let result = Wal::open(&wal_path).await;

                match result {
                    Ok(wal) => {
                        // Wenn open erfolgreich ist, muss replay den Fehler finden
                        let replay_result = wal.replay().await;
                        assert!(
                            replay_result.is_err() || replay_result.unwrap().is_empty(), // unwrap
                            "Corruption at byte {}, bit {} was NOT detected during replay!",
                            byte_idx,
                            bit_idx
                        );
                    }
                    Err(e) => {
                        // Fehler beim Öffnen/Initial-Replay ist auch okay, solange es keine Panic ist
                        assert!(
                            matches!(
                                e,
                                MemFuseError::Serialization(_)
                                    | MemFuseError::WalCorruption { .. }
                                    | MemFuseError::Storage(_)
                            ),
                            "Unexpected error type at byte {}, bit {}: {:?}",
                            byte_idx,
                            bit_idx,
                            e
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn test_wal_entry_header_fuzzing() {
        let op = WalOp::Put {
            tx_id: TxId::new(1),
            key: b"k".to_vec(),
            value: b"v".to_vec(),
        };
        let integrity_key = b"test-integrity-key-32-bytes-long!";
        let entry = WalEntry::try_new(op, 12345, integrity_key, [0u8; 32]).expect("try_new"); // expect

        let original_bytes = entry.to_bytes().expect("serialization failed"); // expect

        // Systematisch jedes Bit der ersten 12 Bytes flippen
        for byte_idx in 0..12 {
            for bit_idx in 0..8 {
                let mut corrupted_bytes = original_bytes.clone();
                corrupted_bytes[byte_idx] ^= 1 << bit_idx;

                // Testverhalten unterscheidet sich je nach Position
                if byte_idx < 4 {
                    // Length prefix corrupted.
                    // Das wird normalerweise von Wal::replay abgefangen,
                    // aber from_bytes kriegt hier nur den Teil ab Index 4.
                    // Wenn wir bytes[0..4] flippen, ändert das für from_bytes(&bytes[4..]) nichts.
                    let result = WalEntry::from_bytes(&corrupted_bytes[4..]);
                    assert!(
                        result.is_ok(),
                        "Flipping bytes[0..4] should not affect from_bytes(bytes[4..])"
                    );
                } else {
                    // CRC (4-7) oder SeqNo (8-11) korrumpiert.
                    // Das MUSS von from_bytes erkannt werden.
                    let result = WalEntry::from_bytes(&corrupted_bytes[4..]);
                    assert!(
                        result.is_err(),
                        "Corruption at byte {}, bit {} was NOT detected! result: {:?}",
                        byte_idx,
                        bit_idx,
                        result
                    );
                }
            }
        }
    }

    #[tokio::test]
    async fn test_wal_random_integrity_keys_per_instance() {
        let dir1 = tempdir().expect("tempdir1"); // expect
        let dir2 = tempdir().expect("tempdir2"); // expect
        let wal_path1 = dir1.path().join("wal1.log");
        let wal_path2 = dir2.path().join("wal2.log");

        let wal1 = Wal::open(&wal_path1).await.expect("open wal1"); // expect
        let wal2 = Wal::open(&wal_path2).await.expect("open wal2"); // expect

        let key1 = wal1.get_integrity_key().expect("key1"); // expect
        let key2 = wal2.get_integrity_key().expect("key2"); // expect

        assert_ne!(
            key1, key2,
            "Two independent WAL instances must receive unique random integrity keys"
        );
    }

    #[tokio::test]
    async fn test_wal_tampered_wrong_key_entry_detected() {
        let dir = tempdir().expect("tempdir"); // expect
        let wal_path = dir.path().join("tamper_wal.log");

        let valid_op = WalOp::Put {
            tx_id: TxId::new(1),
            key: b"secure_key".to_vec(),
            value: b"secure_val".to_vec(),
        };

        {
            let wal = Wal::open(&wal_path).await.expect("open wal"); // expect
            let entry = wal
                .create_entry(valid_op.clone(), 1)
                .await
                .expect("create entry"); // expect
            wal.append(&entry).await.expect("append valid entry"); // expect
        }

        {
            // Inject an entry forged with an arbitrary wrong key
            let wrong_key = b"wrong-attacker-integrity-key-32!";
            let forged_op = WalOp::Put {
                tx_id: TxId::new(2),
                key: b"forged_key".to_vec(),
                value: b"forged_val".to_vec(),
            };
            // Previous HMAC is the valid entry's HMAC, but key is wrong
            let last_valid_entry = Wal::open(&wal_path)
                .await
                .expect("open") // expect
                .replay()
                .await
                .expect("replay")[0] // expect
                .1
                .clone();

            let forged_entry =
                WalEntry::try_new(forged_op, 2, wrong_key, last_valid_entry.checksum)
                    .expect("create forged entry"); // expect

            // Also append a 3rd entry so the forged entry is in the middle of the file (pos < file_size)
            let trailing_entry = WalEntry::try_new(
                WalOp::Put {
                    tx_id: TxId::new(3),
                    key: b"trailing".to_vec(),
                    value: b"val".to_vec(),
                },
                3,
                wrong_key,
                forged_entry.checksum,
            )
            .expect("create trailing entry"); // expect

            let mut file = tokio::fs::OpenOptions::new()
                .append(true)
                .open(&wal_path)
                .await
                .expect("open file for append"); // expect
            file.write_all(&forged_entry.to_bytes().expect("to_bytes")) // expect
                .await
                .expect("write forged entry"); // expect
            file.write_all(&trailing_entry.to_bytes().expect("to_bytes")) // expect
                .await
                .expect("write trailing entry"); // expect
        }

        let wal_reopen = Wal::open(&wal_path).await;
        assert!(
            wal_reopen.is_err() || wal_reopen.unwrap().replay().await.is_err(), // unwrap
            "Replaying a WAL with a wrong-key forged entry must fail HMAC verification"
        );
    }

    #[tokio::test]
    async fn test_wal_legacy_key_fallback_migration() {
        let dir = tempdir().expect("tempdir"); // expect
        let wal_path = dir.path().join("legacy_wal.log");

        {
            // Manually construct a WAL entry with the legacy static integrity key
            let op = WalOp::Put {
                tx_id: TxId::new(1),
                key: b"legacy_key".to_vec(),
                value: b"legacy_val".to_vec(),
            };
            let legacy_entry =
                WalEntry::try_new(op, 1, &LEGACY_INTEGRITY_KEY, [0u8; 32]).expect("legacy entry"); // expect

            tokio::fs::write(&wal_path, legacy_entry.to_bytes().expect("to_bytes")) // expect
                .await
                .expect("write legacy WAL"); // expect
        }

        // Opening and replaying should fallback to LEGACY_INTEGRITY_KEY and succeed
        let wal = Wal::open(&wal_path).await.expect("open legacy wal"); // expect
        let entries = wal.replay().await.expect("replay legacy wal"); // expect
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].1.seq_no, 1);
        if let WalOp::Put { key, value, .. } = &entries[0].1.op {
            assert_eq!(key, b"legacy_key");
            assert_eq!(value, b"legacy_val");
        } else {
            panic!("Expected Put op");
        }
    }

    #[test]
    fn test_wal_entry_crc_roundtrip() {
        let op = WalOp::Put {
            tx_id: TxId::new(42),
            key: b"test_key".to_vec(),
            value: b"test_value".to_vec(),
        };
        let dummy_key = b"test-integrity-key-32-bytes-long!";
        let entry = WalEntry::try_new(op, 100, dummy_key, [0u8; 32]).expect("try_new"); // expect

        let bytes = entry.to_bytes().expect("serialization failed"); // expect
        let decoded = WalEntry::from_bytes(&bytes[4..]).expect("Roundtrip must work"); // expect

        assert_eq!(decoded.seq_no, 100);
        if let WalOp::Put { key, value, .. } = decoded.op {
            assert_eq!(key, b"test_key");
            assert_eq!(value, b"test_value");
        } else {
            panic!("Wrong op type");
        }
    }

    #[tokio::test]
    async fn test_wal_crash_consistency_write_without_fsync() {
        let dir = tempdir().expect("tempdir"); // expect
        let wal_path = dir.path().join("crash_sim.wal");

        // 1. Open WAL and append an entry
        {
            let wal = Wal::open(&wal_path).await.expect("open wal"); // expect
            let op = WalOp::Put {
                tx_id: TxId::new(100),
                key: b"crash_k".to_vec(),
                value: b"crash_v".to_vec(),
            };
            let entry = wal.create_entry(op, 1).await.expect("create entry"); // expect

            // Manually simulate a write + flush to OS buffer WITHOUT file.sync_all()
            let mut file = tokio::fs::OpenOptions::new()
                .append(true)
                .open(&wal_path)
                .await
                .expect("open for append"); // expect
            let bytes = entry.to_bytes().expect("to_bytes"); // expect
            file.write_all(&bytes).await.expect("write_all"); // expect
            file.flush().await.expect("flush"); // expect
                                                // File dropped without calling sync_all() (simulating crash before fsync)
            drop(file);
            drop(wal);
        }

        // 2. Re-open WAL and replay
        let wal_reopen = Wal::open(&wal_path).await;
        assert!(wal_reopen.is_ok(), "WAL open after crash should succeed");
        let wal = wal_reopen.unwrap(); // unwrap

        let replay_result = wal.replay().await;
        match replay_result {
            Ok(entries) => {
                // Should either find the entry or empty set, never panic
                if !entries.is_empty() {
                    assert_eq!(entries.len(), 1);
                    assert_eq!(entries[0].1.seq_no, 1);
                }
            }
            Err(e) => {
                panic!("replay() failed unexpectedly with error: {:?}", e);
            }
        }
    }

    #[tokio::test]
    async fn test_append_batch_partial_write_atomicity() {
        let dir = tempdir().expect("tempdir"); // expect
        let wal_path = dir.path().join("partial_batch.wal");

        let wal = Wal::open(&wal_path).await.expect("open wal"); // expect
        let ops = vec![
            (
                WalOp::Put {
                    tx_id: TxId::new(1),
                    key: b"b1".to_vec(),
                    value: b"v1".to_vec(),
                },
                1,
            ),
            (
                WalOp::Put {
                    tx_id: TxId::new(1),
                    key: b"b2".to_vec(),
                    value: b"v2".to_vec(),
                },
                2,
            ),
            (
                WalOp::Put {
                    tx_id: TxId::new(1),
                    key: b"b3".to_vec(),
                    value: b"v3".to_vec(),
                },
                3,
            ),
        ];

        let entries = wal.prepare_batch(ops).await.expect("prepare_batch"); // expect
        assert_eq!(entries.len(), 3);

        // Serialize all 3 entries into a single bytes payload
        let mut batch_bytes = Vec::new();
        for e in &entries {
            batch_bytes.extend_from_slice(&e.to_bytes().expect("to_bytes")); // expect
        }

        // Truncate the batch in the middle of entry 2 (partial write during crash)
        // Each entry is ~101 bytes. Total ~303 bytes.
        // Subtracting 120 bytes leaves ~183 bytes, truncating entry 2 mid-write.
        let truncated_len = batch_bytes.len() - 120;
        let truncated_bytes = &batch_bytes[..truncated_len];

        // Append the truncated bytes directly to the WAL file
        {
            let mut file = tokio::fs::OpenOptions::new()
                .append(true)
                .open(&wal_path)
                .await
                .expect("open for append"); // expect
            file.write_all(truncated_bytes).await.expect("write_all"); // expect
            file.flush().await.expect("flush"); // expect
        }

        // Reopen and replay
        let wal2 = Wal::open(&wal_path).await.expect("reopen"); // expect
        let replay_entries = wal2
            .replay()
            .await
            .expect("replay must succeed without panic"); // expect

        // Replay must recover entry 1 (which was fully written) and cleanly discard the truncated tail
        assert_eq!(replay_entries.len(), 1, "Only entry 1 should be recovered");
        assert_eq!(replay_entries[0].1.seq_no, 1);
    }

    #[tokio::test]
    async fn test_batch_encryption_single_nonce_layout() {
        let dir = tempdir().expect("tempdir"); // expect
        let wal_path = dir.path().join("single_nonce_test.wal");

        let km = Arc::new(
            KeyManager::try_new("test_passphrase", b"salt123456789012345678901234567890")
                .expect("km"), // expect
        );
        let wal = Wal::open_with_key_manager(&wal_path, Some(km))
            .await
            .expect("open wal"); // expect

        let ops = vec![
            (
                WalOp::Put {
                    tx_id: TxId::new(1),
                    key: b"k1".to_vec(),
                    value: b"v1".to_vec(),
                },
                100,
            ),
            (
                WalOp::Put {
                    tx_id: TxId::new(1),
                    key: b"k2".to_vec(),
                    value: b"v2".to_vec(),
                },
                101,
            ),
            (
                WalOp::Put {
                    tx_id: TxId::new(1),
                    key: b"k3".to_vec(),
                    value: b"v3".to_vec(),
                },
                102,
            ),
        ];

        let batch = wal.prepare_batch(ops).await.expect("prepare batch"); // expect
        assert_eq!(batch.len(), 3);

        wal.append_batch(&batch).await.expect("append batch"); // expect

        let file_bytes = fs::read(&wal_path).await.expect("read wal file"); // expect

        // Layout:
        // Offset 0..4: WAL_V3_HEADER (b"MFW3")
        // Offset 4..8: batch chunk_len (u32 LE)
        // Offset 8..20: single 12-byte nonce
        // Offset 20..: AES-GCM-SIV ciphertext
        assert_eq!(&file_bytes[0..4], &WAL_V3_HEADER);
        let chunk_len = u32::from_le_bytes(file_bytes[4..8].try_into().unwrap()) as usize; // unwrap
        assert_eq!(file_bytes.len(), 4 + 4 + chunk_len);

        // Verify there is exactly one batch chunk header (12-byte nonce) in the file for N=3 entries
        let nonce_bytes = &file_bytes[8..20];
        assert_eq!(nonce_bytes.len(), 12);
    }

    #[tokio::test]
    async fn test_batch_encrypted_wal_roundtrip() {
        let dir = tempdir().expect("tempdir"); // expect
        let wal_path = dir.path().join("roundtrip_test.wal");

        let km = Arc::new(
            KeyManager::try_new("passphrase123", b"salt123456789012345678901234567890")
                .expect("km"), // expect
        );
        let wal = Wal::open_with_key_manager(&wal_path, Some(km.clone()))
            .await
            .expect("open wal"); // expect

        let ops = vec![
            (
                WalOp::Put {
                    tx_id: TxId::new(1),
                    key: b"alice_key".to_vec(),
                    value: b"alice_value".to_vec(),
                },
                1,
            ),
            (
                WalOp::Put {
                    tx_id: TxId::new(2),
                    key: b"bob_key".to_vec(),
                    value: b"bob_value".to_vec(),
                },
                2,
            ),
            (
                WalOp::Delete {
                    tx_id: TxId::new(3),
                    key: b"alice_key".to_vec(),
                },
                3,
            ),
        ];

        let batch = wal.prepare_batch(ops).await.expect("prepare_batch"); // expect
        wal.append_batch(&batch).await.expect("append_batch"); // expect

        let wal_reopen = Wal::open_with_key_manager(&wal_path, Some(km))
            .await
            .expect("reopen wal"); // expect
        let replayed = wal_reopen.replay().await.expect("replay"); // expect

        assert_eq!(replayed.len(), 3);
        assert_eq!(replayed[0].1.seq_no, 1);
        assert_eq!(replayed[1].1.seq_no, 2);
        assert_eq!(replayed[2].1.seq_no, 3);

        if let WalOp::Put { key, value, tx_id } = &replayed[0].1.op {
            assert_eq!(key, b"alice_key");
            assert_eq!(value, b"alice_value");
            assert_eq!(*tx_id, TxId::new(1));
        } else {
            panic!("Expected Put op");
        }

        if let WalOp::Delete { key, tx_id } = &replayed[2].1.op {
            assert_eq!(key, b"alice_key");
            assert_eq!(*tx_id, TxId::new(3));
        } else {
            panic!("Expected Delete op");
        }
    }

    #[tokio::test]
    async fn test_old_v1_format_backward_compatibility() {
        let dir = tempdir().expect("tempdir"); // expect
        let wal_path = dir.path().join("v1_legacy_format.wal");

        let km = Arc::new(
            KeyManager::try_new("legacy_passphrase", b"salt123456789012345678901234567890")
                .expect("km"), // expect
        );

        // Derive sub-key for file ID (same derivation Wal::open_with_key_manager does)
        let uuid_bytes = Wal::load_or_create_wal_uuid(&wal_path).await.expect("uuid"); // expect
        let sub_km = km.derive_file_key(&uuid_bytes).expect("derive file key"); // expect

        // Manually construct an old V1 encrypted WAL file (no MFW2 header, each entry encrypted separately)
        let integrity_key = sub_km.integrity_key().expect("integrity key"); // expect

        let op1 = WalOp::Put {
            tx_id: TxId::new(10),
            key: b"legacy_k1".to_vec(),
            value: b"legacy_v1".to_vec(),
        };
        let entry1 = WalEntry::try_new(op1, 100, &integrity_key, [0u8; 32]).expect("entry1"); // expect
        let bytes1 = entry1.to_bytes().expect("bytes1"); // expect

        let payload1 = &bytes1[4..];
        let (encrypted1, nonce1) = sub_km.encrypt_auto_nonce(payload1).expect("enc1"); // expect

        let mut v1_file_data = Vec::new();
        let chunk_len1 = (12 + encrypted1.len()) as u32;
        v1_file_data.extend_from_slice(&chunk_len1.to_le_bytes());
        v1_file_data.extend_from_slice(&nonce1);
        v1_file_data.extend_from_slice(&encrypted1);

        let op2 = WalOp::Put {
            tx_id: TxId::new(11),
            key: b"legacy_k2".to_vec(),
            value: b"legacy_v2".to_vec(),
        };
        let entry2 = WalEntry::try_new(op2, 101, &integrity_key, entry1.checksum).expect("entry2"); // expect
        let bytes2 = entry2.to_bytes().expect("bytes2"); // expect

        let payload2 = &bytes2[4..];
        let (encrypted2, nonce2) = sub_km.encrypt_auto_nonce(payload2).expect("enc2"); // expect

        let chunk_len2 = (12 + encrypted2.len()) as u32;
        v1_file_data.extend_from_slice(&chunk_len2.to_le_bytes());
        v1_file_data.extend_from_slice(&nonce2);
        v1_file_data.extend_from_slice(&encrypted2);

        fs::write(&wal_path, &v1_file_data)
            .await
            .expect("write v1 wal"); // expect

        // Reopen via standard Wal::open_with_key_manager and replay
        let wal = Wal::open_with_key_manager(&wal_path, Some(km))
            .await
            .expect("open v1 wal"); // expect
        let replayed = wal.replay().await.expect("replay v1 wal"); // expect

        assert_eq!(
            replayed.len(),
            2,
            "Both V1 entries must be replayed correctly"
        );
        assert_eq!(replayed[0].1.seq_no, 100);
        assert_eq!(replayed[1].1.seq_no, 101);
        assert_eq!(replayed[1].1.prev_hmac, replayed[0].1.checksum);
    }

    #[tokio::test]
    async fn test_batch_encrypted_wal_truncation_crash_consistency() {
        let dir = tempdir().expect("tempdir"); // expect
        let wal_path = dir.path().join("batch_truncation.wal");

        let km = Arc::new(
            KeyManager::try_new("passphrase123", b"salt123456789012345678901234567890")
                .expect("km"), // expect
        );

        {
            let wal = Wal::open_with_key_manager(&wal_path, Some(km.clone()))
                .await
                .expect("open wal"); // expect

            // Batch 1: 2 entries
            let ops1 = vec![
                (
                    WalOp::Put {
                        tx_id: TxId::new(1),
                        key: b"k1".to_vec(),
                        value: b"v1".to_vec(),
                    },
                    1,
                ),
                (
                    WalOp::Put {
                        tx_id: TxId::new(1),
                        key: b"k2".to_vec(),
                        value: b"v2".to_vec(),
                    },
                    2,
                ),
            ];
            let batch1 = wal.prepare_batch(ops1).await.expect("prepare 1"); // expect
            wal.append_batch(&batch1).await.expect("append 1"); // expect

            // Batch 2: 2 entries
            let ops2 = vec![
                (
                    WalOp::Put {
                        tx_id: TxId::new(2),
                        key: b"k3".to_vec(),
                        value: b"v3".to_vec(),
                    },
                    3,
                ),
                (
                    WalOp::Put {
                        tx_id: TxId::new(2),
                        key: b"k4".to_vec(),
                        value: b"v4".to_vec(),
                    },
                    4,
                ),
            ];
            let batch2 = wal.prepare_batch(ops2).await.expect("prepare 2"); // expect
            wal.append_batch(&batch2).await.expect("append 2"); // expect
        }

        // Truncate the file mid-ciphertext of Batch 2
        let mut data = fs::read(&wal_path).await.expect("read wal"); // expect
        let truncated_len = data.len() - 15; // chop off 15 bytes from Batch 2's ciphertext
        data.truncate(truncated_len);
        fs::write(&wal_path, &data)
            .await
            .expect("write truncated wal"); // expect

        // Reopen and replay
        let wal2 = Wal::open_with_key_manager(&wal_path, Some(km))
            .await
            .expect("reopen wal"); // expect
        let replayed = wal2
            .replay()
            .await
            .expect("replay must succeed by recovering Batch 1"); // expect

        assert_eq!(
            replayed.len(),
            2,
            "Batch 1 (2 entries) must be recovered, Batch 2 truncated"
        );
        assert_eq!(replayed[0].1.seq_no, 1);
        assert_eq!(replayed[1].1.seq_no, 2);
    }

    #[tokio::test]
    async fn test_integrity_key_atomic_permissions_and_race_condition() {
        let temp = tempfile::tempdir().expect("tempdir");
        let wal_path = temp.path().join("test.wal");

        // Test 1: Created key file has 0o600 permissions on Unix
        let key1 = Wal::load_or_create_integrity_key(&wal_path)
            .await
            .expect("create key");

        let key_path = temp.path().join(".wal_integrity_key");
        assert!(key_path.exists(), "Key file must exist");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::metadata(&key_path).expect("metadata");
            let mode = metadata.permissions().mode() & 0o777;
            assert_eq!(
                mode, 0o600,
                "WAL integrity key file must have permissions 0o600 on Unix, got 0o{:o}",
                mode
            );
        }

        // Test 2: Race condition simulation with multiple concurrent callers
        let wal_path_race = temp.path().join("race.wal");
        let mut handles = Vec::new();
        for _ in 0..10 {
            let path = wal_path_race.clone();
            handles.push(tokio::spawn(async move {
                Wal::load_or_create_integrity_key(&path).await
            }));
        }

        let mut keys = Vec::new();
        for h in handles {
            let res = h.await.expect("join handle").expect("load key");
            keys.push(res);
        }

        for k in &keys {
            assert_eq!(
                k, &keys[0],
                "All concurrent tasks must receive the identical key"
            );
        }
        assert_eq!(key1.len(), 32);
    }

    #[test]
    fn test_wal_op_from_bytes_oversized_key_val() {
        // Construct payload with key_len > 1MB
        let mut payload = vec![0u8; 90];
        // op_type = 0 (Put) at index 72
        payload[72] = 0;
        // tx_id = 1
        payload[73..81].copy_from_slice(&1u64.to_le_bytes());
        // key_len = 2 MB
        payload[81..85].copy_from_slice(&(2 * 1024 * 1024u32).to_le_bytes());

        let crc = crc32fast::hash(&payload);
        let mut data = vec![0u8; 4];
        data[0..4].copy_from_slice(&crc.to_le_bytes());
        data.extend_from_slice(&payload);

        let res = WalEntry::from_bytes(&data);
        assert!(res.is_err());
        if let Err(MemFuseError::Serialization(msg)) = res {
            assert!(msg.contains("key_len exceeds 1 MiB limit"));
        } else {
            panic!("Expected Serialization error for key_len limit");
        }
    }
}
