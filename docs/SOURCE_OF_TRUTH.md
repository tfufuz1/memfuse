# MemFuse — Source of Truth (SOT)

> Dieses Dokument agiert als fester Bestandteil des **Unified Documentation Systems** (siehe `CONSTITUTION.md`) und ist das einzige **Living State Document** für Architektur, Crate-Status, offene Findings, und die Implementierungs-Roadmap. Es gibt keine persistenten Specs oder Archiv-Dokumente – jegliches Wissen wird hier konsolidiert.

---

## 1. Architektur

### 1.1 Schichtmodell (DAG) — Sovereign Core Edition

```mermaid
graph TD
    subgraph "Sovereign Core (Layer 0-2)"
        core["memfuse-core<br/>Foundation"]
        crypto["memfuse-crypto<br/>Security"]
        store["memfuse-store<br/>Persistence"]
        index["memfuse-index<br/>Search"]
        text["memfuse-text<br/>BM25"]
        graph["memfuse-graph<br/>CSR"]
        db["memfuse-db<br/>Orchestration"]
    end

    subgraph "External Integration (Layer 3+)"
        py["memfuse-py<br/>Python Bindings"]
        embed["memfuse-embed<br/>(C-Deps: ONNX)"]
        cluster["memfuse-cluster<br/>(Network)"]
    end

    subgraph "Frozen Components"
        ckpt["memfuse-checkpoint"]
        saos["memfuse-saos-agent"]
        sandbox["memfuse-sandbox"]
    end

    core --> crypto
    core --> store
    core --> index
    core --> text
    core --> graph
    crypto --> store
    graph --> index
    store --> db
    index --> db
    text --> db
    db --> py
    
    %% Optional Integrations
    db -.-> embed
    db -.-> cluster
```

### 1.2 Kritische Invarianten

| # | Invariante | Enforcement |
|---|---|---|
| 1 | **Sovereign Core Doctrine** | `#![forbid(unsafe_code)]` in Layer 0-2. C-Abhängigkeiten (`openssl`, `ort`) sind strikt in optionale Layer-3 Features verbannt. |
| 2 | **Zero-Panic** | Kein `.unwrap()`/`.expect()`. Verifiziert durch `just triple-test` und CI-Audit. |
| 3 | **WAL-First Consistency** | HMAC-chained WAL mit CRC32 Schutz. Robustheit gegen partial writes verifiziert. |
| 4 | **Numerical Determinism** | SIMD (AVX-512/AVX2) Resultate müssen innerhalb von `1e-4` (relativ) identisch zum Skalar-Fallback sein. |
| 5 | **Snapshot Isolation** | Transaktionen garantieren Atomarität über LSM + HNSW, bewiesen durch Concurrent Stress-Testing. |
| 6 | **DAG Integrity** | Unidirektionale Abhängigkeiten. Layer 2 (DB) darf keine Typen aus Layer 3 (Py/Embed) voraussetzen. |

### 1.3 ADRs (Architectural Decision Records)

| ADR | Entscheidung | Status |
|---|---|---|
| ADR-001 | LSM-Tree für Persistenz | ✅ Final |
| ADR-002 | HNSW für Vektor-Indexierung | ✅ Final |
| ADR-003 | RRF (Reciprocal Rank Fusion) für Hybridisierung | ✅ Final |
| ADR-004 | **Sovereign Core (Pure Rust Policy)** | ✅ Final (Refactored 2026-06) |
| ADR-005 | **Feature-Based Scaling** | Auto-Embedding & Cluster sind nun Opt-In Features. |

---

## 2. Crate-Inventar

### 2.1 Übersicht

| Crate | Layer | LOC | Tests | Status | Invarianten-Beweis |
|---|---|---|---|---|---|
| `memfuse-core` | 0 | 1.150 | 44 | 🟢 Clean | `TxBuffer` & Traits (Async Mocks, Default Impls) |
| `memfuse-store` | 1 | 4.130 | 56 | 🟢 Clean | WAL Fault Injection (FIND-STO-001 gelöst: CRC + Start-of-File check) |
| `memfuse-index` | 1 | 3.520 | 30 | 🟢 Clean | SIMD vs Scalar Determinismus via Proptest |
| `memfuse-text` | 1 | 962 | 26 | 🟢 Clean | BM25 Scoring & Morphologische Tokenisierung |
| `memfuse-crypto` | 1 | 313 | 21 | 🟢 Clean | AES-GCM Isolation & WAL HMAC Integrity |
| `memfuse-graph` | 1 | 521 | 8 | 🟢 Clean | CSR BFS & Transactional Edge Isolation |
| `memfuse-db` | 2 | 2.500 | 36 | 🟢 Clean | Snapshot Isolation Stress-Test (Atomic Commits verifiziert) |
| `memfuse-py` | 3 | 536 | 0* | 🟡 Dev | Blocked by C-dependencies in Extensions (memfuse-embed) |
| **Sovereign Total** | | **13.632** | **221** | | |

*Optionale Integrationen:* `memfuse-embed` (ONNX), `memfuse-cluster` (Raft).

---

## 3. Offener Backlog

### 3.1 Aktive Items (Testing-Offensive)

| ID | Crate | Titel | Priorität | Status | Beschreibung |
|---|---|---|---|---|---|
| **TEST-STAB-01** | `index` | SQ8 Precision Bounds | TIER 2 | 🟡 OPEN | Testen der Quantisierungs-Fehlerschranken für f32 -> u8. |
| **TEST-STAB-02** | `db` | OOM Simulation | TIER 1 | 🟡 OPEN | Verifizieren der OOM-Resilienz bei Erreichung von Memory-Quotas. |
| **FEAT-SOV-01** | `cluster` | Rustls Migration | TIER 1 | 🟡 OPEN | Entfernen von `openssl` Abhängigkeit in `memfuse-cluster` via `rustls`. |

---

## 4. Implementierungs-Roadmap (Updated 2026-06)

### Phase A: Souveräne Härtung (Abgeschlossen)
- [x] Bereinigung von C-Abhängigkeiten im Kern.
- [x] SIMD Determinismus-Check.
- [x] WAL Robustheitstest.
- [x] Transaktions-Isolation unter Last.

### Phase B: Skalierung & Integration (Nächster Schritt)
- [ ] Umstellung `memfuse-embed` auf Pure-Rust (Burn/Tract?) oder strikte Feature-Kapselung.
- [ ] MCP (Model Context Protocol) Integration über die souveräne API.
- [ ] Horizontale Skalierung via `memfuse-cluster` (Sovereign Security Mode).

---

## 5. Qualitäts-Gates (TRIPLE-GATE)

Keine Änderung wird akzeptiert, die nicht das Triple-Gate in der Konsole besteht:
1. `cargo check -p <crate> --all-targets --no-default-features`
2. `cargo clippy -p <crate> --all-targets --no-default-features -- -D warnings`
3. `cargo test -p <crate> --no-default-features`

Oder: `just triple-test` (aktualisiert auf Sovereign-Profile).

---

## 6. Referenzen

| Dokument | Pfad | Status |
|---|---|---|
| Architektur | `docs/ARCHITECTURE.md` | ✅ Aktualisiert |
| Sovereign-Core Doctrine | `CONSTITUTION.md` | ✅ Aktiv |
| **Living State** | `docs/SOURCE_OF_TRUTH.md` | ✅ **SOT** |
