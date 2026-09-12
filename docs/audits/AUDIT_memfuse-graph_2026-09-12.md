# Systematischer Audit Report — `memfuse-graph`

**Datum:** 2026-09-12
**Session:** Systematischer Crate-Audit (memfuse-graph)
**Auditor:** Jules (Senior Rust Graph-Algorithmen & System-Architekt)
**Ziel-Crate:** `memfuse-graph` (Layer 2 — Graph-Retrieval, CSR-Graph, PathRAG, Session-DAG, PPR, Community Detection)
**Crate-Scope:** 12 Quellcodedateien (`cascade.rs`, `community.rs`, `consistency_enforcement.rs`, `csr.rs`, `edge_reinforcement.rs`, `edge_reinforcement_buffer.rs`, `lib.rs`, `path_rag.rs`, `percolation.rs`, `ppr.rs`, `provenance.rs`, `session_dag.rs`), 18 Testdateien / Benches, ~10.627 Zeilen.

---

## 1. Executive Summary & Audit Verdict

Ein umfassender, systematischer Audit der Crate `memfuse-graph` wurde durchgeführt. Fokus-Themen waren:
1. **`NodesGuard`-Typsicherheits-Stresstest:** Überprüfung der Compile-Time-Deadlock-Prävention in `crates/memfuse-graph/src/session_dag.rs`.
2. **PathRAG & Cross-Crate-Wiring:** Analyse der Dijkstra-Implementierung, des Sufficiency-Gates und der Einbindung als 3. RRF-Signal in `memfuse-db::fusion.rs` / `search.rs`.
3. **EdgeReinforcementBuffer & Default-Visibilität (P12):** Verdrahtung des `edge-reinforcement-learning`-Feature-Flags und Integration in den `MaintenanceScheduler`.
4. **Community-Detection & PPR Konvergenz:** Prüfung der Stabilitätsschwellenwerte, L1-Norm-Abbruchbedingungen und Dangling-Node Massenerhaltung.

### Audit Verdict: **GO (PASS WITH FINDINGS)**
Die Crate erfüllt höchste Qualitätsstandards:
* `#![forbid(unsafe_code)]` ist strikt in `lib.rs` verankert (0 unsafe Blöcke).
* Null unbehandelte `.unwrap()` / `.expect()` Aufrufe im Produktionscode.
* 146 Unit-, Property- und Integrationstests sowie Benchmarks bestehen fehlerfrei.

---

## 2. 6-Punkte-Prüfkatalog & System-Status

| Prüfpunkt | Status | Befund / Bemerkungen |
| :--- | :---: | :--- |
| **1. Unsafe Code & Memory Safety** | **BESTANDEN** | Strict `#![forbid(unsafe_code)]` in `lib.rs`. Zero `unsafe` Blöcke im gesamten Crate. |
| **2. Zero Panic Policy** | **BESTANDEN** | Keine `.unwrap()` / `.expect()` in Produktionslogik. Fehler werden sauber via `MemFuseError` / `Result` propagiert. |
| **3. Lock-Hierarchie & Deadlock-Prävention** | **BESTANDEN** | `NodesGuard` / `NodesWriteGuard` erzwingen top-down Lock-Reihenfolge (`nodes` -> `edges` / `active_head`) auf Typebene. |
| **4. Numerische Stabilität & NaN/Inf Protection** | **BESTANDEN** | Fließkomma-Scores in PPR, PathRAG und Edge-Reinforcement nutzen `f32::total_cmp` / `to_bits()`, `is_finite()` Checks und Saturated Clamping. |
| **5. Feature-Gates & P12 Default Visibility** | **BESTANDEN** | `edge-reinforcement-learning` und `graph-connectivity-health` sind standardmäßig inaktiv (`default = []`), 0 Laufzeit-Overhead im Default-Build. |
| **6. Cross-Crate Wiring & Signal-Symmetrie** | **BEFUND (MINOR)** | PathRAG wird als gleichwertiges 3. Signal ("graph") in RRF gefust, läuft aber gegen globalen unversionierten Graph-Status (Snapshot-Skew Warning). |

---

## 3. Vertiefte Befunde & Stresstests

### 3.1 `NodesGuard` Typsicherheits-Stresstest (`session_dag.rs`)

**Claim unter der Lupe:** *"Erzwingt die Lock-Reihenfolge nodes -> edges/active_head zur Compile-Zeit."*

**Analyse & Gegenbeweisversuch:**
In `session_dag.rs` sind die Felder `nodes`, `edges` und `active_head` des Structs `SessionBranchTree` modulintern (`pub(crate)` bzw. private).

1. **Direkter Zugriff von außen:**
   Ein externer Aufruf wie `tree.edges.read()` kompiliert nicht, da `edges` private ist (`compile_fail` Test existiert im Rustdoc).
2. **Versuch, `edges` Lock ohne `NodesGuard` innerhalb des Moduls zu erwerben:**
   Innerhalb des Moduls `session_dag` könnte theoretisch direkt `self.edges.read()` aufgerufen werden. Wenn man jedoch die öffentlichen APIs von `SessionBranchTree` betrachtet (`append_step`, `branch_from`, `set_active_head`, `path_to_head`, `children_of`, `save`), verlaufen **alle** Sperroperationen ausschließlich über `self.lock_nodes()` oder `self.lock_nodes_write()`.
3. **Versuch einer Inversion der Lock-Reihenfolge:**
   Kann man ein `edges.write()`-Guard halten und danach versuchen, `nodes` zu sperren?
   Die Methoden `edges()`, `edges_write()`, `active_head()` und `active_head_write()` sind ausschließlich Methoden **auf** `NodesGuard<'a>` bzw. `NodesWriteGuard<'a>`. Sie borgen `&'a self` aus dem Guard aus. Das bedeutet, dass der `nodes`-Read/Write-Lock **bereits gehalten werden MUSS**, bevor die `edges`/`active_head` Methoden überhaupt aufgerufen werden können. Es existiert keine Methodensignatur, die ein temporäres Entkoppeln erlaubt.

**Code-Beispiel des versuchten Gegenbeweises (lokal evaluiert):**
```rust
// Versuchte Lock-Inversion (Sperren von edges vor nodes):
let tree = SessionBranchTree::new("root".into(), "resp".into());
// 1. tree.edges.read(); // -> E0616: field `edges` of `SessionBranchTree` is private
// 2. NodesGuard bietet keine Möglichkeit, erst edges und dann nodes zu sperren,
//    da NodesGuard::edges(&self) zwingend &self vom NodesGuard verlangt!
```
**Ergebnis:** **Hypothese widerlegt — Die Typsicherheit ist KEINE Illusion.** Die Lock-Reihenfolge `nodes` -> `edges`/`active_head` ist durch Kapselung und Ausleihe-Lebensdauern compile-zeitlich abgesichert.

---

### 3.2 PathRAG Sufficiency-Gate & Cross-Crate-Wiring Audit

**1. Sufficiency-Gate Präzisions-Analyse (`path_rag.rs`):**
* Das Sufficiency-Gate `sufficiency_check(path)` prüft, ob `path.confidence >= self.sufficiency_threshold` (Default `DEFAULT_SUFFICIENCY_THRESHOLD = 0.1`).
* `confidence` berechnet sich aus dem Produkt aller Kantengewichte entlang des Pfades: $C = \prod w_i$.
* Bei einem dicht vernetzten Graphen mit vielen Multi-Hop-Pfaden verhindert die Schwelle $0.1$, dass lange Pfade mit vielen schwachen Kanten (z.B. $0.3 \times 0.3 \times 0.3 \times 0.3 = 0.0081 < 0.1$) das RRF-Signal fluten und eine Precision-Kollision auslösen (arXiv:2506.00610).

**2. Cross-Crate Wiring in `memfuse-db`:**
* Wird PathRAG als gleichwertiges Signal eingebunden?
  In `crates/memfuse-db/src/collection/search.rs` wird `GraphTraversalStrategy::PathRag` ausgewertet. Die Ergebnisse von `engine.to_rrf_signal(&paths)` werden als `graph_results` gesammelt und in `signal_sets` mit dem Graph-Gewicht `gw` abgelegt:
  ```rust
  if !graph_results.is_empty() {
      signal_sets.push(("graph".to_string(), graph_results, gw));
  }
  ```
  `weighted_reciprocal_rank_fusion_with_options` verarbeitet dieses Signal vollkommen identisch und symmetrisch zu "vector" (`vw`) und "text" (`tw`).
* **Befund (Snapshot Skew):** Sowohl PPR als auch PathRAG loggen bei MVCC-Snapshot-Anfragen eine Warnung (`hybrid_search_graph_snapshot_skew = true`), da sie gegen den globalen unversionierten CSR-Graphen ausführen, während Hops-Traversal `multi_traverse_at(..., seq)` unterstützt.

---

### 3.3 EdgeReinforcementBuffer & P12 Default Visibility Audit

**Prüfung:** Ist `edge_reinforcement_buffer.rs` produktiv verdrahtet oder existiert es nur hinter einem ungenutzten Feature-Flag?

1. **Crate-Ebene (`memfuse-graph`):**
   In `Cargo.toml` ist `edge-reinforcement-learning = []` definiert (`default = []`). Das Modul `edge_reinforcement_buffer` ist in `lib.rs` und `csr.rs` mit `#[cfg(feature = "edge-reinforcement-learning")]` gekapselt.
2. **Cross-Crate-Ebene (`memfuse-db`):**
   `memfuse-db/Cargo.toml` deklariert:
   `edge-reinforcement-learning = ["memfuse-graph/edge-reinforcement-learning"]`
3. **Integration in `MaintenanceScheduler` (`crates/memfuse-db/src/maintenance_scheduler.rs`):**
   Das Feld `edge_reinforcement_buffer: Option<Arc<memfuse_graph::EdgeReinforcementBuffer>>` existiert **ausschließlich** unter `#[cfg(feature = "edge-reinforcement-learning")]`. Im Konstruktor `new()` wird es auf `None` initialisiert und kann via `with_edge_reinforcement_buffer` übergeben werden. `run_tick()` führt Schritt c (Flush des Buffers in den CSR-Graphen) nur aus, wenn das Feature-Flag aktiv und der Buffer gesetzt ist.

**Ergebnis:** **Prinzip P12 (Default-Unsichtbarkeit) perfekt umgesetzt.** Ohne explizites Aktivieren des Features entsteht weder Speicher- noch CPU-Overhead. Wenn das Feature aktiviert wird, ist es nahtlos in den `MaintenanceScheduler` eingebunden.

---

### 3.4 Community Detection & PPR Konvergenzgarantien

**1. Personalized PageRank (`ppr.rs`):**
* **Konvergenzkriterium:** Abbruch, wenn L1-Norm $\sum |p_{t} - p_{t-1}| < \epsilon$ (`epsilon` default $10^{-6}$).
* **Max Iteration Ceiling:** Hartes Limit auf `max_iterations = 1000` Vergleiche. Wenn PPR nicht konvergiert, wird eine `tracing::warn!` emittiert und das bisher beste Resultat ohne Panic zurückgegeben.
* **Dangling Nodes & Isolated Mass:** Dangling Mass (Knoten ohne ausgehende Kanten) wird exakt berechnet und gleichmäßig auf den Teleport-Vektor verteilt. Die Gesamtrankmasse $\sum p(i) = 1.0$ bleibt strikt erhalten.

**2. Community Detection (`community.rs`):**
* Verwendet synchrone Label Propagation.
* Deterministische Tie-Breaking-Regeln über Entity-IDs verhindern Oszillationen.
* Auto-Trigger-Integration in `memfuse-db`: `Collection` trackt `mutations_since_community_detection` (`Arc<AtomicU64>`) und löst ab Erreichen von `auto_trigger_threshold` einen Hintergrundtask `run_community_detection()` aus.

---

## 4. Verification Execution Log

```bash
# All features test execution
$ cargo test -p memfuse-graph --all-features -- --include-ignored
test result: ok. 146 passed; 0 failed; 0 ignored; finished in 5.80s

# Benchmark / Integration targets
test result: ok. 2 passed (csr_benchmark)
test result: ok. 2 passed (csr_complexity_bench)
test result: ok. 3 passed (dangling_nodes_audit_test)
test result: ok. 1 passed (hub_node_benchmark)
test result: ok. 3 passed (integration_graph)
test result: ok. 3 passed (persistence_test)
test result: ok. 1 passed (ppr_alloc_test)
test result: ok. 1 passed (doc-tests NodesGuard compile_fail)

# Clippy Check
$ cargo clippy -p memfuse-graph --all-features -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.15s
```

---

## 5. Reflect & Empfohlene Folge-Tasks

1. **PathRAG MVCC Snapshot Support (Medium Priority):**
   * *Ist-Zustand:* PathRAG führt Dijkstra-Suchen gegen den aktuellen unversionierten CSR-Graphen aus und emittiert eine Skew-Warnung bei vergangenen `seq`-Transaktionsständen.
   * *Empfehlung:* Optionales `find_path_at(source, target, seq)` in `PathRAGEngine` via `is_edge_visible_bitemporal` auf dem Graphen nachrüsten.
2. **Community Detection Resolution Tuning (Low Priority):**
   * *Ist-Zustand:* Label Propagation terminiert zuverlässig, erzeugt aber bei extrem riesigen Graphen (>1M Knoten) recht grobe Communitys.
   * *Empfehlung:* Modular modularity-based refinement (Louvain/Leiden) für verfeinerte Hierarchien evaluieren.
