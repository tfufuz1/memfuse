#[cfg(test)]
mod tests {
    use crate::protocol::{JsonRpcRequest, JsonRpcResponse};
    use crate::McpServer;
    use async_trait::async_trait;
    use memfuse_core::{Result, TextEmbeddingEngine};
    use memfuse_db::MemFuse;
    use serde_json::{json, Value};
    use std::sync::Arc;
    use tempfile::TempDir;

    #[derive(Debug)]
    struct MockEmbedder {
        dimension: usize,
    }

    #[async_trait]
    impl TextEmbeddingEngine for MockEmbedder {
        async fn embed(&self, _text: &str) -> Result<Vec<f32>> {
            Ok(vec![0.1f32; self.dimension])
        }

        async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
            Ok(vec![vec![0.1f32; self.dimension]; texts.len()])
        }
    }

    async fn create_mock_server() -> (Arc<McpServer>, TempDir) {
        let tmp = TempDir::new().expect("temp dir");
        let db = MemFuse::open(tmp.path()).await.expect("open db");
        let collection = db.collection("default").await.expect("collection");
        let dim = collection.dimension();
        let embedder = Arc::new(MockEmbedder { dimension: dim });
        let server = Arc::new(McpServer::new(Arc::new(db), embedder));
        (server, tmp)
    }

    fn make_request(method: &str, params: Value) -> JsonRpcRequest {
        JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(json!(1)),
            method: method.into(),
            params,
        }
    }

    #[tokio::test]
    async fn test_initialize_returns_protocol_version() {
        let (server, _tmp) = create_mock_server().await;
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            method: "initialize".into(),
            id: Some(json!(1)),
            params: json!({}),
        };
        let response = server.handle(req).await;
        assert_eq!(response.jsonrpc, "2.0");
        let result = response.result.unwrap();
        assert_eq!(result["protocolVersion"], "2024-11-05");
        assert!(result["capabilities"]["tools"].is_object());
    }

    #[tokio::test]
    async fn test_tools_list_returns_all_tools() {
        let (server, _tmp) = create_mock_server().await;
        let req = make_request("tools/list", json!({}));
        let response = server.handle(req).await;
        assert_eq!(response.jsonrpc, "2.0");
        let tools = response.result.unwrap()["tools"].as_array().unwrap().clone();
        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert!(names.contains(&"memfuse_search"));
        assert!(names.contains(&"memfuse_insert"));
        assert!(names.contains(&"memfuse_get"));
        assert!(names.contains(&"memfuse_collections"));
    }

    #[tokio::test]
    async fn test_malformed_json_returns_parse_error() {
        let parse_res = serde_json::from_str::<Value>("{ invalid }");
        assert!(parse_res.is_err());
        let err_resp = JsonRpcResponse::err(None, -32700, "Parse error");
        assert_eq!(err_resp.jsonrpc, "2.0");
        assert_eq!(err_resp.error.unwrap().code, -32700);
    }

    #[tokio::test]
    async fn test_invalid_rpc_request_returns_invalid_request_code() {
        let invalid_rpc = json!({
            "id": 1,
            "method": "initialize"
            // "jsonrpc": "2.0" is missing
        });
        let req_res = serde_json::from_value::<JsonRpcRequest>(invalid_rpc);
        assert!(req_res.is_err());
        let err_resp = JsonRpcResponse::err(Some(json!(1)), -32600, "Invalid Request");
        assert_eq!(err_resp.jsonrpc, "2.0");
        assert_eq!(err_resp.error.unwrap().code, -32600);
    }

    #[tokio::test]
    async fn test_unknown_method_returns_method_not_found() {
        let (server, _tmp) = create_mock_server().await;
        let req = make_request("nonexistent/method", json!({}));
        let response = server.handle(req).await;
        assert_eq!(response.jsonrpc, "2.0");
        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, -32601);
    }

    #[tokio::test]
    async fn test_tools_call_missing_name_returns_invalid_params() {
        let (server, _tmp) = create_mock_server().await;
        let req = make_request("tools/call", json!({ "arguments": {} }));
        let response = server.handle(req).await;
        assert_eq!(response.jsonrpc, "2.0");
        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, -32602);
    }

    #[tokio::test]
    async fn test_insert_and_search_roundtrip() {
        let (server, _tmp) = create_mock_server().await;
        // Insert
        let insert_req = make_request(
            "tools/call",
            json!({
                "name": "memfuse_insert",
                "arguments": {"id": "test-doc", "text": "MemFuse is a database"}
            }),
        );
        let insert_resp = server.handle(insert_req).await;
        assert_eq!(insert_resp.jsonrpc, "2.0");
        assert!(insert_resp.error.is_none());

        // Search
        let search_req = make_request(
            "tools/call",
            json!({
                "name": "memfuse_search",
                "arguments": {"query": "database", "limit": 5}
            }),
        );
        let search_resp = server.handle(search_req).await;
        assert_eq!(search_resp.jsonrpc, "2.0");
        assert!(search_resp.error.is_none());
    }

    #[tokio::test]
    async fn test_stdout_not_polluted_by_logs() {
        let source = std::fs::read_to_string("src/lib.rs")
            .or_else(|_| std::fs::read_to_string("crates/memfuse-mcp/src/lib.rs"))
            .expect("read lib.rs");
        let bin_source = std::fs::read_to_string("src/bin/memfuse-mcp-server.rs")
            .or_else(|_| std::fs::read_to_string("crates/memfuse-mcp/src/bin/memfuse-mcp-server.rs"))
            .expect("read bin");

        let stdout_writes = source
            .lines()
            .chain(bin_source.lines())
            .filter(|line| !line.trim().starts_with("//"))
            .filter(|line| line.contains("println!") || line.contains("print!"))
            .count();

        assert_eq!(
            stdout_writes, 0,
            "No println! or print! allowed in memfuse-mcp — use stderr/tracing"
        );
    }
}
