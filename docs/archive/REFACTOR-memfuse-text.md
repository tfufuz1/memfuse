# REFACTOR-PLAN: memfuse-text
**Datei:** `docs/specs/REFACTOR-memfuse-text.md`
**Erstellt:** 2026-05-27
**Priorität:** MEDIUM
**Geschätzter Aufwand:** 2 Tage
**Voraussetzung:** memfuse-core

---

## CRATE-ZUSTAND: EHRLICHE BEWERTUNG

| Dimension          | Aktuell       | Ziel          |
|--------------------|---------------|---------------|
| Panic-Freiheit     | 100% sauber   | 100%          |
| Skeleton-Anteil    | 1 Stelle      | 0             |
| Test-Coverage      | ~80%          | >90%          |
| API-Vollständigkeit| 85%           | 100%          |
| Algo-Korrektheit   | VERIFIED      | VERIFIED      |

---

## IDENTIFIZIERTE SCHWACHSTELLEN

### BLOCKING (Release-Blocker — SOFORT)

#### FIND-TEXT-001: Division by Zero in BM25 Scoring
**Typ:** Algo-Fehler (NaN/Inf)
**Datei:** `crates/memfuse-text/src/inverted.rs`
**Zeile(n):** 324, 362
**Code (Kontext):**
```rust
let avg_doc_len = if total_docs > 0 {
    total_tokens as f32 / total_docs as f32
} else {
    0.0
};
// ... wird an compute_bm25_score übergeben, wo durch avgdl dividiert wird ...
```
**Problem:** Wenn die Datenbank leer ist oder alle Dokumente in einem Namespace gelöscht wurden, ist `avg_doc_len` gleich 0.0. In `bm25.rs` führt dies zu einer Division durch Null bei der Score-Berechnung.
**Auswirkung:** Hybrid-Search liefert `NaN` Scores, was die RRF-Fusion in `memfuse-db` bricht.

**Refaktorisierungsanweisung:**
```
1. Stelle sicher, dass `avg_doc_len` niemals 0.0 ist (min 1.0 oder Fallback).
2. Füge in `compute_bm25_score` einen Guard gegen `avgdl == 0.0` hinzu.
3. Behandle den Fall "keine Dokumente" explizit in `InvertedIndex::search`.
```

**Akzeptanzkriterien:**
- [ ] `search()` auf leerem Index gibt leeres Resultat statt `NaN` zurück.

---

### HIGH (Pre-Launch — Innerhalb von 1 Woche)

#### FIND-TEXT-002: GermanCompoundSplitter ist ein unvollständiges Skelett
**Typ:** Skeleton
**Datei:** `crates/memfuse-text/src/morphology.rs`
**Zeile(n):** 62-100
**Code (Kontext):**
```rust
// Simple recursive splitting based on a set of known components
let dictionary = ["bundes", "verfassungs", ...];
```
**Problem:** Der `GermanCompoundSplitter` ist als `SCAFFOLD` markiert und nutzt eine extrem kleine, hartcodierte Liste von Wörtern. Er erkennt nur eine Handvoll technischer Begriffe.
**Auswirkung:** Mangelhafter Recall bei deutschen Texten, die nicht exakt die im Code stehenden Begriffe nutzen.

**Refaktorisierungsanweisung:**
```
1. Erweitere das `dictionary` um eine substantielle Liste (min. 500 häufige deutsche Wortstämme) oder binde eine externe Wortliste via Cargo-Feature ein.
2. Implementiere eine robustere Heuristik für das Splitting (z.B. längster Match zuerst).
3. Entferne den `SCAFFOLD`-Status nach der Erweiterung.
```

**Akzeptanzkriterien:**
- [ ] Test `test_german_expansion_ratio` zeigt Expansion bei allgemeinsprachlichen Texten.

---

### MEDIUM (Post-Launch — Tech-Debt Sprint)

#### FIND-TEXT-003: Sub-optimale Performance durch Standard-HashMap
**Typ:** Performance
**Datei:** `crates/memfuse-text/src/inverted.rs`
**Zeile(n):** 95
**Problem:** `InvertedIndex` nutzt die Standard-`HashMap` für Token-Frequenzen. In `memfuse-core` steht `ahash` zur Verfügung, das für kleine Keys (Tokens) signifikant schneller ist.
**Auswirkung:** Höhere Latenz beim Indizieren großer Dokumente.

**Refaktorisierungsanweisung:**
```
1. Ersetze `std::collections::HashMap` durch `ahash::AHashMap` (via `memfuse-core`).
```

**Akzeptanzkriterien:**
- [ ] `upsert_document` Latenz sinkt messbar.

---

## REFAKTORISIERUNGSREIHENFOLGE

```
Schritt 1: FIND-TEXT-001 (Korrektur der BM25-Logik)
Schritt 2: FIND-TEXT-002 (Wortlisten-Erweiterung)
Schritt 3: FIND-TEXT-003 (Performance Tuning)
```

## NEUE TESTS DIE NACH DEM REFACTORING ERSTELLT WERDEN MÜSSEN

```rust
// TEST-1: test_bm25_empty_index_safety
// Erzeuge leeren Index und suche. Darf kein NaN liefern.

// TEST-2: test_compound_splitting_general
// Testet Begriffe wie "Softwarearchitektur", "Wissensmanagement".
```

## SCHNITTSTELLEN-ÄNDERUNGEN (Breaking vs. Non-Breaking)

| Änderung                    | Breaking? | Migration-Pfad für Aufrufer    |
|-----------------------------|-----------|-------------------------------|
| Keine                       | —         | —                             |

## DONE-DEFINITION FÜR DIESES CRATE

Das Refactoring gilt als DONE (Triple-Test-Gate) wenn:
- [ ] BM25-Berechnungen sind numerisch stabil (kein NaN/Inf).
- [ ] German Splitter deckt ein breiteres Vokabular ab.
- [ ] `ahash` wird konsequent für In-Memory Strukturen genutzt.
- [ ] `just triple-test` 3× grün.
