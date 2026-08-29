//! Unit tests for memfuse-router.

#[cfg(test)]
mod tests {
    use crate::{dispatch_to_slm, RouterEngine, RoutingDecision, SlmProfile};
    use memfuse_core::{EntityId, MemFuseError, StorageEngine, TokenBudget};
    use memfuse_db::{MemFuse, MemFuseConfig};
    use serde_json::json;
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
                Some(json!({"text": "function rust_code() { return 42; }"}).into()),
            )
            .await
            .unwrap();

        collection
            .insert(
                docs_key,
                &vec_docs,
                Some(json!({"text": "Dokumentation über Unternehmensrichtlinien."}).into()),
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
                Some(json!({"text": "unrelated content"}).into()),
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
}
