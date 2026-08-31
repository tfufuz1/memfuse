# AUDIT REPORT: `memfuse-mcp` Slowloris-artiger Verbindungsaufbau über stdio

**Datum:** 31. August 2026
**Crate:** `crates/memfuse-mcp`
**Auditor:** Senior Rust Stdio & Security Engineer (Jules)
**Status:** **VERIFIED & DOCUMENTED / PASSED WITH ARCHITECTURAL ANALYSIS**
**Referenzierte Architekturvorgabe:** ADR-010 (Exklusiver stdio Transport, kein TCP/HTTP)

---

## 1. Executive Summary & Zielsetzung

In Runde 1 wurden Flood- und Binärdaten-Angriffe auf das `memfuse-mcp`-Crate erfolgreich geprüft. In Runde 2 wurde ergänzend untersucht, wie sich der MCP-Server gegenüber einem **"Slowloris"-artigen Angriff über stdio** verhält, bei dem ein Client Request-Bytes extrem langsam (z. B. ein Byte alle 100 ms über mehrere Minuten) anstelle von einmaligen oder schnellen Bursts sendet.

### Zentrale Fragestellungen & Audit-Ergebnisse

1. **Bindet ein partieller slow-byte Stream dauerhaft Ressourcen (Buffer, Task) ohne Fortschritt?**
   - **Speicherressourcen (Buffer)**: **Nein, nicht unbegrenzt.** Der Speicheraufbau ist durch die Konstante `MAX_RPC_BYTES = 16 MB` in `read_line_bounded` strikt auf Byte-Ebene gedeckelt. Wenn ein Angreifer eine Zeile ohne Zeilenumbruch (`\n`) aufbläht, wird der Stream bei exakt 16.777.216 Bytes hart mit `std::io::ErrorKind::InvalidData` abgebrochen und der Puffer verworfen.
   - **CPU / Execution Threads**: **Nein.** `read_line_bounded` verwendet asynchrones `reader.fill_buf().await`. Wenn keine neuen Bytes auf `stdin` anliegen, gibt der Tokio Task den Worker Thread sofort an den Tokio Reactor frei. Es entsteht keinerlei CPU-Spinning oder Blocking von Worker Threads.
   - **Task- / Connection-Lebensdauer**: **Ja.** Der Tokio Async Task für den Stdio-Loop bleibt aufrecht, solange `stdin` geöffnet ist und der Client in beliebig langen Abständen vereinzelte Bytes nachschiebt.

2. **Existiert ein Inaktivitäts-Timeout, der solche hängenden Verbindungen abbricht?**
   - **Nein.** Im aktuellen Produktionscode von `memfuse-mcp` (`run_stdio` / `read_line_bounded`) existiert standardmäßig **kein** Inaktivitäts-Timeout (kein `tokio::time::timeout` um das Lesen einer einzelnen Zeile).
   - Eine partiell gesendete Zeile verbleibt im Zustand *"Waiting for line completion"*, bis entweder das Zeilenende (`\n`) empfangen wird oder die Gegenseite den Pipe-Stream schließt (EOF).

---

## 2. Technische Code-Analyse (`read_line_bounded`)

Der Einlesepfad für den stdio JSON-RPC Transport ist in `crates/memfuse-mcp/src/lib.rs` wie folgt implementiert:

```rust
pub async fn read_line_bounded<R: tokio::io::AsyncBufRead + Unpin>(
    reader: &mut R,
    buf: &mut String,
    max_bytes: usize,
) -> std::io::Result<usize> {
    buf.clear();
    let mut raw_bytes = Vec::new();

    loop {
        let available = reader.fill_buf().await?; // Yielded an Tokio-Reactor, wenn keine Bytes anliegen
        if available.is_empty() {
            break;
        }
        ...
        if raw_bytes.len() + used > max_bytes {
            // Hartes Limit: Verwirft überlange Streams
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Message size limit exceeded ({max_bytes} bytes limit)"),
            ));
        }
        ...
    }
}
```

### Ressourcen-Verhaltensmatrix unter Slowloris-Bedingungen

| Ressource | Verhalten bei 1 Byte / 100 ms | Schutzmechanismus / Bewertung |
| :--- | :--- | :--- |
| **Worker Threads (CPU)** | 0% Last während der Wartezeit zwischen Bytes | **Sicher**: `fill_buf().await` suspendiert den Task asynchron |
| **Arbeitsspeicher (RAM)** | Wächst maximal auf 16 MB pro unvollständiger Zeile | **Sicher**: Harte Deckelung via `MAX_RPC_BYTES` |
| **File Descriptors (FD)** | 1 FD (`stdin`) verbleibt belegt | **Sicher im Kontext von ADR-010** (1 Server-Instanz pro Subprozess) |
| **Inaktivitäts-Timeout** | Nicht vorhanden (Wartet unbegrenzt auf `\n` oder EOF) | **Architekturbefund**: Kein Read-Timeout auf Stdio-Ebene |

---

## 3. Empirischer Testbeweis (`test_slowloris_stdio_attack_simulation`)

In `crates/memfuse-mcp/tests/mcp_test.rs` wurde ein Integrationstest implementiert, der eine echte `memfuse-mcp-server` Binary als Subprozess startet und ein Slowloris-Angriffsszenario über Pipes simuliert.

### Testablauf
1. **Slow-Send Phase**: Versenden der ersten 30 Bytes eines JSON-RPC Requests im Abstand von 50 ms pro Byte (Gesamtdauer > 1.4 Sekunden).
2. **Statusprüfung**: Verifikation, dass der Server-Prozess während des langsamen Byte-Streams aktiv bleibt und nicht abgestürzt oder ungerechtfertigt terminiert ist.
3. **Completion Phase**: Senden der verbleibenden Bytes inklusive `\n`.
4. **Antwortprüfung**: Der Server liest die vervollständigte Zeile erfolgreich und antwortet innerhalb von Millisekunden mit der korrekten JSON-RPC Response (`ping`).

### Testausführung & Ergebnis
```text
running 1 test
test test_slowloris_stdio_attack_simulation ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 17 filtered out; finished in 1.58s
```

---

## 4. Bedrohungsanalyse im Kontext von ADR-010

Die Auswirkung des Fehlens eines Inaktivitäts-Timeouts unterscheidet sich grundlegend zwischen HTTP/TCP-Webservern und einem stdio-basierten IPC Server:

1. **ADR-010 Constraint**: `memfuse-mcp` kommuniziert ausschließlich über standard input/output (`stdin`/`stdout`) als Subprozess eines Mutterprozesses (z. B. Claude Desktop). Es gibt **keine Netzwerk-Sockets, Ports oder axum/HTTP-Endpoints**.
2. **Kanal-Exklusivität**: Bei stdio existiert pro Server-Prozess exakt ein einziger Client (die Standard-Input Pipe). Ein entfernter Angreifer kann nicht über das Netzwerk tausende parallele hängende Verbindungen aufbauen, um Sockets oder Thread-Pools zu erschöpfen.
3. **Lebenszyklus-Kopplung**: Wenn der Mutterprozess beendet wird oder abstürzt, wird die `stdin`-Pipe geschlossen (`EOF`), woraufhin `read_line_bounded` `Ok(0)` zurückgibt und der `run_stdio`-Loop geordnet beendet wird.

---

## 5. Empfehlungen & Best Practices

Obwohl Slowloris über stdio aufgrund von ADR-010 kein kritisches Remotesicherheitsrisiko darstellt, werden folgende Härtungsmaßnahmen empfohlen:

1. **Optionales Read-Inactivity-Timeout**:
   Falls in zukünftigen Szenarien hängende Parent-Prozesse automatisch abgefangen werden sollen, kann `read_line_bounded` um ein konfigurierbares Lese-Timeout (z. B. 60 Sekunden Inaktivität zwischen Bytes) mittels `tokio::time::timeout` ergänzt werden.
2. **Beibehalten des 16 MB Limits**:
   Das bestehende `MAX_RPC_BYTES`-Limit bietet effektiven Schutz gegen Memory-Exhaustion-Angriffe und muss weiterhin strikt durchgesetzt werden.

---

## 6. Fazit

- **Ressourcenbindung**: Partieller Slowloris-Datentransfer bindet **weder CPU noch unbegrenzt RAM**. Die maximale Speicherobergrenze von 16 MB verhindert OOM-Attacken.
- **Inaktivitäts-Timeout**: Ein solches Timeout existiert aktuell nicht auf stdio, was im Rahmen des lokalen Subprozess-Modells (ADR-010) konform und unkritisch ist.
