use async_trait::async_trait;
use memfuse_core::{Result, TextEmbeddingEngine};
use memfuse_db::MemFuse;
use memfuse_mcp::{protocol::JsonRpcRequest, McpServer};
use serde_json::json;
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

async fn setup_app() -> (McpServer, TempDir) {
    let tmp = TempDir::new().expect("temp dir");
    let db = MemFuse::open(tmp.path()).await.expect("open db");
    let collection = db.collection("my_docs").await.expect("collection");
    let dim = collection.dimension();
    let embedder = Arc::new(MockEmbedder { dimension: dim });
    let server = McpServer::new(Arc::new(db), embedder);
    (server, tmp)
}

#[tokio::test]
async fn test_list_tools() {
    let (server, _tmp) = setup_app().await;

    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!(1)),
        method: "tools/list".to_string(),
        params: json!({}),
    };

    let response = server.handle(req).await;
    let res_val = serde_json::to_value(&response).unwrap();
    let tools = res_val["result"]["tools"].as_array().expect("tools array");
    let tool_names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();

    assert!(tool_names.contains(&"memfuse_search"));
    assert!(tool_names.contains(&"memfuse_get"));
    assert!(tool_names.contains(&"memfuse_insert"));
    assert!(tool_names.contains(&"memfuse_collections"));
}

#[tokio::test]
async fn test_mcp_flow_insert_get_search_collections() {
    let (server, _tmp) = setup_app().await;

    // 1. Insert document
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!(1)),
        method: "tools/call".to_string(),
        params: json!({
            "name": "memfuse_insert",
            "arguments": {
                "id": "doc1",
                "text": "Rust and MCP server integration",
                "collection": "my_docs",
                "metadata": { "author": "Jules" }
            }
        }),
    };

    let response = server.handle(req).await;
    let res_val = serde_json::to_value(&response).unwrap();
    let text = res_val["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("doc1"));

    // 2. Get document
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!(2)),
        method: "tools/call".to_string(),
        params: json!({
            "name": "memfuse_get",
            "arguments": {
                "id": "doc1",
                "collection": "my_docs"
            }
        }),
    };

    let response = server.handle(req).await;
    let res_val = serde_json::to_value(&response).unwrap();
    let text = res_val["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("doc1"));

    // 3. Search document
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!(3)),
        method: "tools/call".to_string(),
        params: json!({
            "name": "memfuse_search",
            "arguments": {
                "query": "Rust integration",
                "collection": "my_docs",
                "k": 5
            }
        }),
    };

    let response = server.handle(req).await;
    let res_val = serde_json::to_value(&response).unwrap();
    let text = res_val["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("doc1"));

    // 4. List collections
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!(4)),
        method: "tools/call".to_string(),
        params: json!({
            "name": "memfuse_collections",
            "arguments": {}
        }),
    };

    let response = server.handle(req).await;
    let res_val = serde_json::to_value(&response).unwrap();
    let text = res_val["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("my_docs"));
}
