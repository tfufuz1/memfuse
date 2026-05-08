# SPEC-SAOS-WP-5.3 — Agent Orchestration Layer

> **Priority:** 🟠 HOCH  
> **Dependency:** WP-5.1 DONE, WP-5.2 DONE, WP-3.1 DONE  
> **Crate:** `memfuse-saos-agent` (neu)  
> **DONE-Definition:** 3 Integration-Tests 3× grün. Agent-Loop läuft 24h ohne Absturz.

## Zweck

Ein strukturiertes API für Multi-Step-Agent-Tasks mit:
- Automatischem Checkpointing vor jedem Tool-Call
- Sandboxed Tool-Execution
- Fehlerbehandlung mit Retry-from-Checkpoint
- Audit-Log aller Agent-Aktionen (unveränderlich im WAL)

## Python-API (Das Cockpit für Entwickler)

```python
import memfuse

agent = memfuse.Agent(
    db_path="./agent_data",
    dimension=1536,
    sandbox_config=memfuse.SandboxConfig(memory_limit_mb=128, cpu_timeout_ms=3000),
)

@agent.tool
def search_knowledge(query: str) -> list[dict]:
    # Tool läuft in WASM-Sandbox
    return agent.collection("knowledge").hybrid_search(query, k=5)

# Task-Ausführung mit automatischem Checkpointing
async with agent.task("summarize_documents") as task:
    # Automatischer Checkpoint vor jedem Step
    step1 = await task.step("load_docs", search_knowledge, query="AI safety")
    step2 = await task.step("analyze", llm_call, context=step1.result)
    
    # Bei Fehler: Replay ab letztem Checkpoint möglich
    # task.replay_from("load_docs")
```

## Kern-Invarianten

1. **Checkpoint-Before-Tool**: Vor jedem `task.step()` wird automatisch ein
   Checkpoint gesetzt — Name: `task:{task_id}:before:{step_name}`
2. **Audit-Log-Immutability**: Alle Agent-Aktionen werden im WAL geloggt —
   nachträglich nicht veränderbar
3. **Graceful-Shutdown**: `Ctrl+C` während eines Steps komplettiert den aktuellen
   WAL-Write und setzt den letzten Checkpoint als Recovery-Point
4. **24/7-Stability**: Kein `unwrap()`, kein Stack-Overflow durch rekursive Graphs

## Acceptance Criteria (Triple-Test)

| # | Test | Erwartung |
|---|---|---|
| AC-1 | `test_agent_auto_checkpoint_before_step` | 3-Step-Task → 3 Checkpoints mit korrekten Namen in DB |
| AC-2 | `test_agent_replay_from_checkpoint` | Step 2 schlägt fehl → replay_from("step_1") → Step 2 wiederholt |
| AC-3 | `test_agent_audit_log_immutable` | Audit-Entries schreiben → nachträgliches Löschen via API → `Err(AuditError::Immutable)` |

## Neue Dateien

| Datei | Status |
|---|---|
| `crates/memfuse-saos-agent/src/lib.rs` | NEU |
| `crates/memfuse-saos-agent/src/task.rs` | NEU: Task + Step Orchestration |
| `crates/memfuse-saos-agent/src/audit.rs` | NEU: Immutable Audit-Log |
| `crates/memfuse-py/src/agent.rs` | MODIFY: PyO3-Bindings für Agent-API |
