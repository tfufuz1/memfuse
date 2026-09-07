# ADR-049: Audit-Log Append-Only Enforcement via `put_kv_if_absent`

*   **Datum**: 2026-09-03
*   **Status**: ✅ Final
*   **Entscheidung**: `Collection` wird um die atomare Methode `put_kv_if_absent(&self, id: &str, value: &serde_json::Value)` erweitert, die vor dem Schreiben eine tx-scoped Existenzprüfung durchführt und bei Treffer `MemFuseError::Conflict` zurückgibt. `AuditLog::append()` nutzt ausschließlich `put_kv_if_absent()`.
*   **Alternativen**:
    - Nutzung von `put_kv()` mit clientseitigem `get_kv()`-Check: Verworfen, da race-condition-anfällig bei parallelen `append()`-Aufrufen.
    - Schreibsperre auf Tabellenebene: Verworfen wegen unötigem Performance-Overhead für nicht-kollidierende Steps.
*   **Begründung**: Garantiert die deklarierte Invariante des Audit-Logs ("immutable append-only trail, zero overwrite/deletion paths").

---
