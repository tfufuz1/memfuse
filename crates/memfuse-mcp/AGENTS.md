# memfuse-mcp — Crate-Level Agent Rules

## Critical Invariants

### Transport: stdio JSON-RPC 2.0 ONLY
This crate implements the MCP spec (v2024-11-05) over stdin/stdout.
- **No axum**, no HTTP, no SSE — removed per ADR-010
- All JSON-RPC messages are exchanged line-by-line over stdin/stdout
- Logging goes to stderr exclusively (stdout is protocol-only)
- Test via: `echo '{"jsonrpc":"2.0","method":"tools/list","id":1}' | cargo run --bin memfuse-mcp-server`

### Document Insertion — Chunking Required
`memfuse_insert` MUST call `MarkdownChunker` before generating embeddings.
NEVER embed the entire document text as a single vector — this degrades
retrieval quality for documents longer than the embedding model's context window.

### Tool Description Consistency
Tool descriptions in the `tools/list` response MUST match the actual
implementation behavior. After modifying a tool handler, always verify
the corresponding description string.

### Authorization & Read-Only Default (ADR-042)
The MCP server defaults to read-only access for all database operations (`allow_db_writes = false`).
Write tools (`memfuse_insert`, `memfuse_delete`, `memfuse_upsert`, `memfuse_relate`, `memfuse_create_collection`, `memfuse_drop_collection`) are intercepted by the central `McpSandbox::validate_tool_call` guard and rejected with a structured error when write access is disabled.
Write permission must be explicitly enabled via:
- Flag: `--allow-write` (or disabled via `--read-only`)
- Environment variable: `MEMFUSE_MCP_ALLOW_WRITE=1` (or `true`)
- Programmatically: `McpServer::with_write_permission(db, embedder, true)`
