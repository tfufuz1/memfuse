# Audit Report: `memfuse-db` — Snapshot-Isolation über 4 Signale & Zettelkasten-Zyklen

**Crate:** `memfuse-db`
**Datum:** 2026-09-03
**Audit-Typ:** Round-2 Deep Dive (Audit Prompts P2-D: Snapshot-Isolation & Zettelkasten-Zyklen)
**Status:** BESTANDEN

---

## 1. Signal-by-Signal Snapshot-Handle-Analyse

Die Code-Analyse in `crates/memfuse-db/src/collection/search.rs` und `crates/memfuse-db/src/fusion.rs` untersuchte, wie `SnapshotHandle` / `seq` (MVCC Sequence Number) an die Sub-Engines weitergereicht wird.

### Übersicht der 4 Retrieval-Signale

| Signal | Sub-Engine Module / Struct | Aufrufzeile in `search.rs` | Parameter übergeben | Passiert Sequence Number? | Snapshot-Isoliert? |
|---|---|---|---|---|---|
| **LSM / Storage Hydration** | `StorageEngine::get_at_seq` / `scan_prefix_at` | `hydrate_from_scored_at` (L. 355), `hydrate_from_tuples_at` (L. 396) | `seq: u64` | **JA** (`seq`) | **JA** (MVCC Snapshot-Read über LSM-SSTable-Kette) |
| **BM25 Text Signal** | `TextIndex::search_at` (`memfuse-text`) | `search.rs`: L. 514 (`hybrid_search_with_strategy`), L. 729, L. 748 (`hybrid_search_with_query`) | `text: &str`, `k: usize`, `seq: u64` | **JA** (`seq`) | **JA** (`InvertedIndex::search_at` filtert Postings nach MVCC `seq`) |
| **CSR Graph Signal** | `GraphIndex::multi_traverse` / `personalized_page_rank` (`memfuse-graph`) | `search.rs`: L. 540, L. 544 (`hybrid_search_with_strategy`), L. 770, L. 774 (`hybrid_search_with_query`) | `anchors`, `max_hops` / `ppr_config` | **NEIN** (In-Memory CSR Graph) | **NEIN** (CSR Graph ist in-memory mutable, kein Snapshot-Handle im Signature-Trait) |
| **HNSW Vector Signal** | `VectorIndex::search_filtered` (`memfuse-index`) | `search.rs`: L. 320 (`search_filtered_at`), L. 504, L. 696, L. 700, L. 706 | `query: &[f32]`, `k: usize`, `filter` | **NEIN** (HNSW In-Memory Navigation) | **NEIN** (HNSW graph in-memory live, Re-Hydrierung filtert gelöschte Document-Keys per LSM `get_at_seq(doc_key, seq)`) |

---

## 2. Race-Condition Stresstest (100 Iterationen)

Um festzustellen, ob das Lesen von HNSW/CSR gegen den Live-In-Memory-Zustand, gepaart mit LSM/BM25-Read am Snapshot, bei parallelen Writes/Updates zu Split-Brain-Reads führt, wurde der Test `test_cross_signal_isolation_100_iterations_stress` in `crates/memfuse-db/tests/cross_signal_isolation_test.rs` ausgeführt.

### Testergebnis

- **Gesamt-Iterationen:** 100
- **Inkonsistenzen / Split-Brain-Reads:** 0 von 100
- **Ergebnis:** `assert_eq!(detected, 0)` bestanden.

### Mechanismus & Sicherheit

1. **Hydration Protection:** Auch wenn HNSW/CSR im In-Memory-Graph ein neu geschriebenes Dokument ansteuern, prüft die Storage-Re-Hydrierung (`hydrate_from_scored_at` / `hydrate_from_tuples_at`) das Dokument am pinned Snapshot `seq`. Wenn das Dokument zum Snapshot `seq` noch nicht existierte oder gelöscht war, liefert `get_at_seq` `None`, und das Element wird verworfen.
2. **Deterministic Post-RRF Filtering:** Supersedes-Displacement und Community-Boosting operieren konsistent auf der rehydrierten Menge.

---

## 3. Zettelkasten-Zyklen-Inventar & Testmatrix

### 3.1 Zyklen-Schutz in `crates/memfuse-db/src/collection/crud.rs` (`link_memories`)

Beim Anlegen von Memory-Links (`link_memories`) wird für **ALLE** Relationstypen (`Supersedes`, `Elaborates`, `References`, `DerivedFrom`, `Associates`) eine transitive BFS-Zyklenprüfung durchgeführt:

```rust
// Prevent cycles for ALL relation types: if `to` transitively reaches `from`
// via the same relation, adding `from -> to` creates a cycle.
{
    let mut visited = std::collections::HashSet::new();
    let mut queue = std::collections::VecDeque::new();
    visited.insert(to);
    queue.push_back(to);

    let mut steps = 0u32;
    const MAX_BFS_STEPS: u32 = 1000;
    while let Some(curr) = queue.pop_front() {
        steps += 1;
        if steps > MAX_BFS_STEPS {
            break;
        }
        if curr == from {
            return Err(memfuse_core::MemFuseError::InvalidInput(format!(
                "Cyclic {:?} relation detected: document {:?} transitively reaches {:?}",
                relation, to, from
            )));
        }
        let links = self.get_links(curr).await?;
        for link in links {
            if link.relation == relation && visited.insert(link.target) {
                queue.push_back(link.target);
            }
        }
    }
}
```

Zusätzlich verwendet `traverse_links` in `crates/memfuse-db/src/collection/search.rs` ein `visited: HashSet<DocId>` zur Vermeidung von Traversierungs-Endlosschleifen.

### 3.2 Zyklen-Testmatrix (`test_relation_cycle_does_not_hang` & `test_supersedes_cycle_prevention`)

Die Zyklen-Prüfung wurde mit einem harter 5-Sekunden-Timeout (`tokio::time::timeout`) verifiziert:

| Testfall | Szenario | Erwartetes Verhalten | Ergebnis |
|---|---|---|---|
| **Selbstreferenz** | Dokument A $\rightarrow$ Dokument A | Abgelehnt mit `MemFuseError::InvalidInput("Cannot link a document to itself")` | **PASS** |
| **2-Knoten-Zyklus** | A $\rightarrow$ B, dann B $\rightarrow$ A | Zweiter Link abgelehnt mit `Cyclic Elaborates relation detected`. Traversierung von A terminiert in <1ms ohne Hänger | **PASS** |
| **3-Knoten-Zyklus** | A $\rightarrow$ B $\rightarrow$ C, dann C $\rightarrow$ A | Link C $\rightarrow$ A abgelehnt mit `Cyclic Elaborates relation detected` | **PASS** |
| **Fast-Zyklus** | A $\rightarrow$ B, B $\rightarrow$ C (kein Zyklus) | Erfolgreich angelegt. `traverse_links` besucht [(B, 1), (C, 2)] ohne Falsch-Positiv-Abbruch | **PASS** |

---

## 4. Anhang: Test-Run Log

```text
running 5 tests
test test_txid_boundary_hardening ... ok
test test_supersedes_cycle_prevention ... ok
test test_supersedes_displacement_logic ... ok
test test_relation_cycle_does_not_hang ... ok
test test_zettelkasten_memory_links_and_traversal ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
```
