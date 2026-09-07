# ADR-039: reqwest als Workspace-Dependency für memfuse-router

*   **Datum**: 2026-08-29
*   **Status**: ✅ Final
*   **Entscheidung**: `reqwest` wird als zentrale Workspace-Dependency in `[workspace.dependencies]` im Root-`Cargo.toml` aufgenommen und für `memfuse-router` explizit freigegeben.
*   **Alternativen**: Ersetzung durch `memfuse-ollama`.
*   **Begründung**: `memfuse-router` nutzt `reqwest` in `dispatch_to_slm` für generische HTTP JSON-RPC 2.0 Aufrufe (`slm_process_context`) an frei konfigurierbare MCP-Endpunkte von Small Language Models (SLMs). `memfuse-ollama` deckt ausschließlich Ollama REST-API-Endpunkte ab und kann diese generische JSON-RPC-MCP-Dispatch-Funktionalität nicht bereitstellen.
*   **Sicherheitsbewertung**: Nutzung mit `default-features = false` und `rustls-tls` (kein `native-tls` / OpenSSL C-Dependency-Overhead, vollständig konform mit der Sovereign Core Policy aus ADR-004).
*   **Konsequenz**: `reqwest` ist fortan eine explizit genehmigte Workspace-Dependency ohne Version Drift zwischen Crates.

---
