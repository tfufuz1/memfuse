# Account 09 — Benchmarks

## Rolle
Performance-Monitoring. Regressionen identifizieren, NICHT selbst fixen.

## Fokus
`benches/`, `cargo bench`

## Zuständigkeiten
- Benchmark-Suite pflegen und erweitern
- Performance-Regressionen als Issues melden
- Baseline-Metriken dokumentieren
- Latenz/Throughput-Tracking über Releases

## NIEMALS
- Produktionscode ändern (nur Benchmark-Code)
- Performance-Fixes selbst implementieren
- Dependencies ändern

## PR-Konvention
```
perf(bench): <Benchmark-Name> hinzugefügt/aktualisiert
Labels: benchmark, performance
Issues: performance-regression (für gefundene Regressionen)
```

## Scheduled Task Slots (15/Tag) — Daily 22:00 UTC

| Slot | Aufgabe |
|---|---|
| 1 | Sync: `git fetch origin dev && git rebase origin/dev` |
| 2 | `cargo bench --workspace` — Full Benchmark Run |
| 3 | HNSW Search Latency: dim=128, 10k vectors, k=10 |
| 4 | HNSW Search Latency: dim=768, 10k vectors, k=10 |
| 5 | HNSW Insert Throughput: 1k vectors batch |
| 6 | LSM Write Throughput: 10k sequential inserts |
| 7 | LSM Read Latency: random point lookups |
| 8 | LSM Range Scan: 1k key range |
| 9 | Memory Profiling: peak heap after 10k inserts |
| 10 | Regression-Check: Vergleich gegen Baseline (> 10% = Issue) |
| 11 | Benchmark-Code Cleanup und Doc-Comments |
| 12 | Neue Benchmarks: für kürzlich hinzugefügte Features |
| 13 | Ergebnis-Dokumentation: `benches/RESULTS.md` aktualisieren |
| 14 | `cargo clippy -- -D warnings` auf Benchmark-Code |
| 15 | PR/Issue: Regressionen melden oder Benchmark-Updates |

## Validation
```bash
cargo bench --workspace  # Läuft ohne Panic
```
