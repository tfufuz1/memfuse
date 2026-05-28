# REFACTOR-PLAN: memfuse-index
**Datei:** `docs/specs/REFACTOR-memfuse-index.md`
**Erstellt:** 2026-05-28
**Priorität:** HIGH
**Geschätzter Aufwand:** 2 Tage
**Voraussetzung:** memfuse-core, memfuse-graph

---

## CRATE-ZUSTAND: EHRLICHE BEWERTUNG

| Dimension          | Aktuell       | Ziel          |
|--------------------|---------------|---------------|
| SIMD-Sicherheit    | ✅ Exzellent  | VERIFIED      |
| Persistence        | ⚠️ Lückenhaft | ATOMIC        |
| Durability         | ❌ FEHLT       | RECOVERABLE   |
| API-Vollständigkeit| 90%           | 100%          |

---

## IDENTIFIZIERTE SCHWACHSTELLEN

### BLOCKING (Release-Blocker — SOFORT)

#### FIND-IDX-002: Vector Durability Gap (Crash-Consistency)
**Typ:** Verlässlichkeit / Datenversetztheit
**Problem:** Der `HnswIndex` puffert Inserts im `tx_buffer` (RAM). Ein `save()` schreibt den gesamten Index auf Disk. Falls die App zwischen `commit(tx)` und `save()` abstürzt, ist der Vector-Index inkonsistent zum Rest der DB (`memfuse-store`). Es gibt keinen Mechanismus, um den Index beim Start aus dem WAL des `memfuse-store` automatisch zu reparieren.
**Auswirkung:** Datenverlust von Vektoren nach Crash.

**Refaktorisierungsanweisung:**
```
1. Implementiere `HnswIndex::repair_from_store(store: &LsmStorage)`.
2. Beim Öffnen einer DB (`memfuse-db` Layer):
   - Lese `last_tx_id` aus dem HNSW-Header.
   - Falls `store.last_tx_id() > index.last_tx_id()`:
     - Scanne den LsmStorage ab `index.last_tx_id() + 1`.
     - Re-Insert alle fehlenden Vektoren in den HNSW.
3. Markiere `HnswIndex::save()` als kritischen Checkpoint.
```

**Akzeptanzkriterien:**
- [ ] Test `test_hnsw_recovery_after_crash` beweist: Nach einem simulierten Absturz (kein save()) wird der Index beim Neustart durch Replay der fehlenden TXs aus dem Store wieder vervollständigt.

---

### HIGH (Pre-Launch — Innerhalb von 1 Woche)

#### FIND-IDX-003: Rebuild Race Condition
**Typ:** Concurrency / Korrektheit
**Datei:** `crates/memfuse-index/src/hnsw.rs`
**Zeilen:** 185, 221
**Problem:** `trigger_rebuild_async` prüft `is_rebuild_required()`, startet aber den Tokio-Task ohne sofortige Atomic-Flag-Sicherung. Mehrere parallele Aufrufe könnten mehrere Rebuild-Tasks starten.
**Auswirkung:** Überflüssige CPU-Last, potenzielle Heap-Explosion (da HNSW-Rebuild viel RAM benötigt).

**Refaktorisierungsanweisung:**
```rust
pub fn trigger_rebuild_async(&self) {
    // 1. Prüfe UND setze Flag atomar
    if self.rebuilding.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
        if self.is_rebuild_required() {
            let inner = std::sync::Arc::clone(&self.inner);
            tokio::spawn(async move {
                let result = inner.rebuild().await;
                inner.rebuilding.store(false, Ordering::SeqCst);
                if let Err(e) = result {
                    tracing::error!("Failed to rebuild HNSW index: {}", e);
                }
            });
        } else {
            self.rebuilding.store(false, Ordering::SeqCst);
        }
    }
}
```

---

#### FIND-IDX-001: Persistence Atomicity
**Typ:** Datenintegrität
**Datei:** `crates/memfuse-index/src/hnsw.rs`
**Zeile:** 332
**Problem:** `header.connections_offset` wird erst sehr spät im `save()` Flow berechnet. Falls der Prozess während des Schreibens von NodeRecords oder Vectors stirbt, zeigt der Header (falls er bereits teilweise auf Disk liegt) auf falsche Offsets.

**Refaktorisierungsanweisung:**
```
1. Verwende ein "Shadow-File" Strategie für save():
   - Schreibe den kompletten neuen Index in `path.tmp`.
   - `fsync()` auf `path.tmp`.
   - `fs::rename("path.tmp", path)`.
2. Dies garantiert, dass der Index entweder komplett neu oder komplett alt auf Disk ist.
```

---

## REFAKTORISIERUNGSREIHENFOLGE

```
Schritt 1: Shadow-File für atomares Save (IDX-001)
Schritt 2: Rebuild-Flag Handling (IDX-003)
Schritt 3: Repair-from-Store Logik (IDX-002) - Benötigt Integration in memfuse-db
```

## NEUE TESTS

```rust
// TEST-1: test_hnsw_atomic_save_failure
// 1. Init HNSW.
// 2. Starte Save().
// 3. Simuliere IO-Error/Kill mitten im Prozess.
// 4. File am "Ziel-Pfad" muss entweder nicht existieren oder den alten Zustand haben.

// TEST-2: test_hnsw_concurrent_rebuild_triggers
// 1. Setze deleted_nodes > threshold.
// 2. Rufe trigger_rebuild_async() 100x parallel auf.
// 3. Verifiziere via Tracing/Counter, dass nur 1 Task ausgeführt wurde.
```

## DONE-DEFINITION FÜR DIESES CRATE

- [ ] SIMD-Zone ist vollständig mit SAFETY-Docs versehen (erledigt).
- [ ] `save()` ist atomar via Rename.
- [ ] Rebuild-Spawning ist race-frei.
- [ ] `just triple-test -p memfuse-index` 3× grün.
