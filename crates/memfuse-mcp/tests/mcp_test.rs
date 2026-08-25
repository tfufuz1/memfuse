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

#[tokio::test]
async fn test_invalid_tool_name() {
    let (server, _tmp) = setup_app().await;

    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!(5)),
        method: "tools/call".to_string(),
        params: json!({
            "name": "memfuse_does_not_exist",
            "arguments": {}
        }),
    };

    let response = server.handle(req).await;
    let res_val = serde_json::to_value(&response).unwrap();
    let is_error = res_val["result"]["isError"].as_bool().unwrap_or(false);
    assert!(is_error);

    let text = res_val["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("Unbekanntes Tool"));
}

#[tokio::test]
async fn test_missing_arguments() {
    let (server, _tmp) = setup_app().await;

    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!(6)),
        method: "tools/call".to_string(),
        params: json!({
            "name": "memfuse_insert",
            "arguments": {
                // missing "id" and "text"
                "collection": "my_docs"
            }
        }),
    };

    let response = server.handle(req).await;
    let res_val = serde_json::to_value(&response).unwrap();
    let is_error = res_val["result"]["isError"].as_bool().unwrap_or(false);
    assert!(is_error);

    let text = res_val["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("id fehlt") || text.contains("text fehlt"));
}

// ANCHOR[TEST:MCP-002] STATUS:IN-PROGRESS — Error-Path Coverage
#[tokio::test]
async fn test_malformed_request_returns_error() {
    // TESTZWECK: Fehlende Pflichtparameter müssen Fehlermeldung erzeugen
    // REFERENZWERT: JSON-RPC 2.0 Spec — Fehlerfall hat "error"-Feld
    let (server, _tmp) = setup_app().await;
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!(99)),
        method: "tools/call".to_string(),
        params: json!({
            "name": "memfuse_insert",
            "arguments": { "id": "doc_without_text", "collection": "test" }
            // "text" fehlt absichtlich
        }),
    };
    let response = server.handle(req).await;
    let val = serde_json::to_value(&response).unwrap();
    assert!(
        val.get("error").is_some()
            || val["result"]["content"][0]["text"]
                .as_str()
                .unwrap_or("")
                .contains("error")
            || val["result"]["isError"].as_bool().unwrap_or(false),
        "Fehlender 'text'-Parameter muss Fehlermeldung erzeugen: {val}"
    );
}

#[tokio::test]
async fn test_unknown_tool_returns_error() {
    // TESTZWECK: Unbekannte Tool-Namen müssen Fehler zurückgeben
    let (server, _tmp) = setup_app().await;
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!(42)),
        method: "tools/call".to_string(),
        params: json!({ "name": "nonexistent_tool_xyz_abc", "arguments": {} }),
    };
    let response = server.handle(req).await;
    let val = serde_json::to_value(&response).unwrap();
    let text = val["result"]["content"][0]["text"].as_str().unwrap_or("");
    assert!(
        text.contains("Unknown")
            || text.contains("Unbekanntes")
            || text.contains("not found")
            || text.contains("error")
            || val["result"]["isError"].as_bool().unwrap_or(false),
        "Unbekanntes Tool muss Fehler zurückgeben, got: {text}"
    );
}

#[tokio::test]
async fn test_mcp_insert_multi_chunk_document() {
    let (server, _tmp) = setup_app().await;

    // Create a multi-heading document with enough text per section to exceed minimum token threshold (> 50 tokens)
    let paragraph_a =
        "Dies ist der erste Abschnitt eines längeren Dokuments über künstliche Intelligenz. "
            .repeat(10);
    let paragraph_b =
        "Dies ist der zweite Abschnitt über maschinelles Lernen und Neuronale Netze. ".repeat(10);
    let markdown_doc = format!(
        "# Abschnitt 1: KI-Grundlagen\n{}\n\n## Abschnitt 2: Deep Learning\n{}",
        paragraph_a, paragraph_b
    );

    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!(10)),
        method: "tools/call".to_string(),
        params: json!({
            "name": "memfuse_insert",
            "arguments": {
                "id": "doc_multi",
                "text": markdown_doc,
                "collection": "my_docs",
                "metadata": { "department": "R&D" }
            }
        }),
    };

    let response = server.handle(req).await;
    let res_val = serde_json::to_value(&response).unwrap();
    let text = res_val["result"]["content"][0]["text"].as_str().unwrap();

    let insert_res: serde_json::Value = serde_json::from_str(text).unwrap();
    assert_eq!(insert_res["ok"], true);
    assert_eq!(insert_res["id"], "doc_multi");

    let chunks_inserted = insert_res["chunks_inserted"].as_u64().unwrap();
    assert!(
        chunks_inserted > 1,
        "Expected document to be split into multiple chunks, got {chunks_inserted}"
    );

    let chunk_ids = insert_res["chunk_ids"].as_array().unwrap();
    assert_eq!(chunk_ids.len(), chunks_inserted as usize);
    assert_eq!(chunk_ids[0], "doc_multi:chunk:0");

    // Retrieve chunk 0 via memfuse_get
    let req_get = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!(11)),
        method: "tools/call".to_string(),
        params: json!({
            "name": "memfuse_get",
            "arguments": {
                "id": "doc_multi:chunk:0",
                "collection": "my_docs"
            }
        }),
    };

    let get_response = server.handle(req_get).await;
    let get_val = serde_json::to_value(&get_response).unwrap();
    let get_text = get_val["result"]["content"][0]["text"].as_str().unwrap();
    let chunk_doc: serde_json::Value = serde_json::from_str(get_text).unwrap();

    assert_eq!(chunk_doc["metadata"]["source_id"], "doc_multi");
    assert_eq!(chunk_doc["metadata"]["department"], "R&D");
    assert_eq!(chunk_doc["metadata"]["chunk_index"], 0);
}
