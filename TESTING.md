# TESTING.md — Testphilosophie & Qualitätskriterien

Dieses Dokument definiert die Standards für die Qualitätssicherung und Testabdeckung im MemFuse-Projekt.

---

## 1. Das Anti-Mirroring-Prinzip (Zentrales Gesetz)
Ein Test ist wertlos (eine Tautologie), wenn sein Erwartungswert mit derselben Formel oder denselben Berechnungs-Schritten ermittelt wird wie in der Implementierung.

### Falsch (Mirroring)
```rust
let expected = (a - b).powi(2).sqrt(); // Gleiche Formel wie compute()
assert_eq!(result, expected);
```

### Richtig (Unabhängiger Referenzwert)
```rust
// Euklidische Distanz von [1,0] und [0,1] ist sqrt(2) ≈ 1.4142135 (handberechnet/extern verifiziert)
assert!((result - 1.4142135).abs() < 1e-4);
```

---

## 2. Pflicht-Testabdeckung (Grenzfälle)
Jedes neue oder modifizierte Modul MUSS Tests für folgende Szenarien enthalten:
1.  **Happy Path**: Normale Eingaben, erwartete Ergebnisse.
2.  **Leere Eingabe**: Vektoren der Länge 0, leere Maps, leere Transaktionspuffer.
3.  **Einzelnes Element**: Listen/Graphen mit genau einem Element.
4.  **Grenzwerte**: `u64::MAX`, `f32::INFINITY`, extreme Dimensionen.
5.  **Fehlerpfade**: Dimensionen-Mismatches, ungültige Prüfsummen, korrupte WAL-Bytes.
6.  **Concurrency**: Stress-Testing mit parallelen Schreib-/Lesezugriffen bei der Verwendung von Sperren (`Mutex`, `RwLock`) oder atomaren Operationen.

---

## 3. Proptests für numerische Invarianten
Gegenüberstellungen von SIMD-Implementierungen und skalaren Fallbacks dürfen nicht nur mit statischen Handwerten getestet werden. Hier ist die Verwendung von **proptest** Pflicht, um den gesamten Wertebereich abzusichern (Determinismus-Gesetz: relative Abweichung Epsilon ≤ 1e-4).

---

## 4. Mutation-Robustheit (Score)
Bevor eine Änderung freigegeben wird, muss das Mutation-Gedankenexperiment durchgeführt werden:
> *„Wenn ich im Produktionscode einen Operator umkehre (`<` zu `<=`, `+1` zu `+0`), schlägt dann mindestens ein Test fehl?“*
Ist dies nicht der Fall, existiert eine Testlücke im entsprechenden Zweig.

---

## 5. Hermetic Feature Gate Check
Feature-gated crates (such as `memfuse-embed` with the `onnx` feature) must build cleanly when default features are disabled:
```bash
cargo check -p memfuse-embed --no-default-features
```
This check verifies zero leakage of optional dependencies or types into non-feature-gated modules.

## 6. Test-Code-Freigaben (Allowances)
*   **Permitted**: `.unwrap()` und `.expect()` sind in Test-Code (`#[cfg(test)]`) und Test-Hilfsfunktionen erlaubt.
*   **Production Code**: Diese Ausnahmen gelten **niemals** für den von Tests aufgerufenen Produktionscode. Produktionscode muss fehlerfrei über `Result` propagieren.

---

## 7. Weiterführende Regeln
*   [rules/testing.md](file:///home/freddy/Arbeitsplatz/DEV/memfuse/rules/testing.md) — Anti-Test-Mirroring & required categories.
*   [rules/test_quality.md](file:///home/freddy/Arbeitsplatz/DEV/memfuse/rules/test_quality.md) — Detaillierte Code-Beispiele für Test-Qualität.
