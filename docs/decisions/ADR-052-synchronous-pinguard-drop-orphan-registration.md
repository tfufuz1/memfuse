# ADR-052: Synchronous PinGuard Drop Orphan Registration

*   **Datum**: 2026-09-03
*   **Status**: ✅ Final
*   **Entscheidung**: `PinGuard::drop` registriert verwaiste Sequenznummern synchron via `OrphanRegistry` ohne asynchrone Tasks (`tokio::spawn`) abzuspalten.
*   **Begründung**: Verhindert verdeckte Space-Leaks und unvollständige Drops bei Prozess-Shutdowns.

---
