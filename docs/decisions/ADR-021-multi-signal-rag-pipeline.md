# ADR-021: Multi-Signal RAG-Pipeline (Contextual → RRF → Reranking)


*   **Datum**: 2026-08-27
*   **Status**: ✅ Final
*   **Kontext**: Die RAG-Sprints (RAG-01 bis RAG-05) haben die Ingestion-
    und Retrieval-Pipeline mit mehreren Schichten erweitert. Diese
    Entscheidung kodifiziert die Gesamtarchitektur.
*   **Entscheidung**: MemFuse implementiert eine mehrstufige RAG-Pipeline:
    1. **Contextual Ingestion**: ContextPrefixEngine (memfuse-ollama)
       generiert 50–100 Token LLM-Präfixe vor BM25/HNSW-Indexierung
    2. **4-Signal Indexierung**: HNSW + Contextual-BM25 + CSR-Graph +
       Metadaten parallel indexiert
    3. **Hybrid Retrieval via RRF**: Alle Signale über reciprocal_rank_fusion()
       fusioniert (memfuse-db/fusion.rs)
    4. **Multi-Step Expansion**: MultiStepEngine (memfuse-db/multistep.rs)
       führt bis zu 3 iterative Retrieval-Schleifen aus
    5. **Cross-Encoder Reranking**: CrossEncoderReranker (memfuse-embed,
       --features onnx) reordnet Top-K Kandidaten (optionaler Schritt)
    6. **Context Compaction**: ContextCompactor (memfuse-db/compaction.rs)
       ersetzt alte Tool-Outputs durch StatusToken
*   **Alternativen**: Jeder Schritt einzeln opt-in — zu komplex für Nutzer
*   **Begründung**: Empirisch (Anthropic, 2024): Contextual Embeddings →
    35% weniger Fehler; + Contextual BM25 → 49%; + Cross-Encoder → 67%.
    Die gestaffelte Pipeline ist additiv und gracefully degradierend
    (jede Stufe funktioniert ohne die nächste).
*   **Konsequenzen**:
    - BUG-03 (Audit 2026-08-27): combined_token_count() statt token_count()
      in ContextCompactor — Fix-Prompt existiert in docs/Audit-Reports/
    - BUG-02: parking_lot::Mutex statt std::sync::Mutex im Reranker
    - Alle Pipeline-Stufen sind optional und rückwärtskompatibel

---
