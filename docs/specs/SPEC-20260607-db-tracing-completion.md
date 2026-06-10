# SPEC-20260607-db-tracing-completion

## 🎯 1. Das Ziel (Context & "Why")
Lückenlose Observierbarkeit der Datenbank-Operationen sicherstellen, um im Fehlerfall im Feld eine präzise Ursachenanalyse (Root Cause Analysis) ohne Blackbox-Effekte zu ermöglichen.

---

## 🛡️ 2. Die Invariante(n) (The "Law")
- **[INV-DB-001]**: Jede öffentliche asynchrone Methode in `MemFuse` und `Collection` muss mit dem `#[tracing::instrument]` Makro versehen sein.
- **[INV-DB-002]**: Das Log-Level muss `trace` sein, und `self` sollte mittels `skip(self)` ignoriert werden, um Rauschen zu minimieren.

---

## 📍 3. Speicherort & API-Signatur
- **Crate**: `memfuse-db`
- **File**: `src/lib.rs`, `src/collection.rs`

---

## 🛑 4. Definiertes Fehlerverhalten (Fail-Cases)
- N/A (Rein beobachtende Änderung)

---

## ✅ 5. Der TDD Checkpoint (Red-Phase Vorgabe)
- Da dies eine deklarative Makro-Änderung ist, wird die Korrektheit primär durch statische Code-Analyse und Review verifiziert. Optional kann ein Test prüfen, ob Spans bei Aufruf generiert werden.
