# REFACTOR-PLAN: memfuse-checkpoint
**Datei:** `docs/specs/REFACTOR-memfuse-checkpoint.md`
**Erstellt:** 2026-05-28
**Priorität:** HIGH
**Geschätzter Aufwand:** 0.5 Tage
**Voraussetzung:** memfuse-core, memfuse-store

---

## CRATE-ZUSTAND: EHRLICHE BEWERTUNG

| Dimension          | Aktuell       | Ziel          |
|--------------------|---------------|---------------|
| Panic-Freiheit     | 100%          | 100%          |
| Skeleton-Anteil    | 0             | 0             |
| Test-Coverage      | OK            | >90%          |
| API-Vollständigkeit| Gut           | 100%          |
| Algo-Korrektheit   | Partial       | VERIFIED      |

---

## IDENTIFIZIERTE SCHWACHSTELLEN

### HIGH (Core Architecture)

#### FIND-CHK-001: Fehlende Rollbacks bei Fehlerfällen in Checkpoint-Schreibprozessen (S1-B)
**Typ:** Transaction Leak / Locking
**Datei:** `crates/memfuse-checkpoint/src/lib.rs`
**Problem:** In der `AGENTS.md` Anweisung für `@JULES-10` steht: *"Echte LSM und Transaction-Logik muss implementiert werden für `commit`, `rollback`, `flush`"*. `PersistentCheckpointStore` delegiert dies zwar an MemFuse, nutzt dabei aber blind den `?`-Operator. Bei `create_checkpoint` und `drop_checkpoint` wird ein `tx = self.next_tx_id()` allokiert, `self.storage.put/delete` aufgerufen, und bei Erfolg `.commit(tx)` abgesetzt. Wenn `put/delete` scheitert, kehrt die Funktion zurück, ohne jemals `.rollback(tx)` aufzurufen. Das LSM-System behält damit eine "dangling" (Pending) Transaction in seinem Log, die den Memory-State belasten kann.
**Auswirkung:** OOM-Gefahr durch nie-geschlossene Transactions im Error-Fall. Blockierung des LSM.

**Refaktorisierungsanweisung:**
```
1. Verwende Pattern Matching (z.B. `match`) für `put` / `delete`.
2. Wenn `put` fehlschlägt, MUSS explizit `let _ = self.storage.rollback(tx).await;` aufgerufen werden, bevor der `Err` zurückgegeben wird.
3. Äquivalentes Error-Handling auch in `drop_checkpoint` für Storage-Exceptions anwenden.
```

**Akzeptanzkriterien:**
- [ ] Jede in `memfuse-checkpoint` allozierte `tx` (Transaktion) wird endgültig entweder durch `commit` oder `rollback` geschlossen, egal welcher Fehler auftritt.

---

## REFAKTORISIERUNGSREIHENFOLGE

1. FIND-CHK-001 (Transaction Leakage schließen).

## DONE-DEFINITION FÜR DIESES CRATE
- [ ] Fehler-Resilienz bei LSM-Operationen bestätigt.
- [ ] `just triple-test` grün.
