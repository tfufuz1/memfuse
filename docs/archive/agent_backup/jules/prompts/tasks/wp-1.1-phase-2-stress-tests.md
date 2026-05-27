# Task: WP-1.1 Compaction Stress Tests

## Kontext
Die Compaction-Logik ist implementiert, muss aber unter Last bewiesen werden.

## Aufgaben
1. **Schreibe Stress-Tests**:
   - Erstelle `test_heavy_write_load_with_background_compaction`.
   - Simuliere 10.000 Inserts bei gleichzeitigem Compaction-Intervall von 2 SSTables.
2. **Race-Condition Check**:
   - Verwende `tokio::time::sleep` und `tokio::spawn` um Race-Conditions beim Datei-Rename zu provozieren.
3. **Data Integrity**:
   - Verifiziere per Checksumme (oder Full-Scan), dass keine Daten während des SSTable-Merges verloren gehen.

## Erwartetes Ergebnis
Alle Stress-Tests bestehen 3x hintereinander.
PR-Titel: `test(store): WP-1.1 compaction stress and race-condition tests`
