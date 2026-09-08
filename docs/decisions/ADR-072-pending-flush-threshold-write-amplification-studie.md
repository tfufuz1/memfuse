# ADR-068: Studie zur DiskANN PENDING_FLUSH_THRESHOLD Write-Amplification und Empfehlung für adaptiven Schwellenwert

* **Status:** Akzeptiert (Empfehlung normativ festgehalten, Implementierung folgt in separatem Task)
* **Datum:** 2026-09-07
* **Anforderung / Referenz:** Gesamtspezifikation v7.0 §4.2, Technische-Schulden-Dokument A.9

## Kontext & Problemstellung

In `crates/memfuse-index/src/diskann.rs` legt die Konstante `PENDING_FLUSH_THRESHOLD: u64 = 50` fest, nach wie vielen uncommitted Vektoreinfügungen im WAL/RAM automatisch ein DiskANN `persist_delta()` ausgelöst wird. Dieser Wert wurde ohne begleitenden ADR von einem früheren Wert (1.000) auf 50 gesenkt (Faktor 20 häufigeres Background-Persist bei kleinen Collections).

Die Auswirkung dieser Frequenzänderung auf die Schreibverstärkung (Write-Amplification) und die I/O-Belastung von NVMe/SSD-Speichermedien war bislang undokumentiert und unquantifiziert, was eine Dokumentationslücke gemäß v7.0 §4.2 und Technischen Schulden A.9 darstellte.

## Messmethodik & Empirische Ergebnisse

Über den dedizierten Benchmark `crates/memfuse-index/benches/flush_threshold_amplification.rs` wurden DiskANN-Collections der Größen $N \in \{100, 1.000, 10.000, 100.000\}$ mit Insert-Workloads unter Schwellenwerten $T \in \{50, 200, 1.000\}$ vermessen. Die Ergebnisse sind in `crates/memfuse-index/benches/results/flush_threshold_amplification.md` abgelegt.

### Wichtigste Messergebnisse:
1. **Write Amplification (WA) skaliert direkt proportional zur Collection-Größe $N$ und umgekehrt proportional zum Threshold $T$:**
   - Bei $N=100.000$ führt ein statischer Threshold von $T=50$ zu einer exzessiven Write Amplification von **32.025x** (781,87 MB Festplattenschreiben für 0,02 MB Vektordaten).
   - Eine Erhöhung des Schwellenwerts auf $T=200$ bzw. $T=1.000$ senkt das Schreibvolumen um den Faktor **4,0x bis 20,0x**.
2. **Latenz-Verhalten (p95):**
   - Die p95-Latenz einzelner Inserts wird primär vom WAL-`fsync()` dominiert (~7,1 ms bis 7,7 ms).
   - Bei kleinen Thresholds ($T=50$) erzeugen extrem häufige Hintergrund-Flushes permanente I/O-Konkurrenz und Dateisystem-Renames, was auf I/O-begrenzten Systemen zu Latenzspitzen führt.

## Entscheidung & Normative Empfehlung

Auf Basis der Messdaten lehnen wir einen rein statischen Schwellenwert (weder fest 50 noch fest 1.000) ab und beschließen normativ die Einführung eines **adaptiven, von der Collection-Größe $N$ abhängigen Flush-Thresholds**:

$$\text{PENDING\_FLUSH\_THRESHOLD}(N) = \max\left(50, \min\left(1.000, \left\lfloor N \times 0,05 \right\rfloor\right)\right)$$

### Stufenregelung:
1. **Kleine Collections ($N \le 1.000$):** Threshold = **50**
   - Garantiert minimale Uncommitted-WAL-Länge, schnelle Crash-Recovery und minimale Sichtbarkeitsverzögerung bei geringem absolutem Schreibvolumen.
2. **Mittlere Collections ($N = 10.000$):** Threshold = **500**
   - Reduziert die Write-Amplification von 3.241x auf 324x bei weiterhin überschaubarem Recovery-Fenster.
3. **Große Collections ($N \ge 20.000$):** Threshold = **1.000**
   - Deckelt die Write-Amplification bei großen Vektormengen und schont SSD-/NVMe-Speichermedien vor I/O-Sättigung.

*Hinweis:* Die eigentliche Implementierung der adaptiven Funktion in `crates/memfuse-index/src/diskann.rs` ist bewusst Gegenstand eines separaten Folge-Tasks mit eigenem Code-Review.

## Konsequenzen & Dokumentationsabschluss

- **Schließung der Dokumentationslücke:** Erfüllt die Anforderungen aus Gesamtspezifikation v7.0 §4.2 und beseitigt Technische Schulden A.9.
- **Nachvollziehbarkeit:** Der Benchmark `cargo bench -p memfuse-index --bench flush_threshold_amplification --features experimental-diskann` steht als reproduzierbare Messgrundlage im Repository bereit.
