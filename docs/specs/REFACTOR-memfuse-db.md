# REFACTOR-PLAN: memfuse-db
**Datei:** `docs/specs/REFACTOR-memfuse-db.md`
**Erstellt:** 2026-05-28
**Priorität:** CRITICAL
**Geschätzter Aufwand:** 3 Tage
**Voraussetzung:** memfuse-core, memfuse-store, memfuse-index, memfuse-text

---

## CRATE-ZUSTAND: EHRLICHE BEWERTUNG

| Dimension          | Aktuell       | Ziel          |
|--------------------|---------------|---------------|
| Recovery-Speed     | ❌ O(N log N)  | O(Delta)      |
| Transaktions-Sicherheit| ⚠️ Komplex | ATOMIC        |
| Observability      | ❌ Lückenhaft  | 100% Tracing  |
| Snapshot-Feature   | ⚠️ Nur Intern  | PUBLIC API    |

---

## IDENTIFIZIERTE SCHWACHSTELLEN

### BLOCKING (Release-Blocker — SOFORT)

#### FIND-DB-003: Recovery Performance Disaster
**Typ:** Performance / Skalierbarkeit
**Datei:** `crates/memfuse-db/src/collection.rs`
**Zeilen:** 143–204 (`repair`)
**Problem:** Die `repair()` Methode scannt den kompletten LSM-Store und führt für JEDES gefundene Dokument eine Vektor-Suche im HNSW aus, um zu prüfen, ob der Index synchron ist.
**Auswirkung:** Startup-Zeiten von mehreren Stunden bei großen Datenbanken.
**Sovereign Core Verstoß:** Effizienz-Axiom (Start-up Latenz).

**Refaktorisierungsanweisung:**
```
1. Nutze die `last_tx_id` aus dem HNSW-Header (siehe REFACTOR-memfuse-index.md).
2. Ändere `repair()` so, dass nur Transactions > `index.last_tx_id()` aus dem Store gelesen werden.
3. Die Reparatur wird zu einer O(Delta) Operation (Delta = Anzahl der TXs seit dem letzten erfolgreichen save()).
```

**Akzeptanzkriterien:**
- [ ] Test `test_repair_performance_large_db` beweist: Bei einer DB mit 10.000 Docs und nur 10 fehlenden Index-Einträgen dauert die Reparatur < 100ms.

---

### HIGH (Pre-Launch — Innerhalb von 1 Woche)

#### FIND-DB-001: Snapshot-Management API
**Typ:** API-Design / Feature
**Datei:** `crates/memfuse-db/src/lib.rs`
**Problem:** `MemFuse` bietet keine Möglichkeit, benannte Snapshots (Checkpoints) zu erstellen oder den Zustand der gesamten DB konsistent auf einen Snapshot zurückzusetzen.

**Refaktorisierungsanweisung:**
```rust
1. Implementiere `MemFuse::create_checkpoint(name: &str) -> Result<u64>`.
   - Schreibt `(name -> current_seq_no)` in einen speziellen Metadata-Namespace.
   - Ruft `flush()` auf.
2. Implementiere `MemFuse::rollback_to_checkpoint(name: &str) -> Result<()>`.
   - Liest `seq_no` für den Namen.
   - Ruft `storage.rollback_to_tx(seq_no)` und `index.rollback_to_tx(seq_no)`.
```

---

#### FIND-DB-002: Observability (Tracing & Metrics)
**Typ:** Wartbarkeit
**Ziel:** Alle öffentlichen API-Methoden müssen instrumentiert sein.

**Refaktorisierungsanweisung:**
```
1. Annotiere `insert`, `search`, `hybrid_search`, `commit`, `rollback` mit `#[tracing::instrument]`.
2. Füge `tracing::info!` Logs für signifikante Ereignisse (Checkpoint-Erstellung, Recovery-Start) hinzu.
```

---

## REFAKTORISIERUNGSREIHENFOLGE

```
Schritt 1: Delta-basierte Reparatur (DB-003)
Schritt 2: Checkpoint/Snapshot API (DB-001)
Schritt 3: Tracing-Vollständigkeit (DB-002)
```

## NEUE TESTS

```rust
// TEST-1: test_checkpoint_roundtrip
// 1. Inserte Daten.
// 2. Erstelle Checkpoint "C1".
// 3. Inserte weitere Daten.
// 4. Rollback zu "C1".
// 5. Verifiziere: Neue Daten sind weg, alte Daten sind da.

// TEST-2: test_tracing_presence
// 1. Aktiviere Tracing-Subscriber im Test.
// 2. Führe hybrid_search aus.
// 3. Verifiziere, dass entsprechende Spans im Log auftauchen.
```

## DONE-DEFINITION FÜR DIESES CRATE

- [ ] Startup-Recovery ist O(Delta).
- [ ] Checkpoint-API ist public und funktional.
- [ ] Alle Hot-Paths sind instrumentiert.
- [ ] `just triple-test -p memfuse-db` 3× grün.
