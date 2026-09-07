# ADR-053: Instance-Scoped Orphan State in PersistentCheckpointStore

*   **Datum**: 2026-09-03
*   **Status**: ✅ Final
*   **Entscheidung**: Verwaiste Checkpoint- und Pin-Zustände werden instanzspezifisch in `PersistentCheckpointStore` verwaltet anstatt über prozessglobale statische Variablen (`ORPHANED_CHECKPOINTS`). Globale Hilfsfunktionen werden als `#[deprecated]` markiert.
*   **Begründung**: Stellt die Korrektheit in Multi-Session-Servern (MCP, Tauri) sicher, in denen mehrere unabhängige MemFuse-Instanzen parallel existieren.

---
