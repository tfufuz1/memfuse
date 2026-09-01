//! Unit tests for memfuse-router.

#[cfg(test)]
#[allow(clippy::module_inception)]
mod tests {
    use crate::{dispatch_to_slm, RouterEngine, RoutingDecision, SlmProfile};
    use memfuse_core::{EntityId, MemFuseError, StorageEngine, TokenBudget};
    use memfuse_db::{MemFuse, MemFuseConfig};
    use serde_json::json;
    use std::sync::Arc;
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;

    #[tokio::test]
    async fn test_route_deterministic_community_assignment() {
        let dir = tempfile::tempdir().unwrap();
        let config = MemFuseConfig {
            dimension: 4,
            ..Default::default()
        };
        let db = MemFuse::open_with_config(dir.path(), config).await.unwrap();
        let collection = db.collection("default").await.unwrap();

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
            .unwrap();

        collection
            .insert(
                docs_key,
                &vec_docs,
                Some(json!({"text": "Dokumentation über Unternehmensrichtlinien."})),
            )
            .await
            .unwrap();

        // Relate entities to assign/update graph state and test get_community persistence
        let eid_coding = EntityId::from_key(coding_key).unwrap();
        let eid_docs = EntityId::from_key(docs_key).unwrap();

        collection
            .relate(coding_key, docs_key, "references")
            .await
            .unwrap();

        // Manually persist synthetic community IDs in storage for testing get_community:
        // 100 for coding, 200 for docs
        let tx = db.allocate_tx().unwrap();

        let comm_key_coding = format!("__graph:community:{}", eid_coding.inner()).into_bytes();
        let comm_key_docs = format!("__graph:community:{}", eid_docs.inner()).into_bytes();

        db.inner_storage()
            .put(tx, &comm_key_coding, &serde_json::to_vec(&100u64).unwrap())
            .await
            .unwrap();
        db.inner_storage()
            .put(tx, &comm_key_docs, &serde_json::to_vec(&200u64).unwrap())
            .await
            .unwrap();
        db.inner_storage().commit(tx).await.unwrap();

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
            .expect("Routing coding");

        assert_eq!(decision_coding.profile.name, "coding-slm");
        assert!(!decision_coding.context.chunks.is_empty());

        // Query docs
        let decision_docs = router
            .route(&vec_docs, "Unternehmensrichtlinien")
            .await
            .expect("Routing docs");

        assert_eq!(decision_docs.profile.name, "docs-slm");
        assert!(!decision_docs.context.chunks.is_empty());
    }

    #[tokio::test]
    async fn test_route_fallback_error_on_low_relevance() {
        let dir = tempfile::tempdir().unwrap();
        let config = MemFuseConfig {
            dimension: 4,
            ..Default::default()
        };
        let db = MemFuse::open_with_config(dir.path(), config).await.unwrap();
        let collection = db.collection("default").await.unwrap();

        let vec_unrelated = vec![0.0, 0.0, 0.0, 1.0];
        collection
            .insert(
                "unrelated_doc",
                &vec_unrelated,
                Some(json!({"text": "unrelated content"})),
            )
            .await
            .unwrap();

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
        // Setup a mock HTTP JSON-RPC 2.0 server
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let (tx, rx) = oneshot::channel::<serde_json::Value>();

        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buffer = [0u8; 4096];
                let n = socket.read(&mut buffer).await.unwrap();
                let request_str = String::from_utf8_lossy(&buffer[..n]);

                // Extract body after \r\n\r\n
                if let Some(body_idx) = request_str.find("\r\n\r\n") {
                    let body = &request_str[body_idx + 4..];
                    if let Ok(json_body) = serde_json::from_str::<serde_json::Value>(body) {
                        let _ = tx.send(json_body);
                    }
                }

                let response_body = json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": {
                        "answer": "SLM successfully processed context"
                    }
                })
                .to_string();

                let http_response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    response_body.len(),
                    response_body
                );
                socket.write_all(http_response.as_bytes()).await.ok();
            }
        });

        let profile = SlmProfile::new(
            "mock-slm",
            format!("http://{}", addr),
            vec![1],
            TokenBudget::new(50, 0),
            0.1,
        );

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
        };

        let answer = dispatch_to_slm(&decision).await.expect("dispatch ok");
        assert_eq!(answer, "SLM successfully processed context");

        let received_json = rx.await.expect("received request body");
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
        let dir = tempfile::tempdir().unwrap();
        let config = MemFuseConfig {
            dimension: 4,
            ..Default::default()
        };
        let db = MemFuse::open_with_config(dir.path(), config).await.unwrap();
        let collection = db.collection("default").await.unwrap();

        let vec_data = vec![1.0, 0.0, 0.0, 0.0];
        let key = "entity_1";
        collection
            .insert(key, &vec_data, Some(json!({"text": "sample text content"})))
            .await
            .unwrap();

        let eid = EntityId::from_key(key).unwrap();
        let tx = db.allocate_tx().unwrap();
        let comm_key = format!("__graph:community:{}", eid.inner()).into_bytes();
        db.inner_storage()
            .put(tx, &comm_key, &serde_json::to_vec(&10u64).unwrap())
            .await
            .unwrap();
        db.inner_storage().commit(tx).await.unwrap();

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
                    let decision = res.unwrap();
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
            h.await.unwrap();
        }
        writer_handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_route_hot_reload_atomic_snapshot_determinism() {
        let dir = tempfile::tempdir().unwrap();
        let config = MemFuseConfig {
            dimension: 4,
            ..Default::default()
        };
        let db = MemFuse::open_with_config(dir.path(), config).await.unwrap();
        let collection = db.collection("default").await.unwrap();

        let vec_data = vec![1.0, 0.0, 0.0, 0.0];
        let key = "entity_1";
        collection
            .insert(key, &vec_data, Some(json!({"text": "test content"})))
            .await
            .unwrap();

        let eid = EntityId::from_key(key).unwrap();
        let tx = db.allocate_tx().unwrap();
        let comm_key = format!("__graph:community:{}", eid.inner()).into_bytes();
        db.inner_storage()
            .put(tx, &comm_key, &serde_json::to_vec(&42u64).unwrap())
            .await
            .unwrap();
        db.inner_storage().commit(tx).await.unwrap();

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
        let d1 = router.route(&vec_data, "test content").await.unwrap();
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

        let d2 = router.route(&vec_data, "test content").await.unwrap();
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
            self.0.lock().unwrap().push(visitor.0);
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
    fn test_nan_single_chunk_ignored_in_max_score() {
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

        let selected_idx = select_profile_from_chunks(&[profile], &chunks)
            .expect("Profile selection must succeed ignoring NaN chunk");
        assert_eq!(selected_idx, 0);
    }

    #[test]
    fn test_nan_all_chunks_fallback_and_tracing_error() {
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

        let captured = logs.lock().unwrap();
        let found_log = captured.iter().any(|msg| {
            msg.contains("Alle Chunk-Relevanzwerte sind NaN/Inf — mögliche Upstream-Korruption in der Distanzberechnung")
        });

        assert!(
            found_log,
            "Expected tracing::error! message in logs, got: {:?}",
            *captured
        );
    }

    #[test]
    fn test_nan_routing_determinism_repeats() {
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

        let first_result = select_profile_from_chunks(&profiles, &chunks).expect("First selection");

        for i in 0..100 {
            let res = select_profile_from_chunks(&profiles, &chunks)
                .unwrap_or_else(|_| panic!("Selection failed on iteration {}", i));
            assert_eq!(
                res, first_result,
                "Routing selection must be bit-identical across runs (iteration {})",
                i
            );
        }
    }
}
