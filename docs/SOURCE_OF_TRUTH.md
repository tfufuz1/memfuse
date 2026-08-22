# MemFuse — Source of Truth (SOT)

> **Dieses Dokument ist das einzige Living State Document für Architektur-Status, Crate-Inventar, offene Findings und die aktive Roadmap. Es wird synchron mit dem Code aktualisiert — niemals im Voraus.**

---

## 1. Produktstrategie & Mission

**MemFuse** is the **embedded 3-in-1 Memory Engine for Local AI Agents** — combining Vector Search (semantic), BM25 Full-Text Search (lexical), and Entity-Relation Graph Traversal (associative) in a single in-process library.

### 🎯 Kern-USP (Der 3-in-1 Vorteil)
* **Keine Ops-Last**: In-process Library, zero Server, zero Docker, zero Kubernetes.
* **4-Signal-Fusion (RRF)**: Vektor + Volltext + Graph + Metadaten-Filter vereint in einer einzigen Abfrage für optimalen LLM-Prompt-Kontext.
* **Sovereign Core**: 100% Pure-Rust Core ohne C-Abhängigkeiten (ONNX-Embeddings optional per Feature-Gate).
* **ACID-Garantie**: Transaktionssicherheit durch MVCC-Snapshot-Isolation und HMAC-chained WAL.

---

## 2. Architektur-Topologie (DAG)

```
Layer 0:  memfuse-core        — Typen, Primitiven, Fehler (keine Abhängigkeiten)
Layer 1:  memfuse-store       — LSM-Tree, WAL, SSTables, Crypt-at-Rest
          memfuse-index       — HNSW, SIMD-Distanzen, SQ8-Quantisierung
          memfuse-text        — BM25, Inverted Index, Tokenizer
          memfuse-crypto      — AES-GCM-SIV, HMAC-Chaining
          memfuse-graph       — CSR-Graph, Entity-Relation Traversal [🟡 Reaktivierung & LSM-Persistenz aktiv]
Layer 2:  memfuse-db          — Collections, RRF-Fusion, transaktionales 2PC
Layer 3:  memfuse-py          — PyO3 Python FFI-Bindings [🟡 Reaktivierung & Tests aktiv]
```

### 🧊 Ausgelagerte & Entfernte Crates
Um den Fokus zu wahren, wurden folgende Crates physisch aus dem Repository gelöscht (Phase 0 abgeschlossen):
* `memfuse-cluster` (Raft-Verteilung) -> Ausgelagert/Gelöscht.
* `memfuse-sandbox` (WASM Sandboxing) -> Ausgelagert/Gelöscht.
* `memfuse-saos-agent` (Agent Runner) -> Ausgelagert/Gelöscht.
* `memfuse-embed` (ONNX Embedder) -> Verbleibt im Repo, ist aber standardmäßig deaktiviert (opt-in feature).

---

## 3. Crate-Inventar & Status (Sovereign Core — 9 Active Crates)

| Crate | Layer | LOC | Status | Beschreibung / Hauptaufgabe |
| :--- | :---: | :---: | :--- | :--- |
| `memfuse-core` | 0 | ~1.150 | 🟡 Panics | Typen und Fehler. Eliminieren aller unwrap() Aufrufe. |
| `memfuse-store` | 1 | ~4.130 | 🟢 Upgraded | LSM-Tree-Storage. `memmap2` (0.9.11) & `lru` (0.12.5) aktualisiert. |
| `memfuse-index` | 1 | ~3.520 | 🟡 Panics | HNSW-Vektorindex. Zero-Panic-Audit. |
| `memfuse-text` | 1 | ~960 | 🟢 Clean | BM25 Inverted Index für Lexical Search. Commit-Stats gefixt. |
| `memfuse-crypto`| 1 | ~310 | 🟡 Panics | Krypto-Primitiven. Zero-Panic-Audit. |
| `memfuse-graph` | 1 | ~520 | 🟡 Active | CSR Graph. **Persistenz im LSM-Tree implementieren (FIND-GRA-001)**. |
| `memfuse-checkpoint`| 1 | ~600 | 🟢 Clean | Async Checkpointing & State Snapshot Management. |
| `memfuse-db` | 2 | ~2.500 | 🟡 Panics | Collections-Orchestrator. RwLock-unwraps entfernen (parking_lot). |
| `memfuse-py` | 3 | ~1.000 | 🟡 Active | PyO3-Fassade für Python-Anbindung. Pytest & MCP-Server aktiv. |

---

## 4. Aktiver Backlog (Priorisiert nach Roadmap v2.0)

### 🚀 P0: Scope-Bereinigung & Sicherheitsgarantien (Sofort)
| ID | Task | Severity | Status | Rationale / Befund |
| :--- | :--- | :---: | :---: | :--- |
| **P0-1** | Unnötige Crates aus Cargo-Workspace entfernen | Major | 🟢 Erledigt | `memfuse-cluster`, `memfuse-sandbox`, `memfuse-saos-agent` physisch gelöscht. |
| **P0-2** | Upgrade `memmap2` zur Behebung der Sicherheitswarnung | Blockierend | 🟢 Erledigt | `memmap2` auf 0.9.11 aktualisiert. |
| **P0-3** | Upgrade / Härtung von `lru` Cache | Blockierend | 🟢 Erledigt | `lru` auf 0.12.5 aktualisiert. |
| **P0-4** | Zusammenführen redundanter Audit- und Spezifikationsdokumente | Minor | 🟡 Aktiv | MECE-Konformität in Docs hergestellt. |

### 🔒 P1: Datenintegrität & Zero-Panic (Woche 2–4)
| ID | Task | Severity | Status | Rationale / Befund |
| :--- | :--- | :---: | :---: | :--- |
| **P1-1** | RwLock-Panics in `memfuse-db` eliminieren | Kritisch | ⬜ Geplant | Umstellung auf `parking_lot::RwLock` zur Entfernung aller RwLock-unwrap(). |
| **P1-2** | Zero-Panic-Härtung in Layer 0 & Layer 1 | Kritisch | ⬜ Geplant | Beseitigung aller `.unwrap()` und `.expect()` im Produktionscode. |
| **P1-3** | LSM Tombstone-GC reparieren | Kritisch | ⬜ Geplant | **FIND-STO-001**: Phantom-Daten nach Teil-Compaction verhindern. |
| **P1-4** | drop_collection Speicherleck beheben | Kritisch | ⬜ Geplant | **FIND-DB-002**: Implementieren von `delete_prefix` im LSM-Tree. |
| **P1-5** | Snapshot-Isolation im Suchpfad erzwingen | Kritisch | ⬜ Geplant | **FIND-DB-003**: `SnapshotGuard` bei Hybrid- und Vektorsuchen nutzen. |
| **P1-6** | Graph-Persistenz im LSM-Tree implementieren | Kritisch | ⬜ Geplant | **FIND-GRA-001**: CSR-Graph-Daten nach Neustart aus WAL/SSTable wiederherstellen. |

### 🐍 P2: Python-Bindings & Release-Bereitschaft (Woche 5–7)
| ID | Task | Severity | Status | Rationale / Befund |
| :--- | :--- | :---: | :---: | :--- |
| **P2-1** | Reaktivierung von `memfuse-py` im Workspace | Major | ⬜ Geplant | Bereitstellung der Python-Bindings. |
| **P2-2** | Behebung von FFI-Layer-Leakage | Major | ⬜ Geplant | **FIND-PY-001**: FlatBuffer-Generierung nach `memfuse-core::ipc` verschieben. |
| **P2-3** | GIL-Freigabe bei zeitintensiven Operationen | Major | ⬜ Geplant | **FIND-PY-002**: Nutzen von `py.allow_threads` im Suchpfad. |
| **P2-4** | Aufbau der pytest-Suite (mindestens 20 Tests) | Major | ⬜ Geplant | Validierung der Python-Bindings unter Stress. |
| **P2-5** | crates.io & PyPI Alpha-Releases (v0.1.0-alpha) | Major | ⬜ Geplant | Vertriebskanäle aktivieren. |

### ⚡ P3: Unschlagbare Performance & Ökosystem (Woche 8+)
| ID | Task | Severity | Status | Rationale / Befund |
| :--- | :--- | :---: | :---: | :--- |
| **P3-1** | MCP (Model Context Protocol) Server integrieren | Major | ⬜ Geplant | Direkte Kopplung von MemFuse an LLM-Clients wie Claude Desktop. |
| **P3-2** | Benchmarks gegen ChromaDB und LanceDB erstellen | Minor | ⬜ Geplant | Nachweis der Performance und RRF-Präzision. |
| **P3-3** | Community-Launch & Dokumentations-Rollout | Minor | ⬜ Geplant | Launch auf HackerNews, Reddit, GitHub. |

---

## 🛡️ Aktive Sicherheitswarnungen (CVEs)

1. **RUSTSEC-2026-0186** (`memmap2`): Behebung durch Upgrade auf `0.9.11` in `Cargo.toml`.
2. **RUSTSEC-2026-0002** (`lru`): Behebung durch Upgrade auf `0.12.5` in `crates/memfuse-store/Cargo.toml`.

---

## 🚦 Qualitäts-Gates & Definition of Done

* **Automatisierter Gate-Stack**:
  1. `just check`: Formatierung (rustfmt) und Compiler-Warnungen (Clippy) als Fehler behandeln.
  2. `just test`: Gesamte Testsuite ausführen.
  3. `just triple-test`: Führt cargo test 3x hintereinander aus (Flaky-Test-Detektor).
  4. `just debt-audit`: Scannt den Code nach unwrap(), expect() und std::fs-Zugriffen.
  
* **Freigabekriterien für Commits**:
  * Alle Gates müssen vollständig grün durchlaufen.
  * Keine neuen `.unwrap()` oder `.expect()` im Produktionscode.
  * Geänderte Bereiche müssen durch Tests abgesichert sein (Anti-Mirroring-Prinzip gemäß `TESTING.md` einhalten).
