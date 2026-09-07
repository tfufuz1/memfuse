# MemFuse — Retrieval Accuracy Benchmark Report

**Stand / Zeitstempel**: `2026-09-03T10:00:00Z`
**Testkorpus**: 50 Dokument-Chunks, 10 Testabfragen

## Zusammenfassung der Messergebnisse

| Szenario | Modus | Recall@1 | Recall@3 | Recall@5 | MRR | Fehlerrate@1 | Delta (Recall@1) | Delta (Fehler) |
|---|---|---|---|---|---|---|---|---|
| **Szenario A**: Kontext-Präfix | Baseline (Ohne) | 80.0% | 80.0% | 80.0% | 0.800 | 20.0% | - | - |
| | Mit Kontext-Präfix | 80.0% | 80.0% | 80.0% | 0.800 | 20.0% | **+0.0%** | **-0.0%** |
| **Szenario B**: Reranking | Standard RRF (Ohne) | 60.0% | 60.0% | 60.0% | 0.600 | 40.0% | - | - |
| | Mit Cross-Encoder | 60.0% | 60.0% | 60.0% | 0.600 | 40.0% | **+0.0%** | **-0.0%** |
