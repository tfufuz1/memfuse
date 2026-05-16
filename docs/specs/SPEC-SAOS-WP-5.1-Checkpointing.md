# SPEC-SAOS-WP-5.1 — Native State Checkpointing & Time-Travel Debugging

> **Priority:** 🔴 KRITISCH — Primärer Migrations-Hebel vs. LangGraph  
> **Dependency:** WP-1.1 DONE, WP-1.2 DONE  
> **Crate:** `memfuse-checkpoint` (neu)  
> **DONE-Definition:** 4 Tests 3× grün. Snapshot-Restore deterministisch.

## Zweck (Das "Warum")

LangGraph und vergleichbare Frameworks sind Black Boxes — wenn ein Agent in Schritt 7
von 12 versagt, muss der gesamte Task-Graph von vorne durchlaufen werden.
MemFuse bietet durch seinen WAL-basierten Ansatz etwas strukturell Überlegenes:
Der WAL *ist bereits* ein Checkpoint-Log. Wir müssen ihn nur zugänglich machen.

## Was ein Checkpoint enthält

Ein Checkpoint ist ein benannter, immutabler Snapshot einer Collection zu einem
definierten Sequenznummer-Zeitpunkt:

```rust
checkpoint = {
    name: String,          // "before_tool_call_7", "after_llm_step_3"
    collection_id: String, // Welche Collection
    seq_no: u64,           // WAL-Sequenznummer zum Zeitpunkt des Checkpoints
    metadata: JsonValue,   // Agenten-State: aktuelle Variablen, Tool-Outputs, etc.
    created_at: u64,       // Unix-Timestamp
}
```

## Kern-API

```rust
// Checkpoint setzen
db.checkpoint("before_tool_call_7", &agent_state_json).await?;

// Liste aller Checkpoints
db.list_checkpoints().await?  // → Vec<CheckpointMeta>

// Time-Travel: Lesender Zugriff auf historischen Zustand
let historical_view = db.open_at_checkpoint("before_tool_call_7").await?;
let result = historical_view.search(query_vec, k=5).await?;

// Fork: Neuen Branch ab Checkpoint starten ("What-if")
let fork = db.fork_from_checkpoint("before_tool_call_7", "what_if_branch").await?;
```

## Invarianten

1. **Immutabilität**: Checkpoints sind read-only nach Erstellung
2. **Isolation**: `open_at_checkpoint` liefert MVCC-View — keine Seiteneffekte
3. **Fork-Safety**: `fork_from_checkpoint` erzeugt vollständig isolierte Collection
4. **WAL-basiert**: Kein Kopieren von Daten — Checkpoint referenziert nur seq_no
5. **Retention Policy**: Checkpoints können explizit oder via TTL gelöscht werden

## Acceptance Criteria (Triple-Test)

| # | Test | Erwartung |
|---|---|---|
| AC-1 | `test_checkpoint_create_and_restore` | 100 Docs → Checkpoint → 50 weitere Docs → open_at_checkpoint → nur 100 sichtbar |
| AC-2 | `test_fork_is_isolated` | fork → insert in fork → original Collection unberührt |
| AC-3 | `test_checkpoint_metadata_roundtrip` | JSON-State gespeichert → nach Restart abrufbar |
| AC-4 | `test_list_checkpoints_ordered` | 5 Checkpoints → list gibt in seq_no-Reihenfolge zurück |

## Integration mit memfuse-core (MVCC)

Checkpoints nutzen die bereits in `memfuse-core` geplante
Snapshot-Isolation (MVCC). `open_at_checkpoint(name)` ist äquivalent zu
`open_snapshot(seq_no)` — nur mit einem benannten Alias.

## Neue Dateien

| Datei | Status |
|---|---|
| `crates/memfuse-checkpoint/src/lib.rs` | DONE |
| `crates/memfuse-checkpoint/src/store.rs` | DONE |
| `crates/memfuse-checkpoint/src/fork.rs` | DONE |
| `crates/memfuse-db/src/lib.rs` | DONE |
