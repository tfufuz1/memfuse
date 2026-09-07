# ADR-023: Kompensierende Transaktion für Multi-Store relate() Operations (F-01 / AGT-DB-005)


*   **Datum**: 2026-08-28
*   **Status**: ✅ Final
*   **Kontext**: `Collection::relate()` führt Operationen über heterogene Storage-Backends (`LsmStorage` und `CsrGraph`) aus. Nachdem `storage.commit(tx)` aufgerufen wurde, ist der `TxBuffer`-Eintrag für `tx` geleert und im WAL dauerhaft persistiert. Ein nachfolgender Fehler in `graph_index.commit(tx)` führte dazu, dass `rollback_relate(tx)` aufgerufen wurde, was wiederum `storage.rollback(tx)` aufrief. Da `storage.rollback(tx)` jedoch nur uncommittete `TxBuffer`-Einträge verwirft (`tx_buffer.discard(tx)`), war der Rollback für den Storage-Teil ein wirkungsloser No-Op. Dies führte zu inkonsistentem Zustand zwischen Storage und Graph-Index.
*   **Entscheidung**: Implementierung von Option A: Kompensierende Transaktion. Falls `storage.commit(tx)` erfolgreich ist, aber `graph_index.commit(tx)` fehlschlägt, wird eine kompensierende Löschtransaktion (`storage.delete()` + `storage.commit()`) mit einer neu allokierten `TxId` ausgeführt, um den bereits committeten Relations-Key wieder aus dem LSM-Storage zu entfernen (Tombstone-Eintrag schreiben).
*   **Alternativen**:
    - **Option B (2-Phase Commit Protocol)**: Einführung einer `prepare()`-Methode auf `GraphIndex`. Verworfen, da dies Trait-Verträge in `memfuse-core` und allen Implementierungen anpassen müsste und höhere API-Komplexität mit sich bringt.
    - **Option C (Vereinheitlichung der Commit-Klammer)**: `CsrGraph` und `LsmStorage` in eine gemeinsame Transaktionsklammer verschmelzen. Verworfen, da `CsrGraph` in-memory eigene CSR-Strukturen und Delta-Buffer verwaltet und eine Zusammenlegung die Layer-Architektur aufbrechen würde.
*   **Begründung**: Option A benötigt keine breaking API-Änderungen an den Trait-Schnittstellen (`memfuse-core`), hat vernachlässigbaren Performance-Overhead im Fehlerfall und ist vollständig konsistent mit bestehenden Tombstone- und Kompensationsmustern im Repo (wie `DbTransaction::commit()` in `transaction.rs`).
*   **Konsequenzen**:
    - `Collection::relate()` führt bei Fehlschlag von `graph_index.commit(tx)` nach erfolgreichem `storage.commit(tx)` einen kompensierenden Delete-Commit aus.
    - Doc-Kommentare in `LsmStorage` und `StorageEngine` beschreiben die exakte Garantie: `rollback()` verwirft nur uncommittete `TxBuffer`-Einträge; ein Undo nach physischem Commit erfordert einen Compensating-Write.

---
