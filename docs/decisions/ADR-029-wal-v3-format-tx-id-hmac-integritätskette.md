# ADR-029: WAL-V3 Format & tx_id HMAC-Integritätskette


*   **Datum**: 2026-08-29
*   **Status**: ✅ Final
*   **Kontext**:
    In `WalEntry::compute_checksum` wurde `tx_id` bisher ignoriert. Ein Angreifer mit Dateisystemzugriff konnte `tx_id` manipulieren, während die HMAC-Kette valide blieb. Beim Replay erhielt die Transaktion eine falsche ID, was die Kausalordnung gestört hätte.
*   **Entscheidung**:
    1. Einführung des WAL-Formats V3 mit Header `b"MFW3"` (`WAL_V3_HEADER`) und `WalVersion::V3`.
    2. Die HMAC-Berechnung in `compute_checksum_v3` bindet `tx_id` (vor `op_type`) sowie Längen-Präfixe `u32` für `key` und `value` ein, um HMAC-Längen-Extension-Angriffe und `tx_id`-Tampering strukturell zu verhindern.
    3. `Wal::try_new` und `append_batch` erzeugen ausnahmslos WAL V3 Dateien.
    4. Version-aware `replay()` validiert V1, V2 und V3 Formate abwärtskompatibel. Beim Öffnen einer V1/V2-Datei wird nach erfolgreichem Replay automatisch eine transparente Migration/Rewrite zu V3 durchgeführt.
*   **Alternativen**:
    - Belassen von V2 und Vertrauen auf Dateisystem-Rechte: Verworfen, da dies das Zero-Trust/Cryptographic-Integrity-Gebot von MemFuse verletzt.
*   **Begründung**:
    Stellt sicher, dass WAL-Einträge nicht nur bzgl. `seq_no` und Key/Value fälschungssicher sind, sondern auch die Kausalordnung der Transaktions-IDs (`tx_id`) kryptographisch authentifiziert ist.
*   **Konsequenzen**:
    - Neue WAL-Dateien nutzen `MFW3`.
    - Vollständige Abwärtskompatibilität und automatische In-Place-Migration für Alt-WALs.
