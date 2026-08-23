# MemFuse Architektur — Kurzreferenz

## Architektur-Update: Desktop-Applikation (MemFuse Brain)

Über dem bestehenden 3-Schichten-Modell (Triebwerk/Getriebe/Fassade) liegt
nun eine vierte Schicht:

**Layer 4 — Anwendung**:
- `memfuse-tauri`: Desktop-Shell, IPC-Commands, Ingestion-Pipeline, Ollama-Bridge
- `memfuse-mcp`: Standalone MCP-Server (axum/SSE) für externe LLM-Clients

Diese Schicht kennt `memfuse-db` nur über dessen öffentliche API — keine
Layer-Verletzung nach unten.

## Kern-Philosophie
MemFuse ist die **eingebettete 4-Signal-Memory-Engine für lokale AI-Agenten** —
air-gapped, zero-panic (angestrebt), 100% Pure-Rust Sovereign Core ohne externe C-Laufzeitumgebungen.

## Produktstrategie (ADR-007)
- **Hauptausrichtung C**: Lokale eingebettete Agent-Memory-Library (`pip install memfuse` / `cargo add memfuse-db`)
- **Langfristig A**: Sovereign Edge-DB (baut auf denselben Sovereign-Core-Eigenschaften auf)
- **Feature B**: DACH-Morphologie als Differenzierungsmerkmal, nicht eigenständige Produktlinie

## Schichtmodell (DAG)

```
Layer 0:  memfuse-core        — Typen, Traits, Fehler (keine Abhängigkeiten)
Layer 1:  memfuse-store       — LSM-Tree, WAL, SSTables
          memfuse-index       — HNSW, SIMD-Distanz, SQ8
          memfuse-text        — BM25, Inverted Index
          memfuse-crypto      — AES-GCM, HMAC-Chaining
          memfuse-graph       — CSR-Graph, Entity-Relation Traversal
          memfuse-checkpoint  — Async Checkpointing & State Recovery
Layer 2:  memfuse-db          — Collections, 4-Signal Fusion, RRF, 2PC
Layer 3:  memfuse-py          — PyO3-Fassade (Python-Bindings, MCP Server)
          memfuse-embed       — ONNX (C-Deps, opt-in feature) [🧊 Frozen]
```

**Aktiver Workspace-Build**: `memfuse-core`, `memfuse-store`, `memfuse-index`, `memfuse-db`,
`memfuse-text`, `memfuse-checkpoint`, `memfuse-crypto`, `memfuse-graph`, `memfuse-py` (9 Crates Sovereign Core)

## Invarianten-Status (Stand: 2026-07-19)

| Invariante | Status | Befund |
|---|---|---|
| **Souveränität** (Zero-C-Deps) | ✅ Erfüllt | Cargo-Build ohne C-Crates im Default-Profil. |
| **Zero-Panic** | ⚠️ Offen | 16+ Dateien mit `.unwrap()` in Produktionscode (inkl. `memfuse-db`, `memfuse-core`). Ziel, noch nicht erreicht. |
| **Determinismus** (SIMD) | ✅ Erfüllt | Cross-Check SIMD vs. Skalar (Epsilon 1e-4) via Proptest. |
| **WAL-Crash-Consistency** | ✅ Erfüllt | Fault-Injection im WAL (Partial Writes), HMAC-Chaining. |
| **Atomarität** | ⚠️ Lücken | 2PC implementiert; Split-Brain-Risiko bei Crash während Commit (FIND-DB-005). |
| **Tombstone-Safety** | ⚠️ Bug offen | Phantom-Daten nach Teil-Compaction (FIND-STO-001). Behoben: ausstehend. |
| **DAG Integrity** | ✅ Erfüllt | Unidirektionale Abhängigkeiten. `just dag-check` grün. |

## Sicherheit
- **HKDF Key Derivation**: Eigener kryptographischer Kontext pro Datei.
- **HMAC Chaining**: WAL-Integrität gegen Manipulation geschützt.
- **Namespace Isolation**: Vollständige Trennung von Collections auf Storage-Ebene.

## Aktive Security Advisories
- `RUSTSEC-2026-0186`: Behandelt durch Upgrade von `memmap2` auf `0.9.11`.
- `RUSTSEC-2026-0002`: Behandelt durch Upgrade von `lru` auf `0.12.5`.

---
*Status: 2026-07-19 — Richtung C (Agent-Memory-Library) beschlossen (ADR-007). Phase 0 (Scope-Schnitt & Security) aktiv.*
