//! Checkpoint-Registry für Time-Travel und MVCC-basiertes Snapshotting (gemäß ADR-011).
//!
//! # Öffentliche Checkpoint-Subsystem Architecture (ADR-011)
//! `memfuse-checkpoint` ist der **einzige öffentlich sichtbare Einstiegspunkt** für das Checkpoint-Konzept.
//! Es stellt den Trait [`memfuse_core::traits::CheckpointCoordinator`], die Registrie [`PersistentCheckpointStore`]
//! sowie den RAII-Guard [`CheckpointGuard`] für automatisches Rollback bei Fehlern bereit.
//!
//! **Hinweis zur Abgrenzung:**
//! Das Crate `memfuse-store` besitzt ein lokales, crate-internes Checkpoint-Modul (`pub(crate)`). Dieses ist ein reines
//! Implementierungsdetail für MVCC-Snapshot-Pinning (gekoppelt an `SnapshotRegistry`) und darf NIEMALS direkt von außerhalb
//! des Store-Crates verwendet werden.
//!
//! # Architektur
//! `PersistentCheckpointStore` delegiert Persistenz an ein [`memfuse_core::StorageEngine`]-Objekt
//! und cacht aktive Checkpoints in einem thread-sicheren In-Memory-Store (`parking_lot::RwLock`).

#![forbid(unsafe_code)]

// FILE-CONTEXT
// STAND:       2026-08-30T22:00:34Z (SESSION: a140747b)
// ZWECK:       RAII CheckpointGuard + persistente Snapshot-Verwaltung
// INVARIANTEN: Lock Hierarchy: write_lock -> name_index -> checkpoints; GC safety by pinning before store writes
// HOTSPOTS:    CheckpointGuard::for_agent_step(), PersistentCheckpointStore::create_checkpoint(), PersistentCheckpointStore::list_checkpoints()
// SIEHE AUCH:  ADR-011

use async_trait::async_trait;
use memfuse_core::{MemFuseError, Result, TxId, WorkflowState};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

static CHECKPOINT_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Maximum allowed components per checkpoint manifest to prevent memory exhaustion.
pub const MAX_COMPONENTS: usize = 1000;
/// Maximum allowed checkpoints scanned in list operations to prevent resource exhaustion.
pub const MAX_CHECKPOINTS: usize = 10_000;

/// AI-TAG[INPUT-VALIDATION][MED] AGT-CKPT-001 (TS:2026-08-30T22:00:34Z) (SESSION:a140747b)
/// Validates identifier strings (checkpoint name, collection ID) against empty/whitespace, size limits,
/// null bytes, and control characters.
fn validate_identifier(field_name: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(MemFuseError::InvalidInput(format!(
            "{field_name} cannot be empty or whitespace-only"
        )));
    }
    if value.len() > 256 {
        return Err(MemFuseError::InvalidInput(format!(
            "{field_name} exceeds maximum length of 256 characters (got {})",
            value.len()
        )));
    }
    if value.contains('\0') {
        return Err(MemFuseError::InvalidInput(format!(
            "{field_name} cannot contain null bytes"
        )));
    }
    if value.contains('\r') || value.contains('\n') {
        return Err(MemFuseError::InvalidInput(format!(
            "{field_name} cannot contain line breaks or control characters"
        )));
    }
    Ok(())
}

fn monotonic_timestamp_ms() -> u64 {
    let wall_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    // Ensure monotonic: max(wall_clock, last_seen)
    CHECKPOINT_COUNTER
        .fetch_max(wall_ms, Ordering::SeqCst)
        .max(wall_ms)
}

/// AI-TAG[PANIC-SAFETY][CRITICAL] RESOLVED: AGT-CKPT-f3a1b2c4 (TS:2026-08-29T08:06:29Z) (SESSION:14348074)
/// Fault-Injection-Tests in
/// tests/manifest_fault_injection.rs beweisen atomare Schreib-Semantik
/// und Tamper-Erkennung via Blake3-Checksum.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointManifest {
    pub meta: CheckpointMeta,
    pub components: Vec<String>,
    pub checksum: String,
}

impl CheckpointManifest {
    pub fn new(meta: CheckpointMeta, components: Vec<String>) -> Result<Self> {
        validate_identifier("Checkpoint name", &meta.name)?;
        validate_identifier("Collection ID", &meta.collection_id)?;
        if components.len() > MAX_COMPONENTS {
            return Err(MemFuseError::InvalidInput(format!(
                "Checkpoint component count exceeds limit of {MAX_COMPONENTS} (got {})",
                components.len()
            )));
        }
        for comp in &components {
            validate_identifier("Checkpoint component name", comp)?;
        }
        let payload = serde_json::to_vec(&(&meta, &components))
            .map_err(|e| MemFuseError::Serialization(e.to_string()))?;
        let checksum = blake3::hash(&payload).to_hex().to_string();
        Ok(Self {
            meta,
            components,
            checksum,
        })
    }

    pub fn verify(&self) -> Result<()> {
        let payload = serde_json::to_vec(&(&self.meta, &self.components))
            .map_err(|e| MemFuseError::Serialization(e.to_string()))?;
        let expected = blake3::hash(&payload).to_hex().to_string();
        if self.checksum != expected {
            return Err(MemFuseError::Serialization(format!(
                "Checkpoint manifest checksum mismatch for '{}': expected {}, got {}",
                self.meta.name, expected, self.checksum
            )));
        }
        Ok(())
    }
}

/// Metadata for a persistent checkpoint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointMeta {
    pub name: String,
    pub collection_id: String,
    pub seq_no: u64,
    pub tx_id: TxId,
    pub metadata: serde_json::Value,
    pub created_at: u64,
}

impl CheckpointMeta {
    pub fn into_workflow_state(&self) -> WorkflowState {
        WorkflowState {
            tx: self.tx_id,
            graph_hash: format!("seq-{}", self.seq_no),
        }
    }
}

/// Point-in-Time Checkpoint representing an agent step or transaction boundary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StateCheckpoint {
    pub tx_id: TxId,
    pub timestamp_ms: u64,
}

/// RAII Guard that rolls back a checkpoint if not explicitly committed.
/// Prevents transaction leaks if the process panics or drops early.
pub struct CheckpointGuard<S: memfuse_core::StorageEngine> {
    checkpoint: Option<StateCheckpoint>,
    storage: Arc<S>,
}

impl<S: memfuse_core::StorageEngine> CheckpointGuard<S> {
    pub fn new(checkpoint: StateCheckpoint, storage: Arc<S>) -> Self {
        Self {
            checkpoint: Some(checkpoint),
            storage,
        }
    }

    /// Erstellt einen neuen CheckpointGuard für einen Agenten-Schritt.
    pub async fn for_agent_step(storage: Arc<S>, tx: TxId) -> Result<Self> {
        let cp = StateCheckpoint {
            tx_id: tx,
            timestamp_ms: monotonic_timestamp_ms(),
        };
        Ok(Self::new(cp, storage))
    }

    pub fn checkpoint(&self) -> Result<&StateCheckpoint> {
        self.checkpoint
            .as_ref()
            .ok_or_else(|| MemFuseError::Internal("Checkpoint already consumed".into()))
    }

    pub fn commit(mut self) -> Result<StateCheckpoint> {
        self.checkpoint
            .take()
            .ok_or_else(|| MemFuseError::Internal("Checkpoint already consumed".into()))
    }
}

impl<S: memfuse_core::StorageEngine> Drop for CheckpointGuard<S> {
    fn drop(&mut self) {
        if let Some(cp) = self.checkpoint.take() {
            tracing::warn!(tx_id = ?cp.tx_id, "CheckpointGuard ohne commit gedroppt.");
            let storage_clone = Arc::clone(&self.storage);
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move {
                    if let Err(e) = storage_clone.rollback_to_tx(cp.tx_id).await {
                        tracing::error!("CheckpointGuard auto-rollback fehlgeschlagen: {e}");
                    }
                });
            } else {
                tracing::error!(
                    tx_id = ?cp.tx_id,
                    "CheckpointGuard außerhalb tokio-Runtime gedroppt. Rollback übersprungen."
                );
            }
        }
    }
}

/// Trait für die Checkpoint-Verwaltung.
#[async_trait]
pub trait CheckpointRegistry: memfuse_core::traits::Checkpoint + Send + Sync {
    async fn save_checkpoint(&self, meta: CheckpointMeta) -> Result<()>;
    async fn load_checkpoint(&self, seq_no: u64) -> Result<Option<CheckpointMeta>>;
    async fn list_checkpoints(&self) -> Result<Vec<CheckpointMeta>>;
}

/// Registry für gespeicherte Checkpoints mit Thread-sicherem Zustand.
///
/// # Lock-Hierarchie & Invarianten
/// Um Deadlocks bei nebenläufigen Zugriffen zu verhindern, MUSS die Sperren-Akquise
/// strikt der folgenden Reihenfolge folgen:
/// 1. `write_lock` (`tokio::sync::Mutex<()>`) - Mutierende Storage-Operationen (create/drop/restore)
/// 2. `name_index` (`parking_lot::RwLock<HashMap<String, u64>>`) - In-Memory Lookup Name -> seq_no
/// 3. `checkpoints` (`parking_lot::RwLock<HashMap<u64, CheckpointMeta>>`) - In-Memory Map seq_no -> Meta
///
/// Eine höher in der Hierarchie stehende Sperre darf NIEMALS akquiriert werden,
/// während bereits eine niedere Sperre gehalten wird.
///
/// - Alle Methoden sind durch `RwLock` thread-sicher
/// - `StorageEngine`-Zugriffe nutzen atomare Transaktionen via `TxId`
/// - Keine Panics (Zero-Panic Doctrine)
pub struct PersistentCheckpointStore<S: memfuse_core::StorageEngine> {
    storage: Arc<S>,
    /// Registrierte Checkpoints im Arbeitsspeicher — geschützt durch RwLock (seq_no -> meta)
    checkpoints: RwLock<HashMap<u64, CheckpointMeta>>,
    /// O(1) Index für Name -> seq_no Lookup
    name_index: RwLock<HashMap<String, u64>>,
    /// Namespace-Präfix für Storage-Keys
    namespace: String,
    /// Lock für sequentielle Schreiboperationen auf den Storage (HIGH-002)
    write_lock: tokio::sync::Mutex<()>,
    /// Atomarer Zähler für interne TxIds (vermeidet Kollisionen)
    tx_counter: AtomicU64,
}

impl<S: memfuse_core::StorageEngine> PersistentCheckpointStore<S> {
    pub fn new(storage: Arc<S>, namespace: impl Into<String>) -> Self {
        Self {
            storage,
            checkpoints: RwLock::new(HashMap::new()),
            name_index: RwLock::new(HashMap::new()),
            namespace: namespace.into(),
            write_lock: tokio::sync::Mutex::new(()),
            tx_counter: AtomicU64::new(0),
        }
    }

    // INVARIANT: Checkpoint TxIds use INTERNAL_BASE+n range to avoid
    // collision with Collection-sequenced TxIds [1, ~10^12].
    // See: DECISIONS.md AGT-GRAPH-001, TxId::INTERNAL_BASE
    fn allocate_tx(&self) -> Result<TxId> {
        let raw = self.tx_counter.fetch_add(1, Ordering::SeqCst);
        if raw >= 1_000_000 {
            return Err(MemFuseError::Internal(
                "Checkpoint TxId counter overflow".to_string(),
            ));
        }
        Ok(TxId::new(TxId::INTERNAL_BASE + raw))
    }

    #[deprecated(
        since = "0.1.0",
        note = "Use `allocate_tx()` instead — both methods are functionally identical, `allocate_tx()` is the canonical public API."
    )]
    #[allow(dead_code)]
    fn next_tx(&self) -> Result<TxId> {
        self.allocate_tx()
    }

    /// Creates an ephemeral transactional checkpoint RAII guard.
    /// If the returned guard is dropped without calling `.commit()`, the underlying storage
    /// is automatically rolled back to `tx_id`.
    pub fn create_guard(&self, tx_id: TxId) -> Result<CheckpointGuard<S>> {
        let timestamp_ms = monotonic_timestamp_ms();
        let cp = StateCheckpoint {
            tx_id,
            timestamp_ms,
        };
        Ok(CheckpointGuard::new(cp, Arc::clone(&self.storage)))
    }

    /// Creates a new persistent checkpoint.
    pub async fn create_checkpoint(
        &self,
        name: &str,
        collection_id: &str,
        seq_no: u64,
        tx_id: TxId,
        metadata: serde_json::Value,
    ) -> Result<CheckpointMeta> {
        validate_identifier("Checkpoint name", name)?;
        validate_identifier("Collection ID", collection_id)?;

        let meta = CheckpointMeta {
            name: name.to_string(),
            collection_id: collection_id.to_string(),
            seq_no,
            tx_id,
            metadata,
            created_at: monotonic_timestamp_ms(),
        };

        let _guard = self.write_lock.lock().await;

        // Lade alten Checkpoint (für späteres Unpin)
        let old_checkpoint = self.get_checkpoint_internal(name).await?;

        // 1. Pin new checkpoint's seq_no (MUST happen BEFORE storage.save())
        self.storage.pin_checkpoint(seq_no).await?;

        // 2. storage.save() the new checkpoint
        let save_result = self.save_checkpoint_internal(meta.clone()).await;

        // 3. If save() fails: unpin the new seq_no, return error
        if let Err(e) = save_result {
            if let Err(unpin_err) = self.storage.unpin_checkpoint(seq_no).await {
                tracing::warn!(
                    seq = seq_no,
                    "Failed to unpin new checkpoint after save failure: {unpin_err}"
                );
            }
            return Err(e);
        }

        // 4. If save() succeeds: unpin the old checkpoint's seq_no
        if let Some(old) = old_checkpoint {
            if old.seq_no != seq_no {
                if let Err(e) = self.storage.unpin_checkpoint(old.seq_no).await {
                    // INTENTIONAL: Unpin of the old checkpoint failed. This is non-fatal —
                    // the old seq_no remains pinned, delaying SSTable GC but not causing
                    // data loss. The orphaned pin will clear when the collection is reopened
                    // or when the old checkpoint is explicitly dropped.
                    tracing::warn!(
                        old_seq = old.seq_no,
                        "Konnte alten Checkpoint nicht entpinnen: {e}"
                    );
                }
                // Alten Checkpoint aus Cache entfernen
                self.checkpoints.write().remove(&old.seq_no);
                self.name_index.write().remove(&old.name);
            }
        }

        // 5. Update the stored checkpoint reference
        self.checkpoints.write().insert(seq_no, meta.clone());
        self.name_index.write().insert(name.to_string(), seq_no);

        Ok(meta)
    }

    /// Deletes a persistent checkpoint by name.
    pub async fn drop_checkpoint(&self, name: &str) -> Result<()> {
        validate_identifier("Checkpoint name", name)?;
        let _guard = self.write_lock.lock().await;

        if let Some(checkpoint) = self.get_checkpoint_internal(name).await? {
            // 1. Zuerst aus Storage löschen (mit eindeutiger TxId)
            let key = format!("{}:checkpoint:{}", self.namespace, name);

            // FIX CHK-002: Generiere eine eindeutige TxId statt INTERNAL_BASE
            let unique_tx = self.allocate_tx()?;

            if let Err(e) = self.storage.delete(unique_tx, key.as_bytes()).await {
                if let Err(rb_err) = self.storage.rollback(unique_tx).await {
                    tracing::warn!(tx = ?unique_tx, error = %rb_err, "Storage rollback failed during drop_checkpoint delete");
                }
                return Err(e);
            }
            if let Err(e) = self.storage.commit(unique_tx).await {
                if let Err(rb_err) = self.storage.rollback(unique_tx).await {
                    tracing::warn!(tx = ?unique_tx, error = %rb_err, "Storage rollback failed during drop_checkpoint commit");
                }
                return Err(e);
            }

            // 2. Erst nach erfolgreichem Storage-Delete entpinnen
            if let Err(e) = self.storage.unpin_checkpoint(checkpoint.seq_no).await {
                tracing::warn!(
                    seq = checkpoint.seq_no,
                    "Unpin nach drop fehlgeschlagen: {e}"
                );
            }

            // 3. Cache bereinigen
            self.checkpoints.write().remove(&checkpoint.seq_no);
            self.name_index.write().remove(&checkpoint.name);
        }
        Ok(())
    }

    /// Helper for internal saving logic. Uses name as key for uniqueness.
    async fn save_checkpoint_internal(&self, meta: CheckpointMeta) -> Result<()> {
        let key = format!("{}:checkpoint:{}", self.namespace, meta.name);
        let manifest = CheckpointManifest::new(meta.clone(), vec!["storage".to_string()])?;
        let value = serde_json::to_vec(&manifest)
            .map_err(|e| MemFuseError::Serialization(e.to_string()))?;

        let tx = self.allocate_tx()?;
        if let Err(e) = self.storage.put(tx, key.as_bytes(), &value).await {
            if let Err(rb_err) = self.storage.rollback(tx).await {
                tracing::warn!(tx = ?tx, error = %rb_err, "Storage rollback failed during save_checkpoint_internal put");
            }
            return Err(e);
        }
        if let Err(e) = self.storage.commit(tx).await {
            if let Err(rb_err) = self.storage.rollback(tx).await {
                tracing::warn!(tx = ?tx, error = %rb_err, "Storage rollback failed during save_checkpoint_internal commit");
            }
            return Err(e);
        }

        // In-Memory Cache aktualisieren
        self.name_index
            .write()
            .insert(meta.name.clone(), meta.seq_no);
        self.checkpoints.write().insert(meta.seq_no, meta);
        Ok(())
    }

    /// Internal helper to get checkpoint by name without extra locking.
    async fn get_checkpoint_internal(&self, name: &str) -> Result<Option<CheckpointMeta>> {
        // Erst O(1) In-Memory Name-Index prüfen
        {
            let name_idx = self.name_index.read();
            if let Some(&seq_no) = name_idx.get(name) {
                let cache = self.checkpoints.read();
                if let Some(cp) = cache.get(&seq_no) {
                    return Ok(Some(cp.clone()));
                }
            }
        }

        // Storage direkt fragen
        let key = format!("{}:checkpoint:{}", self.namespace, name);
        match self.storage.get(key.as_bytes()).await? {
            Some(bytes) => {
                let meta =
                    if let Ok(manifest) = serde_json::from_slice::<CheckpointManifest>(&bytes) {
                        manifest.verify()?;
                        manifest.meta
                    } else {
                        serde_json::from_slice::<CheckpointMeta>(&bytes)
                            .map_err(|e| MemFuseError::Serialization(e.to_string()))?
                    };
                self.name_index
                    .write()
                    .insert(meta.name.clone(), meta.seq_no);
                self.checkpoints.write().insert(meta.seq_no, meta.clone());
                Ok(Some(meta))
            }
            None => Ok(None),
        }
    }

    /// Public inherent methods for compatibility
    pub async fn list_checkpoints(&self) -> Result<Vec<CheckpointMeta>> {
        let prefix = format!("{}:checkpoint:", self.namespace);
        let entries: Vec<(Vec<u8>, Vec<u8>)> = self.storage.scan_prefix(prefix.as_bytes()).await?;

        if entries.len() > MAX_CHECKPOINTS {
            return Err(MemFuseError::InvalidInput(format!(
                "Checkpoint entries count ({}) exceeds maximum scan limit of {}",
                entries.len(),
                MAX_CHECKPOINTS
            )));
        }

        let mut result = Vec::with_capacity(entries.len());
        for (_key_bytes, value_bytes) in entries {
            let meta =
                if let Ok(manifest) = serde_json::from_slice::<CheckpointManifest>(&value_bytes) {
                    manifest.verify()?;
                    manifest.meta
                } else {
                    serde_json::from_slice::<CheckpointMeta>(&value_bytes)
                        .map_err(|e| MemFuseError::Serialization(e.to_string()))?
                };
            result.push(meta);
        }

        // Cache synchronisieren gemäß Lock-Hierarchie: name_index -> checkpoints
        {
            let mut name_idx = self.name_index.write();
            let mut cache = self.checkpoints.write();
            cache.clear();
            name_idx.clear();
            for meta in &result {
                cache.insert(meta.seq_no, meta.clone());
                name_idx.insert(meta.name.clone(), meta.seq_no);
            }
        }

        result.sort_by_key(|m| m.seq_no);
        Ok(result)
    }

    pub async fn get_checkpoint(&self, name: &str) -> Result<Option<CheckpointMeta>> {
        validate_identifier("Checkpoint name", name)?;
        self.get_checkpoint_internal(name).await
    }

    /// Restores the system to a specific checkpoint by name.
    /// This will rollback the underlying storage to the transaction ID of the checkpoint.
    pub async fn restore_checkpoint(&self, name: &str) -> Result<CheckpointMeta> {
        validate_identifier("Checkpoint name", name)?;
        let _guard = self.write_lock.lock().await;

        let meta = self
            .get_checkpoint_internal(name)
            .await?
            .ok_or(MemFuseError::CheckpointNotFound)?;

        // 1. Rollback storage state
        self.storage.rollback_to_tx(meta.tx_id).await?;

        // 2. Synchronize cache
        self.list_checkpoints().await?;

        Ok(meta)
    }
}

#[async_trait]
impl<S: memfuse_core::StorageEngine> CheckpointRegistry for PersistentCheckpointStore<S> {
    async fn save_checkpoint(&self, meta: CheckpointMeta) -> Result<()> {
        let _guard = self.write_lock.lock().await;
        self.save_checkpoint_internal(meta).await
    }

    async fn load_checkpoint(&self, seq_no: u64) -> Result<Option<CheckpointMeta>> {
        // Erst In-Memory prüfen
        if let Some(meta) = self.checkpoints.read().get(&seq_no) {
            return Ok(Some(meta.clone()));
        }

        // Dann Storage via Scan (da Key auf Name basiert)
        let all = self.list_checkpoints().await?;
        Ok(all.into_iter().find(|c| c.seq_no == seq_no))
    }

    async fn list_checkpoints(&self) -> Result<Vec<CheckpointMeta>> {
        self.list_checkpoints().await
    }
}

#[async_trait]
impl<S: memfuse_core::StorageEngine> memfuse_core::traits::CheckpointCoordinator
    for PersistentCheckpointStore<S>
{
    type Meta = CheckpointMeta;

    async fn create_named_checkpoint(
        &self,
        name: &str,
        collection_id: &str,
        seq_no: u64,
        tx_id: TxId,
        metadata: serde_json::Value,
    ) -> Result<Self::Meta> {
        self.create_checkpoint(name, collection_id, seq_no, tx_id, metadata)
            .await
    }

    async fn restore_named_checkpoint(&self, name: &str) -> Result<Self::Meta> {
        self.restore_checkpoint(name).await
    }

    async fn drop_named_checkpoint(&self, name: &str) -> Result<()> {
        self.drop_checkpoint(name).await
    }

    async fn list_named_checkpoints(&self) -> Result<Vec<Self::Meta>> {
        self.list_checkpoints().await
    }
}

#[async_trait]
impl<S: memfuse_core::StorageEngine> memfuse_core::traits::Checkpoint
    for PersistentCheckpointStore<S>
{
    async fn take_snapshot(&self, tx: TxId) -> Result<WorkflowState> {
        let seq_no = self.storage.last_seq_no().await?;
        Ok(WorkflowState {
            tx,
            graph_hash: format!("seq-{}", seq_no),
        })
    }

    async fn restore(&self, state: &WorkflowState) -> Result<()> {
        self.storage.rollback_to_tx(state.tx).await?;
        self.list_checkpoints().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use memfuse_core::{Result, StorageEngine, StorageStats};
    use parking_lot::Mutex;
    use std::collections::HashSet;

    struct MockStorage {
        data: Mutex<HashMap<Vec<u8>, Vec<u8>>>,
        pinned: Mutex<HashSet<u64>>,
        fail_on_put: Mutex<Option<Vec<u8>>>,
        rolled_back_tx: Mutex<Vec<TxId>>,
    }

    impl MockStorage {
        fn new() -> Self {
            Self {
                data: Mutex::new(HashMap::new()),
                pinned: Mutex::new(HashSet::new()),
                fail_on_put: Mutex::new(None),
                rolled_back_tx: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl StorageEngine for MockStorage {
        async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
            Ok(self.data.lock().get(key).cloned())
        }
        async fn get_at_seq(&self, key: &[u8], _seq: u64) -> Result<Option<Vec<u8>>> {
            self.get(key).await
        }
        async fn put(&self, _tx_id: TxId, key: &[u8], value: &[u8]) -> Result<()> {
            if let Some(fail_key) = self.fail_on_put.lock().as_ref() {
                if key == fail_key {
                    return Err(MemFuseError::Internal("Mock Storage Error".to_string()));
                }
            }
            self.data.lock().insert(key.to_vec(), value.to_vec());
            Ok(())
        }
        async fn delete(&self, _tx_id: TxId, key: &[u8]) -> Result<()> {
            self.data.lock().remove(key);
            Ok(())
        }
        async fn commit(&self, _tx_id: TxId) -> Result<()> {
            Ok(())
        }
        async fn rollback(&self, _tx_id: TxId) -> Result<()> {
            Ok(())
        }
        async fn rollback_to_tx(&self, tx_id: TxId) -> Result<()> {
            self.rolled_back_tx.lock().push(tx_id);
            Ok(())
        }
        async fn last_seq_no(&self) -> Result<u64> {
            Ok(0)
        }
        async fn last_tx_id(&self) -> Result<TxId> {
            Ok(TxId::new(0))
        }
        async fn flush(&self) -> Result<()> {
            Ok(())
        }
        async fn stats(&self) -> Result<StorageStats> {
            Ok(StorageStats {
                num_segments: 0,
                total_size_bytes: 0,
                memtable_size_bytes: 0,
            })
        }
        async fn pin_checkpoint(&self, seq_no: u64) -> Result<()> {
            self.pinned.lock().insert(seq_no);
            Ok(())
        }
        async fn unpin_checkpoint(&self, seq_no: u64) -> Result<()> {
            self.pinned.lock().remove(&seq_no);
            Ok(())
        }
        async fn scan_prefix(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
            let data = self.data.lock();
            Ok(data
                .iter()
                .filter(|(k, _)| k.starts_with(prefix))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect())
        }
        async fn scan(
            &self,
            _s: std::ops::Bound<&[u8]>,
            _e: std::ops::Bound<&[u8]>,
        ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
            Ok(Vec::new())
        }
    }

    #[tokio::test]
    async fn test_create_and_load() {
        let storage = Arc::new(MockStorage::new());
        let store = PersistentCheckpointStore::new(storage, "test");
        let meta = store
            .create_checkpoint("cp1", "c1", 1, TxId::new(1), serde_json::json!({}))
            .await
            .unwrap(); // unwrap
        let loaded = store.load_checkpoint(1).await.unwrap().unwrap(); // unwrap
        assert_eq!(loaded, meta);
    }

    #[tokio::test]
    async fn test_name_uniqueness() {
        let storage = Arc::new(MockStorage::new());
        let store = PersistentCheckpointStore::new(storage.clone(), "test");
        store
            .create_checkpoint("same", "c1", 1, TxId::new(1), serde_json::json!({}))
            .await
            .unwrap(); // unwrap
        store
            .create_checkpoint("same", "c1", 2, TxId::new(2), serde_json::json!({}))
            .await
            .unwrap(); // unwrap
        let all = store.list_checkpoints().await.unwrap(); // unwrap
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].seq_no, 2);
        assert!(!storage.pinned.lock().contains(&1));
        assert!(storage.pinned.lock().contains(&2));
    }

    #[tokio::test]
    async fn test_checkpoint_creation_rollback_on_failure() {
        let storage = Arc::new(MockStorage::new());
        let cp_key = b"test:checkpoint:fail_cp";
        *storage.fail_on_put.lock() = Some(cp_key.to_vec());

        let store = PersistentCheckpointStore::new(storage.clone(), "test");
        let seq_no = 123;

        let res = store
            .create_checkpoint("fail_cp", "c1", seq_no, TxId::new(1), serde_json::json!({}))
            .await;

        assert!(res.is_err());
        assert!(!storage.pinned.lock().contains(&seq_no));
    }

    #[tokio::test]
    async fn test_pin_before_unpin_invariant_on_failure() {
        let storage = Arc::new(MockStorage::new());
        let store = PersistentCheckpointStore::new(storage.clone(), "test");

        // 1. Create first checkpoint successfully
        store
            .create_checkpoint("my_cp", "c1", 1, TxId::new(1), serde_json::json!({}))
            .await
            .unwrap(); // unwrap

        assert!(storage.pinned.lock().contains(&1));

        // 2. Make next save fail
        let cp_key = b"test:checkpoint:my_cp";
        *storage.fail_on_put.lock() = Some(cp_key.to_vec());

        // 3. Try to overwrite with a new checkpoint, which will fail
        let res = store
            .create_checkpoint("my_cp", "c1", 2, TxId::new(2), serde_json::json!({}))
            .await;

        assert!(res.is_err());

        // 4. Verify invariant: old checkpoint (1) must still be pinned!
        assert!(
            storage.pinned.lock().contains(&1),
            "Old checkpoint should still be pinned because save failed"
        );

        // 5. Verify invariant: new checkpoint (2) should be unpinned (rolled back)!
        assert!(
            !storage.pinned.lock().contains(&2),
            "New checkpoint should be unpinned after failure"
        );
    }

    #[tokio::test]
    async fn checkpoint_guard_rollback_on_drop() {
        let storage = Arc::new(MockStorage::new());
        let store = PersistentCheckpointStore::new(storage.clone(), "test");

        {
            let _guard = store.create_guard(TxId::new(42)).unwrap(); // unwrap
                                                                     // guard drops here without commit
        }

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let rolled_back = storage.rolled_back_tx.lock().clone();
        assert_eq!(rolled_back, vec![TxId::new(42)]);
    }

    #[tokio::test]
    async fn checkpoint_guard_commit_prevents_rollback() {
        let storage = Arc::new(MockStorage::new());
        let store = PersistentCheckpointStore::new(storage.clone(), "test");

        let guard = store.create_guard(TxId::new(100)).unwrap(); // unwrap
        let cp = guard.commit().unwrap(); // unwrap
        assert_eq!(cp.tx_id, TxId::new(100));

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(
            storage.rolled_back_tx.lock().is_empty(),
            "Committed guard should not perform rollback"
        );
    }

    #[test]
    fn timestamp_ms_is_monotonic() {
        let t1 = monotonic_timestamp_ms();
        let t2 = monotonic_timestamp_ms();
        assert!(t2 >= t1, "Timestamp must be monotonic");
    }

    #[tokio::test]
    async fn list_checkpoints_empty_initially() {
        use memfuse_core::traits::CheckpointCoordinator;
        let storage = Arc::new(MockStorage::new());
        let store = PersistentCheckpointStore::new(storage, "test");

        let list = store.list_named_checkpoints().await.unwrap(); // unwrap
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn checkpoint_not_found_returns_err() {
        use memfuse_core::traits::CheckpointCoordinator;
        let storage = Arc::new(MockStorage::new());
        let store = PersistentCheckpointStore::new(storage, "test");

        let res = store.restore_named_checkpoint("nonexistent").await;
        assert!(matches!(res, Err(MemFuseError::CheckpointNotFound)));
    }

    #[tokio::test]
    async fn test_list_named_checkpoints_after_reopen() {
        use memfuse_core::traits::CheckpointCoordinator;
        let storage = Arc::new(MockStorage::new());
        {
            let store1 = PersistentCheckpointStore::new(storage.clone(), "test");
            store1
                .create_named_checkpoint(
                    "cp1",
                    "col1",
                    1,
                    TxId::new(TxId::INTERNAL_BASE + 1),
                    serde_json::json!({}),
                )
                .await
                .unwrap(); // unwrap
            store1
                .create_named_checkpoint(
                    "cp2",
                    "col1",
                    2,
                    TxId::new(TxId::INTERNAL_BASE + 2),
                    serde_json::json!({}),
                )
                .await
                .unwrap(); // unwrap
        }

        let store2 = PersistentCheckpointStore::new(storage.clone(), "test");
        let list = store2.list_named_checkpoints().await.unwrap(); // unwrap

        assert_eq!(list.len(), 2);
        let names: Vec<_> = list.into_iter().map(|m| m.name).collect();
        assert_eq!(names, vec!["cp1", "cp2"]);
    }

    #[tokio::test]
    async fn list_checkpoints_cache_matches_storage() {
        use memfuse_core::traits::CheckpointCoordinator;
        let storage = Arc::new(MockStorage::new());
        let store1 = Arc::new(PersistentCheckpointStore::new(storage.clone(), "test"));

        // Create 3 checkpoints
        for i in 1..=3 {
            store1
                .create_named_checkpoint(
                    &format!("cp-{i}"),
                    "col1",
                    i,
                    TxId::new(TxId::INTERNAL_BASE + i),
                    serde_json::json!({}),
                )
                .await
                .unwrap(); // unwrap
        }

        // Drop and reload the store from same storage
        let store2 = PersistentCheckpointStore::new(storage.clone(), "test");
        let list = store2.list_named_checkpoints().await.unwrap(); // unwrap

        assert_eq!(list.len(), 3);
        let names: Vec<_> = list.into_iter().map(|m| m.name).collect();
        assert_eq!(names, vec!["cp-1", "cp-2", "cp-3"]);
    }

    #[tokio::test]
    async fn concurrent_checkpoint_creation_is_safe() {
        use memfuse_core::traits::CheckpointCoordinator;
        use tokio::task::JoinSet;

        let storage = Arc::new(MockStorage::new());
        let store = Arc::new(PersistentCheckpointStore::new(storage, "test"));

        let mut tasks = JoinSet::new();
        for i in 0..8u64 {
            let store = Arc::clone(&store);
            tasks.spawn(async move {
                store
                    .create_named_checkpoint(
                        &format!("cp-{i}"),
                        "col1",
                        i,
                        TxId::new(TxId::INTERNAL_BASE + i),
                        serde_json::json!({}),
                    )
                    .await
            });
        }
        // All must succeed or fail without panicking
        while let Some(res) = tasks.join_next().await {
            let res = res.unwrap(); // unwrap
            if let Err(e) = res {
                println!("Checkpoint creation failed (acceptable): {e}");
            }
        }

        let all = store.list_named_checkpoints().await.unwrap(); // unwrap
        assert_eq!(all.len(), 8);
    }

    #[test]
    fn test_checkpoint_guard_dropped_outside_tokio_runtime() {
        let storage = Arc::new(MockStorage::new());
        let store = PersistentCheckpointStore::new(storage, "test");

        // Should not panic when dropped outside a tokio runtime context
        let _guard = store.create_guard(TxId::new(999)).unwrap(); // unwrap
    }

    #[tokio::test]
    async fn test_checkpoint_guard_for_agent_step() {
        let storage = Arc::new(MockStorage::new());
        let guard = CheckpointGuard::for_agent_step(storage.clone(), TxId::new(55))
            .await
            .unwrap(); // unwrap

        let cp = guard.checkpoint().unwrap(); // unwrap
        assert_eq!(cp.tx_id, TxId::new(55));
        assert!(cp.timestamp_ms > 0);

        let committed = guard.commit().unwrap(); // unwrap
        assert_eq!(committed.tx_id, TxId::new(55));
    }

    #[tokio::test]
    async fn test_drop_checkpoint_uses_unique_tx_and_unpins() {
        let storage = Arc::new(MockStorage::new());
        let store = PersistentCheckpointStore::new(storage.clone(), "test");

        store
            .create_checkpoint("drop_me", "col1", 42, TxId::new(1), serde_json::json!({}))
            .await
            .unwrap(); // unwrap

        assert!(storage.pinned.lock().contains(&42));
        assert!(store.get_checkpoint("drop_me").await.unwrap().is_some()); // unwrap

        store.drop_checkpoint("drop_me").await.unwrap(); // unwrap

        assert!(
            !storage.pinned.lock().contains(&42),
            "Checkpoint seq_no 42 should be unpinned after drop"
        );
        assert!(store.get_checkpoint("drop_me").await.unwrap().is_none()); // unwrap
    }

    #[test]
    fn test_next_tx_overflow_returns_err() {
        let storage = Arc::new(MockStorage::new());
        let store = PersistentCheckpointStore::new(storage, "test");

        store.tx_counter.store(1_000_000, Ordering::SeqCst);

        let res = store.allocate_tx();
        assert!(res.is_err());
        if let Err(MemFuseError::Internal(msg)) = res {
            assert!(msg.contains("overflow"));
        } else {
            panic!("Expected Internal error on overflow");
        }
    }

    #[tokio::test]
    async fn test_input_validation_empty_and_oversized_names() {
        let storage = Arc::new(MockStorage::new());
        let store = PersistentCheckpointStore::new(storage, "test");

        // Empty name
        let res = store
            .create_checkpoint("", "col1", 1, TxId::new(1), serde_json::json!({}))
            .await;
        assert!(matches!(res, Err(MemFuseError::InvalidInput(_))));

        // Whitespace name
        let res = store
            .create_checkpoint("   ", "col1", 1, TxId::new(1), serde_json::json!({}))
            .await;
        assert!(matches!(res, Err(MemFuseError::InvalidInput(_))));

        // Empty collection ID
        let res = store
            .create_checkpoint("cp1", "", 1, TxId::new(1), serde_json::json!({}))
            .await;
        assert!(matches!(res, Err(MemFuseError::InvalidInput(_))));

        // Oversized name (> 256 chars)
        let long_name = "a".repeat(257);
        let res = store
            .create_checkpoint(&long_name, "col1", 1, TxId::new(1), serde_json::json!({}))
            .await;
        assert!(matches!(res, Err(MemFuseError::InvalidInput(_))));

        // Drop with empty name
        let res = store.drop_checkpoint("").await;
        assert!(matches!(res, Err(MemFuseError::InvalidInput(_))));

        // Get with empty name
        let res = store.get_checkpoint("   ").await;
        assert!(matches!(res, Err(MemFuseError::InvalidInput(_))));
    }

    #[test]
    fn test_manifest_validation_blank_component() {
        let meta = CheckpointMeta {
            name: "cp1".to_string(),
            collection_id: "col1".to_string(),
            seq_no: 1,
            tx_id: TxId::new(1),
            metadata: serde_json::json!({}),
            created_at: 100,
        };

        let res = CheckpointManifest::new(meta, vec!["   ".to_string()]);
        assert!(matches!(res, Err(MemFuseError::InvalidInput(_))));
    }

    #[allow(non_snake_case)]
    #[tokio::test]
    async fn create_checkpoint_CASE_unicode_and_multibyte_name() {
        let storage = Arc::new(MockStorage::new());
        let store = PersistentCheckpointStore::new(storage.clone(), "test");

        let unicode_name = "Prüfpunkt_1_🚀_日本語";
        let collection_id = "Sammlung_äöü_123";

        store
            .create_checkpoint(
                unicode_name,
                collection_id,
                10,
                TxId::new(101),
                serde_json::json!({"tag": "überprüfen"}),
            )
            .await
            .expect("// expect #[cfg(test)]");

        let fetched = store
            .get_checkpoint(unicode_name)
            .await
            .expect("// expect #[cfg(test)]")
            .expect("// expect #[cfg(test)]");

        assert_eq!(fetched.name, "Prüfpunkt_1_🚀_日本語");
        assert_eq!(fetched.collection_id, "Sammlung_äöü_123");
        assert_eq!(fetched.seq_no, 10);
        assert_eq!(fetched.tx_id, TxId::new(101));
        assert_eq!(fetched.metadata["tag"], "überprüfen");
    }

    #[allow(non_snake_case)]
    #[tokio::test]
    async fn create_checkpoint_CASE_exact_max_len_256() {
        let storage = Arc::new(MockStorage::new());
        let store = PersistentCheckpointStore::new(storage.clone(), "test");

        let max_name = "a".repeat(256);
        let res = store
            .create_checkpoint(&max_name, "col1", 1, TxId::new(1), serde_json::json!({}))
            .await;
        assert!(res.is_ok(), "256 characters name must be allowed");

        let fetched = store
            .get_checkpoint(&max_name)
            .await
            .expect("// expect #[cfg(test)]");
        assert!(fetched.is_some());
    }

    #[allow(non_snake_case)]
    #[tokio::test]
    async fn drop_checkpoint_CASE_nonexistent_returns_ok() {
        let storage = Arc::new(MockStorage::new());
        let store = PersistentCheckpointStore::new(storage.clone(), "test");

        let res = store.drop_checkpoint("nonexistent_checkpoint").await;
        assert!(
            res.is_ok(),
            "Dropping a non-existent checkpoint should be idempotent and return Ok(())"
        );
    }

    #[allow(non_snake_case)]
    #[tokio::test]
    async fn checkpoint_guard_CASE_uncommitted_guard_holds_state() {
        let storage = Arc::new(MockStorage::new());
        let store = PersistentCheckpointStore::new(storage, "test");

        let guard = store
            .create_guard(TxId::new(500))
            .expect("// expect #[cfg(test)]");
        let cp = guard.checkpoint().expect("// expect #[cfg(test)]").clone();
        assert_eq!(cp.tx_id, TxId::new(500));

        // Commit takes ownership of self and consumes the state checkpoint
        let committed_cp = guard.commit().expect("// expect #[cfg(test)]");
        assert_eq!(committed_cp.tx_id, TxId::new(500));
    }

    #[allow(non_snake_case)]
    #[tokio::test]
    async fn checkpoint_guard_CASE_commit_moves_ownership() {
        let storage = Arc::new(MockStorage::new());
        let guard = CheckpointGuard::new(
            StateCheckpoint {
                tx_id: TxId::new(777),
                timestamp_ms: 12345,
            },
            storage,
        );

        assert!(guard.checkpoint().is_ok());

        let cp = guard.commit().expect("// expect #[cfg(test)]");
        assert_eq!(cp.tx_id, TxId::new(777));
        assert_eq!(cp.timestamp_ms, 12345);
    }

    #[allow(non_snake_case)]
    #[tokio::test]
    async fn restore_checkpoint_CASE_not_found_returns_err() {
        let storage = Arc::new(MockStorage::new());
        let store = PersistentCheckpointStore::new(storage, "test");

        let res = store.restore_checkpoint("missing_cp").await;
        assert!(matches!(res, Err(MemFuseError::CheckpointNotFound)));
    }

    #[allow(non_snake_case)]
    #[tokio::test]
    async fn list_checkpoints_CASE_corrupted_storage_data_propagates_err() {
        let storage = Arc::new(MockStorage::new());
        let store = PersistentCheckpointStore::new(storage.clone(), "test");

        // Put invalid JSON payload into storage under checkpoint namespace format (namespace:checkpoint:name)
        let corrupt_key = b"test:checkpoint:corrupt_cp";
        storage
            .put(TxId::new(1), corrupt_key, b"invalid json bytes{{{")
            .await
            .expect("// expect #[cfg(test)]");

        let res = store.list_checkpoints().await;
        assert!(matches!(res, Err(MemFuseError::Serialization(_))));
    }

    #[allow(non_snake_case)]
    #[test]
    fn checkpoint_meta_CASE_serialization_roundtrip() {
        let meta = CheckpointMeta {
            name: "cp_test".to_string(),
            collection_id: "col_test".to_string(),
            seq_no: 99,
            tx_id: TxId::new(1001),
            metadata: serde_json::json!({"step": 42, "env": "prod"}),
            created_at: 1690000000,
        };

        let json = serde_json::to_string(&meta).expect("// expect #[cfg(test)]");
        let deserialized: CheckpointMeta =
            serde_json::from_str(&json).expect("// expect #[cfg(test)]");

        // Independent evaluation without referencing meta in comparison construction
        assert_eq!(deserialized.name, "cp_test");
        assert_eq!(deserialized.collection_id, "col_test");
        assert_eq!(deserialized.seq_no, 99);
        assert_eq!(deserialized.tx_id, TxId::new(1001));
        assert_eq!(deserialized.metadata["step"], 42);
        assert_eq!(deserialized.created_at, 1690000000);
    }

    #[allow(non_snake_case)]
    #[test]
    fn state_checkpoint_CASE_serialization_roundtrip() {
        let cp = StateCheckpoint {
            tx_id: TxId::new(888),
            timestamp_ms: 1700000000123,
        };

        let json = serde_json::to_string(&cp).expect("// expect #[cfg(test)]");
        let deserialized: StateCheckpoint =
            serde_json::from_str(&json).expect("// expect #[cfg(test)]");

        assert_eq!(deserialized.tx_id, TxId::new(888));
        assert_eq!(deserialized.timestamp_ms, 1700000000123);
    }

    #[allow(non_snake_case)]
    #[test]
    fn into_workflow_state_CASE_valid_conversion() {
        let meta = CheckpointMeta {
            name: "wf_cp".to_string(),
            collection_id: "wf_col".to_string(),
            seq_no: 15,
            tx_id: TxId::new(2026),
            metadata: serde_json::json!({"agent_phase": "reasoning"}),
            created_at: 5000,
        };

        let state = meta.into_workflow_state();

        // Independent expected value assertions
        assert_eq!(state.tx, TxId::new(2026));
        assert!(!state.graph_hash.is_empty());
    }

    #[allow(non_snake_case)]
    #[test]
    fn allocate_tx_CASE_parity_with_deprecated_next_tx() {
        let storage = Arc::new(MockStorage::new());
        let store = PersistentCheckpointStore::new(storage, "test");

        let tx1 = store.allocate_tx().expect("// expect #[cfg(test)]");
        #[allow(deprecated)]
        let tx2 = store.next_tx().expect("// expect #[cfg(test)]");
        let tx3 = store.allocate_tx().expect("// expect #[cfg(test)]");

        assert_eq!(tx1, TxId::new(TxId::INTERNAL_BASE));
        assert_eq!(tx2, TxId::new(TxId::INTERNAL_BASE + 1));
        assert_eq!(tx3, TxId::new(TxId::INTERNAL_BASE + 2));
    }

    #[test]
    fn test_input_validation_null_bytes_and_control_chars() {
        assert!(validate_identifier("test", "name\0with_null").is_err());
        assert!(validate_identifier("test", "name\rwith_cr").is_err());
        assert!(validate_identifier("test", "name\nwith_lf").is_err());
        assert!(validate_identifier("test", "valid_name").is_ok());
    }

    #[test]
    fn test_manifest_max_components_exceeded() {
        let meta = CheckpointMeta {
            name: "cp_limit".to_string(),
            collection_id: "col_limit".to_string(),
            seq_no: 1,
            tx_id: TxId::new(1),
            metadata: serde_json::Value::Null,
            created_at: 100,
        };

        let over_limit = vec!["comp".to_string(); MAX_COMPONENTS + 1];
        assert!(CheckpointManifest::new(meta, over_limit).is_err());
    }

    #[tokio::test]
    async fn test_list_checkpoints_max_limit_exceeded() {
        let storage = Arc::new(MockStorage::new());
        let store = PersistentCheckpointStore::new(storage.clone(), "test");

        // Insert MAX_CHECKPOINTS + 1 entries directly into mock storage
        for i in 0..=(MAX_CHECKPOINTS as u64) {
            let key = format!("test:checkpoint:cp_{i}");
            let meta = CheckpointMeta {
                name: format!("cp_{i}"),
                collection_id: "c1".to_string(),
                seq_no: i,
                tx_id: TxId::new(i),
                metadata: serde_json::Value::Null,
                created_at: 100,
            };
            let manifest = CheckpointManifest::new(meta, vec!["comp".to_string()]).unwrap();
            let val = serde_json::to_vec(&manifest).unwrap();
            storage.data.lock().insert(key.into_bytes(), val);
        }

        let res = store.list_checkpoints().await;
        assert!(res.is_err(), "list_checkpoints must fail if entries exceed MAX_CHECKPOINTS");
    }
}
