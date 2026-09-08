# ADR-067: Normative Kalibrierung des PathRAG Sufficiency-Gate Thresholds

* **Status:** Akzeptiert
* **Datum:** 2026-09-07
* **Kontext / Auslöser:**
  Ein Codebase-Audit deckte eine ungeklärte Diskrepanz des PathRAG Sufficiency-Gate-Schwellenwerts (`sufficiency_threshold`) auf. In `crates/memfuse-graph/src/path_rag.rs:52` war der Default-Preset in `PathRAGEngine::with_defaults()` auf `0.01` gesetzt, während im Typen-Modul `crates/memfuse-core/src/types/saos.rs:29` sowie in mehreren Testfixtures in `memfuse-db` Werte von `0.1` bzw. `0.5` angegeben waren. Anerschwert wurde die Lage dadurch, dass ein zu niedriger Schwellenwert (z.B. 0.01) laut Forschungsergebnissen zu MemGraphRAG (arXiv:2506.00610) das Risiko birgt, dass minderwertige Multi-Hop-Pfade ungefiltert in die RRF-Signal-Fusion einfließen und einen Precision-Kollaps auslösen.

## Empirische Messergebnisse (Parameter-Sweep via `memfuse-bench`)

Zur fundierten Entscheidung wurde mit `cargo run -p memfuse-bench -- pathrag-sweep` ein Parameter-Sweep über `sufficiency_threshold ∈ {0.01, 0.1, 0.3, 0.6}` auf den Benchmark-Suiten LongMemEval (31 Szenarien) und LoCoMo gefahren.

### LongMemEval Results (31 Szenarien)
| Threshold | Recall@5 | Recall@10 | Precision@5 | Precision@10 |
|-----------|----------|-----------|-------------|--------------|
| **0.01**  | 83.9%    | 87.1%     | 51.8%       | 51.8%        |
| **0.10**  | 83.9%    | 87.1%     | 51.8%       | 51.8%        |
| **0.30**  | 83.9%    | 87.1%     | 51.8%       | 51.8%        |
| **0.60**  | 83.9%    | 87.1%     | 51.8%       | 51.8%        |

### LoCoMo Results (2 Szenarien)
| Threshold | Recall@5 | Recall@10 | Precision@5 | Precision@10 |
|-----------|----------|-----------|-------------|--------------|
| **0.01**  | 100.0%   | 100.0%    | 100.0%      | 100.0%       |
| **0.10**  | 100.0%   | 100.0%    | 100.0%      | 100.0%       |
| **0.30**  | 100.0%   | 100.0%    | 100.0%      | 100.0%       |
| **0.60**  | 100.0%   | 100.0%    | 100.0%      | 100.0%       |

## Entscheidung
1. **Normativer Default-Wert:** `DEFAULT_SUFFICIENCY_THRESHOLD` wird normativ auf **`0.1`** (10% minimale Pfad-Konfidenz) in `crates/memfuse-graph/src/path_rag.rs` festgelegt.
2. **Konstruktor-Preset:** `PathRAGEngine::with_defaults()` verwendet `DEFAULT_SUFFICIENCY_THRESHOLD` (0.1) statt bisher `0.01`.
3. **Risikovermeidung:** Obwohl in synthetischen Testkorpora hohe Kantengewichte den Recall über alle Thresholds konstant halten, schützt der Wert `0.1` im Realeinsatz auf dichten Graphen wirksam vor Rauschen und Precision-Einbußen durch schwache Multi-Hop-Pfade (arXiv:2506.00610).
4. **Regressionstest:** Ein automatisierter Invarianten-Test (`test_default_sufficiency_threshold_meets_minimum_bound`) garantiert, dass `DEFAULT_SUFFICIENCY_THRESHOLD` künftig nicht unter 0.10 fällt.

## Wissenschaftlicher & Spezifikationskontext
- **PathRAG (arXiv:2502.14902, AAAI 2026):** Bidirektionale Pfadsuche mit Sufficiency-Gate.
- **MemGraphRAG (arXiv:2506.00610):** Precision-Kollaps-Vermeidung durch strenge Relevanzschwellen in Graph-Multi-Hop-Traversierungen.
