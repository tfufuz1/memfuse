# ADR-041: TOMBSTONE_BIT-Disziplin in Sequenznummer-Berechnungen und rollback_to_tx

*   **Datum**: 2026-08-29
*   **Status**: ✅ Final
*   **Entscheidung**: In allen Pfaden der LSM-Storage-Engine (`rollback_to_tx`, WAL-Replay, SSTable-Recovery), in denen maximale Sequenznummern (`max_seq`) ermittelt werden, MUSS das `TOMBSTONE_BIT` (Bit 63, `1 << 63`) strikt maskiert werden (`seq & !TOMBSTONE_BIT`), bevor Vergleiche, Zuweisungen oder Hochzählungen für `next_seq_no` stattfinden.
*   **Alternativen**:
    - Unmaskierte Übernahme in `max_seq`: Verworfen, da Bit 63 in `next_seq_no` wandert und nachfolgende reguläre Inserts fälschlich als gelöscht (Tombstone) markiert.
    - Maskierung beim Schreiben der SSTable-Metadaten verändern: Verworfen, um bestehende Metadatenformate und Disk-Layouts nicht zu verändern.
*   **Begründung**: Bit 63 signalisiert ausschließlich das Lösch-Tombstone-Flag in Datenzeilen. Es stellt keinen numerischen Wertanteil der Sequenznummer dar. Maskierung an den Lesestellen schützt die Invariante "Bit 63 darf niemals in `next_seq_no` einfließen" vollständig vor stillem Datenverlust nach Rollbacks auf Delete-Operationen.

---
