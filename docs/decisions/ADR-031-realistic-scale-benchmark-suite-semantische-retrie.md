# ADR-031: Realistic-Scale Benchmark Suite & Semantische Retrieval-Evaluierung


*   **Datum**: 2026-08-29
*   **Status**: ✅ Final
*   **Entscheidung**: Einführung einer reproduzierbaren, skalierbaren Benchmark-Suite (`benches/scale_bench.rs`), RSS-Speicherprofilierung (`/proc/self/status` logging nach `benches/results/scale_rss.csv`), semantischer Retrieval-Evaluierung (`crates/memfuse-db/tests/semantic_recall.rs` Recall@k) und eines CI-Baseline-Jobs (`.github/workflows/bench.yml`).
*   **Alternativen**: Weiterhin Verlass auf Micro-Benchmarks (1–1000 Chunks) und Quantisierungs-Konsistenz-Tests. Verworfen, da diese keine empirische Grundlage für künftige Architekturentscheidungen bzgl. Vamana/DiskANN und Quantisierung (v2-Spezifikation R3/R6) bieten.
*   **Begründung**: Bietet empirisch gemessene Durchsatz-, Latenz-Perzentil- (p50/p95/p99) und Speicher-Baselines (VmRSS) auf In-Memory HNSW & LSM-Storage sowie automatisierte Qualitäts-Gates für `hybrid_search()`.

---
