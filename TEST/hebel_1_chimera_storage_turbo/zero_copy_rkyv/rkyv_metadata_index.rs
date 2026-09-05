//! # Chimera Metadata Index
//!
//! Key-Value based metadata storage for Chimera.
//! Implements the `MetadataIndex` trait using RoaringBitmaps and FSTs.

use async_trait::async_trait;
use chimera_core::{
    ChimeraContext, ChimeraError, DocId, IndexObserver, IndexOp, MetadataIndex, NamespaceId,
    Payload, Persist, Result, TxBuffer, TxId,
};
use parking_lot::RwLock;
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use roaring::RoaringTreemap;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::Arc;
use tokio::fs;

/// Represents a parsed filter expression from a user query.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterExpr {
    Equal { key: String, value: Value },
    And(Vec<FilterExpr>),
    Or(Vec<FilterExpr>),
    Not(Box<FilterExpr>),
}

/// In-memory Metadata Index using Roaring Bitmaps & FST with Namespace Isolation
#[derive(Debug)]
pub struct ChimeraMetadataIndex {
    store: RwLock<HashMap<(NamespaceId, DocId), Payload>>,
    all_docs: RwLock<HashMap<NamespaceId, RoaringTreemap>>,
    fst: RwLock<Arc<fst::Map<Vec<u8>>>>,
    mem_keys: RwLock<BTreeMap<String, u32>>,
    bitmaps: RwLock<Vec<RoaringTreemap>>,
    /// Transactional buffer for pending operations.
    tx_buffer: TxBuffer<Payload>,
    /// Immutable zero-copy backend.
    mmap: Option<Arc<memmap2::Mmap>>,
}

impl ChimeraMetadataIndex {
    pub fn try_new() -> Result<Self> {
        let fst_map = fst::Map::from_iter(std::iter::empty::<(Vec<u8>, u64)>())
            .map_err(|e| ChimeraError::Internal(format!("Failed to initialize FST: {}", e)))?;
        Ok(Self {
            store: RwLock::new(HashMap::new()),
            all_docs: RwLock::new(HashMap::new()),
            fst: RwLock::new(Arc::new(fst_map)),
            mem_keys: RwLock::new(BTreeMap::new()),
            bitmaps: RwLock::new(Vec::new()),
            tx_buffer: TxBuffer::new_with_config(64, std::time::Duration::from_secs(60)),
            mmap: None,
        })
    }

    /// Creates a new MetadataIndex.
    ///
    /// # Warning
    /// Legacy constructor that returns an empty index if initialization fails.
    /// Prefer `try_new()` in production code (zero-panic doctrine).
    #[deprecated(note = "Use try_new() for fallible initialization (zero-panic doctrine)")]
    pub fn new() -> Self {
        Self::try_new().unwrap_or_else(|e| {
            tracing::error!(
                "CRITICAL: MetadataIndex init failed: {:?}. Returning empty index.",
                e
            );
            Self {
                store: RwLock::new(HashMap::new()),
                all_docs: RwLock::new(HashMap::new()),
                // Safety: Empty FST map is a known-safe constant.
                fst: RwLock::new(Arc::new(
                    fst::Map::from_iter(std::iter::empty::<(Vec<u8>, u64)>()).unwrap_or_else(
                        |_| {
                            // In the absolute worst case, we use a pre-allocated empty map.
                            // fst::Map doesn't have an easy way to create a 'null' map without iter.
                            // This branch is logically unreachable given try_new() already covers errors.
                            unreachable!("FST initialization of empty iterator failed")
                        },
                    ),
                )),
                mem_keys: RwLock::new(BTreeMap::new()),
                bitmaps: RwLock::new(Vec::new()),
                tx_buffer: TxBuffer::new_with_config(64, std::time::Duration::from_secs(60)),
                mmap: None,
            }
        })
    }

    fn flatten_json(prefix: &str, value: &Value, out: &mut Vec<String>) {
        match value {
            Value::Object(map) => {
                for (k, v) in map {
                    let new_prefix = if prefix.is_empty() {
                        k.clone()
                    } else {
                        format!("{}.{}", prefix, k)
                    };
                    Self::flatten_json(&new_prefix, v, out);
                }
            }
            Value::Array(arr) => {
                for v in arr {
                    Self::flatten_json(prefix, v, out);
                }
            }
            Value::Null => out.push(format!("{}:null", prefix)),
            Value::Bool(b) => out.push(format!("{}:{}", prefix, b)),
            Value::Number(n) => out.push(format!("{}:{}", prefix, n)),
            Value::String(s) => out.push(format!("{}:{}", prefix, s)),
        }
    }

    pub fn index_doc(&self, ns: NamespaceId, doc_id: DocId, metadata: &Value) -> Result<()> {
        let mut kvs = Vec::new();
        Self::flatten_json("", metadata, &mut kvs);

        let mut mem_keys = self.mem_keys.write();
        let mut bitmaps = self.bitmaps.write();
        let fst = self.fst.read().clone();

        for kv in kvs {
            // SPEC-030: Prefix keys with NamespaceId for isolation
            let namespaced_kv = format!("ns:{}:{}", ns.as_str(), kv);
            let value_id = if let Some(id) = fst.get(namespaced_kv.as_bytes()) {
                id as u32
            } else if let Some(&id) = mem_keys.get(&namespaced_kv) {
                id
            } else {
                let id = bitmaps.len() as u32;
                bitmaps.push(RoaringTreemap::new());
                mem_keys.insert(namespaced_kv, id);
                id
            };

            bitmaps[value_id as usize].insert(doc_id.inner());
        }

        self.all_docs
            .write()
            .entry(ns)
            .or_default()
            .insert(doc_id.inner());
        Ok(())
    }

    /// Internal insert implementation called during commit.
    async fn do_insert(
        &self,
        _ctx: &ChimeraContext,
        ns: NamespaceId,
        id: DocId,
        payload: &Payload,
    ) -> Result<()> {
        let mut store = self.store.write();
        store.insert((ns.clone(), id), payload.clone());
        if let Ok(json) = payload.as_json::<Value>() {
            self.index_doc(ns, id, &json)?;
        }
        Ok(())
    }

    fn parse_filter_json(&self, val: &Value) -> Result<FilterExpr> {
        if let Some(obj) = val.as_object() {
            if let Some(eq) = obj.get("$eq") {
                if let Some(eq_obj) = eq.as_object() {
                    let mut iter = eq_obj.iter();
                    if let Some((key, value)) = iter.next() {
                        return Ok(FilterExpr::Equal {
                            key: key.clone(),
                            value: value.clone(),
                        });
                    } else {
                        return Err(ChimeraError::invalid_input(
                            "Empty $eq object in metadata filter",
                        ));
                    }
                }
            }
        }
        Err(ChimeraError::invalid_input(
            "Unsupported or malformed filter format",
        ))
    }

    /// Internal delete implementation called during commit.
    async fn do_delete(&self, _ctx: &ChimeraContext, ns: NamespaceId, id: DocId) -> Result<()> {
        let mut store = self.store.write();
        store.remove(&(ns.clone(), id));

        let mut bitmaps = self.bitmaps.write();
        for bitmap in bitmaps.iter_mut() {
            bitmap.remove(id.inner());
        }
        if let Some(all) = self.all_docs.write().get_mut(&ns) {
            all.remove(id.inner());
        }
        Ok(())
    }

    fn get_bitmap_for_kv(&self, ns: NamespaceId, kv: &str) -> RoaringTreemap {
        let namespaced_kv = format!("ns:{}:{}", ns.as_str(), kv);
        let mut bitmap = RoaringTreemap::new();

        let fst = self.fst.read().clone();
        let val_id = if let Some(id) = fst.get(namespaced_kv.as_bytes()) {
            Some(id as u32)
        } else {
            self.mem_keys.read().get(&namespaced_kv).copied()
        };

        if let Some(id) = val_id {
            let bitmaps = self.bitmaps.read();
            if let Some(b) = bitmaps.get(id as usize) {
                bitmap |= b;
            }
        }

        if let Some(mmap) = &self.mmap {
            let archived = unsafe { rkyv::archived_root::<MetadataIndexState>(mmap) };
            if let Ok(idx) = archived
                .mem_keys
                .binary_search_by(|kv| kv.0.as_str().cmp(namespaced_kv.as_str()))
            {
                let b_id: u32 = archived.mem_keys[idx].1;
                if (b_id as usize) < archived.bitmaps.len() {
                    let b_bytes = &archived.bitmaps[b_id as usize];
                    if !b_bytes.is_empty() {
                        bitmap |=
                            RoaringTreemap::deserialize_from(&b_bytes[..]).unwrap_or_default();
                    }
                }
            }
        }

        bitmap
    }

    pub fn query(&self, ns: NamespaceId, expr: &FilterExpr) -> Result<RoaringTreemap> {
        match expr {
            FilterExpr::Equal { key, value } => {
                let mut kvs = Vec::new();
                Self::flatten_json(key, value, &mut kvs);
                let mut res = RoaringTreemap::new();
                for kv in kvs {
                    res |= self.get_bitmap_for_kv(ns.clone(), &kv);
                }
                Ok(res)
            }
            FilterExpr::And(exprs) => {
                if exprs.is_empty() {
                    return Ok(RoaringTreemap::new());
                }
                let mut res = self.query(ns.clone(), &exprs[0])?;
                for e in exprs.iter().skip(1) {
                    res &= self.query(ns.clone(), e)?;
                }
                Ok(res)
            }
            FilterExpr::Or(exprs) => {
                let mut res = RoaringTreemap::new();
                for e in exprs {
                    res |= self.query(ns.clone(), e)?;
                }
                Ok(res)
            }
            FilterExpr::Not(expr) => {
                let inner = self.query(ns.clone(), expr)?;
                let mut all = self.all_docs.read().get(&ns).cloned().unwrap_or_default();
                if let Some(mmap) = &self.mmap {
                    use rkyv::Deserialize;
                    let archived = unsafe { rkyv::archived_root::<MetadataIndexState>(mmap) };
                    if let Ok(idx) = archived.all_docs.binary_search_by(|kv| {
                        let a_ns: NamespaceId = match kv.0.deserialize(&mut rkyv::Infallible) {
                            Ok(v) => v,
                            Err(e) => match e {},
                        };
                        a_ns.cmp(&ns)
                    }) {
                        let b_bytes = &archived.all_docs[idx].1;
                        if !b_bytes.is_empty() {
                            all |=
                                RoaringTreemap::deserialize_from(&b_bytes[..]).unwrap_or_default();
                        }
                    }
                }
                Ok(all - inner)
            }
        }
    }
}

#[async_trait]
impl IndexObserver for ChimeraMetadataIndex {
    async fn on_prepare(&self, _ctx: &ChimeraContext, _ns: &NamespaceId, tx: TxId) -> Result<()> {
        self.tx_buffer.validate_pending_ops(tx)
    }

    async fn on_commit(&self, ctx: &ChimeraContext, _ns: &NamespaceId, tx: TxId) -> Result<()> {
        let ops = self.tx_buffer.drain(tx);
        for op in ops {
            match op {
                IndexOp::Insert {
                    namespace_id,
                    doc_id,
                    data,
                } => {
                    self.do_insert(ctx, namespace_id, doc_id, &data).await?;
                }
                IndexOp::Delete {
                    namespace_id,
                    doc_id,
                    ..
                } => {
                    self.do_delete(ctx, namespace_id, doc_id).await?;
                }
            }
        }
        Ok(())
    }

    async fn on_rollback(&self, _ctx: &ChimeraContext, _ns: &NamespaceId, tx: TxId) -> Result<()> {
        self.tx_buffer.discard(tx);
        Ok(())
    }

    fn serialize_pending_ops(&self, _ns: &NamespaceId, tx: TxId) -> Vec<u8> {
        self.tx_buffer
            .get_ops(tx)
            .and_then(|ops| bincode::serialize(&ops).ok())
            .unwrap_or_default()
    }
}

#[async_trait]
impl chimera_core::IdempotentApply for ChimeraMetadataIndex {
    async fn apply_idempotent(
        &self,
        ctx: &ChimeraContext,
        _ns: &NamespaceId,
        _tx: TxId,
        payload: &[u8],
    ) -> Result<()> {
        if payload.is_empty() {
            return Ok(());
        }
        let ops: Vec<IndexOp<Payload>> = bincode::deserialize(payload).map_err(|e| {
            ChimeraError::Serialization(format!(
                "Failed to deserialize metadata idempotent payload: {}",
                e
            ))
        })?;

        for op in ops {
            match op {
                IndexOp::Insert {
                    namespace_id,
                    doc_id,
                    data,
                } => {
                    self.do_insert(ctx, namespace_id, doc_id, &data).await?;
                }
                IndexOp::Delete {
                    namespace_id,
                    doc_id,
                    ..
                } => {
                    self.do_delete(ctx, namespace_id, doc_id).await?;
                }
            }
        }
        Ok(())
    }
}

#[async_trait]
impl chimera_core::plugin::ChimeraPlugin for ChimeraMetadataIndex {
    fn name(&self) -> &'static str {
        "metadata"
    }

    async fn on_start(&self, _ctx: &chimera_core::plugin::PluginContext) -> Result<()> {
        Ok(())
    }

    async fn on_prepare(
        &self,
        ctx: &ChimeraContext,
        ns: &NamespaceId,
        tx: TxId,
        _ops: &[chimera_core::plugin::PendingOp],
    ) -> Result<()> {
        IndexObserver::on_prepare(self, ctx, ns, tx).await
    }

    async fn on_commit(&self, ctx: &ChimeraContext, ns: &NamespaceId, tx: TxId) -> Result<()> {
        IndexObserver::on_commit(self, ctx, ns, tx).await
    }

    async fn on_rollback(&self, ctx: &ChimeraContext, ns: &NamespaceId, tx: TxId) -> Result<()> {
        IndexObserver::on_rollback(self, ctx, ns, tx).await
    }

    fn get_involved_docs(&self, ns: &NamespaceId, tx: TxId) -> Vec<chimera_core::QualifiedDocId> {
        self.tx_buffer.get_involved_docs_for_ns(tx, ns)
    }

    fn serialize_pending_ops(&self, _ns: &NamespaceId, tx: TxId) -> Vec<u8> {
        self.tx_buffer
            .get_ops(tx)
            .and_then(|ops| bincode::serialize(&ops).ok())
            .unwrap_or_default()
    }

    fn gc_dirty_txs(&self, _ns: &NamespaceId, _timeout: std::time::Duration) {
        let _ = self.tx_buffer.reap_orphans();
    }

    fn as_metadata_index(&self) -> Option<&dyn chimera_core::traits::MetadataIndex> {
        Some(self)
    }
}

#[async_trait]
impl MetadataIndex for ChimeraMetadataIndex {
    async fn insert(
        &self,
        ctx: &ChimeraContext,
        tx: TxId,
        id: DocId,
        payload: &Payload,
    ) -> Result<()> {
        self.tx_buffer.stage(
            tx,
            IndexOp::Insert {
                namespace_id: ctx.namespace_id(),
                doc_id: id,
                data: payload.clone(),
            },
        );
        Ok(())
    }

    async fn get(&self, ctx: &ChimeraContext, id: DocId) -> Result<Option<Payload>> {
        let ns = ctx.namespace_id();
        let store = self.store.read();
        if let Some(payload) = store.get(&(ns.clone(), id)) {
            return Ok(Some(payload.clone()));
        }

        if let Some(mmap) = &self.mmap {
            use rkyv::Deserialize;
            let archived = unsafe { rkyv::archived_root::<MetadataIndexState>(mmap) };
            if let Ok(idx) = archived.store.binary_search_by(|kv| {
                let a_ns: NamespaceId = match kv.0 .0.deserialize(&mut rkyv::Infallible) {
                    Ok(v) => v,
                    Err(e) => match e {},
                };
                let a_id: DocId = match kv.0 .1.deserialize(&mut rkyv::Infallible) {
                    Ok(v) => v,
                    Err(e) => match e {},
                };
                (a_ns, a_id).cmp(&(ns.clone(), id))
            }) {
                let payload: Payload =
                    match archived.store[idx].1.deserialize(&mut rkyv::Infallible) {
                        Ok(v) => v,
                        Err(e) => match e {},
                    };
                return Ok(Some(payload));
            }
        }
        Ok(None)
    }

    async fn filter(
        &self,
        ctx: &ChimeraContext,
        predicate: &(dyn for<'a> Fn(&'a Payload) -> bool + Send + Sync),
    ) -> Result<Vec<DocId>> {
        let ns = ctx.namespace_id();
        let store = self.store.read();
        let mut res = Vec::new();
        for ((node_ns, id), payload) in store.iter() {
            if *node_ns == ns && predicate(payload) {
                res.push(*id);
            }
        }

        if let Some(mmap) = &self.mmap {
            use rkyv::Deserialize;
            let archived = unsafe { rkyv::archived_root::<MetadataIndexState>(mmap) };
            for kv in archived.store.iter() {
                let a_ns: NamespaceId = match kv.0 .0.deserialize(&mut rkyv::Infallible) {
                    Ok(v) => v,
                    Err(e) => match e {},
                };
                if a_ns == ns {
                    let payload: Payload = match kv.1.deserialize(&mut rkyv::Infallible) {
                        Ok(v) => v,
                        Err(e) => match e {},
                    };
                    if predicate(&payload) {
                        let a_id: DocId = match kv.0 .1.deserialize(&mut rkyv::Infallible) {
                            Ok(v) => v,
                            Err(e) => match e {},
                        };
                        res.push(a_id);
                    }
                }
            }
        }
        Ok(res)
    }

    async fn evaluate(&self, ctx: &ChimeraContext, filter: &str) -> Result<RoaringTreemap> {
        let ns = ctx.namespace_id();
        // Try to parse filter as direct FilterExpr JSON first
        if let Ok(expr) = serde_json::from_str::<FilterExpr>(filter) {
            return self.query(ns, &expr);
        }

        // Fallback: Try to parse as a simple JSON object (e.g. {"$eq": {"key": "val"}})
        if let Ok(val) = serde_json::from_str::<Value>(filter) {
            let expr = self.parse_filter_json(&val)?;
            return self.query(ns, &expr);
        }

        // Final fallback: No match
        Ok(RoaringTreemap::new())
    }

    async fn delete(&self, ctx: &ChimeraContext, tx: TxId, id: DocId) -> Result<()> {
        self.tx_buffer.stage(
            tx,
            IndexOp::Delete {
                namespace_id: ctx.namespace_id(),
                doc_id: id,
                data: None,
            },
        );
        Ok(())
    }
}

/// Serialized state of the Metadata Index.
#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug)]
#[archive(check_bytes)]
pub struct MetadataIndexState {
    pub store: Vec<((NamespaceId, DocId), Payload)>,
    pub all_docs: Vec<(NamespaceId, Vec<u8>)>,
    pub mem_keys: Vec<(String, u32)>,
    pub bitmaps: Vec<Vec<u8>>,
}

#[async_trait]
impl Persist for ChimeraMetadataIndex {
    async fn save(&self, path: &Path) -> Result<()> {
        let state = {
            let store = self.store.read();
            let all_docs = self.all_docs.read();
            let mem_keys = self.mem_keys.read();
            let bitmaps = self.bitmaps.read();

            use rkyv::Deserialize;

            let mut merged_store = HashMap::new();
            let mut merged_all_docs = HashMap::new();
            let mut merged_mem_keys = BTreeMap::new();
            let mut merged_bitmaps: Vec<RoaringTreemap> = Vec::new();

            if let Some(mmap) = &self.mmap {
                let archived = unsafe { rkyv::archived_root::<MetadataIndexState>(mmap) };
                for kv in archived.store.iter() {
                    let a_ns: NamespaceId = kv.0 .0.deserialize(&mut rkyv::Infallible).unwrap();
                    let a_id: DocId = kv.0 .1.deserialize(&mut rkyv::Infallible).unwrap();
                    let payload: Payload = kv.1.deserialize(&mut rkyv::Infallible).unwrap();
                    merged_store.insert((a_ns, a_id), payload);
                }

                for kv in archived.all_docs.iter() {
                    let a_ns: NamespaceId = kv.0.deserialize(&mut rkyv::Infallible).unwrap();
                    if !kv.1.is_empty() {
                        let bitmap =
                            RoaringTreemap::deserialize_from(&kv.1[..]).unwrap_or_default();
                        merged_all_docs.insert(a_ns, bitmap);
                    }
                }

                let mut b_map = Vec::new();
                for b_bytes in archived.bitmaps.iter() {
                    if !b_bytes.is_empty() {
                        let bm = RoaringTreemap::deserialize_from(&b_bytes[..]).unwrap_or_default();
                        b_map.push(bm);
                    } else {
                        b_map.push(RoaringTreemap::new());
                    }
                }

                for kv in archived.mem_keys.iter() {
                    let k: String = kv.0.deserialize(&mut rkyv::Infallible).unwrap();
                    let b_id: u32 = kv.1;
                    merged_mem_keys.insert(k.clone(), merged_bitmaps.len() as u32);
                    if (b_id as usize) < b_map.len() {
                        merged_bitmaps.push(b_map[b_id as usize].clone());
                    } else {
                        merged_bitmaps.push(RoaringTreemap::new());
                    }
                }
            }

            for (k, v) in store.iter() {
                merged_store.insert(k.clone(), v.clone());
            }

            for (ns, bm) in all_docs.iter() {
                let existing = merged_all_docs.entry(ns.clone()).or_default();
                *existing |= bm.clone();
            }

            for (k, idx) in mem_keys.iter() {
                if let Some(existing_idx) = merged_mem_keys.get(k) {
                    merged_bitmaps[*existing_idx as usize] |= bitmaps[*idx as usize].clone();
                } else {
                    merged_mem_keys.insert(k.clone(), merged_bitmaps.len() as u32);
                    merged_bitmaps.push(bitmaps[*idx as usize].clone());
                }
            }

            let mut all_docs_list = Vec::new();
            for (ns, bitmap) in merged_all_docs.iter() {
                let mut b = Vec::new();
                bitmap
                    .serialize_into(&mut b)
                    .map_err(|e| ChimeraError::Serialization(e.to_string()))?;
                all_docs_list.push((ns.clone(), b));
            }
            all_docs_list.sort_by(|a, b| a.0.cmp(&b.0));

            let mut bitmaps_bytes = Vec::new();
            for bitmap in merged_bitmaps.iter() {
                let mut b = Vec::new();
                bitmap
                    .serialize_into(&mut b)
                    .map_err(|e| ChimeraError::Serialization(e.to_string()))?;
                bitmaps_bytes.push(b);
            }

            let mut store_sorted: Vec<_> = merged_store.into_iter().collect();
            store_sorted.sort_by(|a, b| a.0.cmp(&b.0));

            let mut mem_keys_sorted: Vec<_> = merged_mem_keys.into_iter().collect();
            mem_keys_sorted.sort_by(|a, b| a.0.cmp(&b.0));

            MetadataIndexState {
                store: store_sorted,
                all_docs: all_docs_list,
                mem_keys: mem_keys_sorted,
                bitmaps: bitmaps_bytes,
            }
        };

        let bytes = rkyv::to_bytes::<_, 4096>(&state).map_err(|e| {
            ChimeraError::Serialization(format!("Failed to serialize metadata index: {}", e))
        })?;

        let temp_path = path.with_extension("tmp");
        fs::write(&temp_path, bytes).await.map_err(|e| {
            ChimeraError::Serialization(format!("Failed to write metadata index temp file: {}", e))
        })?;

        fs::rename(&temp_path, path).await.map_err(|e| {
            ChimeraError::Serialization(format!("Failed to rename metadata index file: {}", e))
        })?;

        Ok(())
    }

    async fn load(path: &Path) -> Result<Self> {
        let path_buf = path.to_path_buf();
        let mmap = tokio::task::spawn_blocking(move || -> Result<memmap2::Mmap> {
            let file = std::fs::File::open(&path_buf).map_err(|e| {
                ChimeraError::Serialization(format!("Failed to open metadata index file: {}", e))
            })?;
            let mmap = unsafe {
                memmap2::Mmap::map(&file).map_err(|e| {
                    ChimeraError::Serialization(format!(
                        "Failed to mmap metadata index file: {}",
                        e
                    ))
                })?
            };

            let _archived =
                rkyv::check_archived_root::<MetadataIndexState>(&mmap).map_err(|e| {
                    ChimeraError::Internal(format!(
                        "Failed to validate metadata index archive: {}",
                        e
                    ))
                })?;

            Ok(mmap)
        })
        .await
        .map_err(|e| ChimeraError::Internal(format!("Blocking task failed: {}", e)))??;

        let fst_map = fst::Map::from_iter(std::iter::empty::<(Vec<u8>, u64)>())
            .map_err(|e| ChimeraError::Internal(format!("Failed to initialize FST: {}", e)))?;

        Ok(Self {
            store: RwLock::new(HashMap::new()),
            all_docs: RwLock::new(HashMap::new()),
            fst: RwLock::new(Arc::new(fst_map)),
            mem_keys: RwLock::new(BTreeMap::new()),
            bitmaps: RwLock::new(Vec::new()),
            tx_buffer: TxBuffer::new_with_config(64, std::time::Duration::from_secs(60)),
            mmap: Some(Arc::new(mmap)),
        })
    }
}

impl Default for ChimeraMetadataIndex {
    /// Creates a default MetadataIndex.
    ///
    /// # Panics
    /// Panics if FST initialization fails. Use `try_new()` for fallible initialization.
    #[allow(deprecated)]
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chimera_test_utils::async_helpers::test_context;

    #[tokio::test]
    async fn insert_and_get_round_trip() -> Result<()> {
        let idx = ChimeraMetadataIndex::try_new()?;
        let ctx = test_context();
        let tx = TxId::new(1);
        let payload = Payload::from_json(&serde_json::json!({"name": "Alice"}))?;

        idx.insert(&ctx, tx, DocId::new(1), &payload).await?;
        idx.on_commit(&ctx, &ctx.namespace_id(), tx).await?;

        let retrieved = idx.get(&ctx, DocId::new(1)).await?;
        let retrieved =
            retrieved.ok_or_else(|| ChimeraError::Internal("Doc not found".to_string()))?;
        let json: serde_json::Value = retrieved.as_json()?;
        assert_eq!(json["name"], "Alice");
        Ok(())
    }

    #[tokio::test]
    async fn get_missing_returns_none() -> Result<()> {
        let idx = ChimeraMetadataIndex::try_new()?;
        let ctx = test_context();
        assert!(idx.get(&ctx, DocId::new(999)).await?.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn delete_removes_entry() -> Result<()> {
        let idx = ChimeraMetadataIndex::try_new()?;
        let ctx = test_context();
        let tx = TxId::new(1);
        let payload = Payload::new(b"data".to_vec());

        idx.insert(&ctx, tx, DocId::new(5), &payload).await?;
        idx.on_commit(&ctx, &ctx.namespace_id(), tx).await?;
        idx.delete(&ctx, tx, DocId::new(5)).await?;
        idx.on_commit(&ctx, &ctx.namespace_id(), tx).await?;

        assert!(idx.get(&ctx, DocId::new(5)).await?.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn text_query_bitmap() -> Result<()> {
        let idx = ChimeraMetadataIndex::try_new()?;
        let ctx = test_context();
        let tx = TxId::new(1);

        for i in 1..=5 {
            let active = i % 2 == 0;
            let val = serde_json::json!({ "active": active, "group": "test" });
            let payload = Payload::from_json(&val)?;
            idx.insert(&ctx, tx, DocId::new(i), &payload).await?;
            idx.on_commit(&ctx, &ctx.namespace_id(), tx).await?;
        }

        let q1 = FilterExpr::Equal {
            key: "active".to_string(),
            value: serde_json::Value::Bool(true),
        };
        let res = idx.query(NamespaceId::default_ns(), &q1)?;
        assert_eq!(res.len(), 2);
        assert!(res.contains(2));
        assert!(res.contains(4));

        let q2 = FilterExpr::And(vec![
            FilterExpr::Equal {
                key: "group".to_string(),
                value: serde_json::Value::String("test".into()),
            },
            FilterExpr::Not(Box::new(FilterExpr::Equal {
                key: "active".to_string(),
                value: serde_json::Value::Bool(true),
            })),
        ]);
        let res2 = idx.query(NamespaceId::default_ns(), &q2)?;
        assert_eq!(res2.len(), 3);
        assert!(res2.contains(1));
        assert!(res2.contains(3));
        assert!(res2.contains(5));
        Ok(())
    }

    #[tokio::test]
    async fn persistence_round_trip() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let path = temp_dir.path().join("metadata.bin");
        let ctx = test_context();
        let tx = TxId::new(1);

        let idx = ChimeraMetadataIndex::try_new()?;
        let val = serde_json::json!({ "active": true, "group": "test" });
        let payload = Payload::from_json(&val)?;
        idx.insert(&ctx, tx, DocId::new(1), &payload).await?;
        idx.on_commit(&ctx, &ctx.namespace_id(), tx).await?;

        // Save
        idx.save(&path).await?;

        // Load
        let idx2 = ChimeraMetadataIndex::load(&path).await?;

        let retrieved = idx2.get(&ctx, DocId::new(1)).await?;
        let retrieved =
            retrieved.ok_or_else(|| ChimeraError::Internal("Doc not found".to_string()))?;
        let json: serde_json::Value = retrieved.as_json()?;
        assert_eq!(json["active"], true);

        // Check fuzzy query still works (FST rebuild check)
        let q = FilterExpr::Equal {
            key: "group".to_string(),
            value: serde_json::Value::String("test".into()),
        };
        let res = idx2.query(NamespaceId::default_ns(), &q)?;
        assert!(res.contains(1));
        Ok(())
    }
}
