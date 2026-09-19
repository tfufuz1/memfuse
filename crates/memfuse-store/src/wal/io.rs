use memfuse_core::{MemFuseError, Result, TxId};
use memfuse_crypto::wal_crypto::{IntegrityVerifier, WalEntrySnapshot};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

use super::{
    legacy_integrity_key, PreparedBatch, Wal, WalCommand, WalEntry, WalOp, WalVersion,
    MAX_WAL_ENTRY_SIZE, WAL_V2_HEADER, WAL_V3_HEADER,
};

#[cfg(feature = "fault-injection")]
use super::{DELAY_APPEND_FOR_TX, DELAY_APPEND_MS, FAIL_APPEND_FOR_TX};

pub(crate) async fn do_scan_entries_with_callback<F>(
    file: &mut super::fs::File,
    file_size: u64,
    path: &Path,
    key_manager: Option<&memfuse_crypto::crypto::KeyManager>,
    fallback_integrity_key: Option<[u8; 32]>,
    allow_legacy_integrity_key_fallback: bool,
    mut callback: F,
) -> Result<WalVersion>
where
    F: FnMut(u64, WalEntry, u64) -> bool,
{
    file.seek(std::io::SeekFrom::Start(0))
        .await
        .map_err(|e| MemFuseError::Storage(format!("WAL replay seek failed: {}", e)))?;

    let mut reader = tokio::io::BufReader::new(file);

    let mut entries_count = 0u64;
    let mut pos = 0u64;

    let mut version = WalVersion::V1;
    if file_size == 0 {
        return Ok(version);
    }

    let integrity_key = if let Some(km) = key_manager {
        km.integrity_key()?
    } else if let Some(key) = fallback_integrity_key {
        key
    } else {
        return Err(MemFuseError::Storage("No integrity key available".into()));
    };

    let mut verifier = IntegrityVerifier::new(&integrity_key);
    let mut using_legacy_key = false;

    if file_size >= 4 {
        let mut header_buf = [0u8; 4];
        if reader.read_exact(&mut header_buf).await.is_ok() {
            if header_buf == WAL_V3_HEADER {
                version = WalVersion::V3;
                pos = 4;
            } else if header_buf == WAL_V2_HEADER {
                version = WalVersion::V2;
                pos = 4;
            } else {
                reader
                    .get_mut()
                    .seek(std::io::SeekFrom::Start(0))
                    .await
                    .map_err(|e| MemFuseError::Storage(format!("WAL replay seek failed: {}", e)))?;
                reader = tokio::io::BufReader::new(reader.into_inner());
                pos = 0;
            }
        }
    }

    'scan_loop: while pos < file_size {
        let mut len_buf = [0u8; 4];
        if let Err(e) = reader.read_exact(&mut len_buf).await {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                tracing::warn!(
                    "WAL tail corruption (partial entry length) at offset {}",
                    pos
                );
                break;
            }
            return Err(MemFuseError::Storage(format!("WAL read failed: {}", e)));
        }

        let len = u32::from_le_bytes(len_buf) as usize;

        if len > MAX_WAL_ENTRY_SIZE as usize {
            if pos + 4 + len as u64 > file_size {
                if entries_count == 0 && file_size > 64 {
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
            if entries_count == 0 && file_size > 64 {
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
        if let Err(e) = reader.read_exact(&mut entry_data_raw).await {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                tracing::warn!(
                    "WAL tail corruption (truncated entry payload) at offset {}",
                    pos
                );
                break;
            }
            return Err(MemFuseError::Storage(format!("WAL read failed: {}", e)));
        }

        let chunk_start_pos = pos;
        pos += (4 + len) as u64;

        if matches!(version, WalVersion::V2 | WalVersion::V3) && key_manager.is_some() {
            let km = match key_manager {
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

            let mut inner_slice = decrypted_data.as_slice();
            while !inner_slice.is_empty() {
                if inner_slice.len() < 4 {
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
                let inner_len_bytes: [u8; 4] = match inner_slice[0..4].try_into() {
                    Ok(b) => b,
                    Err(_) => {
                        return Err(MemFuseError::wal_corruption(
                            chunk_start_pos,
                            "Failed to extract inner WAL entry length",
                        ));
                    }
                };
                let inner_len = u32::from_le_bytes(inner_len_bytes) as usize;
                if inner_slice.len() < 4 + inner_len {
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
                let inner_entry_bytes = &inner_slice[4..4 + inner_len];
                inner_slice = &inner_slice[4 + inner_len..];

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
                    WalOp::TxEnd { committed, .. } => (2u8, Vec::new(), vec![*committed as u8]),
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
                    if !using_legacy_key && allow_legacy_integrity_key_fallback {
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

                entries_count += 1;
                let seq = entry.seq_no;
                if !callback(seq, entry, pos) {
                    break 'scan_loop;
                }
            }
        } else {
            let decrypted_data;
            let entry_data = if let Some(km) = key_manager {
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
                            return Err(MemFuseError::Storage(format!(
                                "WAL entry at {} claims V1/plaintext format while KeyManager is active for {} \
                                 (decryption failed: {}) — refusing potential downgrade attack. \
                                 Set allow_legacy_integrity_key_fallback / min_wal_version appropriately if \
                                 this WAL genuinely predates encryption and requires migration.",
                                chunk_start_pos,
                                path.display(),
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
                        Wal::handle_wal_entry_parse_error(e, chunk_start_pos, pos, file_size)
                    {
                        return Err(err);
                    }
                    break;
                }
            };

            let (op_type, key, value) = match &entry.op {
                WalOp::Put { key, value, .. } => (0u8, key.clone(), value.clone()),
                WalOp::Delete { key, .. } => (1u8, key.clone(), Vec::new()),
                WalOp::TxEnd { committed, .. } => (2u8, Vec::new(), vec![*committed as u8]),
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
                if !using_legacy_key && allow_legacy_integrity_key_fallback {
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

            entries_count += 1;
            let seq = entry.seq_no;
            if !callback(seq, entry, pos) {
                break 'scan_loop;
            }
        }
    }

    Ok(version)
}

impl Wal {
    pub async fn append_batch(&self, batch: PreparedBatch) -> Result<()> {
        let truncate_guard = self.truncate_lock.lock().await;
        self.append_batch_locked(batch, &truncate_guard).await
    }

    pub async fn append_batch_locked(
        &self,
        batch: PreparedBatch,
        _guard: &tokio::sync::MutexGuard<'_, ()>,
    ) -> Result<()> {
        if self.is_sealed() {
            return Err(MemFuseError::Storage(format!(
                "Cannot append to sealed WAL segment {}",
                self.path.display()
            )));
        }

        let entries = &batch.0;
        if entries.is_empty() {
            return Ok(());
        }

        #[cfg(feature = "fault-injection")]
        {
            let fail_tx = FAIL_APPEND_FOR_TX.load(std::sync::atomic::Ordering::SeqCst);
            if fail_tx != 0
                && entries
                    .iter()
                    .any(|e| e.tx_id().inner() == fail_tx || fail_tx == u64::MAX)
            {
                FAIL_APPEND_FOR_TX.store(0, std::sync::atomic::Ordering::SeqCst);
                return Err(MemFuseError::Storage(
                    "Simulated WAL append_batch I/O failure via fault injection".into(),
                ));
            }

            let delay_tx = DELAY_APPEND_FOR_TX.load(std::sync::atomic::Ordering::SeqCst);
            if delay_tx != 0 && entries.iter().any(|e| e.tx_id().inner() == delay_tx) {
                let delay_ms = DELAY_APPEND_MS.load(std::sync::atomic::Ordering::SeqCst);
                DELAY_APPEND_FOR_TX.store(0, std::sync::atomic::Ordering::SeqCst);
                if delay_ms > 0 {
                    tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                }
            }
        }

        let estimated_size = entries.len() * 256;
        let mut payload_bytes = Vec::with_capacity(estimated_size);
        let mut last_hmac_val = [0u8; 32];

        if let Some(km) = &self.key_manager {
            let mut batch_plaintext = Vec::with_capacity(estimated_size);
            for entry in entries {
                let bytes = entry.to_bytes()?;
                batch_plaintext.extend_from_slice(&bytes);
                last_hmac_val = entry.checksum;
            }

            let km_clone = Arc::clone(km);
            let encrypted_result =
                tokio::task::spawn_blocking(move || km_clone.encrypt_auto_nonce(&batch_plaintext))
                    .await
                    .map_err(|e| {
                        MemFuseError::Storage(format!("WAL encryption task panicked: {e}"))
                    })?;

            let (encrypted, nonce) = encrypted_result?;
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

        let flusher_tx = {
            let guard = self.flusher_tx.read().unwrap_or_else(|e| e.into_inner());
            guard.clone()
        };

        let tx = flusher_tx
            .ok_or_else(|| MemFuseError::Storage("WAL flusher actor is not enabled".into()))?;

        let (ack_tx, ack_rx) = tokio::sync::oneshot::channel();
        tx.send(WalCommand::Append {
            payload: payload_bytes,
            last_hmac_val,
            ack: ack_tx,
        })
        .await
        .map_err(|_| MemFuseError::Storage("WAL flusher channel closed".into()))?;

        ack_rx
            .await
            .map_err(|_| MemFuseError::Storage("WAL flusher dropped".into()))??;
        Ok(())
    }

    /// Non-blocking attempt to append a prepared batch. Returns `Err(MemFuseError::Storage("WAL queue full (backpressure)"))`
    /// if the flusher channel buffer is full.
    pub async fn try_append_batch(&self, batch: PreparedBatch) -> Result<()> {
        let truncate_guard = self.truncate_lock.lock().await;
        self.try_append_batch_locked(batch, &truncate_guard).await
    }

    pub async fn try_append_batch_locked(
        &self,
        batch: PreparedBatch,
        _guard: &tokio::sync::MutexGuard<'_, ()>,
    ) -> Result<()> {
        if self.is_sealed() {
            return Err(MemFuseError::Storage(format!(
                "Cannot append to sealed WAL segment {}",
                self.path.display()
            )));
        }

        let entries = &batch.0;
        if entries.is_empty() {
            return Ok(());
        }

        #[cfg(feature = "fault-injection")]
        {
            let fail_tx = FAIL_APPEND_FOR_TX.load(std::sync::atomic::Ordering::SeqCst);
            if fail_tx != 0
                && entries
                    .iter()
                    .any(|e| e.tx_id().inner() == fail_tx || fail_tx == u64::MAX)
            {
                FAIL_APPEND_FOR_TX.store(0, std::sync::atomic::Ordering::SeqCst);
                return Err(MemFuseError::Storage(
                    "Simulated WAL append_batch I/O failure via fault injection".into(),
                ));
            }

            let delay_tx = DELAY_APPEND_FOR_TX.load(std::sync::atomic::Ordering::SeqCst);
            if delay_tx != 0 && entries.iter().any(|e| e.tx_id().inner() == delay_tx) {
                let delay_ms = DELAY_APPEND_MS.load(std::sync::atomic::Ordering::SeqCst);
                DELAY_APPEND_FOR_TX.store(0, std::sync::atomic::Ordering::SeqCst);
                if delay_ms > 0 {
                    tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                }
            }
        }

        let estimated_size = entries.len() * 256;
        let mut payload_bytes = Vec::with_capacity(estimated_size);
        let mut last_hmac_val = [0u8; 32];

        if let Some(km) = &self.key_manager {
            let mut batch_plaintext = Vec::with_capacity(estimated_size);
            for entry in entries {
                let bytes = entry.to_bytes()?;
                batch_plaintext.extend_from_slice(&bytes);
                last_hmac_val = entry.checksum;
            }

            let km_clone = Arc::clone(km);
            let encrypted_result =
                tokio::task::spawn_blocking(move || km_clone.encrypt_auto_nonce(&batch_plaintext))
                    .await
                    .map_err(|e| {
                        MemFuseError::Storage(format!("WAL encryption task panicked: {e}"))
                    })?;

            let (encrypted, nonce) = encrypted_result?;
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

        let flusher_tx = {
            let guard = self.flusher_tx.read().unwrap_or_else(|e| e.into_inner());
            guard.clone()
        };

        let tx = flusher_tx
            .ok_or_else(|| MemFuseError::Storage("WAL flusher actor is not enabled".into()))?;

        let (ack_tx, ack_rx) = tokio::sync::oneshot::channel();
        tx.try_send(WalCommand::Append {
            payload: payload_bytes,
            last_hmac_val,
            ack: ack_tx,
        })
        .map_err(|e| match e {
            tokio::sync::mpsc::error::TrySendError::Full(_) => {
                MemFuseError::Storage("WAL queue full (backpressure)".into())
            }
            tokio::sync::mpsc::error::TrySendError::Closed(_) => {
                MemFuseError::Storage("WAL flusher channel closed".into())
            }
        })?;

        ack_rx
            .await
            .map_err(|_| MemFuseError::Storage("WAL flusher dropped".into()))??;
        Ok(())
    }

    /// Helper for creating entries bound to this WAL's current chain.
    #[allow(dead_code)]
    #[deprecated(
        note = "Use prepare_batch with a single-element Vec instead — direct use bypasses chain-fork protection"
    )]
    pub(crate) async fn create_entry(&self, op: WalOp, seq_no: u64) -> Result<WalEntry> {
        let last_hmac = self.last_hmac.lock().await;
        let integrity_key = self.get_integrity_key()?;
        WalEntry::try_new(op, seq_no, &integrity_key, *last_hmac)
    }

    /// Scans the WAL entry by entry, executing full HMAC chain validation, CRC checks, and key manager decryption.
    ///
    /// Invokes `callback(seq_no, entry, end_offset)` for each valid entry.
    /// If `callback` returns `false`, scanning halts early.
    pub async fn scan_entries_with_callback<F>(
        &self,
        file_size: u64,
        mut callback: F,
    ) -> Result<WalVersion>
    where
        F: FnMut(u64, WalEntry, u64) -> bool,
    {
        let flusher_tx = {
            let guard = self.flusher_tx.read().unwrap_or_else(|e| e.into_inner());
            guard.clone()
        };

        let tx = flusher_tx
            .ok_or_else(|| MemFuseError::Storage("WAL flusher actor is not enabled".into()))?;

        let (item_tx, mut item_rx) = tokio::sync::mpsc::unbounded_channel();
        let (ack_tx, ack_rx) = tokio::sync::oneshot::channel();

        tx.send(WalCommand::Scan {
            file_size,
            item_tx,
            ack: ack_tx,
        })
        .await
        .map_err(|_| MemFuseError::Storage("WAL flusher channel closed".into()))?;

        let mut stopped = false;
        while let Some((seq, entry, pos)) = item_rx.recv().await {
            if !stopped && !callback(seq, entry, pos) {
                stopped = true;
            }
        }

        let version = ack_rx
            .await
            .map_err(|_| MemFuseError::Storage("WAL flusher dropped".into()))??;

        Ok(version)
    }

    /// Rewrites legacy V1 or V2 WAL files as V3.
    pub(crate) async fn rewrite_as_v3(
        &self,
        replayed_entries: &[(u64, WalEntry, u64)],
    ) -> Result<()> {
        let integrity_key = self.get_integrity_key()?;

        let flusher_tx = {
            let guard = self.flusher_tx.read().unwrap_or_else(|e| e.into_inner());
            guard.clone()
        };

        let tx = flusher_tx
            .ok_or_else(|| MemFuseError::Storage("WAL flusher actor is not enabled".into()))?;

        let (ack_tx, ack_rx) = tokio::sync::oneshot::channel();
        tx.send(WalCommand::Rewrite {
            replayed_entries: replayed_entries.to_vec(),
            integrity_key,
            ack: ack_tx,
        })
        .await
        .map_err(|_| MemFuseError::Storage("WAL flusher channel closed".into()))?;

        ack_rx
            .await
            .map_err(|_| MemFuseError::Storage("WAL flusher dropped".into()))??;

        Ok(())
    }

    pub async fn truncate(&self, offset: u64, new_last_hmac: [u8; 32]) -> Result<()> {
        let _truncate_guard = self.truncate_lock.lock().await;

        if self.is_sealed() {
            return Err(MemFuseError::Storage(format!(
                "Cannot truncate sealed WAL segment {}",
                self.path.display()
            )));
        }

        let flusher_tx = {
            let guard = self.flusher_tx.read().unwrap_or_else(|e| e.into_inner());
            guard.clone()
        };
        let tx = flusher_tx
            .ok_or_else(|| MemFuseError::Storage("WAL flusher actor is not enabled".into()))?;

        let (ack_tx, ack_rx) = tokio::sync::oneshot::channel();
        tx.send(WalCommand::Truncate {
            offset,
            new_last_hmac,
            ack: ack_tx,
        })
        .await
        .map_err(|_| MemFuseError::Storage("WAL flusher channel closed".into()))?;

        ack_rx.await.map_err(|_| {
            MemFuseError::Storage("WAL flusher dropped ack for truncate command".into())
        })??;

        Ok(())
    }

    pub async fn rotate_and_seal(&self) -> Result<PathBuf> {
        self.sealed.store(true, std::sync::atomic::Ordering::SeqCst);

        let flusher_tx = {
            let mut guard = self
                .flusher_tx
                .write()
                .map_err(|_| MemFuseError::Storage("flusher_tx RwLock poisoned".into()))?;
            guard.take()
        };

        let tx = flusher_tx
            .ok_or_else(|| MemFuseError::Storage("WAL flusher actor is not enabled".into()))?;

        let (ack_tx, ack_rx) = tokio::sync::oneshot::channel();
        tx.send(WalCommand::Seal { ack: ack_tx })
            .await
            .map_err(|_| MemFuseError::Storage("WAL flusher channel closed".into()))?;

        let sealed_path = ack_rx
            .await
            .map_err(|_| MemFuseError::Storage("WAL flusher dropped".into()))??;

        Ok(sealed_path)
    }

    pub async fn find_tx_offset(&self, target_tx_id: TxId) -> Result<(u64, [u8; 32])> {
        let metadata = crate::wal::fs::metadata(&self.path)
            .await
            .map_err(|e| MemFuseError::Storage(e.to_string()))?;
        let file_size = metadata.len();

        let mut last_offset = 0u64;
        let mut last_hmac = [0u8; 32];
        let mut found_rollback_point = false;

        self.scan_entries_with_callback(file_size, |_seq, entry, offset| {
            let entry_tx = entry.tx_id().inner();
            if target_tx_id.inner() < TxId::INTERNAL_BASE && entry_tx >= TxId::INTERNAL_BASE {
                last_offset = offset;
                last_hmac = entry.checksum;
                return true;
            }

            if entry_tx > target_tx_id.inner() {
                found_rollback_point = true;
                return false;
            }
            last_offset = offset;
            last_hmac = entry.checksum;
            true
        })
        .await?;

        let _ = found_rollback_point;
        Ok((last_offset, last_hmac))
    }

    /// Returns a snapshot of the last HMAC written to the log.
    pub async fn last_hmac_snapshot(&self) -> [u8; 32] {
        *self.last_hmac.lock().await
    }

    /// Aktiviert eine einmalige simulierte Truncate-I/O-Failure für Tests.
    /// Der Flusher gibt beim nächsten `WalCommand::Truncate` einen Fehler zurück,
    /// ohne `set_len()` auszuführen. `size` wird daher nicht aktualisiert.
    ///
    /// Setzt `FAIL_TRUNCATE_ONCE = true`. Wird vom Flusher nach Auslösung zurückgesetzt.
    #[cfg(feature = "fault-injection")]
    pub fn arm_truncate_failure_for_test(&self) {
        crate::wal::FAIL_TRUNCATE_ONCE.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

#[allow(dead_code)]
pub(crate) fn set_restrictive_file_acl(path: &Path) -> Result<()> {
    memfuse_sys::set_restrictive_file_acl(path).map_err(|e| MemFuseError::Storage(e.to_string()))
}
