# ADR-011: Consolidate Checkpoint Subsystems (CheckpointCoordinator Trait)

*   **Datum**: 2026-08-23
*   **Status**: ✅ Final
*   **Entscheidung**: Einführung des Trait `CheckpointCoordinator` in `memfuse-core::traits` zur Harmonisierung der Checkpoint-Architektur. `PersistentCheckpointStore` (in `memfuse-checkpoint`) implementiert `CheckpointCoordinator`. `Checkpointer`/`CheckpointGuard` in `memfuse-store` verbleiben als interne RAII-Guards für transaktionale WAL-Rollbacks.
*   **Alternativen**: Physische Löschung von `memfuse-checkpoint` und Migration aller Typen in `memfuse-store`.
*   **Begründung**: Klare Rollentrennung: `CheckpointCoordinator` stellt die öffentliche, benannte API für persistenten State bereit (verwendet in `memfuse-db`), während `Checkpointer`/`CheckpointGuard` RAII-Abstraktionen für WAL-Level Rollbacks innerhalb der LSM-Engine sind. Behebt Befund AGT-STORE-002 [DUPLICATION][MAJOR].

---
