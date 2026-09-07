# ADR-058: Error-Logging-Pattern für synchrones Orphan-State Persistieren in Checkpoint

*   **Datum**: 2026-09-04
*   **Status**: ✅ Final
*   **Entscheidung**: In `InstanceOrphanRegistry` und `register_pinned_seq_no_orphan` (`crates/memfuse-checkpoint/src/lib.rs`) werden Schreibfehler beim synchronen Persistieren des Orphan-Zustands (`persist_sync()`) nicht mehr mit `let _ =` verworfen, sondern explizit über `if let Err(e) = ... { tracing::error!(?e, "..."); }` kontextspezifisch geloggt.
*   **Alternativen**:
    - Ändern der Rückgabetypen auf `Result<()>`: Verworfen, da Aufrufer in `Drop`-Implementierungen und synchronen Legacy-Funktionen keinen `?`-Kontext besitzen und dies zu kaskadierenden API-Breaks führen würde.
*   **Begründung**: Erfüllt CONSTITUTION.md §2 (kein stilles Verwerfen von E/A-Fehlern auf Recovery-Persistenzpfaden) ohne API-Signaturen zu brechen.

---
