# Account 05 — Text Engine

## Rolle
Aufbau der Volltext-Suchengine. BM25 + Inverted Index als neues Crate.

## Fokus-Crate
`crates/memfuse-text/` (NEUES CRATE)

## Zuständigkeiten
- `tokenizer.rs` — Unicode-aware Tokenizer (~100 LoC)
- `inverted.rs` — LSM-backed Inverted Index / Posting Lists (~200 LoC)
- `bm25.rs` — BM25 Scoring pure function (~80 LoC)

## Work Packages
| WP | Priorität | Dependency | Status |
|---|---|---|---|
| WP-2.1 | 🟠 HOCH | WP-1.2 DONE | Primary |

## Dependency-Limit
**Maximal 2 neue externe Dependencies:**
- `unicode-segmentation = "1"` (Unicode word boundaries)
- `bincode = "1"` (Posting List Serialisierung)
- **KEINE Tantivy, Lucene, oder andere Search-Engines**

## NIEMALS
- Bestehende Crates ändern (nur `memfuse-db/src/fusion.rs` + `lib.rs` für Integration)
- Dependencies > 500 LoC transitiv hinzufügen
- Separaten Index außerhalb des LSM-Stores aufbauen

## Scheduled Task Slots (15/Tag) — Phase: WP-2.1

| Slot | Aufgabe |
|---|---|
| 1 | Sync: `git fetch origin dev && git rebase origin/dev` |
| 2 | Crate-Scaffold: `Cargo.toml`, `src/lib.rs` mit `//!` Doc |
| 3 | SPEC lesen: `docs/specs/SPEC-*-WP-2.1-HybridSearch.md` |
| 4 | RED: `test_bm25_ranks_exact_keyword_higher` |
| 5 | RED: `test_rrf_combines_result_sets` |
| 6 | RED: `test_empty_text_falls_back_to_vector` |
| 7 | RED: `test_tokenizer_handles_unicode` |
| 8 | GREEN: `Tokenizer` — Unicode word segmentation + lowercase |
| 9 | GREEN: `InvertedIndex` — LSM-backed posting lists |
| 10 | GREEN: `bm25_score()` — pure function (tf, idf, doc_len) |
| 11 | GREEN: `fusion.rs` in `memfuse-db` — RRF Kombination |
| 12 | GREEN: `hybrid_search()` Integration in `memfuse-db/lib.rs` |
| 13 | Triple-Test: `nix develop -c cargo test -p memfuse-text` × 3 |
| 14 | Workspace-Test: `nix develop -c cargo test --workspace` |
| 15 | PR: `feat(text): WP-2.1 Hybrid Search BM25 + RRF` |

## RRF-Formel
```
score(doc) = Σ_{r ∈ result_sets} 1 / (k + rank_r(doc))
k = 60  (Standard-Wert, konfigurierbar)
```

## Validation
```bash
nix develop -c cargo test -p memfuse-text   # 3×
nix develop -c cargo test -p memfuse-db     # Keine Regressionen
nix develop -c cargo test --workspace       # Alles grün
```
