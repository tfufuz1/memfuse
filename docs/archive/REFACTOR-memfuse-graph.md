# REFACTOR-PLAN: memfuse-graph
**Datei:** `docs/specs/REFACTOR-memfuse-graph.md`
**Erstellt:** 2026-05-27
**Priorität:** MEDIUM
**Geschätzter Aufwand:** 2 Tage
**Voraussetzung:** memfuse-core

---

## CRATE-ZUSTAND: EHRLICHE BEWERTUNG

| Dimension          | Aktuell       | Ziel          |
|--------------------|---------------|---------------|
| Panic-Freiheit     | 100% sauber   | 100%          |
| Skeleton-Anteil    | 2 Stellen     | 0             |
| Test-Coverage      | ~80%          | >90%          |
| API-Vollständigkeit| 70%           | 100%          |
| Algo-Korrektheit   | VERIFIED      | VERIFIED      |

---

## IDENTIFIZIERTE SCHWACHSTELLEN

### BLOCKING (Release-Blocker — SOFORT)

#### FIND-GRAPH-001: Performance-Degradierung durch naive Kompaktierung
**Typ:** Performance / Design-Fehler
**Datei:** `crates/memfuse-graph/src/csr.rs`
**Zeile(n):** 79, 175
**Code (Kontext):**
```rust
// compact() rebuilds from scratch if is_dirty
fn compact(&mut self) {
    if !self.is_dirty { return; }
    // ... Rebuilds ALL arrays from scratch ...
}

async fn traverse(&self, start: EntityId, max_hops: usize) -> Result<Vec<(EntityId, f32)>> {
    self.compact(); // Ruft jedes Mal compact auf
    // ...
}
```
**Problem:** `traverse` erzwingt jedes Mal eine Kompaktierung des gesamten Graphen, falls `is_dirty` wahr ist (was nach jedem `add_edge` der Fall ist). Bei großen Graphen führt dies zu einer quadratischen Zeitkomplexität bei gemischten Write/Read-Lasten.
**Auswirkung:** Massive Latenzspitzen bei der Suche, wenn der Graph aktiv aktualisiert wird.

**Refaktorisierungsanweisung:**
```
1. Implementiere einen Hybrid-Traversal: Die BFS-Suche muss sowohl die CSR-Arrays als auch die `staged_edges` HashMap durchsuchen.
2. Entferne den automatischen `compact()` Aufruf in `traverse()`.
3. `compact()` sollte nur noch explizit (z.B. via Background Task oder bei `commit()`) aufgerufen werden.
4. Optimiere `compact()`, um inkrementell zu arbeiten oder zumindest effizienter zu mergen, statt alles neu zu allozieren.
```

**Akzeptanzkriterien:**
- [ ] `traverse()` funktioniert korrekt, auch wenn `is_dirty == true` (bewiesen durch Test).
- [ ] Performance-Benchmark zeigt O(V+E) statt O(E^2) bei inkrementellen Updates.

---

### HIGH (Pre-Launch — Innerhalb von 1 Woche)

#### FIND-GRAPH-002: Fehlende Persistenz-Schnittstelle
**Typ:** API-Lücke / Architektur
**Datei:** `crates/memfuse-graph/src/csr.rs`
**Zeile(n):** N/A
**Problem:** Der Graph lebt rein im RAM. Es gibt keine Methoden zum Speichern (`save`) oder Laden (`load`) des CSR-Zustands. Dies macht den Graphen unbrauchbar für persistente Datenbanken.
**Auswirkung:** Datenverlust nach Neustart; Graph muss jedes Mal mühsam aus dem WAL rekonstruiert werden.

**Refaktorisierungsanweisung:**
```
1. Implementiere `serde::Serialize` und `Deserialize` für `GraphInner`.
2. Füge Methoden `to_bytes()` und `from_bytes()` zu `CsrGraph` hinzu.
3. Stelle sicher, dass die CSR-Arrays (offsets, targets, weights) binär-effizient (z.B. via rkyv oder bincode) serialisiert werden.
```

**Akzeptanzkriterien:**
- [ ] Roundtrip-Test `test_graph_persistence_roundtrip` ist grün.

---

### MEDIUM (Post-Launch — Tech-Debt Sprint)

#### FIND-GRAPH-003: Unvollständige RAM-Schätzung
**Typ:** Performance
**Datei:** `crates/memfuse-graph/src/csr.rs`
**Zeile(n):** 245-251
**Code (Kontext):**
```rust
let mem = (inner.reverse_map.len() * std::mem::size_of::<EntityId>())
    + (inner.entities.len() * std::mem::size_of::<Entity>())
    // ... staged_edges wird ignoriert ...
```
**Problem:** Die `stats()` Methode ignoriert den signifikanten Speicherverbrauch der `id_map` (HashMap) und `staged_edges`.
**Auswirkung:** `ResourceTracker` unterschätzt den Speicherverbrauch, was zu OOM-Crashes führen kann.

**Refaktorisierungsanweisung:**
```
1. Berechne den Speicherverbrauch der HashMaps inkl. Bucket-Overhead (ca. `capacity * size_of::<Node>`).
```

**Akzeptanzkriterien:**
- [ ] `memory_usage_bytes` spiegelt die Realität besser wider.

---

## REFAKTORISIERUNGSREIHENFOLGE

```
Schritt 1: FIND-GRAPH-001 (Ermöglichen von Dirty-Traversals)
Schritt 2: FIND-GRAPH-002 (Serialisierung hinzufügen)
Schritt 3: FIND-GRAPH-003 (Stats Korrektur)
```

## NEUE TESTS DIE NACH DEM REFACTORING ERSTELLT WERDEN MÜSSEN

```rust
// TEST-1: test_traverse_uncompacted
// Testet: add_edge() ohne compact(), dann traverse().
// Assert: Findet die neue Kante.

// TEST-2: test_graph_serialization
// Testet: Graph mit 1000 Kanten serialisieren -> deserialisieren -> traverse.
// Assert: Ergebnisse identisch.
```

## SCHNITTSTELLEN-ÄNDERUNGEN (Breaking vs. Non-Breaking)

| Änderung                    | Breaking? | Migration-Pfad für Aufrufer    |
|-----------------------------|-----------|-------------------------------|
| `to_bytes`/`from_bytes`     | Nein      | Neue Funktionalität.          |
| `traverse` ohne `compact`   | Nein      | Verhalten verbessert.         |

## DONE-DEFINITION FÜR DIESES CRATE

Das Refactoring gilt als DONE (Triple-Test-Gate) wenn:
- [ ] Hybrid-Traversal (CSR + Staged) implementiert und verifiziert.
- [ ] Serialisierung funktioniert (Bincode/Serde).
- [ ] `just triple-test` 3× grün.
- [ ] Keine O(E^2) Pfade mehr in der Standard-Nutzung.
