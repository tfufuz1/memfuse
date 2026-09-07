# ADR-010: MCP-Transport — HTTP-REST-Stub → stdio JSON-RPC 2.0

*   **Datum**: 2026-08-23
*   **Status**: ✅ Final
*   **Entscheidung**: `memfuse-mcp` implementiert den stdio-Transport des Model Context Protocol (MCP Spec v2024-11-05) anstelle eines HTTP-REST-Stubs. Alle JSON-RPC-Nachrichten werden zeilenweise über stdin/stdout ausgetauscht.
*   **Alternativen**: SSE+HTTP-Transport (ebenfalls MCP-konform, aber komplexer für lokale Clients).
*   **Begründung**:
    - Claude Desktop, Cursor und andere MCP-Clients erwarten für lokale Server den stdio-Transport per Definition.
    - stdio ist zero-config (kein Port-Binding, keine Firewall-Regeln, kein TLS).
    - Logging wird auf stderr beschränkt, damit stdout ausschließlich dem Protokoll gehört.
    - axum/tower-Abhängigkeiten aus `memfuse-mcp` entfernt; das Crate verwendet nur tokio-util + futures-util als zusätzliche Dependencies (bereits transitiv im Workspace vorhanden).
*   **Konsequenzen**:
    - `mcp.json` im Repo-Root enthält das `mcpServers`-Format für Claude Desktop.
    - Kein HTTP-Listener mehr — der Server kann nicht via curl/Postman direkt getestet werden; stattdessen via `echo '{"jsonrpc":"2.0","method":"tools/list","id":1}' | cargo run --bin memfuse-mcp-server`.

---
