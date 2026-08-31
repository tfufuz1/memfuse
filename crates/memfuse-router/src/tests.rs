//! Unit tests for memfuse-router.

#[cfg(test)]
#[allow(clippy::module_inception)]
mod tests {
    use crate::{dispatch_to_slm, RouterEngine, RoutingDecision, SlmProfile};
    use memfuse_core::{
        ContextChunk, ContextWindow, DocId, EntityId, MemFuseError, StorageEngine, TokenBudget,
    };
    use memfuse_db::{MemFuse, MemFuseConfig};
    use serde_json::json;
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;

    #[test]
    fn test_slm_profile_new_and_serialization() {
        let profile = SlmProfile::new(
            "test-slm",
            "http://127.0.0.1:8080/mcp",
            vec![42, 100],
            TokenBudget::new(2048, 256),
            0.75,
        );

        assert_eq!(profile.name, "test-slm");
        assert_eq!(profile.mcp_endpoint, "http://127.0.0.1:8080/mcp");
        assert_eq!(profile.domain_communities, vec![42, 100]);
        assert_eq!(profile.token_budget.limit, 2048);
        assert_eq!(profile.token_budget.reserved, 256);
        assert!((profile.min_relevance_score - 0.75).abs() < f32::EPSILON);

        // Serde roundtrip test
        let json_str = serde_json::to_string(&profile).expect("serialize profile");
        let deserialized: SlmProfile =
            serde_json::from_str(&json_str).expect("deserialize profile");
        assert_eq!(profile, deserialized);
    }

    #[test]
    fn test_routing_decision_serialization() {
        let profile = SlmProfile::new(
            "serde-slm",
            "http://localhost:5000/mcp",
            vec![1],
            TokenBudget::new(512, 64),
            0.5,
        );

        let chunk = ContextChunk {
            doc_id: DocId::new(10),
            content: "Test chunk content".to_string(),
            relevance: 0.8,
            token_count: 3,
            metadata: None,
            contextual_prefix: None,
            links: Vec::new(),
        };

        let context = ContextWindow {
            chunks: vec![chunk],
            total_tokens: 3,
            truncated: false,
        };

        let decision = RoutingDecision { profile, context };

        let json_str = serde_json::to_string(&decision).expect("serialize routing decision");
        let deserialized: RoutingDecision =
            serde_json::from_str(&json_str).expect("deserialize routing decision");

        assert_eq!(decision.profile, deserialized.profile);
        assert_eq!(
            decision.context.chunks.len(),
            deserialized.context.chunks.len()
        );
        assert_eq!(
            decision.context.chunks[0].content,
            deserialized.context.chunks[0].content
        );
        assert_eq!(
            decision.context.total_tokens,
            deserialized.context.total_tokens
        );
        assert_eq!(decision.context.truncated, deserialized.context.truncated);
    }

    #[tokio::test]
    async fn test_route_err_no_profiles_configured() {
        let dir = tempfile::tempdir().unwrap();
        let config = MemFuseConfig {
            dimension: 4,
            ..Default::default()
        };
        let db = MemFuse::open_with_config(dir.path(), config).await.unwrap();
        let collection = db.collection("default").await.unwrap();

        let router = RouterEngine::new(collection, vec![]);
        let result = router.route(&[1.0, 0.0, 0.0, 0.0], "query").await;

        assert!(result.is_err());
        match result.unwrap_err() {
            MemFuseError::NotFound(msg) => {
                assert!(msg.contains("Keine SLM-Profile"));
            }
            other => panic!("Expected NotFound, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_route_err_empty_collection_no_search_results() {
        let dir = tempfile::tempdir().unwrap();
        let config = MemFuseConfig {
            dimension: 4,
            ..Default::default()
        };
        let db = MemFuse::open_with_config(dir.path(), config).await.unwrap();
        let collection = db.collection("default").await.unwrap();

        let profile = SlmProfile::new(
            "dummy-slm",
            "http://localhost:9999/mcp",
            vec![1],
            TokenBudget::new(1000, 100),
            0.1,
        );

        let router = RouterEngine::new(collection, vec![profile]);
        let result = router.route(&[1.0, 0.0, 0.0, 0.0], "query").await;

        assert!(result.is_err());
        match result.unwrap_err() {
            MemFuseError::NotFound(msg) => {
                assert!(msg.contains("Keine relevanten Suchergebnisse"));
            }
            other => panic!("Expected NotFound, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_route_with_unicode_query() {
        let dir = tempfile::tempdir().unwrap();
        let config = MemFuseConfig {
            dimension: 4,
            ..Default::default()
        };
        let db = MemFuse::open_with_config(dir.path(), config).await.unwrap();
        let collection = db.collection("default").await.unwrap();

        let vec_data = vec![1.0, 0.0, 0.0, 0.0];
        let doc_key = "unicode_doc_key";
        collection
            .insert(
                doc_key,
                &vec_data,
                Some(json!({"text": "Künstliche Intelligenz 🤖 und Mehrbyte-Zeichen テスト"})),
            )
            .await
            .unwrap();

        let eid_unicode = EntityId::from_key(doc_key).unwrap();
        let tx = db.allocate_tx().unwrap();
        let comm_key_unicode = format!("__graph:community:{}", eid_unicode.inner()).into_bytes();

        db.inner_storage()
            .put(tx, &comm_key_unicode, &serde_json::to_vec(&500u64).unwrap())
            .await
            .unwrap();
        db.inner_storage().commit(tx).await.unwrap();

        let profile = SlmProfile::new(
            "unicode-slm",
            "http://localhost:9999/mcp",
            vec![500],
            TokenBudget::new(1000, 100),
            0.0,
        );

        let router = RouterEngine::new(collection, vec![profile]);
        let decision = router
            .route(&vec_data, "Künstliche Intelligenz 🤖 テスト")
            .await
            .expect("routing with unicode query should succeed");

        assert_eq!(decision.profile.name, "unicode-slm");
        assert!(!decision.context.chunks.is_empty());
    }

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

        let chunk = ContextChunk {
            doc_id: DocId::new(1),
            content: "Minimal context content for SLM".to_string(),
            relevance: 0.95,
            token_count: 5,
            metadata: None,
            contextual_prefix: None,
            links: Vec::new(),
        };

        let context_window = ContextWindow {
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
    async fn test_dispatch_to_slm_connection_error() {
        let profile = SlmProfile::new(
            "offline-slm",
            "http://127.0.0.1:1/mcp", // Unreachable port 1
            vec![],
            TokenBudget::new(100, 10),
            0.0,
        );

        let decision = RoutingDecision {
            profile,
            context: ContextWindow {
                chunks: vec![],
                total_tokens: 0,
                truncated: false,
            },
        };

        let result = dispatch_to_slm(&decision).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            MemFuseError::Internal(msg) => {
                assert!(msg.contains("Fehler bei MCP-Dispatch"));
            }
            other => panic!("Expected Internal error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_dispatch_to_slm_http_500_error() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                use tokio::io::AsyncWriteExt;
                let http_response =
                    "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n";
                socket.write_all(http_response.as_bytes()).await.ok();
            }
        });

        let profile = SlmProfile::new(
            "error-slm",
            format!("http://{}", addr),
            vec![],
            TokenBudget::new(100, 10),
            0.0,
        );

        let decision = RoutingDecision {
            profile,
            context: ContextWindow {
                chunks: vec![],
                total_tokens: 0,
                truncated: false,
            },
        };

        let result = dispatch_to_slm(&decision).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            MemFuseError::Internal(msg) => {
                assert!(msg.contains("meldet HTTP Status 500"));
            }
            other => panic!("Expected Internal error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_dispatch_to_slm_jsonrpc_error_response() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                use tokio::io::AsyncWriteExt;
                let response_body = json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "error": {
                        "code": -32601,
                        "message": "Method not found"
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
            "rpc-error-slm",
            format!("http://{}", addr),
            vec![],
            TokenBudget::new(100, 10),
            0.0,
        );

        let decision = RoutingDecision {
            profile,
            context: ContextWindow {
                chunks: vec![],
                total_tokens: 0,
                truncated: false,
            },
        };

        let result = dispatch_to_slm(&decision).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            MemFuseError::Internal(msg) => {
                assert!(msg.contains("MCP RPC Fehler [-32601]: Method not found"));
            }
            other => panic!("Expected Internal error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_dispatch_to_slm_missing_result_and_error() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                use tokio::io::AsyncWriteExt;
                let response_body = json!({
                    "jsonrpc": "2.0",
                    "id": 1
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
            "empty-resp-slm",
            format!("http://{}", addr),
            vec![],
            TokenBudget::new(100, 10),
            0.0,
        );

        let decision = RoutingDecision {
            profile,
            context: ContextWindow {
                chunks: vec![],
                total_tokens: 0,
                truncated: false,
            },
        };

        let result = dispatch_to_slm(&decision).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            MemFuseError::Internal(msg) => {
                assert!(msg.contains("weder result noch error"));
            }
            other => panic!("Expected Internal error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_dispatch_to_slm_fallback_result_format() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                use tokio::io::AsyncWriteExt;
                let response_body = json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": {
                        "status": "processed",
                        "code": 200
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
            "fallback-slm",
            format!("http://{}", addr),
            vec![],
            TokenBudget::new(100, 10),
            0.0,
        );

        let decision = RoutingDecision {
            profile,
            context: ContextWindow {
                chunks: vec![],
                total_tokens: 0,
                truncated: false,
            },
        };

        let answer = dispatch_to_slm(&decision).await.expect("dispatch ok");
        assert!(answer.contains("\"code\":200"));
        assert!(answer.contains("\"status\":\"processed\""));
    }
}
