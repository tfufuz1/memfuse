use crate::compaction::Compactor;
use crate::manifest::{ManifestManager, ManifestRecord, SegmentMeta, Version};
use crate::memtable::MemTable;
use crate::sstable::{SSTableBuilder, SSTableReader};
use async_trait::async_trait;
use bytes::Bytes;
use chimera_core::{
    ChimeraContext, ChimeraError, DocId, IndexObserver, IndexOp, NamespaceId, Result,
    SnapshotRegistry, StorageEngine, TxBuffer, TxId,
};
use rkyv::Deserialize;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_stream::Stream;

use chimera_core::budget::{ResourceBudget, ResourceTracker};
use chimera_metrics::ChimeraMetrics;
use std::time::Duration;

#[derive(Clone)]
pub struct LSMConfig {
    pub path: PathBuf,
    pub memtable_size_limit: usize,
    pub max_ram_mb: u64,
    pub tx_timeout: Duration,
}

impl Default for LSMConfig {
    fn default() -> Self {
        Self {
            path: PathBuf::from("storage"),
            memtable_size_limit: 64 * 1024 * 1024,
            max_ram_mb: 2048,
            tx_timeout: Duration::from_secs(60),
        }
    }
}

struct LSMState {
    memtable: Arc<MemTable>,
}

pub struct LSMStorage {
    config: LSMConfig,
    state: RwLock<LSMState>,
    manifest: Arc<ManifestManager>,
    version: Arc<RwLock<Version>>,
    compactor: Compactor,
    tx_buffer: TxBuffer<(Vec<u8>, Vec<u8>)>,
    budget: Arc<ResourceTracker>,
    pub snapshot_registry: Arc<SnapshotRegistry>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct StorageStats {
    pub num_segments: usize,
    pub total_size_bytes: u64,
    pub total_entries: u64,
    pub total_tombstones: u64,
    pub memory_usage_bytes: u64,
}

impl LSMStorage {
    pub fn start_background_tasks(self: Arc<Self>, purge_interval: Duration) {
        let storage = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(purge_interval);
            loop {
                interval.tick().await;
                let _ = storage.tx_buffer.reap_orphans();
            }
        });
    }

    pub async fn stats(&self) -> Result<StorageStats> {
        let v = self.version.read().await;
        let mut total_size = 0;
        let mut total_entries = 0;
        let mut total_tombstones = 0;
        for seg in &v.segments {
            total_size += seg.size_bytes;
            total_entries += seg.entry_count;
            total_tombstones += seg.tombstone_count;
        }
        Ok(StorageStats {
            num_segments: v.segments.len(),
            total_size_bytes: total_size,
            total_entries,
            total_tombstones,
            memory_usage_bytes: self.budget.memory_used(),
        })
    }

    pub async fn new(config: LSMConfig) -> Result<Self> {
        tokio::fs::create_dir_all(&config.path)
            .await
            .map_err(|e| ChimeraError::Storage(e.to_string()))?;
        let (manifest, records) = ManifestManager::open(&config.path).await?;
        let mut version = Version::default();
        for record in records {
            version.apply(record);
        }

        let memtable = MemTable::new();
        let manifest = Arc::new(manifest);
        let version = Arc::new(RwLock::new(version));
        let snapshot_registry = Arc::new(SnapshotRegistry::new());
        let compactor = Compactor::new(
            config.path.clone(),
            manifest.clone(),
            version.clone(),
            snapshot_registry.clone(),
        );
        let budget = Arc::new(ResourceTracker::new(ResourceBudget {
            memory_limit: config.max_ram_mb * 1024 * 1024,
            cpu_cycle_limit: u64::MAX,
        }));

        Ok(Self {
            config: config.clone(),
            state: RwLock::new(LSMState {
                memtable: Arc::new(memtable),
            }),
            manifest,
            version,
            compactor,
            tx_buffer: TxBuffer::new_with_config(64, config.tx_timeout),
            budget,
            snapshot_registry,
        })
    }

    pub async fn last_seq_no(&self) -> u64 {
        self.version.read().await.last_seq_no
    }

    pub async fn snapshot(&self, snapshot_path: PathBuf) -> Result<()> {
        let ctx = ChimeraContext::default();
        self.flush(&ctx).await?;
        tokio::fs::create_dir_all(&snapshot_path)
            .await
            .map_err(|e| ChimeraError::Storage(e.to_string()))?;

        let segments = self.version.read().await.segments.clone();
        for seg in segments {
            let filename = format!("{:06}.sst", seg.id);
            tokio::fs::copy(
                self.config.path.join(&filename),
                snapshot_path.join(&filename),
            )
            .await?;
        }
        // Copy Manifest
        let mut entries = tokio::fs::read_dir(&self.config.path).await?;
        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with("MANIFEST-") || name_str == "CURRENT" {
                tokio::fs::copy(entry.path(), snapshot_path.join(name)).await?;
            }
        }
        Ok(())
    }

    pub fn min_active_seq_no(&self) -> u64 {
        self.snapshot_registry.min_active_seqno()
    }
    pub async fn trigger_compaction(&self) -> Result<bool> {
        self.compactor.trigger_compaction().await
    }
}

#[async_trait]
impl chimera_core::traits::IdempotentApply for LSMStorage {
    async fn apply_idempotent(
        &self,
        _ctx: &ChimeraContext,
        _ns: &NamespaceId,
        _tx: TxId,
        payload: &[u8],
    ) -> Result<()> {
        let mut aligned_data = rkyv::AlignedVec::with_capacity(payload.len());
        aligned_data.extend_from_slice(payload);

        let ops: Vec<IndexOp<(Vec<u8>, Vec<u8>)>> =
            rkyv::check_archived_root::<Vec<IndexOp<(Vec<u8>, Vec<u8>)>>>(&aligned_data)
                .map_err(|e| ChimeraError::Serialization(e.to_string()))?
                .deserialize(&mut rkyv::Infallible)
                .map_err(|e| ChimeraError::Internal(e.to_string()))?;

        let state = self.state.write().await;
        for op in ops {
            let lsn = {
                let mut v = self.version.write().await;
                v.last_seq_no += 1;
                v.last_seq_no
            };
            match op {
                IndexOp::Insert { data, .. } => {
                    state
                        .memtable
                        .put(Bytes::from(data.0), Bytes::from(data.1), lsn);
                }
                IndexOp::Delete { data, .. } => {
                    if let Some((key, _)) = data {
                        state.memtable.put(Bytes::from(key), Bytes::new(), lsn);
                    }
                }
            }
        }
        Ok(())
    }
}

#[async_trait]
impl IndexObserver for LSMStorage {
    async fn on_prepare(&self, _ctx: &ChimeraContext, _ns: &NamespaceId, tx: TxId) -> Result<()> {
        self.tx_buffer.validate_pending_ops(tx)?;
        Ok(())
    }

    async fn on_commit(&self, ctx: &ChimeraContext, _ns: &NamespaceId, tx: TxId) -> Result<()> {
        let ops = self.tx_buffer.drain(tx);
        let mut total_size = 0;
        for op in &ops {
            match op {
                IndexOp::Insert { data, .. } => total_size += (data.0.len() + data.1.len()) as u64,
                IndexOp::Delete { data, .. } => {
                    if let Some((k, _)) = data {
                        total_size += k.len() as u64;
                    }
                }
            }
        }

        self.budget.consume_memory(total_size)?;
        ChimeraMetrics::record_memory_pressure(self.budget.memory_used());

        let state = self.state.read().await;
        for op in ops {
            let seq_no = {
                let mut v = self.version.write().await;
                v.last_seq_no += 1;
                v.last_seq_no
            };

            match op {
                IndexOp::Insert { data, .. } => {
                    state
                        .memtable
                        .put(Bytes::from(data.0), Bytes::from(data.1), seq_no);
                }
                IndexOp::Delete { data, .. } => {
                    if let Some((key, _)) = data {
                        state.memtable.put(Bytes::from(key), Bytes::new(), seq_no);
                    }
                }
            }
        }

        if state.memtable.size() > self.config.memtable_size_limit {
            drop(state);
            self.flush(ctx).await?;
        }
        Ok(())
    }

    async fn on_rollback(&self, _ctx: &ChimeraContext, _ns: &NamespaceId, tx: TxId) -> Result<()> {
        self.tx_buffer.discard(tx);
        Ok(())
    }

    fn serialize_pending_ops(&self, _ns: &NamespaceId, tx: TxId) -> Vec<u8> {
        match self.tx_buffer.get_ops(tx) {
            Some(ops) if !ops.is_empty() => rkyv::to_bytes::<_, 2048>(&ops)
                .map(|v| v.to_vec())
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    }

    fn get_involved_docs(&self, ns: &NamespaceId, tx: TxId) -> Vec<chimera_core::QualifiedDocId> {
        self.tx_buffer.get_involved_docs_for_ns(tx, ns)
    }

    fn gc_dirty_txs(&self, _ns: &NamespaceId, _timeout: Duration) {
        let _ = self.tx_buffer.reap_orphans();
    }
}

#[async_trait]
impl StorageEngine for LSMStorage {
    async fn get(&self, ctx: &ChimeraContext, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let ns_id = ctx.namespace_id();
        let prefix = ns_id.key_prefix_bytes();
        let mut full_key = Vec::with_capacity(prefix.len() + 3 + key.len());
        full_key.extend_from_slice(&prefix);
        full_key.extend_from_slice(b":k:");
        full_key.extend_from_slice(key);
        let key = &full_key;

        let seq_no = self.last_seq_no().await;
        let _guard = self.snapshot_registry.register(seq_no);

        {
            let state = self.state.read().await;
            if let Some((val, _seq)) = state.memtable.get(key) {
                if val.is_empty() {
                    return Ok(None);
                }
                return Ok(Some(val.to_vec()));
            }
        }

        let segments = self.version.read().await.segments.clone();
        for seg in segments.iter().rev() {
            let path = self.config.path.join(format!("{:06}.sst", seg.id));
            let key_vec = key.to_vec();
            let found = tokio::task::spawn_blocking(move || {
                let mut reader = SSTableReader::open(&path).ok()?;
                reader.get(&key_vec).ok().flatten()
            })
            .await
            .map_err(|e| ChimeraError::Internal(e.to_string()))?;

            if let Some((val, _seq)) = found {
                if val.is_empty() {
                    return Ok(None);
                }
                return Ok(Some(val.to_vec()));
            }
        }
        Ok(None)
    }

    async fn put(&self, ctx: &ChimeraContext, tx_id: TxId, key: &[u8], value: &[u8]) -> Result<()> {
        let ns_id = ctx.namespace_id();
        let prefix = ns_id.key_prefix_bytes();
        let mut full_key = Vec::with_capacity(prefix.len() + 3 + key.len());
        full_key.extend_from_slice(&prefix);
        full_key.extend_from_slice(b":k:");
        full_key.extend_from_slice(key);

        let doc_id = if key.len() == 8 {
            DocId::new(u64::from_le_bytes(key.try_into().unwrap_or([0; 8])))
        } else {
            DocId::new(0)
        };

        self.tx_buffer.stage(
            tx_id,
            IndexOp::Insert {
                namespace_id: ctx.namespace_id(),
                doc_id,
                data: (full_key, value.to_vec()),
            },
        );
        Ok(())
    }

    async fn delete(&self, ctx: &ChimeraContext, tx_id: TxId, key: &[u8]) -> Result<()> {
        let ns_id = ctx.namespace_id();
        let prefix = ns_id.key_prefix_bytes();
        let mut full_key = Vec::with_capacity(prefix.len() + 3 + key.len());
        full_key.extend_from_slice(&prefix);
        full_key.extend_from_slice(b":k:");
        full_key.extend_from_slice(key);

        self.tx_buffer.stage(
            tx_id,
            IndexOp::Delete {
                namespace_id: ctx.namespace_id(),
                doc_id: DocId::new(0),
                data: Some((full_key, Vec::new())),
            },
        );
        Ok(())
    }

    fn scan<'a>(
        &'a self,
        ctx: &'a ChimeraContext,
        start: &'a [u8],
        end: &'a [u8],
    ) -> Pin<Box<dyn Stream<Item = Result<(Vec<u8>, Vec<u8>)>> + Send + 'a>> {
        let ns_id = ctx.namespace_id();
        let ns_prefix = ns_id.key_prefix_bytes();
        let mut prefix = ns_prefix.clone();
        prefix.extend_from_slice(b":k:");

        let mut full_start = prefix.clone();
        full_start.extend_from_slice(start);
        let mut full_end = prefix.clone();
        full_end.extend_from_slice(end);

        let start_bytes = Bytes::from(full_start);
        let end_bytes = Bytes::from(full_end);
        let prefix_len = prefix.len();
        let storage_path = self.config.path.clone();

        Box::pin(async_stream::try_stream! {
            let seq_no = self.last_seq_no().await;
            let _guard = self.snapshot_registry.register(seq_no);
            let segments = self.version.read().await.segments.clone();
            let memtable = self.state.read().await.memtable.clone();
            let (tx, mut rx) = tokio::sync::mpsc::channel(64);

            tokio::task::spawn_blocking(move || {
                let mut iters: Vec<crate::merge::BoxedIterator> = Vec::new();
                let mem_data: Vec<_> = memtable.iter().filter(|(k, _, _)| k >= &start_bytes && k < &end_bytes).map(|(k, v, s)| Ok((k, v, s))).collect();
                iters.push(Box::new(mem_data.into_iter()));

                for seg in segments.iter().rev() {
                    let path = storage_path.join(format!("{:06}.sst", seg.id));
                    let reader = match SSTableReader::open(path) {
                        Ok(r) => r,
                        Err(e) => { let _ = tx.blocking_send(Err(ChimeraError::Storage(e.to_string()))); return; }
                    };
                    let start_clone = start_bytes.clone();
                    let end_clone = end_bytes.clone();
                    iters.push(Box::new(reader.iter().map(|res| res.map(|(k, v, s)| (Bytes::from(k), Bytes::from(v), s)).map_err(|e| ChimeraError::Storage(e.to_string())))
                        .filter(move |res| match res { Ok((k, _, _)) => k >= &start_clone && k < &end_clone, _ => true })));
                }

                if let Ok(merge_iter) = crate::merge::MergeIterator::new(iters) {
                    for item in merge_iter { if tx.blocking_send(item).is_err() { break; } }
                }
            });

            while let Some(res) = rx.recv().await {
                let (k, v, _) = res?;
                if !k.starts_with(&prefix) { break; }
                if k.len() >= prefix_len { yield (k[prefix_len..].to_vec(), v.to_vec()); }
            }
        })
    }

    async fn flush(&self, _ctx: &ChimeraContext) -> Result<()> {
        let old_mem = {
            let mut state = self.state.write().await;
            if state.memtable.is_empty() {
                return Ok(());
            }
            let old_mem = state.memtable.clone();
            state.memtable = Arc::new(MemTable::new());
            self.budget.release_memory(old_mem.size() as u64);
            old_mem
        };

        let file_id = {
            let mut v = self.version.write().await;
            let id = v.next_file_number;
            v.next_file_number += 1;
            id
        };

        let sst_path = self.config.path.join(format!("{:06}.sst", file_id));
        let sst_path_clone = sst_path.clone();
        let old_mem_clone = old_mem.clone();

        let result = tokio::task::spawn_blocking(move || {
            let mut builder = SSTableBuilder::new(sst_path_clone, false, old_mem_clone.size())
                .map_err(|e| ChimeraError::Storage(e.to_string()))?;
            for (k, v, s) in old_mem_clone.iter() {
                builder
                    .add(&k, &v, s)
                    .map_err(|e| ChimeraError::Storage(e.to_string()))?;
            }
            builder
                .finish()
                .map_err(|e| ChimeraError::Storage(e.to_string()))
        })
        .await
        .map_err(|e| ChimeraError::Internal(e.to_string()))??;

        let meta = SegmentMeta {
            id: file_id,
            min_seq_no: result.min_seq_no,
            max_seq_no: result.max_seq_no,
            min_key: result.min_key,
            max_key: result.max_key,
            tombstone_count: result.tombstone_count,
            size_bytes: tokio::fs::metadata(&sst_path).await?.len(),
            entry_count: result.entry_count,
            level: 0,
            creation_timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| ChimeraError::Internal(e.to_string()))?
                .as_secs(),
        };

        self.manifest
            .log(&ManifestRecord::AddSegment(meta.clone()))
            .await?;
        self.version.write().await.segments.push(meta);
        let _ = self.compactor.trigger_compaction().await;
        Ok(())
    }
}
