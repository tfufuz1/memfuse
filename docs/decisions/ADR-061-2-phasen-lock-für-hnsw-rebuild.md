# ADR-061: 2-Phasen-Lock für HNSW Rebuild

*   **Datum**: 2026-09-04
*   **Status**: ✅ Final
*   **Kontext**: `HnswIndex::rebuild()` hielt bisher `write_mutex` über die gesamte Rebuild-Dauer (Snapshot, Offline-Index-Aufbau, Quantizer-Retraining, Re-Insert aller aktiven Nodes, Atomic Swap). Bei großen Indices blockierte dies Schreiboperationen (`insert`, `delete`, `commit`) für mehrere Sekunden bis Minuten.
*   **Entscheidung**: Umstellung von `rebuild()` auf ein 2-Phasen-Verfahren:
    - **Phase 1 (lock-frei bzgl. `write_mutex`)**: Erfassen eines Snapshot-TxId Watermarks (`last_tx_id`), Snapshot der aktiven Nodes unter kurzen Read-Locks, Aufbau des neuen Index inkl. Quantizer-Retraining komplett offline. Ingest-Schreibzugriffe laufen ungestört auf dem alten Index weiter.
    - **Phase 2 (kurzer exklusiver `write_mutex`-Scope)**: Erwerben des `write_mutex`, Ermittlung aller seit `snapshot_tx` getätigten Operationen via `SequenceLog::changes_since()`, Replay des Deltas auf den neuen Index und atomarer Swap der internen Datenstrukturen.
*   **Verworfene Alternativen**:
    - *Vollständig lock-freie Datenstruktur via `crossbeam-epoch`*: Verworfen, da dies ein komplettes Redesign der HNSW-Graphrepräsentation erfordern würde und mit hoher Komplexität verbunden ist.
*   **Konsequenzen**: Phase 2 skaliert mit $O(\Delta)$ (Anzahl Ingest-Operationen während Phase 1) statt $O(N)$ (Gesamtzahl Dokumente). Schreiblatenz während Rebuild sinkt von Sekunden auf Millisekunden.

---
