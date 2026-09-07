# ADR-025: Memory Importance Score & Recency-Decay als Post-Processing-Filter (Erweiterung ADR-021 & ADR-024)


*   **Datum**: 2026-08-28
*   **Status**: ✅ Final
*   **Kontext**: Roadmap Phase 2 fordert ein LLM-bewertetes Memory Importance Scoring (`ImportanceScore`) und eine Recency-Decay-Funktion (`DecayFunction`) für episodische Relevanz. Es stellte sich die Frage, wie der berechnete `effective_score(now_tx)` in die RAG-Pipeline (ADR-021) integriert wird.
*   **Entscheidung**:
    - Der `effective_score(now_tx)` wird als Nachbearbeitungsschritt **NACH** RRF (Reciprocal Rank Fusion) und **NACH** Cross-Encoder Reranking in der RAG-Pipeline ausgeführt (`Collection::filter_by_importance`).
    - Kandidaten mit `effective_score` unterhalb eines konfigurierbaren Schwellwerts werden aus den finalen Suchergebnissen entfernt.
    - Es findet **KEINE** Neubewertung / Re-Ranking durch Multiplikation des RRF- / Cross-Encoder-Scores mit dem `effective_score` statt.
*   **Alternativen**:
    - Multiplikation des `effective_score` direkt in die RRF-Rankings: Verworfen, da dies die mathematischen RRF-Skalierungsunabhängigkeiten und die empirisch validierte RRF/Reranking-Reihenfolge aus ADR-021 zerstören würde.
*   **Begründung**: Filterung statt Re-Ranking schützt die empirisch nachgewiesenen Trefferquoten des Hybrid-Retrievals (Anthropic Pattern, ADR-021), während irrelevante oder veraltete Erinnerungen (Low Importance / High Decay) zuverlässig ausgeschieden werden.
*   **Konsequenzen**:
    - `filter_by_importance()` in `Collection` filtert nach RRF/Reranker ohne Umsortierung.
    - Zero-Panic Invariante in `ImportanceScore`, `DecayFunction` und `MemoryImportance`.

---
