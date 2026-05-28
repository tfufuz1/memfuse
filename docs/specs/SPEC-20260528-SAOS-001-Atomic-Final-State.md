# Atomic Spec: FIND-SAOS-001 Atomic Final State

## Kontext / Ziel
Gewährleistung, dass der Abschluss eines Agenten-Workflows (`NodeType::End`) atomar und wiederherstellbar ist. Dies verhindert, dass ein Agent nach einem Absturz kurz vor dem Ende in einem inkonsistenten Zustand verbleibt (z.B. "Running", obwohl er eigentlich fertig war).

## Die Invariante(n)
- **[INV-SAOS-001]**: Bevor die Markierung `task:{id}:final` in der State-Collection geschrieben wird, MUSS ein persistenter Checkpoint existieren, der den Zustand des Agenten am `End`-Knoten abbildet.
- **[INV-SAOS-002]**: Fehler beim Erstellen des Checkpoints am Ende MÜSSEN den Prozess abbrechen (`?`-Operator), um zu verhindern, dass ein nicht-wiederherstellbarer Final-State geschrieben wird.

## Speicherort / Betroffene Crate
- **Crate**: `memfuse-saos-agent`
- **Datei**: `crates/memfuse-saos-agent/src/engine.rs`

## Datenstrukturen
Keine neuen Strukturen erforderlich. Bestehende `OrchestratorEngine` und `AgentContext` werden genutzt.

## Fail-Cases
- Wenn `self.checkpoint(ctx).await?` fehlschlägt, wird der Workflow mit einem `MemFuseError` abgebrochen. Der `persist_final_state` Aufruf erfolgt nicht.
- Bei einem Replay zu diesem Checkpoint landet der Agent erneut am `End`-Knoten und kann den Abschluss-Prozess idempotent wiederholen.
