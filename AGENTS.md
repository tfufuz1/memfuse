# MemFuse — Agentic OS Directory & Crate Lookup

> **Phase:** Feature Implementation & TDD  
> **Doctrine:** Zero-Panic / Sovereign Core / Triple-Test-Gate  
> **Updated:** 2026-05-05

---

## Crate-Übersicht & Lokale Agent-Kontexte

| Crate | Rolle | LoC | Status |
|:------|:------|:----|:-------|
| **`memfuse-core`** | Grundlegende Typen, TxBuffer, MemBank, Error, Snapshots | ~280 | ✅ Stabil |
| **`memfuse-store`** | LSM-Storage, MemTables, WAL, Compaction | ~1400 | ✅ Stabil |
| **`memfuse-index`** | HNSW-Graphen, SIMD Vektor-Distanz, Quantization | ~1300 | ✅ Stabil |
| **`memfuse-db`** | Orchestrierung, Hybrid-Search Facade, Collections | ~700 | ✅ Stabil |
| **`memfuse-text`** | BM25, Inverted Index, Tokenizer | — | 🔵 WP-2.1 geplant |
| **`memfuse-py`** | PyO3 Bindings, maturin | — | 🔵 WP-3.1 geplant |

---

## ⚠️ Entwicklungsregeln (ABSOLUT VERBINDLICH)

### Sovereign Core Doctrine

1. **`#![forbid(unsafe_code)]`** in jedem Crate (Ausnahme: [`distance.rs`](./crates/memfuse-index/src/distance.rs))
2. **Zero `.unwrap()`** außerhalb von `#[cfg(test)]` — nur `?` oder explizites Error-Handling
3. **Zero blockierendes I/O** in async-Kontexten — `tokio::fs` statt `std::fs`
4. **Warnings = Errors**: `cargo clippy -- -D warnings` muss immer sauber sein
5. **Jede neue public API** bekommt mindestens einen `#[tokio::test]` Contract-Test
6. **Jede Datei** braucht ein `//!` Crate/Module Doc-Comment im Header
7. **Backward Compatibility**: bestehende API-Signaturen dürfen nicht gebrochen werden

### Triple-Test-Gate (DONE-Definition)

> **Ein Work Package gilt als DONE wenn und nur wenn:**
> 1. Alle zugehörigen Contract-Tests bestehen **3× hintereinander** ohne Änderung
> 2. `cargo clippy -- -D warnings` ist grün (0 Warnings)
> 3. Der GitHub Actions CI-Check ist grün (`.github/workflows/jules-quality-gate.yml`)
> 4. Keine bestehenden Tests des Workspace sind neu rot

```bash
# Triple-Test-Gate manuell ausführen:
just triple-test
```

### Tech-Debt Elimination Priority

Technische Schulden haben **höhere Priorität** als neue Features.  
Vor jedem neuen WP:

```bash
just debt-audit  # fürt alle Debt-Checks aus
```

Akzeptanzkriterium: **kein einziger Treffer** in den Debt-Scans.

---

## Atomic Spec Pflicht

> **Nichts wird implementiert ohne Atomic Spec.**

Alle Spezifikationen liegen in `docs/specs/`. Namenskonvention:
```
SPEC-YYYYMMDD-WP-X.Y-NAME.md
```

Neue Spec erstellen:
```bash
just spec WP-X.Y-NAME
```

---

## Work Package Status

| WP | Name | Priorität | Status | Spec |
|---|---|---|---|---|
| **WP-0.0** | Dependency Audit & Tech Debt | 🔴 KRITISCH | ✅ Stabil | [SPEC](./docs/specs/SPEC-20260505-WP-0.0-DependencyAudit.md) |
| **WP-1.1** | Background Compaction | 🔴 KRITISCH | ✅ Stabil | [SPEC](./docs/specs/SPEC-20260505-WP-1.1-Compaction.md) |
| **WP-1.2** | Collections / Namespaces | 🟠 HOCH | ✅ Stabil | [SPEC](./docs/specs/SPEC-20260505-WP-1.2-Collections.md) |
| **WP-2.1** | Hybrid Search (BM25+RRF) | 🟠 HOCH | ✅ Stabil | [SPEC](./docs/specs/SPEC-20260505-WP-2.1-HybridSearch.md) |
| **WP-2.2** | Scalar Quantization (SQ8) | 🟡 MITTEL | ⬜ Offen | [SPEC](./docs/specs/SPEC-20260505-WP-2.2-Quantization.md) |
| **WP-3.1** | Python Bindings (PyO3) | 🟠 HOCH | ⬜ Offen | [SPEC](./docs/specs/SPEC-20260505-WP-3.1-PythonBindings.md) |
| **WP-3.2** | Encryption at Rest | 🟡 MITTEL | ⬜ Offen | [SPEC](./docs/specs/SPEC-20260505-WP-3.2-Encryption.md) |
| **WP-4.1** | Memory-Mapped I/O | 🟡 MITTEL | ⬜ Offen | [SPEC](./docs/specs/SPEC-20260505-WP-4.x-Scale.md) |
| **WP-4.2** | Advanced Filtering | 🟡 MITTEL | ⬜ Offen | [SPEC](./docs/specs/SPEC-20260505-WP-4.x-Scale.md) |
| **WP-4.3** | DiskANN Out-of-Core | 🔵 ZUKUNFT | ⬜ Offen | [SPEC](./docs/specs/SPEC-20260505-WP-4.x-Scale.md) |
| **WP-6.1** | 4-Signal Fusion API | 🔵 ZUKUNFT | ⬜ Offen | [SPEC](./docs/specs/SPEC-20260509-GOLDSTANDARD-Funktionskatalog.md) |
| **WP-6.2** | Declarative StateGraph API | 🔵 ZUKUNFT | ⬜ Offen | [SPEC](./docs/specs/SPEC-20260509-GOLDSTANDARD-Funktionskatalog.md) |
| **WP-6.3** | Autonomes Kontext-Management | 🔵 ZUKUNFT | ⬜ Offen | [SPEC](./docs/specs/SPEC-20260509-GOLDSTANDARD-Funktionskatalog.md) |
| **WP-6.4** | Multi-Agent Namespaces | 🔵 ZUKUNFT | ⬜ Offen | [SPEC](./docs/specs/SPEC-20260509-GOLDSTANDARD-Funktionskatalog.md) |
| **WP-6.5** | Morphologische Inferenz-Optimierung | 🔵 ZUKUNFT | ⬜ Offen | [SPEC](./docs/specs/SPEC-20260509-GOLDSTANDARD-Funktionskatalog.md) |
| **WP-6.6** | Air-Gap Deployment Profile | 🔵 ZUKUNFT | ⬜ Offen | [SPEC](./docs/specs/SPEC-20260509-GOLDSTANDARD-Funktionskatalog.md) |
| **WP-6.7** | Kryptografische WAL-Verifikation | 🔵 ZUKUNFT | ⬜ Offen | [SPEC](./docs/specs/SPEC-20260509-GOLDSTANDARD-Funktionskatalog.md) |

---

## ⚠️ Autonomous Squad Protocol (13 Agents)

MemFuse is built by a squad of 13 autonomous agents. Each agent has a specific domain and a staggered execution window.

| # | Role | Domain | Schedule |
|---|---|---|---|
| 13 | **Debt Hunter** | Tech Debt & Invariant Cleanup | 05:00 UTC |
| 01 | **Core Guardian** | `memfuse-core` & Shared Types | 06:00 UTC |
| 02 | **Store Engineer** | `memfuse-store` (LSM / WAL) | 07:00 UTC |
| ... | ... | ... | ... |
| 07 | **QA Cross-Crate**| Integration & PR Verification | 20:00 UTC |

### <protocol name="Dynamic Queue Dispatch">
1. **Merge-Trigger**: On push to `develop`, the `jules-queue-dispatcher` calculates the next agent in the logical dependency chain.
2. **Lock-Sync**: The dispatcher executes `jules-sync-locks.sh` to block high-level tasks while low-level crates are `WIP`.
3. **Invocation**: The next agent is triggered via `jules-invoke.yml` with its specific API key.
</protocol>

### <protocol name="Triple-Test-Gate">
No code enters the `main` branch without passing 3 consecutive test runs, a Zero-Unwrap scan, and an Async-Safety audit. Warnings are treated as hard errors.
</protocol>

---

## Coding Doctrine (NON-NEGOTIABLE)

```rust
// ❌ FORBIDDEN:
.unwrap()                    // → Propagate error
std::fs::read()              // → strictly tokio::fs
unsafe { ... }               // → Only SIMD + // SAFETY: proof
```

**Prompts:** `.agent/jules/prompts/accounts/XX-name.md`
**Schedule:** `.agent/jules/SCHEDULE.md`
**Prompt-Generator:** `bash .agent/jules/scripts/generate-jules-prompt.sh <ACCOUNT> [WP]`
**CI:** `.github/workflows/jules-quality-gate.yml`

---

## Globale Architektur

[`Architecture Context`](./.agent/context/ARCHITECTURE.md)
