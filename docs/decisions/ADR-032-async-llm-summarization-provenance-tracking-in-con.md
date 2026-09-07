# ADR-032: Async LLM-Summarization & Provenance Tracking in ContextCompactor (ID: AGT-DB-004)


*   **Datum**: 2026-08-28
*   **Status**: ✅ Final
*   **Kontext**: Der bisherige `ContextCompactor` in `memfuse-db/src/compaction.rs` ersetzte veraltete Tool-Outputs durch Status-Token (ADR-021). Dies entsprach einer Kürzung/Löschung ohne kognitiven Wissenserhalt. Für Phase 3 der Roadmap ("Memory Consolidation") wird die Zusammenfassung alter Chunks via LLM unter Erhaltung der Provenienz benötigt.
*   **Entscheidung**:
    - Erweiterung der `CompactionStrategy` Enum um die additive Variante `LlmSummarize { max_input_chunks: usize }`.
    - Implementierung der asynchronen Methode `consolidate_via_llm(&self, chunks: &[ContextChunk], ollama: &OllamaClient) -> Result<CompactedContext>` in `compaction.rs`.
    - Das Ergebnis `CompactedContext` enthält ein neues Feld `pub source_doc_ids: Vec<DocId>` zur Nachvollziehbarkeit der Quell-Dokumente.
    - Fehler im LLM-Aufruf werden direkt als `Err(...)` an den Aufrufer propagiert und schlagen NICHT still auf StatusToken zurück (Prinzip: Kein stiller Kontrollflussverlust; Fallback-Entscheidung obliegt der Agenten-Orchestrierung).
*   **Alternativen**:
    - Stiller Fallback auf StatusToken innerhalb von `consolidate_via_llm` bei Netzwerk-/LLM-Fehlern. Verworfen, da dies Kontrollflussverluste verschleiern würde.
*   **Begründung**: Bietet eine saubere, provenance-bewahrende Konsolidierungsstrategie für Memory Consolidation und erfüllt das Gebot "No Silent Failures".
*   **Konsequenzen**:
    - Aufrufer können veraltete Chunks via `consolidate_via_llm` zusammenfassen und behalten Rückverfolgbarkeit auf alle Quell-DocIds.

---
