# ADR-044: MCP Write-Authorization & Sandbox Policy (Default Read-Only)

*   **Datum**: 2026-08-30
*   **Status**: ✅ Final
*   **Entscheidung**: `memfuse-mcp` erzwingt eine strikte Sandbox-Policy für alle MCP Tool-Aufrufe. Datenbank-Schreibzugriffe (`DatabaseWrite` Tools wie `memfuse_insert`, `memfuse_delete`, `memfuse_upsert`, `memfuse_relate`, `memfuse_create_collection`, `memfuse_drop_collection`) sind standardmäßig GESPERRT (`allow_db_writes = false`). Schreibberechtigungen können ausschließlich explizit per Aufruf-Parameter/Server-Initialisierung (`McpServer::with_write_permission()`) bzw. Umgebungsvariable `MEMFUSE_MCP_ALLOW_WRITE=true` aktiviert werden. Vor jedem Tool-Dispatch prüft `call_tool` zentral `McpSandbox::validate_tool_call()`.
*   **Alternativen**:
    - Uneingeschränkter Schreibzugriff im Default: Verworfen aus Sicherheitsgründen (Zero-Trust/Least-Privilege Prinzipsschutz für LLM-MCP-Integrationen).
    - Einzelne Tool-Gefahrenstufen ohne zentrale Sandbox-Validierung: Verworfen, da dezentrale Prüfungen fehleranfällig und schwer zu auditieren sind.
*   **Begründung**: Schutz der lokalen Knowledge Base vor unbeabsichtigten oder böswilligen Schreib- und Löschoperationen durch extern gesteuerte MCP-Clients (R-01 Containment Protection).

---
