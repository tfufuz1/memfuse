# Sprint 3: Architectural Remediation — Cluster, Graph-Persistenz, Fassaden-Compliance

## Ziel
Alle architekturellen Defizite heilen, die **verteilte Konsistenz**, **Persistenz-Vollständigkeit** und **Schichtenreinheit** betreffen. Dieser Sprint berührt die Integrations- und Distributionsschichten (`memfuse-cluster`, `memfuse-graph`, `memfuse-py`, `memfuse-embed`, `memfuse-sandbox`). Er setzt voraus, dass Sprint 1 und 2 abgeschlossen sind.

> [!CAUTION]
> FIND-CLU-001 (Index-Blindheit) macht das verteilte System **funktionsunfähig**. FIND-GRA-001 (volatile Graph) bedeutet **vollständigen Datenverlust** bei Neustart. Beide sind Architektur-Redesigns, keine Bugfixes.

## Betroffene Findings (12)

| ID | Crate | Severity | Kurzname |
|---|---|---|---|
| FIND-CLU-001 | `memfuse-cluster` | 🔴 Kritisch | Index-Blindheit auf Follower-Knoten |
| FIND-CLU-002 | `memfuse-cluster` | 🔴 Kritisch | Ephemerer Raft-Log (kein Persist) |
| FIND-CLU-003 | `memfuse-cluster` | 🔴 Kritisch | Inkonsistente Raft-Snapshots |
| FIND-GRA-001 | `memfuse-graph` | 🔴 Kritisch | Volatile-Only Architecture |
| FIND-GRA-002 | `memfuse-graph` | 🟡 Mittel | O(N+E) Compaction Bottleneck |
| FIND-GRA-003 | `memfuse-graph` | 🟢 Niedrig | Hardcoded Traversal Limits |
| FIND-PY-001  | `memfuse-py`      | 🔴 Kritisch | FlatBuffer-Logik in Fassade (§20-Verstoß) |
| FIND-PY-002  | `memfuse-py`      | 🟡 Mittel | GIL-Bottleneck bei Serialisierung |
| FIND-EMB-001 | `memfuse-embed`   | 🟡 Mittel | Souveränitätsrisiko via `from_hub()` |
| FIND-EMB-002 | `memfuse-embed`   | 🟢 Niedrig | Statische ONNX-Pfad-Annahmen |
| FIND-FRZ-001 | `memfuse-sandbox` | 🔴 Kritisch | Host-Function Daten-Rückkanal fehlt |
| FIND-FRZ-002 | `memfuse-sandbox` | 🟡 Mittel | Code-Duplikation in `airgap.rs` |

> [!IMPORTANT]
> `memfuse-sandbox` ist eine **Frozen Zone** (§6). Arbeiten an FIND-FRZ-001 und FIND-FRZ-002 erfordern die explizite Aufhebung gemäß **Artikel IX §27**. Der Implementierende Agent muss dies vor Beginn von Phase 4 bestätigen lassen.

---

## Prompt / Implementierungsplan

> **Kontext**: Du bist der Architekt des Sovereign Core. Sprint 1 (Panic-Safety) und Sprint 2 (ACID + Data Integrity) sind abgeschlossen. Dein Fokus liegt auf den verbleibenden architekturellen Defizite: verteilte Konsistenz, fehlende Persistenz, Schichtenreinheit. Die AGENTS.md Verfassung ist bindend — insbesondere §5 (Schichtenreinheit), §6 (Frozen Zone), §18 (Persistenzgesetz), §20 (Fassadengesetz).

### Phase 1: Cluster-Architektur Reparatur

#### Schritt 1.1 — Raft-Log Persistierung (FIND-CLU-002)
- **Datei**: [crates/memfuse-cluster/src/storage.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-cluster/src/storage.rs), L106
- **Warum zuerst**: Ohne persistentes Log ist jede weitere Cluster-Arbeit auf Sand gebaut.
- **Aktion**:
  1. Ersetze `SyncRwLock<BTreeMap<u64, Entry>>` durch einen dedizierten LSM-Namespace.
  2. Namespace-Konvention: `__raft_log:{log_index}` → serialisierter Entry.
  3. `append()`, `truncate()`, `get_log_entries()` über `LsmStorage` routen.
  4. Startup-Pfad: `restore_log_from_lsm()` beim Knoten-Start.
- **Annahme**: `memfuse-store::LsmStorage` ist bereits als Abhängigkeit vorhanden (bestätigt durch Audit).
- **Test**:
  ```
  test_raft_log_survives_restart:
    1. Schreibe 100 Log-Entries
    2. Drop + Neustart des Storage
    3. Assert: Alle 100 Entries sind wiederherstellbar
  ```

#### Schritt 1.2 — Index-Blindheit beheben (FIND-CLU-001)
- **Datei**: [crates/memfuse-cluster/src/storage.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-cluster/src/storage.rs), L345–387
- **Problem**: `apply()` schreibt direkt in `LsmStorage`, umgeht `memfuse-db::Collection`.
- **Aktion**:
  1. `apply()` muss über die `Collection` API operieren, nicht direkt über den Storage.
  2. Architektur-Entscheidung: `MemFuseStateMachine` erhält eine `Arc<CollectionManager>` statt `Arc<LsmStorage>`.
  3. Für jede Raft-Operation (`Insert`, `Delete`, `Update`) die entsprechende Collection-Methode aufrufen, die automatisch HNSW + TextIndex + GraphIndex aktualisiert.
- **Cross-Crate-Analyse (§22)**:
  - `memfuse-cluster` darf `memfuse-db` importieren (ist eine höhere Schicht als db? → Prüfung gegen DAG-Check).
  - Falls `memfuse-cluster` auf gleicher Ebene wie `memfuse-db` steht: Trait-basierte Abstraktion nötig.
- **Test**:
  ```
  test_follower_search_returns_results:
    1. Cluster mit 2 Knoten starten
    2. Insert Doc über Leader (via Raft)
    3. Warte auf Replikation
    4. Search auf Follower → Assert: Doc gefunden
  ```

#### Schritt 1.3 — Konsistente Raft-Snapshots (FIND-CLU-003)
- **Datei**: [crates/memfuse-cluster/src/storage.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-cluster/src/storage.rs), L279
- **Aktion**: `build_snapshot()` muss vor dem Scan einen `SnapshotGuard` anfordern (abhängig von Sprint 2 Snapshot-Infrastruktur).
  ```rust
  let guard = self.storage.snapshot();
  let entries = self.storage.scan_all_at(guard.seq_no());
  ```
- **Test**: Concurrent-Write während Snapshot-Build → Assert: Snapshot enthält nur pre-Snapshot Daten.

### Phase 2: Graph-Persistenz

#### Schritt 2.1 — CSR Persistence Layer (FIND-GRA-001)
- **Datei**: [crates/memfuse-graph/src/csr.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-graph/src/csr.rs)
- **Design-Entscheidung**: Binärformat analog zu HNSW-Persistenz (nicht LSM-backed, da CSR ein kompaktes Array-Format ist).
- **Aktion**:
  1. `CsrGraph::save(path: &Path) -> Result<()>` implementieren:
     - Header: Magic `MFGR` + Version + Node-Count + Edge-Count
     - Body: `offsets` Array (LE bytes), `targets` Array (LE bytes), `weights` Array (LE bytes)
  2. `CsrGraph::load(path: &Path) -> Result<Self>` implementieren.
  3. Integration in `memfuse-db::Collection::persist()` — Graph wird neben HNSW gespeichert.
- **Test**:
  ```
  test_graph_persistence_roundtrip:
    1. Graph mit 1000 Knoten und 5000 Kanten bauen
    2. save() → load()
    3. Assert: Identische Traversal-Ergebnisse auf Original und Loaded
  ```

#### Schritt 2.2 — Inkrementelle Graph-Compaction (FIND-GRA-002)
- **Datei**: [crates/memfuse-graph/src/csr.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-graph/src/csr.rs), L83–136
- **Aktion**: Statt Full-Rebuild:
  1. Delta-Merge: Neue committed Edges werden in ein Adjazenzlisten-Zwischenformat geschrieben.
  2. CSR-Rebuild nur wenn `delta_edges > threshold` (z.B. 10% der Gesamtkanten).
  3. Traversal liest aus CSR + Delta-Buffer (merge on read).
- **Test**: Benchmark: 1M Edges, 100 neue Edges → Compaction darf keine Full-Rebuild-Kosten erzeugen.

#### Schritt 2.3 — Konfigurierbare Traversal-Tiefe (FIND-GRA-003)
- **Datei**: [crates/memfuse-graph/src/csr.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-graph/src/csr.rs), L20
- **Aktion**: `MAX_TRAVERSAL_HOPS` als Parameter in `TraversalConfig` struct. Default bleibt 3.
- **Test**: `test_traversal_depth_4_reaches_deeper_nodes`.

### Phase 3: Fassaden-Compliance & Embedding-Souveränität

#### Schritt 3.1 — FlatBuffer-Logik aus `memfuse-py` extrahieren (FIND-PY-001)
- **Datei**: [crates/memfuse-py/src/lib.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-py/src/lib.rs), L388–441, L540–594
- **Ziel-Datei**: `crates/memfuse-db/src/ipc.rs` (neues Modul)
- **Aktion**:
  1. Neues Modul `crates/memfuse-db/src/ipc.rs` erstellen.
  2. `SearchResultFlatBuffer::build(results: &[SearchResult]) -> Vec<u8>` implementieren.
  3. `memfuse-py::search_fb` ruft nur noch `db.search_fb()` auf, das intern `ipc::build()` nutzt.
  4. Fassade leitet fertige `Vec<u8>` an Python durch.
- **Cross-Crate (§22)**: `memfuse-py` importiert `memfuse-db` — DAG-konform (L4 → L3).
- **Test**: Bestehende `search_fb` Tests müssen identische Bytes liefern.

#### Schritt 3.2 — GIL-Release während Serialisierung (FIND-PY-002)
- **Datei**: [crates/memfuse-py/src/lib.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-py/src/lib.rs), L408ff
- **Aktion**: Nach Extraktion der IPC-Logik (3.1) wird die Serialisierung automatisch im `allow_threads` Bereich ausgeführt, da sie kein Python-Objekt mehr berührt. Verifizieren, dass kein Python-Typ-Zugriff innerhalb des serialisierten Pfads stattfindet.
- **Test**: Implizit durch 3.1. Benchmark: Messung der GIL-Hold-Time mit `py-spy` bei großen Ergebnismengen.

#### Schritt 3.3 — `from_hub()` Feature-Gate (FIND-EMB-001)
- **Datei**: [crates/memfuse-embed/src/lib.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-embed/src/lib.rs) + [crates/memfuse-embed/Cargo.toml](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-embed/Cargo.toml)
- **Aktion**:
  1. Neues Feature-Flag `fetch` in [Cargo.toml](file:///home/freddy/Arbeitsplatz/DEV/memfuse/Cargo.toml):
     ```toml
     [features]
     default = []
     fetch = ["hf-hub"]
     ```
  2. `from_hub()` hinter `#[cfg(feature = "fetch")]` gaten.
  3. `hf-hub` von `[dependencies]` nach `[dependencies.hf-hub]` mit `optional = true` verschieben.
- **Test**: `cargo check -p memfuse-embed --no-default-features` darf `from_hub` nicht referenzieren.

#### Schritt 3.4 — Konfigurierbarer Modellpfad (FIND-EMB-002)
- **Datei**: [crates/memfuse-embed/src/lib.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-embed/src/lib.rs), L37–43
- **Aktion**: `model_filename` als Parameter in `EmbedConfig`. Default: `"model.onnx"`.
- **Test**: `test_custom_model_path_resolution`.

### Phase 4: Frozen Zone (NUR mit §27-Freigabe)

> [!WARNING]
> Die folgenden Schritte betreffen die **Frozen Zone** (§6). Vor Beginn muss der Nutzer explizit Artikel IX §27 aufrufen und die strategische Begründung bestätigen.

#### Schritt 4.1 — Host-Function Daten-Rückkanal (FIND-FRZ-001)
- **Datei**: [crates/memfuse-sandbox/src/host_functions.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-sandbox/src/host_functions.rs), L35–73
- **Aktion**:
  1. Shared-Buffer Architektur: `db_search` schreibt Ergebnis in Host-seitigen `Vec<u8>`.
  2. Neue Host-Function `db_read_response(offset: i32, len: i32) -> i32`:
     - Kopiert `len` Bytes ab `offset` aus dem Host-Buffer in den WASM-Linear-Memory.
     - Gibt die tatsächlich kopierten Bytes zurück.
  3. WASM-Modul-Kontrakt: `db_search()` → Länge erhalten → `db_read_response(0, len)` → Daten verarbeiten.
- **Test**: Integration-Test: WASM-Modul führt Search aus, liest Ergebnis, validiert JSON-Struktur.

#### Schritt 4.2 — Air-Gap Test-Konsolidierung (FIND-FRZ-002)
- **Datei**: [crates/memfuse-sandbox/src/airgap.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-sandbox/src/airgap.rs)
- **Aktion**: Alle duplizierten Test-Funktionen in ein einziges `#[cfg(test)] mod tests` Modul konsolidieren.
- **Test**: `cargo test -p memfuse-sandbox` muss identische Coverage liefern.

---

## Verifikationsplan

### Automatisiert (Triple-Gate gemäß Artikel V)

```bash
# Gate I — Kompilierbarkeit
nix develop -c cargo check --all-targets --workspace

# Gate II — Stilgesetz
nix develop -c cargo clippy --all-targets -- -D warnings

# Gate III — Verhalten
nix develop -c cargo test --workspace

# Sprint-spezifische Tests:
nix develop -c cargo test -p memfuse-cluster -- raft_log
nix develop -c cargo test -p memfuse-cluster -- follower
nix develop -c cargo test -p memfuse-cluster -- snapshot
nix develop -c cargo test -p memfuse-graph -- persistence
nix develop -c cargo test -p memfuse-graph -- traversal
nix develop -c cargo test -p memfuse-py -- search_fb
nix develop -c cargo test -p memfuse-embed --no-default-features
nix develop -c cargo test -p memfuse-sandbox -- host_function
```

### Strukturelle Verifikation
```bash
# DAG muss intakt bleiben
just dag-check

# Debt Audit
just debt-audit
```

### Manuelle Verifikation
- **Cluster-Test**: 2-Knoten-Setup mit `cargo test --test cluster_integration` (falls separater Integrationstest existiert). Andernfalls: User deployt 2 Nodes lokal und führt Insert+Search-Roundtrip durch.
- **Graph-Persistenz**: User startet Anwendung, fügt Graph-Kanten hinzu, beendet Prozess, startet neu → Graph-Traversal liefert identische Ergebnisse.
- **Frozen-Zone**: Explizite Bestätigung des Users, dass §27 aufgerufen wurde, bevor Phase 4 implementiert wird.
