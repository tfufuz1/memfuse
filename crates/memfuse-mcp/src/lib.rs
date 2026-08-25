pub mod protocol;

use memfuse_core::{DocId, TextEmbeddingEngine};
use memfuse_db::chunker::{ChunkerConfig, MarkdownChunker};
use memfuse_db::MemFuse;
use protocol::{JsonRpcRequest, JsonRpcResponse};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// MCP-Server mit stdio-Transport (JSON-RPC 2.0).
///
/// stdout ist dem Protokoll vorbehalten — Logs gehen ausschließlich nach stderr.
pub struct McpServer {
    pub db: Arc<MemFuse>,
    pub embedder: Arc<dyn TextEmbeddingEngine>,
}

impl McpServer {
    pub fn new(db: Arc<MemFuse>, embedder: Arc<dyn TextEmbeddingEngine>) -> Self {
        Self { db, embedder }
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

            let response = match serde_json::from_str::<JsonRpcRequest>(trimmed) {
                Ok(req) => self.handle(req).await,
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
                            "description": "Dokument einspeichern (auto-embedding, auto-chunking).",
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
                let tool_name = req
                    .params
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let args = req.params.get("arguments").cloned().unwrap_or_default();

                match self.call_tool(tool_name, &args).await {
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
                            "content": [{ "type": "text", "text": e }]
                        }),
                    ),
                }
            }

            // ── Ping ───────────────────────────────────────────────────────────
            "ping" => JsonRpcResponse::ok(id, json!({})),

            // ── Unbekannte Methode ──────────────────────────────────────────────
            other => JsonRpcResponse::err(id, -32601, format!("Method not found: {other}")),
        }
    }

    async fn call_tool(&self, name: &str, args: &Value) -> Result<Value, String> {
        match name {
            "memfuse_search" => {
                let query = args
                    .get("query")
                    .and_then(|v| v.as_str())
                    .ok_or("query fehlt")?;
                let col_name = args
                    .get("collection")
                    .and_then(|v| v.as_str())
                    .unwrap_or("default");
                let k = args.get("k").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

                let col = self
                    .db
                    .collection(col_name)
                    .await
                    .map_err(|e| e.to_string())?;
                let vec = self
                    .embedder
                    .embed(query)
                    .await
                    .map_err(|e| e.to_string())?;
                let results = col
                    .hybrid_search(query, &vec, k, None)
                    .await
                    .map_err(|e| e.to_string())?;
                serde_json::to_value(results).map_err(|e| e.to_string())
            }

            "memfuse_insert" => {
                let id = args.get("id").and_then(|v| v.as_str()).ok_or("id fehlt")?;
                let text = args
                    .get("text")
                    .and_then(|v| v.as_str())
                    .ok_or("text fehlt")?;
                let col_name = args
                    .get("collection")
                    .and_then(|v| v.as_str())
                    .unwrap_or("default");
                let metadata = args.get("metadata").cloned();

                let col = self
                    .db
                    .collection(col_name)
                    .await
                    .map_err(|e| e.to_string())?;

                // AUTO-CHUNKING: Text in semantische Einheiten aufteilen
                // Verhindert Embedding-Verwässerung bei Dokumenten >512 Tokens
                let chunker = MarkdownChunker::new(ChunkerConfig::default());
                let doc_id = DocId::from_key(id).unwrap_or_else(|_| DocId::new(0));
                let chunks = chunker.chunk(doc_id, text);

                let chunks_to_process: Vec<String> = if chunks.is_empty() {
                    vec![text.to_string()]
                } else {
                    chunks.into_iter().map(|c| c.content).collect()
                };

                let total = chunks_to_process.len();
                for (i, chunk_text) in chunks_to_process.iter().enumerate() {
                    let chunk_id = if total == 1 {
                        id.to_string()
                    } else {
                        format!("{id}:chunk:{i}")
                    };

                    let embedding = self
                        .embedder
                        .embed(chunk_text)
                        .await
                        .map_err(|e| e.to_string())?;

                    let mut chunk_meta = metadata.clone().unwrap_or_else(|| json!({}));
                    if !chunk_meta.is_object() {
                        chunk_meta = json!({});
                    }
                    if let Some(obj) = chunk_meta.as_object_mut() {
                        obj.entry("text")
                            .or_insert_with(|| json!(chunk_text.clone()));
                        if total > 1 {
                            obj.insert("_chunk_index".into(), json!(i));
                            obj.insert("_chunk_total".into(), json!(total));
                            obj.insert("_source_id".into(), json!(id));
                        }
                    }

                    col.insert(&chunk_id, &embedding, Some(chunk_meta))
                        .await
                        .map_err(|e| e.to_string())?;
                }

                Ok(json!({
                    "inserted": id,
                    "chunks": total,
                    "collection": col_name
                }))
            }

            "memfuse_get" => {
                let id = args.get("id").and_then(|v| v.as_str()).ok_or("id fehlt")?;
                let col_name = args
                    .get("collection")
                    .and_then(|v| v.as_str())
                    .unwrap_or("default");
                let col = self
                    .db
                    .collection(col_name)
                    .await
                    .map_err(|e| e.to_string())?;
                match col.get(id).await.map_err(|e| e.to_string())? {
                    Some(doc) => serde_json::to_value(doc).map_err(|e| e.to_string()),
                    None => Ok(json!(null)),
                }
            }

            "memfuse_collections" => {
                let names = self
                    .db
                    .list_collections()
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(json!({ "collections": names }))
            }

            other => Err(format!("Unbekanntes Tool: {other}")),
        }
    }
}
