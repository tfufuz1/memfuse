# ATOMIC SPEC: FIND-TXT-003 BM25 Stability (Zero-Panic & NaN-Free)

## 1. Problemstellung
Der BM25-Scoring-Algorithmus in `memfuse-text` ist anfällig für Divisionen durch Null oder ungültige Logarithmus-Eingaben, was zu `NaN` (Not-a-Number) Scores führt. Dies tritt insbesondere auf bei:
- Leeren Indizes (0 Dokumente).
- Dokumenten mit Länge 0.
- Termen, die in mehr Dokumenten vorkommen als im Index registriert sind (Korruptionsfall).

## 2. Anforderungen (Invarianten)
1. **NaN-Freiheit:** Kein Aufruf von `search_bm25` darf Vektoren mit `NaN` oder `Inf` Scores zurückgeben.
2. **Deterministische Fallbacks:**
   - Wenn `total_docs == 0` -> Rückgabe leeres Ergebnis (bereits implementiert).
   - Wenn `avg_doc_len == 0.0` -> `norm_doc_len` soll als `1.0` gewertet werden (Neutral).
   - Wenn `doc_len == 0` -> `norm_doc_len` ist `0.0`.
   - Wenn `n < df` (Korruption) -> IDF soll auf einen minimalen positiven Wert (`1e-6`) gedeckelt werden, statt `NaN`.
3. **Sovereign Core:** Keine Panics (`unwrap()`, `expect()`).

## 3. Implementierungsplan

### Phase 1: Test-Enforcement (Red)
Schreibe Tests in `crates/memfuse-text/src/bm25.rs` und `inverted.rs`, die:
- Einen Score für ein Dokument der Länge 0 berechnen.
- Einen Score berechnen, wenn `avg_doc_len` 0.0 ist.
- Einen Score berechnen, wenn `df > n`.

### Phase 2: Korrektur `bm25.rs` (Green)
- Sichere `idf` Berechnung ab.
- Sichere `norm_doc_len` Berechnung ab.
- Stelle sicher, dass `tf_denominator` niemals 0 ist (durch die Konstanten `k1` und `b` bereits weitgehend gegeben, aber explizite Prüfung schadet nicht).

### Phase 3: Korrektur `inverted.rs` (Green)
- Validiere `avg_doc_len` Berechnung.
- Sicherstellen, dass `partial_cmp` in der Sortierung robust bleibt (obwohl `NaN` durch Phase 2 verhindert werden sollte).

## 4. Validierung
- `cargo test -p memfuse-text`
- `just triple-test -p memfuse-text`
