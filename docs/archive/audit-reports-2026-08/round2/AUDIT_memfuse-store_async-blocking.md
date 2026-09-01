# Audit Report: `memfuse-store` Async-Blocking & Executor Starvation Analysis

**Datum:** 31. August 2026
**Crate:** `crates/memfuse-store`
**Auditor:** Senior Rust Async-Runtime Engine (Jules)
**Status:** **DISPROVED / WIDERLEGT (Kein Executor-Starvation-Bug in `memfuse-store`)**

---

## 1. Executive Summary

Die Hypothese aus der Gemini-Analyse besagte:
> *Falls Datei-I/O-Operationen (insbesondere Block-Level-Random-Access auf SSTables) direkt in der Tokio-Async-Runtime ausgeführt werden statt via `tokio::task::spawn_blocking` ausgelagert zu werden, blockiert dies den gesamten Async-Executor (inklusive des MCP-Servers `memfuse-mcp`), was unter I/O-Last zur Reaktionsunfähigkeit des Systems führt.*

### Ergebnis
1. **Hypothese widerlegt**: Das `memfuse-store`-Crate implementiert und hält die in `crates/memfuse-store/src/lib.rs` dokumentierte Invariante strikt ein:
   - Synchroner Block-Level Random-Access via `std::fs::File` und `pread_exact` wird **ausnahmslos** in `tokio::task::spawn_blocking`-Closures ausgeführt.
   - Sequenzielle Schreib- und Lifecycle-Operationen (`SstableBuilder`, `Wal`) verwenden ausschließlich asynchrone `tokio::fs::File`-Primitive (`write_all().await`, `sync_all().await`).
2. **Empirischer Beweis**:
   - Sowohl auf einem **Multi-Worker Runtime (4 Threads)** als auch auf dem **Single-Worker Runtime (`current_thread`)** (dem schärfsten Testfall) führte eine synchrone/schwere SSTable-Datei-Operation (~165 MB I/O) **zu keiner Zeit** zu einer Executor-Starvation.
   - Die Latenz eines parallelen "leichten" Tasks (`tokio::time::sleep(1ms)`) blieb durchgehend nahe der Soll-Dauer: Median (**p50**) **~2.0ms**, **p95 ~2.2ms**, **p99 ~2.5ms**.

---

## 2. Vollständiges `std::fs::File`-Aufruf-Inventar

Nachfolgend ist das vollständige Inventar aller `std::fs::File`-Vorkommen und Datei-I/O-Operationen im `crates/memfuse-store`-Crate aufgeführt:

| Datei:Zeile | Operation | Kontext / Beschreibung | innerhalb `spawn_blocking` | Bewertung |
| :--- | :--- | :--- | :---: | :--- |
| `sstable.rs:624-625` | `std::fs::File::open` | Öffnen der SSTable-Datei beim Initialisieren des `SstableReader` | **Ja** | Compliant |
| `sstable.rs:643-647` | `pread_exact` | Positioniertes Lesen des Trailers (54 Bytes) in `SstableReader` | **Ja** | Compliant |
| `sstable.rs:766-769` | `pread_exact` | Positioniertes Lesen des Bloom-Filters in `SstableReader` | **Ja** | Compliant |
| `sstable.rs:821-823` | `pread_exact` | Positioniertes Lesen des SSTable-Index in `SstableReader` | **Ja** | Compliant |
| `sstable.rs:951-953` | `pread_exact` | Positioniertes Lesen eines Datenblocks in `read_block_at_file` | **Ja** | Compliant |
| `sstable.rs:345` | `tokio::fs::File::create` | Erstellen neuer SSTables in `SstableBuilder` | N/A (Async `tokio::fs`) | Compliant |
| `sstable.rs:427..497` | `file.write_all().await`, `file.sync_all().await` | Schreiben von Blöcken, Index, Bloom-Filter und Trailer | N/A (Async `tokio::fs`) | Compliant |
| `wal.rs:354, 433` | `tokio::fs::OpenOptions` | Öffnen/Erstellen von WAL-Dateien | N/A (Async `tokio::fs`) | Compliant |
| `wal.rs:706..844` | `file.write_all().await`, `file.sync_all().await` | Append & Fsync von WAL-Einträgen | N/A (Async `tokio::fs`) | Compliant |
| `wal.rs:928..1019` | `BufReader::read_exact().await` | Replay & Truncate von WAL-Dateien | N/A (Async `tokio::fs`) | Compliant |
| `wal.rs:2364` | `std::fs::metadata` | Abfrage der Dateirechte im Unit-Test `test_wal_integrity_key` | N/A (In `#[test]`) | Compliant |
| `compaction.rs:173, 211` | `tokio::fs::remove_file` | Entfernen alter SSTables / temporärer Compaction-Dateien | N/A (Async `tokio::fs`) | Compliant |
| `lsm.rs:173..302` | `tokio::fs::create_dir_all`, `tokio::fs::read_dir` | LSM-Ordner-Setup & Startup-Cleanup | N/A (Async `tokio::fs`) | Compliant |
| `util.rs:22` | `tokio::fs::File::open` | Fsync auf Verzeichnis-FD | N/A (Async `tokio::fs`) | Compliant |

**Fazit des Code-Reviews**: 100% aller synchronen `std::fs::File`-Operationen im Produktionscode befinden sich innerhalb von `tokio::task::spawn_blocking`. Es existieren keine verbotenen synchronen Blocking-Aufrufe auf Tokio-Worker-Threads.

---

## 3. Empirischer Testbeweis (Multi-Worker Runtime)

Der Test misst die Ausführungslatenz eines leichtgewichtigen Überwachungstasks (`tokio::time::sleep(Duration::from_millis(1))`), während über die öffentliche API von `memfuse-store` eine massive I/O-Last erzeugt wird (~165 MB SSTable Writes & Reads).

### Test-Konfiguration (Multi-Worker)
- Runtime: `#[tokio::test(flavor = "multi_thread", worker_threads = 4)]`
- Data Load: 20,000 Einträge à 8,192 Bytes Payload (~165 MB SSTable)
- Read Phase: Cache-Bypassed Random Lookups über `SstableReader::get`

### Messwerte (Multi-Worker)
- **Gesamte Monitor-Proben**: 2,132 Samples
- **Schreib-Phase Dauer**: 3.91s (~165 MB geschrieben & fsynced)
- **Lese-Phase Dauer**: 688.57ms (Random Access via `spawn_blocking`)
- **Median Latenz (p50)**: `2.183 ms`
- **95th Percentile (p95)**: `2.295 ms`
- **99th Percentile (p99)**: `2.413 ms`
- **99.9th Percentile (p99.9)**: `15.568 ms`
- **Maximale Einzellatenz**: `31.252 ms`

---

## 4. Empirischer Testbeweis (Single-Worker Runtime, Schärfster Test)

Um eine mögliche Maskierung von Blocking-Effekten durch freie Parallel-Worker in Multi-Thread-Runtimes auszuschließen, wurde derselbe Test in einer Single-Threaded Tokio Runtime ausgeführt. Falls synchrone I/O auf dem Worker-Thread liefe, würde der Single-Worker während der 3–4 Sekunden langen I/O-Phase vollständig einfrieren (Latenz = 3000–4000 ms).

### Test-Konfiguration (Single-Worker)
- Runtime: `#[tokio::test(flavor = "current_thread")]`
- Data Load: 20,000 Einträge à 8,192 Bytes Payload (~165 MB SSTable)

### Messwerte (Single-Worker)
- **Gesamte Monitor-Proben**: 2,263 Samples
- **Schreib-Phase Dauer**: 3.66s
- **Lese-Phase Dauer**: 887.09ms
- **Median Latenz (p50)**: `2.016 ms`
- **95th Percentile (p95)**: `2.246 ms`
- **99th Percentile (p99)**: `2.614 ms`
- **99.9th Percentile (p99.9)**: `8.428 ms`
- **Maximale Einzellatenz**: `15.535 ms`

**Ergebnis**: Auch auf einem einzelnen Worker-Thread läuft der leichte Task ungestört im 2ms-Intervall weiter. Starvation ist empirisch widerlegt.

---

## 5. Ursachen-Isolation & Diskurs

Die von Gemini aufgeworfene Vermutung einer Executor-Starvation trifft auf `memfuse-store` nicht zu.
1. **Architektur-Einhaltung**: `memfuse-store` trennt sauber zwischen:
   - Async Schreibpfad (`tokio::fs` für unbuffered Writes & Fsync)
   - Blocking Lesepfad (`spawn_blocking` + OS `pread_exact` für thread-safe Random Access auf unveränderliche SSTables)
2. **Blocking Isolation**: Tokio lagerte alle `pread_exact`-Aufrufe korrekt in den internen `blocking`-Threadpool aus. Der Haupt-Event-Loop blieb für den leichten Task zu jedem Zeitpunkt frei.

---

## 6. Priorisierte Bugliste

| ID | Komponente | Beschreibung | Priorität | Status |
| :--- | :--- | :--- | :---: | :---: |
| - | `memfuse-store` | *Keine Blocking-Bugs identifiziert.* | - | **RESOLVED / NOT A BUG** |

---

## 7. Anhang: Zeitreihen-Rohlogs & Benchmark-Integration

Der empirische Test ist fest integriert in die Testsuite von `memfuse-store`:
- **Testdatei**: `crates/memfuse-store/tests/executor_starvation_test.rs`
- **Ausführung**: `cargo test -p memfuse-store --test executor_starvation_test -- --nocapture`

### Auszug aus den Test-Outputs
```text
=======================================================
RUNNING TEST: Single-Worker Runtime (current_thread)
Configuration: 20000 entries, 8192 bytes payload (~156 MB total)
=======================================================
[Single-Worker Runtime (current_thread)] Starting Heavy Write Phase...
[Single-Worker Runtime (current_thread)] Write Phase Finished in 3.66s. File size: 165359894 bytes
[Single-Worker Runtime (current_thread)] Starting Heavy Read Phase (Cache Bypassed)...
[Single-Worker Runtime (current_thread)] Read Phase Finished in 887.09ms

--- [Single-Worker Runtime (current_thread)] RESULTS ---
Total Monitor Samples: 2263
Max Light Task Latency: 15.535ms (Expected: ~1.0ms)
p50: 2.016ms
p95: 2.246ms
p99: 2.614ms
p99.9: 8.428ms
Max: 15.535ms
test test_executor_starvation_single_thread ... ok
```
