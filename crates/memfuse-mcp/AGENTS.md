# AGENTS.md — memfuse-mcp
> Layer 4 | Model Context Protocol (MCP) Server, Sandbox, Security | ~1600 LOC

## 1. Zweck & Architekturrolle

Implementiert den Model Context Protocol (MCP) Server für MemFuse, wodurch externe Agenten
(wie Claude oder Jules) die Codebasis über standardisierte JSON-RPC 2.0 Schnittstellen
steuern können. Enthält eine harte Security-Sandbox (`McpSandbox`), Prompt-Injection-Guards
und Volatile-Result-Speicherung.

## 2. Modul-Karte

| Datei | Verantwortung |
|---|---|
| `lib.rs` | `#![deny(unsafe_code)]`, `McpServer`, Stdio-Event-Loop |
| `protocol.rs` | `McpError`, JSON-RPC 2.0 Message-Parser und Typen |
| `sandbox.rs` | `McpSandbox`, `SandboxPolicy`, `VolatileToolResult` (verschlüsselter RAM) |
| `prompt_injection.rs` | `PromptInjectionGuard`, `SecurityAuditLogger`, Pattern-Matching |

## 3. Kritische Invarianten

### Nur Stdio-Transport (ADR-010)
Der MCP-Server kommuniziert **ausschließlich** über Stdio (Standard I/O) mittels JSON-RPC 2.0.
Die HTTP-Schnittstelle wurde laut ADR-010 aus Sicherheitsgründen restlos entfernt.
Ein Einbau von `axum` oder `hyper` ist ein Security-Blocker!

### Sandbox-Defaults
Die `SandboxPolicy` definiert harte Grenzen:
- `allow_db_reads`: true
- `allow_db_writes`: false (muss explizit opt-in via Env-Var `MEMFUSE_MCP_WRITE_ALLOW`)
- `allow_code_execution`: false (strikt verboten by default)
Schreibende Operationen (Write-Authorization, ADR-044) werden vor Ausführung blockiert, wenn deaktiviert.

### Prompt-Injection Guard (Quarantäne)
Eingehende Texte (für Embeddings/Graph) passieren den `PromptInjectionGuard`.
Treffer auf Patterns (wie `ignore all previous instructions`) lösen sofortige Quarantäne aus.
Diese Events werden im `SecurityAuditLogger` unwiderruflich erfasst.

### Volatile Results (Anthropic Containment)
Sehr große Tool-Ergebnisse (MAX_VOLATILE_OUTPUT_BYTES = 16 MB) oder sensitive Daten 
werden nicht als JSON im Klartext zurückgeschickt, sondern im `VolatileToolResult` 
RAM-verschlüsselt (via `memfuse-crypto::VolatileEncryptionKey`). Der Agent erhält 
nur einen Reference-Key, den andere Tools einlösen können.
Die Anzahl ist begrenzt (`MAX_VOLATILE_RESULTS` = 1000).

## 4. Public API Quick-Reference

```rust
// === McpServer (lib.rs) ===
pub struct McpServer { ... }
impl McpServer {
    pub fn new(collection: Arc<Collection<LsmStorage>>) -> Self;
    pub fn with_sandbox(self, sandbox: Arc<McpSandbox>) -> Self;
    pub fn with_injection_guard(self, guard: Arc<PromptInjectionGuard>) -> Self;
    pub async fn run_stdio(self: Arc<Self>) -> Result<(), Box<dyn Error>>;
}

// === Sandbox & Security (sandbox.rs, prompt_injection.rs) ===
pub struct SandboxPolicy {
    pub allow_db_reads: bool,
    pub allow_db_writes: bool,
    pub allow_code_execution: bool,
    pub max_execution_ms: u64,
}
pub struct McpSandbox { ... }
impl McpSandbox {
    pub async fn execute_with_timeout<F, T, E>(&self, category: ToolCategory, future: F) -> Result<T, McpError>;
}
pub struct PromptInjectionGuard { ... }
```

## 5. Anti-Patterns & LLM-Fallstricke

```rust
// ❌ FALSCH — Timeout im Tool ignorieren:
let result = tool_call().await;
// ✅ KORREKT — Alle Tool-Ausführungen MÜSSEN durch die Sandbox:
let result = sandbox.execute_with_timeout(ToolCategory::DatabaseRead, tool_call()).await?;

// ❌ FALSCH — HTTP Server hinzufügen:
// ✅ KORREKT — MCP läuft exklusiv über `run_stdio`.
```

## 6. Concurrency & Lock-Hierarchie

`McpSandbox` verwaltet die `VolatileToolResult` in einer `HashMap`, die durch 
ein **einziges**, nicht geschachteltes `parking_lot::Mutex` geschützt ist.
Regel `detect_nested_locks.yml` verbietet geschachtelte Locks innerhalb von Layer 4 strikt.

## 7. Cross-Crate-Schnittstellen & DAG-Grenzen

- **Erlaubte Imports**: Alle L0-L3 Crates (`memfuse-core`, `memfuse-db`, `memfuse-agent`, `memfuse-crypto`)
- **Verbotene Imports**: `memfuse-tauri` (L4 Peer)
- **Genutzt von**: CLI (`memfuse-cli`) und externen MCP-Clients (Cursor, Jules, Claude)

## 8. Relevante ADRs & Rules

| ADR/Rule | Relevanz |
|---|---|
| ADR-010 | Entfernung der HTTP-Schicht (Nur Stdio) |
| ADR-044 | MCP Write-Authorization (Opt-In Security) |
| `rules/detect_nested_locks.yml` | Verbietet Deadlock-anfällige Lock-Schachtelungen |
