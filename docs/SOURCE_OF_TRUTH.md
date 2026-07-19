# MemFuse — Source of Truth (SOT)

> Dieses Dokument ist das einzige **Living State Document** für Architektur-Status, Crate-Inventar, offene Findings und die aktive Roadmap. Es wird **synchron mit dem Code** aktualisiert — niemals im Voraus. Keine anderen persistenten Specs existieren.

---

## 1. Produktstrategie & Mission

**MemFuse** ist die **eingebettete 4-Signal-Memory-Engine für lokale AI-Agenten**.

> *„Ein Agent braucht keinen Server, kein SQL, kein Kubernetes. Er braucht eine In-Process-Bibliothek mit 4-Signal Fusion, die direkt im Agent-Prozess lebt."*

**Ausrichtung (ADR-007, 2026-07-19):**
- **Primär (C)**: Embedded Agent-Memory-Library — `pip install memfuse` / `cargo add memfuse-db`
- **Langfristig (A)**: Sovereign Edge-DB — baut auf denselben Sovereign-Core-Eigenschaften auf
- **Feature**: DACH-Morphologie als Differenzierungsmerkmal, nicht eigene Produktlinie

**USP gegenüber ChromaDB / LanceDB / Qdrant:**
- 4-Signal Fusion (Vektor + BM25 + Graph + Metadata) in **einer** eingebetteten Library
- Zero-C-Deps im Default-Profil (Sovereign Core)
- ACID mit WAL-First und Snapshot-Isolation

---

## 2. Architektur

### 2.1 Schichtmodell (DAG)

```
Layer 0:  memfuse-core        — Typen, Traits, Fehler (keine Abhängigkeiten)
Layer 1:  memfuse-store       — LSM-Tree, WAL, SSTables
          memfuse-index       — HNSW, SIMD-Distanz, SQ8-Quantisierung
          memfuse-text        — BM25, Inverted Index, Deutsche Morphologie
          memfuse-crypto      — AES-GCM, HMAC-Chaining
          memfuse-graph       — CSR-Graph, BFS   [🟡 Workspace-Reaktivierung ausstehend]
Layer 2:  memfuse-db          — Collections, 4-Signal Fusion, RRF, 2PC
Layer 3:  memfuse-py          — PyO3-Fassade     [🟡 Workspace-Reaktivierung ausstehend]
          memfuse-embed       — ONNX (C-Deps, opt-in, feature-gated)   [🧊 Frozen]
          memfuse-cluster     — Raft (kritische Bugs: FIND-CLU-001/002) [🧊 Frozen]
```

**Aktiver Workspace-Build** (buildfähig):
`memfuse-core`, `memfuse-store`, `memfuse-index`, `memfuse-db`, `memfuse-text`, `memfuse-checkpoint`, `memfuse-crypto` — **7 Crates, ~13.600 LOC**

**Nicht im Workspace** (auskommentiert in `Cargo.toml`):
`memfuse-graph`, `memfuse-py`, `memfuse-cluster`, `memfuse-embed`, `memfuse-sandbox`, `memfuse-saos-agent`

### 2.2 Kritische Invarianten

| # | Invariante | Status | Enforcement |
|---|---|---|---|
| 1 | **Sovereign Core** (Zero-C-Deps) | ✅ Erfüllt | `#![forbid(unsafe_code)]` in Layer 0-2 (außer SIMD in `distance.rs`). C-Deps feature-gated. |
| 2 | **Zero-Panic** | ⚠️ Angestrebt | Ziel: kein `.unwrap()`/`.expect()` außerhalb `#[cfg(test)]`. Aktuell: 16+ Dateien mit Verstößen. P1-Priorität. |
| 3 | **WAL-First Consistency** | ✅ Erfüllt | HMAC-chained WAL, CRC32-Schutz, Fault-Injection-Tests grün. |
| 4 | **Numerical Determinism** | ✅ Erfüllt | SIMD (AVX-512/AVX2) innerhalb `1e-4` von Skalar-Fallback via Proptest. |
| 5 | **Snapshot Isolation (MVCC)** | ⚠️ Lücken | `get_at_seq` korrekt; **FIND-DB-003** offen: Search-Pfad nutzt keine SnapshotGuards → Dirty Reads möglich. |
| 6 | **DAG Integrity** | ✅ Erfüllt | Unidirektionale Abhängigkeiten. `just dag-check` grün. |

---

## 3. Crate-Inventar (Stand: 2026-07-19)

| Crate | Layer | LOC (ca.) | Tests | Status | Kritischste offene Findings |
|---|---|---|---|---|---|
| `memfuse-core` | 0 | 1.150 | 44 | 🟡 Panics | `unwrap()` in `tx_buffer.rs`, `snapshot.rs`, `types/` — Zero-Panic verletzt |
| `memfuse-store` | 1 | 4.130 | 56 | 🟡 Bug | **FIND-STO-001**: Phantom-Daten nach Teil-Compaction. **RUSTSEC-2026-0186** (`memmap2`), **RUSTSEC-2026-0002** (`lru`) |
| `memfuse-index` | 1 | 3.520 | 30 | 🟡 Panics | `unwrap()` in `hnsw.rs`, `persistence.rs`, `distance.rs`. FIND-IND-002: SQ8 globale Min/Max |
| `memfuse-text` | 1 | 962 | 26 | 🟢 Clean | — |
| `memfuse-crypto` | 1 | 313 | 21 | 🟡 Panics | `unwrap()` in `wal_crypto.rs`, `crypto.rs` |
| `memfuse-graph` | 1 | 521 | 8 | 🔴 Nicht im Workspace | **FIND-GRA-001**: Keine Persistenz — Graph verliert alle Daten nach Neustart |
| `memfuse-db` | 2 | 2.500 | 36 | 🔴 Bugs | **FIND-DB-001**: 12× `unwrap()` (RwLock). **FIND-DB-002**: Storage Leak bei drop_collection. **FIND-DB-003**: Dirty Reads möglich. |
| `memfuse-py` | 3 | ~1.000 | 0 | 🔴 Nicht im Workspace | **FIND-PY-001**: Layer Leakage. 0 Tests. Kein PyPI-Release. |

---

## 4. Aktiver Backlog (Geordnet nach Priorität)

### P0 — Scope-Schnitt & Security (sofort)

| ID | Task | Severity | Befund |
|---|---|---|---|
| P0-1 | `memfuse-cluster`, `memfuse-sandbox`, `memfuse-saos-agent` aus Repo entfernen (→ `memfuse-agentos` Repo) | — | Scope-Fokus |
| P0-2 | `memmap2` auf gepatchte Version upgraden | 🔴 CVE | RUSTSEC-2026-0186 |
| P0-3 | `lru` upgraden oder durch `quick_cache` ersetzen | 🔴 CVE | RUSTSEC-2026-0002 |

### P1 — Zero-Panic durchsetzen (Kunden-Blocker)

| ID | Task | Severity | Befund |
|---|---|---|---|
| P1-1 | `memfuse-db` RwLock-`unwrap()` → `parking_lot::RwLock` | 🔴 Kritisch | FIND-DB-001 |
| P1-2 | `memfuse-core` `unwrap()` → `?` / `map_err` | 🔴 Kritisch | Zero-Panic |
| P1-3 | `memfuse-store` `unwrap()` auditieren und ersetzen | 🔴 Kritisch | Zero-Panic |
| P1-4 | `memfuse-crypto` `unwrap()` → `?` | 🔴 Kritisch | Zero-Panic |
| P1-5 | **FIND-STO-001** Tombstone-Fix: nur bei Full-Compaction oder unterstem Tier löschen | 🔴 Kritisch | Phantom-Daten |
| P1-6 | **FIND-DB-002** Storage Leak: `delete_prefix()` bei drop_collection | 🔴 Kritisch | Ressourcen-Leak |
| P1-7 | **FIND-DB-003** Snapshot Isolation in Search: SnapshotGuard in `search_with_filter` | 🔴 Kritisch | Dirty Reads |
| P1-8 | `memfuse-graph` in Workspace reaktivieren + **FIND-GRA-001** Persistenz implementieren | 🔴 Kritisch | Signal 3 fehlt |

### P2 — Release-Fähigkeit

| ID | Task | Severity | Befund |
|---|---|---|---|
| P2-1 | `memfuse-py` Workspace-Reaktivierung + **FIND-PY-001** Layer Leakage beheben | 🔴 | Vertriebskanal |
| P2-2 | pytest-Testsuite ≥20 Tests via `maturin develop` | — | 0 Tests aktuell |
| P2-3 | **FIND-IND-002** SQ8 Per-Dimension Quantisierung | 🟡 | Recall-Verlust |
| P2-4 | **FIND-IND-003** HNSW-Save Endian-Safety | 🟡 | Portabilität |
| P2-5 | PyPI v0.1.0-alpha Release | — | Invariante 1 |
| P2-6 | crates.io v0.1.0 Release | — | Vertriebskanal |

### P3 — Sichtbarkeit

| ID | Task | |
|---|---|---|
| P3-1 | Öffentliche Benchmark-Suite (MemFuse vs. ChromaDB vs. LanceDB) | |
| P3-2 | HN/Reddit-Launch mit Benchmark-Zahlen | |
| P3-3 | MCP (Model Context Protocol) Integration | |

---

## 5. Security Advisories (Aktiv)

| Advisory | Crate | Version | Severity | Pfad |
|---|---|---|---|---|
| RUSTSEC-2026-0186 | `memmap2` | 0.9.10 | ⚠️ Unsound | `memfuse-store`, `memfuse-index` |
| RUSTSEC-2026-0002 | `lru` | 0.12.5 | ⚠️ Unsound | `memfuse-store` |

---

## 6. Qualitäts-Gates

```bash
just check         # fmt + clippy + compile
just test          # cargo test workspace
just triple-test   # 3× test (Flaky-Detektor)
just dag-check     # DAG-Integrität
just debt-audit    # unwrap() + unsafe + std::fs scan
```

**Definition of Done (Phase 1):** `just triple-test` 3× grün, `rg 'unwrap()' crates --glob '*.rs'` liefert 0 Treffer außerhalb `#[cfg(test)]`, FIND-STO-001 Regression-Test grün, `memfuse-graph` im Workspace und Graph-State überlebt Neustart.

---

## 7. Referenzen

| Dokument | Pfad | Zweck |
|---|---|---|
| Architektur-Diagramm | `docs/ARCHITECTURE.md` | Struktureller DAG, Invarianten-Status |
| ADRs | `DECISIONS.md` | Architekturentscheidungen |
| Agent-Regeln | `AGENTS.md` | LLM-Governance |
| Sicherheitsmodell | `SECURITY.md` | Bedrohungsmodell |
| Testphilosophie | `TESTING.md` | Teststandards |
| Glossar | `GLOSSARY.md` | Domänenbegriffe |
