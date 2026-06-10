# SPEC-20260607-wal-header-resilience

## 🎯 1. Das Ziel (Context & "Why")
Sicherstellen, dass der WAL-Parser in `memfuse-store` extrem robust gegen Header-Korruption ist und bei Bit-Fehlern in den ersten 12 Bytes eines Eintrags NIEMALS eine Panic auslöst, sondern kontrolliert Fehler propagiert.

---

## 🛡️ 2. Die Invariante(n) (The "Law")
- **[INV-STO-001]**: Jede bitweise Manipulation der ersten 12 Bytes (`length` [4b], `crc` [4b], `seq_no` [Teil, 4b]) eines serialisierten `WalEntry` muss bei `WalEntry::from_bytes` zu einem `Err(MemFuseError::Serialization(_))` oder `Err(MemFuseError::WalCorruption { .. })` führen.
- **[INV-STO-002]**: Eine Panic (uncontrolled crash) ist bei beliebig korrupten Eingangsdaten strikt untersagt.

---

## 📍 3. Speicherort & API-Signatur
- **Crate**: `memfuse-store`
- **File**: `src/wal.rs`

---

## 🛑 4. Definiertes Fehlerverhalten (Fail-Cases)
- CRC Mismatch -> `Err(MemFuseError::Serialization(format!("CRC mismatch...")))`
- Ungültige Header-Werte (z.B. Länge zu groß) -> `Err(MemFuseError::WalCorruption { .. })` oder `Err(MemFuseError::Storage(_))`

---

## ✅ 5. Der TDD Checkpoint (Red-Phase Vorgabe)
- Erstelle einen validen `WalEntry`, serialisiere ihn zu Bytes.
- Iteriere bitweise durch die ersten 12 Bytes.
- Flip jeweils 1 Bit, rufe `WalEntry::from_bytes` auf und assert `is_err()`.
- Assert am Ende jeder Iteration, dass keine Panic aufgetreten ist.
