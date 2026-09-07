# ADR-012: Invarianten-Spannungsfeld — std::fs innerhalb spawn_blocking vs. Pure Async-I/O

*   **Datum**: 2026-08-23
*   **Status**: ✅ Final
*   **Entscheidung**: Die Modul-Dokumentation von `memfuse-store/src/lib.rs` behauptet "Alle Disk-I/O via tokio::fs (zero std::fs imports)". Jedoch verwenden `SstableReader` und `SstableBuilder` `std::fs::File` innerhalb von `tokio::task::spawn_blocking`.
*   **Alternativen**:
    - **Option A (Empfohlen)**: Doku und `docs/ARCHITECTURE.md` anpassen zu: *"tokio::fs für alle Metadaten- und Lifecycle-Operationen; std::fs::File ausschließlich innerhalb von spawn_blocking für Performanz-kritische Block-Level Random-Access Reads/Writes."*
    - **Option B**: Code vollständig auf `tokio::fs::File` refactoren (bringt Wrapper-Overhead bei wahlfreien Block-Zugriffen mit sich).
*   **Begründung**: Option A wahrt die maximale Lese-/Schreibperformanz von SSTables auf NVMe-Speichern, ohne Async-Executoren zu blockieren (da `spawn_blocking` dedizierte Worker-Threads nutzt). Option B verringert die Komplexität der Invarianten-Aussage auf Kosten von Latenz.
*   **Eskalation**: Entscheidung erfordert Freigabe durch den Entwickler (ASK-FIRST Tier).

---
