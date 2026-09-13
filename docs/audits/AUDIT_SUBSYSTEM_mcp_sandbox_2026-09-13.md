# Subsystem-Audit: MCP Sandbox (memfuse-mcp/src/sandbox.rs)

## Overview
- **Crate**: `memfuse-mcp`
- **File**: `crates/memfuse-mcp/src/sandbox.rs`
- **Scope**: Zero-Trust Tool-Isolation Layer (`McpSandbox`)
- **Date**: 2026-09-13

---
## MCP-Sandbox-Sub-Audit 2026-09-13T01:25:33Z
- INV-1 SandboxPolicy::default() fail-closed (writes/exec off): [OK]
- INV-2 classify_method() fail-closed auf unbekannte Methoden: [OK]
- INV-3 Volatile-Result-Grenzen vor Insert geprüft: [OK]
- INV-4 Single-Lock-Disziplin (kein geschachtelter Lock): [OK]
- INV-5 Zeroize-Garantie bei Drop/Fehlerpfad: [OK]
- INV-6 execute_with_timeout() lückenlos erzwungen: [OK]

## Detailed Analysis & Findings
- **INV-1 (Default Policy)**: `SandboxPolicy::default()` sets `allow_db_reads: true`, `allow_db_writes: false`, `allow_code_execution: false`, enforcing fail-closed defaults for mutative actions.
- **INV-2 (Whitelist Classification)**: `classify_method()` maps known read/write methods explicitly while defaulting all unclassified methods (`_`) to `ToolCategory::CodeExecution` (fail-closed).
- **INV-3 (Volatile Boundaries)**: `store_volatile()` enforces key length (`<= 256`), output size (`<= 16 MB`), and total entry capacity (`<= 1,000`) before allocation and encryption insertion.
- **INV-4 (Lock Discipline)**: `McpSandbox` holds a single `parking_lot::Mutex<HashMap<String, VolatileToolResult>>` (`volatile_results`) with zero nested lock acquisition.
- **INV-5 (Zeroization)**: `VolatileToolResult` uses `zeroize::Zeroizing<Vec<u8>>` for encrypted payloads and `McpSandbox::drop()` invokes `session_key.emergency_wipe()` to clear session keys upon drop.
- **INV-6 (Execution Timeouts)**: `execute_with_timeout()` wraps tool execution futures in `tokio::time::timeout` bounded by `max_execution_ms`.
