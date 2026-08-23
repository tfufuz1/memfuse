use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use memfuse_db::MemFuse;
use memfuse_mcp::{create_router, McpServerState};
use serde_json::{json, Value};
use std::sync::Arc;
use tempfile::TempDir;
use tower::ServiceExt;

async fn setup_app() -> (axum::Router, TempDir) {
    let tmp = TempDir::new().expect("temp dir");
    let db = MemFuse::open(tmp.path()).await.expect("open db");
    let state = Arc::new(McpServerState::new(Arc::new(db)));
    let app = create_router(state);
    (app, tmp)
}

#[tokio::test]
async fn test_list_tools() {
    let (app, _tmp) = setup_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/mcp/tools/list")
                .method("GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    let tools = json["tools"].as_array().expect("tools array");
    let tool_names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();

    assert!(tool_names.contains(&"memfuse_search"));
    assert!(tool_names.contains(&"memfuse_get"));
    assert!(tool_names.contains(&"memfuse_insert"));
    assert!(tool_names.contains(&"memfuse_collections"));
}

#[tokio::test]
async fn test_mcp_flow_insert_get_search_collections() {
    let (app, _tmp) = setup_app().await;

    // 1. Insert document
    let req_body = json!({
        "name": "memfuse_insert",
        "arguments": {
            "id": "doc1",
            "text": "Rust and MCP server integration",
            "collection": "my_docs",
            "metadata": { "author": "Jules" }
        }
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/mcp/tools/call")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let res_json: Value = serde_json::from_slice(&body).unwrap();
    assert!(res_json["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("inserted"));

    // 2. Get document
    let req_body = json!({
        "name": "memfuse_get",
        "arguments": {
            "id": "doc1",
            "collection": "my_docs"
        }
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/mcp/tools/call")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let res_json: Value = serde_json::from_slice(&body).unwrap();
    assert!(res_json["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("doc1"));

    // 3. Search document
    let req_body = json!({
        "name": "memfuse_search",
        "arguments": {
            "query": "Rust integration",
            "collection": "my_docs",
            "k": 5
        }
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/mcp/tools/call")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let res_json: Value = serde_json::from_slice(&body).unwrap();
    assert!(res_json["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("doc1"));

    // 4. List collections
    let req_body = json!({
        "name": "memfuse_collections",
        "arguments": {}
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/mcp/tools/call")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let res_json: Value = serde_json::from_slice(&body).unwrap();
    assert!(res_json["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("my_docs"));
}
