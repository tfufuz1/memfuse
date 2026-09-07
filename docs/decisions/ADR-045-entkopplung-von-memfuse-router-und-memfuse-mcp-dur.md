# ADR-045: Entkopplung von `memfuse-router` und `memfuse-mcp` durch IPC JSON-RPC Typverschiebung

*   **Datum**: 2026-08-31
*   **Status**: ✅ Final
*   **Entscheidung**: Die generischen JSON-RPC 2.0 Protokolltypen (`JsonRpcRequest`, `JsonRpcResponse`, `JsonRpcError`) werden aus `memfuse-mcp` nach `memfuse-core::ipc::jsonrpc` verschoben und in `memfuse-mcp::protocol` re-exportiert. `memfuse-router` importiert diese Typen fortan direkt aus `memfuse-core::ipc`. Die Abhängigkeit `memfuse-mcp` wird aus `crates/memfuse-router/Cargo.toml` sowie aus den Ausnahmeregeln in `.github/workflows/dag-check.yml` entfernt.
*   **Alternativen**:
    - Erstellung eines separaten `memfuse-jsonrpc`-Crates in Layer 1: Verworfen, um Crate-Explosion zu vermeiden; `memfuse-core::ipc` existiert bereits als zentrales IPC-Typ-Modul in Layer 0.
    - Beibehaltung der Layer-4-Dependency in Layer 3: Verworfen, da dies das 5-Layer-DAG-Modell verletzt und Zirkelbezüge zwischen Router und MCP verhindert.
*   **Begründung**: Beseitigt die Schichtgrenzenverletzung (Layer 3 → Layer 4) ohne Verhaltensänderung oder Breaking Changes für externe Konsumenten von `memfuse_mcp::protocol::*`.

---
