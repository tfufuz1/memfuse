# MemFuse SAOS — Goldstandard-Roadmap

> **Updated:** 2026-05-21
> **Basis:** Audit Report + SAOS Architecture

## Sprint 0 — Foundations (Blocking Everything)

Ziel: Codebase ist stabil. Kein Agent kann sinnvoll arbeiten bevor dies fertig ist.

| WP | Assignee | Blocker | Status |
|----|---------|---------|--------|
| WP-0.0 Tech Debt | Jules Account 01 | — | ✅ Stabil |

## Sprint 1 — Cockpit MVP (Sofort greifbar für Entwickler)

Ziel: Python-Entwickler können MemFuse installieren, Collections anlegen,
Hybrid-Search nutzen. Dies ist die "Wow-Moment"-Milestone.

| WP | Assignee | Dependency | Status |
|----|---------|-----------|--------|
| WP-1.1 Compaction | Jules Account 02 | WP-0.0 | ✅ Stabil |
| WP-1.2 Collections | Jules Account 04 | WP-1.1 | ✅ Stabil |
| WP-2.1 Hybrid Search | Jules Account 05 | WP-1.2 | ✅ Stabil |
| WP-3.1 Python Bindings | Jules Account 06 | WP-2.1 | ✅ Stabil |

## Sprint 2 — Sovereign Security & Efficiency

| WP | Assignee | Dependency | Status |
|----|---------|-----------|--------|
| WP-2.2 SQ8 Quantization | Jules Account 03 | WP-2.1 | ✅ Stabil |
| WP-3.2 Encryption | Jules Account 10 | WP-1.1 | ✅ Stabil |

## Sprint 3 — SAOS Core (Migrations-Hebel)

| WP | Assignee | Dependency | Status |
|----|---------|-----------|--------|
| WP-5.1 Checkpointing | Jules Account 07 | WP-1.2 + MVCC | ✅ Stabil |
| WP-5.2 WASM Sandbox | Jules Account 08 | WP-3.1 | ⬜ Designed |
| WP-5.3 Agent Orchestration | Jules Account 09 | WP-5.1 + WP-5.2 | ⬜ Designed |

## Sprint 4 — Hyper-Scale

| WP | Assignee | Dependency | Status |
|----|---------|-----------|--------|
| WP-4.1 mmap | Jules Account 02 | WP-1.1 + WP-3.2 | ⬜ Offen |
| WP-4.2/5.4 Adaptive Filter | Jules Account 04 | WP-1.2 | ⬜ Designed |
| WP-4.3 DiskANN | Jules Account 03 | WP-2.2 + WP-4.1 | 🔄 Partial |

---

## Der entscheidende Migrations-Hebel (Priorisierungsbegründung)

**WP-5.1 (Checkpointing)** ist der stärkste Hebel für Migration von LangGraph:
- → Löst Nicht-Determinismus (das größte Pain-Point)
- → Ermöglicht echte lokale Debug-Erfahrung (IDE-Feeling für KI-Agenten)
- → Senkt Inferenz-Kosten (kein vollständiger Re-Run nach Fehler)
- → Ist strukturell bereits vorbereitet durch WAL in WP-1.1

**WP-3.1 (Python Bindings)** ist der schnellste Weg zum ersten Nutzer:
- → 90% der KI-Entwickler sind Python-Entwickler
- → Erst die greifbare API validiert den "SQLite-für-AI-Agenten"-Anspruch

## Kritischer Pfad (Sequenzielle Abhängigkeiten)

```
WP-1.2 → WP-2.1 → WP-3.1 → WP-5.1 → WP-5.3
  │                              │
  └──→ WP-5.4                   └──→ WP-5.2
```

Alles konvergiert auf WP-5.3 (Agent Orchestration) als das finale Integrations-WP.
