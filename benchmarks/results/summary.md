# MemFuse — Retrieval Accuracy Benchmark Report

**Stand / Zeitstempel**: `2026-09-03T10:00:00Z`
**Testkorpus**: 8 Dokument-Chunks, 9 Testabfragen

## Zusammenfassung der Messergebnisse

| Szenario | Modus | Recall@1 | Recall@3 | Recall@5 | MRR | Fehlerrate@1 | Delta (Recall@1) | Delta (Fehler) |
|---|---|---|---|---|---|---|---|---|
| **Szenario A**: Kontext-Präfix | Baseline (Ohne) | 100.0% | 100.0% | 100.0% | 1.000 | 0.0% | - | - |
| | Mit Kontext-Präfix | 100.0% | 100.0% | 100.0% | 1.000 | 0.0% | **+0.0%** | **-0.0%** |
| **Szenario B**: Reranking | Standard RRF (Ohne) | 75.0% | 100.0% | 100.0% | 0.875 | 25.0% | - | - |
| | Mit Cross-Encoder | 75.0% | 100.0% | 100.0% | 0.875 | 25.0% | **+0.0%** | **-0.0%** |
