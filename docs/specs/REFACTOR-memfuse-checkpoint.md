# REFACTOR-PLAN: memfuse-checkpoint
**Datei:** `docs/specs/REFACTOR-memfuse-checkpoint.md`
**Erstellt:** 2026-05-28
**Priorität:** MEDIUM (Da FROZEN)
**Geschätzter Aufwand:** 1 Tag
**Voraussetzung:** memfuse-core, memfuse-store

---

## CRATE-ZUSTAND: EHRLICHE BEWERTUNG

| Dimension          | Aktuell       | Ziel          |
|--------------------|---------------|---------------|
| ACID-Compliance    | ⚠️ Leak-Gefahr | ATOMIC        |
| ID-Isolation       | ✅ Exzellent   | VERIFIED      |
| Cache-Konsistenz   | ✅ Gut         | ATOMIC        |
| Test-Abdeckung     | ✅ Hoch        | VERIFIED      |

---

## IDENTIFIZIERTE SCHWACHSTELLEN

### HIGH (Pre-Launch — Innerhalb von 1 Woche)

#### FIND-CHK-001: Transaction Leak on Failure
**Typ:** Datenintegrität / Stabilität
**Datei:** `crates/memfuse-checkpoint/src/lib.rs`
**Zeilen:** 104–151 (`create_checkpoint`), 195–220 (`drop_checkpoint`)
**Problem:** Wenn `storage.put` oder `storage.commit` fehlschlägt, wird der Transaktions-Slot im Storage-Engine nicht freigegeben (kein Rollback).
**Auswirkung:** Verwaiste Transaktionen in der WAL/Memtable, die Ressourcen binden.
**Sovereign Core Verstoß:** ACID-Atomarität.

**Refaktorisierungsanweisung:**
```rust
1. Wickle Storage-Operationen in ein Result-Mapping ein.
2. Implementiere ein explizites Rollback im Fehlerfall:
   ```rust
   let tx = self.next_tx_id();
   if let Err(e) = self.storage.put(tx, ...).await {
       self.storage.rollback(tx).await.ok(); // Best effort rollback
       return Err(e);
   }
   if let Err(e) = self.storage.commit(tx).await {
       self.storage.rollback(tx).await.ok();
       return Err(e);
   }
   ```
```

**Akzeptanzkriterien:**
- [ ] Test `test_checkpoint_rollback_on_failure` (mit MockStorage, der Fehler provoziert) beweist: `rollback()` wird aufgerufen.

---

## REFAKTORISIERUNGSREIHENFOLGE

```
Schritt 1: Rollback-Garantie in create_checkpoint
Schritt 2: Rollback-Garantie in drop_checkpoint
```

## DONE-DEFINITION FÜR DIESES CRATE

- [ ] Alle Transaktionspfade haben explizite Rollback-Guards.
- [ ] `just triple-test -p memfuse-checkpoint` 3× grün.
