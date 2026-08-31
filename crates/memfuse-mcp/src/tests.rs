#[cfg(test)]
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
    create_mock_server_with_write(true).await
}

async fn create_mock_server_with_write(allow_db_writes: bool) -> (Arc<McpServer>, TempDir) {
    let tmp = TempDir::new().expect("temp dir"); // expect
    let db = MemFuse::open(tmp.path()).await.expect("open db"); // expect
    let collection = db.collection("default").await.expect("collection"); // expect
    let dim = collection.dimension();
    let embedder = Arc::new(MockEmbedder { dimension: dim });
    let server = Arc::new(
        McpServer::with_write_permission(Arc::new(db), embedder, allow_db_writes)
            .expect("server new"),
    );
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
async fn test_tools_list_returns_all_tools() {
    let (server, _tmp) = create_mock_server().await;
    let req = make_request("tools/list", json!({}));
    let response = server.handle(req).await;
    assert_eq!(response.jsonrpc, "2.0");
    let tools = response.result.unwrap()["tools"] // unwrap
        .as_array()
        .unwrap() // unwrap
        .clone();
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
    assert_eq!(err_resp.error.unwrap().code, -32700); // unwrap
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
    assert_eq!(err_resp.error.unwrap().code, -32600); // unwrap
}

#[tokio::test]
async fn test_request_id_echo_roundtrip() {
    let (server, _tmp) = create_mock_server().await;

    // String ID
    let req1 = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!("abc-123")),
        method: "ping".into(),
        params: json!({}),
    };
    let resp1 = server.handle(req1).await;
    assert_eq!(resp1.id, Some(json!("abc-123")));
    assert_eq!(resp1.jsonrpc, "2.0");

    // Numeric ID
    let req2 = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(42)),
        method: "ping".into(),
        params: json!({}),
    };
    let resp2 = server.handle(req2).await;
    assert_eq!(resp2.id, Some(json!(42)));
    assert_eq!(resp2.jsonrpc, "2.0");
}

#[tokio::test]
async fn test_unknown_method_returns_method_not_found() {
    let (server, _tmp) = create_mock_server().await;
    let req = make_request("nonexistent/method", json!({}));
    let response = server.handle(req).await;
    assert_eq!(response.jsonrpc, "2.0");
    assert_eq!(response.id, Some(json!(1)));
    let err = response.error.expect("error object expected"); // expect
    assert_eq!(err.code, -32601);
}

#[tokio::test]
async fn test_missing_required_param_returns_invalid_params_32602() {
    let (server, _tmp) = create_mock_server().await;
    let req = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(101)),
        method: "memfuse_insert".into(),
        params: json!({
            // missing required "id" field
            "collection": "default",
            "text": "some text"
        }),
    };
    let response = server.handle(req).await;
    assert_eq!(response.id, Some(json!(101)));
    let err = response
        .error
        .expect("error expected for missing required param"); // expect
    assert_eq!(err.code, -32602);
    assert!(err.message.contains("id"));
}

#[tokio::test]
async fn test_internal_error_returns_32603() {
    use crate::protocol::{response_from_error, McpError};
    let err = McpError::internal_error("storage layer failure");
    assert_eq!(err.code(), -32603);
    let resp = response_from_error(Some(json!(102)), err);
    assert_eq!(resp.id, Some(json!(102)));
    let err_obj = resp.error.expect("error expected"); // expect
    assert_eq!(err_obj.code, -32603);
    assert_eq!(err_obj.message, "storage layer failure");
}

#[tokio::test]
async fn test_notification_expects_no_response() {
    let (server, _tmp) = create_mock_server().await;
    let req = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: None,
        method: "initialized".into(),
        params: json!({}),
    };
    assert!(req.id.is_none());
    let response = server.handle(req).await;
    assert_eq!(response.id, None);
}

#[tokio::test]
async fn test_read_line_bounded_enforces_limit() {
    use crate::{read_line_bounded, MAX_RPC_BYTES};
    use std::io::Cursor;
    use tokio::io::BufReader;

    // 1. Normal line within limit
    let data = "{\"jsonrpc\":\"2.0\",\"id\":1}\n";
    let mut reader = BufReader::new(Cursor::new(data));
    let mut buf = String::new();
    let res = read_line_bounded(&mut reader, &mut buf, MAX_RPC_BYTES).await;
    assert!(res.is_ok());
    assert_eq!(buf, data);

    // 2. Line exceeding limit (e.g. 100 bytes when limit is 50)
    let oversized = "A".repeat(100) + "\n";
    let mut oversized_reader = BufReader::new(Cursor::new(oversized));
    let mut buf2 = String::new();
    let res_err = read_line_bounded(&mut oversized_reader, &mut buf2, 50).await;
    assert!(res_err.is_err());
    let err = res_err.unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("limit exceeded"));
}

#[tokio::test]
async fn test_stdout_not_polluted_by_logs() {
    let source = std::fs::read_to_string("src/lib.rs")
        .or_else(|_| std::fs::read_to_string("crates/memfuse-mcp/src/lib.rs"))
        .expect("read lib.rs"); // expect
    let bin_source = std::fs::read_to_string("src/bin/memfuse-mcp-server.rs")
        .or_else(|_| std::fs::read_to_string("crates/memfuse-mcp/src/bin/memfuse-mcp-server.rs"))
        .expect("read bin"); // expect

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

#[tokio::test]
async fn test_search_validates_empty_or_oversized_query() {
    let (server, _tmp) = create_mock_server().await;

    // Whitespace query
    let req_whitespace = make_request("memfuse_search", json!({"query": "   "}));
    let resp = server.handle(req_whitespace).await;
    let err = resp.error.expect("error expected for empty query");
    assert_eq!(err.code, -32602);
    assert!(err.message.contains("query cannot be empty"));

    // Oversized query
    let huge_query = "x".repeat(crate::MAX_SEARCH_QUERY_BYTES + 1);
    let req_huge = make_request("memfuse_search", json!({"query": huge_query}));
    let resp_huge = server.handle(req_huge).await;
    let err_huge = resp_huge.error.expect("error expected for oversized query");
    assert_eq!(err_huge.code, -32602);
    assert!(err_huge.message.contains("query size exceeds limit"));
}

#[tokio::test]
async fn test_insert_validates_vector_nan_inf_and_empty() {
    let (server, _tmp) = create_mock_server().await;

    // Empty vector
    let req_empty_vec = make_request(
        "memfuse_insert",
        json!({
            "id": "doc1",
            "vector": []
        }),
    );
    let resp = server.handle(req_empty_vec).await;
    let err = resp.error.expect("error expected for empty vector");
    assert_eq!(err.code, -32602);
    assert!(err.message.contains("vector cannot be empty"));

    // Vector with float value that overflows f32 (e.g. 1.0e39)
    let req_inf = make_request(
        "memfuse_insert",
        json!({
            "id": "doc2",
            "vector": [0.1, 1.0e39]
        }),
    );
    let resp_inf = server.handle(req_inf).await;
    let err_inf = resp_inf.error.expect("error expected for Inf in vector");
    assert_eq!(err_inf.code, -32602);
    assert!(err_inf.message.contains("NaN or Inf"));
}

#[tokio::test]
async fn test_insert_validates_oversized_id() {
    let (server, _tmp) = create_mock_server().await;

    let long_id = "i".repeat(257);
    let req = make_request(
        "memfuse_insert",
        json!({
            "id": long_id,
            "text": "hello world"
        }),
    );
    let resp = server.handle(req).await;
    let err = resp.error.expect("error expected for long ID");
    assert_eq!(err.code, -32602);
    assert!(err.message.contains("id length exceeds limit"));
}

#[tokio::test]
async fn test_whitespace_collection_name_fallback_or_rejection() {
    let (server, _tmp) = create_mock_server().await;

    // "   " as collection defaults to "default"
    let req = make_request(
        "memfuse_search",
        json!({
            "query": "find me",
            "collection": "   "
        }),
    );
    let resp = server.handle(req).await;
    assert!(resp.result.is_some());
}

#[tokio::test]
async fn test_write_tool_rejected_when_read_only() {
    let (server, _tmp) = create_mock_server_with_write(false).await;

    let write_tools = [
        "memfuse_insert",
        "memfuse_delete",
        "memfuse_upsert",
        "memfuse_relate",
        "memfuse_create_collection",
        "memfuse_drop_collection",
    ];

    for tool in write_tools {
        let req = make_request(
            "tools/call",
            json!({
                "name": tool,
                "arguments": {
                    "id": "test_id",
                    "text": "test_text"
                }
            }),
        );
        let resp = server.handle(req).await;
        let res_val = serde_json::to_value(&resp).unwrap();
        assert_eq!(res_val["result"]["isError"], true);
        let text = res_val["result"]["content"][0]["text"].as_str().unwrap();
        assert!(
            text.contains("Sandbox: DB-Schreibzugriff gesperrt"),
            "Expected write rejection for '{tool}', got: '{text}'"
        );
    }
}

#[tokio::test]
async fn test_write_tool_allowed_when_explicitly_enabled() {
    let (server, _tmp) = create_mock_server_with_write(true).await;

    let req = make_request(
        "tools/call",
        json!({
            "name": "memfuse_insert",
            "arguments": {
                "id": "write_enabled_doc",
                "text": "Write enabled content"
            }
        }),
    );
    let resp = server.handle(req).await;
    let res_val = serde_json::to_value(&resp).unwrap();
    assert_ne!(res_val["result"]["isError"], true);
    let text = res_val["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("write_enabled_doc"));
}

#[tokio::test]
async fn test_read_tools_always_allowed_regardless_of_flag() {
    let (server_ro, _tmp1) = create_mock_server_with_write(false).await;
    let (server_rw, _tmp2) = create_mock_server_with_write(true).await;

    let read_req = make_request(
        "tools/call",
        json!({
            "name": "memfuse_collections",
            "arguments": {}
        }),
    );

    let resp_ro = server_ro.handle(read_req.clone()).await;
    let res_ro = serde_json::to_value(&resp_ro).unwrap();
    assert_ne!(res_ro["result"]["isError"], true);

    let resp_rw = server_rw.handle(read_req).await;
    let res_rw = serde_json::to_value(&resp_rw).unwrap();
    assert_ne!(res_rw["result"]["isError"], true);
}

#[test]
fn test_mcp_error_constructors_and_codes() {
    use crate::protocol::McpError;

    let parse_err = McpError::parse_error("syntax error");
    assert_eq!(parse_err.code(), -32700);
    assert_eq!(parse_err.to_string(), "syntax error");

    let invalid_req = McpError::invalid_request("bad request");
    assert_eq!(invalid_req.code(), -32600);
    assert_eq!(invalid_req.to_string(), "bad request");

    let method_not_found = McpError::method_not_found("unknown method");
    assert_eq!(method_not_found.code(), -32601);
    assert_eq!(method_not_found.to_string(), "unknown method");

    let invalid_params = McpError::invalid_params("bad params");
    assert_eq!(invalid_params.code(), -32602);
    assert_eq!(invalid_params.to_string(), "bad params");

    let internal_err = McpError::internal_error("system fault");
    assert_eq!(internal_err.code(), -32603);
    assert_eq!(internal_err.to_string(), "system fault");
}

#[test]
fn test_mcp_error_from_conversions() {
    use crate::protocol::McpError;
    use memfuse_core::MemFuseError;

    // MemFuseError::InvalidInput -> InvalidParams (-32602)
    let err_input = MemFuseError::InvalidInput("invalid key".into());
    let mcp_input: McpError = err_input.into();
    assert_eq!(mcp_input.code(), -32602);
    assert_eq!(mcp_input.to_string(), "invalid key");

    // MemFuseError::NotFound -> InvalidParams (-32602)
    let err_nf = MemFuseError::NotFound("missing doc".into());
    let mcp_nf: McpError = err_nf.into();
    assert_eq!(mcp_nf.code(), -32602);
    assert_eq!(mcp_nf.to_string(), "missing doc");

    // MemFuseError::Internal -> InternalError (-32603)
    let err_int = MemFuseError::Internal("crash".into());
    let mcp_int: McpError = err_int.into();
    assert_eq!(mcp_int.code(), -32603);

    // String -> InvalidParams (-32602)
    let mcp_str: McpError = String::from("string error").into();
    assert_eq!(mcp_str.code(), -32602);
    assert_eq!(mcp_str.to_string(), "string error");

    // &str -> InvalidParams (-32602)
    let mcp_str_ref: McpError = "str ref error".into();
    assert_eq!(mcp_str_ref.code(), -32602);
    assert_eq!(mcp_str_ref.to_string(), "str ref error");
}

#[test]
fn test_response_from_error_helper() {
    use crate::protocol::{response_from_error, McpError};

    let id = Some(json!("req_42"));
    let err = McpError::invalid_params("missing query");
    let resp = response_from_error(id.clone(), err);

    assert_eq!(resp.jsonrpc, "2.0");
    assert_eq!(resp.id, id);
    let rpc_err = resp.error.expect("error object");
    assert_eq!(rpc_err.code, -32602);
    assert_eq!(rpc_err.message, "missing query");
}

#[test]
fn test_jsonrpc_struct_serialization_roundtrip() {
    use crate::protocol::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};

    // 1. JsonRpcRequest roundtrip
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!(123)),
        method: "memfuse_search".to_string(),
        params: json!({"query": "test query"}),
    };
    let json_req = serde_json::to_string(&req).expect("serialize req");
    let deserialized_req: JsonRpcRequest =
        serde_json::from_str(&json_req).expect("deserialize req");
    assert_eq!(deserialized_req.jsonrpc, "2.0");
    assert_eq!(deserialized_req.id, Some(json!(123)));
    assert_eq!(deserialized_req.method, "memfuse_search");
    assert_eq!(deserialized_req.params["query"], "test query");

    // 2. JsonRpcResponse (success) roundtrip
    let resp_ok = JsonRpcResponse::ok(Some(json!("id_1")), json!({"ok": true}));
    let json_resp_ok = serde_json::to_string(&resp_ok).expect("serialize ok resp");
    let deserialized_ok: JsonRpcResponse =
        serde_json::from_str(&json_resp_ok).expect("deserialize ok resp");
    assert_eq!(deserialized_ok.jsonrpc, "2.0");
    assert_eq!(deserialized_ok.id, Some(json!("id_1")));
    assert_eq!(deserialized_ok.result.expect("result"), json!({"ok": true}));
    assert!(deserialized_ok.error.is_none());

    // 3. JsonRpcResponse (error) roundtrip
    let resp_err = JsonRpcResponse::err(Some(json!(99)), -32601, "Method not found");
    let json_resp_err = serde_json::to_string(&resp_err).expect("serialize err resp");
    let deserialized_err: JsonRpcResponse =
        serde_json::from_str(&json_resp_err).expect("deserialize err resp");
    assert_eq!(deserialized_err.jsonrpc, "2.0");
    assert_eq!(deserialized_err.id, Some(json!(99)));
    assert!(deserialized_err.result.is_none());
    let err = deserialized_err.error.expect("error object");
    assert_eq!(err.code, -32601);
    assert_eq!(err.message, "Method not found");

    // 4. Standalone JsonRpcError serialization roundtrip
    let rpc_err_obj = JsonRpcError {
        code: -32700,
        message: "Parse error".to_string(),
        data: Some(json!({"details": "syntax"})),
    };
    let json_rpc_err = serde_json::to_string(&rpc_err_obj).expect("serialize err obj");
    let deserialized_rpc_err: JsonRpcError =
        serde_json::from_str(&json_rpc_err).expect("deserialize err obj");
    assert_eq!(deserialized_rpc_err.code, -32700);
    assert_eq!(deserialized_rpc_err.message, "Parse error");
    assert_eq!(
        deserialized_rpc_err.data,
        Some(json!({"details": "syntax"}))
    );
}

#[tokio::test]
async fn test_read_line_bounded_edge_cases() {
    use crate::read_line_bounded;
    use std::io::Cursor;
    use tokio::io::BufReader;

    // 1. EOF (empty reader)
    let mut reader_eof = BufReader::new(Cursor::new(""));
    let mut buf = String::new();
    let res_eof = read_line_bounded(&mut reader_eof, &mut buf, 100).await;
    assert!(res_eof.is_ok());
    assert_eq!(res_eof.unwrap(), 0);
    assert!(buf.is_empty());

    // 2. Exact boundary fit
    let exact_data = "123456789\n"; // 10 bytes
    let mut reader_exact = BufReader::new(Cursor::new(exact_data));
    let res_exact = read_line_bounded(&mut reader_exact, &mut buf, 10).await;
    assert!(res_exact.is_ok());
    assert_eq!(res_exact.unwrap(), 10);
    assert_eq!(buf, exact_data);

    // 3. CRLF line ending
    let crlf_data = "hello world\r\n";
    let mut reader_crlf = BufReader::new(Cursor::new(crlf_data));
    let res_crlf = read_line_bounded(&mut reader_crlf, &mut buf, 100).await;
    assert!(res_crlf.is_ok());
    assert_eq!(buf, crlf_data);

    // 4. Invalid UTF-8 input
    let invalid_utf8 = vec![0x61, 0x62, 0xFF, 0xFE, 0x0A]; // ab<invalid>\n
    let mut reader_invalid = BufReader::new(Cursor::new(invalid_utf8));
    let res_invalid = read_line_bounded(&mut reader_invalid, &mut buf, 100).await;
    assert!(res_invalid.is_err());
    let err = res_invalid.unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("Invalid UTF-8"));
}

#[test]
fn test_is_write_allowed_by_env_values() {
    use crate::is_write_allowed_by_env;

    let key = "MEMFUSE_MCP_ALLOW_WRITE";

    std::env::set_var(key, "1");
    assert!(is_write_allowed_by_env());

    std::env::set_var(key, "true");
    assert!(is_write_allowed_by_env());

    std::env::set_var(key, "YES");
    assert!(is_write_allowed_by_env());

    std::env::set_var(key, "0");
    assert!(!is_write_allowed_by_env());

    std::env::set_var(key, "false");
    assert!(!is_write_allowed_by_env());

    std::env::remove_var(key);
    assert!(!is_write_allowed_by_env());
}

#[tokio::test]
async fn test_mcp_server_constructors_and_lifecycle() {
    let tmp = TempDir::new().expect("temp dir");
    let db = Arc::new(MemFuse::open(tmp.path()).await.expect("open db"));
    let col = db.collection("default").await.expect("col");
    let embedder: Arc<dyn TextEmbeddingEngine> = Arc::new(MockEmbedder {
        dimension: col.dimension(),
    });

    // 1. McpServer::new
    let server_default = McpServer::new(db.clone(), embedder.clone()).expect("server new");
    assert!(!server_default.sandbox.policy().allow_db_writes);

    // 2. McpServer::with_sandbox
    let policy = crate::sandbox::SandboxPolicy {
        allow_db_reads: true,
        allow_db_writes: true,
        allow_code_execution: false,
        max_execution_ms: 2000,
    };
    let sandbox = Arc::new(crate::sandbox::McpSandbox::new(policy).expect("sandbox"));
    let server_custom = McpServer::with_sandbox(db.clone(), embedder.clone(), sandbox);
    assert!(server_custom.sandbox.policy().allow_db_writes);

    // 3. Lifecycle handle: initialize
    let req_init = make_request("initialize", json!({}));
    let resp_init = server_custom.handle(req_init).await;
    assert_eq!(resp_init.jsonrpc, "2.0");
    let res_val = resp_init.result.expect("result");
    assert_eq!(res_val["protocolVersion"], "2024-11-05");
    assert_eq!(res_val["serverInfo"]["name"], "memfuse");

    // 4. Lifecycle handle: ping
    let req_ping = make_request("ping", json!({}));
    let resp_ping = server_custom.handle(req_ping).await;
    assert_eq!(resp_ping.result.expect("result"), json!({}));
}

#[tokio::test]
async fn test_tools_call_missing_or_empty_name() {
    let (server, _tmp) = create_mock_server().await;

    // Missing tool name
    let req_missing = make_request("tools/call", json!({"arguments": {}}));
    let resp_missing = server.handle(req_missing).await;
    let err = resp_missing.error.expect("error");
    assert_eq!(err.code, -32602);
    assert!(err.message.contains("missing or empty tool 'name'"));

    // Empty tool name
    let req_empty = make_request("tools/call", json!({"name": "", "arguments": {}}));
    let resp_empty = server.handle(req_empty).await;
    let err_empty = resp_empty.error.expect("error");
    assert_eq!(err_empty.code, -32602);
    assert!(err_empty.message.contains("missing or empty tool 'name'"));
}

#[tokio::test]
async fn test_memfuse_insert_raw_vector_and_missing_payload() {
    let (server, _tmp) = create_mock_server_with_write(true).await;
    let col = server.db.collection("default").await.expect("collection");
    let dim = col.dimension();
    let raw_vector = vec![0.1f32; dim];

    // Direct vector insert (no text)
    let req_vector_only = make_request(
        "memfuse_insert",
        json!({
            "id": "raw_vec_doc",
            "vector": raw_vector,
            "collection": "default"
        }),
    );
    let resp_vec = server.handle(req_vector_only).await;
    assert!(
        resp_vec.error.is_none(),
        "Unexpected error: {:?}",
        resp_vec.error
    );
    let res_val = resp_vec.result.expect("result");
    assert_eq!(res_val["ok"], true);
    assert_eq!(res_val["chunks_inserted"], 1);
    assert_eq!(res_val["id"], "raw_vec_doc");

    // Missing both vector and text
    let req_missing_both = make_request(
        "memfuse_insert",
        json!({
            "id": "no_payload_doc",
            "collection": "default"
        }),
    );
    let resp_missing = server.handle(req_missing_both).await;
    let err = resp_missing.error.expect("error for missing payload");
    assert_eq!(err.code, -32602);
    assert!(err.message.contains("text/vector fehlt"));
}

#[tokio::test]
async fn test_memfuse_get_nonexistent_and_unicode() {
    let (server, _tmp) = create_mock_server_with_write(true).await;

    // Get non-existent document returns null
    let req_get_missing = make_request(
        "memfuse_get",
        json!({
            "id": "non_existent_doc_id_999",
            "collection": "default"
        }),
    );
    let resp_missing = server.handle(req_get_missing).await;
    assert!(resp_missing.error.is_none());
    assert_eq!(resp_missing.result.expect("result"), json!(null));

    // Multi-byte Unicode insertion & retrieval (🚀 Emoji, German Umlauts, Japanese text)
    let unicode_id = "doc_unicode_🚀_日本語";
    let unicode_text = "MemFuse Gedächtnis mit 日本語 Text und Emojis 🦀🚀✨";

    let req_insert = make_request(
        "memfuse_insert",
        json!({
            "id": unicode_id,
            "text": unicode_text,
            "collection": "default"
        }),
    );
    let resp_insert = server.handle(req_insert).await;
    assert!(resp_insert.error.is_none());

    let req_get = make_request(
        "memfuse_get",
        json!({
            "id": unicode_id,
            "collection": "default"
        }),
    );
    let resp_get = server.handle(req_get).await;
    assert!(resp_get.error.is_none());
    let get_res = resp_get.result.expect("result");
    assert_eq!(get_res["metadata"]["text"], unicode_text);
}
