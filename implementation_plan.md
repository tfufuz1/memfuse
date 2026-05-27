# WP-5.3 Implementation Plan: StateGraph & Agent Execution Loop

> **Lead Architect Plan** — Für Jules-Agents und Implementierer  
> **Datum:** 2026-05-27  
> **Status:** DRAFT — Pending User Approval  
> **Crate:** `memfuse-saos-agent` (L2 Orchestration)

---

## 1. Zielsetzung

Das `memfuse-saos-agent` Crate von einem leeren Scaffold in eine funktionale **Declarative Agent Workflow Engine** überführen. Jeder Agent-Schritt ("Thought") wird als Graph-Walk über den CSR-Graphen modelliert und **zwingend LSM-persistent** committet, bevor der nächste Schritt ausgeführt werden darf.

## 2. Ist-Zustand (Forensic Audit)

### Was existiert und funktioniert ✅

| Crate | Status | Beweis |
|:------|:-------|:-------|
| `memfuse-graph/csr.rs` | **475 LoC, voll implementiert** | BFS-Traversierung mit Score-Decay (0.7^hop), CSR-Compaction, 5 Tests grün |
| `memfuse-db/transaction.rs` | **224 LoC, voll implementiert** | 2-Phase-Commit mit Intent-WAL, Compensating TX (3 Retries), Split-Brain-Logging |
| `memfuse-core/traits.rs` | **249 LoC, stabil** | [GraphIndex](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs#209-238), [StorageEngine](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs#61-120), [VectorIndex](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs#127-171) Traits mit nativen async fn |
| `memfuse-core/types/domain.rs` | **stabil** | [Entity](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/domain.rs#172-177), [Edge](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/domain.rs#190-196), [EntityId](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/domain.rs#65-66), [WorkflowState](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/domain.rs#6-12) |
| `memfuse-checkpoint` | **stabil** | [PersistentCheckpointStore](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-checkpoint/src/lib.rs#54-62) mit LSM-Backend |

### Was leer/Stub ist ❌

| Datei | LoC | Problem |
|:------|:----|:--------|
| [memfuse-saos-agent/src/lib.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-saos-agent/src/lib.rs) | 111 | `OrchestratorEngine::execute()` → `Ok(())` (Stub) |
| [memfuse-saos-agent/src/graph.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-saos-agent/src/graph.rs) | 60 | `StateGraph::run_workflow()` → leerer Body (Stub) |
| [memfuse-sandbox/src/lib.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-sandbox/src/lib.rs) | 87 | `WasmSandbox::execute_isolated()` → `Ok(Vec::new())` (Stub) |
| [memfuse-sandbox/src/sandbox.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-sandbox/src/sandbox.rs) | 48 | `WasmSandbox::execute()` → Placeholder-String (Stub) |

### Duplikation / Architektur-Drift ⚠️

- **Zwei konkurrierende [StateGraph](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-saos-agent/src/graph.rs#22-27) Definitionen**: Eine in [lib.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-py/src/lib.rs) (mit [GraphNode](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-saos-agent/src/lib.rs#29-33)/[WorkflowEdge](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-saos-agent/src/lib.rs#36-41) Structs) und eine in [graph.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-saos-agent/src/graph.rs) (mit [AgentNode](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-saos-agent/src/graph.rs#14-18)/`NodeId` HashMap). Diese müssen **unifiziert** werden.
- **Zwei konkurrierende [WasmSandbox](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-sandbox/src/sandbox.rs#30-34) Definitionen**: Eine in [lib.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-py/src/lib.rs) (Trait-basiert) und eine in [sandbox.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-sandbox/src/sandbox.rs) (Config-basiert). Diese müssen **unifiziert** werden.

---

## 3. Architektur-Entscheidungen

### AD-1: Middle-Out Strategie
Wir definieren zuerst die High-Level Graph-Logik (Pfadfindung, Knoten-Typen, Edge-Evaluation), aber **jede Struktur wird von Tag 1 mit einem [TxId](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/domain.rs#94-95)-Slot für Persistenz entworfen**.

### AD-2: Dependency Wiring
```
memfuse-saos-agent
  ├── memfuse-core        (Traits, Types)
  ├── memfuse-db           (NEU: Collection + DbTransaction Zugriff)
  ├── memfuse-graph        (NEU: CsrGraph für Zustandsnavigation)
  └── memfuse-checkpoint   (NEU: Snapshot vor jedem Step)
```

### AD-3: Kein WASM in Phase 1
`memfuse-sandbox` bleibt FROZEN. Agent-Tools werden als native Rust `async fn` Closures ausgeführt, nicht als WASM-Module. Das reduziert die Implementierungskomplexität drastisch und erlaubt uns, den Execution-Loop zuerst stabil zu machen.

---

## 4. Implementierungsplan (4 Phasen)

### Phase 1: Unifizierung & Cleanup (Agent: 09 oder 13)

> **Ziel:** Duplikation eliminieren, einheitliches Datenmodell schaffen.

#### [MODIFY] [lib.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-saos-agent/src/lib.rs)
- Entferne die alten [GraphNode](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-saos-agent/src/lib.rs#29-33), [WorkflowEdge](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-saos-agent/src/lib.rs#36-41), [StateGraph](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-saos-agent/src/graph.rs#22-27), [OrchestratorEngine](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-saos-agent/src/lib.rs#65-66) Structs
- Re-exportiere die unifizierte API aus den neuen Submodulen

#### [MODIFY] [graph.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-saos-agent/src/graph.rs)
- Ersetze [AgentNode](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-saos-agent/src/graph.rs#14-18) / lose Tuple-Edges durch typisiertes Modell:

```rust
pub struct AgentNode {
    pub id: NodeId,
    pub description: String,
    pub node_type: NodeType, // NEU: Task | Decision | End | Start
    pub handler: Option<String>, // NEU: Registered tool/function name
}

pub enum NodeType {
    Start,
    Task,       // Führt eine Action aus
    Decision,   // Evaluiert Bedingung, wählt Kante
    End,
}

pub struct WorkflowEdge {
    pub from: NodeId,
    pub to: NodeId,
    pub condition: Option<String>,  // z.B. "on_success", "on_failure"
    pub priority: u8,               // NEU: Für deterministische Auswahl bei mehreren Kanten
}
```

#### [MODIFY] [Cargo.toml](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-saos-agent/Cargo.toml)
- `memfuse-db`, `memfuse-graph`, `memfuse-checkpoint` als Dependencies hinzufügen
- `serde`, `serde_json`, `tracing`, `tokio` zu Dependencies

---

### Phase 2: AgentContext & Step-Abstraktion (Agent: 09)

> **Ziel:** Den persistenten Agenten-Zustand und die Step-Schnittstelle definieren.

#### [NEW] [context.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-saos-agent/src/context.rs)

```rust
pub struct AgentContext {
    /// Aktiver Knoten im Workflow-Graph
    pub current_node: NodeId,
    /// Zähler für ausgeführte Steps (monoton steigend)
    pub step_count: u64,
    /// Referenz auf die MemFuse DB für persistente Speicherung
    pub db: Arc<MemFuse>,
    /// Collection für Agent-State Storage
    pub state_collection: Arc<Collection>,
    /// Token-Budget Tracker
    pub budget: TokenBudget,
    /// Akkumulierter Kontext (Ergebnisse vorheriger Steps)
    pub memory: HashMap<String, serde_json::Value>,
}
```

#### [NEW] [step.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-saos-agent/src/step.rs)

```rust
/// Ergebnis eines einzelnen Agenten-Schritts.
pub struct StepResult {
    pub node_id: NodeId,
    pub output: serde_json::Value,
    pub tokens_consumed: usize,
    pub next_edge: Option<String>,   // Welche Kante soll genommen werden?
}

/// Trait für registrierbare Agent-Tools.
pub trait AgentTool: Send + Sync {
    fn name(&self) -> &str;
    async fn execute(&self, ctx: &AgentContext, input: serde_json::Value) -> Result<StepResult>;
}
```

---

### Phase 3: Execution Loop (Agent: 09, Review: 07)

> **Ziel:** Den deterministischen, persistenten Graph-Walker implementieren.

#### [NEW] [engine.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-saos-agent/src/engine.rs)

Die Kernlogik des gesamten Agent-Systems:

```rust
impl OrchestratorEngine {
    pub async fn run(&self, ctx: &mut AgentContext, graph: &StateGraph) -> Result<()> {
        loop {
            let node = graph.get_node(&ctx.current_node)
                .ok_or(MemFuseError::Internal("Node not found"))?;

            match node.node_type {
                NodeType::End => {
                    self.persist_final_state(ctx).await?;
                    return Ok(());
                }
                NodeType::Start | NodeType::Task => {
                    // 1. Checkpoint BEFORE execution (AC-1)
                    self.checkpoint(ctx).await?;

                    // 2. Execute the registered tool
                    let result = self.execute_tool(ctx, node).await?;

                    // 3. Atomic commit of step result to LSM
                    self.commit_step(ctx, &result).await?;

                    // 4. Audit log (immutable, AC-3)
                    self.audit_log(ctx, &result).await?;

                    // 5. Budget check
                    if ctx.budget.available() == 0 {
                        return Err(MemFuseError::Internal("Token budget exhausted"));
                    }

                    // 6. Resolve next edge
                    ctx.current_node = self.resolve_next_node(graph, &ctx.current_node, &result)?;
                    ctx.step_count += 1;
                }
                NodeType::Decision => {
                    // Evaluate condition without tool execution
                    let next = self.evaluate_decision(graph, node, ctx)?;
                    ctx.current_node = next;
                }
            }
        }
    }
}
```

**Invarianten des Loops:**
- [checkpoint()](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-checkpoint/src/lib.rs#243-247) → Schreibt Snapshot via [PersistentCheckpointStore](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-checkpoint/src/lib.rs#54-62) mit Key `task:{task_id}:before:{step_name}`
- `commit_step()` → Nutzt [DbTransaction](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/transaction.rs#17-24) für atomaren 2-Phase-Commit ins LSM
- [audit_log()](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-saos-agent/src/lib.rs#100-110) → Append-Only WAL-Eintrag, kein Delete/Update möglich (AC-3)
- `resolve_next_node()` → Deterministische Kanten-Auswahl nach `priority` + `condition`

---

### Phase 4: Audit-Log & Replay (Agent: 09, Review: 07)

> **Ziel:** Immutable Audit-Trail und Replay-from-Checkpoint (AC-2, AC-3)

#### [NEW] [audit.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-saos-agent/src/audit.rs)

```rust
pub struct AuditLog {
    collection: Arc<Collection>,
}

impl AuditLog {
    /// Appends an immutable audit entry. Delete/Update via this interface is impossible.
    pub async fn append(&self, entry: &AuditEntry) -> Result<()> { ... }

    /// Replays all audit entries for a given task.
    pub async fn replay_task(&self, task_id: &str) -> Result<Vec<AuditEntry>> { ... }
}
```

#### Replay-from-Checkpoint Mechanismus:
```rust
impl OrchestratorEngine {
    /// Setzt den AgentContext auf einen früheren Checkpoint zurück (AC-2).
    pub async fn replay_from(&self, ctx: &mut AgentContext, step_name: &str) -> Result<()> {
        let checkpoint_name = format!("task:{}:before:{}", ctx.task_id, step_name);
        let checkpoint = self.checkpoint_store.get_checkpoint(&checkpoint_name).await?
            .ok_or(MemFuseError::Internal("Checkpoint not found"))?;
        // Restore state from checkpoint seq_no
        ctx.current_node = /* restored from checkpoint metadata */;
        ctx.step_count = /* restored */;
        Ok(())
    }
}
```

---

## 5. Zu erstellende Dateien (Zusammenfassung)

| Datei | Typ | Beschreibung |
|:------|:----|:-------------|
| [src/lib.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-py/src/lib.rs) | MODIFY | Cleanup + Re-exports |
| [src/graph.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-saos-agent/src/graph.rs) | MODIFY | Unifiziertes [StateGraph](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-saos-agent/src/graph.rs#22-27) + `NodeType` |
| [src/context.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/context.rs) | NEW | `AgentContext` mit DB-Referenz + Budget |
| `src/step.rs` | NEW | `StepResult` + `AgentTool` Trait |
| `src/engine.rs` | NEW | `OrchestratorEngine::run()` Loop |
| `src/audit.rs` | NEW | Immutable `AuditLog` |
| [Cargo.toml](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/Cargo.toml) | MODIFY | Dependencies erweitert |

## 6. Verifikationsplan

### Automatisierte Tests (Triple-Test-Gate)
```bash
# Alle 3 ACs aus der Spec müssen bestehen, 3× hintereinander
cargo test -p memfuse-saos-agent test_agent_auto_checkpoint_before_step
cargo test -p memfuse-saos-agent test_agent_replay_from_checkpoint
cargo test -p memfuse-saos-agent test_agent_audit_log_immutable

# Crosscrate-Integration
cargo test -p memfuse-saos-agent --test e2e_integration

# Clippy Gate
cargo clippy -p memfuse-saos-agent -- -D warnings
```

### Manuelle Verifikation
- Crash-Injection: `kill -9` während Step-Execution → Restart muss am letzten Checkpoint fortsetzen
- Token-Budget: Setze `max_tokens: 10` → Engine muss nach Erschöpfung sauber terminieren

## 7. Was explizit NICHT in diesen Plan fällt

| Thema | Grund |
|:------|:------|
| WASM Sandbox (`memfuse-sandbox`) | Bleibt FROZEN — Tools laufen als native Closures |
| Python Bindings für Agent API | Phase 2 — erst nach stabilem Rust-API |
| DiskANN (WP-4.3) | Orthogonal, L1-Optimierung |
| MCP Provider (WP-7.3) | Phase 3 — benötigt stabilen Agent-Loop |
