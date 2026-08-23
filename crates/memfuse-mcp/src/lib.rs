use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use memfuse_core::TextEmbeddingEngine;
use memfuse_db::MemFuse;
use memfuse_ollama::OllamaEmbedder;
use serde_json::Value;
use std::sync::Arc;

/// Shared state for the MCP server.
pub struct McpServerState {
    pub db: Arc<MemFuse>,
    pub embedder: Arc<dyn TextEmbeddingEngine>,
}

impl McpServerState {
    pub fn new(db: Arc<MemFuse>) -> Self {
        Self {
            db,
            embedder: Arc::new(OllamaEmbedder::with_defaults()),
        }
    }

    pub fn with_embedder(db: Arc<MemFuse>, embedder: Arc<dyn TextEmbeddingEngine>) -> Self {
        Self { db, embedder }
    }
}

/// Creates the axum router with all MCP JSON-RPC / HTTP endpoints.
pub fn create_router(state: Arc<McpServerState>) -> Router {
    Router::new()
        .route("/mcp/tools/list", get(list_tools))
        .route("/mcp/tools/call", post(call_tool))
        .with_state(state)
}

async fn list_tools() -> Json<Value> {
    Json(serde_json::json!({
        "tools": [
            {
                "name": "memfuse_search",
                "description": "Hybrid search across stored documents (vector + BM25 + graph — 4-Signal Fusion)",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Natural language query" },
                        "collection": { "type": "string", "description": "Collection name", "default": "default" },
                        "k": { "type": "integer", "description": "Number of results", "default": 10 }
                    },
                    "required": ["query"]
                }
            },
            {
                "name": "memfuse_get",
                "description": "Retrieve a specific document by ID",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "Document ID" },
                        "collection": { "type": "string", "description": "Collection name", "default": "default" }
                    },
                    "required": ["id"]
                }
            },
            {
                "name": "memfuse_insert",
                "description": "Store a document with embedding and metadata",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "Document ID" },
                        "text": { "type": "string", "description": "Document text (auto-chunked and embedded)" },
                        "collection": { "type": "string", "description": "Collection name", "default": "default" },
                        "metadata": { "type": "object", "description": "Optional metadata", "default": {} }
                    },
                    "required": ["id", "text"]
                }
            },
            {
                "name": "memfuse_collections",
                "description": "List all available collections",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            }
        ]
    }))
}

async fn call_tool(
    State(state): State<Arc<McpServerState>>,
    Json(request): Json<Value>,
) -> Json<Value> {
    let tool_name = match request
        .get("name")
        .or_else(|| request.get("params").and_then(|p| p.get("name")))
        .and_then(|n| n.as_str())
    {
        Some(n) => n,
        None => {
            return Json(serde_json::json!({
                "isError": true,
                "content": [{ "type": "text", "text": "Missing required field: tool name" }]
            }))
        }
    };

    let args = request
        .get("arguments")
        .or_else(|| request.get("params").and_then(|p| p.get("arguments")))
        .cloned()
        .unwrap_or_default();

    let result = match tool_name {
        "memfuse_search" => handle_search(&state, &args).await,
        "memfuse_insert" => handle_insert(&state, &args).await,
        "memfuse_get" => handle_get(&state, &args).await,
        "memfuse_collections" => handle_collections(&state).await,
        other => Err(format!("Unbekanntes Tool: {other}")),
    };

    match result {
        Ok(value) => {
            Json(serde_json::json!({ "content": [{ "type": "text", "text": value.to_string() }] }))
        }
        Err(e) => {
            Json(serde_json::json!({ "isError": true, "content": [{ "type": "text", "text": e }] }))
        }
    }
}

async fn handle_search(state: &McpServerState, args: &Value) -> Result<Value, String> {
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or("query fehlt")?;
    let collection_name = args
        .get("collection")
        .and_then(|v| v.as_str())
        .unwrap_or("default");
    let k = args.get("k").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

    let collection = state
        .db
        .collection(collection_name)
        .await
        .map_err(|e| e.to_string())?;

    let query_vector = state
        .embedder
        .embed(query)
        .await
        .map_err(|e| format!("Embedding query failed: {e}"))?;

    let results = collection
        .hybrid_search(query, &query_vector, k, None)
        .await
        .map_err(|e| e.to_string())?;

    serde_json::to_value(results).map_err(|e| format!("Serialization error: {e}"))
}

async fn handle_insert(state: &McpServerState, args: &Value) -> Result<Value, String> {
    let id = args.get("id").and_then(|v| v.as_str()).ok_or("id fehlt")?;
    let text = args
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or("text fehlt")?;
    let collection_name = args
        .get("collection")
        .and_then(|v| v.as_str())
        .unwrap_or("default");

    let mut metadata = args
        .get("metadata")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    if !metadata.is_object() {
        metadata = serde_json::json!({});
    }
    if let Some(obj) = metadata.as_object_mut() {
        obj.entry("text")
            .or_insert_with(|| serde_json::Value::String(text.to_string()));
    }

    let embedding = state
        .embedder
        .embed(text)
        .await
        .map_err(|e| format!("Embedding text failed: {e}"))?;

    let collection = state
        .db
        .collection(collection_name)
        .await
        .map_err(|e| e.to_string())?;

    if embedding.len() != collection.dimension() {
        return Err(format!(
            "Embedding dimension {} does not match collection dimension {}",
            embedding.len(),
            collection.dimension()
        ));
    }

    collection
        .insert(id, &embedding, Some(metadata))
        .await
        .map_err(|e| e.to_string())?;

    Ok(serde_json::json!({ "status": "inserted", "id": id }))
}

async fn handle_get(state: &McpServerState, args: &Value) -> Result<Value, String> {
    let id = args.get("id").and_then(|v| v.as_str()).ok_or("id fehlt")?;
    let collection_name = args
        .get("collection")
        .and_then(|v| v.as_str())
        .unwrap_or("default");

    let collection = state
        .db
        .collection(collection_name)
        .await
        .map_err(|e| e.to_string())?;
    let doc = collection.get(id).await.map_err(|e| e.to_string())?;

    serde_json::to_value(doc).map_err(|e| format!("Serialization error: {e}"))
}

async fn handle_collections(state: &McpServerState) -> Result<Value, String> {
    let collections = state
        .db
        .list_collections()
        .await
        .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "collections": collections }))
}
