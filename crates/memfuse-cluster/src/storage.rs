use crate::{NodeId, TypeConfig};
use memfuse_core::StorageEngine;
use memfuse_store::lsm::LsmStorage;
use openraft::storage::{RaftLogReader, RaftLogStorage};
use openraft::LogId;
use parking_lot::RwLock as SyncRwLock;
use std::collections::BTreeMap;
use std::ops::RangeBounds;
use std::sync::Arc;

// ─── Snapshot Serialization Format ──────────────────────────────────────────
// Binary format: [u64:entry_count][foreach: u32:key_len, key_bytes, u32:val_len, val_bytes]

fn serialize_snapshot(entries: &[(Vec<u8>, Vec<u8>)]) -> Vec<u8> {
    let mut buf = Vec::new();
    let count = entries.len() as u64;
    buf.extend_from_slice(&count.to_le_bytes());
    for (k, v) in entries {
        let klen = k.len() as u32;
        let vlen = v.len() as u32;
        buf.extend_from_slice(&klen.to_le_bytes());
        buf.extend_from_slice(k);
        buf.extend_from_slice(&vlen.to_le_bytes());
        buf.extend_from_slice(v);
    }
    buf
}

type SnapshotEntries = Vec<(Vec<u8>, Vec<u8>)>;

fn deserialize_snapshot(data: &[u8]) -> Result<SnapshotEntries, String> {
    if data.len() < 8 {
        return Err("Snapshot data too small for entry count".into());
    }
    let count =
        u64::from_le_bytes(data[0..8].try_into().map_err(|_| "Invalid count bytes")?) as usize;

    let mut entries = Vec::with_capacity(count);
    let mut cursor = 8;

    for _ in 0..count {
        if cursor + 4 > data.len() {
            return Err("Truncated key length".into());
        }
        let klen = u32::from_le_bytes(
            data[cursor..cursor + 4]
                .try_into()
                .map_err(|_| "Bad klen")?,
        ) as usize;
        cursor += 4;

        if cursor + klen > data.len() {
            return Err("Truncated key data".into());
        }
        let key = data[cursor..cursor + klen].to_vec();
        cursor += klen;

        if cursor + 4 > data.len() {
            return Err("Truncated value length".into());
        }
        let vlen = u32::from_le_bytes(
            data[cursor..cursor + 4]
                .try_into()
                .map_err(|_| "Bad vlen")?,
        ) as usize;
        cursor += 4;

        if cursor + vlen > data.len() {
            return Err("Truncated value data".into());
        }
        let value = data[cursor..cursor + vlen].to_vec();
        cursor += vlen;

        entries.push((key, value));
    }

    Ok(entries)
}

// ─── Helper: Convert any error to openraft::StorageError ────────────────────

fn to_storage_err<E: std::fmt::Display>(e: E) -> openraft::StorageError<NodeId> {
    openraft::StorageError::IO {
        source: openraft::StorageIOError::new(
            openraft::ErrorSubject::Store,
            openraft::ErrorVerb::Read,
            openraft::AnyError::new(&std::io::Error::other(e.to_string())),
        ),
    }
}

// ─── Raft Log Storage ──────────────────────────────────────────────────────

/// In-memory Raft log storage with LSM-backed state machine.
///
/// The log entries are kept in memory (BTreeMap) for fast access.
/// The state machine delegates to the LSM storage engine for persistence.
pub struct Store {
    /// Reference to the underlying LSM storage engine
    pub lsm: Arc<LsmStorage>,

    // ── Log Storage State ──
    /// Vote state (persisted across restarts via Raft protocol)
    vote: SyncRwLock<Option<openraft::Vote<NodeId>>>,
    /// In-memory log entries indexed by log index
    log: SyncRwLock<BTreeMap<u64, openraft::Entry<TypeConfig>>>,
    /// Last purged log ID (entries before this have been compacted)
    last_purged: SyncRwLock<Option<LogId<NodeId>>>,

    // ── State Machine State ──
    /// Last applied log ID
    last_applied_log: SyncRwLock<Option<LogId<NodeId>>>,
    /// Current cluster membership
    last_membership: SyncRwLock<openraft::StoredMembership<NodeId, crate::Node>>,
    /// Cached last built snapshot
    last_snapshot: SyncRwLock<Option<StoredSnapshot>>,
    /// Monotonic snapshot ID counter
    snapshot_counter: SyncRwLock<u64>,
}

/// Stored snapshot data (the Cursor is not Clone, so we store raw bytes).
struct StoredSnapshot {
    meta: openraft::SnapshotMeta<NodeId, crate::Node>,
    data: Vec<u8>,
}

impl Store {
    /// Creates a new Raft storage instance backed by the given LSM engine.
    pub fn new(lsm: Arc<LsmStorage>) -> Self {
        Self {
            lsm,
            vote: SyncRwLock::new(None),
            log: SyncRwLock::new(BTreeMap::new()),
            last_purged: SyncRwLock::new(None),
            last_applied_log: SyncRwLock::new(None),
            last_membership: SyncRwLock::new(openraft::StoredMembership::default()),
            last_snapshot: SyncRwLock::new(None),
            snapshot_counter: SyncRwLock::new(0),
        }
    }
}

impl Clone for Store {
    fn clone(&self) -> Self {
        Self {
            lsm: Arc::clone(&self.lsm),
            vote: SyncRwLock::new(*self.vote.read()),
            log: SyncRwLock::new(self.log.read().clone()),
            last_purged: SyncRwLock::new(*self.last_purged.read()),
            last_applied_log: SyncRwLock::new(*self.last_applied_log.read()),
            last_membership: SyncRwLock::new(self.last_membership.read().clone()),
            last_snapshot: SyncRwLock::new(None), // Snapshots are not cloned
            snapshot_counter: SyncRwLock::new(*self.snapshot_counter.read()),
        }
    }
}

// ─── RaftLogStorage Implementation ──────────────────────────────────────────

impl RaftLogStorage<TypeConfig> for Store {
    type LogReader = Store;

    async fn get_log_state(
        &mut self,
    ) -> Result<openraft::LogState<TypeConfig>, openraft::StorageError<NodeId>> {
        let log = self.log.read();
        let last_purged = *self.last_purged.read();

        let last_log_id = log
            .iter()
            .next_back()
            .map(|(_, entry)| entry.log_id)
            .or(last_purged);

        Ok(openraft::LogState {
            last_purged_log_id: last_purged,
            last_log_id,
        })
    }

    async fn save_vote(
        &mut self,
        vote: &openraft::Vote<NodeId>,
    ) -> Result<(), openraft::StorageError<NodeId>> {
        *self.vote.write() = Some(*vote);
        Ok(())
    }

    async fn read_vote(
        &mut self,
    ) -> Result<Option<openraft::Vote<NodeId>>, openraft::StorageError<NodeId>> {
        Ok(*self.vote.read())
    }

    async fn get_log_reader(&mut self) -> Self::LogReader {
        self.clone()
    }

    async fn append<I>(
        &mut self,
        entries: I,
        callback: openraft::storage::LogFlushed<TypeConfig>,
    ) -> Result<(), openraft::StorageError<NodeId>>
    where
        I: IntoIterator<Item = openraft::Entry<TypeConfig>> + Send,
    {
        let mut log = self.log.write();
        let mut last_log_id = None;

        for entry in entries {
            last_log_id = Some(entry.log_id);
            log.insert(entry.log_id.index, entry);
        }

        // Signal that log entries are durably stored (in-memory is immediate)
        if let Some(_log_id) = last_log_id {
            callback.log_io_completed(Ok(()));
        }

        Ok(())
    }

    async fn truncate(&mut self, log_id: LogId<u64>) -> Result<(), openraft::StorageError<NodeId>> {
        let mut log = self.log.write();
        // Remove all entries with index >= log_id.index
        let keys_to_remove: Vec<u64> = log.range(log_id.index..).map(|(k, _)| *k).collect();
        for key in keys_to_remove {
            log.remove(&key);
        }
        Ok(())
    }

    async fn purge(&mut self, log_id: LogId<u64>) -> Result<(), openraft::StorageError<NodeId>> {
        {
            let mut last_purged = self.last_purged.write();
            *last_purged = Some(log_id);
        }

        let mut log = self.log.write();
        // Remove all entries with index <= log_id.index
        let keys_to_remove: Vec<u64> = log.range(..=log_id.index).map(|(k, _)| *k).collect();
        for key in keys_to_remove {
            log.remove(&key);
        }
        Ok(())
    }
}

// ─── RaftLogReader Implementation ───────────────────────────────────────────

impl RaftLogReader<TypeConfig> for Store {
    async fn try_get_log_entries<RB>(
        &mut self,
        range: RB,
    ) -> Result<Vec<openraft::Entry<TypeConfig>>, openraft::StorageError<NodeId>>
    where
        RB: RangeBounds<u64> + Send,
    {
        let log = self.log.read();
        let entries: Vec<_> = log.range(range).map(|(_, entry)| entry.clone()).collect();
        Ok(entries)
    }
}

// ─── Snapshot Builder ───────────────────────────────────────────────────────

pub struct StoreSnapshotBuilder {
    pub store: Store,
}

impl openraft::storage::RaftSnapshotBuilder<TypeConfig> for StoreSnapshotBuilder {
    async fn build_snapshot(
        &mut self,
    ) -> Result<openraft::Snapshot<TypeConfig>, openraft::StorageError<NodeId>> {
        // 1. Scan all key-value pairs from the LSM storage
        let entries = self
            .store
            .lsm
            .scan(std::ops::Bound::Unbounded, std::ops::Bound::Unbounded)
            .await
            .map_err(to_storage_err)?;

        // 2. Serialize to binary snapshot format
        let data = serialize_snapshot(&entries);

        // 3. Generate monotonic snapshot ID
        let snapshot_id = {
            let mut counter = self.store.snapshot_counter.write();
            *counter += 1;
            format!("snapshot-{}", *counter)
        };

        // 4. Build metadata
        let last_applied = *self.store.last_applied_log.read();
        let membership = self.store.last_membership.read().clone();

        let meta = openraft::SnapshotMeta {
            last_log_id: last_applied,
            last_membership: membership,
            snapshot_id,
        };

        // 5. Cache the snapshot
        {
            let mut snap = self.store.last_snapshot.write();
            *snap = Some(StoredSnapshot {
                meta: meta.clone(),
                data: data.clone(),
            });
        }

        Ok(openraft::Snapshot {
            meta,
            snapshot: Box::new(std::io::Cursor::new(data)),
        })
    }
}

// ─── RaftStateMachine Implementation ────────────────────────────────────────

impl openraft::storage::RaftStateMachine<TypeConfig> for Store {
    type SnapshotBuilder = StoreSnapshotBuilder;

    async fn applied_state(
        &mut self,
    ) -> Result<
        (
            Option<LogId<NodeId>>,
            openraft::StoredMembership<NodeId, crate::Node>,
        ),
        openraft::StorageError<NodeId>,
    > {
        let last_applied = *self.last_applied_log.read();
        let membership = self.last_membership.read().clone();
        Ok((last_applied, membership))
    }

    async fn apply<I>(&mut self, entries: I) -> Result<Vec<Vec<u8>>, openraft::StorageError<NodeId>>
    where
        I: IntoIterator<Item = openraft::Entry<TypeConfig>> + Send,
        I::IntoIter: Send,
    {
        let mut responses = Vec::new();

        for entry in entries {
            // Track last applied log
            *self.last_applied_log.write() = Some(entry.log_id);

            match entry.payload {
                openraft::EntryPayload::Blank => {
                    responses.push(Vec::new());
                }
                openraft::EntryPayload::Normal(data) => {
                    // The data payload is Vec<u8> — interpret as a key-value put operation.
                    // Format: [u32:key_len][key][rest = value]
                    if data.len() >= 4 {
                        let key_len = u32::from_le_bytes(
                            data[0..4]
                                .try_into()
                                .map_err(|_| to_storage_err("Invalid key length in Raft entry"))?,
                        ) as usize;

                        if data.len() >= 4 + key_len {
                            let key = &data[4..4 + key_len];
                            let value = &data[4 + key_len..];

                            // Use an internal TxId for Raft-applied writes
                            let tx_id = memfuse_core::TxId::internal();
                            self.lsm
                                .put(tx_id, key, value)
                                .await
                                .map_err(to_storage_err)?;
                            self.lsm.commit(tx_id).await.map_err(to_storage_err)?;
                        }
                    }
                    responses.push(Vec::new());
                }
                openraft::EntryPayload::Membership(membership) => {
                    let stored = openraft::StoredMembership::new(Some(entry.log_id), membership);
                    *self.last_membership.write() = stored;
                    responses.push(Vec::new());
                }
            }
        }

        Ok(responses)
    }

    async fn get_snapshot_builder(&mut self) -> Self::SnapshotBuilder {
        StoreSnapshotBuilder {
            store: self.clone(),
        }
    }

    async fn begin_receiving_snapshot(
        &mut self,
    ) -> Result<Box<std::io::Cursor<Vec<u8>>>, openraft::StorageError<NodeId>> {
        Ok(Box::new(std::io::Cursor::new(Vec::new())))
    }

    async fn install_snapshot(
        &mut self,
        meta: &openraft::SnapshotMeta<NodeId, crate::Node>,
        snapshot: Box<std::io::Cursor<Vec<u8>>>,
    ) -> Result<(), openraft::StorageError<NodeId>> {
        // 1. Read the snapshot data
        let data = snapshot.into_inner();

        // 2. Deserialize the key-value entries
        let entries = deserialize_snapshot(&data).map_err(to_storage_err)?;

        // 3. Write all entries to LSM storage
        let tx_id = memfuse_core::TxId::internal();
        for (key, value) in &entries {
            self.lsm
                .put(tx_id, key, value)
                .await
                .map_err(to_storage_err)?;
        }
        self.lsm.commit(tx_id).await.map_err(to_storage_err)?;

        // 4. Update state machine metadata
        *self.last_applied_log.write() = meta.last_log_id;
        *self.last_membership.write() = meta.last_membership.clone();

        // 5. Cache the installed snapshot
        {
            let mut snap = self.last_snapshot.write();
            *snap = Some(StoredSnapshot {
                meta: meta.clone(),
                data,
            });
        }

        Ok(())
    }

    async fn get_current_snapshot(
        &mut self,
    ) -> Result<Option<openraft::Snapshot<TypeConfig>>, openraft::StorageError<NodeId>> {
        let snap = self.last_snapshot.read();
        match snap.as_ref() {
            Some(stored) => Ok(Some(openraft::Snapshot {
                meta: stored.meta.clone(),
                snapshot: Box::new(std::io::Cursor::new(stored.data.clone())),
            })),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_serialization_roundtrip() -> Result<(), String> {
        let entries = vec![
            (b"key1".to_vec(), b"value1".to_vec()),
            (b"key2".to_vec(), b"value2".to_vec()),
            (b"".to_vec(), b"empty-key".to_vec()),
            (b"big-key".to_vec(), vec![0xAB; 1024]),
        ];

        let serialized = serialize_snapshot(&entries);
        let deserialized = deserialize_snapshot(&serialized)?;

        assert_eq!(entries.len(), deserialized.len());
        for (orig, deser) in entries.iter().zip(deserialized.iter()) {
            assert_eq!(orig.0, deser.0);
            assert_eq!(orig.1, deser.1);
        }
        Ok(())
    }

    #[test]
    fn test_snapshot_empty() -> Result<(), String> {
        let entries: Vec<(Vec<u8>, Vec<u8>)> = vec![];
        let serialized = serialize_snapshot(&entries);
        let deserialized = deserialize_snapshot(&serialized)?;
        assert!(deserialized.is_empty());
        Ok(())
    }

    #[test]
    fn test_snapshot_deserialize_truncated() {
        assert!(deserialize_snapshot(&[]).is_err());
        assert!(deserialize_snapshot(&[1, 0, 0, 0, 0, 0, 0, 0]).is_err()); // count=1 but no data
    }
}
