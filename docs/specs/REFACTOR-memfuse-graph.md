# REFACTOR-PLAN: memfuse-graph
**Datei:** `docs/specs/REFACTOR-memfuse-graph.md`
**Erstellt:** 2026-05-28
**Priorität:** HIGH
**Geschätzter Aufwand:** 2 Tage
**Voraussetzung:** memfuse-core (für Trait-Fixes)

---

## CRATE-ZUSTAND: EHRLICHE BEWERTUNG

| Dimension          | Aktuell       | Ziel          |
|--------------------|---------------|---------------|
| Panic-Freiheit     | 100% sauber   | 100%          |
| Skeleton-Anteil    | 0             | 0             |
| Test-Coverage      | ~80%          | >90%          |
| API-Vollständigkeit| 60% (Isol-Fehl)| 100%          |
| Algo-Korrektheit   | ⚠️ DIRTY READS | VERIFIED      |

---

## IDENTIFIZIERTE SCHWACHSTELLEN

### BLOCKING (Release-Blocker — SOFOT)

#### FIND-GRA-001: Transaction Isolation Leak (Dirty Reads)
**Typ:** Korrektheit / Isolation
**Datei:** `crates/memfuse-graph/src/csr.rs`
**Zeilen:** 174, 187, 265
**Code (Kontext):**
```rust
async fn add_edge(&self, _tx: TxId, edge: Edge) -> Result<()> {
    // ...
    inner.staged_edges.entry(from_idx).or_default().push((to_idx, edge.weight));
    // ...
}
```
**Problem:** `add_entity` und `add_edge` ignorieren den `TxId`-Parameter. Alle Änderungen werden sofort in einem globalen `staged_edges`-Buffer gespeichert. `compact()` (aufgerufen bei `commit` oder `traverse`) mergt diese uncommitted Daten in die CSR-Arrays.
**Auswirkung:** Uncommitted Daten sind für alle parallelen Reads (Traversals) sichtbar. Rollback einer Transaction löscht fälschlicherweise alle staged Edges aller Transactions (line 272).

**Refaktorisierungsanweisung:**
```
1. Ändere `staged_edges` in `GraphInner` zu `HashMap<TxId, HashMap<InternalIndex, Vec<(InternalIndex, f32)>>>`.
2. `add_edge` muss die Daten spezifisch für die `tx` stagen.
3. `add_entity` muss ebenfalls Versionierung unterstützen oder Snapshots nutzen.
4. `commit(tx)` darf nur die Edges der spezifischen `tx` in die CSR-Struktur mergen (Copy-on-Write oder Rebuild-Merge).
5. `rollback(tx)` darf nur die Edges der spezifischen `tx` löschen.
```

**Akzeptanzkriterien:**
- [ ] Neuer Test `test_graph_isolation` beweist: `traverse` sieht keine Edges einer uncommitted Transaction.

---

### HIGH (Pre-Launch — Innerhalb von 1 Woche)

#### FIND-GRA-002: Lock Contention in Traversal Hot-Path
**Typ:** Performance
**Datei:** `crates/memfuse-graph/src/csr.rs`
**Zeilen:** 151, 203
**Problem:** `traverse` ruft `compact()` auf, welches einen `Write`-Lock auf den gesamten `GraphInner` erwirbt, selbst wenn `is_dirty` false ist.
**Auswirkung:** Parallele Read-Traversals blockieren sich gegenseitig. Massive Latenz-Spikes bei hoher Query-Last.

**Refaktorisierungsanweisung:**
```
1. Nutze Double-Checked Locking in `compact()`.
2. Erst `read()`-Lock: Falls `is_dirty == false` -> sofort return.
3. Nur wenn `is_dirty == true`, erwerbe `write()`-Lock und compacte.
4. Optimaler: Compacte asynchron im Hintergrund oder nur bei `commit`.
```

**Akzeptanzkriterien:**
- [ ] Benchmark zeigt signifikant höheren Durchsatz bei parallelen Read-Traversals.

---

#### FIND-GRA-003: Lifetime Mismatch in Trait Implementation
**Typ:** Kompilierbarkeit
**Datei:** `crates/memfuse-graph/src/csr.rs`
**Zeilen:** 173–275
**Problem:** Clippy meldet Lifetime-Mismatches zwischen `GraphIndex`-Trait und der Implementierung in `CsrGraph`.
**Auswirkung:** `cargo check` schlägt fehl (E0195).

**Refaktorisierungsanweisung:**
```
1. Synchronisiere die Signaturen exakt mit `memfuse-core::traits::GraphIndex`.
2. Verwende `#[async_trait]` (sobald FIND-COR-004 behoben ist), um manuelle Lifetime-Probleme zu vermeiden.
```

---

## REFAKTORISIERUNGSREIHENFOLGE

```
Schritt 1: FIND-GRA-003 (Sicherstellen der Kompilierbarkeit)
Schritt 2: FIND-GRA-001 (Kritische Korrektheit)
Schritt 3: FIND-GRA-002 (Performance-Optimierung)
```

## NEUE TESTS

```rust
// TEST-1: test_graph_transaction_isolation
// 1. Tx1 fügt Edge hinzu.
// 2. Traverse (ohne Tx) darf Edge NICHT sehen.
// 3. Tx1 committet.
// 4. Traverse MUSS Edge sehen.

// TEST-2: test_graph_rollback_isolation
// 1. Tx1 und Tx2 fügen Edges hinzu.
// 2. Tx1 rollt back.
// 3. Tx2 committet.
// 4. Nur Edges von Tx2 dürfen existieren.
```

## DONE-DEFINITION FÜR DIESES CRATE

- [ ] Transaktions-Isolation für Entities und Edges belegt.
- [ ] Kompilierfehler (E0195) behoben.
- [ ] `just triple-test -p memfuse-graph` grün.
- [ ] Keine Write-Locks im Read-Pfad (außer bei notwendiger Compaction).
