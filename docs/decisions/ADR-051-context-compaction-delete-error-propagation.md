# ADR-051: Context Compaction Delete Error Propagation

*   **Datum**: 2026-09-03
*   **Status**: ✅ Final
*   **Entscheidung**: In `ConsolidationSession::commit()` MUSS das Ergebnis der Quelldokument-Löschung (`delete_op`) zwingend mit `?` propagiert werden. Deserialisierungsfehler beim Lesen der Quelldokument-Metadaten geben `MemFuseError::Serialization` zurück. Nicht mehr auffindbare Quelldokumente werden geloggt und als Idempotenz-OK übergangen.
*   **Begründung**: Verhindert Datenverlust und stille Discards im Konsolidierungspfad.

---
