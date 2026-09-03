# Audit-Report: Phase 2 Kalibriertes Kaskaden-Routing in `memfuse-router`

**Datum:** 2026-09-03
**Crate:** `crates/memfuse-router`
**Auditor:** Jules (Senior Software Engineer)

---

## 1. Vorher/Nachher Routing-Algorithmus

### Vorher (Score-Aggregation & Emergency Fallback)
1. **Aggregierte Score-Berechnung:** Es wurden effekive Profile anhand von `calibrated_min_score` erstellt.
2. **Auswahl via `select_profile_from_chunks`:** Evaluierte `aggregated_score >= profile.min_relevance_score || max_score >= profile.min_relevance_score` über ungeordnete Kandidaten.
3. **Emergency Fallback:** Falls kein Profil die Relevanzschwelle erreichte, wurde über einen `Err(MemFuseError::NotFound)` abgefangen und das Profil mit dem global niedrigsten `min_relevance_score` gewählt.
4. **Schwächen:** Das war ein primitiver Notfall-Fallback bei Schwellenwert-Unterschreitung und kein strukturierter, kalibrierter Mehrstufen-Kaskaden-Entscheidungsprozess.

### Nachher (Kalibriertes Multi-Stage Kaskaden-Routing)
1. **Community-Filtering:** Nur Profile mit passender Community-Zuordnung (oder leerer `domain_communities`-Anforderung) werden als Kaskaden-Kandidaten berücksichtigt.
2. **Kaskaden-Sortierung:** Kandidaten-Profile werden deterministisch absteigend nach `min_relevance_score` sortiert (präzisestes/restriktivstes Profil zuerst). Bei gleichen Schwellenwerten greift Tie-Breaking (absteigender Aggregat-Score, gefolgt vom niedrigeren ursprünglichen Index).
3. **Stufenweise Kaskaden-Evaluierung (`select_profile_cascade`):**
   - Für jedes Profil in Kaskaden-Reihenfolge wird der Aggregat-Score berechnet.
   - Wenn `conformal.window_total > 10` vorliegt, gilt das Profil als konformitätskalibriert (`calibrated = true`) und die Schwelle ist `st.calibrated_min_score` (bzw. `st.conformal.quantile_threshold`).
   - Bei `window_total <= 10` gilt die initiale `min_relevance_score` Schwelle mit `calibrated = false`.
   - Sobald `score >= threshold` erfüllt ist, wird dieses Profil sofort ausgewählt.
4. **Geordneter Kaskaden-Fallthrough:** Erfüllt kein Profil die Schwelle, wird das am wenigsten restriktive Profil (am Ende der Kaskadensortierung) als sicherer Fallback gewählt. Eine `tracing::warn!` Warnung wird ausgegeben und `ConfidenceMetrics.calibrated = false` gesetzt.

---

## 2. Cascade-Test-Matrix

| Testname | Testfokus | Erwartetes Verhalten | Status |
| :--- | :--- | :--- | :---: |
| `test_cascade_hit` | 3 Profile (High 0.8, Mid 0.5, Low 0.2). Chunk Score = 0.6 | Wählt `mid-slm` (Score 0.6 >= Mid 0.5, aber < High 0.8). `calibrated = false`. | 🟢 PASSED |
| `test_cascade_fallthrough` | Chunk Score = 0.12 (erfüllt kein Profil: High 0.9, Mid 0.7, Low 0.5) | Fällt kaskadierend durch auf `low-slm`. `calibrated = false` + `tracing::warn!`. | 🟢 PASSED |
| `test_calibrated_threshold_convergence` | 15 aufeinanderfolgende Routing-Aufrufe | Aufrufe 1..10 liefern `calibrated = false`. Ab Aufruf 11 (>10 Samples) gilt `calibrated = true`. | 🟢 PASSED |
| `test_cascade_determinism` | 50 identische Routing-Aufrufe | Alle 50 Aufrufe wählen exakt dasselbe Profil (Invariante INV-ROUTER-2). | 🟢 PASSED |

---

## 3. Konfidenz-Konvergenz-Nachweis (15-Sample Test Output)

```text
Call 1: window_total=1, quantile_threshold=0.0105, calibrated=false
Call 2: window_total=2, quantile_threshold=0.02, calibrated=false
Call 3: window_total=3, quantile_threshold=0.0295, calibrated=false
Call 4: window_total=4, quantile_threshold=0.039, calibrated=false
Call 5: window_total=5, quantile_threshold=0.0485, calibrated=false
Call 6: window_total=6, quantile_threshold=0.058000002, calibrated=false
Call 7: window_total=7, quantile_threshold=0.0675, calibrated=false
Call 8: window_total=8, quantile_threshold=0.077, calibrated=false
Call 9: window_total=9, quantile_threshold=0.0865, calibrated=false
Call 10: window_total=10, quantile_threshold=0.09599999, calibrated=false
Call 11: window_total=11, quantile_threshold=0.10549999, calibrated=true
Call 12: window_total=12, quantile_threshold=0.11499999, calibrated=true
Call 13: window_total=13, quantile_threshold=0.124499984, calibrated=true
Call 14: window_total=14, quantile_threshold=0.13399999, calibrated=true
Call 15: window_total=15, quantile_threshold=0.14349999, calibrated=true
```

---

## 4. Testabdeckung & Verifikation Gesamtsuite

```text
cargo test -p memfuse-router --all-features
test result: ok. 40 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.34s

cargo clippy -p memfuse-router --no-deps -- -D warnings
Finished dev profile [unoptimized + debuginfo] target(s) in 0.79s
```

Alle Abnahmekriterien vollständig erfüllt.
