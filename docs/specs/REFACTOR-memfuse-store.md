# REFACTOR-PLAN: memfuse-store
**Datei:** `docs/specs/REFACTOR-memfuse-store.md`
**Erstellt:** 2026-05-28
**Priorität:** CRITICAL
**Geschätzter Aufwand:** 3 Tage
**Voraussetzung:** memfuse-core, memfuse-crypto

---

## CRATE-ZUSTAND: EHRLICHE BEWERTUNG

| Dimension          | Aktuell       | Ziel          |
|--------------------|---------------|---------------|
| Panic-Freiheit     | 98% sauber    | 100%          |
| Skeleton-Anteil    | 2 Stellen     | 0             |
| Test-Coverage      | ~85%          | >95%          |
| API-Vollständigkeit| 80%           | 100%          |
| Algo-Korrektheit   | ❌ ROLLBACK-BUG| VERIFIED      |

---

## IDENTIFIZIERTE SCHWACHSTELLEN

### BLOCKING (Release-Blocker — SOFOT)

#### FIND-STO-003: Rollback Inconsistency (Stale SSTables)
**Typ:** Korrektheit / Datenintegrität
**Datei:** `crates/memfuse-store/src/lsm.rs`
**Zeilen:** 361–371
**Code (Kontext):**
```rust
sstables_lock.retain(|sst| {
    if sst.metadata().min_tx_id > target_tx.inner() {
        sst_to_remove.push(sst.file_path().to_path_buf());
        false
    } else { true }
});
```
**Problem:** SSTables, die sowohl Transactions vor als auch nach `target_tx` enthalten, werden NICHT gelöscht. Da SSTables im LSM-Tree unveränderlich sind, enthalten sie weiterhin "Zukunfts-Daten", die bei GET/SCAN Operationen nach dem Rollback fälschlicherweise sichtbar sind.
**Auswirkung:** Rollback ist unvollständig. Zeitreise-Inkonsistenz.

**Refaktorisierungsanweisung:**
```
1. Ändere die Lösch-Logik: JEDE SSTable mit `max_tx_id > target_tx` muss behandelt werden.
2. Falls `min_tx_id <= target_tx` AND `max_tx_id > target_tx`:
   - Lade die SSTable-Entries.
   - Filtere alle Entries > target_tx aus.
   - Schreibe eine neue, reduzierte SSTable (Re-Write).
   - Ersetze die alte SSTable durch die neue.
3. Nur SSTables, deren `max_tx_id <= target_tx` ist, dürfen unverändert bleiben.
```

**Akzeptanzkriterien:**
- [ ] Test `test_lsm_rollback_partial_sstable` beweist: Daten aus der "Zukunft" einer teilweise betroffenen SSTable verschwinden nach Rollback.

---

#### FIND-STO-004: MVCC-Loss during Flush
**Typ:** Korrektheit / MVCC
**Datei:** `crates/memfuse-store/src/lsm.rs`
**Zeilen:** 630–632
**Code (Kontext):**
```rust
for (k, v, seq, tx) in old_memtable.iter_latest() {
    builder.add(&k, &v, seq, tx).await?;
}
```
**Problem:** `iter_latest()` verwirft beim Flush alle historischen Versionen im MemTable. Falls jedoch noch ein Reader-Snapshot auf `seq_old` zeigt, der noch nicht in einer SSTable persistiert wurde, verliert dieser Reader beim Swap seine Daten.
**Auswirkung:** Snapshot Isolation Violation. Silent Data Loss für aktive Snapshots.

**Refaktorisierungsanweisung:**
```
1. Verwende `old_memtable.iter()` (alle Versionen) statt `iter_latest()`.
2. Implementiere in `SstableBuilder::add` Logik, die mehrere Versionen desselben Keys erlaubt (was das SSTable-Format bereits unterstützt).
3. Stelle sicher, dass `SstableReader::get` die Versionierung korrekt berücksichtigt (bereits teilweise implementiert).
```

**Akzeptanzkriterien:**
- [ ] Test `test_mvcc_integrity_during_flush` beweist: Ein Snapshot sieht die alte Version eines Keys auch nachdem der MemTable, der diese Version enthielt, auf Disk geflusht wurde.

---

### HIGH (Pre-Launch — Innerhalb von 1 Woche)

#### FIND-STO-001: Compaction CPU Starvation
**Typ:** Performance / Stabilität
**Datei:** `crates/memfuse-store/src/compaction.rs`
**Zeilen:** 334–353
**Problem:** Die `run_loop` führt potenziell schwere Merges aus, ohne das Tokio-Runtime-Yielding zu berücksichtigen.
**Auswirkung:** Andere Tasks (z.B. API-Endpoints) könnten hohe Latenzen erfahren während der Compaction.

**Refaktorisierungsanweisung:**
```
1. Füge `tokio::task::yield_now().await` in `merge_sstables` innerhalb der `while let Some` Loop ein (nach N Iterationen).
```

---

## REFAKTORISIERUNGSREIHENFOLGE

```
Schritt 1: FIND-STO-004 (Essential für MVCC Sicherheit)
Schritt 2: FIND-STO-003 (Korrektes Rollback-Verhalten)
Schritt 3: FIND-STO-001 (Runtime-Stabilität)
```

## NEUE TESTS

```rust
// TEST-1: test_lsm_rollback_partial_sstable
// 1. Tx100: Put(A, v1). Flush -> SST1.
// 2. Tx200: Put(B, v2). Flush -> SST2.
// 3. Tx300: Put(A, v3). (No flush, in MemTable).
// 4. Rollback to Tx250.
// 5. Get(A) MUST return v1 (from SST1).

// TEST-2: test_mvcc_integrity_during_flush
// 1. Snapshot S1 anfordern.
// 2. Key K=v2 schreiben.
// 3. Flush auslösen.
// 4. S1 muss immer noch K=v1 sehen (falls vorhanden) oder None, 
//    selbst wenn K=v2 jetzt in SSTable liegt.
```

## DONE-DEFINITION FÜR DIESES CRATE

- [ ] Rollback-Garantie für SSTables verifiziert.
- [ ] MVCC-Erhaltung beim Flush belegt.
- [ ] `just triple-test -p memfuse-store` 3× grün.
- [ ] Keine Panics bei korrupten SSTables (Error handling check).
