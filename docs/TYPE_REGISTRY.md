# MemFuse — Central Type Registry (`TYPE_REGISTRY.md`)

> **Regel (AGENTS.md §4)**: Vor dem Anlegen eines neuen Typs oder Traits MUSS diese Tabelle konsultiert werden. Bei semantischen Überschneidungen ist der bestehende Typ zu erweitern oder die Kollision explizit per ADR zu begründen.

---

## 🏛️ Domain Types & Structs

| Typ / Struct / Enum | Crate | Datei : Zeile | Zweck / Domäne |
|---|---|---|---|
| `DocId` | `memfuse-core` | `crates/memfuse-core/src/types.rs:18` | 64-Bit BLAKE3-getrunkierte Dokumenten-ID |
| `TxId` | `memfuse-core` | `crates/memfuse-core/src/types.rs:42` | Transaktions-ID (`[1, 10^12]` vs. `INTERNAL_BASE` System-Range) |
| `EntityId` | `memfuse-core` | `crates/memfuse-core/src/types.rs:65` | Wissensgraph Knoten-Entitäts-ID |
| `MemFuseError` | `memfuse-core` | `crates/memfuse-core/src/error.rs:14` | Zentraler, abweisungsfreier Fehler-Enum |
| `MemFuseErrorDto` | `memfuse-core` | `crates/memfuse-core/src/error_dto.rs:10` | Serialisierbare FFI/IPC DTO-Fehlerdarstellung |
| `ContextChunk` | `memfuse-core` | `crates/memfuse-core/src/types/saos.rs:15` | Dokument-Chunk mit optionalem `contextual_prefix` (ADR-019) |
| `HybridQuery` | `memfuse-core` | `crates/memfuse-core/src/types/saos.rs:75` | Query-Spezifikation für 4-Signal-Suche |
| `MetadataFilter` | `memfuse-core` | `crates/memfuse-core/src/types/saos.rs:120` | Metadaten-Filter-Prädikate (Eq, Ne, In, Range, Contains, And, Or) |
| `CheckpointGuard` | `memfuse-checkpoint` | `crates/memfuse-checkpoint/src/lib.rs:24` | RAII-Guard für automatischen WAL-Rollback bei Drop |
| `CompactionStrategy` | `memfuse-db` | `crates/memfuse-db/src/compaction.rs:18` | Kontext-Kompaktierungs-Strategien (DropOld, Summarize, LlmSummarize) |
| `StoredDocument` | `memfuse-db` | `crates/memfuse-db/src/collection.rs:85` | In-Storage Repräsentation eines Dokuments inklusive Embeddings |
| `StoredDocumentMeta` | `memfuse-db` | `crates/memfuse-db/src/collection.rs:110` | In-Storage Repräsentation für schnelle Result-Hydration (ohne Vektoren) |
| `MemoryType` | `memfuse-core` | `crates/memfuse-core/src/types/domain.rs:535` | Klassifikation kognitiver Gedächtnistypen (Episodic, Semantic, Procedural, Working) (ADR-041) |

---

## 🔌 Central Traits

| Trait | Crate | Datei : Zeile | Zweck / Contract |
|---|---|---|---|
| `StorageEngine` | `memfuse-core` | `crates/memfuse-core/src/traits.rs:75` | LSM-Tree Speicherengine (MVCC, Transaktionen, Scans) |
| `VectorIndex` | `memfuse-core` | `crates/memfuse-core/src/traits.rs:175` | Vektor-Suchindex (HNSW/DiskANN) |
| `TextIndex` | `memfuse-core` | `crates/memfuse-core/src/traits.rs:305` | Lexikalischer Volltextindex (BM25) |
| `GraphIndex` | `memfuse-core` | `crates/memfuse-core/src/traits.rs:395` | CSR Entity-Relation-Wissensgraph |
| `TextEmbeddingEngine` | `memfuse-core` | `crates/memfuse-core/src/traits.rs:275` | Embedding-Provider Interface |
| `CheckpointCoordinator` | `memfuse-core` | `crates/memfuse-core/src/traits.rs:35` | Konsolidiertes Checkpoint-Management (ADR-011) |
