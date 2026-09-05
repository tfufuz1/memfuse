# Hebel 5: Chaos Engineering & Crash-Resilienz (aus Project Chimera SPEC-035)

## 1. Ausgangslage & Optimierungspotenzial für MemFuse
MemFuse setzt auf ein robustes Write-Ahead-Log (WAL V3) mit CRC32-Prüfsummen und MVCC-Snapshot-Isolation.
In Produktionssystemen und Multi-Agenten-Umgebungen treten jedoch extreme Randfälle auf:
- Stromausfall oder plötzliches `kill -9` während eines WAL-Writes ("Torn Write").
- Beschädigung einzelner Sektoren auf NVMe/SSD ("Bit-Flips").
- Plötzlicher Abbruch von Tokio-Tasks bei asynchronen Writes.
- Unerwartetes Memory-Limit (OOM).

**Project Chimera** hat in `crates/chimera-chaos` (SPEC-035) eine hochmoderne **Chaos Engineering Pipeline** implementiert, die das System systematisch unter unkontrollierten Fehlerszenarien stresst und beweist, dass es **ohne Datenverlust oder Inkonsistenz** wiederhergestellt werden kann.

## 2. Extrahierte Komponenten

| Datei | Quelle | Beschreibung |
|:---|:---|:---|
| [`chaos_engine.rs`](./chaos_engine.rs) | `chimera-chaos/src/lib.rs` | Vollständige Chaos-Engine mit 10 deterministischen Fehlerszenarien |
| [`SPEC-035_chaos_engineering.md`](./SPEC-035_chaos_engineering.md) | `docs/specs/SPEC-035_chaos_engineering.md` | Formale Spezifikation des Chaos-Testing-Frameworks |
| [`memfuse_chaos_test.rs`](./memfuse_chaos_test.rs) | Neu erstellt | Angepasste Test-Suite zur Überprüfung von MemFuse WAL V3 und MVCC |

## 3. Die 10 Chaos-Szenarien
1. `TaskMassacre`: Bricht zufällige Tokio-Worker-Threads während aktiver Transaktionen ab.
2. `BitFlipInjection`: Verändert gezielt Bytes in WAL-Segmenten und SSTables zur Validierung der CRC32C-Erkennung.
3. `PowerCutSimulation`: Simuliert harten Prozess-Crash mitten im Schreibzyklus.
4. `TruncatedWALFile`: Kürzt die letzte WAL-Datei ab, um zu prüfen, ob der Replay bis zum letzten validen Commit intakt bleibt.
5. `MemoryExhaustion`: Flutet den RAM, um die Drosselungs- und Reject-Logik von SPEC-025 zu erzwingen.
6. `RogueAgentFlood`: Simuliert einen Amok laufenden Agenten mit tausenden Schreibanfragen pro Sekunde.
7. `DroppedWrite`: Simuliert plötzliche EIO-Fehler des Betriebssystems.
8. `IOLatency`: Simuliert Festplatten-Latenz-Spikes bis zu 2000 ms.
9. `NetworkDegradation`: Simuliert Paketverluste auf RPC-Kanälen.
10. `OOMGuardTriggered`: Löst gezielt Speicherschutz-Interrupts aus.
