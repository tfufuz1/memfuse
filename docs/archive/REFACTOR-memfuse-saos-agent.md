# REFACTOR-PLAN: memfuse-saos-agent
**Datei:** `docs/specs/REFACTOR-memfuse-saos-agent.md`
**Erstellt:** 2026-05-28
**Priorität:** HIGH
**Geschätzter Aufwand:** 0.5 Tage
**Voraussetzung:** memfuse-core, memfuse-checkpoint

---

## CRATE-ZUSTAND: EHRLICHE BEWERTUNG

| Dimension          | Aktuell       | Ziel          |
|--------------------|---------------|---------------|
| Panic-Freiheit     | 100%          | 100%          |
| Skeleton-Anteil    | 0             | 0             |
| Test-Coverage      | OK            | >90%          |
| API-Vollständigkeit| Gut           | 100%          |
| Algo-Korrektheit   | Crash-Lücke   | VERIFIED      |

---

## IDENTIFIZIERTE SCHWACHSTELLEN

### HIGH (Core Orchestration)

#### FIND-SAOS-001: Fehlender Final-Checkpoint bei `NodeType::End` (Crash Recovery Lücke) (S3-A)
**Typ:** Crash Recovery / State Inconsistency
**Datei:** `crates/memfuse-saos-agent/src/engine.rs`
**Problem:** In der `AGENTS.md` Anweisung für `@JULES-09` steht: *"persist_final_state muss atomar den Graph-State fixieren und Crash Recovery erlauben"*.
Im `OrchestratorEngine::run()` Loop wird beim Erreichen von `NodeType::End`:
1. `ctx.status = Completed` gesetzt.
2. `self.persist_final_state(ctx).await?;` aufgerufen.
3. `return Ok(())` aufgerufen.
Es wird jedoch KEIN `self.checkpoint(ctx).await;` wie bei `NodeType::Task` erstellt. Wenn die Agent-Host-Schicht abstürzt, direkt nachdem `persist_final_state` beendet ist, oder auch währenddessen, kann der Agent nicht korrekt auf den Status `Completed` wiederhergestellt werden, weil der Checkpointer diese Mutation nicht abgebildet hat.
**Auswirkung:** Agenten, die crashen, könnten bei Replay die letzte Task fälschlicherweise nochmal ausführen, was Seiteneffekte in Produktionssystemen verdoppelt (z.B. API-Calls).

**Refaktorisierungsanweisung:**
```
1. Rufe in `OrchestratorEngine::run()` im `NodeType::End` Match-Zweig `self.checkpoint(ctx).await?;` VOR `self.persist_final_state(ctx).await?;` auf.
2. Alternativ: Binde `persist_final_state` und den Checkpoint in eine einzige transaktionale Klammer.
```

**Akzeptanzkriterien:**
- [ ] Bei Erreichen eines `End` Nodes ist sichergestellt, dass der Checkpoint-Store den Workflow als `Completed` markiert.

---

## REFAKTORISIERUNGSREIHENFOLGE

1. FIND-SAOS-001 (Final-State Atomicity).

## DONE-DEFINITION FÜR DIESES CRATE
- [ ] Kein "Double-Run" von finalen Tasks bei Agent-Crashes.
- [ ] `just triple-test` grün.
