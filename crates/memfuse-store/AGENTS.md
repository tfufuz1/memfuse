# memfuse-store — Agent Context

## 🎯 Crate Purpose
`memfuse-store` ist die LSM-Tree basierte Speicher-Engine. Hier werden WAL (Write-Ahead-Log), MemTables, SSTables und die Compaction implementiert. Die Engine persistiert Dokumenten-Metadaten absolut verlässlich (Zero-Toleranz).

## 🛡️ Critical Invariants
- **[INV-IO-1] Strictly Async**: JEDE Dateisystem-Operation MUSS über `tokio::fs` laufen. Kein blockierendes `std::fs` (verhindert Executor Starvation).
- **[INV-STORE-1] Append-Only WAL**: Das WAL darf nur beschrieben, nie überschrieben werden, bis eine SSTable flusht.
- **[INV-SAFETY-1] Unsafe Limit**: Diese Crate benötigt in der Regel gar kein `unsafe`. Nutze native Tokio- und Standard-Garantien.

## 🔄 TDD Workflow Requirement
Wenn du an SSD/WAL-I/O arbeitest, verwende das `tempfile`-Crate im TDD-Setup.
1. Schreibe einen Test, der einen Crash nach dem Schreiben simuliert und die Daten auskrepiert.
2. Beobachte, dass der Test (ohne deine zukünftige Logik) fehlschlägt.
3. Erst danach implementiere die Recovery-Routine in der WAL-Logik.
