# SDD Specification: `memfuse-graph`

**Status:** DRAFT  
**Crate-Layer:** 1 (Engine)  
**Souveränität:** CSR-basiert, In-Memory, Transaktional.

---

## 1. Systemgrenzen & Verantwortlichkeit (MECE)

`memfuse-graph` (Signal 3) ermöglicht die Verknüpfung von Entitäten über gewichtete Kanten für komplexe Relation-Retrievals.

### Verantwortlichkeiten:
- **Graphen-Struktur:** Implementierung einer Compressed Sparse Row (CSR) für extreme Cache-Effizienz.
- **Relation-Traversal:** BFS-basierte Suche mit konfigurierbarem Score-Decay ($0.7^{hop}$).
- **Transaktionalität:** Staging von Kanten & Knoten per `TxId` mit explizitem `commit`/`rollback`.
- **Topologie-Optimierung:** Kompaktierung der `staged_edges` in die CSR-Arrays (`offsets`, `targets`, `weights`) bei Lese-Bedarf.

### Nicht-Verantwortlichkeiten:
- **Persistente Speicherung:** Der CSR-Graph ist aktuell flüchtig und wird über den WAL (Layer 2) rekonstruiert.
- **Deep-Graph Analytics:** Kein PageRank oder Community-Detection (Fokus liegt auf Hops <= 3).

---

## 2. Kritische Invarianten & SDD-Garantien

| ID | Invariante | Beschreibung |
|---|---|---|
| **GRAPH-INV-001** | **Isolation-Barrier** | Uncommittete Kanten sind für `traverse()` unsichtbar (Double-Checked Locking in `compact()`). |
| **GRAPH-INV-002** | **Score-Decay** | Jede Hop-Ebene reduziert den Signal-Beitrag um einen festen Faktor (Default: 0.7). |
| **GRAPH-INV-003** | **Memory-Layout** | Zusammenhängende Arrays für `targets` und `weights` zur Vermeidung von Pointer-Chasing. |

---

## 3. Schnittstellen-Spezifikation (High-Precision)

### 3.1 GraphIndex Trait (`csr.rs`)
Implementiert die `memfuse-core` Schnittstelle:
- **`traverse(start, max_hops)`**: Führt BFS aus, limitiert auf `MAX_TRAVERSAL_HOPS` (3).
- **`add_edge(tx, edge)`**: Staged eine gerichtete Beziehung.

### 3.2 Compaction-Trigger
Die Kompaktierung erfolgt "lazy" beim ersten `traverse()` nach einem `commit()`. Dies schützt den Write-Pfad vor dem Overhead der CSR-Reorganisierung.

---

## 4. Codebase-Checklist (src/)

| Modul | Status | Bezug auf Spec |
|---|---|---|
| `lib.rs` | ✅ | Rollen-Definition im Sovereign Core. |
| `csr.rs` | ✅ | Komplette Logik inkl. Sharded-States und BFS. |

---

## 5. Verifikation (Triple-Gate)

- **I - Kompilierbarkeit:** `cargo check -p memfuse-graph`
- **II - Stil:** `cargo clippy -p memfuse-graph`
- **III - Verhalten:** 
  - `test_graph_transaction_isolation`: Verifiziert, dass uncommittete Edges unsichtbar bleiben.
  - `test_csr_graph_bfs_score_decay`: Validierung der numerischen Korrektheit der Hop-Gewichtung.
