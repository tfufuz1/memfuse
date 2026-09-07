// FILE-CONTEXT
// ZWECK: MemFuse Database Orchestrator & Facade (Layer 2).
// INVARIANTEN: Monoton steigende TxId-Allokation; Reparaturgarantie beim Öffnen (repair_on_open); Strikte Isolation von Namespaces.
// NICHT-OFFENSICHTLICH: Lock-Hierarchie: collections (RwLock) -> insert_lock (Mutex) -> embedder (RwLock).
// STAND: TS:2026-08-29T17:22:29Z (SESSION: 0dcb9f3b)

// INVARIANT: Orchestrator Facade (Getriebe — Layer 2).
//! # MemFuse — Embedded Hybrid-Search for AI Agents
//!
//! ## Concurrency & Lock Hierarchy
//! When acquiring multiple locks in `MemFuse`, the following order must be respected to avoid deadlocks:
//! 1. `MemFuse::collections` (`tokio::sync::RwLock`)
//! 2. `MemFuse::embedder` (`parking_lot::RwLock`)
//! 3. `Collection::insert_lock` (`tokio::sync::Mutex`) / `Collection::embedder` (`parking_lot::RwLock`)
//!
//!
//! MemFuse is a zero-boilerplate embedded database for AI agent memory.
//! It combines vector search (HNSW), persistent storage (LSM-Tree),
//! and relationship tracking in a single library.
//!
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use memfuse_db::MemFuse;
//!
//! # async fn example() -> memfuse_core::Result<()> {
//! // Open or create a database
//! let db = MemFuse::open("./my_data").await?;
//!
//! // Insert a document with embedding and metadata
//! let embedding = vec![0.1, 0.2, 0.3, 0.4];
//! db.insert(
//!     "doc-1",
//!     &embedding,
//!     Some(serde_json::json!({"topic": "rust"})),
//! ).await?;
//!
//! // Semantic search (via Collection::query() builder facade)
//! let col = db.collection("default").await?;
//! let results = col
//!     .query()
//!     .embedding(&[0.1, 0.2, 0.3, 0.4])
//!     .k(5)
//!     .execute()
//!     .await?;
//! for result in &results {
//!     println!("{}: score={:.3}", result.id, result.score);
//! }
//!
//! // Get by key
//! let doc = db.get("doc-1").await?;
//!
//! // Delete
//! db.delete("doc-1").await?;
//! # Ok(())
//! # }
//! ```

#![forbid(unsafe_code)]

// FILE-CONTEXT
// STAND:       2026-08-29T15:22:34Z (SESSION: 2c814094)
// ZWECK:       Orchestrator-Facade (Layer 2) — öffentliche API der Collection
// INVARIANTEN: Unified transaction semantics across HNSW, LSM, Graph and Text indexes; thread-safe concurrent collection access
// HOTSPOTS:    hybrid_search(), insert(), relate()
// SIEHE AUCH:  crates/memfuse-db/AGENTS.md

pub use memfuse_core::TextEmbeddingEngine;
use memfuse_core::{DocId, Result, StorageEngine, TxId};
use memfuse_index::{HnswConfig, HnswIndex};
use memfuse_store::LsmStorage;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub mod chunker;
pub mod collection;
pub mod context;
pub mod context_compaction;

pub use context_compaction::{
    cleanup_orphaned_consolidation_intents, CompactedContext, CompactionStrategy,
    ConsolidationSession, ContextCompactor, StatusToken,
};

#[cfg(feature = "sandbox")]
pub trait SandboxBridge: Send + Sync {
    fn db_search<'a>(&'a self, query: &'a [u8], k: usize) -> BoxFuture<'a, Result<Vec<u8>>>;
    fn db_insert<'a>(&'a self, key: &'a [u8], value: &'a [u8]) -> BoxFuture<'a, Result<()>>;
    fn db_get<'a>(&'a self, key: &'a [u8]) -> BoxFuture<'a, Result<Option<Vec<u8>>>>;
}

// mod Collection is used via pub mod collection
pub mod filter;
pub mod fusion;
pub mod multistep;
pub mod reaper;
pub mod sleep_cycle;
pub mod thermostat;
pub mod transaction;

pub use sleep_cycle::{
    run_rem_phase, run_rem_phase_with_tx, CommunityStabilityTracker, MetaChunk, RemConfig,
    RemPhaseResult,
};
pub use thermostat::{FreeEnergyThermostat, ThermostatConfig, ThermostatInputs};

pub use multistep::{MultiStepConfig, MultiStepEngine, MultiStepResult, QueryRewriter};

pub use collection::query_builder::{HybridQueryBuilder, SearchStrategy, SignalWeights};
pub use collection::Collection;
#[allow(deprecated)]
pub use filter::MetadataFilter;
pub use memfuse_checkpoint;
use memfuse_core::FilterExpr;
pub use memfuse_text::Language;

/// Herkunftsnachweis für ein einzelnes Suchergebnis.
///
/// Gibt Auskunft darüber, durch welche Signale und Indexe ein Dokument
/// in die Suchergebnisse gelangt ist, sowie über die Rohdistanzen
/// vor der Fusion.
///
/// # Phase-2-Roadmap
/// Implementiert: "ProvenanceRecord (abfragbarer Herkunftsnachweis pro Suchergebnis)"
///
/// # Invariants
/// - INV-PROV-1: sum(signal_contributions[*].rrf_contribution) ≈ unboosted RRF score (|Δ| < 1e-6)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProvenanceRecord {
    /// Vektor-Distanz vor Normalisierung (None falls Vektor-Signal nicht gefeuert)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vector_distance: Option<f32>,

    /// BM25-Score vor Fusion (None falls Text-Signal nicht gefeuert)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bm25_score: Option<f32>,

    /// Graph-Traversal-Score (None falls Graph-Signal nicht gefeuert)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph_score: Option<f32>,

    /// Reranking-Score nach Cross-Encoder (None falls Reranking nicht aktiv)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rerank_score: Option<f32>,

    /// RRF-Rang pro Signal vor Fusion: (signal_name → rang)
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub signal_ranks: std::collections::HashMap<String, u32>,

    /// Collection-Name aus der das Ergebnis stammt
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_collection: Option<String>,

    /// Index-Typ der das Ergebnis geliefert hat ("hnsw", "bm25", "graph", "diskann")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index_type: Option<String>,

    /// Per-signal RRF attribution: maps signal name to its contribution details.
    /// INV-PROV-1: The sum of all rrf_contribution values equals the unboosted RRF score.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub signal_contributions: std::collections::HashMap<String, SignalContribution>,
}

impl ProvenanceRecord {
    /// Ergänzt einen Herkunftsnachweis für synthetisierte / konsolidierte Dokumente.
    pub fn synthesized_from(source_doc_ids: &[DocId]) -> Self {
        let mut signal_ranks = std::collections::HashMap::new();
        for (idx, id) in source_doc_ids.iter().enumerate() {
            signal_ranks.insert(id.0.to_string(), (idx + 1) as u32);
        }
        ProvenanceRecord {
            index_type: Some("consolidated".to_string()),
            signal_ranks,
            ..Default::default()
        }
    }
}

/// Detailed contribution of a single signal to the final RRF score.
///
/// Enables 4-signal attribution auditing: "Why did the agent remember fact X?"
/// by recording the exact fraction each signal contributed to the fusion score.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignalContribution {
    /// Raw score from the signal's native scoring system (e.g., cosine distance, BM25 TF-IDF).
    pub raw_score: f32,
    /// 1-indexed rank of the document within this signal's result list.
    pub rank: u32,
    /// Absolute RRF contribution: weight / (k + rank + 1).
    pub rrf_contribution: f32,
}

/// User-facing search result containing the ID, score, and optional metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// The string ID provided during insert.
    pub id: String,
    /// Similarity score (higher = more similar).
    pub score: f32,
    /// Metadata associated with the document (if any).
    pub metadata: Option<Value>,
    /// List of signals (e.g. "vector", "text", "graph") that matched this document.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub matched_signals: Vec<String>,
    /// Optionaler Herkunftsnachweis — wird gesetzt wenn die Suche mit
    /// `include_provenance: true` aufgerufen wurde.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<ProvenanceRecord>,
}

/// User-facing document structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    /// The string ID.
    pub id: String,
    /// Metadata associated with the document.
    pub metadata: Option<Value>,
}

/// Overall database statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbStats {
    /// Statistics for the vector index.
    pub index_stats: memfuse_core::VectorIndexStats,
    /// Statistics for the LSM storage engine.
    pub storage_stats: memfuse_core::StorageStats,
}

/// Global configuration settings for the MemFuse database.
#[derive(Debug, Clone)]
pub struct MemFuseConfig {
    /// Vector dimensionality (must match your embeddings).
    pub dimension: usize,
    /// Maximum number of vectors to store.
    pub max_elements: usize,
    /// Distance metric for vector comparison.
    pub distance_metric: memfuse_core::DistanceMetric,
    /// Optional passphrase for encryption at rest.
    pub encryption_passphrase: Option<String>,
    /// Interval for periodic expiry reaper background tasks.
    pub expiry_reaper_interval: std::time::Duration,
    /// Optional custom persistence path for the instance-scoped orphan registry.
    /// If `None`, defaults to `<db_path>/.orphan_registry.json` when the database is opened.
    pub orphan_registry_path: Option<std::path::PathBuf>,
}

impl Default for MemFuseConfig {
    fn default() -> Self {
        // Standard-Dimension = 768 (nomic-embed-text Fallback)
        Self {
            dimension: 768,
            max_elements: 1_000_000,
            distance_metric: memfuse_core::DistanceMetric::Cosine,
            encryption_passphrase: None,
            expiry_reaper_interval: std::time::Duration::from_secs(60),
            orphan_registry_path: None,
        }
    }
}

/// # Concurrency & Lock Acquisition Hierarchy
///
/// To prevent deadlocks across concurrent database operations, all lock acquisitions must adhere strictly
/// to the following top-down hierarchy:
///
/// 1. `MemFuse::collections` (`tokio::sync::RwLock`):
///    Registry for active collection instances.
/// 2. `Collection::insert_lock` (`tokio::sync::Mutex`):
///    Mutex serializing mutation paths (`insert`, `update`, `delete`, `relate`, `repair`, `drop_collection`) per collection.
/// 3. `Collection::embedder` / `MemFuse::embedder` (`parking_lot::RwLock`):
///    Synchronous lock guarding configured text embedding engines.
///
/// **Rule**: Higher-level locks MUST always be acquired BEFORE lower-level locks. Never acquire `collections`
/// while holding `insert_lock` or `embedder`.
///
/// MemFuse — Embedded hybrid-search database for AI agents.
///
/// This is the primary entry point for all operations. It provides
/// a simple, zero-boilerplate API on top of a LSM-Tree storage engine
/// and HNSW vector index.
pub struct MemFuse {
    storage: Arc<LsmStorage>,
    next_tx: Arc<AtomicU64>,
    dimension: usize,
    expiry_reaper_interval: std::time::Duration,
    collections:
        tokio::sync::RwLock<std::collections::HashMap<String, Arc<Collection<LsmStorage>>>>,
    cancel_token: tokio_util::sync::CancellationToken,
    task_tracker: tokio_util::task::TaskTracker,
    /// Global text embedder for default collection.
    embedder: parking_lot::RwLock<Option<Arc<dyn TextEmbeddingEngine>>>,
    /// Instance-scoped orphan registry for sequence pins and checkpoints (ADR-053).
    orphan_registry: Arc<memfuse_checkpoint::InstanceOrphanRegistry>,
}

// BL-01-DB-001: Snapshot-Recovery API now exposed via create_snapshot() /
// get_at_snapshot() below.
impl MemFuse {
    #[tracing::instrument(level = "trace", skip(path))]
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_with_config(path, MemFuseConfig::default()).await
    }

    /// Opens or creates a MemFuse database with custom configuration.
    #[tracing::instrument(level = "trace", skip(path, config))]
    pub async fn open_with_config(path: impl AsRef<Path>, config: MemFuseConfig) -> Result<Self> {
        if config.dimension == 0 {
            return Err(memfuse_core::MemFuseError::invalid_input(
                "dimension must be > 0",
            ));
        }
        if config.dimension > 65536 {
            return Err(memfuse_core::MemFuseError::invalid_input(
                "dimension exceeds maximum",
            ));
        }

        let lsm_config = memfuse_store::LsmConfig {
            path: path.as_ref().to_path_buf(),
            encryption_passphrase: config.encryption_passphrase.clone(),
            ..Default::default()
        };

        let storage = Arc::new(LsmStorage::new(lsm_config).await?);
        let last_tx = storage.last_tx_id().await?.inner();
        let next_tx = Arc::new(AtomicU64::new(last_tx + 1));

        // Dimension-Check: prüfe ob gespeicherte Dim mit Config übereinstimmt
        let dim_key = b"__meta:dimension";
        if let Some(stored_dim_bytes) = storage.get(dim_key).await? {
            if let Ok(s) = std::str::from_utf8(&stored_dim_bytes) {
                if let Ok(stored_dim) = s.parse::<usize>() {
                    if stored_dim != config.dimension {
                        return Err(memfuse_core::MemFuseError::invalid_input(format!(
                            "Dimension mismatch: DB wurde mit dim={} erstellt, \
                             Config fordert dim={}. \
                             Passe MemFuseConfig::dimension an oder nutze eine neue DB.",
                            stored_dim, config.dimension
                        )));
                    }
                }
            }
        } else {
            // Erste Öffnung: Dimension persistieren
            let tx = TxId::new(0); // Internal bootstrap TX
            storage
                .put(tx, dim_key, config.dimension.to_string().as_bytes())
                .await?;
            storage.commit(tx).await?;
        }

        let cancel_token = tokio_util::sync::CancellationToken::new();
        let task_tracker = tokio_util::task::TaskTracker::new();

        let orphan_path = config
            .orphan_registry_path
            .clone()
            .unwrap_or_else(|| path.as_ref().join(".orphan_registry.json"));
        let orphan_registry = Arc::new(memfuse_checkpoint::InstanceOrphanRegistry::new(
            &orphan_path,
        ));

        let db = Self {
            storage,
            next_tx,
            dimension: config.dimension,
            expiry_reaper_interval: config.expiry_reaper_interval,
            collections: tokio::sync::RwLock::new(std::collections::HashMap::new()),
            cancel_token,
            task_tracker,
            embedder: parking_lot::RwLock::new(None),
            orphan_registry,
        };

        // Initialize already existing collections from storage
        db.initialize_collections().await?;

        // Cleanup orphaned consolidation intents on startup
        let cleaned_intents =
            context_compaction::cleanup_orphaned_consolidation_intents(db.storage.as_ref()).await?;
        tracing::debug!(cleaned_intents, "Orphaned consolidation cleanup on startup");

        // Repair-on-Open: resolve pending transaction intents and re-sync indices
        db.repair_on_open().await?;

        // Initialize the default collection backwards compatibility
        let _ = db.collection("default").await?;

        Ok(db)
    }

    #[tracing::instrument(level = "trace", skip(self))]
    async fn initialize_collections(&self) -> Result<()> {
        let col_idx_prefix = b"__col_idx:\x00";
        let entries = self.storage.scan_prefix(col_idx_prefix).await?;
        for (k, _) in entries {
            let name_bytes = &k[col_idx_prefix.len()..];
            if let Ok(name) = String::from_utf8(name_bytes.to_vec()) {
                let _ = self.collection(&name).await?;
            }
        }
        Ok(())
    }

    /// Repair-on-Open pipeline: scans for unresolved transaction intents and
    /// re-syncs HNSW indices from LSM storage to recover from crash scenarios.
    ///
    /// Recovery strategy:
    /// 1. Scan all `__tx_intent:` keys for `"pending"` status (incomplete commits).
    /// 2. For each pending intent, use forward-commit: replay missing HNSW entries
    ///    from LSM via `Collection::repair()`, then mark the intent as `"repaired"`.
    /// 3. Run `Collection::repair()` on all loaded collections to ensure LSM↔HNSW parity.
    #[tracing::instrument(level = "trace", skip(self))]
    async fn repair_on_open(&self) -> Result<()> {
        let start_time = std::time::Instant::now();

        // 1. Scan for pending transaction intents across all namespaces
        //    Default collection uses `__tx_intent:` prefix, named collections use
        //    their own namespaced prefix with key_type=3.
        let pending_intents = self.scan_pending_intents().await?;

        if !pending_intents.is_empty() {
            tracing::warn!(
                "repair_on_open: found {} pending transaction intent(s), initiating recovery",
                pending_intents.len()
            );
        }

        // 2. Forward-commit: repair all loaded collections by re-syncing HNSW from LSM.
        //    This deterministically replays any missing index entries that were lost
        //    due to the crash (LSM committed but HNSW didn't).
        let collections = self.collections.read().await;
        let mut total_repairs = 0u64;
        let mut repair_errors: Vec<String> = Vec::new();
        for (name, col) in collections.iter() {
            if let Err(e) = col.repair().await {
                tracing::error!(
                    "repair_on_open: Collection '{}' konnte nicht repariert werden: {}",
                    name,
                    e
                );
                repair_errors.push(format!("'{}': {}", name, e));
            } else {
                total_repairs += 1;
            }
        }

        // 3. Mark pending intents as "repaired" ONLY if collection repair succeeded.
        if !pending_intents.is_empty() && repair_errors.is_empty() {
            for intent_key in &pending_intents {
                let tx = match self.allocate_tx() {
                    Ok(t) => t,
                    Err(e) => {
                        tracing::error!("repair_on_open: failed to allocate tx: {}", e);
                        continue;
                    }
                };
                if let Err(e) = self.storage.put(tx, intent_key, b"repaired").await {
                    tracing::error!("repair_on_open: failed to mark intent as repaired: {}", e);
                    continue;
                }
                if let Err(e) = self.storage.commit(tx).await {
                    tracing::error!("repair_on_open: failed to commit repaired marker: {}", e);
                }
            }
        }

        let elapsed = start_time.elapsed();
        if !pending_intents.is_empty() || total_repairs > 0 {
            tracing::info!(
                "repair_on_open: completed in {:?} — {} intents resolved, {} collections verified",
                elapsed,
                pending_intents.len(),
                total_repairs
            );
        }

        if !repair_errors.is_empty() {
            return Err(memfuse_core::MemFuseError::Storage(format!(
                "repair_on_open: {} Collection(s) konnten nach Crash nicht \
                 wiederhergestellt werden: {}. \
                 Datenbankintegrität nicht garantiert — manuelle Intervention erforderlich.",
                repair_errors.len(),
                repair_errors.join(", ")
            )));
        }

        Ok(())
    }

    /// Scans storage for all transaction intent keys with `"pending"` or `"consolidation"` status.
    /// Performs garbage collection by treating orphaned Consolidation intents as pending
    /// so they get cleaned up by repair_on_open.
    #[tracing::instrument(level = "trace", skip(self))]
    async fn scan_pending_intents(&self) -> Result<Vec<Vec<u8>>> {
        let mut pending = Vec::new();

        let mut process_intent = |key: Vec<u8>, value: Vec<u8>| {
            // Legacy support
            if value == b"pending" {
                pending.push(key);
                return;
            }
            if let Ok(intent) = serde_json::from_slice::<crate::transaction::CommitIntent>(&value) {
                match intent {
                    crate::transaction::CommitIntent::Pending { .. } => pending.push(key),
                    crate::transaction::CommitIntent::Consolidation { .. } => {
                        // Orphaned consolidation sessions on startup get cleaned up
                        pending.push(key);
                    }
                    _ => {}
                }
            }
        };

        // Scan default collection's intent namespace
        let default_prefix = b"__tx_intent:";
        let entries = self.storage.scan_prefix(default_prefix).await?;
        for (key, value) in entries {
            process_intent(key, value);
        }

        // Scan named collections' intent namespaces (key_type=3 within each prefix)
        let col_idx_prefix = b"__col_idx:\x00";
        let col_entries = self.storage.scan_prefix(col_idx_prefix).await?;
        for (k, _) in col_entries {
            let name_bytes = &k[col_idx_prefix.len()..];
            if let Ok(name) = String::from_utf8(name_bytes.to_vec()) {
                let prefix = format!("__col:{}:\x00", name);
                let mut ns_prefix = prefix.into_bytes();
                ns_prefix.push(3); // key_type=3 for tx intents
                let ns_entries = self.storage.scan_prefix(&ns_prefix).await?;
                for (ns_key, ns_value) in ns_entries {
                    process_intent(ns_key, ns_value);
                }
            }
        }

        Ok(pending)
    }

    /// Returns a specific collection (namespace) with standard English tokenizer.
    /// Creates the collection if it does not already exist.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn collection(&self, name: &str) -> Result<Arc<Collection<LsmStorage>>> {
        self.collection_with_language(name, Language::English).await
    }

    /// Returns a specific collection (namespace) with a specified tokenizer language.
    /// Creates the collection if it does not already exist.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn collection_with_language(
        &self,
        name: &str,
        language: Language,
    ) -> Result<Arc<Collection<LsmStorage>>> {
        // Validation
        if name.len() > 64 {
            return Err(memfuse_core::MemFuseError::invalid_input(
                "Collection name too long (max 64)",
            ));
        }
        if !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(memfuse_core::MemFuseError::invalid_input(
                "Invalid characters in collection name",
            ));
        }

        let read_guard = self.collections.read().await;
        if let Some(col) = read_guard.get(name) {
            return Ok(Arc::clone(col));
        }
        drop(read_guard);

        let mut write_guard = self.collections.write().await;
        if let Some(col) = write_guard.get(name) {
            return Ok(Arc::clone(col));
        }

        let hnsw_config = HnswConfig {
            dimension: self.dimension,
            ..Default::default()
        };
        let index = Arc::new(HnswIndex::try_new(hnsw_config)?);

        let mut graph = memfuse_graph::CsrGraph::load_from_storage(self.storage.as_ref()).await?;
        graph.set_storage(self.storage.clone());
        let graph_index = Arc::new(graph);

        let mut col = Collection::new(
            name.to_string(),
            Arc::clone(&self.storage),
            index,
            graph_index,
            Arc::clone(&self.next_tx),
            self.dimension,
            language,
        );

        // Inherit global embedder if set
        if let Some(emb) = self.embedder.read().as_ref() {
            col = col.with_embedder(Arc::clone(emb));
        }

        // Register in storage if not default
        if name != "default" {
            let col_idx_key = [b"__col_idx:\x00", name.as_bytes()].concat();
            let tx = self.allocate_tx()?;
            self.storage.put(tx, &col_idx_key, b"{}").await?;
            self.storage.commit(tx).await?;
        }

        // Load existing data into HNSW and Text index cache
        col.load_index().await?;
        col.load_text_stats().await?;
        col.migrate_doc_keys_v1().await?;

        let col_arc = Arc::new(col);
        write_guard.insert(name.to_string(), Arc::clone(&col_arc));

        let reaper_handle = reaper::start_expiry_reaper(
            Arc::clone(&col_arc),
            self.expiry_reaper_interval,
            self.cancel_token.clone(),
        );
        self.task_tracker.spawn(async move {
            if let Err(e) = reaper_handle.await {
                tracing::warn!(error = %e, "Reaper handle task failed or was cancelled");
            }
        });

        Ok(col_arc)
    }

    /// Allokiert eine eindeutige, atomar inkrementierte Transaction-ID.
    /// EINZIGE legale TxId-Quelle für externe Crates (verhindert Kollisionen).
    pub fn allocate_tx(&self) -> Result<TxId> {
        let id = self.next_tx.fetch_add(1, Ordering::SeqCst);
        if id > TxId::MAX_COLLECTION_SEQUENCE {
            return Err(memfuse_core::MemFuseError::Transaction(
                "TxId counter exhausted: MAX_COLLECTION_SEQUENCE range exceeded. Collection must be recreated.".into(),
            ));
        }
        Ok(TxId::new(id))
    }

    /// Lists all existing collection names (including those persisted in storage).
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn list_collections(&self) -> Result<Vec<String>> {
        let col_idx_prefix = b"__col_idx:\x00";
        let entries = self.storage.scan_prefix(col_idx_prefix).await?;

        let mut names = std::collections::HashSet::new();
        names.insert("default".to_string());

        for (k, _) in entries {
            let name_bytes = &k[col_idx_prefix.len()..];
            if let Ok(name) = String::from_utf8(name_bytes.to_vec()) {
                names.insert(name);
            }
        }

        // Also add active in-memory ones (should be covered by storage but just in case)
        let guard = self.collections.read().await;
        for name in guard.keys() {
            names.insert(name.clone());
        }

        let mut sorted_names: Vec<String> = names.into_iter().collect();
        sorted_names.sort();
        Ok(sorted_names)
    }

    /// Drops a collection, removing all its data from storage.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn drop_collection(&self, name: &str) -> Result<()> {
        if name == "default" {
            return Err(memfuse_core::MemFuseError::invalid_input(
                "Cannot drop default collection",
            ));
        }

        let tx = self.allocate_tx()?;

        // 1. Delete all collection data keys (prefix-based)
        let col_data_prefix = format!("__col:{}:", name);
        self.storage
            .delete_prefix(tx, col_data_prefix.as_bytes())
            .await?;

        // 2. Delete all text index data keys for this collection
        let txt_data_prefix = format!("__txt:{}:", name);
        self.storage
            .delete_prefix(tx, txt_data_prefix.as_bytes())
            .await?;

        // 3. Delete the index key itself
        let col_idx_key = [b"__col_idx:\x00", name.as_bytes()].concat();
        self.storage.delete(tx, &col_idx_key).await?;

        // 4. Commit deleting operations in persistent storage first
        self.storage.commit(tx).await?;

        // 5. Remove from in-memory collection registry ONLY after successful commit
        self.collections.write().await.remove(name);

        Ok(())
    }

    // --- Legacy Backwards Compatibility Methods (Wraps "default" collection) ---

    async fn default_col(&self) -> Result<Arc<Collection<LsmStorage>>> {
        self.collection("default").await
    }

    /// Stores a non-vector key-value entry directly in LSM storage without touching vector, text, or graph indices.
    #[tracing::instrument(level = "trace", skip(self, value))]
    pub async fn put_kv(&self, id: &str, value: &Value) -> Result<()> {
        self.default_col().await?.put_kv(id, value).await
    }

    /// Retrieves a key-value entry directly from LSM storage.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn get_kv(&self, id: &str) -> Result<Option<Value>> {
        self.default_col().await?.get_kv(id).await
    }

    /// Inserts a document with an embedding and optional metadata.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn insert(&self, id: &str, embedding: &[f32], metadata: Option<Value>) -> Result<()> {
        self.default_col()
            .await?
            .insert(id, embedding, metadata)
            .await
    }

    /// Speichert ein Dokument mit expliziter kognitiver Gedächtnisklassifikation.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn insert_typed(
        &self,
        collection_name: &str,
        id: &str,
        embedding: &[f32],
        memory_type: memfuse_core::MemoryType,
        metadata: Option<Value>,
    ) -> Result<()> {
        self.collection(collection_name)
            .await?
            .insert_typed(id, embedding, memory_type, metadata)
            .await
    }

    /// Upserts a document (inserts if missing, updates if exists) atomically.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn upsert(&self, id: &str, embedding: &[f32], metadata: Option<Value>) -> Result<()> {
        self.default_col()
            .await?
            .upsert(id, embedding, metadata)
            .await
    }

    /// Inserts multiple documents in a single transaction.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn insert_many(&self, docs: &[(String, Vec<f32>, Option<Value>)]) -> Result<()> {
        self.default_col().await?.insert_many(docs).await
    }

    /// Upserts multiple documents in a single transaction.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn upsert_many(&self, docs: &[(String, Vec<f32>, Option<Value>)]) -> Result<()> {
        self.default_col().await?.upsert_many(docs).await
    }

    /// Retrieves a document by its string key.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn get(&self, id: &str) -> Result<Option<Document>> {
        self.default_col().await?.get(id).await
    }

    /// Retrieves a document at a specific point in time.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn get_at_snapshot(&self, id: &str, seq_no: u64) -> Result<Option<Document>> {
        self.default_col().await?.get_at_snapshot(id, seq_no).await
    }

    /// Returns the last committed sequence number.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn last_committed_seq(&self) -> Result<u64> {
        self.storage.last_seq_no().await
    }

    /// Creates an MVCC snapshot of the current database state.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn create_snapshot(&self) -> Result<u64> {
        self.storage.last_seq_no().await
    }

    /// Updates a document's embedding and/or metadata.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn update(&self, id: &str, embedding: &[f32], metadata: Option<Value>) -> Result<()> {
        self.default_col()
            .await?
            .update(id, embedding, metadata)
            .await
    }

    /// Performs semantic k-NN search over stored embeddings.
    #[allow(deprecated)]
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn search(&self, query: &[f32], k: usize) -> Result<Vec<SearchResult>> {
        self.default_col()
            .await?
            .query()
            .embedding(query)
            .k(k)
            .execute()
            .await
    }

    /// Performs semantic search with an advanced metadata filter.
    #[deprecated(
        since = "0.1.0",
        note = "Use search_with_filter_expr with memfuse_core::FilterExpr directly"
    )]
    #[allow(deprecated)]
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn search_with_filter(
        &self,
        query: &[f32],
        k: usize,
        filter: Option<MetadataFilter>,
    ) -> Result<Vec<SearchResult>> {
        let col = self.default_col().await?;
        let mut builder = col.query().vector(query).k(k);
        if let Some(f) = filter {
            builder = builder.metadata_filter(f);
        }
        builder.execute().await
    }

    /// Performs semantic search with an advanced metadata filter expression (`FilterExpr`).
    #[allow(deprecated)]
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn search_with_filter_expr(
        &self,
        query: &[f32],
        k: usize,
        filter: Option<FilterExpr>,
    ) -> Result<Vec<SearchResult>> {
        let col = self.default_col().await?;
        let mut builder = col.query().embedding(query).k(k);
        if let Some(f) = filter {
            builder = builder.filter(f);
        }
        builder.execute().await
    }

    /// Inserts a text document via the default collection.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn insert_text_only(
        &self,
        id: &str,
        text: &str,
        metadata: Option<Value>,
    ) -> Result<()> {
        self.default_col()
            .await?
            .insert_text_only(id, text, metadata)
            .await
    }

    /// Upserts a text document via the default collection.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn upsert_text_only(
        &self,
        id: &str,
        text: &str,
        metadata: Option<Value>,
    ) -> Result<()> {
        self.default_col()
            .await?
            .upsert_text_only(id, text, metadata)
            .await
    }

    /// Performs text search via the default collection.
    #[allow(deprecated)]
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn search_text(&self, text: &str, k: usize) -> Result<Vec<SearchResult>> {
        self.default_col()
            .await?
            .query()
            .text(text)
            .k(k)
            .execute()
            .await
    }

    /// Performs semantic k-NN search with an optional filter function over documents.
    #[allow(deprecated)]
    #[tracing::instrument(level = "trace", skip(self, filter))]
    pub async fn search_filtered(
        &self,
        query: &[f32],
        k: usize,
        filter: Option<&(dyn Fn(DocId) -> bool + Send + Sync)>,
    ) -> Result<Vec<SearchResult>> {
        #[allow(deprecated)]
        self.default_col()
            .await?
            .search_filtered(query, k, filter)
            .await
    }

    /// Performs hybrid search combining BM25, vector search, and graph traversal.
    #[allow(deprecated)]
    #[tracing::instrument(level = "trace", skip(self, anchor_entities))]
    pub async fn hybrid_search(
        &self,
        text: &str,
        vector: &[f32],
        k: usize,
        anchor_entities: Option<&[memfuse_core::EntityId]>,
    ) -> Result<Vec<SearchResult>> {
        let col = self.default_col().await?;
        let mut builder = col.query().text(text).vector(vector).k(k);
        if let Some(anchors) = anchor_entities {
            builder = builder.anchors(anchors.iter().copied());
        }
        builder.execute().await
    }

    /// Performs hybrid search combining BM25, vector search, and graph traversal, followed by optional Cross-Encoder reranking.
    #[cfg(feature = "reranking")]
    #[allow(deprecated)]
    #[tracing::instrument(level = "trace", skip(self, reranker, anchor_entities))]
    pub async fn hybrid_search_reranked(
        &self,
        text: &str,
        vector: &[f32],
        k: usize,
        reranker: Option<&memfuse_embed::CrossEncoderReranker>,
        anchor_entities: Option<&[memfuse_core::EntityId]>,
    ) -> Result<Vec<SearchResult>> {
        let col = self.default_col().await?;
        let mut builder = col.query().text(text).vector(vector).k(k);
        if let Some(r) = reranker {
            builder = builder.reranker(r);
        }
        if let Some(anchors) = anchor_entities {
            builder = builder.anchors(anchors.iter().copied());
        }
        builder.execute().await
    }

    /// Performs hybrid search with custom signal fusion weights.
    #[allow(deprecated)]
    #[tracing::instrument(level = "trace", skip(self, anchor_entities, weights))]
    pub async fn hybrid_search_with_weights(
        &self,
        text: &str,
        vector: &[f32],
        k: usize,
        anchor_entities: Option<&[memfuse_core::EntityId]>,
        weights: Option<&memfuse_core::FusionWeights>,
    ) -> Result<Vec<SearchResult>> {
        let col = self.default_col().await?;
        let mut builder = col.query().text(text).vector(vector).k(k);
        if let Some(w) = weights {
            builder = builder.fusion_weights(w.clone());
        }
        if let Some(anchors) = anchor_entities {
            builder = builder.anchors(anchors.iter().copied());
        }
        builder.execute().await
    }

    /// Performs hybrid search with custom signal fusion weights and graph traversal strategy.
    #[allow(deprecated)]
    #[tracing::instrument(level = "trace", skip(self, anchor_entities, weights, strategy))]
    pub async fn hybrid_search_with_strategy(
        &self,
        text: &str,
        vector: &[f32],
        k: usize,
        anchor_entities: Option<&[memfuse_core::EntityId]>,
        weights: Option<&memfuse_core::FusionWeights>,
        strategy: Option<&memfuse_core::GraphTraversalStrategy>,
    ) -> Result<Vec<SearchResult>> {
        let col = self.default_col().await?;
        let mut builder = col.query().text(text).vector(vector).k(k);
        if let Some(w) = weights {
            builder = builder.fusion_weights(w.clone());
        }
        if let Some(s) = strategy {
            builder = builder.strategy(s.clone());
        }
        if let Some(anchors) = anchor_entities {
            builder = builder.anchors(anchors.iter().copied());
        }
        builder.execute().await
    }

    /// Performs hybrid search using a `HybridQuery` configuration object.
    #[allow(deprecated)]
    #[tracing::instrument(level = "trace", skip(self, query))]
    pub async fn hybrid_search_with_query(
        &self,
        query: &memfuse_core::HybridQuery,
    ) -> Result<Vec<SearchResult>> {
        self.default_col()
            .await?
            .query()
            .query_config(query)
            .execute()
            .await
    }

    /// Deletes a document by its string ID.
    pub async fn delete(&self, id: &str) -> Result<()> {
        self.default_col().await?.delete(id).await
    }

    /// Creates a bidirectional relationship between two documents.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn relate(&self, from: &str, to: &str, label: &str) -> Result<()> {
        let col = self.default_col().await?;
        col.relate_bidirectional(from, to, label).await
    }

    /// Scans storage for key-value pairs matching a prefix.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn scan_prefix(&self, prefix: &str) -> Result<Vec<(String, Value)>> {
        self.default_col().await?.scan_prefix(prefix).await
    }

    /// Returns the number of vectors in the index.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn len(&self) -> Result<usize> {
        Ok(self.default_col().await?.len().await)
    }

    /// Returns true if the database is empty.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn is_empty(&self) -> Result<bool> {
        Ok(self.default_col().await?.is_empty().await)
    }

    /// Scans a range of keys, returning key-value pairs.
    #[tracing::instrument(level = "trace", skip(self, start, end))]
    pub async fn scan(
        &self,
        start: std::ops::Bound<&[u8]>,
        end: std::ops::Bound<&[u8]>,
    ) -> Result<Vec<(String, Value)>> {
        self.default_col().await?.scan(start, end).await
    }

    /// Returns combined statistics for the vector index and storage engine.
    ///
    /// Stats are approximate and may be briefly inconsistent across subsystems due to concurrent operations.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn stats(&self) -> Result<DbStats> {
        Ok(DbStats {
            index_stats: self.default_col().await?.stats().await?,
            storage_stats: self.storage.stats().await?,
        })
    }
    /// Flushes all pending writes to disk.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn flush(&self) -> Result<()> {
        self.storage.flush().await?;
        Ok(())
    }

    /// Signals background tasks to shut down.
    pub fn shutdown(&self) {
        self.cancel_token.cancel();
    }

    /// Waits for all background tasks to shut down fully.
    pub async fn wait_shutdown(&self) {
        self.shutdown();
        self.task_tracker.close();
        self.task_tracker.wait().await;
    }

    /// Gracefully closes the database, ensuring all data is persisted.
    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn close(self) -> Result<()> {
        self.wait_shutdown().await;
        self.storage.close().await?;
        self.flush().await?;
        Ok(())
    }

    /// Sets the text embedder for default collection operations.
    #[tracing::instrument(level = "trace", skip(self, embedder))]
    pub async fn with_embedder(self, embedder: Arc<dyn TextEmbeddingEngine>) -> Self {
        {
            let mut guard = self.embedder.write();
            *guard = Some(Arc::clone(&embedder));
        }
        let cols = self.collections.read().await;
        if let Some(col) = cols.get("default") {
            *col.embedder.write() = Some(embedder);
        }
        drop(cols);

        self
    }

    /// Sets the text embedder (non-consuming version).
    #[tracing::instrument(level = "trace", skip(self, embedder))]
    pub async fn set_embedder(&self, embedder: Arc<dyn TextEmbeddingEngine>) -> Result<()> {
        {
            let mut guard = self.embedder.write();
            *guard = Some(Arc::clone(&embedder));
        }
        let collections_read = self.collections.read().await;
        if let Some(col) = collections_read.get("default") {
            let mut guard = col.embedder.write();
            *guard = Some(embedder);
        }
        Ok(())
    }

    /// Liefert die instanzgebundene Orphan Registry für diese MemFuse-Instanz.
    pub fn orphan_registry(&self) -> &Arc<memfuse_checkpoint::InstanceOrphanRegistry> {
        &self.orphan_registry
    }
}

// Re-export for convenience
pub use memfuse_core::DistanceMetric;
pub use serde_json::json;

impl MemFuse {
    /// Returns the underlying storage engine.
    /// Internal use only for benchmarks and tests.
    #[doc(hidden)]
    pub fn inner_storage(&self) -> Arc<LsmStorage> {
        self.storage.clone()
    }
}

#[cfg(feature = "sandbox")]
impl SandboxBridge for MemFuse {
    fn db_search<'a>(&'a self, query: &'a [u8], k: usize) -> BoxFuture<'a, Result<Vec<u8>>> {
        Box::pin(async move {
            // Assume query is a binary f32 array (little endian)
            let f32_count = query.len() / 4;
            let mut vector = Vec::with_capacity(f32_count);
            for i in 0..f32_count {
                let start = i * 4;
                let bits = u32::from_le_bytes(
                    query
                        .get(start..start + 4)
                        .ok_or_else(|| {
                            memfuse_core::MemFuseError::Serialization("Query too short".into())
                        })?
                        .try_into()
                        .map_err(|_| {
                            memfuse_core::MemFuseError::Serialization("Invalid slice".into())
                        })?,
                );
                vector.push(f32::from_bits(bits));
            }

            let results: Vec<SearchResult> = self.search(&vector, k).await?;
            Ok(serde_json::to_vec(&results)
                .map_err(|e| memfuse_core::MemFuseError::Internal(e.to_string()))?)
        })
    }

    fn db_insert<'a>(&'a self, key: &'a [u8], value: &'a [u8]) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let id = String::from_utf8_lossy(key).to_string();
            // Assume value is a JSON representing (embedding, metadata) or just value
            let val_json: Value = serde_json::from_slice(value)
                .unwrap_or(serde_json::json!({ "raw_data": String::from_utf8_lossy(value) }));

            self.insert(&id, &[], Some(val_json)).await
        })
    }

    fn db_get<'a>(&'a self, key: &'a [u8]) -> BoxFuture<'a, Result<Option<Vec<u8>>>> {
        Box::pin(async move {
            let id = String::from_utf8_lossy(key).to_string();
            let doc = self.get(&id).await?;
            match doc {
                Some(d) => {
                    Ok(Some(serde_json::to_vec(&d).map_err(|e| {
                        memfuse_core::MemFuseError::Internal(e.to_string())
                    })?))
                }
                None => Ok(None),
            }
        })
    }
}

#[cfg(test)]
#[allow(deprecated)]
mod tests {
    // expect #[cfg(test)]
    // unwrap #[cfg(test)]
    use super::*;
    use tempfile::TempDir;

    async fn test_db(dim: usize) -> (MemFuse, TempDir) {
        let tmp = TempDir::new().expect("temp dir"); // expect
        let config = MemFuseConfig {
            dimension: dim,
            max_elements: 10_000,
            distance_metric: DistanceMetric::Cosine,
            ..Default::default()
        };
        let db = MemFuse::open_with_config(tmp.path(), config)
            .await
            .expect("open db"); // expect
        (db, tmp)
    }

    #[tokio::test]
    async fn test_insert_search_roundtrip() {
        let (db, _tmp) = test_db(4).await;

        db.insert(
            "doc-1",
            &[1.0, 0.0, 0.0, 0.0],
            Some(json!({"topic": "rust"})),
        )
        .await
        .expect("insert"); // expect

        db.insert(
            "doc-2",
            &[0.0, 1.0, 0.0, 0.0],
            Some(json!({"topic": "python"})),
        )
        .await
        .expect("insert"); // expect

        db.insert("doc-3", &[0.9, 0.1, 0.0, 0.0], None)
            .await
            .expect("insert"); // expect

        let results = db.search(&[1.0, 0.0, 0.0, 0.0], 2).await.expect("search"); // expect
        assert_eq!(results.len(), 2);
        // doc-1 should be closest
        assert!(results[0].score > results[1].score);
    }

    #[tokio::test]
    async fn test_insert_search_returns_metadata() {
        let (db, _tmp) = test_db(4).await;

        db.insert(
            "doc-1",
            &[1.0, 0.0, 0.0, 0.0],
            Some(json!({"topic": "rust", "priority": 1})),
        )
        .await
        .expect("insert"); // expect

        let results = db.search(&[1.0, 0.0, 0.0, 0.0], 1).await.expect("search"); // expect
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "doc-1");
        let meta = results[0].metadata.as_ref().expect("metadata should exist"); // expect
        assert_eq!(meta["topic"], "rust");
        assert_eq!(meta["priority"], 1);
    }

    #[tokio::test]
    async fn test_get_by_key() {
        let (db, _tmp) = test_db(4).await;

        db.insert(
            "doc-1",
            &[1.0, 0.0, 0.0, 0.0],
            Some(json!({"topic": "rust"})),
        )
        .await
        .expect("insert"); // expect

        let doc = db.get("doc-1").await.expect("get").expect("should exist"); // expect
        assert_eq!(doc.id, "doc-1");
        assert_eq!(doc.metadata.expect("valid")["topic"], "rust"); // expect

        let none = db.get("nonexistent").await.expect("get"); // expect
        assert!(none.is_none());
    }

    #[tokio::test]
    async fn test_update() {
        let (db, _tmp) = test_db(4).await;

        db.insert("doc-1", &[1.0, 0.0, 0.0, 0.0], Some(json!({"v": 1})))
            .await
            .expect("insert"); // expect

        db.update("doc-1", &[0.0, 1.0, 0.0, 0.0], Some(json!({"v": 2})))
            .await
            .expect("update"); // expect

        // Metadata should be updated
        let doc = db.get("doc-1").await.expect("get").expect("exists"); // expect
        assert_eq!(doc.metadata.expect("valid")["v"], 2); // expect

        // Vector should be updated — search for new vector should find it
        let results = db.search(&[0.0, 1.0, 0.0, 0.0], 1).await.expect("search"); // expect
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "doc-1");
    }

    #[tokio::test]
    async fn test_delete() {
        let (db, _tmp) = test_db(4).await;

        db.insert("doc-1", &[1.0, 0.0, 0.0, 0.0], None)
            .await
            .expect("insert"); // expect
        assert_eq!(db.len().await.expect("len"), 1); // expect

        db.delete("doc-1").await.expect("delete"); // expect
        assert_eq!(db.len().await.expect("len"), 0); // expect

        // get should return None after delete
        let doc = db.get("doc-1").await.expect("get"); // expect
        assert!(doc.is_none());
    }

    #[tokio::test]
    async fn test_relate() {
        let (db, _tmp) = test_db(4).await;

        db.insert("doc-1", &[1.0, 0.0, 0.0, 0.0], None)
            .await
            .expect("insert"); // expect
        db.insert("doc-2", &[0.0, 1.0, 0.0, 0.0], None)
            .await
            .expect("insert"); // expect

        // Should not error
        db.relate("doc-1", "doc-2", "references")
            .await
            .expect("relate"); // expect
    }

    #[tokio::test]
    async fn test_dimension_mismatch() {
        let (db, _tmp) = test_db(4).await;
        let result = db.insert("doc-1", &[1.0, 0.0], None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_empty_search() {
        let (db, _tmp) = test_db(4).await;
        let results = db.search(&[1.0, 0.0, 0.0, 0.0], 5).await.expect("search"); // expect
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_relate_and_scan_prefix() {
        let (db, _tmp) = test_db(4).await;

        db.insert("doc-1", &[1.0, 0.0, 0.0, 0.0], None)
            .await
            .expect("insert"); // expect
        db.insert("doc-2", &[0.0, 1.0, 0.0, 0.0], None)
            .await
            .expect("insert"); // expect
        db.insert("doc-3", &[0.0, 0.0, 1.0, 0.0], None)
            .await
            .expect("insert"); // expect

        db.relate("doc-1", "doc-2", "references")
            .await
            .expect("relate"); // expect
        db.relate("doc-1", "doc-3", "references")
            .await
            .expect("relate"); // expect

        // Scan for relations of doc-1
        let results = db
            .scan_prefix("__rel:doc-1:references:")
            .await
            .expect("scan"); // expect
        assert_eq!(results.len(), 2);

        let related_ids: Vec<String> = results
            .into_iter()
            .map(|(_, v)| v["to"].as_str().expect("valid").to_string()) // expect
            .collect();
        assert!(related_ids.contains(&"doc-2".to_string()));
        assert!(related_ids.contains(&"doc-3".to_string()));

        // Check backward edge setup automatically
        let backward_results = db
            .scan_prefix("__rel:doc-2:references:")
            .await
            .expect("scan bwd"); // expect
        assert_eq!(backward_results.len(), 1);
        assert_eq!(backward_results[0].1["to"], "doc-1");
    }

    #[tokio::test]
    async fn test_stats_aggregation() {
        let (db, _tmp) = test_db(4).await;

        db.insert("doc-1", &[1.0, 0.0, 0.0, 0.0], None)
            .await
            .expect("insert"); // expect

        let stats = db.stats().await.expect("stats"); // expect
        assert_eq!(stats.index_stats.num_vectors, 1);
        assert!(stats.storage_stats.memtable_size_bytes > 0);
    }

    #[tokio::test]
    async fn test_integration_end_to_end() {
        let (db, _tmp) = test_db(4).await;

        // 1. Insert
        db.insert(
            "agent-1",
            &[1.0, 0.5, 0.0, 0.0],
            Some(json!({"type": "agent"})),
        )
        .await
        .expect("insert agent"); // expect
        db.insert(
            "task-1",
            &[0.9, 0.6, 0.0, 0.0],
            Some(json!({"type": "task"})),
        )
        .await
        .expect("insert task"); // expect
        db.insert(
            "task-2",
            &[0.0, 0.0, 1.0, 0.5],
            Some(json!({"type": "task"})),
        )
        .await
        .expect("insert task 2"); // expect

        // 2. Relate
        db.relate("agent-1", "task-1", "assigned_to")
            .await
            .expect("relate 1"); // expect
        db.relate("agent-1", "task-2", "assigned_to")
            .await
            .expect("relate 2"); // expect

        // 3. Search
        let results = db.search(&[1.0, 0.5, 0.0, 0.0], 2).await.expect("search"); // expect
        assert_eq!(results[0].id, "agent-1"); // Exactly matches
        assert_eq!(results[1].id, "task-1"); // Close match

        // 4. Update
        db.update(
            "task-1",
            &[0.1, 0.1, 0.9, 0.9],
            Some(json!({"type": "task", "status": "done"})),
        )
        .await
        .expect("update task"); // expect

        // 5. Scan prefix
        let edges = db
            .scan_prefix("__rel:agent-1:assigned_to:")
            .await
            .expect("scan"); // expect
        assert_eq!(edges.len(), 2);

        // 6. Delete
        db.delete("agent-1").await.expect("delete"); // expect

        // 7. Verify empty search and missing doc
        let get_agent = db.get("agent-1").await.expect("get"); // expect
        assert!(get_agent.is_none());
        assert_eq!(db.len().await.expect("len"), 2); // 3 inserted, 1 deleted // expect
    }

    #[tokio::test]
    async fn test_allocate_tx_exhaustion_returns_err() {
        let tmp = TempDir::new().expect("temp dir"); // expect
        let config = MemFuseConfig {
            dimension: 4,
            ..Default::default()
        };
        let db = MemFuse::open_with_config(tmp.path(), config)
            .await
            .expect("open db"); // expect
        db.next_tx.store(TxId::INTERNAL_BASE, Ordering::SeqCst);
        let res = db.allocate_tx();
        assert!(matches!(
            res,
            Err(memfuse_core::MemFuseError::Transaction(_))
        ));
    }

    #[tokio::test]
    async fn test_concurrent_collection_idempotency() {
        let (db, _tmp) = test_db(4).await;
        let db = Arc::new(db);
        let mut handles = Vec::new();

        for _ in 0..10 {
            let db_clone = db.clone();
            handles.push(tokio::spawn(async move {
                db_clone.collection("c").await.expect("collection c") // expect
            }));
        }

        let mut cols = Vec::new();
        for handle in handles {
            cols.push(handle.await.expect("join handle")); // expect
        }

        let first = &cols[0];
        for col in &cols[1..] {
            assert!(
                Arc::ptr_eq(first, col),
                "collection(\"c\") must return the exact same Arc instance"
            );
        }
    }

    #[tokio::test]
    async fn collections_are_isolated() {
        let (db, _tmp) = test_db(4).await;
        let vec = vec![1.0, 0.0, 0.0, 0.0];
        let col_a = db.collection("alpha").await.unwrap(); // unwrap
        let col_b = db.collection("beta").await.unwrap(); // unwrap
        col_a.insert("doc1", &vec, None).await.unwrap(); // unwrap
        let results = col_b.search(&vec, 10).await.unwrap(); // unwrap
        assert!(
            results.is_empty(),
            "Collection B must not see Collection A's data"
        );
    }

    #[tokio::test]
    async fn test_collections_are_isolated() {
        let (db, _tmp) = test_db(4).await;
        let col_a = db.collection("a").await.expect("col a"); // expect
        let col_b = db.collection("b").await.expect("col b"); // expect

        col_a
            .insert("k1", &[1.0, 0.0, 0.0, 0.0], Some(json!({"val": "a"})))
            .await
            .expect("ins a"); // expect
        col_b
            .insert("k1", &[0.0, 1.0, 0.0, 0.0], Some(json!({"val": "b"})))
            .await
            .expect("ins b"); // expect

        let res_a = col_a.get("k1").await.expect("get a").expect("exists"); // expect
        let res_b = col_b.get("k1").await.expect("get b").expect("exists"); // expect

        assert_eq!(res_a.metadata.expect("test")["val"], "a"); // expect
        assert_eq!(res_b.metadata.expect("test")["val"], "b"); // expect

        let search_a = col_a
            .search(&[1.0, 0.0, 0.0, 0.0], 1)
            .await
            .expect("search a"); // expect
        assert_eq!(search_a.len(), 1);
        assert_eq!(search_a[0].id, "k1");
        assert_eq!(search_a[0].metadata.as_ref().expect("test")["val"], "a"); // expect
    }

    #[tokio::test]
    async fn test_close_and_reopen_100_docs() {
        let tmp = TempDir::new().expect("temp dir"); // expect
        let path = tmp.path().to_path_buf();
        let config = MemFuseConfig {
            dimension: 4,
            ..Default::default()
        };

        {
            let db = MemFuse::open_with_config(&path, config.clone())
                .await
                .expect("open 1"); // expect
            for i in 0..100 {
                let id = format!("doc-{}", i);
                let val = (i as f32) / 100.0;
                db.insert(&id, &[val, 1.0 - val, 0.0, 0.0], Some(json!({"idx": i})))
                    .await
                    .expect("insert"); // expect
            }
            db.close().await.expect("close"); // expect
        }

        {
            let db = MemFuse::open_with_config(&path, config)
                .await
                .expect("open 2"); // expect
            assert_eq!(db.len().await.expect("len"), 100); // expect
            for i in 0..100 {
                let id = format!("doc-{}", i);
                let doc = db.get(&id).await.expect("get").expect("exists"); // expect
                assert_eq!(doc.id, id);
                assert_eq!(doc.metadata.expect("valid")["idx"], i); // expect
            }
            let results = db.search(&[0.5, 0.5, 0.0, 0.0], 10).await.expect("search"); // expect
            assert_eq!(results.len(), 10);
        }
    }

    #[tokio::test]
    async fn test_close_and_reopen() {
        let tmp = TempDir::new().expect("temp dir"); // expect
        let path = tmp.path().to_path_buf();
        let config = MemFuseConfig {
            dimension: 4,
            ..Default::default()
        };

        {
            let db = MemFuse::open_with_config(&path, config.clone())
                .await
                .expect("open 1"); // expect
            db.insert("doc-1", &[1.0, 0.0, 0.0, 0.0], Some(json!({"v": 1})))
                .await
                .expect("insert"); // expect
            db.close().await.expect("close"); // expect
        }

        {
            let db = MemFuse::open_with_config(&path, config)
                .await
                .expect("open 2"); // expect
            let doc = db.get("doc-1").await.expect("get").expect("exists"); // expect
            assert_eq!(doc.id, "doc-1");
            assert_eq!(doc.metadata.expect("valid")["v"], 1); // expect
        }
    }

    #[tokio::test]
    async fn test_drop_removes_all_data() {
        let (db, _tmp) = test_db(4).await;
        let col = db.collection("drop-me").await.expect("col"); // expect
        col.insert("k1", &[1.0, 0.0, 0.0, 0.0], None)
            .await
            .expect("ins"); // expect

        db.drop_collection("drop-me").await.expect("drop"); // expect

        let col2 = db.collection("drop-me").await.expect("re-create"); // expect
        assert_eq!(col2.len().await, 0);
        assert!(col2.get("k1").await.expect("get").is_none()); // expect
    }

    #[tokio::test]
    async fn test_default_collection_compat() {
        let (db, _tmp) = test_db(4).await;
        db.insert("k", &[1.0, 0.0, 0.0, 0.0], Some(json!({"v": 1})))
            .await
            .expect("ins"); // expect

        let doc = db.get("k").await.expect("get").expect("exists"); // expect
        assert_eq!(doc.id, "k");

        let results = db.search(&[1.0, 0.0, 0.0, 0.0], 1).await.expect("search"); // expect
        assert_eq!(results[0].id, "k");
    }

    #[tokio::test]
    async fn test_list_collections() {
        let (db, _tmp) = test_db(4).await;
        db.collection("c1").await.expect("c1"); // expect
        db.collection("c2").await.expect("c2"); // expect
        db.collection("c3").await.expect("c3"); // expect

        let list = db.list_collections().await.expect("list"); // expect
        assert!(list.contains(&"default".to_string()));
        assert!(list.contains(&"c1".to_string()));
        assert!(list.contains(&"c2".to_string()));
        assert!(list.contains(&"c3".to_string()));
        assert_eq!(list.len(), 4);
    }

    #[tokio::test]
    async fn test_repair_on_open_resolves_pending_intents() {
        let tmp = tempfile::TempDir::new().expect("temp dir"); // expect
        let path = tmp.path().to_path_buf();
        let config = MemFuseConfig {
            dimension: 4,
            ..Default::default()
        };

        // 1. Create a doc in LSM but NOT in HNSW to simulate a partial commit
        {
            let db = MemFuse::open_with_config(&path, config.clone())
                .await
                .expect("open 1"); // expect
            let col = db.collection("recovery-test").await.expect("col"); // expect

            // We'll use a direct LSM put to bypass HNSW
            let doc_id = DocId::from_key("recovered-doc").expect("doc_id"); // expect
            let stored = crate::collection::StoredDocument {
                id: "recovered-doc".to_string(),
                embedding: vec![1.0, 0.0, 0.0, 0.0],
                metadata: Some(json!({"status": "recovered"})),
            };
            let data = serde_json::to_vec(&stored).expect("json"); // expect

            let user_key = col.namespaced_key(b"recovered-doc", 0);
            let doc_key = col.namespaced_key(&doc_id.inner().to_le_bytes(), 1);

            // Put in LSM
            let tx = TxId::new(db.next_tx.fetch_add(1, Ordering::SeqCst));
            db.storage
                .put(tx, &user_key, &data)
                .await
                .expect("put user"); // expect
            db.storage.put(tx, &doc_key, &data).await.expect("put doc"); // expect

            // Manually write a "pending" intent
            let intent_key = col.namespaced_key(tx.inner().to_le_bytes().as_ref(), 3);
            db.storage
                .put(tx, &intent_key, b"pending")
                .await
                .expect("put intent"); // expect

            db.storage.commit(tx).await.expect("commit"); // expect

            // Verify it's NOT in HNSW yet (search should fail to find it)
            let results = col.search(&[1.0, 0.0, 0.0, 0.0], 1).await.expect("search"); // expect
            assert!(results.is_empty(), "Should not be in HNSW yet");

            db.close().await.expect("close"); // expect
        }

        // 2. Re-open: repair_on_open should trigger and re-sync
        {
            let db = MemFuse::open_with_config(&path, config)
                .await
                .expect("open 2 (repair)"); // expect
            let col = db.collection("recovery-test").await.expect("col"); // expect

            // Verify it IS now in HNSW
            let results = col.search(&[1.0, 0.0, 0.0, 0.0], 1).await.expect("search"); // expect
            assert_eq!(results.len(), 1, "Should be repaired and found in HNSW");
            assert_eq!(results[0].id, "recovered-doc");

            // Verify intent is marked as repaired
            let entries = db
                .storage
                .scan_prefix(b"__col:recovery-test:\x00\x03")
                .await
                .expect("scan intents"); // expect
            let found_repaired = entries.iter().any(|(_, v)| v == b"repaired");
            assert!(found_repaired, "Intent should be marked as repaired");
        }
    }

    #[tokio::test]
    async fn test_repair_on_open_idempotent_with_existing_vector() {
        use memfuse_core::VectorIndex;
        let tmp = tempfile::TempDir::new().expect("temp dir"); // expect
        let path = tmp.path().to_path_buf();
        let config = MemFuseConfig {
            dimension: 4,
            ..Default::default()
        };

        // 1. Create a doc in LSM and also insert its vector into HNSW, but leave intent as Pending
        {
            let db = MemFuse::open_with_config(&path, config.clone())
                .await
                .expect("open 1"); // expect
            let col = db.collection("idempotent-test").await.expect("col"); // expect

            let doc_id = DocId::from_key("already-indexed-doc").expect("doc_id"); // expect
            let stored = crate::collection::StoredDocument {
                id: "already-indexed-doc".to_string(),
                embedding: vec![1.0, 0.0, 0.0, 0.0],
                metadata: Some(json!({"status": "already_indexed"})),
            };
            let data = serde_json::to_vec(&stored).expect("json"); // expect

            let user_key = col.namespaced_key(b"already-indexed-doc", 0);
            let doc_key = col.namespaced_key(&doc_id.inner().to_le_bytes(), 1);

            let tx = TxId::new(db.next_tx.fetch_add(1, Ordering::SeqCst));
            db.storage
                .put(tx, &user_key, &data)
                .await
                .expect("put user"); // expect
            db.storage.put(tx, &doc_key, &data).await.expect("put doc"); // expect

            // Insert into HNSW directly as well
            col.index
                .insert(tx, doc_id, &stored.embedding)
                .await
                .expect("insert hnsw"); // expect
            col.index.commit(tx).await.expect("commit index"); // expect

            // Manually write a Pending intent
            let intent_key = col.namespaced_key(tx.inner().to_le_bytes().as_ref(), 3);
            let intent = crate::transaction::CommitIntent::Pending {
                doc_ids: vec![doc_id],
                has_text: false,
                has_graph: false,
            };
            let intent_bytes = serde_json::to_vec(&intent).expect("serialize intent"); // expect
            db.storage
                .put(tx, &intent_key, &intent_bytes)
                .await
                .expect("put intent"); // expect

            db.storage.commit(tx).await.expect("commit storage"); // expect

            db.close().await.expect("close"); // expect
        }

        // 2. Re-open: repair_on_open triggers. Since vector is already in index or re-inserted idempotently, it must succeed.
        {
            let db = MemFuse::open_with_config(&path, config)
                .await
                .expect("open 2 (repair idempotent)"); // expect
            let col = db.collection("idempotent-test").await.expect("col"); // expect

            let results = col.search(&[1.0, 0.0, 0.0, 0.0], 1).await.expect("search"); // expect
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].id, "already-indexed-doc");
        }
    }

    #[tokio::test]
    async fn test_repair_on_open_failure_propagates_error() {
        let tmp = tempfile::TempDir::new().expect("temp dir"); // expect
        let path = tmp.path().to_path_buf();
        let config = MemFuseConfig {
            dimension: 4,
            ..Default::default()
        };

        {
            let db = MemFuse::open_with_config(&path, config.clone())
                .await
                .expect("open 1"); // expect
            let col = db.collection("corrupt-test").await.expect("col"); // expect

            // Create a pending intent, user_key (key_type=0) with dim mismatch, and doc_key (key_type=1)
            let doc_id = DocId::from_key("corrupt-doc").expect("doc_id"); // expect
            let stored = crate::collection::StoredDocument {
                id: "corrupt-doc".to_string(),
                embedding: vec![1.0, 0.0], // dim mismatch (2 instead of 4)
                metadata: None,
            };
            let data = serde_json::to_vec(&stored).expect("json"); // expect
            let meta_only = crate::collection::StoredDocumentMeta::from(&stored);
            let meta_data = serde_json::to_vec(&meta_only).expect("meta json"); // expect

            let user_key = col.namespaced_key(b"corrupt-doc", 0);
            let doc_key = col.namespaced_key(&doc_id.inner().to_le_bytes(), 1);
            let tx = TxId::new(db.next_tx.fetch_add(1, Ordering::SeqCst));

            db.storage
                .put(tx, &user_key, &data)
                .await
                .expect("put user_key"); // expect
            db.storage
                .put(tx, &doc_key, &meta_data)
                .await
                .expect("put doc_key"); // expect

            // Write pending intent (key_type=3) referencing doc_id
            let intent_key = col.namespaced_key(tx.inner().to_le_bytes().as_ref(), 3);
            let intent = crate::transaction::CommitIntent::Pending {
                doc_ids: vec![doc_id],
                has_text: false,
                has_graph: false,
            };
            let intent_bytes = serde_json::to_vec(&intent).expect("intent json"); // expect
            db.storage
                .put(tx, &intent_key, &intent_bytes)
                .await
                .expect("put intent"); // expect

            db.storage.commit(tx).await.expect("commit"); // expect

            db.close().await.expect("close"); // expect
        }

        // Re-open with database: repair_on_open will invoke col.repair() which fails on dimension mismatch
        let res = MemFuse::open_with_config(&path, config).await;
        assert!(
            res.is_err(),
            "open_with_config should fail when repair fails"
        );

        if let Err(e) = res {
            let err_msg = e.to_string();
            assert!(
                err_msg.contains("repair_on_open")
                    && err_msg.contains("Datenbankintegrität nicht garantiert"),
                "Expected repair error message, got: {}",
                err_msg
            );
        }
    }

    #[tokio::test]
    async fn test_open_dimension_mismatch_fails() -> Result<()> {
        let dir = tempfile::tempdir()
            .map_err(|e| memfuse_core::MemFuseError::InvalidInput(e.to_string()))?;
        let config_768 = MemFuseConfig {
            dimension: 768,
            ..Default::default()
        };
        let _db = MemFuse::open_with_config(dir.path(), config_768).await?;

        // Zweites Öffnen mit falscher Dimension muss früh fehlschlagen
        let config_1536 = MemFuseConfig {
            dimension: 1536,
            ..Default::default()
        };
        let result = MemFuse::open_with_config(dir.path(), config_1536).await;
        assert!(result.is_err());
        let err_msg = match result {
            Err(e) => e.to_string(),
            Ok(_) => panic!("expected error"),
        };
        assert!(err_msg.contains("Dimension mismatch"));
        Ok(())
    }

    #[test]
    fn test_provenance_record_serialization_roundtrip() {
        let p = ProvenanceRecord {
            vector_distance: Some(0.42),
            bm25_score: Some(1.23),
            graph_score: None,
            rerank_score: None,
            signal_ranks: {
                let mut m = std::collections::HashMap::new();
                m.insert("vector".to_string(), 1u32);
                m.insert("bm25".to_string(), 3u32);
                m
            },
            source_collection: Some("test_col".to_string()),
            index_type: Some("hnsw".to_string()),
            signal_contributions: std::collections::HashMap::new(),
        };
        let json = serde_json::to_string(&p).expect("serialize");
        let back: ProvenanceRecord = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.vector_distance, Some(0.42));
        assert_eq!(back.bm25_score, Some(1.23));
        assert_eq!(back.graph_score, None);
        assert_eq!(back.source_collection.as_deref(), Some("test_col"));
    }

    #[test]
    fn test_search_result_with_provenance_serialization() {
        let sr = SearchResult {
            id: "doc1".to_string(),
            score: 0.9,
            metadata: None,
            matched_signals: vec!["vector".to_string()],
            provenance: Some(ProvenanceRecord {
                source_collection: Some("my_col".to_string()),
                ..Default::default()
            }),
        };
        let json = serde_json::to_string(&sr).expect("serialize");
        assert!(json.contains("provenance"));
        assert!(json.contains("my_col"));
    }

    #[test]
    fn test_search_result_without_provenance_serialization_omits_field() {
        let sr = SearchResult {
            id: "doc2".to_string(),
            score: 0.5,
            metadata: None,
            matched_signals: vec![],
            provenance: None,
        };
        let json = serde_json::to_string(&sr).expect("serialize");
        // provenance: None → Feld soll komplett fehlen (skip_serializing_if)
        assert!(
            !json.contains("provenance"),
            "provenance=None soll im JSON weggelassen werden: {json}"
        );
    }

    #[tokio::test]
    async fn test_provenance_rrf_sum_invariant() {
        let (db, _tmp) = test_db(4).await;
        let col = db.collection("prov_test").await.expect("collection");

        for i in 0..10 {
            let id = format!("doc-{}", i);
            let val = (i as f32) / 10.0;
            col.insert(
                &id,
                &[val, 1.0 - val, 0.0, 0.0],
                Some(json!({ "text": format!("rust memory system {}", i) })),
            )
            .await
            .expect("insert");
        }

        let results = col
            .query()
            .text("rust memory")
            .embedding([0.5, 0.5, 0.0, 0.0])
            .include_provenance(true)
            .k(5)
            .execute()
            .await
            .expect("search");

        assert!(!results.is_empty(), "Results must not be empty");

        for result in &results {
            let prov = result
                .provenance
                .as_ref()
                .expect("Provenance must be present when include_provenance=true");
            let sum_contrib: f32 = prov
                .signal_contributions
                .values()
                .map(|c| c.rrf_contribution)
                .sum();
            assert!(
                (sum_contrib - result.score).abs() < 1e-6,
                "INV-PROV-1 verletzt: Summe der Signal-Beiträge ({}) ≠ RRF-Score ({})",
                sum_contrib,
                result.score
            );
        }
    }

    #[tokio::test]
    async fn test_search_results_have_provenance_is_some() {
        let (db, _tmp) = test_db(4).await;
        let col = db.collection("search_prov_test").await.expect("collection");

        col.insert(
            "doc-1",
            &[1.0, 0.0, 0.0, 0.0],
            Some(json!({"text": "rust search provenance test"})),
        )
        .await
        .expect("insert");

        let vec_results = col
            .search(&[1.0, 0.0, 0.0, 0.0], 1)
            .await
            .expect("vector search");
        assert_eq!(vec_results.len(), 1);
        assert!(
            vec_results[0].provenance.is_some(),
            "Vector search results must contain provenance record"
        );
        let prov = vec_results[0].provenance.as_ref().expect("provenance");
        assert_eq!(prov.source_collection.as_deref(), Some("search_prov_test"));
        assert_eq!(prov.index_type.as_deref(), Some("hnsw"));

        let text_results = col
            .query()
            .text("rust search")
            .embedding([1.0, 0.0, 0.0, 0.0])
            .include_provenance(true)
            .k(1)
            .execute()
            .await
            .expect("text search");
        assert_eq!(text_results.len(), 1);
        assert!(
            text_results[0].provenance.is_some(),
            "Text search results must contain provenance record"
        );
    }
}

#[cfg(all(test, feature = "sandbox"))]
mod dyn_safety {
    use super::*;

    fn _assert_dyn_sandbox_bridge(_: Option<&dyn SandboxBridge>) {}

    #[test]
    fn test_sandbox_bridge_dyn_safety() {
        _assert_dyn_sandbox_bridge(None);
    }
}
