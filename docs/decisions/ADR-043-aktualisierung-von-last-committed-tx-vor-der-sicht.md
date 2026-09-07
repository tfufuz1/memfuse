# ADR-043: Aktualisierung von `last_committed_tx` vor der Sichtbarmachung von SSTables in `LsmStorage::flush`

*   **Datum**: 2026-08-29
*   **Status**: ✅ Final
*   **Entscheidung**: In `LsmStorage::flush()` MUSS `last_committed_tx` aktualisiert werden, BEVOR die neu erstellte SSTable über den `sstables`-Vektor für Lesepfade (z. B. `get_at_seq`, `scan_prefix_at`) sichtbar gemacht wird (`last_committed_tx vor Datensichtbarkeit aktualisieren`).
*   **Alternativen**:
    - Beibehalten der bisherigen Reihenfolge (`sstables.push` vor `last_committed_tx` update): Verworfen, da hierbei ein Race-Fenster entsteht, in dem ein paralleler Reader die neue SSTable bereits im `sstables`-Vektor sieht, sein `snapshot_tx` aber noch vor der Erhöhung von `last_committed_tx` liest und dadurch Daten sieht, die jenseits seines Snapshots liegen.
    - Vollständige Umstellung auf exklusiven Schreib-Lock über den gesamten Reader-Öffnungs-Pfad: Verworfen, um I/O-Operationen (SSTable öffnen) nicht unter Lock zu halten.
*   **Begründung**: MVCC-Snapshot-Isolation erfordert, dass transaktionale Sichtbarkeit atomar oder streng monoton vor der Datensichtbarkeit fortschreitet. Die Aktualisierung von `last_committed_tx` vor `sstables.push()` eliminiert das Race-Fenster für parallele Reader vollständig, ohne Lock-Kontention durch I/O zu erhöhen.

---
