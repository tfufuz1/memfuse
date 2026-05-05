# MemFuse — Agentic OS Directory & Crate Lookup

> **Phase:** Feature Implementation & TDD  
> **Doctrine:** Zero-Panic / Sovereign Core / Triple-Test-Gate  
> **Updated:** 2026-05-05

---

## Crate-Übersicht & Lokale Agent-Kontexte

| Crate | Rolle | LoC | Status |
|:------|:------|:----|:-------|
| **`memfuse-core`** | Grundlegende Typen, TxBuffer, MemBank, Error, Snapshots | ~280 | ✅ Stabil |
| **`memfuse-store`** | LSM-Storage, MemTables, WAL, Compaction | ~1400 | ⚠️ WP-1.1 offen |
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
| **WP-0.0** | Dependency Audit & Tech Debt | 🔴 KRITISCH | ⬜ Offen | [SPEC](./docs/specs/SPEC-20260505-WP-0.0-DependencyAudit.md) |
| **WP-1.1** | Background Compaction | 🔴 KRITISCH | ⬜ Offen | [SPEC](./docs/specs/SPEC-20260505-WP-1.1-Compaction.md) |
| **WP-1.2** | Collections / Namespaces | 🟠 HOCH | ⬜ Offen | [SPEC](./docs/specs/SPEC-20260505-WP-1.2-Collections.md) |
| **WP-2.1** | Hybrid Search (BM25+RRF) | 🟠 HOCH | ⬜ Offen | [SPEC](./docs/specs/SPEC-20260505-WP-2.1-HybridSearch.md) |
| **WP-2.2** | Scalar Quantization (SQ8) | 🟡 MITTEL | ⬜ Offen | [SPEC](./docs/specs/SPEC-20260505-WP-2.2-Quantization.md) |
| **WP-3.1** | Python Bindings (PyO3) | 🟠 HOCH | ⬜ Offen | [SPEC](./docs/specs/SPEC-20260505-WP-3.1-PythonBindings.md) |
| **WP-3.2** | Encryption at Rest | 🟡 MITTEL | ⬜ Offen | [SPEC](./docs/specs/SPEC-20260505-WP-3.2-Encryption.md) |
| **WP-4.1** | Memory-Mapped I/O | 🟡 MITTEL | ⬜ Offen | [SPEC](./docs/specs/SPEC-20260505-WP-4.x-Scale.md) |
| **WP-4.2** | Advanced Filtering | 🟡 MITTEL | ⬜ Offen | [SPEC](./docs/specs/SPEC-20260505-WP-4.x-Scale.md) |
| **WP-4.3** | DiskANN Out-of-Core | 🔵 ZUKUNFT | ⬜ Offen | [SPEC](./docs/specs/SPEC-20260505-WP-4.x-Scale.md) |

---

## Jules Account Matrix

| Account | Crate/Rolle | Scheduled Tasks |
|---|---|---|
| 01 | `memfuse-core` | WP-0.0 Tech Debt |
| 02 | `memfuse-store` | WP-1.1, WP-4.1 |
| 03 | `memfuse-index` | WP-2.2, WP-4.3 |
| 04 | `memfuse-db` | WP-1.2, WP-4.2 |
| 05 | `memfuse-text` | WP-2.1 |
| 06 | `memfuse-py` | WP-3.1 |
| 07 | QA Cross-Crate | Täglich 20:00 UTC |
| 08 | Docs & Specs | Wöchentlich Mo |
| 09 | Benchmarks | Täglich 22:00 UTC |
| 10 | Security/Crypto | WP-3.2 |

**Prompts:** `.agent/jules/prompts/`  
**CI:** `.github/workflows/jules-quality-gate.yml`

---

## Globale Architektur

[`Architecture Context`](./.agent/context/ARCHITECTURE.md)
