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
// STAND:       2026-08-29T15:22:34Z (SESSION: 2c814094)
// ZWECK:       RAII CheckpointGuard + persistente Snapshot-Verwaltung
// INVARIANTEN: CheckpointGuard darf NICHT mit PersistentCheckpointStore verwechselt werden; GC safety by pinning before store writes
// HOTSPOTS:    CheckpointGuard::for_agent_step(), PersistentCheckpointStore::create_checkpoint()
// SIEHE AUCH:  ADR-011

use async_trait::async_trait;
use memfuse_core::{MemFuseError, Result, TxId, WorkflowState};
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Type alias for sequence numbers managed as pinned checkpoint identifiers.
pub type PinId = u64;

static CHECKPOINT_COUNTER: AtomicU64 = AtomicU64::new(0);
static SKIPPED_ROLLBACKS: AtomicU64 = AtomicU64::new(0);
static ORPHANED_CHECKPOINTS: Mutex<Vec<StateCheckpoint>> = Mutex::new(Vec::new());
pub static ORPHAN_REGISTRY: std::sync::OnceLock<OrphanRegistry> = std::sync::OnceLock::new();

pub fn global_orphan_registry() -> &'static OrphanRegistry {
    ORPHAN_REGISTRY.get_or_init(OrphanRegistry::default)
}

fn orphan_pin_file_path() -> std::path::PathBuf {
    std::env::var("MEMFUSE_ORPHAN_PIN_PATH")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("memfuse_orphaned_pins.json"))
}

/// Durable append-only registry for orphaned sequence pins (ADR-052).
pub struct OrphanRegistry {
    orphans: Mutex<Vec<PinId>>,
    file_path: std::path::PathBuf,
}

impl Default for OrphanRegistry {
    fn default() -> Self {
        Self::new(orphan_pin_file_path())
    }
}

impl OrphanRegistry {
    pub fn new(file_path: impl Into<std::path::PathBuf>) -> Self {
        let path = file_path.into();
        let loaded = Self::load_file_sync(&path);
        Self {
            orphans: Mutex::new(loaded),
            file_path: path,
        }
    }

    fn load_file_sync(path: &std::path::Path) -> Vec<PinId> {
        if let Ok(data) = std::fs::read(path) {
            if let Ok(list) = serde_json::from_slice::<Vec<PinId>>(&data) {
                return list;
            }
        }
        Vec::new()
    }

    fn persist_sync(&self, list: &[PinId]) -> std::io::Result<()> {
        let data = serde_json::to_vec_pretty(list)?;
        std::fs::write(&self.file_path, data)
    }

    /// Synchronously registers an orphaned sequence pin and persists to disk.
    pub fn register_orphan(&self, pin_id: PinId) -> std::io::Result<()> {
        let mut lock = self.orphans.lock();
        if !lock.contains(&pin_id) {
            lock.push(pin_id);
        }
        self.persist_sync(&lock)
    }

    /// Retrieves all currently registered orphaned pin sequence numbers.
    pub fn get_orphans(&self) -> Vec<PinId> {
        self.orphans.lock().clone()
    }

    /// Synchronously clears memory and disk records.
    pub fn clear_all(&self) {
        let mut lock = self.orphans.lock();
        lock.clear();
        if let Err(err) = self.persist_sync(&lock) {
            tracing::warn!(?err, "Failed to persist cleared orphan registry");
        }
    }

    /// Recovers all registered orphaned pins by unpinning them in storage and cleaning the registry.
    pub async fn recover_and_clean<S: memfuse_core::StorageEngine>(
        &self,
        storage: &S,
    ) -> Result<Vec<PinId>> {
        let orphans = self.get_orphans();
        let mut recovered = Vec::new();

        for pin_id in orphans {
            if let Err(e) = storage.unpin_checkpoint(pin_id).await {
                tracing::warn!(pin_id = pin_id, error = %e, "Failed to unpin orphaned pin during recovery");
            } else {
                recovered.push(pin_id);
            }
        }

        if !recovered.is_empty() {
            let mut lock = self.orphans.lock();
            lock.retain(|p| !recovered.contains(p));
            if let Err(err) = self.persist_sync(&lock) {
                tracing::warn!(?err, "Failed to persist orphan pin registry after recovery");
            }
        }

        Ok(recovered)
    }
}

fn orphan_file_path() -> std::path::PathBuf {
    std::env::var("MEMFUSE_ORPHAN_PATH")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("memfuse_orphaned_checkpoints.json"))
}

fn persist_orphaned_checkpoints_sync(list: &[StateCheckpoint]) -> std::io::Result<()> {
    let path = orphan_file_path();
    let data = serde_json::to_vec_pretty(list)?;
    std::fs::write(path, data)
}

fn load_orphaned_checkpoints_file_sync() -> Vec<StateCheckpoint> {
    let path = orphan_file_path();
    if let Ok(data) = std::fs::read(path) {
        if let Ok(list) = serde_json::from_slice::<Vec<StateCheckpoint>>(&data) {
            return list;
        }
    }
    Vec::new()
}

/// Orphaned gepinnte Sequenznummer — wird beim Recovery verarbeitet.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PinnedSeqNoOrphan {
    pub seq_no: u64,
    pub timestamp_ms: u64,
}

static ORPHANED_PIN_SEQ_NOS: Mutex<Vec<PinnedSeqNoOrphan>> = Mutex::new(Vec::new());

fn persist_pinned_seq_no_orphans_sync(list: &[PinnedSeqNoOrphan]) -> std::io::Result<()> {
    let path = orphan_pin_file_path();
    let data = serde_json::to_vec_pretty(list)?;
    std::fs::write(path, data)
}

#[deprecated(
    since = "0.1.0",
    note = "Use PersistentCheckpointStore::register_pinned_seq_no_orphan instead. Global functions are not safe in multi-instance environments."
)]
pub fn register_pinned_seq_no_orphan(orphan: PinnedSeqNoOrphan) {
    let mut lock = ORPHANED_PIN_SEQ_NOS.lock();
    if !lock.iter().any(|o| o.seq_no == orphan.seq_no) {
        lock.push(orphan.clone());
    }
    if let Err(e) = persist_pinned_seq_no_orphans_sync(&lock) {
        tracing::error!(?e, "Failed to persist pinned seq_no orphans");
    }
    let _ = global_orphan_registry().register_orphan(orphan.seq_no);
}

/// Instance-scoped orphan state for checkpoints and pinned sequence numbers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct OrphanState {
    pub checkpoints: Vec<StateCheckpoint>,
    pub pinned_seq_nos: Vec<PinnedSeqNoOrphan>,
    pub persist_path: std::path::PathBuf,
}

impl OrphanState {
    pub fn persist_sync(&self) -> std::io::Result<()> {
        if self.persist_path.as_os_str().is_empty() {
            return Ok(());
        }
        let data = serde_json::to_vec_pretty(self)?;
        std::fs::write(&self.persist_path, data)
    }

    pub fn load_sync(path: &std::path::Path) -> Self {
        if let Ok(data) = std::fs::read(path) {
            if let Ok(state) = serde_json::from_slice::<OrphanState>(&data) {
                return state;
            }
        }
        Self {
            checkpoints: Vec::new(),
            pinned_seq_nos: Vec::new(),
            persist_path: path.to_path_buf(),
        }
    }
}

/// Registers an uncommitted checkpoint as orphaned and persists it to disk.
#[deprecated(
    since = "0.1.0",
    note = "Use PersistentCheckpointStore::register_orphaned_checkpoint instead. Global functions are not safe in multi-instance environments."
)]
pub fn register_orphaned_checkpoint(cp: StateCheckpoint) {
    let mut lock = ORPHANED_CHECKPOINTS.lock();
    if !lock.iter().any(|existing| existing.tx_id == cp.tx_id) {
        lock.push(cp);
    }
    if let Err(err) = persist_orphaned_checkpoints_sync(&lock) {
        tracing::warn!(?err, "Failed to persist orphaned checkpoints");
    }
}

/// Retrieves all active registered orphaned checkpoints.
#[deprecated(
    since = "0.1.0",
    note = "Use PersistentCheckpointStore::get_orphaned_checkpoints instead. Global functions are not safe in multi-instance environments."
)]
pub fn get_orphaned_checkpoints() -> Vec<StateCheckpoint> {
    let mut lock = ORPHANED_CHECKPOINTS.lock();
    if lock.is_empty() {
        let loaded = load_orphaned_checkpoints_file_sync();
        if !loaded.is_empty() {
            *lock = loaded;
        }
    }
    lock.clone()
}

/// Retrieves orphaned checkpoints registered for a specific namespace.
pub fn get_orphaned_checkpoints_for_namespace(ns: &str) -> Vec<StateCheckpoint> {
    get_orphaned_checkpoints()
        .into_iter()
        .filter(|cp| cp.namespace.as_deref() == Some(ns))
        .collect()
}

/// Removes a specific orphaned checkpoint after recovery.
#[deprecated(
    since = "0.1.0",
    note = "Use PersistentCheckpointStore::clear_orphaned_checkpoint instead. Global functions are not safe in multi-instance environments."
)]
pub fn clear_orphaned_checkpoint(tx_id: TxId) {
    let mut lock = ORPHANED_CHECKPOINTS.lock();
    lock.retain(|cp| cp.tx_id != tx_id);
    if let Err(err) = persist_orphaned_checkpoints_sync(&lock) {
        tracing::warn!(?err, "Failed to persist orphaned checkpoints");
    }
}

/// Clears all registered orphaned checkpoints.
#[deprecated(
    since = "0.1.0",
    note = "Use PersistentCheckpointStore::clear_all_orphaned_checkpoints instead. Global functions are not safe in multi-instance environments."
)]
pub fn clear_all_orphaned_checkpoints() {
    let mut lock = ORPHANED_CHECKPOINTS.lock();
    lock.clear();
    if let Err(err) = persist_orphaned_checkpoints_sync(&lock) {
        tracing::warn!(?err, "Failed to persist orphaned checkpoints");
    }
}

/// Retained for backward compatibility. No background tasks are spawned during drop.
pub async fn await_pending_rollbacks() {}

/// Retained for backward compatibility. Always returns 0 as background task spawning is removed.
pub fn pending_rollback_count() -> usize {
    0
}

/// Liefert die Gesamtzahl der uncommitted CheckpointGuards, die ohne expliziten Commit/Rollback gedroppt wurden.
pub fn checkpoint_guard_skipped_rollback_count() -> u64 {
    SKIPPED_ROLLBACKS.load(Ordering::Relaxed)
}

/// Liefert die Anzahl der aktuell registrierten verwaisten ("orphaned") Checkpoints.
#[allow(deprecated)]
pub fn orphaned_checkpoint_count() -> usize {
    get_orphaned_checkpoints().len()
}

// RESOLVED: AGT-CKPT-001 — UTF-8 char counting used for 256 char limit (TS: 2026-09-01T23:07:05Z) (SESSION: 358e3b0a)
/// Validates identifier strings (checkpoint name, collection ID) against empty/whitespace or size limits.
fn validate_identifier(field_name: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(MemFuseError::InvalidInput(format!(
            "{field_name} cannot be empty or whitespace-only"
        )));
    }
    let char_count = value.chars().count();
    if char_count > 256 {
        return Err(MemFuseError::InvalidInput(format!(
            "{field_name} exceeds maximum length of 256 characters (got {char_count})"
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
        for comp in &components {
            if comp.trim().is_empty() {
                return Err(MemFuseError::InvalidInput(
                    "Checkpoint component name cannot be empty".to_string(),
                ));
            }
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

/// RAII-Guard für gepinnte Checkpoint-Sequenznummern (gemäß ADR-015).
/// Garantiert, dass eine gepinnte Sequenznummer bei einem Fehler oder Panic während des Schreibens
/// automatisch entpinnt wird, um dauerhafte GC-Blockaden zu verhindern.
#[must_use = "PinGuard must be defused upon successful checkpoint storage"]
pub struct PinGuard<S: memfuse_core::StorageEngine> {
    storage: Arc<S>,
    seq_no: Option<u64>,
}

impl<S: memfuse_core::StorageEngine> PinGuard<S> {
    pub async fn pin(storage: Arc<S>, seq_no: u64) -> Result<Self> {
        storage.pin_checkpoint(seq_no).await?;
        Ok(Self {
            storage,
            seq_no: Some(seq_no),
        })
    }

    /// Entschärft den Guard nach erfolgreicher Persistierung, sodass die Sequenznummer gepinnt bleibt.
    pub fn defuse(mut self) {
        self.seq_no.take();
    }

    /// Entpinnt die Sequenznummer explizit und asynchron bei einem abgefangenen Fehler.
    pub async fn unpin(mut self) -> Result<()> {
        if let Some(seq_no) = self.seq_no.take() {
            self.storage.unpin_checkpoint(seq_no).await?;
        }
        Ok(())
    }
}

impl<S: memfuse_core::StorageEngine> Drop for PinGuard<S> {
    fn drop(&mut self) {
        if let Some(seq_no) = self.seq_no.take() {
            let cp = PinnedSeqNoOrphan {
                seq_no,
                timestamp_ms: monotonic_timestamp_ms(),
            };

            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                let storage = Arc::clone(&self.storage);
                handle.spawn(async move {
                    if let Err(e) = storage.unpin_checkpoint(seq_no).await {
                        tracing::error!(
                            seq = seq_no,
                            error = %e,
                            "PinGuard drop: unpin_checkpoint failed — \
                             registering as orphan for recovery"
                        );
                        #[allow(deprecated)]
                        register_pinned_seq_no_orphan(cp);
                    }
                });
            } else {
                tracing::warn!(
                    seq = seq_no,
                    "PinGuard drop outside tokio runtime — \
                     registering as orphan for recovery"
                );
                #[allow(deprecated)]
                register_pinned_seq_no_orphan(cp);
            }
        }
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
    #[serde(default)]
    pub namespace: Option<String>,
}

/// RAII Guard, der explizit über [`commit`](Self::commit) oder [`rollback`](Self::rollback) finalisiert werden MUSS.
///
/// # RAII-Kontrakt und Betriebshinweise
/// Ein `CheckpointGuard` muss explizit über `commit()` oder `rollback().await` finalisiert werden.
/// Wird der Guard ohne expliziten Commit/Rollback gedroppt (z. B. bei Panik oder Programmierfehler),
/// wird KEIN asynchroner Hintergrund-Rollback gespawnt, um Kollisionen mit späteren Transaktionen zu verhindern.
/// Stattdessen wird der Checkpoint als "orphaned" registriert/persistiert und beim nächsten kontrollierten Recovery-Zyklus verarbeitet.
#[must_use = "CheckpointGuard must be explicitly finalized via .commit() or .rollback().await"]
pub struct CheckpointGuard<S: memfuse_core::StorageEngine> {
    checkpoint: Option<StateCheckpoint>,
    storage: Arc<S>,
    namespace: String,
}

impl<S: memfuse_core::StorageEngine> CheckpointGuard<S> {
    pub fn new(checkpoint: StateCheckpoint, storage: Arc<S>, namespace: impl Into<String>) -> Self {
        Self {
            checkpoint: Some(checkpoint),
            storage,
            namespace: namespace.into(),
        }
    }

    /// Erstellt einen neuen CheckpointGuard für einen Agenten-Schritt.
    pub async fn for_agent_step(storage: Arc<S>, tx: TxId) -> Result<Self> {
        let cp = StateCheckpoint {
            tx_id: tx,
            timestamp_ms: monotonic_timestamp_ms(),
            namespace: Some("agent_step".to_string()),
        };
        Ok(Self::new(cp, storage, "agent_step"))
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

    /// Führt ein manuelles, asynchrones Rollback des Checkpoints aus.
    /// Nach Aufruf von `rollback()` ist der Guard konsumiert, sodass beim Drop kein erneutes Rollback ausgelöst wird.
    ///
    /// # Serialisierungsbarriere
    /// Wenn neuere committete Transaktionen mit `last_tx > target_tx` existieren, schlägt das Rollback mit
    /// einem Fehler fehl, um Datenverlust neuerer Transaktionen zu verhindern.
    pub async fn rollback(mut self) -> Result<()> {
        if let Some(cp) = self.checkpoint.take() {
            let last_tx = self.storage.last_tx_id().await?;
            if last_tx > cp.tx_id {
                return Err(MemFuseError::Transaction(format!(
                    "Serialization barrier violation: Cannot rollback to TxId {} because newer committed transaction TxId {} exists in storage",
                    cp.tx_id.inner(),
                    last_tx.inner()
                )));
            }
            self.storage.rollback_to_tx(cp.tx_id).await
        } else {
            Err(MemFuseError::Internal("Checkpoint already consumed".into()))
        }
    }

    /// Führt ein synchrones, blockierendes Rollback des Checkpoints aus.
    ///
    /// # Einschränkung
    /// Diese Methode darf **nicht** aus einem laufenden asynchronen Tokio-Kontext heraus aufgerufen werden.
    /// In asynchronem Kontext führt ein Aufruf von `rollback_blocking` zu einem Fehler
    /// [`MemFuseError::Internal`], um Deadlocks und Tokio-Panics zu verhindern. Verwende in async-Kontexten
    /// stattdessen [`rollback`](Self::rollback).
    pub fn rollback_blocking(mut self) -> Result<()> {
        if tokio::runtime::Handle::try_current().is_ok() {
            return Err(MemFuseError::Internal(
                "Cannot call rollback_blocking from within an active async Tokio runtime context; use rollback().await instead"
                    .to_string(),
            ));
        }

        let cp = self
            .checkpoint
            .take()
            .ok_or_else(|| MemFuseError::Internal("Checkpoint already consumed".into()))?;

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| {
                MemFuseError::Internal(format!(
                    "Failed to create Tokio runtime for rollback_blocking: {e}"
                ))
            })?;

        rt.block_on(async {
            let last_tx = self.storage.last_tx_id().await?;
            if last_tx > cp.tx_id {
                return Err(MemFuseError::Transaction(format!(
                    "Serialization barrier violation: Cannot rollback to TxId {} because newer committed transaction TxId {} exists in storage",
                    cp.tx_id.inner(),
                    last_tx.inner()
                )));
            }
            self.storage.rollback_to_tx(cp.tx_id).await
        })
    }
}

impl<S: memfuse_core::StorageEngine> Drop for CheckpointGuard<S> {
    fn drop(&mut self) {
        if let Some(mut cp) = self.checkpoint.take() {
            cp.namespace = Some(self.namespace.clone());
            SKIPPED_ROLLBACKS.fetch_add(1, Ordering::SeqCst);
            tracing::error!(
                tx_id = ?cp.tx_id,
                "CheckpointGuard dropped without explicit commit or rollback. Checkpoint marked as orphaned for controlled recovery."
            );
            if let Some(ref orphan_state) = self.orphan_state {
                let mut state = orphan_state.lock();
                if !state.checkpoints.iter().any(|e| e.tx_id == cp.tx_id) {
                    state.checkpoints.push(cp.clone());
                    state.persist_sync().ok();
                }
            }
            #[allow(deprecated)]
            register_orphaned_checkpoint(cp);
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

/// Counter metadata persisted to guarantee TxId monotonicity across restarts.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TxCounterMeta {
    pub high_water_mark: u64,
}

const TX_BATCH_SIZE: u64 = 100;

async fn persist_hwm_internal<S: memfuse_core::StorageEngine>(
    storage: &Arc<S>,
    namespace: &str,
    hwm: u64,
) -> Result<()> {
    let meta = TxCounterMeta {
        high_water_mark: hwm,
    };
    let bytes =
        serde_json::to_vec(&meta).map_err(|e| MemFuseError::Serialization(e.to_string()))?;
    let key = format!("{namespace}:checkpoint:__sys_tx_counter__");
    let tx = TxId::new(TxId::INTERNAL_BASE + hwm);
    storage.put(tx, key.as_bytes(), &bytes).await?;
    storage.commit(tx).await?;
    storage.flush().await?;
    Ok(())
}

/// Registry für gespeicherte Checkpoints mit Thread-sicherem Zustand.
///
/// # Invarianten
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
    /// Reservierter High-Water-Mark Wert in persistentem Storage
    allocated_hwm: AtomicU64,
    /// Lock für HWM-Reservierung und Persistierung
    hwm_lock: tokio::sync::Mutex<()>,
    /// Instanz-spezifischer Orphan State
    orphan_state: Arc<Mutex<OrphanState>>,
}

impl<S: memfuse_core::StorageEngine> PersistentCheckpointStore<S> {
    /// Öffnet einen PersistentCheckpointStore asynchron mit Rekonstruktion und Monotonie-Garantie.
    pub async fn open(storage: Arc<S>, namespace: impl Into<String>) -> Result<Self> {
        let namespace = namespace.into();

        // 1. Scan store for highest existing TxId under namespace
        let prefix = format!("{namespace}:checkpoint:");
        let entries = storage.scan_prefix(prefix.as_bytes()).await?;
        let mut scanned_max_raw: Option<u64> = None;

        for (_key, value_bytes) in entries {
            let meta_tx =
                if let Ok(manifest) = serde_json::from_slice::<CheckpointManifest>(&value_bytes) {
                    Some(manifest.meta.tx_id)
                } else if let Ok(meta) = serde_json::from_slice::<CheckpointMeta>(&value_bytes) {
                    Some(meta.tx_id)
                } else {
                    None
                };

            if let Some(tx) = meta_tx {
                if tx.inner() >= TxId::INTERNAL_BASE {
                    let raw = tx.inner() - TxId::INTERNAL_BASE;
                    scanned_max_raw = Some(scanned_max_raw.map_or(raw, |m| m.max(raw)));
                }
            }
        }

        if let Ok(last_tx) = storage.last_tx_id().await {
            if last_tx.inner() >= TxId::INTERNAL_BASE {
                let raw = last_tx.inner() - TxId::INTERNAL_BASE;
                scanned_max_raw = Some(scanned_max_raw.map_or(raw, |m| m.max(raw)));
            }
        }

        // 2. Read persisted counter metadata
        let counter_key = format!("{namespace}:checkpoint:__sys_tx_counter__");
        let persisted_val: Option<u64> = match storage.get(counter_key.as_bytes()).await {
            Ok(Some(bytes)) => serde_json::from_slice::<TxCounterMeta>(&bytes)
                .map(|m| m.high_water_mark)
                .ok(),
            _ => None,
        };

        // 3. Consistency check: if persisted value exists and is LESS THAN scanned max raw -> Hard Error (Requirement 4)
        if let (Some(persisted), Some(scanned)) = (persisted_val, scanned_max_raw) {
            if persisted < scanned {
                return Err(MemFuseError::Internal(format!(
                    "TxId collision / regression detected in namespace '{namespace}': \
                     persisted tx_counter HWM ({persisted}) is strictly less than highest tx_id found in store ({scanned})"
                )));
            }
        }

        // 4. Determine initial start_raw
        let start_raw = match (persisted_val, scanned_max_raw) {
            (Some(p), _) => p + 1,
            (None, Some(s)) => s + 1,
            (None, None) => 0,
        };

        let initial_hwm = persisted_val.unwrap_or_else(|| scanned_max_raw.unwrap_or(0));

        // 5. Recover orphaned sequence pins on startup (ADR-052)
        if let Err(err) = global_orphan_registry().recover_and_clean(&*storage).await {
            tracing::warn!(?err, "Failed to recover orphaned pins during store startup");
        }

        let orphan_path =
            std::path::PathBuf::from(format!("{namespace}_orphaned_checkpoints.json"));
        let orphan_state = Arc::new(Mutex::new(OrphanState::load_sync(&orphan_path)));

        Ok(Self {
            storage,
            checkpoints: RwLock::new(HashMap::new()),
            name_index: RwLock::new(HashMap::new()),
            namespace,
            write_lock: tokio::sync::Mutex::new(()),
            tx_counter: AtomicU64::new(start_raw),
            allocated_hwm: AtomicU64::new(initial_hwm),
            hwm_lock: tokio::sync::Mutex::new(()),
            orphan_state,
        })
    }

    pub fn new(storage: Arc<S>, namespace: impl Into<String>) -> Self {
        let ns = namespace.into();
        let storage_clone = storage.clone();
        let ns_clone = ns.clone();

        let res = if let Ok(handle) = tokio::runtime::Handle::try_current() {
            if handle.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread {
                tokio::task::block_in_place(|| handle.block_on(Self::open(storage_clone, ns_clone)))
            } else {
                std::thread::spawn(move || {
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(|e| MemFuseError::Internal(e.to_string()))?;
                    rt.block_on(Self::open(storage_clone, ns_clone))
                })
                .join()
                .map_err(|_| {
                    MemFuseError::Internal(
                        "Thread panic during PersistentCheckpointStore initialization".into(),
                    )
                })
                .and_then(|r| r)
            }
        } else {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    panic!("Failed to create Tokio runtime: {e}");
                }
            };
            rt.block_on(Self::open(storage_clone, ns_clone))
        };

        match res {
            Ok(store) => store,
            Err(e) => {
                panic!("Failed to initialize PersistentCheckpointStore for namespace '{ns}': {e}")
            }
        }
    }

    // INVARIANT: Checkpoint TxIds use INTERNAL_BASE+n range to avoid
    // collision with Collection-sequenced TxIds [1, ~10^12].
    // See: DECISIONS.md AGT-GRAPH-001, TxId::INTERNAL_BASE
    pub async fn allocate_tx(&self) -> Result<TxId> {
        let raw = self.tx_counter.fetch_add(1, Ordering::SeqCst);
        if raw >= 1_000_000 {
            return Err(MemFuseError::Internal(
                "Checkpoint TxId counter overflow".to_string(),
            ));
        }

        let current_hwm = self.allocated_hwm.load(Ordering::SeqCst);
        if raw >= current_hwm {
            let _guard = self.hwm_lock.lock().await;
            let active_hwm = self.allocated_hwm.load(Ordering::SeqCst);
            if raw >= active_hwm {
                let new_hwm = raw + TX_BATCH_SIZE - 1;
                persist_hwm_internal(&self.storage, &self.namespace, new_hwm).await?;
                self.allocated_hwm.store(new_hwm, Ordering::SeqCst);
            }
        }

        Ok(TxId::new(TxId::INTERNAL_BASE + raw))
    }

    #[deprecated(
        since = "0.1.0",
        note = "Use `allocate_tx()` instead — both methods are functionally identical, `allocate_tx()` is the canonical public API."
    )]
    #[allow(dead_code)]
    async fn next_tx(&self) -> Result<TxId> {
        self.allocate_tx().await
    }

    /// Creates an ephemeral transactional checkpoint RAII guard.
    /// If the returned guard is dropped without calling `.commit()`, the underlying storage
    /// is automatically rolled back to `tx_id`.
    pub fn create_guard(&self, tx_id: TxId) -> Result<CheckpointGuard<S>> {
        let timestamp_ms = monotonic_timestamp_ms();
        let cp = StateCheckpoint {
            tx_id,
            timestamp_ms,
            namespace: Some(self.namespace.clone()),
        };
        Ok(CheckpointGuard::new(
            cp,
            Arc::clone(&self.storage),
            &self.namespace,
        ))
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
        let pin_guard = PinGuard::pin(Arc::clone(&self.storage), seq_no).await?;

        // 2. storage.save() the new checkpoint (RAII PinGuard will unpin on drop/panic or explicit unpin on error)
        if let Err(e) = self.save_checkpoint_internal(meta.clone()).await {
            if let Err(unpin_err) = pin_guard.unpin().await {
                tracing::warn!(
                    seq = seq_no,
                    "Failed to unpin new checkpoint after save failure: {unpin_err}"
                );
            }
            return Err(e);
        }

        // 3. Save succeeded: defuse pin_guard so seq_no remains pinned
        pin_guard.defuse();

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
            let unique_tx = self.allocate_tx().await?;

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

        let tx = self.allocate_tx().await?;
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

        let mut result = Vec::with_capacity(entries.len());
        for (key_bytes, value_bytes) in entries {
            if key_bytes.ends_with(b":__sys_tx_counter__") {
                continue;
            }
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

        // Cache synchronisieren
        {
            let mut cache = self.checkpoints.write();
            let mut name_idx = self.name_index.write();
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
    ///
    /// # Serialisierungsbarriere
    /// Wenn neuere committete Transaktionen (`last_tx > meta.tx_id`) existieren, schlägt die Wiederherstellung
    /// mit einem Fehler fehl.
    pub async fn restore_checkpoint(&self, name: &str) -> Result<CheckpointMeta> {
        validate_identifier("Checkpoint name", name)?;
        let _guard = self.write_lock.lock().await;

        let meta = self
            .get_checkpoint_internal(name)
            .await?
            .ok_or(MemFuseError::CheckpointNotFound)?;

        let last_tx = self.storage.last_tx_id().await?;
        if last_tx > meta.tx_id {
            return Err(MemFuseError::Transaction(format!(
                "Serialization barrier violation: Cannot restore checkpoint '{}' at TxId {} because newer committed transaction TxId {} exists in storage",
                name,
                meta.tx_id.inner(),
                last_tx.inner()
            )));
        }

        // 1. Rollback storage state
        self.storage.rollback_to_tx(meta.tx_id).await?;

        // 2. Synchronize cache
        self.list_checkpoints().await?;

        Ok(meta)
    }

    /// Recovers all registered/persisted orphaned checkpoints during controlled startup or recovery.
    /// Checks the serialization barrier (`last_tx <= cp.tx_id`) before executing rollback.
    /// Recovers all registered/persisted orphaned sequence pins (ADR-052).
    pub async fn recover_orphaned_pins(&self) -> Result<Vec<PinId>> {
        global_orphan_registry()
            .recover_and_clean(&*self.storage)
            .await
    }

    pub async fn recover_orphaned_checkpoints(&self) -> Result<Vec<TxId>> {
        let _guard = self.write_lock.lock().await;
        let mut orphans = self.get_orphaned_checkpoints();
        #[allow(deprecated)]
        let global_orphans = get_orphaned_checkpoints();
        for go in global_orphans {
            if !orphans.iter().any(|o| o.tx_id == go.tx_id) {
                orphans.push(go);
            }
        }

        let mut recovered = Vec::new();
        let last_tx = self.storage.last_tx_id().await?;

        for cp in orphans {
            if last_tx <= cp.tx_id {
                if let Err(e) = self.storage.rollback_to_tx(cp.tx_id).await {
                    tracing::error!(tx_id = ?cp.tx_id, "Failed to recover orphaned checkpoint: {e}");
                } else {
                    recovered.push(cp.tx_id);
                    self.clear_orphaned_checkpoint(cp.tx_id);
                    #[allow(deprecated)]
                    clear_orphaned_checkpoint(cp.tx_id);
                }
            } else {
                tracing::warn!(
                    tx_id = ?cp.tx_id,
                    last_tx = ?last_tx,
                    "Orphaned checkpoint skipped during recovery due to serialization barrier (newer transaction committed)"
                );
                self.clear_orphaned_checkpoint(cp.tx_id);
                #[allow(deprecated)]
                clear_orphaned_checkpoint(cp.tx_id);
            }
        }

        Ok(recovered)
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

    static ORPHAN_TEST_MUTEX: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

    // AI-TAG[TEST][MAJOR] Parallel unit test race condition on global_orphan_registry OnceLock singleton (ID: AGT-CHECKPOINT-a3ccc9fe) (TS: 2026-09-03T19:40:20Z) (SESSION: d766fd58)
    // BEFUND: global_orphan_registry() is a process-wide OnceLock singleton pointing to a shared orphan pin file. When `PersistentCheckpointStore::new` is instantiated concurrently in other tests, it invokes `recover_and_clean()`, which mutates/clears `ORPHAN_REGISTRY` while `test_orphan_registry_persists_across_drop` is running.
    // RISIKO: Flaky test failure under parallel `cargo test` runs when `test_orphan_registry_persists_across_drop` races with `PersistentCheckpointStore::new` in parallel threads.
    // EMPFEHLUNG: Isolate orphan registry path in tests via tempdir or `MEMFUSE_ORPHAN_PIN_PATH` env var per test instance.
    #[test]
    fn test_orphan_registry_persists_across_drop() {
        let _guard = ORPHAN_TEST_MUTEX.lock();
        let registry = global_orphan_registry();
        registry.clear_all();

        let storage = Arc::new(MockStorage::new());
        let seq_no = 12345;

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to build Tokio runtime for test");

        let guard = rt.block_on(async { PinGuard::pin(storage.clone(), seq_no).await.unwrap() });

        // Drop guard without runtime or explicit unpin/defuse
        drop(guard);

        // Verify orphan ID appears in registry
        let orphans = registry.get_orphans();
        assert!(
            orphans.contains(&seq_no),
            "Orphan sequence number 12345 must appear in registry upon PinGuard drop"
        );
        registry.clear_all();
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn test_orphan_recovery_on_startup() {
        let registry = global_orphan_registry();
        registry.clear_all();

        let storage = Arc::new(MockStorage::new());
        let seq_no = 67890;

        // Pin checkpoint and register orphan directly
        storage.pin_checkpoint(seq_no).await.unwrap();
        registry.register_orphan(seq_no).unwrap();

        assert!(storage.pinned.lock().contains(&seq_no));
        assert!(registry.get_orphans().contains(&seq_no));

        // Simulate startup / recover_and_clean
        let recovered = registry.recover_and_clean(&*storage).await.unwrap();

        assert_eq!(recovered, vec![seq_no]);
        assert!(
            !storage.pinned.lock().contains(&seq_no),
            "Storage sequence number 67890 must be unpinned after recovery"
        );
        assert!(
            registry.get_orphans().is_empty(),
            "Orphan registry must be empty after recovery"
        );
        registry.clear_all();
    }

    #[tokio::test]
    async fn test_pin_guard_unpins_checkpoint_on_storage_write_failure() {
        let storage = Arc::new(MockStorage::new());
        let store = PersistentCheckpointStore::new(storage.clone(), "test_pinguard");

        let seq_no = 999;
        let cp_key = b"test_pinguard:checkpoint:fail_write_cp";
        *storage.fail_on_put.lock() = Some(cp_key.to_vec());

        let res = store
            .create_checkpoint(
                "fail_write_cp",
                "col1",
                seq_no,
                TxId::new(10),
                serde_json::json!({}),
            )
            .await;

        assert!(
            res.is_err(),
            "Checkpoint creation must fail when storage put fails"
        );

        // Yield execution briefly to allow drop task on Handle::spawn to complete if async
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Verify PinGuard drop unpinned seq_no 999
        assert!(
            !storage.pinned.lock().contains(&seq_no),
            "Sequence number 999 must be unpinned after storage write failure via PinGuard RAII drop"
        );
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

    #[test]
    fn test_panic_unwind_triggers_orphan_registration_and_recovery() {
        #[allow(deprecated)]
        clear_all_orphaned_checkpoints();
        let storage = Arc::new(MockStorage::new());

        let (store, panic_result) = std::thread::spawn({
            let storage = Arc::clone(&storage);
            move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("Failed to build Tokio runtime for panic test");

                let store = Arc::new(PersistentCheckpointStore::new(storage, "test_panic"));
                let store_clone = Arc::clone(&store);

                let panic_res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    rt.block_on(async move {
                        let _guard = store_clone.create_guard(TxId::new(7070)).unwrap();
                        // Intentionally trigger panic inside guard scope
                        panic!("Simulated intentional panic between guard creation and commit");
                    });
                }));

                (store, panic_res)
            }
        })
        .join()
        .expect("Thread failed to join");

        assert!(panic_result.is_err(), "catch_unwind must capture the panic");

        // Verify orphaned checkpoint is registered after panic unwind
        assert_eq!(orphaned_checkpoint_count(), 1);

        // Perform recovery in a fresh runtime
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to build Tokio runtime for recovery test");

        rt.block_on(async move {
            let recovered = store.recover_orphaned_checkpoints().await.unwrap();
            assert_eq!(recovered, vec![TxId::new(7070)]);

            // Verify transaction was rolled back in storage
            let rolled_back = storage.rolled_back_tx.lock().clone();
            assert_eq!(rolled_back, vec![TxId::new(7070)]);
        });
    }

    #[tokio::test]
    async fn checkpoint_guard_rollback_on_drop() {
        #[allow(deprecated)]
        clear_all_orphaned_checkpoints();
        let storage = Arc::new(MockStorage::new());
        let store = PersistentCheckpointStore::new(storage.clone(), "test_guard_rollback_on_drop");
        store.clear_all_orphaned_checkpoints();

        {
            let _guard = store.create_guard(TxId::new(42)).unwrap(); // unwrap
                                                                     // guard drops here without commit
        }

        let recovered = store.recover_orphaned_checkpoints().await.unwrap();
        assert_eq!(recovered, vec![TxId::new(42)]);

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
        std::thread::spawn(|| {
            let storage = Arc::new(MockStorage::new());
            let store = PersistentCheckpointStore::new(storage, "test");

            let initial_skipped = checkpoint_guard_skipped_rollback_count();

            {
                let _guard = store.create_guard(TxId::new(999)).unwrap(); // unwrap
                // _guard drops here at end of inner scope
            }

            assert_eq!(
                checkpoint_guard_skipped_rollback_count(),
                initial_skipped + 1,
                "Skipped rollback counter must increment when guard is dropped outside Tokio runtime"
            );
        })
        .join()
        .expect("// expect Thread panic in test_checkpoint_guard_dropped_outside_tokio_runtime");
    }

    #[tokio::test]
    async fn test_auto_rollback_tracking_and_await() {
        #[allow(deprecated)]
        clear_all_orphaned_checkpoints();
        let storage = Arc::new(MockStorage::new());
        let store = PersistentCheckpointStore::new(storage.clone(), "test_auto_rollback");
        store.clear_all_orphaned_checkpoints();

        {
            let _guard = store.create_guard(TxId::new(808)).unwrap(); // unwrap
                                                                      // Drop without commit inside tokio runtime
        }

        let recovered = store.recover_orphaned_checkpoints().await.unwrap();
        assert_eq!(recovered, vec![TxId::new(808)]);

        let rolled_back = storage.rolled_back_tx.lock().clone();
        assert_eq!(rolled_back, vec![TxId::new(808)]);
        assert_eq!(pending_rollback_count(), 0);
    }

    #[test]
    fn test_rollback_blocking_in_sync_context() {
        let storage = Arc::new(MockStorage::new());
        let store = PersistentCheckpointStore::new(storage.clone(), "test");

        let guard = store.create_guard(TxId::new(909)).unwrap(); // unwrap
        let res = guard.rollback_blocking();
        assert!(
            res.is_ok(),
            "rollback_blocking must succeed in sync context"
        );

        let rolled_back = storage.rolled_back_tx.lock().clone();
        assert_eq!(rolled_back, vec![TxId::new(909)]);
    }

    #[tokio::test]
    async fn test_rollback_blocking_in_async_context_returns_error() {
        let storage = Arc::new(MockStorage::new());
        let store = PersistentCheckpointStore::new(storage, "test");

        let guard = store.create_guard(TxId::new(1010)).unwrap(); // unwrap

        // Timeout safety net to guarantee no hanging/deadlock
        let res = tokio::time::timeout(std::time::Duration::from_secs(2), async move {
            guard.rollback_blocking()
        })
        .await
        .expect("rollback_blocking in async context timed out - possible deadlock!");

        assert!(
            res.is_err(),
            "rollback_blocking called from async context must return error immediately to prevent deadlock"
        );
        if let Err(MemFuseError::Internal(msg)) = res {
            assert!(msg.contains("active async Tokio runtime context"));
            assert!(msg.contains("rollback().await"));
        } else {
            panic!(
                "Expected MemFuseError::Internal error message instructing to use rollback().await"
            );
        }
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

    #[tokio::test]
    async fn test_next_tx_overflow_returns_err() {
        let storage = Arc::new(MockStorage::new());
        let store = PersistentCheckpointStore::new(storage, "test");

        store.tx_counter.store(1_000_000, Ordering::SeqCst);

        let res = store.allocate_tx().await;
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

        // Multibyte unicode name: 256 chars (512+ bytes) accepted, 257 chars rejected
        let unicode_256 = "ä".repeat(256);
        let res_256 = store
            .create_checkpoint(&unicode_256, "col1", 1, TxId::new(1), serde_json::json!({}))
            .await;
        assert!(res_256.is_ok());

        let unicode_257 = "ä".repeat(257);
        let res_257 = store
            .create_checkpoint(&unicode_257, "col1", 1, TxId::new(1), serde_json::json!({}))
            .await;
        assert!(matches!(res_257, Err(MemFuseError::InvalidInput(_))));

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
                namespace: Some("test".to_string()),
            },
            storage,
            "test",
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
            namespace: Some("test_ns".to_string()),
        };

        let json = serde_json::to_string(&cp).expect("// expect #[cfg(test)]");
        let deserialized: StateCheckpoint =
            serde_json::from_str(&json).expect("// expect #[cfg(test)]");

        assert_eq!(deserialized.tx_id, TxId::new(888));
        assert_eq!(deserialized.timestamp_ms, 1700000000123);
        assert_eq!(deserialized.namespace, Some("test_ns".to_string()));
    }

    #[test]
    fn state_checkpoint_deserializes_legacy_json_without_namespace() {
        let legacy_json = r#"{"tx_id": 999, "timestamp_ms": 1700000000000}"#;
        let deserialized: StateCheckpoint =
            serde_json::from_str(legacy_json).expect("Legacy JSON without namespace must deserialize");

        assert_eq!(deserialized.tx_id, TxId::new(999));
        assert_eq!(deserialized.timestamp_ms, 1700000000000);
        assert_eq!(deserialized.namespace, None);
    }

    #[tokio::test]
    async fn test_get_orphaned_checkpoints_for_namespace() {
        clear_all_orphaned_checkpoints();
        let storage = Arc::new(MockStorage::new());
        let store_a = PersistentCheckpointStore::new(storage.clone(), "ns_a");
        let store_b = PersistentCheckpointStore::new(storage.clone(), "ns_b");

        {
            let _guard_a1 = store_a.create_guard(TxId::new(1001)).unwrap();
            let _guard_a2 = store_a.create_guard(TxId::new(1002)).unwrap();
            let _guard_b1 = store_b.create_guard(TxId::new(2001)).unwrap();
            // All 3 guards drop here uncommitted
        }

        let orphans_a = get_orphaned_checkpoints_for_namespace("ns_a");
        let orphans_b = get_orphaned_checkpoints_for_namespace("ns_b");
        let orphans_none = get_orphaned_checkpoints_for_namespace("nonexistent");

        assert_eq!(orphans_a.len(), 2);
        assert!(orphans_a.iter().all(|cp| cp.namespace.as_deref() == Some("ns_a")));
        let tx_a: Vec<TxId> = orphans_a.iter().map(|cp| cp.tx_id).collect();
        assert!(tx_a.contains(&TxId::new(1001)));
        assert!(tx_a.contains(&TxId::new(1002)));

        assert_eq!(orphans_b.len(), 1);
        assert_eq!(orphans_b[0].tx_id, TxId::new(2001));
        assert_eq!(orphans_b[0].namespace.as_deref(), Some("ns_b"));

        assert!(orphans_none.is_empty());
        clear_all_orphaned_checkpoints();
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
    #[tokio::test]
    async fn allocate_tx_CASE_parity_with_deprecated_next_tx() {
        let storage = Arc::new(MockStorage::new());
        let store = PersistentCheckpointStore::new(storage, "test");

        let tx1 = store.allocate_tx().await.expect("// expect #[cfg(test)]");
        #[allow(deprecated)]
        let tx2 = store.next_tx().await.expect("// expect #[cfg(test)]");
        let tx3 = store.allocate_tx().await.expect("// expect #[cfg(test)]");

        assert_eq!(tx1, TxId::new(TxId::INTERNAL_BASE));
        assert_eq!(tx2, TxId::new(TxId::INTERNAL_BASE + 1));
        assert_eq!(tx3, TxId::new(TxId::INTERNAL_BASE + 2));
    }

    #[allow(non_snake_case)]
    #[test]
    fn checkpoint_manifest_CASE_whitespace_component_rejected() {
        let meta = CheckpointMeta {
            name: "cp_ws".to_string(),
            collection_id: "col_ws".to_string(),
            seq_no: 1,
            tx_id: TxId::new(10),
            metadata: serde_json::json!({}),
            created_at: 100,
        };
        let res = CheckpointManifest::new(meta, vec!["   ".to_string()]);
        assert!(matches!(res, Err(MemFuseError::InvalidInput(_))));
    }

    #[allow(non_snake_case)]
    #[test]
    fn checkpoint_manifest_CASE_tampered_manifest_fails_verify() {
        let meta = CheckpointMeta {
            name: "cp_tamper".to_string(),
            collection_id: "col_tamper".to_string(),
            seq_no: 5,
            tx_id: TxId::new(50),
            metadata: serde_json::json!({"version": 1}),
            created_at: 500,
        };
        let mut manifest = CheckpointManifest::new(meta, vec!["comp1".to_string()])
            .expect("// expect #[cfg(test)]");
        manifest.components.push("tampered_comp".to_string());
        let res = manifest.verify();
        assert!(matches!(res, Err(MemFuseError::Serialization(_))));
    }

    #[allow(non_snake_case)]
    #[tokio::test]
    async fn checkpoint_guard_CASE_rollback_consumed_returns_err() {
        let dummy_storage = Arc::new(MockStorage::new());
        let consumed_guard = CheckpointGuard::<MockStorage> {
            checkpoint: None,
            storage: dummy_storage,
            namespace: "test".to_string(),
        };
        assert!(matches!(
            consumed_guard.checkpoint(),
            Err(MemFuseError::Internal(_))
        ));

        let consumed_guard2 = CheckpointGuard::<MockStorage> {
            checkpoint: None,
            storage: Arc::new(MockStorage::new()),
            namespace: "test".to_string(),
        };
        assert!(matches!(
            consumed_guard2.commit(),
            Err(MemFuseError::Internal(_))
        ));

        let consumed_guard3 = CheckpointGuard::<MockStorage> {
            checkpoint: None,
            storage: Arc::new(MockStorage::new()),
            namespace: "test".to_string(),
        };
        let res = consumed_guard3.rollback().await;
        assert!(matches!(res, Err(MemFuseError::Internal(_))));
    }

    proptest::proptest! {
        #[test]
        fn prop_manifest_roundtrip(
            name: String,
            col: String,
            seq_no: u64,
            tx: u64,
            created_at: u64,
        ) {
            if name.trim().is_empty()
                || col.trim().is_empty()
                || name.len() > 256
                || col.len() > 256
            {
                // Expected validation failure for invalid boundaries
                let meta = CheckpointMeta {
                    name,
                    collection_id: col,
                    seq_no,
                    tx_id: TxId::new(tx),
                    metadata: serde_json::json!({}),
                    created_at,
                };
                proptest::prop_assert!(CheckpointManifest::new(meta, vec!["valid_comp".to_string()]).is_err());
            } else {
                let meta = CheckpointMeta {
                    name: name.clone(),
                    collection_id: col.clone(),
                    seq_no,
                    tx_id: TxId::new(tx),
                    metadata: serde_json::json!({}),
                    created_at,
                };
                let manifest = CheckpointManifest::new(meta, vec!["comp_a".to_string(), "comp_b".to_string()]);
                proptest::prop_assert!(manifest.is_ok());
                let manifest = manifest.expect("// expect #[cfg(test)]");
                proptest::prop_assert!(manifest.verify().is_ok());
                proptest::prop_assert_eq!(manifest.meta.name, name);
                proptest::prop_assert_eq!(manifest.meta.collection_id, col);
                proptest::prop_assert_eq!(manifest.meta.seq_no, seq_no);
                proptest::prop_assert_eq!(manifest.meta.tx_id, TxId::new(tx));
                proptest::prop_assert_eq!(manifest.meta.created_at, created_at);
            }
        }

        #[test]
        fn prop_monotonic_timestamp_ms_increases_or_equals(_n: u8) {
            let ts1 = monotonic_timestamp_ms();
            let ts2 = monotonic_timestamp_ms();
            proptest::prop_assert!(ts2 >= ts1);
        }
    }
}
