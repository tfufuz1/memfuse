# SPEC-20260607-TracingCoverage

## 🎯 1. Das Ziel (Context & "Why")
Vollständige Transparenz über System-Operationen im Feld durch flächendeckende OpenTelemetry-Abdeckung in der Facade-Layer. Behebung von "Blackbox"-Verhalten bei Performance-Problemen.

---

## 🛡️ 2. Die Invariante(n) (The "Law")
- **[INV-DB-TRACE-01]**: Alle öffentlichen asynchronen Methoden in `MemFuse` und `Collection` MÜSSEN mit `#[tracing::instrument(level = "trace", skip(self, ...))]` annotiert sein.
- **[INV-DB-TRACE-02]**: Sensitive Daten (wie Passphrasen) MÜSSEN explizit von der Instrumentierung ausgeschlossen werden (`skip`).

---

## 📍 3. Speicherort & API-Signatur
- **Crate**: `memfuse-db`
- **Files**: `src/lib.rs`, `src/collection.rs`

---

## 🛑 4. Definiertes Fehlerverhalten (Fail-Cases)
- N/A (Tracing beeinflusst die Logik nicht).

---

## ✅ 5. Der TDD Checkpoint (Red-Phase Vorgabe)
- Da Tracing schwer via Unit-Test zu erzwingen ist (ohne komplexe Subscriber-Setups), wird die Einhaltung via Code-Review (Grep/Audit) sichergestellt.
- Wir prüfen stichprobenartig, ob `insert`, `search` und `query` in `Collection` nun `level = "trace"` besitzen.
