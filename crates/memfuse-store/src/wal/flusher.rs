use memfuse_core::{MemFuseError, Result};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;

use super::{Wal, WalEntry, WalVersion, WAL_V3_HEADER};

pub(crate) enum WalCommand {
    Append {
        payload: Vec<u8>,
        last_hmac_val: [u8; 32],
        ack: tokio::sync::oneshot::Sender<Result<()>>,
    },
    Truncate {
        offset: u64,
        new_last_hmac: [u8; 32],
        ack: tokio::sync::oneshot::Sender<Result<()>>,
    },
    Seal {
        ack: tokio::sync::oneshot::Sender<Result<PathBuf>>,
    },
    Rewrite {
        replayed_entries: Vec<(u64, WalEntry, u64)>,
        integrity_key: [u8; 32],
        ack: tokio::sync::oneshot::Sender<Result<()>>,
    },
    Scan {
        file_size: u64,
        item_tx: tokio::sync::mpsc::UnboundedSender<(u64, WalEntry, u64)>,
        ack: tokio::sync::oneshot::Sender<Result<WalVersion>>,
    },
}

impl std::fmt::Debug for WalCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Append {
                payload,
                last_hmac_val,
                ..
            } => f
                .debug_struct("Append")
                .field("payload_len", &payload.len())
                .field("last_hmac_val", last_hmac_val)
                .finish(),
            Self::Truncate {
                offset,
                new_last_hmac,
                ..
            } => f
                .debug_struct("Truncate")
                .field("offset", offset)
                .field("new_last_hmac", new_last_hmac)
                .finish(),
            Self::Seal { .. } => f.debug_struct("Seal").finish(),
            Self::Rewrite {
                replayed_entries, ..
            } => f
                .debug_struct("Rewrite")
                .field("replayed_entries_len", &replayed_entries.len())
                .finish(),
            Self::Scan { file_size, .. } => f
                .debug_struct("Scan")
                .field("file_size", file_size)
                .finish(),
        }
    }
}

/// Configuration for the WAL background flusher actor.
///
/// Controls how aggressively the flusher coalesces concurrent writes
/// into a single `sync_all()` call to reduce fsync overhead under load,
/// and sets bounded backpressure queue capacity.
#[derive(Debug, Clone, Copy)]
pub struct WalFlusherConfig {
    /// Maximum time to wait for additional messages after receiving the first,
    /// before issuing `sync_all()`. Set to 0 to disable (immediate flush).
    ///
    /// Typical range: 50–500 µs. Higher values coalesce more writes per fsync
    /// at the cost of added tail latency. Default: 100 µs.
    pub batch_window_micros: u64,

    /// Queue capacity for the bounded flusher actor command channel.
    /// Default: 1_024. Must be > 0.
    pub queue_capacity: usize,
}

impl Default for WalFlusherConfig {
    fn default() -> Self {
        Self {
            batch_window_micros: 100,
            queue_capacity: super::DEFAULT_WAL_QUEUE_CAPACITY,
        }
    }
}

use tokio::io::AsyncSeekExt;

impl Wal {
    /// Enables background flusher actor for processing WAL I/O commands sequentially.
    pub(crate) fn enable_flusher_with_config(
        &self,
        mut file: crate::wal::fs::File,
        config: WalFlusherConfig,
    ) -> Result<()> {
        if config.queue_capacity == 0 {
            return Err(MemFuseError::invalid_input(
                "WAL queue_capacity must be greater than 0",
            ));
        }

        let mut tx_guard = self.flusher_tx.write().unwrap_or_else(|e| e.into_inner());
        if tx_guard.is_some() {
            return Ok(());
        }

        let (tx, mut rx) = tokio::sync::mpsc::channel::<WalCommand>(config.queue_capacity);
        let path = self.path.clone();
        let header_written = Arc::clone(&self.header_written);
        let size = Arc::clone(&self.size);
        let last_hmac = Arc::clone(&self.last_hmac);
        let key_manager = self.key_manager.clone();
        let fallback_integrity_key = self.fallback_integrity_key;
        let allow_legacy_integrity_key_fallback = self.allow_legacy_integrity_key_fallback;

        tokio::spawn(async move {
            let mut pending_cmd: Option<WalCommand> = None;

            loop {
                let cmd = match pending_cmd.take() {
                    Some(c) => c,
                    None => match rx.recv().await {
                        Some(c) => c,
                        None => break,
                    },
                };

                match cmd {
                    WalCommand::Append {
                        payload,
                        last_hmac_val: _,
                        ack,
                    } => {
                        let mut batch_payload = payload;
                        let mut acks = vec![ack];

                        while let Ok(next_cmd) = rx.try_recv() {
                            match next_cmd {
                                WalCommand::Append {
                                    payload: p,
                                    last_hmac_val: _,
                                    ack: a,
                                } => {
                                    batch_payload.extend_from_slice(&p);
                                    acks.push(a);
                                }
                                other => {
                                    pending_cmd = Some(other);
                                    break;
                                }
                            }
                        }

                        if pending_cmd.is_none() && config.batch_window_micros > 0 {
                            let deadline = tokio::time::Instant::now()
                                + tokio::time::Duration::from_micros(config.batch_window_micros);
                            loop {
                                match tokio::time::timeout_at(deadline, rx.recv()).await {
                                    Ok(Some(WalCommand::Append {
                                        payload: p,
                                        last_hmac_val: _,
                                        ack: a,
                                    })) => {
                                        batch_payload.extend_from_slice(&p);
                                        acks.push(a);
                                        while let Ok(next_cmd) = rx.try_recv() {
                                            match next_cmd {
                                                WalCommand::Append {
                                                    payload: p,
                                                    last_hmac_val: _,
                                                    ack: a,
                                                } => {
                                                    batch_payload.extend_from_slice(&p);
                                                    acks.push(a);
                                                }
                                                other => {
                                                    pending_cmd = Some(other);
                                                    break;
                                                }
                                            }
                                        }
                                        if pending_cmd.is_some() {
                                            break;
                                        }
                                    }
                                    Ok(Some(other)) => {
                                        pending_cmd = Some(other);
                                        break;
                                    }
                                    Ok(None) => break,
                                    Err(_elapsed) => break,
                                }
                            }
                        }

                        let res: Result<()> = async {
                            let write_header = !header_written
                                .load(std::sync::atomic::Ordering::Acquire)
                                && size.load(std::sync::atomic::Ordering::Acquire) == 0;

                            if write_header {
                                file.write_all(&WAL_V3_HEADER).await.map_err(|e| {
                                    MemFuseError::Storage(format!(
                                        "WAL flusher header write failed for {}: {}",
                                        path.display(),
                                        e
                                    ))
                                })?;
                            }
                            file.write_all(&batch_payload).await.map_err(|e| {
                                MemFuseError::Storage(format!(
                                    "WAL flusher write failed for {}: {}",
                                    path.display(),
                                    e
                                ))
                            })?;
                            file.flush().await.map_err(|e| {
                                MemFuseError::Storage(format!(
                                    "WAL flusher flush failed for {}: {}",
                                    path.display(),
                                    e
                                ))
                            })?;
                            file.sync_all().await.map_err(|e| {
                                MemFuseError::Storage(format!(
                                    "WAL flusher fsync failed for {}: {}",
                                    path.display(),
                                    e
                                ))
                            })?;

                            if write_header {
                                header_written.store(true, std::sync::atomic::Ordering::Release);
                            }

                            let written_len = (if write_header { WAL_V3_HEADER.len() } else { 0 })
                                + batch_payload.len();

                            size.fetch_add(written_len as u64, std::sync::atomic::Ordering::SeqCst);

                            Ok(())
                        }
                        .await;

                        for ack in acks {
                            let send_res = match &res {
                                Ok(()) => Ok(()),
                                Err(e) => Err(MemFuseError::Storage(e.to_string())),
                            };
                            let _ = ack.send(send_res);
                        }
                    }

                    WalCommand::Truncate {
                        offset,
                        new_last_hmac,
                        ack,
                    } => {
                        let res: Result<()> = async {
                            #[cfg(feature = "fault-injection")]
                            if crate::wal::FAIL_TRUNCATE_ONCE
                                .compare_exchange(
                                    true,
                                    false,
                                    std::sync::atomic::Ordering::SeqCst,
                                    std::sync::atomic::Ordering::SeqCst,
                                )
                                .is_ok()
                            {
                                return Err(MemFuseError::Storage(
                                    "Simulated WAL truncate I/O failure (FAIL_TRUNCATE_ONCE)"
                                        .into(),
                                ));
                            }

                            let old_size = size.swap(offset, std::sync::atomic::Ordering::SeqCst);

                            if let Err(e) = file.set_len(offset).await {
                                size.store(old_size, std::sync::atomic::Ordering::SeqCst);
                                return Err(MemFuseError::Storage(format!(
                                    "WAL truncate failed: {e}"
                                )));
                            }
                            size.store(offset, std::sync::atomic::Ordering::SeqCst);
                            if offset < 4 {
                                header_written.store(false, std::sync::atomic::Ordering::Release);
                            }

                            file.sync_all().await.map_err(|e| {
                                MemFuseError::Storage(format!("WAL truncate fsync failed: {e}"))
                            })?;

                            file.seek(std::io::SeekFrom::Start(offset))
                                .await
                                .map_err(|e| {
                                    MemFuseError::Storage(format!(
                                        "WAL seek after truncate failed: {e}"
                                    ))
                                })?;

                            let mut last_hmac_guard = last_hmac.lock().await;
                            *last_hmac_guard = new_last_hmac;

                            Ok(())
                        }
                        .await;

                        let _ = ack.send(res);
                    }

                    WalCommand::Seal { ack } => {
                        let res: Result<PathBuf> = async {
                            file.sync_all().await.map_err(|e| {
                                MemFuseError::Storage(format!(
                                    "WAL fsync vor rotate_and_seal fehlgeschlagen: {}",
                                    e
                                ))
                            })?;

                            let micros = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_micros();
                            let sealed_name = format!(
                                "{}.sealed.{}",
                                path.file_name().and_then(|n| n.to_str()).unwrap_or("wal"),
                                micros
                            );
                            let sealed_path = path.with_file_name(sealed_name);

                            crate::wal::fs::rename(&path, &sealed_path)
                                .await
                                .map_err(|e| {
                                    MemFuseError::Storage(format!(
                                        "WAL rotate_and_seal rename {} → {} fehlgeschlagen: {}",
                                        path.display(),
                                        sealed_path.display(),
                                        e
                                    ))
                                })?;

                            crate::util::fsync_parent_dir(&sealed_path).await?;

                            let mut perms = crate::wal::fs::metadata(&sealed_path)
                                .await
                                .map_err(|e| {
                                    MemFuseError::Storage(format!(
                                        "WAL metadata nach seal fehlgeschlagen: {}",
                                        e
                                    ))
                                })?
                                .permissions();
                            perms.set_readonly(true);
                            crate::wal::fs::set_permissions(&sealed_path, perms)
                                .await
                                .map_err(|e| {
                                    MemFuseError::Storage(format!(
                                        "WAL set_readonly fehlgeschlagen: {}",
                                        e
                                    ))
                                })?;

                            Ok(sealed_path)
                        }
                        .await;

                        let _ = ack.send(res);
                    }

                    WalCommand::Rewrite {
                        replayed_entries,
                        integrity_key,
                        ack,
                    } => {
                        let res: Result<()> = async {
                            let mut v3_entries = Vec::with_capacity(replayed_entries.len());
                            let mut prev_hmac = [0u8; 32];

                            for (_, entry, _) in &replayed_entries {
                                let v3_entry = WalEntry::try_new(
                                    entry.op.clone(),
                                    entry.seq_no,
                                    &integrity_key,
                                    prev_hmac,
                                )?;
                                prev_hmac = v3_entry.checksum;
                                v3_entries.push(v3_entry);
                            }

                            file.seek(std::io::SeekFrom::Start(0)).await.map_err(|e| {
                                MemFuseError::Storage(format!(
                                    "WAL seek failed during migration: {}",
                                    e
                                ))
                            })?;
                            file.set_len(0).await.map_err(|e| {
                                MemFuseError::Storage(format!(
                                    "WAL truncate failed during migration: {}",
                                    e
                                ))
                            })?;

                            let mut total_bytes = Vec::new();
                            total_bytes.extend_from_slice(&WAL_V3_HEADER);

                            let mut last_hmac_val = [0u8; 32];
                            if let Some(km) = &key_manager {
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

                            file.write_all(&total_bytes).await.map_err(|e| {
                                MemFuseError::Storage(format!("WAL migration write failed: {}", e))
                            })?;
                            file.flush().await.map_err(|e| {
                                MemFuseError::Storage(format!("WAL migration flush failed: {}", e))
                            })?;
                            file.sync_all().await.map_err(|e| {
                                MemFuseError::Storage(format!("WAL migration fsync failed: {}", e))
                            })?;

                            size.store(
                                total_bytes.len() as u64,
                                std::sync::atomic::Ordering::SeqCst,
                            );
                            header_written.store(true, std::sync::atomic::Ordering::Release);
                            let mut last_hmac_guard = last_hmac.lock().await;
                            *last_hmac_guard = last_hmac_val;

                            Ok(())
                        }
                        .await;

                        let _ = ack.send(res);
                    }

                    WalCommand::Scan {
                        file_size,
                        item_tx,
                        ack,
                    } => {
                        let res = crate::wal::io::do_scan_entries_with_callback(
                            &mut file,
                            file_size,
                            &path,
                            key_manager.as_deref(),
                            fallback_integrity_key,
                            allow_legacy_integrity_key_fallback,
                            |seq, entry, pos| item_tx.send((seq, entry, pos)).is_ok(),
                        )
                        .await;

                        let _ = ack.send(res);
                    }
                }
            }
        });

        *tx_guard = Some(tx);
        Ok(())
    }
}
