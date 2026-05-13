// ANCHOR:ARCH:DB-001 — Orchestrator Facade (Getriebe — Layer 2).
// WP:WP-0.0 PRIO:1 NEEDS:NONE
// AGENT:01 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-05 DEADLINE:NONE
// ROLLE: Verbindet memfuse-core (Traits), memfuse-store (LSM) und memfuse-index (HNSW).
// DESIGN: Zero-Boilerplate API für Nutzer. Intern wird alles über die Collection-Abstraktion geroutet.
// ABWÄRTSKOMPATIBILITÄT: Bietet weiterhin top-level insert/search an, die intern auf die "default" Collection leiten.
//! # MemFuse — Embedded Hybrid-Search for AI Agents
//!
//! MemFuse is a zero-boilerplate embedded database for AI agent memory.
//! It combines vector search (HNSW), persistent storage (LSM-Tree),
//! and relationship tracking in a single library.
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
//! // Semantic search
//! let results = db.search(&[0.1, 0.2, 0.3, 0.4], 5).await?;
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

use memfuse_core::{DocId, Result, StorageEngine, TxId};
use memfuse_index::{HnswConfig, HnswIndex};
use memfuse_store::LsmStorage;
use serde_json::Value;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub mod collection;
pub mod fusion;
pub mod transaction;

pub use collection::Collection;

/// User-facing search result.
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// The string ID provided during insert.
    pub id: String,
    /// Similarity score (higher = more similar).
    pub score: f32,
    /// Metadata associated with the document (if any).
    pub metadata: Option<Value>,
}

/// Overall database statistics.
#[derive(Debug, Clone)]
pub struct DbStats {
    /// Statistics for the vector index.
    pub index_stats: memfuse_core::VectorIndexStats,
    /// Statistics for the LSM storage engine.
    pub storage_stats: memfuse_core::StorageStats,
}

/// User-facing document retrieved by key.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Document {
    /// The string ID.
    pub id: String,
    /// Metadata associated with the document.
    pub metadata: Option<Value>,
}

/// Configuration for MemFuse.
#[derive(Debug, Clone)]
pub struct MemFuseConfig {
    /// Vector dimensionality (must match your embeddings).
    pub dimension: usize,
    /// Maximum number of vectors to store.
    pub max_elements: usize,
    /// Distance metric for vector comparison.
    pub distance_metric: memfuse_core::DistanceMetric,
}

impl Default for MemFuseConfig {
    fn default() -> Self {
        Self {
            dimension: 1536,
            max_elements: 1_000_000,
            distance_metric: memfuse_core::DistanceMetric::Cosine,
        }
    }
}

/// MemFuse — Embedded hybrid-search database for AI agents.
///
/// This is the primary entry point for all operations. It provides
/// a simple, zero-boilerplate API on top of a LSM-Tree storage engine
/// and HNSW vector index.
pub struct MemFuse {
    storage: Arc<LsmStorage>,
    next_tx: Arc<AtomicU64>,
    dimension: usize,
    collections: tokio::sync::RwLock<std::collections::HashMap<String, Collection>>,
}

impl MemFuse {
    /// Opens or creates a MemFuse database at the given path.
    ///
    /// Uses default configuration (1536 dimensions, cosine distance).
    /// For custom settings, use [`MemFuse::open_with_config`].
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_with_config(path, MemFuseConfig::default()).await
    }

    /// Opens or creates a MemFuse database with custom configuration.
    pub async fn open_with_config(path: impl AsRef<Path>, config: MemFuseConfig) -> Result<Self> {
        let lsm_config = memfuse_store::LsmConfig {
            path: path.as_ref().to_path_buf(),
            ..Default::default()
        };

        let storage = Arc::new(LsmStorage::new(lsm_config).await?);
        let next_tx = Arc::new(AtomicU64::new(1));

        let db = Self {
            storage,
            next_tx,
            dimension: config.dimension,
            collections: tokio::sync::RwLock::new(std::collections::HashMap::new()),
        };

        // Initialize already existing collections from storage
        db.initialize_collections().await?;

        // Initialize the default collection backwards compatibility
        let _ = db.collection("default").await?;

        Ok(db)
    }

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

    /// Returns the next transaction ID (auto-incremented).
    /// Returns a specific collection (namespace).
    /// Creates the collection if it does not already exist.
    // ANCHOR:TODO:COL-001 — Implementiere vollständige Persistenz und Isolation für `collection()`.
    // WP:WP-1.2 PRIO:1 NEEDS:NONE
    // AGENT:@JULES-04 DATE:2026-05-09 STATUS:DONE
    // TEST: cargo test -p memfuse-db test_collections_are_isolated
    // DONE: `collection()` ist wal-gesichert, Isolation ist korrekt.
    // SUCCESSOR: @JULES-04 — "Mach weiter mit COL-002 und COL-003, bis Collections-Modul fully featured ist."
    pub async fn collection(&self, name: &str) -> Result<Collection> {
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
            return Ok(col.clone());
        }
        drop(read_guard);

        let mut write_guard = self.collections.write().await;
        if let Some(col) = write_guard.get(name) {
            return Ok(col.clone());
        }

        let hnsw_config = HnswConfig {
            dimension: self.dimension,
            ..Default::default()
        };
        let index = Arc::new(HnswIndex::new(hnsw_config));

        let col = Collection::new(
            name.to_string(),
            Arc::clone(&self.storage),
            index,
            Arc::clone(&self.next_tx),
            self.dimension,
        );

        // Register in storage if not default
        if name != "default" {
            let col_idx_key = [b"__col_idx:\x00", name.as_bytes()].concat();
            let tx = TxId::new(self.next_tx.fetch_add(1, Ordering::SeqCst));
            self.storage.put(tx, &col_idx_key, b"{}").await?;
            self.storage.commit(tx).await?;
        }

        // Load existing data into HNSW
        col.load_index().await?;

        write_guard.insert(name.to_string(), col.clone());

        Ok(col)
    }

    /// Lists all existing collection names (including those persisted in storage).
    // ANCHOR:TODO:COL-002 — Erweitere `list_collections` so, dass es aus dem LSM-Store/Metadata ließt.
    // WP:WP-1.2 PRIO:1 NEEDS:COL-001
    // AGENT:@JULES-04 DATE:2026-05-09 STATUS:DONE
    // TEST: cargo test -p memfuse-db test_list_collections
    // DONE: list_collections gibt persistierte Collections zurück.
    // SUCCESSOR: @JULES-04 — "Mache weiter mit COL-003."
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
    // ANCHOR:TODO:COL-003 — Löschen der Collection-Keys aus LSM und des HNSW Graphen.
    // WP:WP-1.2 PRIO:1 NEEDS:COL-001
    // AGENT:@JULES-04 DATE:2026-05-09 STATUS:DONE
    // TEST: cargo test -p memfuse-db test_drop_removes_all_data
    // DONE: Alle Daten getilgt, re-öffnen führt zu leerer DB.
    // SUCCESSOR: @JULES-05 — "Collections sind fertig. Beginne mit WP-2.1 SEARCH-001."
    pub async fn drop_collection(&self, name: &str) -> Result<()> {
        if name == "default" {
            return Err(memfuse_core::MemFuseError::invalid_input(
                "Cannot drop default collection",
            ));
        }

        let mut guard = self.collections.write().await;
        if let Some(col) = guard.remove(name) {
            col.drop_collection().await?;

            // Remove from index
            let col_idx_key = [b"__col_idx:\x00", name.as_bytes()].concat();
            let tx = TxId::new(self.next_tx.fetch_add(1, Ordering::SeqCst));
            self.storage.delete(tx, &col_idx_key).await?;
            self.storage.commit(tx).await?;
        }
        Ok(())
    }

    // --- Legacy Backwards Compatibility Methods (Wraps "default" collection) ---

    async fn default_col(&self) -> Result<Collection> {
        self.collection("default").await
    }

    /// Inserts a document with an embedding and optional metadata.
    pub async fn insert(&self, id: &str, embedding: &[f32], metadata: Option<Value>) -> Result<()> {
        self.default_col()
            .await?
            .insert(id, embedding, metadata)
            .await
    }

    /// Retrieves a document by its string key.
    pub async fn get(&self, id: &str) -> Result<Option<Document>> {
        self.default_col().await?.get(id).await
    }

    /// Updates a document's embedding and/or metadata.
    pub async fn update(&self, id: &str, embedding: &[f32], metadata: Option<Value>) -> Result<()> {
        self.default_col()
            .await?
            .update(id, embedding, metadata)
            .await
    }

    /// Performs semantic k-NN search over stored embeddings.
    pub async fn search(&self, query: &[f32], k: usize) -> Result<Vec<SearchResult>> {
        self.default_col().await?.search(query, k).await
    }

    /// Performs semantic k-NN search with an optional filter function over documents.
    pub async fn search_filtered(
        &self,
        query: &[f32],
        k: usize,
        filter: Option<&(dyn Fn(DocId) -> bool + Send + Sync)>,
    ) -> Result<Vec<SearchResult>> {
        self.default_col()
            .await?
            .search_filtered(query, k, filter)
            .await
    }

    // ANCHOR:TODO:SEARCH-001 — Implementiere `hybrid_search(text, vector, k)` die delegiert an Collection.
    // WP:WP-2.1 PRIO:1 NEEDS:COL-001
    // AGENT:@JULES-05 DATE:2026-05-09 STATUS:DONE
    // TEST: cargo test -p memfuse-db test_bm25_ranks_exact_keyword_higher
    // DONE: Funktion existiert und delegiert richtig.
    // SUCCESSOR: @JULES-06 — "Hybrid Search Facade ist ready. Python Bindings (SEARCH-STABLE) können gebaut werden."
    // ANCHOR:FIXME:DB-002 — Build Failure resolved by Watchdog
    // WP:WP-0.0 PRIO:1 NEEDS:NONE
    // AGENT:@JULES-04 DATE:2026-05-13 STATUS:DONE
    // WATCHDOG: Deduplicated hybrid_search to restore Triple-Test-Gate health.
    /// Performs hybrid search combining BM25 and vector search.
    pub async fn hybrid_search(
        &self,
        text: &str,
        vector: &[f32],
        k: usize,
    ) -> Result<Vec<SearchResult>> {
        self.default_col()
            .await?
            .hybrid_search(text, vector, k)
            .await
    }

    /// Deletes a document by its string ID.
    pub async fn delete(&self, id: &str) -> Result<()> {
        self.default_col().await?.delete(id).await
    }

    /// Creates a bidirectional relationship between two documents.
    pub async fn relate(&self, from: &str, to: &str, label: &str) -> Result<()> {
        self.default_col().await?.relate(from, to, label).await?;
        self.default_col().await?.relate(to, from, label).await?;
        Ok(())
    }

    /// Scans storage for key-value pairs matching a prefix.
    pub async fn scan_prefix(&self, prefix: &str) -> Result<Vec<(String, Value)>> {
        self.default_col().await?.scan_prefix(prefix).await
    }

    /// Returns the number of vectors in the index.
    pub async fn len(&self) -> Result<usize> {
        Ok(self.default_col().await?.len().await)
    }

    /// Returns true if the database is empty.
    pub async fn is_empty(&self) -> Result<bool> {
        Ok(self.default_col().await?.is_empty().await)
    }

    /// Scans a range of keys, returning key-value pairs.
    pub async fn scan(
        &self,
        start: std::ops::Bound<&[u8]>,
        end: std::ops::Bound<&[u8]>,
    ) -> Result<Vec<(String, Value)>> {
        self.default_col().await?.scan(start, end).await
    }

    /// Returns combined statistics for the vector index and storage engine.
    pub async fn stats(&self) -> Result<DbStats> {
        Ok(DbStats {
            index_stats: self.default_col().await?.stats().await?,
            storage_stats: self.storage.stats().await?,
        })
    }
}

// Re-export for convenience
pub use memfuse_core::DistanceMetric;
pub use serde_json::json;

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn test_db(dim: usize) -> (MemFuse, TempDir) {
        let tmp = TempDir::new().expect("temp dir");
        let config = MemFuseConfig {
            dimension: dim,
            max_elements: 10_000,
            distance_metric: DistanceMetric::Cosine,
        };
        let db = MemFuse::open_with_config(tmp.path(), config)
            .await
            .expect("open db");
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
        .expect("insert");

        db.insert(
            "doc-2",
            &[0.0, 1.0, 0.0, 0.0],
            Some(json!({"topic": "python"})),
        )
        .await
        .expect("insert");

        db.insert("doc-3", &[0.9, 0.1, 0.0, 0.0], None)
            .await
            .expect("insert");

        let results = db.search(&[1.0, 0.0, 0.0, 0.0], 2).await.expect("search");
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
        .expect("insert");

        let results = db.search(&[1.0, 0.0, 0.0, 0.0], 1).await.expect("search");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "doc-1");
        let meta = results[0].metadata.as_ref().expect("metadata should exist");
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
        .expect("insert");

        let doc = db.get("doc-1").await.expect("get").expect("should exist");
        assert_eq!(doc.id, "doc-1");
        assert_eq!(doc.metadata.expect("valid")["topic"], "rust");

        let none = db.get("nonexistent").await.expect("get");
        assert!(none.is_none());
    }

    #[tokio::test]
    async fn test_update() {
        let (db, _tmp) = test_db(4).await;

        db.insert("doc-1", &[1.0, 0.0, 0.0, 0.0], Some(json!({"v": 1})))
            .await
            .expect("insert");

        db.update("doc-1", &[0.0, 1.0, 0.0, 0.0], Some(json!({"v": 2})))
            .await
            .expect("update");

        // Metadata should be updated
        let doc = db.get("doc-1").await.expect("get").expect("exists");
        assert_eq!(doc.metadata.expect("valid")["v"], 2);

        // Vector should be updated — search for new vector should find it
        let results = db.search(&[0.0, 1.0, 0.0, 0.0], 1).await.expect("search");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "doc-1");
    }

    #[tokio::test]
    async fn test_delete() {
        let (db, _tmp) = test_db(4).await;

        db.insert("doc-1", &[1.0, 0.0, 0.0, 0.0], None)
            .await
            .expect("insert");
        assert_eq!(db.len().await.expect("len"), 1);

        db.delete("doc-1").await.expect("delete");
        assert_eq!(db.len().await.expect("len"), 0);

        // get should return None after delete
        let doc = db.get("doc-1").await.expect("get");
        assert!(doc.is_none());
    }

    #[tokio::test]
    async fn test_relate() {
        let (db, _tmp) = test_db(4).await;

        db.insert("doc-1", &[1.0, 0.0, 0.0, 0.0], None)
            .await
            .expect("insert");
        db.insert("doc-2", &[0.0, 1.0, 0.0, 0.0], None)
            .await
            .expect("insert");

        // Should not error
        db.relate("doc-1", "doc-2", "references")
            .await
            .expect("relate");
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
        let results = db.search(&[1.0, 0.0, 0.0, 0.0], 5).await.expect("search");
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_relate_and_scan_prefix() {
        let (db, _tmp) = test_db(4).await;

        db.insert("doc-1", &[1.0, 0.0, 0.0, 0.0], None)
            .await
            .expect("insert");
        db.insert("doc-2", &[0.0, 1.0, 0.0, 0.0], None)
            .await
            .expect("insert");
        db.insert("doc-3", &[0.0, 0.0, 1.0, 0.0], None)
            .await
            .expect("insert");

        db.relate("doc-1", "doc-2", "references")
            .await
            .expect("relate");
        db.relate("doc-1", "doc-3", "references")
            .await
            .expect("relate");

        // Scan for relations of doc-1
        let results = db
            .scan_prefix("__rel:doc-1:references:")
            .await
            .expect("scan");
        assert_eq!(results.len(), 2);

        let related_ids: Vec<String> = results
            .into_iter()
            .map(|(_, v)| v["to"].as_str().expect("valid").to_string())
            .collect();
        assert!(related_ids.contains(&"doc-2".to_string()));
        assert!(related_ids.contains(&"doc-3".to_string()));

        // Check backward edge setup automatically
        let backward_results = db
            .scan_prefix("__rel:doc-2:references:")
            .await
            .expect("scan bwd");
        assert_eq!(backward_results.len(), 1);
        assert_eq!(backward_results[0].1["to"], "doc-1");
    }

    #[tokio::test]
    async fn test_stats_aggregation() {
        let (db, _tmp) = test_db(4).await;

        db.insert("doc-1", &[1.0, 0.0, 0.0, 0.0], None)
            .await
            .expect("insert");

        let stats = db.stats().await.expect("stats");
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
        .expect("insert agent");
        db.insert(
            "task-1",
            &[0.9, 0.6, 0.0, 0.0],
            Some(json!({"type": "task"})),
        )
        .await
        .expect("insert task");
        db.insert(
            "task-2",
            &[0.0, 0.0, 1.0, 0.5],
            Some(json!({"type": "task"})),
        )
        .await
        .expect("insert task 2");

        // 2. Relate
        db.relate("agent-1", "task-1", "assigned_to")
            .await
            .expect("relate 1");
        db.relate("agent-1", "task-2", "assigned_to")
            .await
            .expect("relate 2");

        // 3. Search
        let results = db.search(&[1.0, 0.5, 0.0, 0.0], 2).await.expect("search");
        assert_eq!(results[0].id, "agent-1"); // Exactly matches
        assert_eq!(results[1].id, "task-1"); // Close match

        // 4. Update
        db.update(
            "task-1",
            &[0.1, 0.1, 0.9, 0.9],
            Some(json!({"type": "task", "status": "done"})),
        )
        .await
        .expect("update task");

        // 5. Scan prefix
        let edges = db
            .scan_prefix("__rel:agent-1:assigned_to:")
            .await
            .expect("scan");
        assert_eq!(edges.len(), 2);

        // 6. Delete
        db.delete("agent-1").await.expect("delete");

        // 7. Verify empty search and missing doc
        let get_agent = db.get("agent-1").await.expect("get");
        assert!(get_agent.is_none());
        assert_eq!(db.len().await.expect("len"), 2); // 3 inserted, 1 deleted
    }

    #[tokio::test]
    async fn test_collections_are_isolated() {
        let (db, _tmp) = test_db(4).await;
        let col_a = db.collection("a").await.expect("col a");
        let col_b = db.collection("b").await.expect("col b");

        col_a
            .insert("k1", &[1.0, 0.0, 0.0, 0.0], Some(json!({"val": "a"})))
            .await
            .expect("ins a");
        col_b
            .insert("k1", &[0.0, 1.0, 0.0, 0.0], Some(json!({"val": "b"})))
            .await
            .expect("ins b");

        let res_a = col_a.get("k1").await.expect("get a").expect("exists");
        let res_b = col_b.get("k1").await.expect("get b").expect("exists");

        assert_eq!(res_a.metadata.expect("test")["val"], "a");
        assert_eq!(res_b.metadata.expect("test")["val"], "b");

        let search_a = col_a
            .search(&[1.0, 0.0, 0.0, 0.0], 1)
            .await
            .expect("search a");
        assert_eq!(search_a.len(), 1);
        assert_eq!(search_a[0].id, "k1");
        assert_eq!(search_a[0].metadata.as_ref().expect("test")["val"], "a");
    }

    #[tokio::test]
    async fn test_drop_removes_all_data() {
        let (db, _tmp) = test_db(4).await;
        let col = db.collection("drop-me").await.expect("col");
        col.insert("k1", &[1.0, 0.0, 0.0, 0.0], None)
            .await
            .expect("ins");

        db.drop_collection("drop-me").await.expect("drop");

        let col2 = db.collection("drop-me").await.expect("re-create");
        assert_eq!(col2.len().await, 0);
        assert!(col2.get("k1").await.expect("get").is_none());
    }

    #[tokio::test]
    async fn test_default_collection_compat() {
        let (db, _tmp) = test_db(4).await;
        db.insert("k", &[1.0, 0.0, 0.0, 0.0], Some(json!({"v": 1})))
            .await
            .expect("ins");

        let doc = db.get("k").await.expect("get").expect("exists");
        assert_eq!(doc.id, "k");

        let results = db.search(&[1.0, 0.0, 0.0, 0.0], 1).await.expect("search");
        assert_eq!(results[0].id, "k");
    }

    #[tokio::test]
    async fn test_list_collections() {
        let (db, _tmp) = test_db(4).await;
        db.collection("c1").await.expect("c1");
        db.collection("c2").await.expect("c2");
        db.collection("c3").await.expect("c3");

        let list = db.list_collections().await.expect("list");
        assert!(list.contains(&"default".to_string()));
        assert!(list.contains(&"c1".to_string()));
        assert!(list.contains(&"c2".to_string()));
        assert!(list.contains(&"c3".to_string()));
        assert_eq!(list.len(), 4);
    }
}
