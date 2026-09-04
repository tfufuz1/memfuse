//! Unit tests for memfuse-router.

#[cfg(test)]
#[allow(clippy::module_inception)]
mod tests {
    use crate::{dispatch_to_slm, RouterEngine, RoutingDecision, SlmProfile};
    use memfuse_core::{EntityId, MemFuseError, StorageEngine, TokenBudget};
    use memfuse_db::{MemFuse, MemFuseConfig};
    use serde_json::json;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_route_deterministic_community_assignment() {
        let dir = tempfile::tempdir().unwrap(); // unwrap
        let config = MemFuseConfig {
            dimension: 4,
            ..Default::default()
        };
        let db = MemFuse::open_with_config(dir.path(), config).await.unwrap(); // unwrap
        let collection = db.collection("default").await.unwrap(); // unwrap

        // Insert documents for two distinct domains/communities
        let vec_coding = vec![1.0, 0.0, 0.0, 0.0];
        let vec_docs = vec![0.0, 1.0, 0.0, 0.0];

        let coding_key = "coding_entity_1";
        let docs_key = "docs_entity_1";

        collection
            .insert(
                coding_key,
                &vec_coding,
                Some(json!({"text": "function rust_code() { return 42; }"})),
            )
            .await
            .unwrap(); // unwrap

        collection
            .insert(
                docs_key,
                &vec_docs,
                Some(json!({"text": "Dokumentation über Unternehmensrichtlinien."})),
            )
            .await
            .unwrap(); // unwrap

        // Relate entities to assign/update graph state and test get_community persistence
        let eid_coding = EntityId::from_key(coding_key).unwrap(); // unwrap
        let eid_docs = EntityId::from_key(docs_key).unwrap(); // unwrap

        collection
            .relate(coding_key, docs_key, "references")
            .await
            .unwrap(); // unwrap

        // Manually persist synthetic community IDs in storage for testing get_community:
        // 100 for coding, 200 for docs
        let tx = db.allocate_tx().unwrap(); // unwrap

        let comm_key_coding = format!("__graph:community:{}", eid_coding.inner()).into_bytes();
        let comm_key_docs = format!("__graph:community:{}", eid_docs.inner()).into_bytes();

        db.inner_storage()
            .put(tx, &comm_key_coding, &serde_json::to_vec(&100u64).unwrap()) // unwrap
            .await
            .unwrap(); // unwrap
        db.inner_storage()
            .put(tx, &comm_key_docs, &serde_json::to_vec(&200u64).unwrap()) // unwrap
            .await
            .unwrap(); // unwrap
        db.inner_storage().commit(tx).await.unwrap(); // unwrap

        let coding_profile = SlmProfile::new(
            "coding-slm",
            "http://localhost:9999/mcp",
            vec![100],
            TokenBudget::new(1000, 100),
            0.01,
        );

        let docs_profile = SlmProfile::new(
            "docs-slm",
            "http://localhost:9999/mcp",
            vec![200],
            TokenBudget::new(1000, 100),
            0.01,
        );

        let router = RouterEngine::new(
            collection.clone(),
            vec![coding_profile.clone(), docs_profile.clone()],
        );

        // Query coding
        let decision_coding = router
            .route(&vec_coding, "rust_code")
            .await
            .expect("Routing coding"); // expect

        assert_eq!(decision_coding.profile.name, "coding-slm");
        assert!(!decision_coding.context.chunks.is_empty());

        // Query docs
        let decision_docs = router
            .route(&vec_docs, "Unternehmensrichtlinien")
            .await
            .expect("Routing docs"); // expect

        assert_eq!(decision_docs.profile.name, "docs-slm");
        assert!(!decision_docs.context.chunks.is_empty());
    }

    #[tokio::test]
    async fn test_route_fallback_error_on_low_relevance() {
        let dir = tempfile::tempdir().unwrap(); // unwrap
        let config = MemFuseConfig {
            dimension: 4,
            ..Default::default()
        };
        let db = MemFuse::open_with_config(dir.path(), config).await.unwrap(); // unwrap
        let collection = db.collection("default").await.unwrap(); // unwrap

        let vec_unrelated = vec![0.0, 0.0, 0.0, 1.0];
        collection
            .insert(
                "unrelated_doc",
                &vec_unrelated,
                Some(json!({"text": "unrelated content"})),
            )
            .await
            .unwrap(); // unwrap

        // High threshold profile that won't be met
        let strict_profile = SlmProfile::new(
            "strict-slm",
            "http://localhost:9999/mcp",
            vec![999], // non-existent community
            TokenBudget::new(1000, 100),
            0.99, // unreachable score threshold
        );

        let router = RouterEngine::new(collection.clone(), vec![strict_profile]);

        let result = router.route(&[0.1, 0.1, 0.1, 0.1], "search").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            MemFuseError::NotFound(msg) => {
                assert!(msg.contains("min_relevance_score") || msg.contains("Community-Zuordnung"));
            }
            other => panic!("Expected NotFound error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_dispatch_to_slm_mock_server_receives_trimmed_context_only() {
        let temp_dir = tempfile::tempdir().unwrap();
        let captured_req_path = temp_dir.path().join("request.json");
        let script_path = temp_dir.path().join("mock_slm.sh");
        std::fs::write(
            &script_path,
            format!(
                "#!/bin/sh\nread line\necho \"$line\" > {}\necho '{{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{{\"answer\":\"SLM successfully processed context\"}}}}'\n",
                captured_req_path.display()
            ),
        )
        .unwrap();

        let endpoint = format!("sh {}", script_path.display());
        let profile = SlmProfile::new("mock-slm", endpoint, vec![1], TokenBudget::new(50, 0), 0.1);

        let chunk = memfuse_core::ContextChunk {
            doc_id: memfuse_core::DocId::new(1),
            content: "Minimal context content for SLM".to_string(),
            relevance: 0.95,
            token_count: 5,
            metadata: None,
            contextual_prefix: None,
            links: Vec::new(),
        };

        let context_window = memfuse_core::ContextWindow {
            chunks: vec![chunk],
            total_tokens: 5,
            truncated: false,
        };

        let decision = RoutingDecision {
            profile,
            context: context_window,
            confidence: None,
        };

        let answer = dispatch_to_slm(&decision).await.expect("dispatch ok"); // expect
        assert_eq!(answer, "SLM successfully processed context");

        let captured_str =
            std::fs::read_to_string(&captured_req_path).expect("read captured request");
        let received_json: serde_json::Value =
            serde_json::from_str(&captured_str).expect("parse captured request");
        assert_eq!(received_json["method"], "slm_process_context");
        let params = &received_json["params"];
        assert_eq!(params["profile_name"], "mock-slm");
        assert!(params.get("context").is_some());
        // Verify that raw full search results are NOT present, only context window
        assert!(params.get("search_results").is_none());
        assert_eq!(
            params["context"]["chunks"][0]["content"],
            "Minimal context content for SLM"
        );
    }

    #[tokio::test]
    async fn test_route_hot_reload_concurrent_safety() {
        let dir = tempfile::tempdir().unwrap(); // unwrap
        let config = MemFuseConfig {
            dimension: 4,
            ..Default::default()
        };
        let db = MemFuse::open_with_config(dir.path(), config).await.unwrap(); // unwrap
        let collection = db.collection("default").await.unwrap(); // unwrap

        let vec_data = vec![1.0, 0.0, 0.0, 0.0];
        let key = "entity_1";
        collection
            .insert(key, &vec_data, Some(json!({"text": "sample text content"})))
            .await
            .unwrap(); // unwrap

        let eid = EntityId::from_key(key).unwrap(); // unwrap
        let tx = db.allocate_tx().unwrap(); // unwrap
        let comm_key = format!("__graph:community:{}", eid.inner()).into_bytes();
        db.inner_storage()
            .put(tx, &comm_key, &serde_json::to_vec(&10u64).unwrap()) // unwrap
            .await
            .unwrap(); // unwrap
        db.inner_storage().commit(tx).await.unwrap(); // unwrap

        let profile_v1 = SlmProfile::new(
            "slm-v1",
            "http://localhost:8001/mcp",
            vec![10],
            TokenBudget::new(1000, 100),
            0.01,
        );

        let router = Arc::new(RouterEngine::new(collection, vec![profile_v1]));

        // Spawn 20 reader tasks continuously calling route()
        let mut handles = Vec::new();
        for _ in 0..20 {
            let r = router.clone();
            let vec_c = vec_data.clone();
            handles.push(tokio::spawn(async move {
                for _ in 0..50 {
                    let res = r.route(&vec_c, "sample text content").await;
                    assert!(res.is_ok());
                    let decision = res.unwrap(); // unwrap
                    assert!(
                        decision.profile.name == "slm-v1" || decision.profile.name == "slm-v2",
                        "Unexpected profile name: {}",
                        decision.profile.name
                    );
                }
            }));
        }

        // Spawn background writer updating profiles dynamically
        let r_writer = router.clone();
        let writer_handle = tokio::spawn(async move {
            for i in 0..50 {
                let name = if i % 2 == 0 { "slm-v1" } else { "slm-v2" };
                let p = SlmProfile::new(
                    name,
                    "http://localhost:8001/mcp",
                    vec![10],
                    TokenBudget::new(1000, 100),
                    0.01,
                );
                r_writer.update_profiles(vec![p]);
                tokio::task::yield_now().await;
            }
        });

        for h in handles {
            h.await.unwrap(); // unwrap
        }
        writer_handle.await.unwrap(); // unwrap
    }

    #[tokio::test]
    async fn test_route_hot_reload_atomic_snapshot_determinism() {
        let dir = tempfile::tempdir().unwrap(); // unwrap
        let config = MemFuseConfig {
            dimension: 4,
            ..Default::default()
        };
        let db = MemFuse::open_with_config(dir.path(), config).await.unwrap(); // unwrap
        let collection = db.collection("default").await.unwrap(); // unwrap

        let vec_data = vec![1.0, 0.0, 0.0, 0.0];
        let key = "entity_1";
        collection
            .insert(key, &vec_data, Some(json!({"text": "test content"})))
            .await
            .unwrap(); // unwrap

        let eid = EntityId::from_key(key).unwrap(); // unwrap
        let tx = db.allocate_tx().unwrap(); // unwrap
        let comm_key = format!("__graph:community:{}", eid.inner()).into_bytes();
        db.inner_storage()
            .put(tx, &comm_key, &serde_json::to_vec(&42u64).unwrap()) // unwrap
            .await
            .unwrap(); // unwrap
        db.inner_storage().commit(tx).await.unwrap(); // unwrap

        let initial_profiles = vec![
            SlmProfile::new(
                "profile-a",
                "http://localhost/a",
                vec![42],
                TokenBudget::new(500, 50),
                0.01,
            ),
            SlmProfile::new(
                "profile-b",
                "http://localhost/b",
                vec![42],
                TokenBudget::new(500, 50),
                0.01,
            ),
        ];

        let router = RouterEngine::new(collection, initial_profiles);

        // Pre-reload decision: deterministic tie-breaking picks profile-a (lower index 0)
        let d1 = router.route(&vec_data, "test content").await.unwrap(); // unwrap
        assert_eq!(d1.profile.name, "profile-a");

        // Hot reload profile configuration with new single profile
        let updated_profiles = vec![SlmProfile::new(
            "profile-c",
            "http://localhost/c",
            vec![42],
            TokenBudget::new(500, 50),
            0.01,
        )];
        router.update_profiles(updated_profiles);

        let d2 = router.route(&vec_data, "test content").await.unwrap(); // unwrap
        assert_eq!(d2.profile.name, "profile-c");
    }

    #[derive(Clone)]
    struct LogCaptureLayer(Arc<std::sync::Mutex<Vec<String>>>);

    impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for LogCaptureLayer {
        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            _ctx: tracing_subscriber::layer::Context<'_, S>,
        ) {
            let mut visitor = StringVisitor(String::new());
            event.record(&mut visitor);
            if let Ok(mut guard) = self.0.lock() {
                guard.push(visitor.0);
            }
        }
    }

    struct StringVisitor(String);
    impl tracing::field::Visit for StringVisitor {
        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            use std::fmt::Write;
            write!(self.0, "{}={:?} ", field.name(), value).ok();
        }
    }

    #[test]
    fn test_nan_single_chunk_ignored_in_max_score() -> Result<(), Box<dyn std::error::Error>> {
        use crate::router::{compute_max_score, select_profile_from_chunks};
        use memfuse_core::{ContextChunk, DocId};

        let profile = SlmProfile::new(
            "test-slm",
            "http://localhost/mcp",
            vec![100],
            TokenBudget::new(1000, 100),
            0.1,
        );

        let chunk_valid_1 = ContextChunk {
            doc_id: DocId::new(1),
            content: "valid 1".to_string(),
            relevance: 0.5,
            token_count: 5,
            metadata: None,
            contextual_prefix: None,
            links: Vec::new(),
        };

        let chunk_nan = ContextChunk {
            doc_id: DocId::new(2),
            content: "corrupted nan".to_string(),
            relevance: f32::NAN,
            token_count: 5,
            metadata: None,
            contextual_prefix: None,
            links: Vec::new(),
        };

        let chunk_valid_2 = ContextChunk {
            doc_id: DocId::new(3),
            content: "valid 2".to_string(),
            relevance: 0.8,
            token_count: 5,
            metadata: None,
            contextual_prefix: None,
            links: Vec::new(),
        };

        let chunks = vec![
            (chunk_valid_1, Some(100)),
            (chunk_nan, Some(100)),
            (chunk_valid_2, Some(100)),
        ];

        let max_score = compute_max_score(&profile, &chunks);
        assert!(!max_score.is_nan(), "max_score must not be NaN");

        // Expected max score = 0.8 * 1.2 (community boost for community 100) = 0.96
        let expected = 0.8f32 * 1.2f32;
        assert!(
            (max_score - expected).abs() < 1e-5,
            "Expected max score {}, got {}",
            expected,
            max_score
        );

        let selected_idx = select_profile_from_chunks(&[profile], &chunks)?;
        assert_eq!(selected_idx, 0);

        Ok(())
    }

    #[test]
    fn test_nan_all_chunks_fallback_and_tracing_error() -> Result<(), Box<dyn std::error::Error>> {
        use crate::router::select_profile_from_chunks;
        use memfuse_core::{ContextChunk, DocId};
        use tracing_subscriber::layer::SubscriberExt;

        let logs = Arc::new(std::sync::Mutex::new(Vec::new()));
        let capture_layer = LogCaptureLayer(logs.clone());
        let subscriber = tracing_subscriber::registry().with(capture_layer);
        let _guard = tracing::subscriber::set_default(subscriber);

        let profile = SlmProfile::new(
            "test-slm",
            "http://localhost/mcp",
            vec![100],
            TokenBudget::new(1000, 100),
            0.1,
        );

        let chunk_nan_1 = ContextChunk {
            doc_id: DocId::new(1),
            content: "nan 1".to_string(),
            relevance: f32::NAN,
            token_count: 5,
            metadata: None,
            contextual_prefix: None,
            links: Vec::new(),
        };

        let chunk_nan_2 = ContextChunk {
            doc_id: DocId::new(2),
            content: "nan 2".to_string(),
            relevance: f32::NAN,
            token_count: 5,
            metadata: None,
            contextual_prefix: None,
            links: Vec::new(),
        };

        let chunks = vec![(chunk_nan_1, Some(100)), (chunk_nan_2, Some(100))];

        let result = select_profile_from_chunks(&[profile], &chunks);
        assert!(result.is_err(), "Expected error when all chunks are NaN");

        match result {
            Err(MemFuseError::NotFound(msg)) => {
                assert!(msg.contains("NaN/Inf"));
            }
            other => panic!("Expected NotFound error, got {:?}", other),
        }

        let captured = logs.lock().map_err(|e| e.to_string())?;
        let found_log = captured.iter().any(|msg| {
            msg.contains("Alle Chunk-Relevanzwerte sind NaN/Inf — mögliche Upstream-Korruption in der Distanzberechnung")
        });

        assert!(
            found_log,
            "Expected tracing::error! message in logs, got: {:?}",
            *captured
        );

        Ok(())
    }

    #[test]
    fn test_nan_routing_determinism_repeats() -> Result<(), Box<dyn std::error::Error>> {
        use crate::router::select_profile_from_chunks;
        use memfuse_core::{ContextChunk, DocId};

        let profile_a = SlmProfile::new(
            "slm-a",
            "http://localhost/a",
            vec![100],
            TokenBudget::new(1000, 100),
            0.1,
        );

        let profile_b = SlmProfile::new(
            "slm-b",
            "http://localhost/b",
            vec![100],
            TokenBudget::new(1000, 100),
            0.1,
        );

        let profiles = vec![profile_a, profile_b];

        let chunks = vec![
            (
                ContextChunk {
                    doc_id: DocId::new(1),
                    content: "corrupted nan".to_string(),
                    relevance: f32::NAN,
                    token_count: 5,
                    metadata: None,
                    contextual_prefix: None,
                    links: Vec::new(),
                },
                Some(100),
            ),
            (
                ContextChunk {
                    doc_id: DocId::new(2),
                    content: "valid chunk".to_string(),
                    relevance: 0.7,
                    token_count: 5,
                    metadata: None,
                    contextual_prefix: None,
                    links: Vec::new(),
                },
                Some(100),
            ),
        ];

        let first_result = select_profile_from_chunks(&profiles, &chunks)?;

        for i in 0..100 {
            let res = select_profile_from_chunks(&profiles, &chunks)?;
            assert_eq!(
                res, first_result,
                "Routing selection must be bit-identical across runs (iteration {})",
                i
            );
        }

        Ok(())
    }

    #[test]
    fn test_slm_profile_validation() {
        // Valid profile
        let valid = SlmProfile::try_new(
            "coding",
            "http://localhost:8000/mcp",
            vec![1],
            TokenBudget::new(1000, 100),
            0.1,
        );
        assert!(valid.is_ok());

        // Empty name
        let empty_name = SlmProfile::try_new(
            "   ",
            "http://localhost:8000/mcp",
            vec![1],
            TokenBudget::new(1000, 100),
            0.1,
        );
        assert!(
            matches!(empty_name, Err(MemFuseError::InvalidInput(msg)) if msg.contains("name cannot be empty"))
        );

        // Empty endpoint
        let empty_ep =
            SlmProfile::try_new("coding", "   ", vec![1], TokenBudget::new(1000, 100), 0.1);
        assert!(
            matches!(empty_ep, Err(MemFuseError::InvalidInput(msg)) if msg.contains("endpoint cannot be empty"))
        );

        // NaN relevance score
        let nan_score = SlmProfile::try_new(
            "coding",
            "http://localhost:8000/mcp",
            vec![1],
            TokenBudget::new(1000, 100),
            f32::NAN,
        );
        assert!(
            matches!(nan_score, Err(MemFuseError::InvalidInput(msg)) if msg.contains("must be finite and non-negative"))
        );
    }

    #[tokio::test]
    async fn test_route_empty_profiles_err() {
        let dir = tempfile::tempdir().unwrap(); // unwrap
        let config = MemFuseConfig {
            dimension: 4,
            ..Default::default()
        };
        let db = MemFuse::open_with_config(dir.path(), config).await.unwrap(); // unwrap
        let collection = db.collection("default").await.unwrap(); // unwrap

        let router = RouterEngine::new(collection, vec![]);
        let err = router.route(&[1.0, 0.0, 0.0, 0.0], "test").await;
        assert!(
            matches!(err, Err(MemFuseError::NotFound(msg)) if msg.contains("Keine SLM-Profile"))
        );
    }

    #[tokio::test]
    async fn test_route_empty_search_results_err() {
        let dir = tempfile::tempdir().unwrap(); // unwrap
        let config = MemFuseConfig {
            dimension: 4,
            ..Default::default()
        };
        let db = MemFuse::open_with_config(dir.path(), config).await.unwrap(); // unwrap
        let collection = db.collection("default").await.unwrap(); // unwrap

        let profile = SlmProfile::new(
            "slm",
            "http://localhost:9999/mcp",
            vec![1],
            TokenBudget::new(1000, 100),
            0.01,
        );
        let router = RouterEngine::new(collection, vec![profile]);

        let err = router.route(&[1.0, 0.0, 0.0, 0.0], "test").await;
        assert!(
            matches!(err, Err(MemFuseError::NotFound(msg)) if msg.contains("Keine relevanten Suchergebnisse"))
        );
    }

    #[tokio::test]
    async fn test_route_unparseable_entity_id() {
        let dir = tempfile::tempdir().unwrap(); // unwrap
        let config = MemFuseConfig {
            dimension: 4,
            ..Default::default()
        };
        let db = MemFuse::open_with_config(dir.path(), config).await.unwrap(); // unwrap
        let collection = db.collection("default").await.unwrap(); // unwrap

        // Key that does not conform to EntityId format
        collection
            .insert(
                "plain_document_without_entity_id_prefix",
                &[1.0, 0.0, 0.0, 0.0],
                Some(json!({"text": "plain doc text"})),
            )
            .await
            .unwrap(); // unwrap

        let profile = SlmProfile::new(
            "slm",
            "http://localhost:9999/mcp",
            vec![100],
            TokenBudget::new(1000, 100),
            0.0,
        );

        let router = RouterEngine::new(collection, vec![profile]);
        let result = router.route(&[1.0, 0.0, 0.0, 0.0], "plain doc").await;
        // Unparseable entity ID results in comm_id = None, which fails community matching for profile
        assert!(matches!(result, Err(MemFuseError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_route_threshold_boundaries() {
        let dir = tempfile::tempdir().unwrap(); // unwrap
        let config = MemFuseConfig {
            dimension: 4,
            ..Default::default()
        };
        let db = MemFuse::open_with_config(dir.path(), config).await.unwrap(); // unwrap
        let collection = db.collection("default").await.unwrap(); // unwrap

        let coding_key = "coding_entity_1";
        collection
            .insert(
                coding_key,
                &[1.0, 0.0, 0.0, 0.0],
                Some(json!({"text": "rust code text"})),
            )
            .await
            .unwrap(); // unwrap

        let eid = EntityId::from_key(coding_key).unwrap(); // unwrap
        let tx = db.allocate_tx().unwrap(); // unwrap
        let comm_key = format!("__graph:community:{}", eid.inner()).into_bytes();
        db.inner_storage()
            .put(tx, &comm_key, &serde_json::to_vec(&100u64).unwrap()) // unwrap
            .await
            .unwrap(); // unwrap
        db.inner_storage().commit(tx).await.unwrap(); // unwrap

        let profile = SlmProfile::new(
            "slm-threshold",
            "http://localhost:9999/mcp",
            vec![100],
            TokenBudget::new(1000, 100),
            0.0, // Low min threshold to guarantee selection
        );

        let router = RouterEngine::new(collection, vec![profile]);
        let res = router.route(&[1.0, 0.0, 0.0, 0.0], "rust code").await;
        assert!(res.is_ok());
    }

    #[test]
    fn test_route_determinism_and_tie_breaking() -> Result<(), Box<dyn std::error::Error>> {
        use crate::router::select_profile_from_chunks;
        use memfuse_core::{ContextChunk, DocId};

        let profile_0 = SlmProfile::new(
            "profile-0",
            "http://localhost/0",
            vec![10],
            TokenBudget::new(1000, 100),
            0.1,
        );

        let profile_1 = SlmProfile::new(
            "profile-1",
            "http://localhost/1",
            vec![10],
            TokenBudget::new(1000, 100),
            0.1,
        );

        let profile_2 = SlmProfile::new(
            "profile-2",
            "http://localhost/2",
            vec![10],
            TokenBudget::new(1000, 100),
            0.1,
        );

        let profiles = vec![profile_0, profile_1, profile_2];
        let chunks = vec![(
            ContextChunk {
                doc_id: DocId::new(1),
                content: "identical score chunk".to_string(),
                relevance: 0.5,
                token_count: 5,
                metadata: None,
                contextual_prefix: None,
                links: Vec::new(),
            },
            Some(10),
        )];

        for _ in 0..100 {
            let selected_idx = select_profile_from_chunks(&profiles, &chunks)?;
            // Lower profile index (0) must always win tie-breaks
            assert_eq!(selected_idx, 0);
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_dispatch_error_paths() {
        // 1. Process spawn error / bad command
        let bad_profile = SlmProfile::new(
            "bad-slm",
            "/nonexistent/binary/path/12345",
            vec![1],
            TokenBudget::new(50, 0),
            0.1,
        );
        let chunk = memfuse_core::ContextChunk {
            doc_id: memfuse_core::DocId::new(1),
            content: "test content".to_string(),
            relevance: 0.9,
            token_count: 5,
            metadata: None,
            contextual_prefix: None,
            links: Vec::new(),
        };
        let decision = RoutingDecision {
            profile: bad_profile,
            context: memfuse_core::ContextWindow {
                chunks: vec![chunk.clone()],
                total_tokens: 5,
                truncated: false,
            },
            confidence: None,
        };
        let res_err = dispatch_to_slm(&decision).await;
        assert!(
            matches!(res_err, Err(MemFuseError::Internal(msg)) if msg.contains("Fehler bei MCP-Dispatch"))
        );

        // 2. Closed stdout without JSON-RPC response
        let profile_closed =
            SlmProfile::new("slm-closed", "true", vec![1], TokenBudget::new(50, 0), 0.1);
        let decision_closed = RoutingDecision {
            profile: profile_closed,
            context: decision.context.clone(),
            confidence: None,
        };
        let res_closed = dispatch_to_slm(&decision_closed).await;
        assert!(
            matches!(res_closed, Err(MemFuseError::Internal(msg)) if msg.contains("Fehler bei MCP-Dispatch"))
        );

        // 3. RPC Error response
        let profile_rpc_err = SlmProfile::new(
            "slm-rpc-err",
            "cat > /dev/null; echo '{\"jsonrpc\":\"2.0\",\"id\":1,\"error\":{\"code\":-32601,\"message\":\"Method not found\"}}'",
            vec![1],
            TokenBudget::new(50, 0),
            0.1,
        );
        let decision_rpc_err = RoutingDecision {
            profile: profile_rpc_err,
            context: decision.context.clone(),
            confidence: None,
        };
        let res_rpc_err = dispatch_to_slm(&decision_rpc_err).await;
        assert!(
            matches!(res_rpc_err, Err(MemFuseError::Internal(ref msg)) if msg.contains("MCP RPC Fehler [-32601]: Method not found")),
            "res_rpc_err was: {:?}",
            res_rpc_err
        );

        // 4. Custom JSON object result (no "answer" key)
        let profile_obj = SlmProfile::new(
            "slm-obj",
            "cat > /dev/null; echo '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"custom_data\":42}}'",
            vec![1],
            TokenBudget::new(50, 0),
            0.1,
        );
        let decision_obj = RoutingDecision {
            profile: profile_obj,
            context: decision.context.clone(),
            confidence: None,
        };
        let res_obj = dispatch_to_slm(&decision_obj).await.unwrap(); // unwrap
        assert_eq!(res_obj, "{\"custom_data\":42}");

        // 5. Neither result nor error present
        let profile_empty = SlmProfile::new(
            "slm-empty",
            "cat > /dev/null; echo '{\"jsonrpc\":\"2.0\",\"id\":1}'",
            vec![1],
            TokenBudget::new(50, 0),
            0.1,
        );
        let decision_empty = RoutingDecision {
            profile: profile_empty,
            context: decision.context.clone(),
            confidence: None,
        };
        let res_empty = dispatch_to_slm(&decision_empty).await;
        assert!(
            matches!(res_empty, Err(MemFuseError::Internal(msg)) if msg.contains("weder result noch error"))
        );
    }

    #[tokio::test]
    async fn test_route_1_and_50_profiles() {
        let dir = tempfile::tempdir().unwrap(); // unwrap
        let config = MemFuseConfig {
            dimension: 4,
            ..Default::default()
        };
        let db = MemFuse::open_with_config(dir.path(), config).await.unwrap(); // unwrap
        let collection = db.collection("default").await.unwrap(); // unwrap

        let vec_data = vec![1.0, 0.0, 0.0, 0.0];
        let key = "entity_1";
        collection
            .insert(key, &vec_data, Some(json!({"text": "sample text"})))
            .await
            .unwrap(); // unwrap

        let eid = EntityId::from_key(key).unwrap(); // unwrap
        let tx = db.allocate_tx().unwrap(); // unwrap
        let comm_key = format!("__graph:community:{}", eid.inner()).into_bytes();
        db.inner_storage()
            .put(tx, &comm_key, &serde_json::to_vec(&100u64).unwrap()) // unwrap
            .await
            .unwrap(); // unwrap
        db.inner_storage().commit(tx).await.unwrap(); // unwrap

        let profiles_50: Vec<_> = (0..50)
            .map(|i| {
                SlmProfile::new(
                    format!("profile-{}", i),
                    format!("http://localhost:8000/mcp/{}", i),
                    vec![100],
                    TokenBudget::new(1000, 100),
                    0.0,
                )
            })
            .collect();

        let router = RouterEngine::try_new(collection, profiles_50).unwrap(); // unwrap
        let decision = router.route(&vec_data, "sample text").await.unwrap(); // unwrap
        assert_eq!(decision.profile.name, "profile-0");

        let update_res = router.try_update_profiles(vec![SlmProfile::new(
            "profile-single",
            "http://localhost/single",
            vec![100],
            TokenBudget::new(1000, 100),
            0.0,
        )]);
        assert!(update_res.is_ok());

        let decision_single = router.route(&vec_data, "sample text").await.unwrap(); // unwrap
        assert_eq!(decision_single.profile.name, "profile-single");
    }

    #[test]
    fn prop_slm_profile_equality() {
        use proptest::prelude::*;

        proptest!(|(
            name in "[a-z0-9_-]{1,20}",
            endpoint in "http://[a-z0-9_-]{1,20}",
            community in 0u64..10000,
            score in 0.0f32..1.0f32,
        )| {
            let p1 = SlmProfile::new(&name, &endpoint, vec![community], TokenBudget::new(1000, 100), score);
            let p2 = SlmProfile::new(&name, &endpoint, vec![community], TokenBudget::new(1000, 100), score);
            prop_assert_eq!(p1, p2);
        });
    }

    #[test]
    fn prop_slm_profile_serde() {
        use proptest::prelude::*;

        proptest!(|(
            name in "[a-z0-9_-]{1,20}",
            endpoint in "http://[a-z0-9_-]{1,20}",
            community in 0u64..10000,
            score in 0.0f32..1.0f32,
        )| {
            let p1 = SlmProfile::new(&name, &endpoint, vec![community], TokenBudget::new(1000, 100), score);
            let serialized = serde_json::to_string(&p1).unwrap(); // unwrap
            let p2: SlmProfile = serde_json::from_str(&serialized).unwrap(); // unwrap
            prop_assert_eq!(p1, p2);
        });
    }

    #[tokio::test]
    async fn test_router_engine_profiles_accessor() {
        let dir = tempfile::tempdir().unwrap(); // unwrap
        let config = MemFuseConfig {
            dimension: 4,
            ..Default::default()
        };
        let db = MemFuse::open_with_config(dir.path(), config).await.unwrap(); // unwrap
        let collection = db.collection("default").await.unwrap(); // unwrap

        let profile = SlmProfile::new(
            "p-acc",
            "http://localhost/mcp",
            vec![1],
            TokenBudget::new(100, 10),
            0.1,
        );
        let router = RouterEngine::new(collection, vec![profile.clone()]);
        let profiles = router.profiles();
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].name, "p-acc");
    }

    #[test]
    fn test_select_profile_from_chunks_empty_chunks() {
        use crate::router::select_profile_from_chunks;
        let profile = SlmProfile::new(
            "p-empty",
            "http://localhost/mcp",
            vec![1],
            TokenBudget::new(100, 10),
            0.1,
        );
        let res = select_profile_from_chunks(&[profile], &[]);
        assert!(
            matches!(res, Err(MemFuseError::NotFound(msg)) if msg.contains("Keine gültigen Chunks"))
        );
    }

    #[test]
    fn test_select_profile_max_score_meets_threshold_when_aggregated_does_not() {
        use crate::router::select_profile_from_chunks;
        use memfuse_core::{ContextChunk, DocId};

        // Profile requires min_relevance_score = 0.8
        let profile = SlmProfile::new(
            "p-max-score",
            "http://localhost/mcp",
            vec![10],
            TokenBudget::new(1000, 100),
            0.8,
        );

        let chunk_pos = ContextChunk {
            doc_id: DocId::new(2),
            content: "pos".to_string(),
            relevance: 0.8,
            token_count: 5,
            metadata: None,
            contextual_prefix: None,
            links: Vec::new(),
        };
        // Aggregated score = 0.8 * 1.2 = 0.96 >= 0.8
        let chunks = vec![(chunk_pos, Some(10))];
        let idx = select_profile_from_chunks(&[profile], &chunks).unwrap(); // unwrap
        assert_eq!(idx, 0);
    }

    #[test]
    fn prop_routing_decision_profile_in_input() {
        use crate::router::select_profile_from_chunks;
        use memfuse_core::{ContextChunk, DocId};
        use proptest::prelude::*;

        proptest!(|(
            score_a in 0.01f32..1.0f32,
            _score_b in 0.01f32..1.0f32,
        )| {
            let p0 = SlmProfile::new("p0", "http://ep0", vec![1], TokenBudget::new(1000, 100), 0.0);
            let p1 = SlmProfile::new("p1", "http://ep1", vec![1], TokenBudget::new(1000, 100), 0.0);
            let profiles = vec![p0, p1];

            let chunks = vec![(
                ContextChunk {
                    doc_id: DocId::new(1),
                    content: "test content".to_string(),
                    relevance: score_a,
                    token_count: 5,
                    metadata: None,
                    contextual_prefix: None,
                    links: Vec::new(),
                },
                Some(1),
            )];

            if let Ok(idx) = select_profile_from_chunks(&profiles, &chunks) {
                prop_assert!(idx < profiles.len());
            }
        });
    }

    #[tokio::test]
    async fn test_router_engine_try_new_and_try_update_profiles_validation_error() {
        let dir = tempfile::tempdir().unwrap(); // unwrap
        let config = MemFuseConfig {
            dimension: 4,
            ..Default::default()
        };
        let db = MemFuse::open_with_config(dir.path(), config).await.unwrap(); // unwrap
        let collection = db.collection("default").await.unwrap(); // unwrap

        let invalid_profile = SlmProfile::new(
            "",
            "http://localhost:8000/mcp",
            vec![1],
            TokenBudget::new(1000, 100),
            0.1,
        );

        let valid_profile = SlmProfile::new(
            "valid",
            "http://localhost:8000/mcp",
            vec![1],
            TokenBudget::new(1000, 100),
            0.1,
        );

        let res_try_new = RouterEngine::try_new(
            collection.clone(),
            vec![valid_profile.clone(), invalid_profile.clone()],
        );
        assert!(matches!(res_try_new, Err(MemFuseError::InvalidInput(_))));

        let router = RouterEngine::new(collection, vec![valid_profile.clone()]);
        let res_try_update = router.try_update_profiles(vec![valid_profile, invalid_profile]);
        assert!(matches!(res_try_update, Err(MemFuseError::InvalidInput(_))));
    }

    #[test]
    fn test_select_profile_from_chunks_empty_chunks_and_unmatched_community() {
        use crate::router::select_profile_from_chunks;
        use memfuse_core::{ContextChunk, DocId};

        let profile = SlmProfile::new(
            "slm-test",
            "http://localhost/mcp",
            vec![100],
            TokenBudget::new(1000, 100),
            0.5,
        );

        let err_empty = select_profile_from_chunks(std::slice::from_ref(&profile), &[]);
        assert!(
            matches!(err_empty, Err(MemFuseError::NotFound(msg)) if msg.contains("Keine gültigen Chunks"))
        );

        let chunk_unmatched = (
            ContextChunk {
                doc_id: DocId::new(1),
                content: "unmatched community chunk".to_string(),
                relevance: 0.9,
                token_count: 5,
                metadata: None,
                contextual_prefix: None,
                links: Vec::new(),
            },
            Some(999),
        );

        let err_unmatched =
            select_profile_from_chunks(std::slice::from_ref(&profile), &[chunk_unmatched]);
        assert!(
            matches!(err_unmatched, Err(MemFuseError::NotFound(msg)) if msg.contains("Kein SLM-Profil"))
        );

        let chunk_low_score = (
            ContextChunk {
                doc_id: DocId::new(2),
                content: "matched community low score".to_string(),
                relevance: 0.01,
                token_count: 5,
                metadata: None,
                contextual_prefix: None,
                links: Vec::new(),
            },
            Some(100),
        );

        let err_low =
            select_profile_from_chunks(std::slice::from_ref(&profile), &[chunk_low_score]);
        assert!(
            matches!(err_low, Err(MemFuseError::NotFound(msg)) if msg.contains("Kein SLM-Profil"))
        );
    }

    #[tokio::test]
    async fn test_dispatch_invalid_json_response() {
        let profile = SlmProfile::new(
            "bad-json-slm",
            "cat > /dev/null; echo '{invalid json'",
            vec![1],
            TokenBudget::new(50, 0),
            0.1,
        );

        let chunk = memfuse_core::ContextChunk {
            doc_id: memfuse_core::DocId::new(1),
            content: "test content".to_string(),
            relevance: 0.9,
            token_count: 5,
            metadata: None,
            contextual_prefix: None,
            links: Vec::new(),
        };

        let decision = RoutingDecision {
            profile,
            context: memfuse_core::ContextWindow {
                chunks: vec![chunk],
                total_tokens: 5,
                truncated: false,
            },
            confidence: None,
        };

        let res = dispatch_to_slm(&decision).await;
        assert!(
            matches!(res, Err(MemFuseError::Internal(msg)) if msg.contains("Ungültige MCP JSON-RPC Antwort"))
        );
    }

    #[test]
    fn test_slm_profile_validation_extended() {
        let inf_score = SlmProfile::try_new(
            "coding",
            "http://localhost:8000/mcp",
            vec![1],
            TokenBudget::new(1000, 100),
            f32::INFINITY,
        );
        assert!(
            matches!(inf_score, Err(MemFuseError::InvalidInput(msg)) if msg.contains("must be finite and non-negative"))
        );

        let neg_inf_score = SlmProfile::try_new(
            "coding",
            "http://localhost:8000/mcp",
            vec![1],
            TokenBudget::new(1000, 100),
            f32::NEG_INFINITY,
        );
        assert!(
            matches!(neg_inf_score, Err(MemFuseError::InvalidInput(msg)) if msg.contains("must be finite and non-negative"))
        );

        let neg_score = SlmProfile::try_new(
            "coding",
            "http://localhost:8000/mcp",
            vec![1],
            TokenBudget::new(1000, 100),
            -0.1,
        );
        assert!(
            matches!(neg_score, Err(MemFuseError::InvalidInput(msg)) if msg.contains("must be finite and non-negative"))
        );
    }

    #[tokio::test]
    async fn test_route_with_missing_community_or_corrupt_result() {
        let dir = tempfile::tempdir().unwrap(); // unwrap
        let config = MemFuseConfig {
            dimension: 4,
            ..Default::default()
        };
        let db = MemFuse::open_with_config(dir.path(), config).await.unwrap(); // unwrap
        let collection = db.collection("default").await.unwrap(); // unwrap

        let key = "entity_no_community";
        collection
            .insert(
                key,
                &[1.0, 0.0, 0.0, 0.0],
                Some(json!({"text": "sample text"})),
            )
            .await
            .unwrap(); // unwrap

        let profile = SlmProfile::new(
            "slm-no-comm",
            "http://localhost:9999/mcp",
            vec![100],
            TokenBudget::new(1000, 100),
            0.0,
        );

        let router = RouterEngine::new(collection, vec![profile]);
        let res = router.route(&[1.0, 0.0, 0.0, 0.0], "sample text").await;
        assert!(matches!(res, Err(MemFuseError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_route_invalid_search_result_skips_chunk() -> Result<(), Box<dyn std::error::Error>>
    {
        let dir = tempfile::tempdir()?;
        let config = MemFuseConfig {
            dimension: 4,
            ..Default::default()
        };
        let db = MemFuse::open_with_config(dir.path(), config).await?;
        let collection = db.collection("default").await?;

        // Insert valid doc and corrupt/invalid doc directly into storage/index or test search result handling
        let valid_key = "valid_entity_1";
        collection
            .insert(
                valid_key,
                &[1.0, 0.0, 0.0, 0.0],
                Some(json!({"text": "valid content"})),
            )
            .await?;

        let eid = EntityId::from_key(valid_key)?;
        let tx = db.allocate_tx()?;
        let comm_key = format!("__graph:community:{}", eid.inner()).into_bytes();
        let comm_val = serde_json::to_vec(&100u64)?;
        db.inner_storage().put(tx, &comm_key, &comm_val).await?;
        db.inner_storage().commit(tx).await?;

        let profile = SlmProfile::new(
            "slm-valid",
            "http://localhost:9999/mcp",
            vec![100],
            TokenBudget::new(1000, 100),
            0.0,
        );

        let router = RouterEngine::new(collection, vec![profile]);
        let res = router.route(&[1.0, 0.0, 0.0, 0.0], "valid content").await;
        assert!(res.is_ok());
        Ok(())
    }

    #[test]
    fn test_cascade_hit() {
        use crate::profile::ProfileCalibrationState;
        use memfuse_core::{ContextChunk, DocId};
        use std::collections::HashMap;

        let profile_high = SlmProfile::new(
            "high-slm",
            "http://localhost/high",
            vec![1],
            TokenBudget::new(1000, 100),
            0.8,
        );
        let profile_mid = SlmProfile::new(
            "mid-slm",
            "http://localhost/mid",
            vec![1],
            TokenBudget::new(1000, 100),
            0.5,
        );
        let profile_low = SlmProfile::new(
            "low-slm",
            "http://localhost/low",
            vec![1],
            TokenBudget::new(1000, 100),
            0.2,
        );

        let profiles = vec![
            profile_mid.clone(),
            profile_high.clone(),
            profile_low.clone(),
        ];

        let dir = tempfile::tempdir().unwrap();
        let config = MemFuseConfig {
            dimension: 4,
            ..Default::default()
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        let db = rt
            .block_on(MemFuse::open_with_config(dir.path(), config))
            .unwrap();
        let collection = rt.block_on(db.collection("default")).unwrap();

        let router = RouterEngine::new(collection, profiles.clone());
        let calibration: HashMap<String, ProfileCalibrationState> = HashMap::new();

        // Chunk score: 0.5 (with community 1 match: 0.5 * 1.2 = 0.6)
        // 0.6 >= mid threshold (0.5), but < high threshold (0.8)
        let chunk = ContextChunk {
            doc_id: DocId::new(1),
            content: "cascade hit text".to_string(),
            relevance: 0.5,
            token_count: 5,
            metadata: None,
            contextual_prefix: None,
            links: Vec::new(),
        };
        let chunks = vec![(chunk, Some(1))];

        let (idx, selected, metrics) = router
            .select_profile_cascade(&chunks, &profiles, &calibration)
            .expect("Cascade selection succeeds");

        assert_eq!(selected.name, "mid-slm");
        assert_eq!(idx, 0); // profile_mid was at original index 0
        assert!(!metrics.calibrated); // window_total <= 10
    }

    #[test]
    fn test_cascade_fallthrough() {
        use crate::profile::ProfileCalibrationState;
        use memfuse_core::{ContextChunk, DocId};
        use std::collections::HashMap;

        let profile_high = SlmProfile::new(
            "high-slm",
            "http://localhost/high",
            vec![1],
            TokenBudget::new(1000, 100),
            0.9,
        );
        let profile_mid = SlmProfile::new(
            "mid-slm",
            "http://localhost/mid",
            vec![1],
            TokenBudget::new(1000, 100),
            0.7,
        );
        let profile_low = SlmProfile::new(
            "low-slm",
            "http://localhost/low",
            vec![1],
            TokenBudget::new(1000, 100),
            0.5,
        );

        let profiles = vec![profile_high, profile_mid, profile_low.clone()];

        let dir = tempfile::tempdir().unwrap();
        let config = MemFuseConfig {
            dimension: 4,
            ..Default::default()
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        let db = rt
            .block_on(MemFuse::open_with_config(dir.path(), config))
            .unwrap();
        let collection = rt.block_on(db.collection("default")).unwrap();

        let router = RouterEngine::new(collection, profiles.clone());
        let calibration: HashMap<String, ProfileCalibrationState> = HashMap::new();

        // Chunk score = 0.1 (0.1 * 1.2 = 0.12) < low threshold (0.5) -> falls through to last profile
        let chunk = ContextChunk {
            doc_id: DocId::new(1),
            content: "low score content".to_string(),
            relevance: 0.1,
            token_count: 5,
            metadata: None,
            contextual_prefix: None,
            links: Vec::new(),
        };
        let chunks = vec![(chunk, Some(1))];

        let (idx, selected, metrics) = router
            .select_profile_cascade(&chunks, &profiles, &calibration)
            .expect("Cascade fallthrough succeeds");

        assert_eq!(selected.name, "low-slm");
        assert_eq!(idx, 2);
        assert!(!metrics.calibrated);
    }

    #[tokio::test]
    async fn test_calibrated_threshold_convergence() {
        let dir = tempfile::tempdir().unwrap();
        let config = MemFuseConfig {
            dimension: 4,
            ..Default::default()
        };
        let db = MemFuse::open_with_config(dir.path(), config).await.unwrap();
        let collection = db.collection("default").await.unwrap();

        let vec_data = vec![1.0, 0.0, 0.0, 0.0];
        let key = "convergence_entity";
        collection
            .insert(
                key,
                &vec_data,
                Some(json!({"text": "convergence test content"})),
            )
            .await
            .unwrap();

        let eid = EntityId::from_key(key).unwrap();
        let tx = db.allocate_tx().unwrap();
        let comm_key = format!("__graph:community:{}", eid.inner()).into_bytes();
        db.inner_storage()
            .put(tx, &comm_key, &serde_json::to_vec(&100u64).unwrap())
            .await
            .unwrap();
        db.inner_storage().commit(tx).await.unwrap();

        let profile = SlmProfile::new(
            "conv-slm",
            "http://localhost:9999/mcp",
            vec![100],
            TokenBudget::new(1000, 100),
            0.001,
        );

        let router = RouterEngine::new(collection, vec![profile]);

        // Perform 55 routing calls (>= 50 samples)
        let mut last_calibrated = false;
        for i in 0..55 {
            let decision = router
                .route(&vec_data, "convergence test content")
                .await
                .unwrap();
            let conf = decision.confidence.expect("Confidence metrics present");
            last_calibrated = conf.calibrated;
            let cal_stats = router.calibration_stats();
            let st = &cal_stats["conv-slm"];
            println!(
                "Call {}: window_total={}, quantile_threshold={}, calibrated={}",
                i + 1,
                st.conformal.window_total,
                st.conformal.quantile_threshold,
                conf.calibrated
            );
        }

        assert!(
            last_calibrated,
            "After 55 decisions (>= 50 samples), decision must be calibrated (calibrated = true)"
        );
    }

    #[tokio::test]
    async fn test_cascade_determinism() {
        use crate::profile::ProfileCalibrationState;
        use memfuse_core::{ContextChunk, DocId};
        use std::collections::HashMap;

        let dir = tempfile::tempdir().unwrap();
        let config = MemFuseConfig {
            dimension: 4,
            ..Default::default()
        };
        let db = MemFuse::open_with_config(dir.path(), config).await.unwrap();
        let collection = db.collection("default").await.unwrap();

        let p1 = SlmProfile::new(
            "slm-1",
            "http://localhost/1",
            vec![100],
            TokenBudget::new(1000, 100),
            0.1,
        );
        let p2 = SlmProfile::new(
            "slm-2",
            "http://localhost/2",
            vec![100],
            TokenBudget::new(1000, 100),
            0.1,
        );

        let profiles = vec![p1, p2];
        let router = RouterEngine::new(collection, profiles.clone());
        let calibration: HashMap<String, ProfileCalibrationState> = HashMap::new();

        let chunk = ContextChunk {
            doc_id: DocId::new(1),
            content: "deterministic content".to_string(),
            relevance: 0.5,
            token_count: 5,
            metadata: None,
            contextual_prefix: None,
            links: Vec::new(),
        };
        let chunks = vec![(chunk, Some(100))];

        let (_, first_profile, _) = router
            .select_profile_cascade(&chunks, &profiles, &calibration)
            .unwrap();

        for i in 0..50 {
            let (_, next_profile, _) = router
                .select_profile_cascade(&chunks, &profiles, &calibration)
                .unwrap();
            assert_eq!(
                next_profile.name, first_profile.name,
                "Inconsistent profile selected at iteration {}",
                i
            );
        }
    }

    #[tokio::test]
    async fn test_parallel_route_conformal_calibration_monotonic_convergence() {
        let dir = tempfile::tempdir().unwrap();
        let config = MemFuseConfig {
            dimension: 4,
            ..Default::default()
        };
        let db = MemFuse::open_with_config(dir.path(), config).await.unwrap();
        let collection = db.collection("default").await.unwrap();

        let vec_data = vec![1.0, 0.0, 0.0, 0.0];
        let key = "parallel_conv_entity";
        collection
            .insert(
                key,
                &vec_data,
                Some(json!({"text": "parallel convergence content"})),
            )
            .await
            .unwrap();

        let eid = EntityId::from_key(key).unwrap();
        let tx = db.allocate_tx().unwrap();
        let comm_key = format!("__graph:community:{}", eid.inner()).into_bytes();
        db.inner_storage()
            .put(tx, &comm_key, &serde_json::to_vec(&100u64).unwrap())
            .await
            .unwrap();
        db.inner_storage().commit(tx).await.unwrap();

        let profile = SlmProfile::new(
            "parallel-conv-slm",
            "http://localhost:9999/mcp",
            vec![100],
            TokenBudget::new(1000, 100),
            0.5,
        );

        let router = Arc::new(RouterEngine::new(collection, vec![profile]));

        // Spawn 100 parallel route() tasks
        let mut handles = Vec::new();
        for _ in 0..100 {
            let r = router.clone();
            let vec_c = vec_data.clone();
            handles.push(tokio::spawn(async move {
                r.route(&vec_c, "parallel convergence content").await
            }));
        }

        for h in handles {
            let res = h.await.unwrap();
            assert!(res.is_ok(), "route() failed in parallel task: {:?}", res);
        }

        let stats = router.calibration_stats();
        let st = &stats["parallel-conv-slm"];

        // Verify exact selected counts and total window
        assert_eq!(st.times_selected, 100);
        assert_eq!(st.conformal.window_total, 100);

        // Verify calibrated_min_score stays strictly bounded and non-oscillating
        let lower_bound = st.original_min_score * 0.5;
        let upper_bound = st.original_min_score * 2.0;
        assert!(
            st.calibrated_min_score >= lower_bound && st.calibrated_min_score <= upper_bound,
            "calibrated_min_score {} out of bounds [{}, {}]",
            st.calibrated_min_score,
            lower_bound,
            upper_bound
        );
    }

    #[test]
    fn test_conformal_calibrator_default_and_reset_window() {
        use crate::profile::ConformalCalibrator;
        let mut cal = ConformalCalibrator::default();
        assert_eq!(cal.alpha, 0.05);
        assert_eq!(cal.gamma, 0.01);
        assert_eq!(cal.quantile_threshold, 0.5);
        assert_eq!(cal.empirical_error_rate(), 0.0);

        cal.update(0.8);
        assert_eq!(cal.window_total, 1);
        assert_eq!(cal.window_errors, 1);

        cal.reset_window();
        assert_eq!(cal.window_total, 0);
        assert_eq!(cal.window_errors, 0);
        assert_eq!(cal.empirical_error_rate(), 0.0);
    }

    #[test]
    fn test_profile_calibration_state_default_and_average_confidence() {
        use crate::profile::ProfileCalibrationState;
        let default_st = ProfileCalibrationState::default();
        assert_eq!(default_st.times_selected, 0);
        assert_eq!(default_st.average_confidence(), 1.0);

        let mut st = ProfileCalibrationState::new(0.5);
        st.times_selected = 2;
        st.cumulative_confidence = 3.0;
        assert_eq!(st.average_confidence(), 1.5);
    }

    #[tokio::test]
    async fn test_router_engine_reset_all_calibration() {
        let dir = tempfile::tempdir().unwrap();
        let config = MemFuseConfig {
            dimension: 4,
            ..Default::default()
        };
        let db = MemFuse::open_with_config(dir.path(), config).await.unwrap();
        let collection = db.collection("default").await.unwrap();

        let p1 = SlmProfile::new("p1", "http://ep1", vec![1], TokenBudget::default(), 0.1);
        let p2 = SlmProfile::new("p2", "http://ep2", vec![2], TokenBudget::default(), 0.2);

        let router = RouterEngine::new(collection, vec![p1, p2]);
        {
            let cal = router.calibration_stats();
            assert_eq!(cal["p1"].times_selected, 0);
        }

        // Simulate selected counts
        {
            let mut cal = router.calibration.write();
            if let Some(st1) = cal.get_mut("p1") {
                st1.times_selected = 10;
            }
            if let Some(st2) = cal.get_mut("p2") {
                st2.times_selected = 20;
            }
        }

        assert_eq!(router.calibration_stats()["p1"].times_selected, 10);
        assert_eq!(router.calibration_stats()["p2"].times_selected, 20);

        router.reset_all_calibration();
        assert_eq!(router.calibration_stats()["p1"].times_selected, 0);
        assert_eq!(router.calibration_stats()["p2"].times_selected, 0);
    }

    #[tokio::test]
    async fn test_route_non_finite_query_embedding_err() {
        let dir = tempfile::tempdir().unwrap();
        let config = MemFuseConfig {
            dimension: 4,
            ..Default::default()
        };
        let db = MemFuse::open_with_config(dir.path(), config).await.unwrap();
        let collection = db.collection("default").await.unwrap();

        let p = SlmProfile::new("p", "http://ep", vec![1], TokenBudget::default(), 0.1);
        let router = RouterEngine::new(collection, vec![p]);

        let res_nan = router.route(&[f32::NAN, 0.0, 0.0, 0.0], "query").await;
        assert!(
            matches!(res_nan, Err(MemFuseError::InvalidInput(msg)) if msg.contains("non-finite"))
        );

        let res_inf = router.route(&[f32::INFINITY, 0.0, 0.0, 0.0], "query").await;
        assert!(
            matches!(res_inf, Err(MemFuseError::InvalidInput(msg)) if msg.contains("non-finite"))
        );
    }

    #[tokio::test]
    async fn test_dispatch_empty_endpoint_err() {
        let profile = SlmProfile::new("empty-ep", "  ", vec![1], TokenBudget::default(), 0.1);
        let decision = RoutingDecision {
            profile,
            context: memfuse_core::ContextWindow {
                chunks: vec![],
                total_tokens: 0,
                truncated: false,
            },
            confidence: None,
        };

        let res = dispatch_to_slm(&decision).await;
        assert!(
            matches!(res, Err(MemFuseError::InvalidInput(msg)) if msg.contains("Empty MCP endpoint"))
        );
    }

    #[test]
    fn test_serde_helpers_sorted_u64_set() -> Result<(), Box<dyn std::error::Error>> {
        use crate::serde_helpers::sorted_u64_set;
        use std::collections::HashSet;

        #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
        struct TestContainer {
            #[serde(with = "sorted_u64_set")]
            set: HashSet<u64>,
        }

        let container = TestContainer {
            set: [42, 10, 5, 100].into_iter().collect(),
        };

        let json = serde_json::to_string(&container)?;
        assert_eq!(json, r#"{"set":[5,10,42,100]}"#);

        let deserialized: TestContainer = serde_json::from_str(&json)?;
        assert_eq!(deserialized, container);
        Ok(())
    }
}
