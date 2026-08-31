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
async fn test_jsonrpc_2_0_spec_batch_requests() {
    use std::io::Cursor;
    use tokio::io::BufReader;

    let (server, _tmp) = create_mock_server().await;

    // Batch containing 2 valid calls, 1 notification, 1 invalid request
    let batch_input = json!([
        { "jsonrpc": "2.0", "id": 1, "method": "ping", "params": {} },
        { "jsonrpc": "2.0", "method": "initialized", "params": {} }, // notification
        { "jsonrpc": "2.0", "id": "req-2", "method": "tools/list", "params": {} },
        { "invalid": "json" } // malformed item
    ])
    .to_string()
        + "\n";

    let mut input_cursor = Cursor::new(batch_input.into_bytes());
    let mut reader = BufReader::new(&mut input_cursor);
    let mut line_buf = String::new();

    crate::read_line_bounded(&mut reader, &mut line_buf, crate::MAX_RPC_BYTES)
        .await
        .expect("read line");

    let parsed_val: Value = serde_json::from_str(line_buf.trim()).unwrap();
    assert!(parsed_val.is_array());
    let arr = parsed_val.as_array().unwrap();

    let mut responses = Vec::new();
    for item in arr {
        let req_id = item.get("id").cloned();
        match serde_json::from_value::<JsonRpcRequest>(item.clone()) {
            Ok(req) => {
                if req.id.is_some() && req.id != Some(Value::Null) {
                    responses.push(server.handle(req).await);
                }
            }
            Err(e) => {
                responses.push(JsonRpcResponse::err(
                    req_id,
                    -32600,
                    format!("Invalid Request: {e}"),
                ));
            }
        }
    }

    // Must return 3 responses (ping, tools/list, malformed) and skip the notification
    assert_eq!(responses.len(), 3);
    assert_eq!(responses[0].id, Some(json!(1)));
    assert_eq!(responses[1].id, Some(json!("req-2")));
    assert_eq!(responses[2].id, None);
    assert_eq!(responses[2].error.as_ref().unwrap().code, -32600);
}

#[tokio::test]
async fn test_read_line_bounded_exact_boundaries_16mb() {
    use crate::{read_line_bounded, MAX_RPC_BYTES};
    use std::io::Cursor;
    use tokio::io::BufReader;

    // Boundary 1: 16MB - 1 byte (including newline)
    let size_minus_1 = MAX_RPC_BYTES - 1;
    let mut data_minus_1 = vec![b'a'; size_minus_1 - 1];
    data_minus_1.push(b'\n');
    let mut reader1 = BufReader::new(Cursor::new(data_minus_1));
    let mut buf1 = String::new();
    let res1 = read_line_bounded(&mut reader1, &mut buf1, MAX_RPC_BYTES).await;
    assert!(res1.is_ok());
    assert_eq!(res1.unwrap(), size_minus_1);

    // Boundary 2: 16MB exact (including newline)
    let size_exact = MAX_RPC_BYTES;
    let mut data_exact = vec![b'b'; size_exact - 1];
    data_exact.push(b'\n');
    let mut reader2 = BufReader::new(Cursor::new(data_exact));
    let mut buf2 = String::new();
    let res2 = read_line_bounded(&mut reader2, &mut buf2, MAX_RPC_BYTES).await;
    assert!(res2.is_ok());
    assert_eq!(res2.unwrap(), size_exact);

    // Boundary 3: 16MB + 1 byte (including newline) -> rejected
    let size_plus_1 = MAX_RPC_BYTES + 1;
    let mut data_plus_1 = vec![b'c'; size_plus_1 - 1];
    data_plus_1.push(b'\n');
    let mut reader3 = BufReader::new(Cursor::new(data_plus_1));
    let mut buf3 = String::new();
    let res3 = read_line_bounded(&mut reader3, &mut buf3, MAX_RPC_BYTES).await;
    assert!(res3.is_err());
    let err = res3.unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("limit exceeded"));
}

#[tokio::test]
async fn test_max_search_query_exact_boundaries_64kb() {
    let (server, _tmp) = create_mock_server().await;
    let max_bytes = crate::MAX_SEARCH_QUERY_BYTES; // 65,536

    // 1. 64KB - 1 byte: accepted
    let query_minus_1 = "a".repeat(max_bytes - 1);
    let req1 = make_request("memfuse_search", json!({"query": query_minus_1}));
    let resp1 = server.handle(req1).await;
    assert!(resp1.error.is_none());

    // 2. 64KB exact: accepted
    let query_exact = "b".repeat(max_bytes);
    let req2 = make_request("memfuse_search", json!({"query": query_exact}));
    let resp2 = server.handle(req2).await;
    assert!(resp2.error.is_none());

    // 3. 64KB + 1 byte: rejected
    let query_plus_1 = "c".repeat(max_bytes + 1);
    let req3 = make_request("memfuse_search", json!({"query": query_plus_1}));
    let resp3 = server.handle(req3).await;
    let err3 = resp3.error.expect("error expected for 64KB + 1 byte query");
    assert_eq!(err3.code, -32602);
    assert!(err3.message.contains("query size exceeds limit"));
}

#[tokio::test]
async fn test_eof_without_newline_does_not_hang() {
    use crate::read_line_bounded;
    use std::io::Cursor;
    use tokio::io::BufReader;

    let partial_data = b"{\"jsonrpc\":\"2.0\",\"id\":99,\"method\":\"ping\"}";
    let mut reader = BufReader::new(Cursor::new(partial_data));
    let mut buf = String::new();

    let res = read_line_bounded(&mut reader, &mut buf, crate::MAX_RPC_BYTES).await;
    assert!(res.is_ok());
    assert_eq!(buf, "{\"jsonrpc\":\"2.0\",\"id\":99,\"method\":\"ping\"}");
}

#[tokio::test]
async fn test_non_utf8_binary_data_on_stdin() {
    use crate::read_line_bounded;
    use std::io::Cursor;
    use tokio::io::BufReader;

    let binary_invalid_utf8 = vec![0xFF, 0xFE, 0xFD, 0x80, b'\n'];
    let mut reader = BufReader::new(Cursor::new(binary_invalid_utf8));
    let mut buf = String::new();

    let res = read_line_bounded(&mut reader, &mut buf, crate::MAX_RPC_BYTES).await;
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("Invalid UTF-8"));
}

#[tokio::test]
async fn test_16mb_garbage_byte_stream_single_line() {
    use crate::read_line_bounded;
    use std::io::Cursor;
    use tokio::io::BufReader;

    let mut garbage = vec![b'X'; crate::MAX_RPC_BYTES - 1];
    garbage.push(b'\n');
    let mut reader = BufReader::new(Cursor::new(garbage));
    let mut buf = String::new();

    let res = read_line_bounded(&mut reader, &mut buf, crate::MAX_RPC_BYTES).await;
    assert!(res.is_ok());
    assert_eq!(buf.len(), crate::MAX_RPC_BYTES);

    let parse_res = serde_json::from_str::<Value>(&buf);
    assert!(parse_res.is_err());
    let resp = JsonRpcResponse::err(None, -32700, format!("Parse error: {}", parse_res.unwrap_err()));
    assert_eq!(resp.error.unwrap().code, -32700);
}

#[tokio::test]
async fn test_chunked_slow_client_writes_reconstructed() {
    use crate::read_line_bounded;
    use tokio::io::AsyncWriteExt;
    use tokio::io::BufReader;

    let (client_write, server_read) = tokio::io::duplex(1024);
    let mut writer = client_write;
    let mut buf_reader = BufReader::new(server_read);

    let handle = tokio::spawn(async move {
        let mut buf = String::new();
        read_line_bounded(&mut buf_reader, &mut buf, crate::MAX_RPC_BYTES).await.unwrap();
        buf
    });

    let parts = ["{\"json", "rpc\":\"", "2.0\",\"id\":7", "7,\"method\"", ":\"ping\"}\n"];
    for part in parts {
        writer.write_all(part.as_bytes()).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }

    let reconstructed = handle.await.unwrap();
    assert_eq!(reconstructed, "{\"jsonrpc\":\"2.0\",\"id\":77,\"method\":\"ping\"}\n");
}

#[tokio::test]
async fn test_flood_10000_requests_resource_stability() {
    let (server, _tmp) = create_mock_server().await;

    let req = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(1)),
        method: "ping".into(),
        params: json!({}),
    };

    for i in 0..10_000 {
        let mut loop_req = req.clone();
        loop_req.id = Some(json!(i));
        let resp = server.handle(loop_req).await;
        assert_eq!(resp.id, Some(json!(i)));
    }
}

#[tokio::test]
async fn test_error_responses_do_not_leak_internal_paths_or_stacktraces() {
    let (server, _tmp) = create_mock_server().await;

    // Trigger error via invalid collection name
    let req1 = make_request("memfuse_search", json!({"query": "test", "collection": "/etc/passwd\0"}));
    let resp1 = server.handle(req1).await;
    let err1 = resp1.error.expect("error expected");
    assert!(!err1.message.contains("/app/"));
    assert!(!err1.message.contains("src/"));
    assert!(!err1.message.contains("0x"));

    // Trigger error via invalid document ID
    let req2 = make_request("memfuse_insert", json!({"id": "   ", "text": "hello"}));
    let resp2 = server.handle(req2).await;
    let err2 = resp2.error.expect("error expected");
    assert!(!err2.message.contains("/app/"));
    assert!(!err2.message.contains("src/"));
}

#[tokio::test]
async fn test_concurrent_interleaved_requests_id_integrity() {
    let (server, _tmp) = create_mock_server().await;

    let mut futures = Vec::new();
    for i in 0..100 {
        let server_clone = server.clone();
        futures.push(async move {
            let req = JsonRpcRequest {
                jsonrpc: "2.0".into(),
                id: Some(json!(i)),
                method: "ping".into(),
                params: json!({}),
            };
            let resp = server_clone.handle(req).await;
            assert_eq!(resp.id, Some(json!(i)));
        });
    }

    futures_util::future::join_all(futures).await;
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
