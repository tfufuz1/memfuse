# SPEC-SAOS-WP-5.4 — Adaptive Filter-Strategie (Pre/Post-Filter Heuristik)

> **Priority:** 🟡 MITTEL  
> **Status:** ✅ DONE
> **Dependency:** WP-1.2 DONE, WP-4.2 (ergänzt und präzisiert diesen WP)  
> **Crate:** `memfuse-db` (Erweiterung)  
> **DONE-Definition:** 3 Tests 3× grün. Automatische Strategie-Wahl validiert.

## Entscheidungslogik

```rust
fn choose_filter_strategy(
    filter_selectivity: f32,  // 0.0 = kein Dokument matched, 1.0 = alle matched
    index_size: usize,
) -> FilterStrategy {
    match filter_selectivity {
        s if s < 0.05 => FilterStrategy::PreFilter,   // <5% matchen → Index erst filtern
        s if s > 0.50 => FilterStrategy::PostFilter,   // >50% matchen → HNSW zuerst
        _             => FilterStrategy::Hybrid,        // Beide parallel, merge
    }
}
```

## Selectivity-Schätzung

Selectivity wird geschätzt via Bloom-Filter auf dem LSM-Store-Layer:

```rust
bloom_filter.contains(filter_key) && approx_count / total_docs
```

## Acceptance Criteria

| # | Test | Erwartung |
|---|---|---|
| AC-1 | `test_pre_filter_chosen_for_low_selectivity` | 2/100 Docs matchen → Strategy == PreFilter |
| AC-2 | `test_post_filter_chosen_for_high_selectivity` | 80/100 Docs matchen → Strategy == PostFilter |
| AC-3 | `test_results_identical_regardless_of_strategy` | Gleiche Query, beide Strategien → identische Result-Sets |
