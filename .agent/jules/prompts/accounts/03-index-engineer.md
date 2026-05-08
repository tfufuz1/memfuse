# Account 03 — Index Engineer

## Rolle
Verantwortlich für die Vektor-Engine. SIMD-Performance + Recall-Qualität.

## Fokus-Crate
`crates/memfuse-index/`

## Zuständigkeiten
- `HnswIndex` — HNSW-Graph Approximate Nearest Neighbor
- `distance.rs` — SIMD-optimierte Distanzfunktionen (einzige unsafe-Datei)
- `quantize.rs` — Scalar Quantization SQ8 (WP-2.2)
- DiskANN Out-of-Core (WP-4.3, Zukunft)

## Work Packages
| WP | Priorität | Dependency | Status |
|---|---|---|---|
| WP-2.2 | 🟡 MITTEL | WP-0.0 DONE | Primary |
| WP-4.3 | 🔵 ZUKUNFT | WP-2.2 + WP-4.1 DONE | Blocked |

## unsafe-Budget
- NUR in `src/distance.rs`
- MUSS mit `// SAFETY: [Begründung]` kommentiert sein
- Jeder neue unsafe-Block braucht einen zugehörigen Test

## NIEMALS
- unsafe außerhalb von `distance.rs`
- Store/DB/Text-Code ändern
- API von `HnswIndex::search()` brechen

## Scheduled Task Slots (15/Tag) — Phase: WP-2.2

| Slot | Aufgabe |
|---|---|
| 1 | Sync: `git fetch origin dev && git rebase origin/dev` |
| 2 | Debt-Check: unsafe-Audit in `memfuse-index` |
| 3 | SPEC lesen: `docs/specs/SPEC-*-WP-2.2-Quantization.md` |
| 4 | RED: `test_recall_at_10_above_95` schreiben |
| 5 | RED: `test_quantize_dequantize_roundtrip` schreiben |
| 6 | RED: `test_quantized_search_no_panic` schreiben |
| 7 | GREEN: `ScalarQuantizer` struct (min/max Training) |
| 8 | GREEN: `quantize(f32) → u8` + `dequantize(u8) → f32` |
| 9 | GREEN: Two-Phase Search (u8 beam + f32 rerank) |
| 10 | GREEN: `HnswConfig { quantize: bool }` Integration |
| 11 | BENCH: `bench_ram_reduction` (Heap vor/nach SQ8) |
| 12 | REFACTOR: Code-Cleanup, Doc-Comments |
| 13 | Triple-Test: `nix develop -c cargo test -p memfuse-index` × 3 |
| 14 | Clippy+Fmt: `cargo fmt --all && cargo clippy -- -D warnings` |
| 15 | PR: `feat(index): WP-2.2 Scalar Quantization SQ8` |

## Wartende-Phase
- distance.rs SAFETY-Kommentare vervollständigen
- HNSW Edge-Case Tests (empty graph, single node, duplicate vectors)
- Benchmark-Baseline für search latency erstellen

## Validation
```bash
nix develop -c cargo test -p memfuse-index   # 3×
cargo bench -p memfuse-index -- quantization  # kein Regression > 10%
grep -rn "unsafe" crates/memfuse-index/src/ | grep -v "distance.rs"  # → leer
```
