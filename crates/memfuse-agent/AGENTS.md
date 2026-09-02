# AGENTS.md — memfuse-agent
> Layer 3 | Orchestrator, Event-Loop, State Graph, Audit Logging | ~3400 LOC

## 1. Zweck & Architekturrolle

Bildet die Ausführungsumgebung für lokale KI-Agenten. Kapselt die `OrchestratorEngine` (Event-Loop),
den `StateGraph` (Workflow-Definitionen), das `AgentContext` (State-Management pro Task)
und das `AuditLog`. Konsumiert Signale über das `EventSource`-Protokoll und interagiert
mit der zugrundeliegenden `memfuse-db` Collection.

## 2. Modul-Karte

| Datei | Verantwortung |
|---|---|
| `lib.rs` | `#![deny(unsafe_code)]`, Modul-Exports |
| `engine.rs` | `OrchestratorEngine` — Event-Loop (`run_event_loop`), Tool-Registrierung, Replay |
| `graph.rs` | `StateGraph`, `AgentNode`, `WorkflowEdge` — Definition der Agenten-Workflows |
| `context.rs` | `AgentContext`, `AgentStatus` — Zustand eines laufenden Agenten-Tasks |
| `event_source.rs` | `EventSource` Trait, `PollingDocumentEventSource` — Trigger-Quellen |
| `step.rs` | `AgentTool` Trait, `StepResult` — Definition für auszuführende Aktionen |
| `audit.rs` | `AuditLog`, `AuditEntry` — Unveränderliche Aufzeichnung aller Agenten-Entscheidungen |

## 3. Kritische Invarianten

### State-Machine Transitions
Ein Agent (`AgentContext`) durchläuft einen strikten Zustandsautomaten (siehe `AgentStatus`):
`Pending -> Running -> Suspended/Completed/Failed`.
Direkte Manipulationen am Status von außen sind **VERBOTEN**.
Die `OrchestratorEngine` ist die einzige Instanz, die State-Transitions orchestrieren darf.

### Identifier-Validierung (AGT-AGN-001)
`task_id` und `node_id` **MÜSSEN** validiert werden:
- Nicht leer
- Keine Null-Bytes (`\0`)
- Maximale Länge 256 Bytes
- Keine Pfad-Trenner (`/`, `\`)
Verstöße führen zu sofortigem Fehler (`MemFuseError::Validation`).

### Resource Limits
- `MAX_EVENT_SOURCE_CAPACITY`: Ein `EventSource` darf maximal N (z.B. 1000) Events cachen.
- `MAX_TELEMETRY_EVENTS`: Der AuditLog-Puffer darf nicht unbegrenzt im Speicher wachsen.

### Audit-Pflicht
Jede State-Transition, Tool-Ausführung und jeder Fehler **MUSS** im `AuditLog` protokolliert werden.
Das AuditLog schreibt atomar über die LSM-Engine (`__agent:audit:` Präfix) in den WAL.

## 4. Public API Quick-Reference

```rust
// === OrchestratorEngine (engine.rs) ===
pub struct OrchestratorEngine { ... }
impl OrchestratorEngine {
    pub fn new(storage: Arc<LsmStorage>) -> Self;
    pub fn try_register_tool(&mut self, tool: Box<dyn AgentTool>) -> Result<()>;
    pub async fn run(&self, ctx: &mut AgentContext, graph: &StateGraph) -> Result<()>;
    pub async fn run_event_loop(&self, sources: Vec<Box<dyn EventSource>>, graph: &StateGraph) -> Result<EventLoopExitReason>;
}

// === AgentContext & Graph (context.rs, graph.rs) ===
pub struct AgentContext { ... }
impl AgentContext {
    pub fn try_new(task_id: &str, initial_node: &str) -> Result<Self>;
}

pub struct StateGraph { ... }
impl StateGraph {
    pub fn try_add_node(&mut self, node: AgentNode) -> Result<()>;
    pub fn try_add_edge(&mut self, from: &str, to: &str, condition: Option<&str>, priority: u8) -> Result<()>;
}

// === EventSource (event_source.rs) ===
pub trait EventSource: Send + Sync {
    async fn next_event(&self) -> Result<Option<BackgroundEvent>>;
}
pub struct PollingDocumentEventSource<S: StorageEngine> { ... }
```

## 5. Anti-Patterns & LLM-Fallstricke

```rust
// ❌ FALSCH — Ungeprüfte Task-IDs (Security Risk):
let ctx = AgentContext { task_id: user_input.into(), ... };
// ✅ KORREKT:
let ctx = AgentContext::try_new(user_input, initial_node)?; // Validiert automatisch!

// ❌ FALSCH — Event-Loop ohne Exit-Bedingung:
// ✅ KORREKT — run_event_loop liefert EventLoopExitReason, das ausgewertet werden muss.

// ❌ FALSCH — Eigene Fehler-Typen in AgentTools:
// ✅ KORREKT — Alle Tools MÜSSEN den globalen `MemFuseError` verwenden.
```

## 6. Concurrency & Lock-Hierarchie

`OrchestratorEngine` führt Tasks aus. Agent-Tools (`AgentTool`) rufen
oft die `Collection` (Layer 2) auf und durchlaufen dort die Lock-Hierarchie.
Die `OrchestratorEngine` selbst hält keine globalen Locks, die Tool-Ausführungen
blockieren würden.
**Wichtig:** Ein Tool darf keine async-Pausen einlegen, während es Locks aus anderen Crates hält!

## 7. Cross-Crate-Schnittstellen & DAG-Grenzen

- **Erlaubte Imports**: `memfuse-core` (L0), `memfuse-store` (L1), `memfuse-db` (L2)
- **Verbotene Imports**: `memfuse-mcp` (L4), `memfuse-tauri` (L4)
- **Genutzt von**: `memfuse-mcp`, `memfuse-router`, `memfuse-tauri`

## 8. Relevante ADRs & Rules

| ADR/Rule | Relevanz |
|---|---|
| ADR-030 | Agent State Machine & Orchestrator Design |
| `rules/llm_protocol.md` | Identifier-Validierung |
