# AGENTS.md — memfuse-text
> Ergänzung zu Root-AGENTS.md. Nur crate-spezifische Regeln hier.

## Kritische Invarianten dieser Crate
- BM25 Inverted Index für lexikalische Volltextsuche.
- Deutsche Morphologie (GermanCompoundSplitter, Umlaut-Normalisierung) zur Keyword-Recall-Optimierung.

## Bekannte Fallstricke
- Tokenisierung muss deterministisch zwischen Indexierungs- und Query-Pfad identisch sein.

## Relevante rules/*.md
- `rules/test_quality.md` — Deterministic Search Recall Verification

## Pflicht-Tests (ANCHOR-Status)
- ANCHOR[TEST:TXT-001] STATUS:DONE — Recall-Evaluation für deutsche Zusammensetzungen (TS:2026-08-30T22:01:55Z SESSION:cf1f75c6)
