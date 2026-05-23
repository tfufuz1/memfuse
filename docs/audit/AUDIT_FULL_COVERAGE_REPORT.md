# AUDIT FULL COVERAGE REPORT: MemFuse SAOS (Status: KRITISCH)

---

## 1. Executive Summary: Der "Ghost-Feature" Befund

Die Codebase leidet unter „Circular Reasoning“: Viele Features gelten als „DONE“, weil ein oberflächlicher Integrationstest grün ist, während die zugrundeliegende Modul-Logik entweder ungetestet ist oder bei Randfällen (Edge-Cases) deterministisch versagt.

### Die 3 größten Bedrohungen (S1/S2):
1.  **S1: Potential Tombstone Resurrection Bug in Compaction.** `CompactionEngine::merge_sstables` GC's tombstones basierend auf `min_snapshot_seq` ohne zu verifizieren, ob ALLE älteren Versionen des Keys im zu kompaktierenden Set enthalten sind. Dies führt zu Daten-Resurrektion nach Teil-Kompaktierungen.
2.  **S1: Sovereign Core Doctrine Verletzung.** Über 200 Instanzen von `.unwrap()` und `.expect()` sowie explizite `panic!` in produktiven Pfaden (DiskANN, Compaction-Tests, SAOS-Typen). Das System ist **nicht** Zero-Panic-sicher nach dem Sovereign Core Standard.
3.  **S2: Test-Vakuum im Domänen-Kern.** Die fundamentalen Bausteine `memfuse-core::types::domain` (DocId, Embedding, Entity) und `memfuse-db::filter` (Metadata Filtering) besitzen **0% Testabdeckung**. Stabilität wird hier nur implizit durch andere Tests angenommen.

---

## 2. Abdeckung & Status-Audit (Coverage-Matrix)

| Crate | Pub Fns | Tests vorhanden | Tests fehlend | Coverage % | Status |
|:------|:--------|:----------------|:--------------|:-----------|:-------|
| `memfuse-core` | ~55 | ~40 | ~15 | ~72% | **SKELETT** (Domain ungetestet) |
| `memfuse-store` | ~45 | ~35 | ~10 | ~77% | **FRAGMENTIERT** (GC-Bug) |
| `memfuse-index` | ~36 | ~32 | ~4 | ~88% | **STABIL** (SIMD gut) |
| `memfuse-db` | ~70+ | ~50 | ~20 | ~71% | **FRAGMENTIERT** (Filter ungetestet) |
| `memfuse-text` | ~15 | ~15 | 0 | 100% | **STABIL** |
| `memfuse-graph` | ~3 | ~1 | ~2 | ~33% | **SKELETT** |
| `memfuse-crypto` | ~8 | ~8 | 0 | 100% | **STABIL** |
| `memfuse-checkpoint` | ~9 | ~9 | 0 | 100% | **STABIL** |
| `memfuse-runtime` | ~8 | ~2 | ~6 | ~25% | **SKELETT** |
| `memfuse-orchestrator` | ~6 | ~0 | ~6 | ~0% | **SKELETT** |
| `memfuse-py` | ~25 | ~0 | ~25 | ~0% | **BLOCKIERT** (Linker Error) |
| **TOTAL** | **~280** | **~192** | **~88** | **~68%** | **KRITISCH** |

---

## 3. Kritische Schwachstellen (Forensic Findings)

### 3.1 [S1] Tombstone Resurrection (LSM Storage)
In `crates/memfuse-store/src/compaction.rs:260`:
```rust
if is_tombstone && raw_seq < min_snapshot_seq {
    continue; // Tombstone is safe to garbage-collect
}
```
**Fehler:** Ein Tombstone darf NUR gelöscht werden, wenn er (a) alle älteren Versionen im Gesamtsystem abdeckt (Major Compaction) oder (b) das System garantiert, dass keine älteren Versionen in SSTables außerhalb des Merges existieren. Die aktuelle Logik löscht Tombstones auch bei Teil-Kompaktierungen, was gelöschte Daten aus älteren SSTables wieder sichtbar macht.

### 3.2 [S2] Silent Panic Vectors
- `crates/memfuse-index/src/diskann.rs:453`: `panic!` bei Input-Validierung.
- `crates/memfuse-core/src/types/saos.rs:225`: `panic!` in `FusionWeights::new` (Test-Zweig?).
- Hunderte `.unwrap()` in `memfuse-db/tests` und `memfuse-store/tests`.

### 3.3 [S1] Restart Persistence Gap (Fixed)
*Update 2026-05-23:* Die fehlerhafte Suche nach `wal.log` wurde heute Morgen durch eine dynamische Discovery-Logik für `wal-{ts}.log` ersetzt. Die Gefahr eines Datenverlusts nach Flush/Neustart ist vorerst gebannt, muss aber durch einen Integrationstest verifiziert werden.

---

## 4. Architektonische Drift

- **Ghost Features:** `memfuse-orchestrator` und `memfuse-runtime` enthalten fast nur Boilerplate ohne funktionale Tests.
- **DAG Integrität:** `memfuse-core` fängt an, komplexe Strukturen zu halten, die eigentlich in `memfuse-db` gehören sollten (z.B. `ContextWindow`).
- **Python Bindings:** Der aktuelle Linker-Error in `memfuse-py` verhindert jegliche QA für die Python-API.

---

## 5. Fehlende Tests — Priorisierte Liste

| Priorität | Crate | Funktion | Grund |
|:----------|:------|:---------|:------|
| **P0** | `memfuse-store` | Major Compaction GC | Datenverlust/Resurrektion |
| **P0** | `memfuse-core` | `snapshot.rs` Invarianten | MVCC Grundpfeiler (Multi-Thread) |
| **P1** | `memfuse-core` | `domain.rs` (alle) | API-Vertrag der Basis-Typen |
| **P1** | `memfuse-db` | `filter.rs` Matches | Filtering-Korrektheit |
| **P2** | `memfuse-index` | `trigger_rebuild_async` | Concurrency/Stability |

---

## 6. Action Plan (Jules-Routing)

### Sofortige Reparatur (Healing Phase)
1.  **⬡ @JULES-02 | P0 | FIXME:** Überarbeite Tombstone-GC in `compaction.rs`. GC nur erlauben, wenn `inputs` alle SSTables umfasst (Major Compaction) ODER `is_last_level` Flag vorhanden.
2.  **⬡ @JULES-01 | P1 | TEST:** Implementiere `crates/memfuse-core/src/types/domain_test.rs` für alle Typen in `domain.rs`.
3.  **⬡ @JULES-04 | P1 | TEST:** Implementiere `crates/memfuse-db/src/filter_test.rs` für komplexe Metadata-Queries.
4.  **⬡ @JULES-13 | P0 | DEBT:** Globaler Scan & Refactor: Alle `.unwrap()` im `/src` Verzeichnis durch `?` ersetzen.

### Stabilisierung
5.  **⬡ @JULES-07 | P1 | INTEGRATION:** Schreibe einen `Restart-Stability-Test`, der `flush()`, `kill`, `open()` und `verify()` zyklisch durchführt.

---
*Bericht erstellt am 23. Mai 2026 durch MemFuse Conductor.*
