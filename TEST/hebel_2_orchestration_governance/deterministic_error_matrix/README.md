# Deterministische Fehler-Matrix für MemFuse

## 1. Motivation & Synergie
In MemFuse existiert bereits `MemFuseErrorDto` (`crates/memfuse-core/src/error_dto.rs`), welches über Tauri IPC und PyO3 FFI übertragen wird. Bislang fehlte jedoch eine **semantische Handlungsanweisung** für aufrufende Systeme:
- Soll Tauri bei einem Fehler einen automatischen Retry ausführen?
- Soll ein Python-Agent die Strategie mutieren oder sofort abbrechen?
- Wie werden System-Invarianten von harmlosen Eingabefehlern unterschieden?

Die **Structured Process Orchestration (SPO) Spec** liefert die deterministische Antwort: Eine Klassifikation aller Fehler in 4 orthogonale Klassen mit strikt vorgegebenen automatischen Reaktionspfaden.

## 2. Die 4 Fehlerklassen

| Klasse | HTTP/IPC Status | Automatische Aktion | MemFuse Beispiele |
|:---|:---|:---|:---|
| **TRANSIENT** | 429, 503, 504 | `RETRY_EXPONENTIAL_BACKOFF` (500ms, 1000ms, 2000ms, max 3) | `TransactionTimeout`, `SandboxTimeout`, `Conflict` |
| **LOGICAL** | 400, 404, 422 | `ABORT_CURRENT_APPROACH` (Prompt-Strategie mutieren, Spec neu laden) | `InvalidInput`, `NotFound`, `EmbeddingDimensionMismatch` |
| **FATAL** | 500 | `ABORT_AND_ESCALATE` (Incident anlegen, State Rollback, Mensch alarmieren) | `WalCorruption`, `ChecksumMismatch`, `Storage`, `Io` |
| **ARCHITECTURAL** | 403, 409, 507 | `HALT_ALL_DEPENDENT_AGENTS` (Output unter Quarantäne, alle Agenten stoppen) | `PolicyViolation`, `MemoryBudgetExceeded`, `HnswConnectivityDegraded` |

## 3. Enthaltene Dateien
- [`error_matrix.yaml`](./error_matrix.yaml): Deklarative YAML-Spezifikation der Fehler-Matrix.
- [`error_matrix_mapper.rs`](./error_matrix_mapper.rs): Einsatzbereites Rust-Modul zur Integration in `memfuse-core` oder `memfuse-tauri`.
