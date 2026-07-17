# Test-Qualitätskriterien

> Referenziert aus `AGENTS.md §8`

## Anti-Mirroring-Regel

Ein Test ist ungültig, wenn sein Erwartungswert aus derselben Formel berechnet wird wie die Implementierung.

**Falsch (Mirroring)**:
```rust
let result = rrf_score(rank: 0, k: 60);
assert_eq!(result, 1.0 / (60.0 + 0.0 + 1.0));  // Formel kopiert
```

**Richtig (unabhängiger Referenzwert)**:
```rust
// doc_b erscheint auf Rang 0 in Vektor-Set UND Rang 0 in Keyword-Set
// Erwarteter Score: 1/(60+1) + 1/(60+1) = 2/61 ≈ 0.03279
// doc_a erscheint nur auf Rang 1 in Vektor-Set: 1/(60+2) = 1/62 ≈ 0.01613
assert!(fused[0].id == "doc_b");  // höchster Score weil in beiden Sets
assert!(fused[0].score > fused[1].score);  // Reihenfolge ist unabhängig begründbar
```

## Pflicht-Grenzfälle

Jedes neue Modul braucht Tests für:
- Leere Eingabe
- Einzelnes Element
- Maximum capacity (OOM-Grenze, falls vorhanden)
- Fehlerfall (ungültige Dimension, korrupte Bytes, etc.)

## Proptest-Pflicht

Numerische Invarianten (SIMD vs. Scalar, Quantisierungsfehler) → **proptest**, nicht einzelne Handwerte.

## Mutation-Robustheit

Prüfprinzip: Würde `< statt <=` oder `+1 statt -1` diesen Test brechen?  
Falls nein: Test hat kein ausreichendes Differenzierungspotenzial → erweitern.
