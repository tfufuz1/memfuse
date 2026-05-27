# SPEC-SAOS-WP-5.2 — WASM Tool Execution Sandbox

> **Priority:** 🟠 HOCH — Defense USP  
> **Dependency:** WP-3.1 DONE (Python API stabil)  
> **Crate:** `memfuse-sandbox` (neu)  
> **DONE-Definition:** 3 Tests 3× grün. Memory-Limit und CPU-Timeout erzwungen.

## Zweck (Das "Warum")

Agenten führen Tools aus (Code, Shell-Commands, HTTP-Calls). Im aktuellen Stand
läuft dies direkt im Host-Prozess — ein kompromittiertes Tool kann die gesamte
Datenbank korrumpieren. Die WASM-Sandbox isoliert Tool-Execution:

```
Vorher: Python-Agent → Tool-Funktion() → Host-Prozess (DB-Zugriff möglich)
Nachher: Python-Agent → WASM-Runtime → Tool-Wasm-Modul (isoliert, kein DB-Zugriff)
```

## Architektur

`memfuse-sandbox` nutzt `wasmtime` (Bytecode Alliance) als WASM-Runtime.
Tools werden als .wasm-Module kompiliert und in der Sandbox ausgeführt.
Die Sandbox hat keinen Zugriff auf:
- Das Dateisystem (außer explizit gemounteten virtuellen Pfaden)
- Netzwerk (opt-in per Policy)
- Den MemFuse-Store direkt (nur über definierte Host-Functions)

## Sandbox-Config

```rust
SandboxConfig {
    memory_limit_mb: u32,    // Default: 64MB
    cpu_timeout_ms: u64,     // Default: 5000ms
    allow_network: bool,     // Default: false
    allowed_host_functions:  // Explizit erlaubte DB-Ops
        Vec<"db.search" | "db.insert" | "db.get">,
}
```

## Tool-Interface (Host Functions)

```rust
// Tools können nur über diese Funktionen mit MemFuse interagieren:
extern "C" {
    fn db_search(query_ptr: i32, query_len: i32, k: i32) -> i32;  // returns JSON ptr
    fn db_insert(key_ptr: i32, key_len: i32, vec_ptr: i32, vec_len: i32) -> i32;
    fn db_get(key_ptr: i32, key_len: i32) -> i32;
}
```

## Acceptance Criteria (Triple-Test)

| # | Test | Erwartung |
|---|---|---|
| AC-1 | `test_sandbox_memory_limit_enforced` | WASM-Modul alloziert > memory_limit → `Err(SandboxError::MemoryLimitExceeded)` |
| AC-2 | `test_sandbox_cpu_timeout_enforced` | WASM-Modul in Endlosschleife → nach cpu_timeout_ms → `Err(SandboxError::Timeout)` |
| AC-3 | `test_sandbox_cannot_access_host_fs` | WASM-Modul versucht Datei zu öffnen → `Err(SandboxError::PolicyViolation)` |

## Erlaubte Dependency

```toml
wasmtime = "20"  # Bytecode Alliance, industrieller Standard
```

## Neue Dateien

| Datei | Status |
|---|---|
| `crates/memfuse-sandbox/src/lib.rs` | NEU |
| `crates/memfuse-sandbox/src/runtime.rs` | NEU: wasmtime Wrapper |
| `crates/memfuse-sandbox/src/host_functions.rs` | NEU: DB-Zugriffs-Interface |
| `crates/memfuse-sandbox/src/policy.rs` | NEU: SandboxConfig Enforcement |
