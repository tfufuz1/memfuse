# SPEC-20260527-HIGH-001-WAL-CRC

## 🎯 1. Das Ziel (Context & "Why")
Implementiert eine robuste CRC32-Validierung während des WAL-Replays, um Datenkorruption frühzeitig zu erkennen und partielle Writes (Crash während Flush) am Dateiende sicher abzufangen.

---

## 🛡️ 2. Die Invariante(n) (The "Law")
- **[INV-WAL-CRC-1]**: Kein WAL-Eintrag darf ohne verifizierte CRC32-Checksumme in die MemTable übernommen werden.
- **[INV-WAL-RECOVERY-1]**: Korrupte Daten am Ende der WAL-Datei (Tails) dürfen den Datenbank-Start nicht verhindern, sondern müssen geloggt und verworfen werden (Recovery bis zum letzten validen Zustand).
- **[INV-WAL-RECOVERY-2]**: Korrupte Daten in der MITTE der WAL-Datei müssen zu einem harten Fehler führen, da dies auf ernsthafte Hardware- oder Softwarefehler hindeutet.

---

## 📍 3. Speicherort & API-Signatur
- **Crate**: `memfuse-store`
- **File**: `src/wal.rs`

Die bestehende `replay`-Methode wird dahingehend erweitert, dass sie zwischen "EOF-nahen Fehlern" und "In-Stream Fehlern" unterscheidet.

---

## 🛑 4. Definiertes Fehlerverhalten (Fail-Cases)
- CRC Mismatch im Stream -> `Err(MemFuseError::WalCorruption)`
- CRC Mismatch am Dateiende (unvollständiger Block) -> `tracing::warn!` + `break` (Replay erfolgreich abschließen)
- Ungültige HMAC-Kette -> `tracing::warn!` + `break` (Invariante aus WP-1.3)

---

## ✅ 5. Der TDD Checkpoint (Red-Phase Vorgabe)
- **Test 1 (`test_wal_crc_middle_corruption`)**: Schreibt 3 Einträge, korrumpiert den 2. Eintrag, erwartet `Err(MemFuseError::WalCorruption)`.
- **Test 2 (`test_wal_crc_tail_corruption`)**: Schreibt 2 Einträge, hängt 10 Bytes Müll an, erwartet 2 valide Einträge ohne Error.
