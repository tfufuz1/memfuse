# ADR-015: RAII CheckpointGuard Integration & Konsolidierung in `memfuse-checkpoint` (AGT-CKPT-001 / AGT-STORE-002)

*   **Datum**: 2026-08-24
*   **Status**: ✅ Final
*   **Entscheidung**:
    1. Das RAII-Guard-Muster für transaktionales Auto-Rollback bei Drop (`CheckpointGuard`) wird aus `memfuse-store::checkpoint` abstrahiert und als generischer Guard `CheckpointGuard<S: StorageEngine>` in `memfuse-checkpoint` (Layer 1) implementiert.
    2. `PersistentCheckpointStore` wird um ein optionales RAII-Guard-Verfahren ergänzt (`begin_guarded_checkpoint(...) -> Result<CheckpointGuard<S>>`), welches `StorageEngine::rollback_to_tx` im `Drop`-Handler ausführt, sofern der Guard nicht vorab via `.commit()` explizit konsumiert wurde.
    3. `memfuse-store::checkpoint::Checkpointer` entfällt als redundantes Duplikat bzw. delegiert fortan intern an `PersistentCheckpointStore<LsmStorage>`.
*   **Alternativen**:
    - **Option A (Entkoppelt lassen)**: Führt zu dauerhafter Code-Duplizierung und zwei verschiedenen Checkpoint-Konzepten (`StateCheckpoint` vs `CheckpointMeta`), was gegen AGT-STORE-002 und AGT-CKPT-001 verstößt.
    - **Option B (Entfernen von CheckpointGuard)**: Entfernt die RAII-Garantie gegen Transaktions-Leaks bei Unhandled Panics oder unvollständigen Operationen.
*   **Begründung**:
    - `memfuse-checkpoint` ist Layer 1 und die in ADR-011 definierte Zielarchitektur für Checkpointing.
    - `CheckpointGuard` hängt funktional nur vom Trait `memfuse_core::StorageEngine` ab (Layer 0), nicht von `LsmStorage` (Layer 1). Daher kann `CheckpointGuard<S: StorageEngine>` ohne DAG-Zyklen sauber in Layer 1 (`memfuse-checkpoint`) beheimatet werden.
    - Die bestehende öffentliche API von `PersistentCheckpointStore` und `CheckpointRegistry` bleibt zu 100% abwärtskompatibel erhalten.
*   **Konsequenzen**:
    - Verlinkung mit `AGT-STORE-002` in `memfuse-store`.
    - Sobald der Entwurf vom Entwickler freigegeben ist, erfolgt die Migration in `memfuse-checkpoint` und `memfuse-store` ohne API-Bruch.

---
