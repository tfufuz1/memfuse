//! Write-Ahead Log (WAL) for durability and crash recovery with HMAC chaining.
// FILE-CONTEXT
// STAND: 2026-08-30T21:49:55Z (SESSION: 283abf0f)
// ZWECK: Write-Ahead-Log mit HMAC-Chaining für crash-sichere WAL-Operationen
// INVARIANTEN: fsync NACH jedem Schreibvorgang (ADR-002); WAL VOR MemTable schreiben
// NICHT-OFFENSICHTLICH: sync_all() auf dem Verzeichnis-FD nötig, nicht nur auf der Datei
// SIEHE AUCH: rules/tag_taxonomy.md, DECISIONS.md ADR-002

use memfuse_core::{MemFuseError, Result, TxId};
use memfuse_security::crypto::KeyManager;
use memfuse_security::wal_crypto::{IntegrityVerifier, WalEntrySnapshot, WalHmac};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WalVersion {
    V1, // Legacy: kein HMAC
    V2, // Current: HMAC ohne tx_id
    V3, // New: HMAC mit tx_id
}

/// Obfuscated legacy static HMAC integrity key used strictly for backward-compatibility fallback during WAL replay of legacy databases.
///
/// Cryptographic Audit Guarantee (Task E):
/// 1. This key is ONLY used during replay of pre-migration WAL files when per-file key verification fails.
/// 2. It is NEVER used for new write or append operations (all new WAL writes derive an integrity key via `KeyManager`).
/// 3. After successful replay and LSM compaction into SSTables, old WAL files using `legacy_integrity_key()` are superseded and truncated/removed.
///
/// ANCHOR[MIGRATION:WAL-HMAC-001] STATUS:DONE (TS:2026-06-01T00:00:00Z)
const LEGACY_KEY_OBFUSCATION_MASK: u8 = 0x5A;
const LEGACY_INTEGRITY_KEY_OBFUSCATED: [u8; 32] = *b"7?7</)?w34.?=(3.#w1?#w,kZZZZZZZZ";

/// SICHERHEITSHINWEIS: Dieser Schlüssel bietet KEINE kryptografische Sicherheit.
/// Die "Obfuskierung" (XOR mit einer fest im Binary kodierten Maske) ist trivial
/// aus jedem Release-Binary rekonstruierbar und dient ausschließlich dazu, den
/// Klartext-Schlüssel nicht direkt als grep-bares ASCII im Binary sichtbar zu
/// machen (Schutz gegen oberflächliches Scannen, NICHT gegen gezielte Extraktion).
/// `allow_legacy_integrity_key_fallback = true` darf NUR für befristete
/// Migrationsszenarien aktiviert werden und NIEMALS dauerhaft in produktiven
/// Umgebungen aktiv bleiben.
pub(crate) const fn legacy_integrity_key() -> [u8; 32] {
    let mut out = [0u8; 32];
    let mut i = 0;
    while i < 32 {
        out[i] = LEGACY_INTEGRITY_KEY_OBFUSCATED[i] ^ LEGACY_KEY_OBFUSCATION_MASK;
        i += 1;
    }
    out
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
                        return Err(MemFuseError::Serialization(
                            "key_len exceeds 1 MiB limit".into(),
                        ));
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
                        return Err(MemFuseError::Serialization(
                            "val_len exceeds 128 MiB limit".into(),
                        ));
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
                        return Err(MemFuseError::Serialization(
                            "key_len exceeds 1 MiB limit".into(),
                        ));
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

/// Configuration options for opening a Write-Ahead Log.
///
/// **Security Notice**: For production deployments, `min_wal_version` should be explicitly set
/// to `WalVersion::V3` to enforce HMAC verification and disable unauthenticated legacy WAL formats.
#[derive(Debug, Clone)]
pub struct WalConfig {
    pub key_manager: Option<Arc<KeyManager>>,
    /// SICHERHEITSHINWEIS: Dieser Schlüssel bietet KEINE kryptografische Sicherheit.
    /// Die "Obfuskierung" (XOR mit einer fest im Binary kodierten Maske) ist trivial
    /// aus jedem Release-Binary rekonstruierbar und dient ausschließlich dazu, den
    /// Klartext-Schlüssel nicht direkt als grep-bares ASCII im Binary sichtbar zu
    /// machen (Schutz gegen oberflächliches Scannen, NICHT gegen gezielte Extraktion).
    /// `allow_legacy_integrity_key_fallback = true` darf NUR für befristete
    /// Migrationsszenarien aktiviert werden und NIEMALS dauerhaft in produktiven
    /// Umgebungen aktiv bleiben.
    pub allow_legacy_integrity_key_fallback: bool,
    /// Minimum allowed WAL version for replay. WAL files with a version below
    /// this minimum will be automatically migrated to V3 and backed up (`.v1.bak`).
    ///
    /// Default: `WalVersion::V1` for backward compatibility. Production deployments SHOULD set `WalVersion::V3`.
    // TODO(audit-M-8): Reject unencrypted V1 plaintext entries during replay when KeyManager is present and active.
    pub min_wal_version: WalVersion,
}

impl Default for WalConfig {
    fn default() -> Self {
        Self {
            key_manager: None,
            allow_legacy_integrity_key_fallback: false,
            min_wal_version: WalVersion::V1,
        }
    }
}

/// Write-Ahead Log for crash recovery.
pub struct Wal {
    path: PathBuf,
    pub(crate) file: tokio::sync::Mutex<tokio::fs::File>,
    size: std::sync::atomic::AtomicU64,
    header_written: std::sync::atomic::AtomicBool,
    key_manager: Option<Arc<KeyManager>>,
    fallback_integrity_key: Option<[u8; 32]>,
    allow_legacy_integrity_key_fallback: bool,
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

/// Global fault injection flag to simulate a WAL `append_batch` failure for a specific transaction ID during tests.
#[cfg(feature = "fault-injection")]
pub static FAIL_APPEND_FOR_TX: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

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
        Self::open_with_config(path, WalConfig::default()).await
    }

    /// Opens or creates a WAL file with an optional KeyManager.
    pub async fn open_with_key_manager(
        path: impl AsRef<Path>,
        key_manager: Option<Arc<KeyManager>>,
    ) -> Result<Self> {
        Self::open_with_config(
            path,
            WalConfig {
                key_manager,
                allow_legacy_integrity_key_fallback: false,
                min_wal_version: WalVersion::V1,
            },
        )
        .await
    }

    /// Opens or creates a WAL file with explicit configuration options.
    pub async fn open_with_config(path: impl AsRef<Path>, config: WalConfig) -> Result<Self> {
        let path = path.as_ref().to_path_buf();

        // Vor dem eigentlichen Öffnen der WAL-Datei: prüfen, ob ein Crash-Recovery aus
        // einem .bak-Backup nötig ist (z. B. Crash zwischen set_len(0) und V3-Rewrite).
        let _ = recover_from_bak_if_present(&path).await?;

        // SD-09-CRYPTO-002: Use a persisted UUID v4 as file_id instead of the
        // filename.  This makes the WAL's cryptographic sub-key independent of
        // the filesystem path — renaming or moving the file cannot cause nonce-
        // reuse between two WAL instances sharing the same master key.
        let (derived_key_manager, fallback_integrity_key) = if let Some(km) = config.key_manager {
            let uuid_bytes = Self::load_or_create_wal_uuid(&path).await?;
            (Some(Arc::new(km.derive_file_key(&uuid_bytes)?)), None)
        } else {
            let key = Self::load_or_create_integrity_key(&path).await?;
            (None, Some(key))
        };

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
                    .map_err(|e| MemFuseError::Storage(format!("Failed to open WAL: {}", e)))?;
                (file, false)
            }
            Err(e) => {
                return Err(MemFuseError::Storage(format!(
                    "Failed to create WAL: {}",
                    e
                )));
            }
        };

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
            header_written: std::sync::atomic::AtomicBool::new(metadata.len() > 0),
            file: tokio::sync::Mutex::new(file),
            key_manager: derived_key_manager,
            fallback_integrity_key,
            allow_legacy_integrity_key_fallback: config.allow_legacy_integrity_key_fallback,
            last_hmac: tokio::sync::Mutex::new([0u8; 32]),
        };

        // If file is not empty, find the last valid HMAC to continue the chain
        if metadata.len() > 0 {
            let (entries, version) = wal.replay_with_size_and_version(metadata.len()).await?;
            if version < config.min_wal_version || version != WalVersion::V3 {
                tracing::info!(
                    "WAL {:?} format detected at {:?}. Will be rewritten as V3 after successful replay.",
                    version,
                    wal.path
                );
                let bak_suffix = match version {
                    WalVersion::V1 => "v1.bak",
                    WalVersion::V2 => "v2.bak",
                    WalVersion::V3 => "v3.bak",
                };
                let bak_path = PathBuf::from(format!("{}.{}", wal.path.display(), bak_suffix));
                let copy_res = tokio::fs::copy(&wal.path, &bak_path).await;
                if copy_res.is_ok() {
                    // Backup-Datei fsyncen: Recovery-Sicherheit VOR der Truncation der Original-WAL.
                    match tokio::fs::OpenOptions::new().write(true).open(&bak_path).await {
                        Ok(bak_file) => {
                            if let Err(e) = bak_file.sync_all().await {
                                tracing::warn!("WAL backup fsync failed before rewrite: {e}");
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Could not reopen WAL backup for fsync: {e}");
                        }
                    }
                }
                let rewrite_res = wal.rewrite_as_v3(&entries).await;

                if copy_res.is_err() || rewrite_res.is_err() {
                    if config.min_wal_version > WalVersion::V1 || version < config.min_wal_version {
                        let err_msg = match (copy_res, rewrite_res) {
                            (Err(e), _) => {
                                format!("Failed to create backup copy {:?}: {}", bak_path, e)
                            }
                            (_, Err(e)) => format!("Failed to rewrite WAL as V3: {}", e),
                            (Ok(_), Ok(_)) => unreachable!(),
                        };
                        return Err(MemFuseError::invalid_input(format!(
                            "Configuration error: WAL version {:?} is below min_wal_version {:?} and migration failed: {}",
                            version, config.min_wal_version, err_msg
                        )));
                    } else {
                        rewrite_res?;
                    }
                }
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
}

/// Prüft, ob eine `.bak`-Datei (v1.bak, v2.bak) als Recovery-Quelle für `wal_path`
/// herangezogen werden sollte, und führt die Wiederherstellung ggf. durch.
///
/// Rückgabe `Ok(true)`: Recovery wurde durchgeführt (Backup wurde an die Stelle der
/// regulären WAL-Datei verschoben und gefsynct).
/// Rückgabe `Ok(false)`: Kein Recovery nötig oder kein passendes Backup gefunden.
pub(crate) async fn recover_from_bak_if_present(wal_path: &std::path::Path) -> Result<bool> {
    for suffix in &["v1.bak", "v2.bak"] {
        let bak_path = PathBuf::from(format!("{}.{}", wal_path.display(), suffix));
        if !tokio::fs::try_exists(&bak_path).await.unwrap_or(false) {
            continue;
        }
        let wal_len = tokio::fs::metadata(wal_path).await.map(|m| m.len()).unwrap_or(0);
        let bak_len = match tokio::fs::metadata(&bak_path).await {
            Ok(m) => m.len(),
            Err(_) => continue,
        };
        if wal_len == 0 && bak_len > 0 {
            // Reguläre WAL ist leer, aber ein nicht-leeres Backup existiert:
            // sehr wahrscheinlich ein Crash zwischen set_len(0) und dem Schreiben
            // des neuen V3-Contents. Backup wiederherstellen.
            match tokio::fs::rename(&bak_path, wal_path).await {
                Ok(()) => {}
                Err(_) => {
                    // Cross-Device-Fallback: copy + remove statt rename.
                    tokio::fs::copy(&bak_path, wal_path).await.map_err(|e| {
                        MemFuseError::Storage(format!(
                            "WAL backup recovery copy failed: {e}"
                        ))
                    })?;
                    let _ = tokio::fs::remove_file(&bak_path).await;
                }
            }
            if let Ok(f) = tokio::fs::OpenOptions::new().write(true).open(wal_path).await {
                f.sync_all().await.map_err(|e| {
                    MemFuseError::Storage(format!("WAL backup sync failed: {e}"))
                })?;
            }
            return Ok(true);
        }
    }
    Ok(false)
}

/// Configures restrictive Windows file ACL permissions for `.wal_integrity_key`.
///
/// Disables inherited ACLs from parent directories and grants full control (`GENERIC_ALL`)
/// strictly to the current process owner SID.
///
/// # Safety
/// This function calls Win32 security APIs (`OpenProcessToken`, `GetTokenInformation`,
/// `InitializeAcl`, `AddAccessAllowedAce`, `SetNamedSecurityInfoW`).
/// All raw pointers derived from heap buffers or Win32 structures are checked for non-nullness,
/// handles are freed with `CloseHandle`, and Win32 return codes are mapped to `MemFuseError::Storage`.
#[cfg(windows)]
#[allow(unsafe_code)]
fn set_restrictive_file_acl(path: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::null_mut;
    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, ERROR_SUCCESS, GENERIC_ALL, HANDLE,
    };
    use windows_sys::Win32::Security::Authorization::{SetNamedSecurityInfoW, SE_FILE_OBJECT};
    use windows_sys::Win32::Security::{
        AddAccessAllowedAce, GetLengthSid, GetTokenInformation, InitializeAcl, TokenUser,
        ACCESS_ALLOWED_ACE, ACL, ACL_REVISION, DACL_SECURITY_INFORMATION,
        PROTECTED_DACL_SECURITY_INFORMATION, TOKEN_QUERY, TOKEN_USER,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    let mut token_handle: HANDLE = std::ptr::null_mut();
    // SAFETY: GetCurrentProcess returns a pseudo-handle for the current process. OpenProcessToken initializes token_handle if successful.
    let res = unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token_handle) };
    if res == 0 {
        let err = unsafe { GetLastError() };
        return Err(MemFuseError::Storage(format!(
            "Failed to open process token for ACL restriction: Win32 error {}",
            err
        )));
    }

    struct TokenGuard(HANDLE);
    impl Drop for TokenGuard {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { CloseHandle(self.0) };
            }
        }
    }
    let _guard = TokenGuard(token_handle);

    let mut len = 0u32;
    // SAFETY: First call to GetTokenInformation determines required buffer size.
    unsafe {
        GetTokenInformation(token_handle, TokenUser, null_mut(), 0, &mut len);
    }

    if len == 0 {
        return Err(MemFuseError::Storage(
            "GetTokenInformation returned 0 buffer length for TokenUser".into(),
        ));
    }

    let mut buffer = vec![0u8; len as usize];
    // SAFETY: Passing allocated buffer of size `len` to receive TOKEN_USER struct and SID data.
    let res = unsafe {
        GetTokenInformation(
            token_handle,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            len,
            &mut len,
        )
    };
    if res == 0 {
        let err = unsafe { GetLastError() };
        return Err(MemFuseError::Storage(format!(
            "Failed to retrieve process owner SID: Win32 error {}",
            err
        )));
    }

    let token_user = buffer.as_ptr() as *const TOKEN_USER;
    let owner_sid = unsafe { (*token_user).User.Sid };
    if owner_sid.is_null() {
        return Err(MemFuseError::Storage(
            "Retrieved null owner SID from process token".into(),
        ));
    }

    let sid_len = unsafe { GetLengthSid(owner_sid) };
    let acl_size =
        std::mem::size_of::<ACL>() + std::mem::size_of::<ACCESS_ALLOWED_ACE>() + sid_len as usize;

    let mut acl_buf = vec![0u8; acl_size];
    let p_acl = acl_buf.as_mut_ptr() as *mut ACL;

    // SAFETY: Initializing ACL structure with valid allocated buffer size.
    if unsafe { InitializeAcl(p_acl, acl_size as u32, ACL_REVISION) } == 0 {
        let err = unsafe { GetLastError() };
        return Err(MemFuseError::Storage(format!(
            "Failed to initialize ACL: Win32 error {}",
            err
        )));
    }

    // SAFETY: Adding Access-Allowed ACE for the validated process owner SID with GENERIC_ALL permissions.
    if unsafe { AddAccessAllowedAce(p_acl, ACL_REVISION, GENERIC_ALL, owner_sid) } == 0 {
        let err = unsafe { GetLastError() };
        return Err(MemFuseError::Storage(format!(
            "Failed to add ACE to ACL: Win32 error {}",
            err
        )));
    }

    let path_wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    // SAFETY: SetNamedSecurityInfoW sets explicit DACL and disables inheritance (PROTECTED_DACL_SECURITY_INFORMATION).
    let status = unsafe {
        SetNamedSecurityInfoW(
            path_wide.as_ptr() as *mut _,
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            p_acl,
            null_mut(),
        )
    };

    if status != ERROR_SUCCESS {
        return Err(MemFuseError::Storage(format!(
            "SetNamedSecurityInfoW failed for {} with Win32 error code {}",
            path.display(),
            status
        )));
    }

    Ok(())
}

impl Wal {
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
                // Cleanup-Fehler hier ist unkritisch: der ursprüngliche Fehler wurde bereits oben propagiert.
                let _ = tokio::fs::remove_file(&tmp_path).await;
                return Err(MemFuseError::Storage(format!(
                    "Failed to write WAL integrity key: {}",
                    e
                )));
            }
            if let Err(e) = file.sync_all().await {
                // Cleanup-Fehler hier ist unkritisch: der ursprüngliche Fehler wurde bereits oben propagiert.
                let _ = tokio::fs::remove_file(&tmp_path).await;
                return Err(MemFuseError::Storage(format!(
                    "Failed to sync WAL integrity key file: {}",
                    e
                )));
            }
            drop(file);

            #[cfg(windows)]
            if let Err(e) = set_restrictive_file_acl(&tmp_path) {
                // Cleanup-Fehler hier ist unkritisch: der ursprüngliche Fehler wurde bereits oben propagiert.
                let _ = tokio::fs::remove_file(&tmp_path).await;
                return Err(e.into());
            }

            // Atomically link tmp_path to key_path. Fails if key_path already exists (O_EXCL semantics).
            let link_res = tokio::fs::hard_link(&tmp_path, &key_path).await;
            // Cleanup-Fehler hier ist unkritisch: das Ergebnis der Link-Operation wird unten ausgewertet.
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

        async fn read_uuid_file(path: &Path) -> Result<[u8; 16]> {
            let bytes = tokio::fs::read(path).await.map_err(|e| {
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
        }

        if uuid_path.exists() {
            read_uuid_file(&uuid_path).await
        } else {
            use rand::RngCore;

            let uuid = uuid::Uuid::new_v4();
            let bytes = *uuid.as_bytes();

            let uuid_filename = uuid_path
                .file_name()
                .map(|s| s.to_string_lossy())
                .unwrap_or_default();
            let tmp_filename = format!(
                "{}.tmp.{}.{}",
                uuid_filename,
                std::process::id(),
                rand::thread_rng().next_u64()
            );

            let parent = uuid_path.parent().unwrap_or_else(|| Path::new(""));
            let tmp_path = if parent.as_os_str().is_empty() {
                PathBuf::from(tmp_filename)
            } else {
                parent.join(tmp_filename)
            };

            let mut options = tokio::fs::OpenOptions::new();
            options.write(true).create_new(true);

            let mut file = match options.open(&tmp_path).await {
                Ok(f) => f,
                Err(e) => {
                    if uuid_path.exists() {
                        return read_uuid_file(&uuid_path).await;
                    }
                    return Err(MemFuseError::Storage(format!(
                        "Failed to create temporary WAL UUID sidecar at {}: {}",
                        tmp_path.display(),
                        e
                    )));
                }
            };

            if let Err(e) = file.write_all(&bytes).await {
                // Cleanup-Fehler hier ist unkritisch: der ursprüngliche Fehler wurde bereits oben propagiert.
                let _ = tokio::fs::remove_file(&tmp_path).await;
                return Err(MemFuseError::Storage(format!(
                    "Failed to write WAL UUID sidecar: {}",
                    e
                )));
            }

            if let Err(e) = file.sync_all().await {
                // Cleanup-Fehler hier ist unkritisch: der ursprüngliche Fehler wurde bereits oben propagiert.
                let _ = tokio::fs::remove_file(&tmp_path).await;
                return Err(MemFuseError::Storage(format!(
                    "Failed to sync WAL UUID sidecar file: {}",
                    e
                )));
            }
            drop(file);

            if let Err(e) = tokio::fs::rename(&tmp_path, &uuid_path).await {
                // Cleanup-Fehler hier ist unkritisch: der ursprüngliche Fehler wurde bereits oben propagiert.
                let _ = tokio::fs::remove_file(&tmp_path).await;
                if uuid_path.exists() {
                    return read_uuid_file(&uuid_path).await;
                }
                return Err(MemFuseError::Storage(format!(
                    "Failed to rename WAL UUID sidecar from {} to {}: {}",
                    tmp_path.display(),
                    uuid_path.display(),
                    e
                )));
            }

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
    // TODO(audit-C-3): Atomically check file header/size under file lock before writing header in append_batch to prevent double WAL headers.
    pub async fn append_batch(&self, entries: &[WalEntry]) -> Result<()> {
        if entries.is_empty() {
            return Ok(());
        }

        #[cfg(feature = "fault-injection")]
        {
            let fail_tx = FAIL_APPEND_FOR_TX.load(std::sync::atomic::Ordering::SeqCst);
            if fail_tx != 0 && entries.iter().any(|e| e.tx_id().inner() == fail_tx) {
                FAIL_APPEND_FOR_TX.store(0, std::sync::atomic::Ordering::SeqCst);
                return Err(MemFuseError::Storage(
                    "Simulated WAL append_batch I/O failure via fault injection".into(),
                ));
            }
        }

        let mut payload_bytes = Vec::new();
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

            payload_bytes.extend_from_slice(&chunk_len.to_le_bytes());
            payload_bytes.extend_from_slice(&nonce);
            payload_bytes.extend_from_slice(&encrypted);
        } else {
            for entry in entries {
                let bytes = entry.to_bytes()?;
                payload_bytes.extend_from_slice(&bytes);
                last_hmac_val = entry.checksum;
            }
        }

        let mut file = self.file.lock().await;

        let write_header = !self
            .header_written
            .load(std::sync::atomic::Ordering::Acquire)
            && self.size.load(std::sync::atomic::Ordering::Acquire) == 0;

        let total_bytes = if write_header {
            let mut buf = Vec::with_capacity(WAL_V3_HEADER.len() + payload_bytes.len());
            buf.extend_from_slice(&WAL_V3_HEADER);
            buf.extend_from_slice(&payload_bytes);
            buf
        } else {
            payload_bytes
        };

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

        if write_header {
            self.header_written
                .store(true, std::sync::atomic::Ordering::Release);
        }

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
    /// Returns the prepared entries along with a snapshot of the pre-prepare HMAC chain link.
    pub async fn prepare_batch(&self, ops: Vec<(WalOp, u64)>) -> Result<(Vec<WalEntry>, [u8; 32])> {
        let mut last_hmac = self.last_hmac.lock().await;
        let prev_hmac = *last_hmac;
        let integrity_key = self.get_integrity_key()?;

        let mut entries = Vec::with_capacity(ops.len());
        let mut current_chain = prev_hmac;

        for (op, seq_no) in ops {
            let entry = WalEntry::try_new(op, seq_no, &integrity_key, current_chain)?;
            current_chain = entry.checksum;
            entries.push(entry);
        }

        *last_hmac = current_chain;

        Ok((entries, prev_hmac))
    }

    /// Restores the in-memory last HMAC to a previous snapshot state.
    pub async fn restore_last_hmac(&self, hmac: [u8; 32]) -> Result<()> {
        let mut guard = self.last_hmac.lock().await;
        *guard = hmac;
        Ok(())
    }

    fn get_integrity_key(&self) -> Result<[u8; 32]> {
        if let Some(km) = &self.key_manager {
            km.integrity_key().map_err(Into::into)
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

            if matches!(version, WalVersion::V2 | WalVersion::V3) && self.key_manager.is_some() {
                let km = match self.key_manager.as_ref() {
                    Some(km) => km,
                    None => unreachable!(),
                };
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

                let mut slice = decrypted_data.as_slice();
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
                                    format!("CRC validation failed: {e}")
                                } else {
                                    format!("Deserialization failed: {e}")
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
                        tx_id: entry.tx_id().inner(),
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
                        if !using_legacy_key && self.allow_legacy_integrity_key_fallback {
                            let mut legacy_verifier =
                                IntegrityVerifier::new(&legacy_integrity_key());
                            legacy_verifier.set_last_hmac(verifier.last_hmac_snapshot());
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
                                return Err(e.into());
                            }
                        } else {
                            return Err(e.into());
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
                            if version == WalVersion::V1 {
                                // SICHERHEIT: Ein aktiver KeyManager bedeutet, dass Verschlüsselung für diese
                                // WAL verpflichtend ist. Ein Eintrag, der nur unverschlüsselt (V1-Klartext)
                                // parsbar ist, wird NIEMALS stillschweigend akzeptiert — das wäre eine
                                // Downgrade-Angriffsfläche für einen Schreibzugriff-Angreifer. Stattdessen wird
                                // dies immer als Integritätsfehler behandelt, unabhängig davon, ob die rohen
                                // Bytes zufällig als gültiger V1-Eintrag parsbar wären.
                                return Err(MemFuseError::Storage(format!(
                                    "WAL entry at {} claims V1/plaintext format while KeyManager is active for {} \
                                     (decryption failed: {}) — refusing potential downgrade attack. \
                                     Set allow_legacy_integrity_key_fallback / min_wal_version appropriately if \
                                     this WAL genuinely predates encryption and requires migration.",
                                    chunk_start_pos,
                                    self.path.display(),
                                    e
                                )));
                            } else {
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
                    tx_id: entry.tx_id().inner(),
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
                    if !using_legacy_key && self.allow_legacy_integrity_key_fallback {
                        let mut legacy_verifier = IntegrityVerifier::new(&legacy_integrity_key());
                        legacy_verifier.set_last_hmac(verifier.last_hmac_snapshot());
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
                            return Err(e.into());
                        }
                    } else {
                        return Err(e.into());
                    }
                }

                entries.push((entry.seq_no, entry, pos));
            }
        }

        Ok((entries, version))
    }

    /// Rewrites legacy V1 or V2 WAL files as V3.
    // TODO(audit-H-6): Implement post-crash recovery to restore .v1.bak / .v2.bak backup files if primary WAL is corrupted or truncated during rewrite.
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
        self.header_written
            .store(true, std::sync::atomic::Ordering::Release);
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

    /// Truncates the WAL file to `offset` and resets `last_hmac` to `new_last_hmac`.
    ///
    /// # Concurrency invariant
    /// `size` and `last_hmac` are updated while the `file` lock is still held, to avoid
    /// a TOCTOU window in which a concurrent reader could observe a stale (too large)
    /// `size` after the file has already been physically truncated on disk.
    ///
    /// # Errors
    /// Returns `MemFuseError::Storage` if setting file length or seeking fails.
    // TODO(audit-C-2): Call file.sync_all() immediately after file.set_len() to ensure length truncation is crash-persisted.
    pub async fn truncate(&self, offset: u64, new_last_hmac: [u8; 32]) -> Result<()> {
        use tokio::io::AsyncSeekExt;

        let mut file = self.file.lock().await;

        // Update in-memory size BEFORE physical truncation so that
        // in-memory size never exceeds physical file length on disk
        // (closing the TOCTOU window during async OS file.set_len).
        self.size.store(offset, std::sync::atomic::Ordering::SeqCst);
        if offset < 4 {
            self.header_written
                .store(false, std::sync::atomic::Ordering::Release);
        }

        file.set_len(offset)
            .await
            .map_err(|e| MemFuseError::Storage(format!("WAL truncate failed: {e}")))?;

        // Truncation physisch auf Platte erzwingen, BEVOR der In-Memory-Cursor per seek()
        // gesetzt wird. Ohne dies überlebt set_len() einen Crash u. U. nicht (Page-Cache-only).
        file.sync_all()
            .await
            .map_err(|e| MemFuseError::Storage(format!("WAL truncate fsync failed: {e}")))?;

        file.seek(std::io::SeekFrom::Start(offset))
            .await
            .map_err(|e| MemFuseError::Storage(format!("WAL seek after truncate failed: {e}")))?;

        {
            let mut last_hmac_guard = self.last_hmac.lock().await;
            *last_hmac_guard = new_last_hmac;
        }

        drop(file);

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
    // TODO(audit-M-2): Optimize transaction offset search from O(N) sequential replay scan to index lookup or reverse offset scanning.
    pub async fn find_tx_offset(&self, target_tx_id: TxId) -> Result<(u64, [u8; 32])> {
        let entries = self.replay().await?;
        let mut last_offset = 0;
        let mut last_hmac = [0u8; 32];

        for (_, entry, offset) in entries {
            let entry_tx = entry.tx_id().inner();
            // System metadata transactions (tx >= INTERNAL_BASE) are preserved during user state rollbacks
            if target_tx_id.inner() < TxId::INTERNAL_BASE && entry_tx >= TxId::INTERNAL_BASE {
                last_offset = offset;
                last_hmac = entry.checksum;
                continue;
            }

            if entry_tx > target_tx_id.inner() {
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
                WalEntry::try_new(op, 1, &legacy_integrity_key(), [0u8; 32]).expect("legacy entry"); // expect

            let mut wal_bytes = Vec::new();
            wal_bytes.extend_from_slice(&WAL_V3_HEADER);
            wal_bytes.extend_from_slice(&legacy_entry.to_bytes().expect("to_bytes"));

            tokio::fs::write(&wal_path, wal_bytes) // expect
                .await
                .expect("write legacy WAL"); // expect
        }

        // Opening without explicit opt-in must fail (downgrade attack protection)
        let open_res = Wal::open(&wal_path).await;
        assert!(
            open_res.is_err(),
            "Opening legacy WAL without allow_legacy_integrity_key_fallback must fail"
        );

        // Opening with explicit opt-in must succeed
        let wal = Wal::open_with_config(
            &wal_path,
            WalConfig {
                allow_legacy_integrity_key_fallback: true,
                ..Default::default()
            },
        )
        .await
        .expect("open legacy wal with fallback opt-in"); // expect
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

        let (entries, _) = wal.prepare_batch(ops).await.expect("prepare_batch"); // expect
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

        let (batch, _) = wal.prepare_batch(ops).await.expect("prepare batch"); // expect
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

        let (batch, _) = wal.prepare_batch(ops).await.expect("prepare_batch"); // expect
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
        let integrity_key = sub_km
            .integrity_key()
            .map_err(MemFuseError::from)
            .expect("integrity key"); // expect

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
            let (batch1, _) = wal.prepare_batch(ops1).await.expect("prepare 1"); // expect
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
            let (batch2, _) = wal.prepare_batch(ops2).await.expect("prepare 2"); // expect
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
        let temp = tempfile::tempdir().expect("tempdir"); // expect
        let wal_path = temp.path().join("test.wal");

        // Test 1: Created key file has 0o600 permissions on Unix
        let key1 = Wal::load_or_create_integrity_key(&wal_path)
            .await
            .expect("create key"); // expect

        let key_path = temp.path().join(".wal_integrity_key");
        assert!(key_path.exists(), "Key file must exist");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::metadata(&key_path).expect("metadata"); // expect
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
            let res = h.await.expect("join handle").expect("load key"); // expect
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

    /// Windows ACL verification test.
    /// Note: This test executes only on Windows platforms (e.g. `windows-latest` CI runner).
    #[cfg(windows)]
    #[test]
    #[allow(unsafe_code)]
    fn test_windows_wal_integrity_key_acl() {
        use std::os::windows::ffi::OsStrExt;
        use std::ptr::null_mut;
        use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_SUCCESS, HANDLE};
        use windows_sys::Win32::Security::Authorization::{GetNamedSecurityInfoW, SE_FILE_OBJECT};
        use windows_sys::Win32::Security::{
            EqualSid, GetAce, GetTokenInformation, TokenUser, ACCESS_ALLOWED_ACE, ACL,
            DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, SECURITY_DESCRIPTOR_CONTROL,
            SE_DACL_PROTECTED, TOKEN_QUERY, TOKEN_USER,
        };
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

        let temp = tempfile::tempdir().expect("tempdir"); // expect
        let wal_path = temp.path().join("test_acl.wal");

        let rt = tokio::runtime::Runtime::new().expect("tokio runtime"); // expect
        let key = rt
            .block_on(Wal::load_or_create_integrity_key(&wal_path))
            .expect("load_or_create_integrity_key"); // expect
        assert_eq!(key.len(), 32);

        let key_path = temp.path().join(".wal_integrity_key");
        assert!(key_path.exists(), "Key file must exist");

        let path_wide: Vec<u16> = key_path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let mut p_sec_desc: PSECURITY_DESCRIPTOR = null_mut();
        let mut p_dacl: *mut ACL = null_mut();
        let mut control: SECURITY_DESCRIPTOR_CONTROL = 0;
        let mut revision = 0u32;

        // Query file's DACL and Control bits
        let status = unsafe {
            GetNamedSecurityInfoW(
                path_wide.as_ptr() as *mut _,
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                &mut p_dacl,
                null_mut(),
                &mut p_sec_desc,
            )
        };
        assert_eq!(
            status, ERROR_SUCCESS,
            "GetNamedSecurityInfoW failed with error {}",
            status
        );

        struct SecDescGuard(PSECURITY_DESCRIPTOR);
        impl Drop for SecDescGuard {
            fn drop(&mut self) {
                if !self.0.is_null() {
                    unsafe {
                        windows_sys::Win32::Foundation::LocalFree(self.0 as _);
                    }
                }
            }
        }
        let _guard = SecDescGuard(p_sec_desc);

        // Verify DACL is present
        assert!(!p_dacl.is_null(), "DACL should not be null");

        // Verify control bits to check that DACL inheritance is protected/disabled
        let status = unsafe {
            windows_sys::Win32::Security::GetSecurityDescriptorControl(
                p_sec_desc,
                &mut control,
                &mut revision,
            )
        };
        assert_ne!(
            status,
            0,
            "GetSecurityDescriptorControl failed with error {}",
            unsafe { GetLastError() }
        );
        assert_ne!(
            control & SE_DACL_PROTECTED,
            0,
            "DACL inheritance must be disabled (SE_DACL_PROTECTED bit set)"
        );

        // Query process token user SID
        let mut token_handle: HANDLE = null_mut();
        let res = unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token_handle) };
        assert_ne!(res, 0, "OpenProcessToken failed");

        let mut len = 0u32;
        unsafe {
            GetTokenInformation(token_handle, TokenUser, null_mut(), 0, &mut len);
        }
        let mut buffer = vec![0u8; len as usize];
        let res = unsafe {
            GetTokenInformation(
                token_handle,
                TokenUser,
                buffer.as_mut_ptr().cast(),
                len,
                &mut len,
            )
        };
        assert_ne!(res, 0, "GetTokenInformation failed");
        unsafe { CloseHandle(token_handle) };

        let token_user = buffer.as_ptr() as *const TOKEN_USER;
        let owner_sid = unsafe { (*token_user).User.Sid };
        assert!(!owner_sid.is_null());

        // Inspect ACE count and verify ACE matches process owner SID
        let ace_count = unsafe { (*p_dacl).AceCount };
        assert_eq!(ace_count, 1, "DACL must contain exactly 1 ACE (owner only)");

        let mut p_ace: *mut std::ffi::c_void = null_mut();
        let res = unsafe { GetAce(p_dacl, 0, &mut p_ace) };
        assert_ne!(res, 0, "GetAce failed");

        let ace = p_ace as *const ACCESS_ALLOWED_ACE;
        let ace_sid = unsafe { &(*ace).SidStart as *const u32 as *mut std::ffi::c_void };

        let same_sid = unsafe { EqualSid(owner_sid, ace_sid) };
        assert_ne!(same_sid, 0, "ACE SID must match the process owner SID");
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

    #[test]
    fn test_wal_entry_from_bytes_invalid_cases() {
        // 1. Too short data (< 94 bytes)
        let short_data = vec![0u8; 50];
        let res = WalEntry::from_bytes(&short_data);
        assert!(matches!(res, Err(MemFuseError::Serialization(_))));

        // 2. Invalid WalOp tag (e.g., tag = 255)
        let mut payload = vec![0u8; 90];
        payload[72] = 255; // Invalid tag
        let crc = crc32fast::hash(&payload);
        let mut data = vec![0u8; 4];
        data[0..4].copy_from_slice(&crc.to_le_bytes());
        data.extend_from_slice(&payload);

        let res_op = WalEntry::from_bytes(&data);
        assert!(matches!(res_op, Err(MemFuseError::Serialization(_))));
    }

    #[tokio::test]
    async fn test_uuid_sidecar_crash_fault_injection() {
        let dir = tempdir().expect("tempdir"); // expect
        let wal_path = dir.path().join("fault_uuid.wal");
        let uuid_path = dir.path().join("fault_uuid.wal.uuid");

        let km = Arc::new(
            KeyManager::try_new("passphrase123", b"salt123456789012345678901234567890")
                .expect("km"), // expect
        );

        // 1. Simulate a leftover interrupted temp file from a crashed write
        let tmp_path = dir.path().join("fault_uuid.wal.uuid.tmp.99999.12345");
        tokio::fs::write(&tmp_path, b"incomplete_uuid")
            .await
            .expect("write leftover tmp file"); // expect

        // 2. Opening WAL should cleanly recover, create valid 16-byte UUID sidecar, and ignore leftover tmp file
        let wal = Wal::open_with_key_manager(&wal_path, Some(km.clone()))
            .await
            .expect("open wal should succeed despite leftover tmp file"); // expect

        assert!(uuid_path.exists(), "UUID sidecar must exist");
        let uuid_bytes = tokio::fs::read(&uuid_path).await.expect("read uuid"); // expect
        assert_eq!(uuid_bytes.len(), 16);

        drop(wal);

        // 3. Re-opening should read the same valid UUID sidecar
        let uuid_bytes_after = Wal::load_or_create_wal_uuid(&wal_path)
            .await
            .expect("load uuid"); // expect
        assert_eq!(uuid_bytes.as_slice(), uuid_bytes_after);
    }

    #[tokio::test]
    async fn test_prepare_batch_hmac_chain_concurrency() {
        let dir = tempdir().expect("tempdir"); // expect
        let wal_path = dir.path().join("concurrency_batch.wal");

        let wal = Arc::new(Wal::open(&wal_path).await.expect("open wal")); // expect

        let wal1 = wal.clone();
        let wal2 = wal.clone();

        let handle1 = tokio::spawn(async move {
            let ops = vec![(
                WalOp::Put {
                    tx_id: TxId::new(1),
                    key: b"k1".to_vec(),
                    value: b"v1".to_vec(),
                },
                1,
            )];
            wal1.prepare_batch(ops).await.expect("batch 1") // expect
        });

        let handle2 = tokio::spawn(async move {
            let ops = vec![(
                WalOp::Put {
                    tx_id: TxId::new(2),
                    key: b"k2".to_vec(),
                    value: b"v2".to_vec(),
                },
                2,
            )];
            wal2.prepare_batch(ops).await.expect("batch 2") // expect
        });

        let (res1, res2) = tokio::join!(handle1, handle2);
        let (batch1, _) = res1.expect("join 1"); // expect
        let (batch2, _) = res2.expect("join 2"); // expect

        let prev1 = batch1[0].prev_hmac;
        let prev2 = batch2[0].prev_hmac;

        // One batch must have chained off the initial [0u8; 32] HMAC, and the second batch off the first's checksum.
        // Crucially, their starting prev_hmac values must NOT be identical.
        assert_ne!(
            prev1, prev2,
            "Concurrent prepare_batch calls must produce unique prev_hmac chain links"
        );

        if prev1 == [0u8; 32] {
            assert_eq!(prev2, batch1[0].checksum);
        } else {
            assert_eq!(prev1, batch2[0].checksum);
            assert_eq!(prev2, [0u8; 32]);
        }
    }

    #[tokio::test]
    async fn test_open_with_key_manager_is_new_race_condition() {
        let dir = tempdir().expect("tempdir"); // expect
        let wal_path = dir.path().join("race_open.wal");

        let km = Arc::new(
            KeyManager::try_new("passphrase123", b"salt123456789012345678901234567890")
                .expect("km"), // expect
        );

        // Pre-create the UUID sidecar so both calls race purely on the WAL file open
        Wal::load_or_create_wal_uuid(&wal_path)
            .await
            .expect("create uuid sidecar"); // expect

        let path1 = wal_path.clone();
        let path2 = wal_path.clone();
        let km1 = km.clone();
        let km2 = km.clone();

        let h1 = tokio::spawn(async move { Wal::open_with_key_manager(path1, Some(km1)).await });
        let h2 = tokio::spawn(async move { Wal::open_with_key_manager(path2, Some(km2)).await });

        let (res1, res2) = tokio::join!(h1, h2);
        let wal1 = res1.expect("join 1").expect("open 1"); // expect
        let wal2 = res2.expect("join 2").expect("open 2"); // expect

        // Both WAL instances are opened successfully
        assert_eq!(wal1.path(), wal2.path());
    }

    #[test]
    fn test_legacy_integrity_key_deobfuscation() {
        let key = legacy_integrity_key();
        assert_eq!(&key, b"memfuse-integrity-key-v1\0\0\0\0\0\0\0\0");
    }

    #[tokio::test]
    async fn test_wal_v1_auto_migration_on_min_version_v3() {
        let dir = tempdir().expect("tempdir"); // expect
        let wal_path = dir.path().join("migrate_v1.wal");
        let bak_path = dir.path().join("migrate_v1.wal.v1.bak");

        {
            // Create an unencrypted V1 WAL file
            let op = WalOp::Put {
                tx_id: TxId::new(1),
                key: b"mig_key".to_vec(),
                value: b"mig_val".to_vec(),
            };
            let entry =
                WalEntry::try_new(op, 1, &legacy_integrity_key(), [0u8; 32]).expect("v1 entry"); // expect

            let mut v1_bytes = Vec::new();
            // V1 WAL file has no MFW3 or MFW2 header prefix
            v1_bytes.extend_from_slice(&entry.to_bytes().expect("to_bytes")); // expect
            fs::write(&wal_path, v1_bytes).await.expect("write v1 wal"); // expect
        }

        // Open with min_wal_version = WalVersion::V3 and legacy key fallback allowed
        let wal = Wal::open_with_config(
            &wal_path,
            WalConfig {
                allow_legacy_integrity_key_fallback: true,
                min_wal_version: WalVersion::V3,
                ..Default::default()
            },
        )
        .await
        .expect("open and auto-migrate v1 wal"); // expect

        // Verify that backup file exists
        assert!(
            bak_path.exists(),
            "Backup file .v1.bak must exist after migration"
        );

        // Verify replayed entries
        let entries = wal.replay().await.expect("replay migrated wal"); // expect
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].1.seq_no, 1);
        if let WalOp::Put { key, value, .. } = &entries[0].1.op {
            assert_eq!(key, b"mig_key");
            assert_eq!(value, b"mig_val");
        } else {
            panic!("Expected Put op");
        }

        // Read the actual WAL file from disk and verify it now has the V3 header
        let raw_disk_bytes = fs::read(&wal_path).await.expect("read wal_path"); // expect
        assert_eq!(
            &raw_disk_bytes[0..4],
            &WAL_V3_HEADER,
            "Migrated file must start with WAL_V3_HEADER"
        );
    }

    #[tokio::test]
    async fn test_hmac_chain_intact_after_append_failure() {
        let dir = tempdir().expect("tempdir");
        let wal_path = dir.path().join("append_failure.wal");

        let wal = Wal::open(&wal_path).await.expect("open wal");

        // 1. Initial write
        let op1 = WalOp::Put {
            tx_id: TxId::new(1),
            key: b"k1".to_vec(),
            value: b"v1".to_vec(),
        };
        let (batch1, _) = wal
            .prepare_batch(vec![(op1, 1)])
            .await
            .expect("prepare batch 1");
        wal.append_batch(&batch1).await.expect("append batch 1");

        let hmac_before = wal.last_hmac_snapshot().await;

        // 2. Prepare a second batch that advances last_hmac
        let op2 = WalOp::Put {
            tx_id: TxId::new(2),
            key: b"k2".to_vec(),
            value: b"v2".to_vec(),
        };
        let (batch2, prev_hmac) = wal
            .prepare_batch(vec![(op2, 2)])
            .await
            .expect("prepare batch 2");
        assert_ne!(
            wal.last_hmac_snapshot().await,
            hmac_before,
            "prepare_batch should advance in-memory last_hmac"
        );
        assert_eq!(prev_hmac, hmac_before);

        // 3. Simulate append failure by replacing file with a read-only file handle
        {
            let ro_file = tokio::fs::OpenOptions::new()
                .read(true)
                .write(false)
                .open(&wal_path)
                .await
                .expect("open read-only");
            let mut guard = wal.file.lock().await;
            *guard = ro_file;
        }

        let append_res = wal.append_batch(&batch2).await;
        assert!(
            append_res.is_err(),
            "append_batch must fail on read-only file handle"
        );

        // Restore last_hmac as lsm commit would do upon append failure
        wal.restore_last_hmac(prev_hmac)
            .await
            .expect("restore last hmac");

        // 4. Verify last_hmac_snapshot is back to hmac_before
        let hmac_after = wal.last_hmac_snapshot().await;
        assert_eq!(
            hmac_after, hmac_before,
            "last_hmac_snapshot must match value before failed prepare_batch"
        );
    }

    #[tokio::test]
    async fn test_truncate_size_visible_atomically_with_file_state() {
        let dir = tempdir().expect("tempdir");
        let wal_path = dir.path().join("truncate_atomic.wal");

        let wal = Arc::new(Wal::open(&wal_path).await.expect("open wal"));

        let done = Arc::new(std::sync::atomic::AtomicBool::new(false));

        let wal_trunc = wal.clone();
        let done_trunc = done.clone();
        let truncater = tokio::spawn(async move {
            for i in 0..100 {
                // Prepare and append a batch of entries so file grows
                let ops = vec![
                    (
                        WalOp::Put {
                            tx_id: TxId::new(i * 2 + 1),
                            key: b"atomic_key_1".to_vec(),
                            value: b"atomic_val_1".to_vec(),
                        },
                        i * 2 + 1,
                    ),
                    (
                        WalOp::Put {
                            tx_id: TxId::new(i * 2 + 2),
                            key: b"atomic_key_2".to_vec(),
                            value: b"atomic_val_2".to_vec(),
                        },
                        i * 2 + 2,
                    ),
                ];
                let (batch, _) = wal_trunc.prepare_batch(ops).await.expect("prepare_batch");
                wal_trunc.append_batch(&batch).await.expect("append_batch");

                // Truncate back to offset 4 (length of WAL_V3_HEADER)
                wal_trunc
                    .truncate(4, [0xAA; 32])
                    .await
                    .expect("truncate failed");
            }
            done_trunc.store(true, std::sync::atomic::Ordering::SeqCst);
        });

        let wal_poll = wal.clone();
        let done_poll = done.clone();
        let poller = tokio::spawn(async move {
            while !done_poll.load(std::sync::atomic::Ordering::SeqCst) {
                if let Ok(meta) = fs::metadata(wal_poll.path()).await {
                    let disk_size = meta.len();
                    let mem_size = wal_poll.size();
                    // In-memory size must never observe stale mem_size > disk_size after truncation
                    assert!(
                        mem_size <= disk_size,
                        "TOCTOU violation: in-memory WAL size ({mem_size}) > physical disk size ({disk_size})"
                    );
                }
                tokio::task::yield_now().await;
            }
        });

        let (res_trunc, res_poll) = tokio::join!(truncater, poller);
        res_trunc.expect("truncater panicked");
        res_poll.expect("poller panicked");
    }

    #[tokio::test]
    async fn test_concurrent_append_batch_header_atomicity() {
        let dir = tempdir().expect("tempdir");
        let wal_path = dir.path().join("concurrent_header.wal");

        let wal = Arc::new(Wal::open(&wal_path).await.expect("open wal"));

        let num_tasks = 8;
        let mut handles = Vec::new();

        for i in 0..num_tasks {
            let wal_clone = wal.clone();
            handles.push(tokio::spawn(async move {
                let op = WalOp::Put {
                    tx_id: TxId::new(i + 1),
                    key: format!("key_{}", i).into_bytes(),
                    value: format!("val_{}", i).into_bytes(),
                };
                let (batch, _) = wal_clone
                    .prepare_batch(vec![(op, i + 1)])
                    .await
                    .expect("prepare_batch");
                wal_clone.append_batch(&batch).await.expect("append_batch");
            }));
        }

        for h in handles {
            h.await.expect("join handle");
        }

        let file_bytes = fs::read(&wal_path).await.expect("read wal file");

        // Assert header is present at start
        assert!(
            file_bytes.len() >= 4,
            "WAL file must be at least 4 bytes long"
        );
        assert_eq!(
            &file_bytes[0..4],
            &WAL_V3_HEADER,
            "WAL file must start with WAL_V3_HEADER"
        );

        // Count header occurrences across entire file
        let header_count = file_bytes
            .windows(4)
            .filter(|window| *window == WAL_V3_HEADER)
            .count();
        assert_eq!(
            header_count, 1,
            "WAL_V3_HEADER must appear exactly once at the start of the file, but was found {header_count} times"
        );

        // Reopen and replay to verify no stream corruption
        let wal_reopen = Wal::open(&wal_path).await.expect("reopen wal");
        let replayed = wal_reopen.replay().await.expect("replay must succeed");
        assert_eq!(
            replayed.len(),
            num_tasks as usize,
            "Replay must yield all {} entries",
            num_tasks
        );
    }

    #[tokio::test]
    async fn test_truncate_is_durable_across_simulated_crash() {
        let dir = tempdir().expect("tempdir");
        let wal_path = dir.path().join("truncate_durability.wal");

        let wal = Wal::open(&wal_path).await.expect("open wal");

        // Write several entries so file grows
        for i in 1..=5 {
            let op = WalOp::Put {
                tx_id: TxId::new(i),
                key: format!("k{}", i).into_bytes(),
                value: format!("v{}", i).into_bytes(),
            };
            let entry = wal.create_entry(op, i).await.expect("create entry");
            wal.append(&entry).await.expect("append entry");
        }

        let initial_size = tokio::fs::metadata(&wal_path).await.expect("meta").len();
        assert!(initial_size > 4, "File size should be larger than header");

        // Truncate to offset 4 (HEADER length)
        let new_hmac = [0x77u8; 32];
        wal.truncate(4, new_hmac).await.expect("truncate");

        // Open via a new independent File handle (simulates restart after crash without the original Wal handle)
        let file = tokio::fs::File::open(&wal_path).await.expect("reopen file");
        let metadata = file.metadata().await.expect("metadata");
        assert_eq!(
            metadata.len(),
            4,
            "Physical file length on disk must equal truncated offset 4 after fsync"
        );
    }

    #[tokio::test]
    async fn test_recover_from_bak_if_present_cases() {
        let dir = tempdir().expect("tempdir");
        let wal_path = dir.path().join("test_recovery.wal");
        let bak_path = dir.path().join("test_recovery.wal.v1.bak");

        // Case (a): no backup present -> Ok(false), file untouched
        tokio::fs::write(&wal_path, b"some content").await.unwrap();
        let res = recover_from_bak_if_present(&wal_path).await.expect("recover");
        assert!(!res);
        assert_eq!(tokio::fs::read(&wal_path).await.unwrap(), b"some content");

        // Case (c): backup present, regular file non-empty -> Ok(false), no override
        tokio::fs::write(&bak_path, b"backup content").await.unwrap();
        let res = recover_from_bak_if_present(&wal_path).await.expect("recover");
        assert!(!res);
        assert_eq!(tokio::fs::read(&wal_path).await.unwrap(), b"some content");

        // Case (b): backup present, regular file empty (0 bytes) -> Ok(true), backup restored
        tokio::fs::write(&wal_path, b"").await.unwrap();
        let res = recover_from_bak_if_present(&wal_path).await.expect("recover");
        assert!(res);
        assert_eq!(tokio::fs::read(&wal_path).await.unwrap(), b"backup content");
        assert!(!bak_path.exists(), "Backup file should be renamed/removed after recovery");
    }

    #[tokio::test]
    async fn test_full_rewrite_crash_recovery_pipeline() {
        let dir = tempdir().expect("tempdir");
        let wal_path = dir.path().join("rewrite_crash.wal");

        // 1. Write legacy V1 entry
        let op = WalOp::Put {
            tx_id: TxId::new(1),
            key: b"k_crash".to_vec(),
            value: b"v_crash".to_vec(),
        };
        let entry = WalEntry::try_new(op, 1, &legacy_integrity_key(), [0u8; 32]).expect("v1 entry");
        let v1_bytes = entry.to_bytes().expect("to_bytes");
        tokio::fs::write(&wal_path, &v1_bytes).await.expect("write v1 wal");

        // 2. Simulate backup creation and fsync
        let bak_path = dir.path().join("rewrite_crash.wal.v1.bak");
        tokio::fs::copy(&wal_path, &bak_path).await.expect("copy backup");
        let bak_file = tokio::fs::OpenOptions::new().write(true).open(&bak_path).await.expect("open bak");
        bak_file.sync_all().await.expect("fsync bak");
        drop(bak_file);

        // 3. Simulate crash after truncating original WAL to 0 bytes before V3 rewrite finishes
        let file = tokio::fs::OpenOptions::new().write(true).open(&wal_path).await.expect("open wal");
        file.set_len(0).await.expect("truncate wal to 0");
        file.sync_all().await.expect("fsync truncated wal");
        drop(file);

        // 4. Wal::open() on the path -> recover_from_bak_if_present recovers backup and replays successfully
        let wal = Wal::open_with_config(
            &wal_path,
            WalConfig {
                allow_legacy_integrity_key_fallback: true,
                min_wal_version: WalVersion::V3,
                ..Default::default()
            },
        )
        .await
        .expect("open and recover wal from backup");

        let replayed = wal.replay().await.expect("replay recovered wal");
        assert_eq!(replayed.len(), 1);
        if let WalOp::Put { key, value, .. } = &replayed[0].1.op {
            assert_eq!(key, b"k_crash");
            assert_eq!(value, b"v_crash");
        } else {
            panic!("Expected Put op");
        }
    }

    #[tokio::test]
    async fn test_v1_plaintext_rejected_when_key_manager_active() {
        let dir = tempdir().expect("tempdir");
        let wal_path = dir.path().join("v1_downgrade.wal");

        let km = Arc::new(
            KeyManager::try_new("passphrase123", b"salt123456789012345678901234567890")
                .expect("km"),
        );

        // 1. Manually construct an unencrypted V1 plaintext WAL entry
        let op = WalOp::Put {
            tx_id: TxId::new(1),
            key: b"unencrypted_key".to_vec(),
            value: b"unencrypted_val".to_vec(),
        };
        let entry = WalEntry::try_new(op, 1, &legacy_integrity_key(), [0u8; 32]).expect("entry");
        let entry_bytes = entry.to_bytes().expect("to_bytes");

        // Write directly to file (bypassing Wal API)
        tokio::fs::write(&wal_path, &entry_bytes).await.expect("write plaintext entry");

        // 2. Opening/replaying with active KeyManager MUST reject the V1 plaintext entry
        let open_res = Wal::open_with_key_manager(&wal_path, Some(km.clone())).await;
        if let Ok(wal) = open_res {
            let replay_res = wal.replay().await;
            assert!(
                replay_res.is_err(),
                "Replaying unencrypted V1 entry with active KeyManager MUST return an error"
            );
            let err_msg = format!("{}", replay_res.unwrap_err());
            assert!(
                err_msg.contains("refusing potential downgrade attack"),
                "Error message should mention downgrade attack refusal, got: {}",
                err_msg
            );
        } else {
            // Opening failed during initial replay in open_with_key_manager, which is also valid
            let err_msg = format!("{}", open_res.unwrap_err());
            assert!(
                err_msg.contains("refusing potential downgrade attack"),
                "Error message should mention downgrade attack refusal, got: {}",
                err_msg
            );
        }

        // 3. Opening/replaying WITHOUT KeyManager MUST succeed for the same V1 plaintext entry
        let wal_no_km = Wal::open_with_config(
            &wal_path,
            WalConfig {
                allow_legacy_integrity_key_fallback: true,
                ..Default::default()
            },
        )
        .await
        .expect("open without key manager should succeed");

        let replayed = wal_no_km.replay().await.expect("replay without key manager");
        assert_eq!(replayed.len(), 1);
        if let WalOp::Put { key, value, .. } = &replayed[0].1.op {
            assert_eq!(key, b"unencrypted_key");
            assert_eq!(value, b"unencrypted_val");
        } else {
            panic!("Expected Put op");
        }
    }

    #[tokio::test]
    async fn test_split_brain_legacy_fallback_chain_continuity() {
        let dir = tempdir().expect("tempdir");
        let wal_path = dir.path().join("split_brain.wal");

        let normal_key = b"normal-integrity-key-32-bytes---";
        let legacy_key = legacy_integrity_key();

        // 1. Entry 1: created with normal key, prev_hmac = [0u8; 32]
        let op1 = WalOp::Put {
            tx_id: TxId::new(1),
            key: b"k1".to_vec(),
            value: b"v1".to_vec(),
        };
        let e1 = WalEntry::try_new(op1, 1, normal_key, [0u8; 32]).unwrap();

        // 2. Entry 2: created with normal key, prev_hmac = e1.checksum
        let op2 = WalOp::Put {
            tx_id: TxId::new(2),
            key: b"k2".to_vec(),
            value: b"v2".to_vec(),
        };
        let e2 = WalEntry::try_new(op2, 2, normal_key, e1.checksum).unwrap();

        // 3. Entry 3 (attacker/legacy entry):
        // prev_hmac = [0u8; 32] (trying to pretend it's the start of chain), but signed with legacy_key.
        let op3 = WalOp::Put {
            tx_id: TxId::new(3),
            key: b"k3".to_vec(),
            value: b"v3".to_vec(),
        };
        let e3_forged = WalEntry::try_new(op3, 3, &legacy_key, [0u8; 32]).unwrap();

        let mut file_bytes = Vec::new();
        file_bytes.extend_from_slice(&WAL_V3_HEADER);
        file_bytes.extend_from_slice(&e1.to_bytes().unwrap());
        file_bytes.extend_from_slice(&e2.to_bytes().unwrap());
        file_bytes.extend_from_slice(&e3_forged.to_bytes().unwrap());

        tokio::fs::write(&wal_path, &file_bytes).await.unwrap();

        // Pre-create integrity key file with normal_key
        let key_file_path = dir.path().join(".wal_integrity_key");
        tokio::fs::write(&key_file_path, normal_key).await.unwrap();

        // Opening / replaying with legacy fallback enabled MUST fail during replay/open due to HMAC mismatch
        let open_res = Wal::open_with_config(
            &wal_path,
            WalConfig {
                allow_legacy_integrity_key_fallback: true,
                ..Default::default()
            },
        )
        .await;

        let replay_err = match open_res {
            Ok(wal) => wal.replay().await.unwrap_err(),
            Err(e) => e,
        };

        assert!(
            matches!(replay_err, MemFuseError::WalCorruption { .. }),
            "Forged entry 3 with prev_hmac=[0;32] must be rejected during fallback because chain state was non-zero! Got: {:?}",
            replay_err
        );
    }
}
