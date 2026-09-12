#![cfg(feature = "replicator-dynamics-weights")]
// FILE-CONTEXT: Cross-domain chaos matrix integration tests combining Candle ML inference, storage fault injection, fusion aborts, and checkpoint pinning.
//! Cross-Domain Chaos Matrix Integration Tests.
//!
//! Evaluates systemic resiliency across Candle embedding inference, LsmStorage disk I/O faults,
//! ReplicatorState weighted fusion cancellation, and concurrent ConfigFingerprint model switches under pinned checkpoints.

use memfuse_calibration::ReplicatorState;
use memfuse_core::traits::embedding::EmbeddingError;
use memfuse_core::traits::{BoxFuture, EmbeddingProvider, TextEmbeddingEngine};
use memfuse_core::{ConfigFingerprint, DocId, StorageEngine, VectorIndex};
use memfuse_db::collection::Collection;
use memfuse_db::fusion::{weighted_reciprocal_rank_fusion_with_options, MetadataMergePriority};
use memfuse_graph::csr::CsrGraph;
use memfuse_index::{HnswConfig, HnswIndex};
use memfuse_store::lsm::{LsmConfig, LsmStorage};
use memfuse_text::Language;

use std::collections::HashSet;
use std::os::unix::fs::PermissionsExt;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

/// Lightweight deterministic Xorshift PRNG for chaos tests without external dependency assumptions.
struct SimpleRng(u64);

impl SimpleRng {
    fn seed_from_u64(seed: u64) -> Self {
        Self(if seed == 0 { 0x9E3779B97F4A7C15 } else { seed })
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn gen_range_u64(&mut self, min: u64, max: u64) -> u64 {
        if min >= max {
            return min;
        }
        min + (self.next_u64() % (max - min))
    }

    fn gen_range_f32(&mut self, min: f32, max: f32) -> f32 {
        let frac = (self.next_u64() as f64) / (u64::MAX as f64);
        (min as f64 + frac * ((max - min) as f64)) as f32
    }
}

/// Resolves the test seed from `CHAOS_SEED` environment variable or generates a new random seed from system time.
/// Logs and prints the seed to ensure reproducibility of any test failure.
fn resolve_and_log_seed() -> u64 {
    let seed: u64 = std::env::var("CHAOS_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            use std::time::{SystemTime, UNIX_EPOCH};
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(123456789)
        });

    println!("CHAOS_SEED={}", seed);
    tracing::info!("CHAOS_SEED={}", seed);
    seed
}

/// Simulated embedding provider producing deterministic embeddings with realistic latency.
#[derive(Clone)]
struct MockCandleEmbedder {
    dim: usize,
    latency: Duration,
}

impl MockCandleEmbedder {
    fn new(dim: usize, latency: Duration) -> Self {
        Self { dim, latency }
    }
}

impl EmbeddingProvider for MockCandleEmbedder {
    fn provider_name(&self) -> &str {
        "candle-mock"
    }

    fn embedding_dim(&self) -> usize {
        self.dim
    }

    fn embed<'a>(
        &'a self,
        text: &'a str,
    ) -> BoxFuture<'a, std::result::Result<Vec<f32>, EmbeddingError>> {
        let dim = self.dim;
        let latency = self.latency;
        let text_owned = text.to_string();

        Box::pin(async move {
            if latency > Duration::ZERO {
                tokio::time::sleep(latency).await;
            }
            let mut vec = vec![0.0f32; dim];
            for (i, byte) in text_owned.bytes().enumerate() {
                vec[i % dim] += byte as f32 / 255.0;
            }
            // Normalize vector so it's non-zero
            let norm: f32 = vec.iter().map(|v| v * v).sum::<f32>().sqrt();
            if norm > 0.0 {
                for v in vec.iter_mut() {
                    *v /= norm;
                }
            } else {
                vec[0] = 1.0;
            }
            Ok(vec)
        })
    }

    fn embed_batch<'a>(
        &'a self,
        texts: &'a [&'a str],
    ) -> BoxFuture<'a, std::result::Result<Vec<Vec<f32>>, EmbeddingError>> {
        let dim = self.dim;
        let latency = self.latency;
        let texts_owned: Vec<String> = texts.iter().map(|s| s.to_string()).collect();

        Box::pin(async move {
            if latency > Duration::ZERO {
                tokio::time::sleep(latency).await;
            }
            let mut results = Vec::with_capacity(texts_owned.len());
            for text in texts_owned {
                let mut vec = vec![0.0f32; dim];
                for (i, byte) in text.bytes().enumerate() {
                    vec[i % dim] += byte as f32 / 255.0;
                }
                let norm: f32 = vec.iter().map(|v| v * v).sum::<f32>().sqrt();
                if norm > 0.0 {
                    for v in vec.iter_mut() {
                        *v /= norm;
                    }
                } else {
                    vec[0] = 1.0;
                }
                results.push(vec);
            }
            Ok(results)
        })
    }
}

/// Independent ground truth tracker recording insert attempts vs completed commits.
#[derive(Clone, Default)]
struct GroundTruth {
    attempted: Arc<parking_lot::Mutex<HashSet<String>>>,
    committed: Arc<parking_lot::Mutex<HashSet<String>>>,
}

impl GroundTruth {
    fn new() -> Self {
        Self {
            attempted: Arc::new(parking_lot::Mutex::new(HashSet::new())),
            committed: Arc::new(parking_lot::Mutex::new(HashSet::new())),
        }
    }

    fn record_attempt(&self, doc_id: &str) {
        self.attempted.lock().insert(doc_id.to_string());
    }

    fn record_commit(&self, doc_id: &str) {
        self.committed.lock().insert(doc_id.to_string());
    }
}

/// Scenario A: GPU/Inference busy + ENOSPC disk full simulation does not leave orphaned embeddings.
#[tokio::test]
#[ignore]
async fn chaos_gpu_busy_disk_full_no_orphaned_embedding() {
    let seed = resolve_and_log_seed();
    let mut rng = SimpleRng::seed_from_u64(seed);

    let tmp_dir = TempDir::new().expect("temp dir");
    let db_path = tmp_dir.path().to_path_buf();

    let config = LsmConfig {
        path: db_path.clone(),
        memtable_size_limit: 8 * 1024,
        ..Default::default()
    };

    let storage = Arc::new(LsmStorage::new(config).await.expect("storage init"));
    let index = Arc::new(
        HnswIndex::try_new(HnswConfig {
            dimension: 16,
            ..Default::default()
        })
        .expect("hnsw init"),
    );
    let graph_index = Arc::new(CsrGraph::new());

    let embedder = MockCandleEmbedder::new(16, Duration::from_millis(5));
    let collection = Arc::new(Collection::new(
        "chaos_col".to_string(),
        Arc::clone(&storage),
        Arc::clone(&index),
        graph_index,
        Arc::new(AtomicU64::new(1)),
        16,
        Language::English,
    ));
    collection
        .set_embedder(Arc::new(embedder.clone()) as Arc<dyn TextEmbeddingEngine>)
        .await
        .expect("set embedder");

    let ground_truth = GroundTruth::new();
    let num_items = rng.gen_range_u64(15, 30) as usize;

    // Phase 1: Concurrently process batch Candle embeddings while simulating disk ENOSPC via read-only chmod
    let mut tasks = Vec::new();

    for i in 0..num_items {
        let col = Arc::clone(&collection);
        let gt = ground_truth.clone();
        let embedder_clone = embedder.clone();
        let doc_id = format!("doc_{:03}", i);
        let text = format!("Sample document content text for item {:03}", i);

        // Inject ENOSPC (chmod 0o444) halfway through insertions
        if i == num_items / 2 {
            let _ = std::fs::set_permissions(&db_path, std::fs::Permissions::from_mode(0o444));
        }

        let task = tokio::spawn(async move {
            gt.record_attempt(&doc_id);
            let vec_res = EmbeddingProvider::embed(&embedder_clone, &text).await;
            if let Ok(vec) = vec_res {
                let doc = serde_json::json!({ "id": doc_id, "text": text });
                let insert_res = col.insert(&doc_id, &vec, Some(doc)).await;
                if insert_res.is_ok() {
                    gt.record_commit(&doc_id);
                }
            }
        });
        tasks.push(task);
    }

    for t in tasks {
        let _ = t.await;
    }

    // Restore permissions for inspection
    let _ = std::fs::set_permissions(&db_path, std::fs::Permissions::from_mode(0o755));

    // Phase 2: Verify Ground Truth and absolute Absence of Orphaned Embeddings (JULES-P3)
    let attempted = ground_truth.attempted.lock().clone();
    let committed = ground_truth.committed.lock().clone();

    for doc_id in &attempted {
        let doc_id_num = DocId::from_key(doc_id).expect("doc_id parse");

        let user_key = collection.namespaced_key(doc_id.as_bytes(), 0);
        let doc_key = collection.namespaced_key(&doc_id_num.inner().to_le_bytes(), 1);

        let has_user_doc = storage.get(&user_key).await.expect("storage get").is_some();
        let has_meta_doc = storage.get(&doc_key).await.expect("storage get").is_some();
        let has_doc = has_user_doc && has_meta_doc;

        // Check if vector index contains vector or if vector search finds it
        let dummy_query = vec![0.1f32; 16];
        let search_res = VectorIndex::search_filtered(index.as_ref(), &dummy_query, 100, None)
            .await
            .expect("search");
        let has_vector = search_res.iter().any(|sd| sd.doc_id == doc_id_num);

        if committed.contains(doc_id) {
            // (a) Fully committed: MUST have BOTH document entry AND vector entry
            assert!(
                has_doc,
                "Committed document {} missing from storage",
                doc_id
            );
            assert!(
                has_vector,
                "Committed document {} missing from vector index",
                doc_id
            );
        } else {
            // (b) Not committed / Failed write: MUST NOT have orphaned vector or partial doc
            assert!(
                !has_vector,
                "Ground-truth violation: Orphaned vector found for uncommitted document {}",
                doc_id
            );
        }
    }
}

/// Scenario B: Tokio task cancellation during weighted fusion preserves ReplicatorState weight invariant.
#[tokio::test]
#[ignore]
async fn chaos_tokio_abort_during_weighted_fusion_preserves_weight_invariant() {
    let seed = resolve_and_log_seed();
    let mut rng = SimpleRng::seed_from_u64(seed);

    let state: Arc<parking_lot::RwLock<ReplicatorState>> =
        Arc::new(parking_lot::RwLock::new(ReplicatorState::new(
            vec![
                "vector".to_string(),
                "text".to_string(),
                "graph".to_string(),
            ],
            0.05,
        )));

    let state_clone: Arc<parking_lot::RwLock<ReplicatorState>> = Arc::clone(&state);

    // Spawn task performing continuous fusion updates
    let fusion_task = tokio::spawn(async move {
        let mut loop_rng = SimpleRng::seed_from_u64(seed.wrapping_add(1));
        loop {
            let r0 = loop_rng.gen_range_f32(0.0, 1.0);
            let r1 = loop_rng.gen_range_f32(0.0, 1.0);
            let r2 = loop_rng.gen_range_f32(0.0, 1.0);

            {
                let mut guard = state_clone.write();
                guard.update(&[r0, r1, r2]);
            }

            let weights = {
                let guard = state_clone.read();
                guard.weights.clone()
            };

            let vec_set = (
                "vector".to_string(),
                vec![memfuse_db::SearchResult {
                    id: "doc1".to_string(),
                    score: 0.9,
                    metadata: None,
                    matched_signals: vec!["vector".to_string()],
                    provenance: None,
                }],
                weights[0],
            );
            let text_set = (
                "text".to_string(),
                vec![memfuse_db::SearchResult {
                    id: "doc2".to_string(),
                    score: 0.8,
                    metadata: None,
                    matched_signals: vec!["text".to_string()],
                    provenance: None,
                }],
                weights[1],
            );

            let _fused = weighted_reciprocal_rank_fusion_with_options(
                vec![vec_set, text_set],
                10,
                MetadataMergePriority::default(),
                true,
                None,
            );

            tokio::task::yield_now().await;
        }
    });

    // Abort at a randomized point in time derived from CHAOS_SEED
    let abort_delay_micros = rng.gen_range_u64(500, 5000);
    tokio::time::sleep(Duration::from_micros(abort_delay_micros)).await;

    // Execute abrupt abort
    fusion_task.abort();
    let _ = fusion_task.await;

    // Immediately inspect state post-abort
    let guard = state.read();
    let sum: f32 = guard.weights.iter().sum();

    assert!(
        (sum - 1.0).abs() < 1e-5,
        "ReplicatorState sum invariant violated post-abort: sum={sum}, weights={:?}",
        guard.weights
    );

    for (idx, &w) in guard.weights.iter().enumerate() {
        assert!(
            w > 0.0 && w.is_finite(),
            "Weight at index {idx} is invalid: {w}"
        );
    }
}

/// Scenario C: Fingerprint change during pinned checkpoint and inference causes no deadlock.
#[tokio::test]
#[ignore]
async fn chaos_fingerprint_change_during_pinned_checkpoint_and_inference_no_deadlock() {
    let seed = resolve_and_log_seed();

    let tmp_dir = TempDir::new().expect("temp dir");
    let config = LsmConfig {
        path: tmp_dir.path().to_path_buf(),
        ..Default::default()
    };

    let storage = Arc::new(LsmStorage::new(config).await.expect("storage init"));
    let embedder = Arc::new(MockCandleEmbedder::new(32, Duration::from_millis(10)));
    let replicator: Arc<parking_lot::RwLock<ReplicatorState>> = Arc::new(parking_lot::RwLock::new(
        ReplicatorState::new(vec!["vector".to_string(), "text".to_string()], 0.05),
    ));

    // Pin checkpoint seq 1
    storage.pin_checkpoint(1).await.expect("pin checkpoint");

    let timeout_res = tokio::time::timeout(Duration::from_secs(30), async move {
        // Concurrently run 3 tasks: pinned checkpoint queries, candle inference, and fingerprint model switches
        let storage_task = {
            let s = Arc::clone(&storage);
            tokio::spawn(async move {
                for _ in 0..10 {
                    let _scan = s.scan_prefix_at(b"__docid:", 1).await;
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            })
        };

        let inference_task = {
            let e = Arc::clone(&embedder);
            tokio::spawn(async move {
                for i in 0..10 {
                    let text = format!("inference test query {}", i);
                    let _vec = EmbeddingProvider::embed(e.as_ref(), &text).await;
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            })
        };

        let fingerprint_task = {
            let r = Arc::clone(&replicator);
            tokio::spawn(async move {
                let mut fp_rng = SimpleRng::seed_from_u64(seed.wrapping_add(42));
                for _ in 0..10 {
                    let model_id = format!("model-v{}", fp_rng.gen_range_u64(1, 100));
                    let fp = ConfigFingerprint::new(&model_id, "Q4_K_M", "default", 0.05);
                    r.write().invalidate_on_config_change(fp);
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            })
        };

        let (r1, r2, r3) = tokio::join!(storage_task, inference_task, fingerprint_task);
        r1.unwrap();
        r2.unwrap();
        r3.unwrap();

        // Unpin checkpoint
        storage.unpin_checkpoint(1).await.expect("unpin checkpoint");
    })
    .await;

    assert!(
        timeout_res.is_ok(),
        "CRITICAL DEADLOCK DETECTED: Concurrent pinned checkpoint, Candle inference, and ConfigFingerprint switch timed out after 30s!"
    );
}
