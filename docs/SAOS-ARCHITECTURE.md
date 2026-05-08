# MemFuse SAOS — Finale Architektur

> **Status:** Design Phase  
> **Updated:** 2026-05-08  
> **Basis:** Audit Report `AUDIT-REPORT-2026-05-08.md`

## Crate-DAG

```
                    ┌─────────────────────────────────┐
                    │        memfuse-py (WP-3.1)       │  Python API / pip install
                    └──────────────┬──────────────────┘
                                   │
                    ┌──────────────▼──────────────────┐
                    │   memfuse-saos-agent (WP-5.3)   │  Task/Step Orchestration
                    └──────────┬────────┬─────────────┘
                               │        │
           ┌───────────────────▼──┐  ┌──▼───────────────────┐
           │ memfuse-checkpoint   │  │  memfuse-sandbox      │
           │ (WP-5.1)             │  │  (WP-5.2)             │
           │ Time-Travel / Fork   │  │  WASM Tool Isolation  │
           └──────────┬───────────┘  └──────────┬────────────┘
                      │                          │
                    ┌─▼──────────────────────────▼─┐
                    │      memfuse-db (WP-1.2)      │  Collections + Hybrid Search
                    │      + Adaptive Filter (5.4)  │
                    └──────┬──────────┬─────────────┘
                           │          │
           ┌───────────────▼──┐  ┌────▼───────────────┐
           │ memfuse-store    │  │  memfuse-index      │
           │ (WP-1.1 + 4.1)  │  │  (WP-2.2)           │
           │ LSM + Compaction │  │  HNSW + SQ8         │
           │ + mmap + crypto  │  └────────────────────┘
           └──────────────────┘
                           │
                    ┌──────▼──────────────────────────┐
                    │  memfuse-text (WP-2.1)           │  BM25 + Inverted Index
                    └──────────────────────────────────┘
                           │ (alle importieren)
                    ┌──────▼──────────────────────────┐
                    │  memfuse-core (WP-0.0)           │  Shared Kernel — DARF NICHT
                    │  MemFuseError, MVCC, TxBuffer     │  andere Crates importieren
                    └──────────────────────────────────┘
```

## DAG-Invariante

Kein Pfeil darf nach unten zeigen. `memfuse-core` importiert niemanden.

## Layer-Architektur

| Layer | Name | Crates | Funktion |
|---|---|---|---|
| **L0** | Cockpit | `memfuse-py` | User-facing Python API |
| **L1** | Getriebe | `memfuse-saos-agent`, `memfuse-checkpoint`, `memfuse-sandbox` | Agent orchestration & safety |
| **L2** | Triebwerk | `memfuse-db`, `memfuse-store`, `memfuse-index`, `memfuse-text` | Core DB engine |
| **L3** | Kernel | `memfuse-core` | Shared types & error handling |

## Entwicklungs-Reihenfolge (strikt)

| Phase | Work Packages | Beschreibung |
|---|---|---|
| Phase 0 | WP-0.0 (Tech Debt) | Fundament — ✅ Stabil |
| Phase 1 | WP-1.1, WP-1.2 | Stable Storage — WP-1.1 ✅, WP-1.2 🔄 |
| Phase 2 | WP-2.1, WP-2.2 | Search Engines |
| Phase 3 | WP-3.1, WP-3.2 | User API + Security |
| Phase 4 | WP-5.1 | Checkpointing (nutzt MVCC aus Phase 0) |
| Phase 5 | WP-5.2 | WASM Sandbox |
| Phase 6 | WP-5.3 | Agent Orchestration (nutzt 5.1 + 5.2) |
| Phase 7 | WP-4.1, WP-4.2/5.4, WP-4.3 | Hyper-Scale |

## Crate Rename Plan (Bestehend → SAOS)

| Aktuell | SAOS-Name | Rationale |
|---|---|---|
| `memfuse-runtime` | `memfuse-sandbox` | Klarere Bezeichnung für WASM-Isolation |
| `memfuse-orchestrator` | `memfuse-saos-agent` | Umbenennung zu Agent-fokussiertem Crate |
| `memfuse-store/checkpoint.rs` | `memfuse-checkpoint` (neuer Crate) | Eigenständiger Crate für saubere DAG-Separierung |
