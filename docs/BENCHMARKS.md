# MemFuse Performance & Evaluation Benchmarks

*Datum: 2026-08-29*
*Hardware / Environment: Linux x86_64, 4 CPU Cores, 7.8 GiB RAM (Jules Sandbox VM)*

---

## 1. Realistic-Scale Throughput & Latenz (`benches/scale_bench.rs`)

Die Messungen wurden mit synthetischen, 768-dimensionalen Vektoren und variierenden technischen Text-Chunks auf einem In-Memory/LSM hybrid storage engine setup durchgeführt.

| Corpus-Größe (Chunks) | Insert-Durchsatz (docs/sec) | Search Latenz p50 | Search Latenz p95 | Search Latenz p99 | VmRSS Peak (MB) |
|---|---|---|---|---|---|
| **1,000** | ~117.3 docs/sec | 88.03 ms | 90.73 ms | 90.73 ms | 26.14 MB |
| **5,000** | ~89.5 docs/sec | 809.15 ms | 825.68 ms | 825.68 ms | 121.14 MB |
| **10,000** | ~75.2 docs/sec | 337.77 ms | 351.51 ms | 351.51 ms | 207.13 MB |
| **100,000** *(extrapoliert)* | ~25 docs/sec | ~3,500 ms | ~4,200 ms | ~4,500 ms | ~1,950 MB |
| **1,000,000** *(extrapoliert)* | ~5 docs/sec | > 30 s | > 45 s | > 60 s | > 18.5 GB (exceeds RAM) |

*Rohdaten-Log für RSS-Messung:* `benches/results/scale_rss.csv`

---

## 2. Semantische Retrieval-Evaluierung (`Recall@k`)

Verifizierte Messung gegen synthetische Ground Truth (20 Themen-Cluster × 50 Dokumente = 1.000 Dokumente, 100 Test-Queries):

- **Recall@5**:  1.0000 (100.0 %)
- **Recall@10**: 1.0000 (100.0 %)
- **Recall@20**: 1.0000 (100.0 %)

*Testergebnis:* `crates/memfuse-db/tests/semantic_recall.rs` bestanden (`assert!(mean_recall_at_10 >= 0.80)`).

---

## 3. Was diese Zahlen NICHT zeigen (Explizite Messgrenzen)

1. **Kein Cross-System-Vergleich**: Diese Benchmarks messen ausschließlich die interne Performance von MemFuse. Es wurden keine Messungen gegen Redis, ChromaDB, Qdrant oder Milvus durchgeführt.
2. **Kein Real-World-Corpus**: Die Dokumente und Vektoren wurden deterministisch synthetisiert (Cluster-Phasenvektoren + Gaussian-Noise, vordefinierte Keyword-Familien). Real-World-Textkorpora (z.B. MS MARCO, BEIR) weisen abweichende Sparsity- und Cluster-Eigenschaften auf.
3. **Keine echten Neural Model Embeddings**: Es wurden keine Embeddings durch ein lokales ONNX/Ollama-Modell während des Benchmarks berechnet (Embedding-Generierungszeit ist excluded).
4. **Vollständiger In-Memory HNSW-Graph**: Bei 1M+ Chunks überschreitet der RAM-Bedarf von `HnswIndex` die physische RAM-Grenze typischer Developer-VMs (7.8 GiB). Dies unterstreicht die Notwendigkeit künftiger Vamana/DiskANN- und SQ8-Quantisierungsarchitekturen gemäß v2-Spezifikation R3/R6.
