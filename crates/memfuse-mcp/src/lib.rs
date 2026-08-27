pub mod protocol;
pub mod sandbox;
#[cfg(test)]
mod tests;

use memfuse_core::{DocId, MemFuseError, TextEmbeddingEngine, MAX_SEARCH_K};
use memfuse_db::chunker::{ChunkerConfig, MarkdownChunker};
use memfuse_db::MemFuse;
use protocol::{JsonRpcRequest, JsonRpcResponse, McpError};
use sandbox::{McpSandbox, SandboxPolicy};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// MCP-Server mit stdio-Transport (JSON-RPC 2.0).
///
/// stdout ist dem Protokoll vorbehalten — Logs gehen ausschließlich nach stderr.
fn validate_collection_name(name: &str) -> Result<(), McpError> {
    if name.is_empty() || name.len() > 256 {
        return Err(McpError::invalid_params(format!(
            "Invalid collection name length: {}",
            name.len()
        )));
    }
    if name.contains('\0') || name.contains(':') || name.contains('/') {
        return Err(McpError::invalid_params(format!(
            "Collection name '{name}' contains forbidden characters"
        )));
    }
    Ok(())
}

pub struct McpServer {
    pub db: Arc<MemFuse>,
    pub embedder: Arc<dyn TextEmbeddingEngine>,
    pub sandbox: Arc<McpSandbox>,
}

impl McpServer {
    pub fn new(
        db: Arc<MemFuse>,
        embedder: Arc<dyn TextEmbeddingEngine>,
    ) -> Result<Self, MemFuseError> {
        let policy = SandboxPolicy {
            allow_db_reads: true,
            allow_db_writes: true,
            allow_code_execution: false,
            max_execution_ms: 5_000,
        };
        let sandbox = McpSandbox::new(policy)
            .map_err(|e| MemFuseError::Internal(format!("Sandbox init: {e}")))?;
        Ok(Self::with_sandbox(db, embedder, Arc::new(sandbox)))
    }

    pub fn with_sandbox(
        db: Arc<MemFuse>,
        embedder: Arc<dyn TextEmbeddingEngine>,
        sandbox: Arc<McpSandbox>,
    ) -> Self {
        Self {
            db,
            embedder,
            sandbox,
        }
    }

    /// Startet den MCP stdio-Loop.
    ///
    /// Liest zeilenweise von stdin, dispatcht JSON-RPC-Requests und schreibt
    /// Antworten als einzelne JSON-Zeile nach stdout.
    /// Der Loop endet, wenn stdin geschlossen wird (EOF).
    pub async fn run_stdio(self: Arc<Self>) -> Result<(), Box<dyn std::error::Error>> {
        let stdin = tokio::io::stdin();
        let mut stdout = tokio::io::stdout();
        let mut lines = BufReader::new(stdin).lines();

        while let Some(line) = lines.next_line().await? {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let response = match serde_json::from_str::<Value>(trimmed) {
                Ok(val) => {
                    let req_id = val.get("id").cloned();
                    match serde_json::from_value::<JsonRpcRequest>(val) {
                        Ok(req) => {
                            if req.id.is_none() || req.id == Some(Value::Null) {
                                let _ = self.handle(req).await;
                                continue; // notification: no response required
                            }
                            self.handle(req).await
                        }
                        Err(e) => {
                            JsonRpcResponse::err(req_id, -32600, format!("Invalid Request: {e}"))
                        }
                    }
                }
                Err(e) => JsonRpcResponse::err(None, -32700, format!("Parse error: {e}")),
            };

            // MCP-Protokoll: eine JSON-Antwort pro Zeile, abgeschlossen mit '\n'.
            let mut out = serde_json::to_string(&response)?;
            out.push('\n');
            stdout.write_all(out.as_bytes()).await?;
            stdout.flush().await?;
        }
        Ok(())
    }

    pub async fn handle(&self, req: JsonRpcRequest) -> JsonRpcResponse {
        let id = req.id.clone();
        if req.jsonrpc != "2.0" {
            return JsonRpcResponse::err(
                id,
                -32600,
                format!(
                    "Invalid Request: jsonrpc version must be '2.0', got '{}'",
                    req.jsonrpc
                ),
            );
        }
        match req.method.as_str() {
            // ── Lifecycle ──────────────────────────────────────────────────────
            "initialize" => JsonRpcResponse::ok(
                id,
                json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "memfuse", "version": "0.1.0" }
                }),
            ),
            // Notification — Client bestätigt erfolgte Initialisierung; keine Antwort nötig,
            // aber wir senden ein leeres ok damit kein Parse-Fehler im Client entsteht.
            "initialized" => JsonRpcResponse::ok(id, json!({})),

            // ── Tool-Discovery ─────────────────────────────────────────────────
            "tools/list" => JsonRpcResponse::ok(
                id,
                json!({
                    "tools": [
                        {
                            "name": "memfuse_search",
                            "description": "Hybrid semantic search (vector + BM25 + graph) über gespeicherte Dokumente.",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "query":      { "type": "string" },
                                    "collection": { "type": "string", "default": "default" },
                                    "k":          { "type": "integer", "default": 10 }
                                },
                                "required": ["query"]
                            }
                        },
                        {
                            "name": "memfuse_insert",
                            "description": "Dokument einspeichern (auto-embedding, auto-chunking mit MarkdownChunker, ~512 Tokens).",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "id":         { "type": "string" },
                                    "text":       { "type": "string" },
                                    "collection": { "type": "string", "default": "default" },
                                    "metadata":   { "type": "object" }
                                },
                                "required": ["id", "text"]
                            }
                        },
                        {
                            "name": "memfuse_get",
                            "description": "Dokument per ID abrufen.",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "id":         { "type": "string" },
                                    "collection": { "type": "string", "default": "default" }
                                },
                                "required": ["id"]
                            }
                        },
                        {
                            "name": "memfuse_collections",
                            "description": "Alle Collections auflisten.",
                            "inputSchema": {
                                "type": "object",
                                "properties": {}
                            }
                        }
                    ]
                }),
            ),

            // ── Tool-Dispatch ──────────────────────────────────────────────────
            "tools/call" => {
                let tool_name = match req.params.get("name").and_then(|v| v.as_str()) {
                    Some(name) if !name.is_empty() => name,
                    _ => {
                        return JsonRpcResponse::err(
                            id,
                            -32602,
                            "Invalid params: missing or empty tool 'name'",
                        );
                    }
                };
                let args = req.params.get("arguments").cloned().unwrap_or_default();

                match self
                    .sandbox
                    .execute_with_timeout(tool_name, self.call_tool(tool_name, &args))
                    .await
                {
                    Ok(content) => JsonRpcResponse::ok(
                        id,
                        json!({
                            "content": [{ "type": "text", "text": content.to_string() }]
                        }),
                    ),
                    Err(e) => JsonRpcResponse::ok(
                        id,
                        json!({
                            "isError": true,
                            "content": [{ "type": "text", "text": e.to_string() }]
                        }),
                    ),
                }
            }

            "memfuse_search" | "memfuse_insert" | "memfuse_get" | "memfuse_collections" => {
                let tool_name = req.method.as_str();
                match self
                    .sandbox
                    .execute_with_timeout(tool_name, self.call_tool(tool_name, &req.params))
                    .await
                {
                    Ok(res) => JsonRpcResponse::ok(id, res),
                    Err(e) => JsonRpcResponse::from_error(id, e),
                }
            }

            // ── Ping ───────────────────────────────────────────────────────────
            "ping" => JsonRpcResponse::ok(id, json!({})),

            // ── Unbekannte Methode ──────────────────────────────────────────────
            other => JsonRpcResponse::err(id, -32601, format!("Method not found: {other}")),
        }
    }

    async fn call_tool(&self, name: &str, args: &Value) -> Result<Value, McpError> {
        self.sandbox
            .validate_tool_call(name, args)
            .map_err(McpError::from)?;
        match name {
            "memfuse_search" => {
                let query = match args.get("query") {
                    Some(v) => v.as_str().ok_or_else(|| {
                        McpError::invalid_params("Invalid params: 'query' must be a string")
                    })?,
                    None => {
                        return Err(McpError::invalid_params("missing required field: 'query'"));
                    }
                };

                let col_name = if let Some(col_val) = args.get("collection") {
                    let s = col_val.as_str().ok_or_else(|| {
                        McpError::invalid_params("Invalid params: 'collection' must be a string")
                    })?;
                    if s.trim().is_empty() {
                        "default"
                    } else {
                        validate_collection_name(s)?;
                        s
                    }
                } else {
                    "default"
                };

                let k_val = args.get("k").or_else(|| args.get("limit"));
                let k_raw = match k_val {
                    Some(Value::Number(n)) => n.as_u64().ok_or_else(|| {
                        McpError::invalid_params("k/limit muss eine positive Ganzzahl sein")
                    })? as usize,
                    Some(_) => return Err(McpError::invalid_params("k/limit muss eine Zahl sein")),
                    None => 10,
                };
                let k = k_raw.min(MAX_SEARCH_K);
                if k_raw > MAX_SEARCH_K {
                    tracing::warn!(
                        requested_k = k_raw,
                        capped_k = MAX_SEARCH_K,
                        "Client k capped to MAX_SEARCH_K"
                    );
                }

                let col = self.db.collection(col_name).await.map_err(McpError::from)?;
                let vec = self
                    .embedder
                    .embed(query)
                    .await
                    .map_err(|e| McpError::internal_error(e.to_string()))?;
                let results = col
                    .hybrid_search(query, &vec, k, None)
                    .await
                    .map_err(McpError::from)?;
                serde_json::to_value(results).map_err(|e| McpError::internal_error(e.to_string()))
            }

            "memfuse_insert" => {
                // Validate collection parameter if present
                let col_name = if let Some(col_val) = args.get("collection") {
                    let s = col_val.as_str().ok_or_else(|| {
                        McpError::invalid_params("Invalid params: 'collection' must be a string")
                    })?;
                    if s.trim().is_empty() {
                        "default"
                    } else {
                        validate_collection_name(s)?;
                        s
                    }
                } else {
                    "default"
                };

                // Validate id parameter
                let id = match args.get("id") {
                    Some(v) => {
                        let s = v.as_str().ok_or_else(|| {
                            McpError::invalid_params("Invalid params: 'id' must be a string")
                        })?;
                        if s.is_empty() {
                            return Err(McpError::invalid_params("id cannot be empty"));
                        }
                        s
                    }
                    None => {
                        return Err(McpError::invalid_params(
                            "id fehlt: missing required field 'id'",
                        ));
                    }
                };

                // Validate vector / text parameters
                let vec_val = args.get("vector");
                let text_val = args.get("text");

                let vector_opt: Option<Vec<f32>> = if let Some(v) = vec_val {
                    let arr = v.as_array().ok_or_else(|| {
                        McpError::invalid_params(
                            "Invalid params: 'vector' must be an array of numbers",
                        )
                    })?;
                    let mut vec = Vec::with_capacity(arr.len());
                    for elem in arr {
                        let num = elem.as_f64().ok_or_else(|| {
                            McpError::invalid_params(
                                "Invalid params: 'vector' must contain numbers",
                            )
                        })?;
                        vec.push(num as f32);
                    }
                    Some(vec)
                } else {
                    None
                };

                let text_opt = if let Some(t) = text_val {
                    let s = t.as_str().ok_or_else(|| {
                        McpError::invalid_params("Invalid params: 'text' must be a string")
                    })?;
                    if s.is_empty() {
                        return Err(McpError::invalid_params("text cannot be empty"));
                    }
                    const MAX_INSERT_TEXT_BYTES: usize = 10 * 1024 * 1024; // 10MB
                    if s.len() > MAX_INSERT_TEXT_BYTES {
                        return Err(McpError::invalid_params(format!(
                            "text too large: {}MB > 10MB limit",
                            s.len() / 1_048_576
                        )));
                    }
                    Some(s)
                } else {
                    None
                };

                if vector_opt.is_none() && text_opt.is_none() {
                    return Err(McpError::invalid_params(
                        "text/vector fehlt: missing required field 'vector' or 'text'",
                    ));
                }

                let base_metadata = args
                    .get("metadata")
                    .and_then(|v| v.as_object())
                    .cloned()
                    .unwrap_or_default();

                let col = self.db.collection(col_name).await.map_err(McpError::from)?;

                if let Some(vector) = vector_opt {
                    let mut meta = json!({
                        "source_id": id,
                    });
                    if let Some(obj) = meta.as_object_mut() {
                        if let Some(text) = text_opt {
                            obj.insert("text".to_string(), json!(text));
                        }
                        for (k, v) in &base_metadata {
                            obj.entry(k.clone()).or_insert_with(|| v.clone());
                        }
                    }
                    col.insert(id, &vector, Some(meta))
                        .await
                        .map_err(McpError::from)?;

                    return Ok(json!({
                        "ok": true,
                        "id": id,
                        "chunks_inserted": 1,
                        "chunk_ids": [id],
                        "collection": col_name
                    }));
                }

                // AUTO-CHUNKING: Text in semantische Einheiten aufteilen mit MarkdownChunker (~512 Tokens)
                let text = match text_opt {
                    Some(t) => t,
                    None => {
                        return Err(McpError::invalid_params(
                            "text/vector fehlt: missing required field 'text'",
                        ));
                    }
                };
                let chunker = MarkdownChunker::new(ChunkerConfig::default());
                let doc_id = DocId::from_key(id).map_err(|e| {
                    McpError::invalid_params(format!("Invalid document ID '{}': {}", id, e))
                })?;
                let chunks = chunker.chunk(doc_id, text);

                if chunks.is_empty() {
                    return Ok(json!({
                        "ok": false,
                        "error": "Text konnte nicht in Chunks aufgeteilt werden (leer?)"
                    }));
                }

                let total = chunks.len();
                let mut inserted_chunk_ids = Vec::new();

                for (i, chunk) in chunks.iter().enumerate() {
                    let chunk_id = if total == 1 {
                        id.to_string()
                    } else {
                        format!("{id}:chunk:{i}")
                    };

                    let embedding = self
                        .embedder
                        .embed(&chunk.content)
                        .await
                        .map_err(|e| McpError::internal_error(e.to_string()))?;

                    let mut chunk_meta = json!({
                        "text": &chunk.content,
                        "source_id": id,
                        "chunk_index": i,
                        "chunk_total": total
                    });

                    if let Some(obj) = chunk_meta.as_object_mut() {
                        if let Some(meta) = &chunk.metadata {
                            if let Some(m_obj) = meta.as_object() {
                                for (k, v) in m_obj {
                                    obj.insert(k.clone(), v.clone());
                                }
                            }
                        }
                        for (k, v) in &base_metadata {
                            obj.entry(k.clone()).or_insert_with(|| v.clone());
                        }
                    }

                    col.insert(&chunk_id, &embedding, Some(chunk_meta))
                        .await
                        .map_err(McpError::from)?;

                    inserted_chunk_ids.push(chunk_id);
                }

                Ok(json!({
                    "ok": true,
                    "id": id,
                    "chunks_inserted": inserted_chunk_ids.len(),
                    "chunk_ids": inserted_chunk_ids,
                    "collection": col_name
                }))
            }

            "memfuse_get" => {
                let id = match args.get("id") {
                    Some(v) => {
                        let s = v.as_str().ok_or_else(|| {
                            McpError::invalid_params("Invalid params: 'id' must be a string")
                        })?;
                        if s.is_empty() {
                            return Err(McpError::invalid_params("id cannot be empty"));
                        }
                        s
                    }
                    None => {
                        return Err(McpError::invalid_params(
                            "id fehlt: missing required field 'id'",
                        ));
                    }
                };

                let col_name = if let Some(col_val) = args.get("collection") {
                    let s = col_val.as_str().ok_or_else(|| {
                        McpError::invalid_params("Invalid params: 'collection' must be a string")
                    })?;
                    if s.trim().is_empty() {
                        "default"
                    } else {
                        validate_collection_name(s)?;
                        s
                    }
                } else {
                    "default"
                };

                let col = self.db.collection(col_name).await.map_err(McpError::from)?;
                match col.get(id).await.map_err(McpError::from)? {
                    Some(doc) => serde_json::to_value(doc)
                        .map_err(|e| McpError::internal_error(e.to_string())),
                    None => Ok(json!(null)),
                }
            }

            "memfuse_collections" => {
                let names = self.db.list_collections().await.map_err(McpError::from)?;
                Ok(json!({ "collections": names }))
            }

            other => Err(McpError::invalid_params(format!(
                "Unbekanntes Tool: {other}"
            ))),
        }
    }
}
