# ADR-038: Zettelkasten Memory Links (A-MEM) & Supersedes Displacement Logic

*   **Datum**: 2026-08-29
*   **Status**: ✅ Final
*   **Entscheidung**:
    1. Erweiterung von `ContextChunk` (`memfuse-core`) um `links: Vec<MemoryLink>` mit `#[serde(default)]`.
    2. Einführung von `LinkRelation` (`Elaborates`, `Contradicts`, `Supersedes`, `References`) und `MemoryLink` (`target: DocId`, `relation: LinkRelation`, `created_at_tx: TxId`).
    3. Implementierung der Methode `Collection::link_memories` (idempotent, interne `TxId` via `allocate_tx()`) und `Collection::traverse_links` (iterativer BFS mit `VecDeque`, zyklen-sicher, max `MAX_SEARCH_K`).
    4. Implementierung der Supersedes-Verdrängungslogik in `hybrid_search_with_query()`: Wenn `include_superseded = false` (Default), werden Chunks verdrängt, auf die ein anderes Treffer-Dokument einen `MemoryLink` der Relation `Supersedes` trägt.
*   **Alternativen**:
    - **Entity-to-Entity Verlinkung**: Verworfen, da CSR-Graph-Terrain (EntityId-zu-EntityId). Zettelkasten A-MEM operiert rein auf DocId-zu-DocId Ebene für ContextChunks.
*   **Begründung**:
    - Schafft explizite, benannte Querverweise zwischen ContextChunks zur Repräsentation geordneter Wissensnetze.
    - Automatisches Ausfiltern veralteter/ersetzter Chunks erhöht die Präzision des RAG-Retrievals, ohne Historie aus dem Speicher zu löschen.

---
