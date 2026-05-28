# FINAL-REFACTOR-MASTER: MemFuse Global Hardening
**Datei:** `docs/specs/FINAL-REFACTOR-MASTER.md`
**Status:** DRAFT / FOR REVIEW
**Datum:** 2026-05-28
**Lead:** Lead Rust Architect

---

## 🎯 ÜBERSICHT: DIE SOVEREIGN CORE MISSION
Diese Master-Spec konsolidiert die Ergebnisse des forensischen Audits aller 11 Crates. Ziel ist die Transformation von **MemFuse** von einem Prototyp in eine produktionsreife, ausfallsichere "Sovereign Core" Datenbank für agentische RAG-Systeme.

---

## 🛑 KRITISCHE BEFUNDE (TIER 0 — INSTABILITÄTS-RISIKO)

1.  **SSTable-Rollback Gap (FIND-STO-003):** LSM-Kompaktierung ist nicht rückrollbar. Stale Daten können nach einem Crash/Rollback "wiederauferstehen".
2.  **Vector Persistence Race (FIND-IDX-002):** HNSW-Graphen werden nicht atomar gespeichert. Abstürze führen zu korrupten Index-Dateien.
3.  **Posting List Scalability (FIND-TXT-003):** Inverted-Index nutzt O(N) Read-Modify-Write für Posting-Listen. Performance bricht bei >10k Docs ein.
4.  **Recovery Performance (FIND-DB-003):** Startup-Check führt O(N log N) Voll-Scans durch.

---

## 🛠️ REFAKTORISIERUNGS-HIERARCHIE (ORDER OF OPERATIONS)

Die Umsetzung MUSS in dieser Reihenfolge erfolgen, um die DAG-Integrität zu wahren:

### PHASE 1: THE FOUNDATION (Persistence & Safety)
1.  **memfuse-core:** Trait-Bereinigung (Error Types, Distance Traits).
2.  **memfuse-store:** WAL Integrity (HMAC), SSTable Rewrite für Rollback, MVCC Flush Fix.
3.  **memfuse-index:** Shadow-File Atomicity für HNSW, `last_tx_id` Header-Erweiterung.

### PHASE 2: SCALABILITY & ORCHESTRATION
4.  **memfuse-text:** Refactor von `Vec<DocId>` zu `pl:{term}:{doc_id}` (Sharded Posting Lists).
5.  **memfuse-db:** Implementierung von O(Delta) Startup-Recovery; Checkpoint API.

### PHASE 3: INTERFACE & WORKFLOWS
6.  **memfuse-py:** Granulares Exception Mapping, MCP Server Implementierung.
7.  **memfuse-checkpoint:** Rollback-Garantie (Acid) für Metadaten.
8.  **memfuse-saos-agent:** Step-basiertes Checkpoint-Naming (Loop-Fix).
9.  **memfuse-sandbox:** Real Host Functions (CRUD) und Air-Gap Verifier.

---

## 🛡️ SOVEREIGN CORE INVARIANTS (ENFORCEMENT)

Jede PR, die diesen Refactor umsetzt, muss folgende Kriterien erfüllen:
- **Zero-Panic:** Keine expliziten `unwrap()` in der `src/` Logik.
- **Atomic-First:** Daten am Host sind erst gültig, wenn die WAL erfolgreich gesynct wurde.
- **Safe-Rust:** Nur `core` und `index` (SIMD) dürfen `unsafe` nutzen (mit Dokumentation).

---

## 📊 ERFOLGS-METRIKEN (TEST-GATE)

| Test-Typ           | Tool                | Ziel-Kriterium                    |
|--------------------|---------------------|-----------------------------------|
| Stability          | `just triple-test`  | 500 Iterationen ohne Fail/Panic   |
| Scaling insert     | `criterion`         | < 1ms pro Insert bei 100k Docs    |
| Recovery speed     | `tracing`           | Startup < 500ms bei Large DB      |
| Python UX          | `pytest`            | Granulare Exceptions (KeyError)   |

---

## ABNAHME
Diese Spezifikation dient als Grundlage für alle Jules-Agenten. Keine Änderung am Kern ohne Abgleich mit diesem Master Plan.
