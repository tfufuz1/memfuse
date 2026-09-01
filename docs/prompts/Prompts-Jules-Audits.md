# MemFuse — Google-Jules Crate-Audit-Prompts

**Repository:** `https://github.com/tfufuz1/memfuse`
**Zweck dieses Dokuments:** Für jedes der 15 aktiven Workspace-Crates einen eigenständigen, maximal ausführlichen Experten-Prompt bereitzustellen, den Google-Jules in seiner Cloud-VM ausführt, um das jeweilige Crate vollständig zu bauen, zu testen, zu verifizieren, zu benchmarken und darüber einen extrem detaillierten Audit-Report zu erzeugen.

**Basis der Analyse:** Klon des Repos, Auswertung von `Cargo.toml` (Workspace mit 15 Crates + `xtask`), `README.md`, `DECISIONS.md` (ADR-001 bis ADR-018+), `TESTING.md` (Anti-Mirroring-Prinzip, Pflicht-Testkategorien, Mutation-Testing-Pflicht), sowie der Modulstruktur und `lib.rs`-Header (FILE-CONTEXT-Blöcke) jedes Crates.

---

## Wie dieses Dokument zu benutzen ist

1. Jeder der 15 Abschnitte unten enthält **einen vollständigen, in sich geschlossenen Prompt**, der 1:1 als Jules-Task-Beschreibung kopiert werden kann.
2. Die Prompts sind bewusst redundant zueinander in Struktur und Formalismus (Rolle → Mission → Kontext → Aufgabenumfang → Methodik → Report-Struktur → Abnahmekriterien), damit jeder Prompt **unabhängig** vom Kontext der anderen ausführbar ist — Jules erhält pro Lauf nur einen dieser Prompts.
3. Reihenfolge folgt der Architektur-Layer-Struktur (Layer 0 → Layer 4), da spätere Crates von früheren abhängen — bei sequenzieller Abarbeitung ist das die sinnvollste Reihenfolge.
4. Alle Prompts fordern **ausschließlich empirisch erzeugte Ergebnisse** (echte `cargo test`/`cargo bench`/`cargo llvm-cov`/`cargo miri`/`cargo clippy`-Läufe in der Jules-VM) — keine geschätzten oder halluzinierten Zahlen.

---

## Globaler Kontext, der in JEDEN Prompt eingebettet ist

MemFuse Brain ist ein **Pure-Rust, air-gapped Cognitive Operating System** für lokale KI-Agenten: eine 4-Signal-Hybridsuche-Engine (Vektor/HNSW + Volltext/BM25 + Wissensgraph/CSR + Metadaten-Filter, fusioniert via Reciprocal Rank Fusion), mit LSM-Tree-Persistenz, AES-256-GCM-SIV-Verschlüsselung, Contextual-Retrieval-Chunking, Cross-Encoder-Reranking, einer Multi-Step-Query-Engine, einem MCP-Server (stdio JSON-RPC), einer Tauri-Desktop-App und einer persistenten Agent-Workflow-Engine (checkpoint → execute → commit → audit). Das Projekt folgt einem strikten 5-Schichten-DAG (Layer 0–4) ohne Aufwärts-Importe, verfolgt eine "Sovereign Core Doctrine" (kein `unsafe` außer explizit dokumentierten Ausnahmen), und hat ein eigenes Test-Manifest (`TESTING.md`) mit einem **Anti-Mirroring-Prinzip**: Testerwartungswerte dürfen niemals mit derselben Formel wie die Implementierung berechnet werden, sondern müssen unabhängig (handberechnet, extern verifiziert, oder aus einer Referenzimplementierung) stammen.

---

# 1. `memfuse-core` (Layer 0 — Fundament)

```
ROLLE
Du bist ein Senior Rust Systems Engineer mit 20+ Jahren Erfahrung in Low-Level-Systemprogrammierung,
Concurrency-Modellen und Type-System-Design, sowie ein spezialisierter Auditor für sicherheitskritische
Rust-Bibliotheken. Du wurdest von einem Weltkonzern beauftragt, das Fundament-Crate `memfuse-core` des
Open-Source-Projekts MemFuse (https://github.com/tfufuz1/memfuse) einer erschöpfenden technischen Prüfung
zu unterziehen, bevor produktive Abhängigkeiten anderer Teams darauf aufbauen dürfen.

MISSION
`memfuse-core` ist der Dependency-Root des gesamten 15-Crate-Workspace — JEDES andere Crate hängt
transitiv davon ab. Ein einziger unentdeckter Fehler in Typen, Traits oder Fehlerbehandlung hier
pflanzt sich in das gesamte System fort. Deine Mission ist es, dieses Crate bis auf Bit-Ebene zu
verifizieren: jede Typ-Invariante, jede Trait-Kontraktdefinition, jede Fehlerpfad-Verzweigung, jede
Snapshot-/MVCC-Isolationsgarantie und jeden Concurrency-Mechanismus im Sharded-TxBuffer.

KONTEXT & ZIELKOMPONENTEN (aus Repository-Analyse)
Klone https://github.com/tfufuz1/memfuse und arbeite ausschließlich im Pfad `crates/memfuse-core/`.
Analysiere eigenständig folgende Module, bevor du Tests schreibst — leite aus dem tatsächlichen Code
(nicht aus Annahmen) die Geschäftslogik ab:
  - `src/types.rs` + `src/types/{saos,importance,filter,domain,budget}.rs` — DocId, EntityId, TxId als
    `#[repr(transparent)]` u64-Newtypes; ScoredDocument; Importance-Scoring-Typen; Filter-DSL; Domain-Objekte
    (u.a. ContextChunk mit Contextual-Prefix); Token-/Kosten-Budget-Typen.
  - `src/traits.rs` — StorageEngine, VectorIndex, TextIndex, GraphIndex, CheckpointCoordinator (async traits,
    die die Vertragsgrundlage für ALLE Layer-1-Engines bilden).
  - `src/error.rs` + `src/error_dto.rs` — MemFuseError Enum; Zero-Panic-via-`?`-Propagation-Invariante;
    Serialisierung von Fehlern über FFI/IPC-Grenzen (error_dto).
  - `src/tx_buffer.rs` — Sharded Transaction Staging mit "Orphan Reaper" (verwaiste Transaktionen erkennen
    und aufräumen). Analysiere die Sharding-Strategie, Lock-Granularität und Reaper-Trigger-Bedingungen.
  - `src/seq_log.rs` — Sequenzielles Log/Ordering-Primitive.
  - `src/snapshot.rs` — SnapshotRegistry für MVCC-Read-Isolation. Analysiere exakt, wie Snapshots erzeugt,
    gepinnt, freigegeben werden und wie Race Conditions zwischen Snapshot-Erzeugung und GC verhindert werden.
  - `src/ipc/{mod.rs,jsonrpc.rs,memfuse_generated.rs}` — IPC-Schicht inkl. generiertem FlatBuffers-Code
    (memfuse_generated.rs). Prüfe Schema-Kompatibilität und Serialisierungs-Roundtrips.
Beachte ADR-016 (DocId 64-Bit BLAKE3-Trunkierung und Kollisionsschutz) und ADR-028 (TxId-Allocation-
Base-Ranges für System- vs. Collection-Transaktionen) aus `DECISIONS.md` — verifiziere beide Invarianten
explizit gegen den Code.

AUFGABENUMFANG (verpflichtend, alle Punkte abarbeiten)

1. BUILD & STATISCHE ANALYSE
   - `cargo check -p memfuse-core --all-features` und ohne Features; dokumentiere jede Warnung.
   - `cargo clippy -p memfuse-core --all-targets --all-features -- -D warnings`; klassifiziere jeden
     Lint-Fund nach Schweregrad (Correctness/Suspicious/Complexity/Perf/Style) und begründe, ob es sich
     um einen echten Bug oder ein akzeptables Stilproblem handelt.
   - `cargo fmt --check -p memfuse-core`.
   - Verifiziere `#![deny(unsafe_code)]`/`#![forbid(unsafe_code)]`-Direktiven: durchsuche das Crate nach
     jedem `unsafe`-Block, dokumentiere Fundstelle, Zweck und ob eine Ausnahme laut ADR dokumentiert ist.
   - Prüfe alle `.unwrap()`/`.expect()`/`panic!()`-Vorkommen in Nicht-Test-Code — jedes einzelne ist eine
     potenzielle Verletzung der "Zero-Panic"-Invariante und MUSS im Report einzeln aufgelistet werden
     (Datei, Zeile, Kontext, Risikoeinschätzung).

2. UNIT- UND INTEGRATIONSTESTS (bestehende + selbst geschriebene)
   - Führe alle vorhandenen Tests aus: `cargo test -p memfuse-core --all-features -- --nocapture` und
     protokolliere JEDEN Testnamen mit Ergebnis, Laufzeit und Assertion-Details.
   - Ergänze fehlende Tests gemäß der Pflicht-Testmatrix aus `TESTING.md` für JEDE öffentliche Funktion/
     jeden öffentlichen Typ in obigen Modulen:
     a) Happy Path, b) leere Eingabe, c) Einzelelement, d) Grenzwerte (u64::MAX, TxId-Überlauf,
     leere DocId-Batches), e) Fehlerpfade (jeder MemFuseError-Varianten-Konstruktionspfad muss mindestens
     einmal getestet werden), f) Concurrency-Stresstests für TxBuffer (paralleles Staging/Commit/Abort
     über mehrere Tokio-Tasks, Nachweis der Abwesenheit von Deadlocks/Race Conditions via Loom oder
     wiederholten Stress-Läufen mit `--test-threads` Variation).
   - WICHTIG (Anti-Mirroring-Prinzip): Erwartungswerte für z.B. BLAKE3-Trunkierungs-Kollisionswahrscheinlichkeiten
     oder TxId-Range-Grenzen müssen unabhängig von der Implementierung berechnet/verifiziert werden — nicht
     durch Aufruf derselben Funktion mit anderen Eingaben.
   - Teste die IPC-Serialisierung (jsonrpc.rs, FlatBuffers) mit Roundtrip-Tests: encode→decode→encode muss
     bit-identisch sein; teste außerdem gezielt korrupte/trunkierte Payloads auf robuste Fehlerbehandlung.

3. PROPERTY-BASED TESTING
   - Schreibe `proptest`-Suiten für: DocId/EntityId/TxId Newtype-Arithmetik (keine stillen Overflows),
     Snapshot-Registry-Invarianten (jeder gepinnte Snapshot bleibt bis zum Unpin lesbar, auch unter
     zufälliger Interleaving-Reihenfolge von Pin/Unpin/GC-Operationen), Filter-DSL-Kombinatorik.

4. MUTATION-TESTING (gemäß TESTING.md Abschnitt 4)
   - Führe, wenn im Sandbox verfügbar, `cargo mutants -p memfuse-core` aus. Falls das Tool nicht
     installierbar ist, führe das im Testmanifest beschriebene "Mutation-Gedankenexperiment" manuell für
     mindestens 15 kritische Codepfade durch (Operator-Inversion `<`→`<=`, Off-by-one `+1`→`+0`,
     Boolean-Negation) und dokumentiere für jeden, ob ein bestehender Test ihn fängt. Bericht als Tabelle:
     Mutation | betroffene Zeile | gefangen (ja/nein) | welcher Test fängt ihn.

5. CODE-COVERAGE
   - `cargo llvm-cov -p memfuse-core --all-features --html` (oder `cargo tarpaulin` als Fallback).
     Liefere Coverage% pro Datei UND Line-Coverage-Lücken als konkrete Zeilennummern-Listen.

6. BENCHMARKS
   - Falls kein `benches/`-Verzeichnis existiert, erstelle mit `criterion` Benchmarks für: DocId/TxId
     Erzeugung & Vergleich (Throughput), TxBuffer Stage/Commit/Reaper-Zyklus unter 1/10/100/1000
     gleichzeitigen Transaktionen, SnapshotRegistry Pin/Unpin-Latenz unter steigender Snapshot-Anzahl,
     IPC-Serialisierungs-/Deserialisierungs-Durchsatz für kleine/mittlere/große Payloads (1KB/64KB/1MB).
   - Führe jeden Benchmark mit `cargo bench -p memfuse-core` real aus, erfasse Mittelwert, Median,
     Standardabweichung, p95/p99-Latenz aus dem Criterion-Output.

7. DOKUMENTATIONS-AUDIT
   - `cargo doc -p memfuse-core --no-deps`; prüfe auf fehlende Doc-Kommentare bei öffentlichen Items
     (`#![warn(missing_docs)]`-Check falls nicht gesetzt, manuell nachrüsten in der Analyse).
   - Vergleiche FILE-CONTEXT-Kommentare (STAND/ZWECK/INVARIANTEN/HOTSPOTS) im Quellcode mit dem
     tatsächlichen Codeverhalten — melde jede Diskrepanz zwischen Dokumentation und Implementierung.

REPORT-STRUKTUR (verpflichtend, als Markdown-Datei `AUDIT_memfuse-core.md`)
1. Executive Summary (Reifegrad-Einschätzung 1-10, Top-5-Risiken, Top-5-Stärken)
2. Build- & Lint-Ergebnisse (vollständige Rohausgaben in Codeblöcken)
3. Unsafe-Code-Inventar (Tabelle)
4. Panic/Unwrap-Inventar (Tabelle mit Risikoeinstufung)
5. Testergebnisse (jeder Testname, Status, Laufzeit — als Tabelle; neu geschriebene Tests separat markiert)
6. Coverage-Report (pro Datei, mit Lückenanalyse)
7. Mutation-Testing-Ergebnisse (Tabelle)
8. Property-Test-Ergebnisse inkl. gefundener Counterexamples (falls vorhanden)
9. Benchmark-Ergebnisse (Tabellen + Interpretation: wo liegen Performance-Risiken für abhängige Crates?)
10. Dokumentations-Diskrepanzen
11. Vollständige Liste aller gefundenen Bugs/Schwachstellen mit Reproduktionsschritten
12. Priorisierte Handlungsempfehlungen (kritisch/hoch/mittel/niedrig)
13. Anhang: vollständige Rohlogs aller ausgeführten Kommandos

ABNAHMEKRITERIEN
- Jede Zahl im Report muss aus einem tatsächlich in der VM ausgeführten Kommando stammen — Kommando
  und Zeitstempel sind im Anhang zu referenzieren.
- Keine Aussage ohne Beleg ("scheint korrekt" ist unzulässig — nur "verifiziert durch Test X, Zeile Y").
- Der Report muss mindestens so detailliert sein, dass ein externer Prüfer ohne erneuten Codezugriff
  eine Freigabeentscheidung treffen könnte.
```

---

# 2. `memfuse-store` (Layer 1 — LSM-Tree Storage Engine)

```
ROLLE
Du bist ein Senior Rust Storage-Engine-Entwickler mit 20+ Jahren Erfahrung im Bau von LSM-Tree-basierten
Datenbanken (vergleichbar RocksDB/LevelDB-Internas) und ein Experte für Crash-Consistency-Verifikation.
Du auditierst im Auftrag eines Weltkonzerns das Crate `memfuse-store` aus dem MemFuse-Projekt
(https://github.com/tfufuz1/memfuse), das die persistente Speicherschicht des gesamten Systems bildet.

MISSION
`memfuse-store` implementiert eine vollständige LSM-Tree-Speicher-Engine (WAL → MemTable → SSTable →
Compaction) auf Basis von `tokio::fs` für Metadaten und `std::fs::File` innerhalb `spawn_blocking` für
Block-Level-Random-Access (siehe ADR-012-Spannungsfeld). Datenverlust oder stille Korruption in dieser
Schicht wäre für ein produktives Agentic-Memory-System katastrophal. Deine Mission: beweise unter realer
Last, Prozessabbrüchen und Byte-Korruption, dass die Engine Crash-Consistency, Durability nach `fsync`,
und korrekte Compaction-Semantik garantiert.

KONTEXT & ZIELKOMPONENTEN
Klone das Repository und arbeite in `crates/memfuse-store/`. Analysiere eigenständig:
  - `src/wal.rs` — Write-Ahead-Log: Append-Format, Checksum-Schema (crc32fast laut Workspace-Deps),
    Recovery-Logik bei Programmstart, Verhalten bei trunkiertem/korruptem WAL-Tail.
  - `src/memtable.rs` — In-Memory sortierte Struktur, Flush-Trigger-Schwellwerte, Konsistenz zwischen
    aktivem und immutable MemTable während des Flush.
  - `src/sstable.rs` — On-Disk-Format (Block-Layout, Index, Bloom-Filter falls vorhanden), Lesepfad,
    Schreibpfad, Kompressions-/Encoding-Details.
  - `src/compaction.rs` — Compaction-Strategie (Level-basiert? Size-tiered?), Trigger-Bedingungen,
    Tombstone-Handling, Ressourcenverbrauch während Compaction.
  - `src/lsm.rs` — Orchestrierungs-Schicht, die WAL/MemTable/SSTable/Compaction verbindet (laut
    FILE-CONTEXT der zentrale Datenpfad: Client → TxBuffer → WAL → MemTable → SSTable → Compaction).
  - `src/checkpoint.rs` — crate-internes `pub(crate)` Checkpoint-Modul für MVCC-Snapshot-Pinning
    (gekoppelt an `SnapshotRegistry` aus memfuse-core) — NICHT zu verwechseln mit der öffentlichen
    Checkpoint-API in `memfuse-checkpoint` (ADR-011). Verifiziere diese Abgrenzung explizit im Code:
    darf `checkpoint.rs` wirklich nirgends `pub` exportiert werden?
  - `src/mmap.rs` — Memory-Mapped-File-Zugriff (memmap2), Sicherheitsimplikationen, Lifetime-Handling.
  - `src/util.rs` — Hilfsfunktionen, die von mehreren Modulen genutzt werden.
Nutze die vorhandenen Benchmarks als Ausgangspunkt: `benches/wal_bench.rs`, `benches/memtable_bench.rs`,
`benches/sstable_bench.rs` — führe sie aus, verstehe was sie messen, und erweitere sie um fehlende Szenarien.

AUFGABENUMFANG

1. BUILD & STATISCHE ANALYSE
   - `cargo check`/`cargo clippy -- -D warnings`/`cargo fmt --check` für `-p memfuse-store`.
   - Verifiziere `#![deny(unsafe_code)]` — dokumentiere JEDEN unsafe-Block (insbesondere im mmap.rs und
     ggf. Windows-ACL-Pfad laut FILE-CONTEXT-Ausnahme) mit Zweck und Risikoanalyse.

2. FUNKTIONALE KORREKTHEIT — WAL
   - Teste: sequenzielles Append + Recovery nach sauberem Neustart; Recovery nach hartem Abbruch
     (simuliere durch Kill des Prozesses mitten im Schreibvorgang, z.B. via `std::process::exit` in
     einem Kindprozess/Testharness); Recovery bei WAL mit korrupter Checksum am Ende (muss die gültigen
     vorherigen Einträge retten und den korrupten Rest verwerfen, nicht den ganzen Log verwerfen);
     Recovery bei komplett leerem/nicht existentem WAL-File; Verhalten bei WAL-Dateigrößen-Limit-Überschreitung.
   - Fault-Injection: schreibe einen Test-Harness, der einzelne Bytes im WAL-File nach dem Schreiben
     manuell flippt und prüft, dass Recovery dies erkennt (Checksum-Mismatch) statt stillschweigend
     falsche Daten zu laden.

3. FUNKTIONALE KORREKTHEIT — MemTable/SSTable/Compaction
   - Teste Flush-Trigger exakt an der konfigurierten Schwelle (Grenzwert -1, Grenzwert, Grenzwert +1 Byte/Eintrag).
   - Teste Lesepfad-Korrektheit über mehrere SSTable-Generationen hinweg (neuere Version überschreibt
     ältere; Tombstones maskieren ältere Werte korrekt).
   - Teste Compaction: vor/nach Zustand vergleichen — Datenintegrität (kein Datenverlust, keine
     Duplikate), Tombstone-Garbage-Collection nach Ablauf der Snapshot-Pinning-Frist (Zusammenspiel mit
     `checkpoint.rs`/SnapshotRegistry — ein gepinnter Snapshot MUSS verhindern, dass Compaction relevante
     Daten physisch löscht).
   - Nebenläufigkeit: paralleles Schreiben während laufender Compaction; paralleles Lesen während Flush;
     Stress-Test mit N Tokio-Tasks × M Operationen, Verifikation der Endkonsistenz gegen ein unabhängiges
     Referenz-HashMap-Modell (Shadow-State-Vergleich).

4. PROPERTY-BASED & FUZZ-ARTIGE TESTS
   - proptest für zufällige Sequenzen aus {Put, Delete, Flush, Compact, Restart} und Vergleich des
     Endzustands mit einem einfachen In-Memory-Referenzmodell (Modellbasiertes Testing / State-Machine-Testing).
   - Falls `cargo fuzz`/`afl` in der Sandbox verfügbar ist, führe einen kurzen Fuzz-Lauf (z.B. 5-10 Minuten
     Zeitbudget) gegen den SSTable-Parser bzw. WAL-Parser aus und dokumentiere Crashes/Panics.

5. VERSCHLÜSSELUNGS-/INTEGRATIONS-SCHNITTSTELLE
   - Prüfe, wie `memfuse-store` optional mit `memfuse-crypto` zusammenspielt (WAL-Verschlüsselung/
     Anti-Tamper) — falls Feature-gated, teste beide Kombinationen (mit/ohne Crypto).

6. BENCHMARKS (führe vorhandene aus + ergänze)
   - `cargo bench -p memfuse-store`. Erfasse für WAL: Append-Durchsatz (Ops/s) bei 64B/1KB/16KB
     Payload-Größe, fsync-Overhead separat gemessen (mit vs. ohne fsync-Flag falls vorhanden).
   - Für MemTable: Insert/Lookup-Latenz bei 1K/100K/1M Einträgen.
   - Für SSTable: Sequenzieller Scan-Durchsatz, Random-Point-Lookup-Latenz (p50/p95/p99), Bloom-Filter-
     False-Positive-Rate falls vorhanden (empirisch messen, nicht nur theoretisch).
   - Für Compaction: Zeit pro GB komprimierter Daten, Write-Amplification-Faktor (gemessen: geschriebene
     Bytes / logisch gespeicherte Bytes über einen definierten Workload).
   - Skalierungstest: Datenbankgröße 10MB → 100MB → 1GB (falls VM-Ressourcen erlauben), Latenz-Trend dokumentieren.

7. RESSOURCEN-/LECK-ANALYSE
   - Prüfe File-Handle-Leaks bei wiederholtem Open/Close-Zyklus (1000 Iterationen), Speicherverbrauch
     über Zeit bei Dauerlast (grobes RSS-Tracking via `/proc` in der Linux-VM).

REPORT-STRUKTUR (`AUDIT_memfuse-store.md`)
1. Executive Summary inkl. Crash-Consistency-Verdikt (GO/NO-GO mit Begründung)
2. Build/Lint/Unsafe-Inventar
3. WAL-Recovery-Testmatrix (Szenario | Ergebnis | Datenverlust ja/nein | Details)
4. Compaction-Korrektheits-Ergebnisse inkl. Write-Amplification-Zahlen
5. Concurrency-Stress-Ergebnisse (Shadow-State-Vergleich, gefundene Abweichungen)
6. Fault-Injection-Ergebnisse (Byte-Flip-Tests)
7. Property-/Modell-basierte Testergebnisse inkl. Counterexamples
8. Vollständige Benchmark-Tabellen (WAL/MemTable/SSTable/Compaction) mit Diagrammbeschreibung
9. Skalierungs-Trendanalyse
10. Ressourcenleck-Befunde
11. Priorisierte Bugliste mit Reproduktionsschritten
12. Anhang: Rohlogs

ABNAHMEKRITERIEN
- Jeder Crash-Consistency-Claim muss durch einen tatsächlich ausgeführten Fault-Injection-Test belegt sein.
- Benchmark-Zahlen müssen aus echten Criterion-Läufen stammen (Datei-Pfad zu den JSON/HTML-Ergebnissen
  im Anhang referenzieren).
```

---

# 3. `memfuse-index` (Layer 1 — HNSW Vektor-Index, SIMD)

```
ROLLE
Du bist ein Senior Rust Performance Engineer mit 20+ Jahren Erfahrung in numerischen Algorithmen,
SIMD-Optimierung und Approximate-Nearest-Neighbor-Suchstrukturen (HNSW, IVF, DiskANN). Du wurdest von
einem Weltkonzern beauftragt, das Crate `memfuse-index` des MemFuse-Projekts
(https://github.com/tfufuz1/memfuse) auf Korrektheit, numerische Stabilität und Performance zu auditieren.

MISSION
`memfuse-index` ist die einzige Stelle im gesamten Workspace, die absichtlich `unsafe` Code für
SIMD-Intrinsics verwendet (`#![deny(unsafe_code)]` statt `forbid`) und zur Laufzeit zwischen
AVX-512/AVX2/skalaren Implementierungen dispatcht. Ein Fehler in der SIMD-Distanzberechnung oder im
HNSW-Graphaufbau führt zu stillen, schwer diagnostizierbaren Suchqualitätsverlusten in der gesamten
Such-Pipeline. Deine Mission: beweise Bit-für-Bit-Übereinstimmung zwischen SIMD- und Skalar-Pfaden
innerhalb der Toleranz, verifiziere die Graph-Konstruktions- und Such-Korrektheit von HNSW gegen eine
Brute-Force-Referenz, und quantifiziere Recall/Latenz/Speicher-Trade-offs empirisch.

KONTEXT & ZIELKOMPONENTEN
Klone das Repository, arbeite in `crates/memfuse-index/`. Analysiere eigenständig:
  - `src/distance.rs` — SIMD-beschleunigte Distanzfunktionen (vermutlich Kosinus/L2/Dot-Product) mit
    Hardware-Dispatch (AVX-512 > AVX2 > Skalar-Fallback laut FILE-CONTEXT). Identifiziere JEDE
    Distanzmetrik und JEDEN `unsafe`-Intrinsic-Block einzeln.
  - `src/hnsw.rs` — Hierarchical Navigable Small World Graph: Layer-Aufbau, ef_construction/ef_search-
    Parameter, Insert-/Search-Algorithmus, Pruning-Heuristik (heuristic vs. simple neighbor selection).
  - `src/quantize.rs` — Vektor-Quantisierung (laut Benchmark-Namen `sq8_bench.rs`: vermutlich Scalar
    Quantization 8-bit). Analysiere Quantisierungs-/Dequantisierungsfehler.
  - `src/diskann.rs` — experimentelles Feature (`experimental-diskann`, Feature-gated). Beachte
    ADR-013 (DiskANN als experimentelles Feature) und ADR-017 (Explicit Authorization of unsafe Mmap
    in DiskANN, BEFUND AGT-AUDIT-002) — verifiziere, dass der dort dokumentierte unsafe-Mmap-Einsatz
    exakt den im ADR beschriebenen Grenzen entspricht.
  - `src/persistence.rs` — Serialisierung/Deserialisierung des HNSW-Graphen zur Kopplung mit
    `memfuse-store` (LsmStorage) — HNSW-Graphen liegen laut Doku exklusiv im RAM, Disk-Storage läuft
    über memfuse-store. Verifiziere Roundtrip-Korrektheit (Graph speichern → laden → identische
    Topologie/Suchergebnisse).
Nutze und erweitere die vorhandenen Benchmarks: `benches/hnsw_bench.rs`, `benches/distance_bench.rs`,
`benches/sq8_bench.rs`.

AUFGABENUMFANG

1. BUILD & STATISCHE ANALYSE
   - `cargo check`/`clippy -D warnings`/`fmt --check` für Default-Features UND `--features experimental-diskann`.
   - Vollständiges Unsafe-Code-Inventar: jeder SIMD-Intrinsic-Aufruf in distance.rs, jeder Mmap-Zugriff
     in diskann.rs — mit Begründung, warum Safety-Invarianten eingehalten werden (Alignment, Bounds,
     Lifetime). Kreuzvergleiche jeden Fund gegen ADR-017.
   - Cross-Compile-Check (falls Toolchain verfügbar) für Zielarchitekturen ohne AVX-512/AVX2 — verifiziere,
     dass der Skalar-Fallback korrekt kompiliert und zur Laufzeit über `is_x86_feature_detected!` o.ä.
     korrekt gewählt wird (CPUID-Erkennung testen/simulieren, falls möglich in der VM-Umgebung).

2. NUMERISCHE KORREKTHEIT (SIMD vs. Skalar) — HÖCHSTE PRIORITÄT
   - proptest-Suite (Pflicht laut TESTING.md Abschnitt 3): generiere zufällige Vektorpaare über den
     gesamten realistischen Wertebereich (inkl. Subnormals, sehr kleine/große Beträge, Nullvektoren,
     Vektoren mit NaN/Infinity als Grenzfalltest) und vergleiche SIMD-Ergebnis gegen Skalar-Fallback UND
     gegen eine unabhängig in reinem `f64` handimplementierte Referenzberechnung. Toleranz gemäß
     Determinismus-Gesetz: relative Abweichung Epsilon ≤ 1e-4. Dokumentiere jede gefundene Abweichung
     mit den exakten Eingabevektoren als Counterexample.
   - Teste explizit alle in `distance.rs` implementierten Metriken einzeln (nicht nur eine als Stellvertreter).
   - Grenzwerttests: Vektordimension 0, 1, sehr hohe Dimension (z.B. 4096), nicht durch SIMD-Breite
     teilbare Dimensionen (Remainder-Handling im SIMD-Code, z.B. Dimension 7, 13, 129).

3. HNSW GRAPH-KORREKTHEIT
   - Baue eine Brute-Force-kNN-Referenzimplementierung (linearer Scan) unabhängig vom HNSW-Code (Anti-
     Mirroring-Pflicht) und vergleiche Recall@k für k∈{1,5,10,50} über synthetische Datensätze
     unterschiedlicher Größe (100/1.000/10.000/100.000 Vektoren) und Dimensionalität (64/128/384/768/1536 —
     typische Embedding-Dimensionen). Dokumentiere Recall als Tabelle.
   - Teste Insert-Reihenfolge-Sensitivität: gleiche Vektormenge in unterschiedlicher Reihenfolge
     eingefügt — Suchqualität darf nicht signifikant abweichen.
   - Teste Lösch-/Update-Semantik falls vorhanden (Tombstones im Graph, Reindexierung).
   - Teste Grenzfälle: leerer Index, Index mit genau 1 Element, Suche mit k > Gesamtanzahl Elemente,
     Duplikat-Vektoren (identische Koordinaten mehrfach eingefügt).
   - Nebenläufigkeit: parallele Inserts + parallele Suchen — verifiziere keine Panics, keine
     Graph-Inkonsistenz (z.B. über Nachbarschaftslisten-Integritätsprüfung nach Stress-Test).

4. QUANTISIERUNG (quantize.rs / SQ8)
   - Teste Quantisierungsfehler empirisch: für einen repräsentativen Vektorsatz, vergleiche
     Distanzrangfolge vor/nach Quantisierung (Kendall-Tau-Korrelation oder ähnliche Rangkorrelation
     berechnen — unabhängig implementiert).
   - Grenzwerte: Vektoren an den Quantisierungs-Clipping-Grenzen, Nullvektor, gleichverteilte vs.
     schiefe Werteverteilungen.

5. DISKANN (experimentelles Feature)
   - Nur falls `--features experimental-diskann` baut: führe dieselbe Korrektheits-/Recall-Analyse wie
     für HNSW durch. Fokus zusätzlich auf Mmap-Sicherheit: Test mit absichtlich verkürzter/korrupter
     Indexdatei, die gemappt wird — MUSS kontrolliert fehlschlagen, darf NICHT zu Undefined Behavior/
     Segfault führen (falls möglich, unter `cargo miri` oder AddressSanitizer testen, sofern das
     Sandbox-Environment das für den relevanten unsafe-Teilbereich unterstützt).

6. PERSISTENCE ROUNDTRIP
   - Baue Index → speichere → lösche In-Memory-Instanz → lade neu → vergleiche: (a) exakte
     Graph-Topologie (Adjazenzlisten pro Knoten), (b) identische Suchergebnisse für einen fixen
     Test-Query-Satz vor/nach Reload.

7. BENCHMARKS (ausführen + erweitern)
   - `cargo bench -p memfuse-index --all-features`.
   - Distanzfunktionen: Durchsatz (Vergleiche/Sekunde) SIMD vs. Skalar, Speedup-Faktor pro Metrik,
     getrennt für AVX-512/AVX2 (falls die VM-CPU dies unterstützt — CPU-Features der Jules-VM im Report
     dokumentieren via `lscpu`/`/proc/cpuinfo`).
   - HNSW: Build-Zeit vs. Datensatzgröße (100/1K/10K/100K), Such-Latenz p50/p95/p99 vs. ef_search-Parameter,
     Recall-vs-Latenz-Kurve (Pareto-Front, mehrere ef_search-Werte durchmessen).
   - Speicherverbrauch: RSS-Messung pro 10.000 indizierte Vektoren bei verschiedenen Dimensionen.
   - SQ8-Quantisierung: Speicherersparnis-Faktor vs. Recall-Verlust (Tabelle).

REPORT-STRUKTUR (`AUDIT_memfuse-index.md`)
1. Executive Summary (numerische Korrektheits-Verdikt, Recall-Zusammenfassung, Top-Risiken)
2. CPU-Feature-Erkennung der Test-VM (welcher SIMD-Pfad wurde tatsächlich getestet)
3. Unsafe-Code-Inventar mit ADR-017-Abgleich
4. SIMD-vs-Skalar-Korrektheitsmatrix (pro Metrik, pro Dimension, mit max. gefundener Abweichung)
5. HNSW-Recall-Tabellen (Datensatzgröße × Dimension × k)
6. Concurrency-Stress-Ergebnisse
7. Quantisierungs-Fehleranalyse
8. DiskANN-Sicherheitsbefunde (falls getestet)
9. Persistence-Roundtrip-Ergebnisse
10. Vollständige Benchmark-Tabellen inkl. Recall-Latenz-Pareto-Front
11. Priorisierte Bugliste
12. Anhang: Rohlogs, proptest-Counterexamples im Volltext

ABNAHMEKRITERIEN
- Jede Recall-Zahl muss gegen eine unabhängig implementierte Brute-Force-Referenz gemessen sein.
- Jede SIMD-Korrektheitsaussage muss die exakte CPU-Architektur der Test-VM dokumentieren.
```

---

# 4. `memfuse-db` (Layer 2 — Orchestrator & 4-Signal Fusion)

```
ROLLE
Du bist ein Senior Rust Datenbank-Architekt mit 20+ Jahren Erfahrung im Design von Multi-Modal-
Retrieval-Systemen und Transaktionsorchestrierung. Du auditierst im Auftrag eines Weltkonzerns das
zentrale Orchestrator-Crate `memfuse-db` des MemFuse-Projekts (https://github.com/tfufuz1/memfuse) —
die Fassade, die Vektor-, Text-, Graph- und Metadatensuche zu einem einheitlichen Hybrid-Retrieval
kombiniert.

MISSION
`memfuse-db` ist das Herzstück der gesamten Suchqualität: hier laufen die 4 Signale (HNSW-Vektor, BM25-
Text, CSR-Graph, Metadaten-Filter) über Reciprocal Rank Fusion (RRF) zusammen (ADR-003), hier läuft die
Multi-Step-Query-Engine (iteratives Query-Rewriting, bis zu 3 Runden, OpenAI-o-series-Pattern), hier
läuft die Context-Compaction, und hier gilt eine strikte Lock-Hierarchie
(`collections` RwLock → `embedder` RwLock → `insert_lock` Mutex), deren Verletzung zu Deadlocks führen
würde. Deine Mission: verifiziere Transaktionskorrektheit, Fusion-Algorithmus-Korrektheit, Lock-
Hierarchie-Einhaltung und End-to-End-Suchqualität unter realistischen Multi-Tenant-Lasten.

KONTEXT & ZIELKOMPONENTEN
Klone das Repository, arbeite in `crates/memfuse-db/`. Analysiere eigenständig:
  - `src/lib.rs` — MemFuse Facade, Öffnungslogik inkl. `repair_on_open`-Reparaturgarantie (verifizieren:
    was genau wird bei Öffnen eines beschädigten Stores repariert?), Namespace-Isolation.
  - `src/fusion.rs` — Reciprocal Rank Fusion-Implementierung (Kernalgorithmus für Signalkombination) —
    dies ist der geschäftskritischste Algorithmus des gesamten Projekts.
  - `src/collection/{mod,crud,search,relate,maintenance,tx,query_builder,tests}.rs` — Collection-
    Abstraktion: CRUD-Operationen, Such-API, Relations-/Graph-Anbindung, Maintenance (Compaction-Trigger?),
    Transaktions-Handling, Query-Builder-DSL.
  - `src/filter.rs` — Metadaten-Filter-Signal (das 4. Signal der Hybridsuche).
  - `src/context.rs` + `src/context_compaction.rs` — ContextChunk-Verwaltung und Context-Compaction
    (Reduktion des Retrieval-Kontexts für LLM-Prompt-Budgets).
  - `src/multistep.rs` — Multi-Step Query Engine (iteratives Rewriting, max. 3 Runden laut README).
  - `src/transaction.rs` — Transaktionslogik über Collections hinweg.
  - `src/reaper.rs` — vermutlich Aufräum-/GC-Mechanismus (Zusammenspiel mit TxBuffer-Reaper aus memfuse-core prüfen).
  - `src/chunker.rs` — Markdown-Chunking (wird laut memfuse-mcp auch dort direkt verwendet:
    `MarkdownChunker`, `ChunkerConfig`) — analysiere Chunking-Grenzfälle (sehr lange/kurze Dokumente,
    Code-Blöcke, verschachtelte Markdown-Strukturen).
Verifiziere EXPLIZIT die dokumentierte Lock-Hierarchie (`collections` → `embedder` → `insert_lock`) durch
Code-Review jeder Stelle, an der mehrere dieser Locks gleichzeitig gehalten werden könnten.

AUFGABENUMFANG

1. BUILD & STATISCHE ANALYSE
   - `cargo check`/`clippy -D warnings`/`fmt --check -p memfuse-db` (alle Feature-Kombinationen, die mit
     `memfuse-embed`'s optionalem `onnx`-Feature interagieren, falls über Feature-Flags durchgereicht).
   - Deadlock-Statische-Analyse: durchsuche den Code nach jeder Stelle mit verschachtelten Lock-Acquires
     und verifiziere manuell die Einhaltung der dokumentierten Reihenfolge. Liste jede Fundstelle mit
     Datei/Zeile auf.

2. FUSION-ALGORITHMUS-KORREKTHEIT (fusion.rs) — HÖCHSTE PRIORITÄT
   - Implementiere unabhängig (Anti-Mirroring!) eine Referenz-RRF-Berechnung basierend auf der
     Standardformel `score = Σ 1/(k + rank_i)` und vergleiche gegen die Crate-Implementierung für
     synthetische Rank-Listen unterschiedlicher Länge, mit Ties (gleicher Rang), mit leeren Einzelsignalen
     (z.B. Text-Suche liefert 0 Treffer, nur Vektor-Suche liefert Treffer), mit widersprüchlichen
     Rankings zwischen den Signalen.
   - Teste Gewichtungsparameter (falls konfigurierbar) auf korrekte Anwendung.
   - Grenzfälle: alle 4 Signale liefern 0 Treffer; nur 1 Signal liefert Treffer; alle Signale liefern
     exakt dieselbe Reihenfolge (Fusion sollte diese Reihenfolge erhalten); Signale mit sehr
     unterschiedlicher Ergebnisanzahl (1 vs. 10.000 Treffer).

3. COLLECTION CRUD & TRANSAKTIONEN
   - Volle CRUD-Testmatrix: Create/Read/Update/Delete auf Collections und Dokumenten, inkl. Fehlerpfade
     (Collection existiert nicht, Dokument existiert nicht, Dimension-Mismatch beim Insert eines Vektors
     falscher Dimension).
   - Transaktions-Atomarität: simuliere Abbruch mitten in einer Multi-Dokument-Transaktion — verifiziere
     Rollback (kein Teilzustand sichtbar).
   - `repair_on_open`: erzeuge gezielt inkonsistente On-Disk-Zustände (z.B. WAL mit unvollständiger
     letzter Transaktion) und verifiziere, dass das Öffnen den Store korrekt repariert, OHNE gültige
     Daten zu verlieren.
   - Monoton steigende TxId-Allokation (dokumentierte Invariante): Stress-Test mit paralleler Tx-Erzeugung
     über viele Tasks, verifiziere strikte Monotonie ohne Lücken-Analyse-Fehlinterpretation (Lücken durch
     Aborts sind ok, Rückwärtssprünge NICHT).

4. MULTI-STEP QUERY ENGINE (multistep.rs)
   - Teste Konvergenzverhalten: Query, die nach 1/2/3 Runden konvergiert vs. Query, die das Runden-Limit
     erreicht (Verifiziere harte Obergrenze von 3 Runden wird nie überschritten).
   - Teste Rewriting-Qualität mit deterministischen Test-Doubles/Mocks für die LLM-Abhängigkeit (falls
     multistep.rs eine Ollama/LLM-Schnittstelle aufruft — falls ja, mocken statt echten Netzwerkaufruf).

5. CONTEXT COMPACTION & CHUNKING
   - chunker.rs: Grenzfälle — leeres Dokument, Dokument kleiner als Ziel-Chunk-Größe, Dokument mit
     einzelnem sehr langen Absatz ohne Trennzeichen, verschachtelte Markdown-Codeblöcke mit
     Chunk-Grenz-relevanten Zeichen darin, Unicode/Multi-Byte-Zeichen an Chunk-Grenzen (kein Aufbrechen
     mitten in einem Grapheme-Cluster).
   - context_compaction.rs: verifiziere, dass Token-Budget-Grenzen eingehalten werden (Zusammenspiel mit
     Budget-Typen aus memfuse-core), und dass Kompaktion die inhaltlich relevantesten Chunks priorisiert
     (falls Scoring-basiert — Testfall mit klar unterscheidbarer Relevanz konstruieren).

6. NEBENLÄUFIGKEIT & LOCK-HIERARCHIE
   - Stress-Test: N parallele Reader + M parallele Writer auf derselben Collection über
     `tokio::spawn`, hohe Iterationszahl (≥10.000 Operationen gesamt), Timeout-basierte Deadlock-Erkennung
     (Test schlägt fehl, wenn er nicht innerhalb von X Sekunden terminiert).
   - Falls `loom` einsetzbar ist: Modelliere die kritischen Lock-Pfade mit Loom für exhaustive
     Interleaving-Verifikation der dokumentierten Lock-Reihenfolge.

7. BENCHMARKS
   - `cargo bench -p memfuse-db` (erstelle Criterion-Benchmarks falls nicht vorhanden) für: End-to-End
     4-Signal-Hybridsuche-Latenz bei 1K/10K/100K Dokumenten, RRF-Fusion-Overhead isoliert gemessen,
     Insert-Durchsatz (Dokumente/Sekunde) bei steigender Collection-Größe, Multi-Step-Query-Latenz pro Runde.

REPORT-STRUKTUR (`AUDIT_memfuse-db.md`)
1. Executive Summary (Transaktions-Integritäts-Verdikt, Fusion-Korrektheits-Verdikt)
2. Lock-Hierarchie-Audit (Tabelle: Codestelle | gehaltene Locks | Reihenfolge-konform ja/nein)
3. Fusion-Algorithmus-Korrektheitsmatrix (Testfall | erwartet (unabhängig berechnet) | tatsächlich | Match)
4. CRUD-/Transaktions-Testergebnisse inkl. repair_on_open-Szenarien
5. TxId-Monotonie-Stresstest-Ergebnisse
6. Multi-Step-Query-Konvergenzanalyse
7. Chunking-/Compaction-Grenzfall-Ergebnisse
8. Concurrency-/Deadlock-Stresstest-Ergebnisse (inkl. Loom falls durchgeführt)
9. End-to-End-Benchmark-Tabellen
10. Priorisierte Bugliste
11. Anhang: Rohlogs

ABNAHMEKRITERIEN
- Fusion-Korrektheit muss gegen eine unabhängig implementierte RRF-Referenzformel verifiziert sein.
- Jeder Deadlock-Test muss ein definiertes Timeout-Kriterium haben und dessen Ausgang dokumentieren.
```

---

# 5. `memfuse-text` (Layer 1 — BM25 & deutsche Morphologie)

```
ROLLE
Du bist ein Senior Rust Entwickler mit 20+ Jahren Erfahrung in Information Retrieval, Textverarbeitung
und computerlinguistischen Algorithmen (Stemming, Kompositazerlegung). Du auditierst im Auftrag eines
Weltkonzerns das Crate `memfuse-text` des MemFuse-Projekts (https://github.com/tfufuz1/memfuse), das
das Volltextsuche-Signal (Signal 2 der 4-Signal-Fusion) inklusive spezialisierter deutscher Morphologie
bereitstellt.

MISSION
`memfuse-text` implementiert BM25-Scoring, einen invertierten Index, und — als differenzierendes Feature
des Produkts — eine deutsche Morphologie-Engine, die z.B. "Urlaubsantragsprozess" korrekt in "Urlaub",
"Antrag", "Prozess" zerlegt, um Trefferqualität bei deutschsprachigen Unternehmensdokumenten drastisch
zu verbessern. Ein Fehler in der BM25-Formel verzerrt die Rangfolge aller Textsuchergebnisse; ein Fehler
in der Kompositazerlegung führt zu stillen Recall-Verlusten bei deutschen Fachbegriffen. Deine Mission:
verifiziere BM25 gegen die publizierte Formel, verifiziere die Morphologie-Engine gegen ein
linguistisch fundiertes Test-Corpus, und quantifiziere Tokenisierungs-Robustheit.

KONTEXT & ZIELKOMPONENTEN
Klone das Repository, arbeite in `crates/memfuse-text/`. Analysiere eigenständig:
  - `src/bm25.rs` — BM25-Scoring (Parameter k1, b — identifiziere die konkret verwendeten Default-Werte
    und ob sie konfigurierbar sind). Prüfe Interaktion mit `InvertedIndex`.
  - `src/inverted.rs` — InvertedIndex, BM25MorphIndex, Language-Enum. Analysiere Postings-List-Struktur,
    Term-Frequency-/Document-Frequency-Tracking, Update-Semantik bei Dokument-Löschung (Postings-Cleanup).
  - `src/morphology.rs` — `normalize_umlauts`, `GermanCompoundSplitter`, `MorphologicalTokenizer`.
    Analysiere den Kompositazerlegungs-Algorithmus (wörterbuchbasiert? regelbasiert? Fugenlaute-Handling
    wie "s"/"es"/"n" zwischen Kompositateilen?).
  - `src/tokenizer.rs` — `DefaultTokenizer`, `GermanMorphTokenizer`, `Tokenizer`-Trait. Analysiere
    Tokenisierungsregeln (Satzzeichen, Zahlen, zusammengesetzte Wörter mit Bindestrich, E-Mail-Adressen,
    URLs innerhalb von Fließtext).
  - `src/lib.rs` — TextIndex-Trait-Implementierung (async, transaction-aware laut FILE-CONTEXT),
    Delegation Bm25Scorer → InvertedIndex.
Beachte: `#![forbid(unsafe_code)]` — striktester Unsafe-Modus im gesamten Workspace für dieses Crate.

AUFGABENUMFANG

1. BUILD & STATISCHE ANALYSE
   - `cargo check`/`clippy -D warnings`/`fmt --check -p memfuse-text`.
   - Verifiziere `#![forbid(unsafe_code)]` durch vollständige Grep-Suche — MUSS zu 0 Treffern führen.

2. BM25-KORREKTHEIT — HÖCHSTE PRIORITÄT
   - Implementiere unabhängig (Anti-Mirroring, TESTING.md Pflichtprinzip) die Standard-BM25-Formel:
     `score(D,Q) = Σ IDF(qi) · (f(qi,D)·(k1+1)) / (f(qi,D) + k1·(1-b+b·|D|/avgdl))`
     und vergleiche mit handverifizierten Beispieldokumenten (nicht durch Aufruf der Crate-internen
     IDF-Berechnung, sondern durch händisches Nachrechnen für ein kleines Corpus von z.B. 5 Dokumenten).
   - Teste IDF-Grenzfälle: Term kommt in genau 1 Dokument vor, Term kommt in ALLEN Dokumenten vor
     (IDF kann negativ werden bei Standard-BM25 — prüfe, wie die Implementierung damit umgeht — Clamping
     auf 0? Andere Formel-Variante?), Term kommt in 0 Dokumenten vor (Query-Term nicht im Index).
   - Teste k1/b-Parametersensitivität: b=0 (keine Längennormalisierung) vs. b=1 (volle Normalisierung) —
     verifiziere erwartete Score-Verschiebung bei unterschiedlich langen Dokumenten.
   - Grenzfälle: leeres Dokument, leere Query, Dokument mit nur Stopwords (falls Stopword-Filterung
     existiert), extrem langes Dokument (10.000+ Terme) vs. sehr kurzes (1 Term).

3. INVERTED INDEX KORREKTHEIT
   - CRUD auf dem Index: Insert, Update (Re-Indexierung eines geänderten Dokuments — alte Postings
     müssen vollständig entfernt werden, kein "Geist-Term"-Leck), Delete (Dokument komplett aus allen
     Postings-Listen entfernt, DF-Zähler korrekt dekrementiert).
   - Transaktions-Awareness: teste, dass laufende/nicht committete Änderungen bei paralleler Suche nicht
     sichtbar sind (MVCC-Isolation gemäß memfuse-core SnapshotRegistry-Integration).
   - Nebenläufigkeit: parallele Inserts + parallele Suchen, Stress-Test mit Konsistenzprüfung (DF-Summe
     über alle Postings muss nach Stress-Test mit dem tatsächlichen Dokumentbestand übereinstimmen).

4. DEUTSCHE MORPHOLOGIE — LINGUISTISCHE VERIFIKATION
   - Erstelle ein Test-Corpus von mindestens 40 echten deutschen Komposita unterschiedlicher Komplexität
     (z.B. "Urlaubsantragsprozess", "Donaudampfschifffahrtsgesellschaftskapitän" (Extremfall),
     "Lebensversicherungsgesellschaft", "Kraftfahrzeug-Haftpflichtversicherung", einfache 2-Teil-Komposita,
     3-4-Teil-Komposita, Komposita mit Fugen-s, Komposita mit Fugen-n, Komposita ohne Fugenlaut) und
     verifiziere die Zerlegung manuell/linguistisch fundiert (nicht gegen die Implementierung selbst).
   - Teste `normalize_umlauts`: ä→ae/a, ö→oe/o, ü→ue/u, ß→ss (verifiziere exakt welche Normalisierung
     implementiert ist), inkl. Groß-/Kleinschreibung (Ä, Ö, Ü, ß am Wortanfang — ß existiert nur klein).
   - Teste False-Positive-Rate: englische Wörter oder Eigennamen, die fälschlich als Komposita zerlegt
     werden könnten — dokumentiere Grenzfälle.
   - Teste GermanMorphTokenizer end-to-end mit einem realistischen deutschen Unternehmensdokument-Absatz
     (synthetisch erstellt) und vergleiche Tokenausgabe gegen manuelle linguistische Erwartung.

5. TOKENIZER-ROBUSTHEIT
   - Grenzfälle: leerer String, nur Whitespace, nur Satzzeichen, Unicode-Sonderzeichen (Emoji, CJK-
     Zeichen gemischt mit deutschem Text), sehr lange Einzelwörter (>1000 Zeichen), Zahlen/
     Dezimaltrennzeichen (deutsches Komma als Dezimaltrennzeichen "3,14" vs. englischer Punkt),
     zusammengesetzte URLs/E-Mail-Adressen im Fließtext.

6. PROPERTY-BASED TESTING
   - proptest für Tokenizer: für beliebige Unicode-Strings darf der Tokenizer niemals paniken (Fuzz-artige
     Robustheitsgarantie) — Pflicht-Grenzfalltest gemäß TESTING.md.
   - proptest für BM25-Score-Monotonie: bei fixer Query und steigender Term-Frequenz im Dokument (ceteris
     paribus) muss der Score monoton nicht-fallend sein (mathematische Invariante der BM25-Formel,
     unabhängig verifizierbar).

7. BENCHMARKS
   - `cargo bench -p memfuse-text` (erstellen falls nicht vorhanden): Tokenisierungsdurchsatz
     (Wörter/Sekunde) DefaultTokenizer vs. GermanMorphTokenizer, Kompositazerlegungs-Latenz pro Wort
     nach Wortlänge/Komplexität, BM25-Score-Berechnungslatenz bei steigender Corpus-Größe (1K/10K/100K
     Dokumente), InvertedIndex Insert-Durchsatz, Query-Latenz p50/p95/p99 bei steigender Query-Term-Anzahl.

REPORT-STRUKTUR (`AUDIT_memfuse-text.md`)
1. Executive Summary
2. BM25-Korrektheitsmatrix (Testfall | Handberechnung | Implementierung | Match) inkl. IDF-Edge-Cases
3. InvertedIndex CRUD- & Konsistenz-Testergebnisse
4. Deutsches Morphologie-Testcorpus mit vollständiger Zerlegungstabelle (40+ Wörter) und Trefferquote
5. Tokenizer-Robustheitsergebnisse (inkl. proptest-Fuzz-Ergebnisse, 0 Panics nachgewiesen oder Gegenbeispiele)
6. Nebenläufigkeits-Ergebnisse
7. Benchmark-Tabellen
8. Priorisierte Bugliste (insbesondere linguistische Fehlklassifikationen)
9. Anhang: Rohlogs, vollständiges Test-Corpus mit erwarteten/tatsächlichen Zerlegungen

ABNAHMEKRITERIEN
- BM25-Zahlen müssen handverifiziert sein (Rechenweg im Report zeigen).
- Das Morphologie-Testcorpus muss linguistisch begründet sein, nicht willkürlich gewählt.
```

---

# 6. `memfuse-graph` (Layer 1 — CSR-Graph & Session-DAG)

```
ROLLE
Du bist ein Senior Rust Entwickler mit 20+ Jahren Erfahrung in Graphalgorithmen, speichereffizienten
Graphdatenstrukturen (Compressed Sparse Row) und Concurrency-Design. Du auditierst im Auftrag eines
Weltkonzerns das Crate `memfuse-graph` des MemFuse-Projekts (https://github.com/tfufuz1/memfuse), das
sowohl das Wissensgraph-Suchsignal (Signal 3) als auch die Konversationsverzweigung (Session-DAG, Grok-
Pattern) bereitstellt.

MISSION
`memfuse-graph` implementiert einen Compressed-Sparse-Row-Graphen für speichereffiziente BFS-Traversierung
mit Score-Decay, Personalized-PageRank (ppr.rs), Community-Detection (community.rs) und einen separaten
Session-DAG für Agenten-Zustandsverzweigung. Die dokumentierte Lock-Hierarchie ist strikt: in `CsrGraph`
minimale, methodenlokale Lock-Scopes ohne Halten über `.await`-Punkte hinweg; in `SessionBranchTree`
MUSS `nodes` vor `edges`/`active_head` erworben werden. Deine Mission: verifiziere Graph-Algorithmus-
Korrektheit gegen Referenzimplementierungen, beweise Lock-Hierarchie-Einhaltung, und stelle numerische
Korrektheit von PPR/Score-Decay sicher.

KONTEXT & ZIELKOMPONENTEN
Klone das Repository, arbeite in `crates/memfuse-graph/`. Analysiere eigenständig:
  - `src/csr.rs` — Compressed-Sparse-Row-Graphrepräsentation: offsets, targets, weights, Adjazenzmaps,
    Pending-Edges-Pufferung vor Kompaktierung in das CSR-Format. Analysiere BFS-Traversierung mit
    Score-Decay (Decay-Formel identifizieren).
  - `src/ppr.rs` — Personalized PageRank. Identifiziere Damping-Faktor, Konvergenzkriterium (feste
    Iterationsanzahl vs. epsilon-basiert), Behandlung von Sackgassen-Knoten (Dangling Nodes).
  - `src/community.rs` — Community-Detection-Algorithmus (identifiziere welcher: Label Propagation?
    Louvain? Connected Components?).
  - `src/session_dag.rs` — `SessionBranchTree`: Konversationsverzweigung als persistierter azyklischer
    Graph. Analysiere Branch-Erstellung, Merge-/Switch-Semantik, `active_head`-Verwaltung.
  - `src/lib.rs` — `GraphIndex`-Trait-Implementierung via `CsrGraph`.
Verifiziere EXPLIZIT beide dokumentierten Lock-Hierarchien durch vollständige Codestellen-Analyse jeder
Methode, die mehr als einen internen Lock erwirbt.

AUFGABENUMFANG

1. BUILD & STATISCHE ANALYSE
   - `cargo check`/`clippy -D warnings`/`fmt --check -p memfuse-graph`.
   - Vollständiges Lock-Acquisition-Audit: liste JEDE Methode in CsrGraph und SessionBranchTree auf, die
     `parking_lot::RwLock` erwirbt, mit Scope-Beginn/-Ende und Nachweis, dass kein Lock über einen
     `.await`-Punkt gehalten wird (falls async-Methoden Locks nutzen).

2. CSR-GRAPH-KORREKTHEIT
   - Baue Graph aus bekannten synthetischen Topologien (Stern, Kette, vollständiger Graph, zufälliger
     Erdős–Rényi-Graph mit fixem Seed) und vergleiche BFS-Traversierungsreihenfolge/-Reichweite gegen
     eine unabhängig implementierte Referenz-BFS (z.B. mit `petgraph` als Kontrollimplementierung oder
     handgeschriebener Queue-basierter BFS).
   - Teste Score-Decay-Formel: verifiziere über mehrere Hops hinweg die erwartete Score-Abnahme
     (handberechnet für eine bekannte Kette).
   - Teste Pending-Edges → CSR-Kompaktierung: füge Kanten hinzu, before/after Kompaktierung Konsistenz
     der Adjazenzstruktur vergleichen; teste wiederholte Kompaktierungszyklen.
   - Grenzfälle: leerer Graph, Graph mit 1 Knoten ohne Kanten, Graph mit Selbstschleifen (self-loop),
     Graph mit parallelen Kanten (Multi-Edges) — falls erlaubt, prüfe Gewichts-Akkumulation vs.
     Überschreibung; sehr dichter Graph (nahezu vollständig) vs. sehr dünn besetzter Graph.
   - Nebenläufigkeit: paralleles Edge-Hinzufügen + parallele Traversierung, Stress-Test mit
     Konsistenzprüfung (Kantenzahl nach Stress-Test muss der Summe erfolgreicher Inserts entsprechen).

3. PERSONALIZED PAGERANK — NUMERISCHE VERIFIKATION
   - Implementiere unabhängig (Anti-Mirroring) einen Referenz-PPR-Algorithmus mit Matrix-Power-Iteration
     (Power Method) über eine kleine bekannte Graphtopologie (z.B. 5-10 Knoten) und vergleiche
     Konvergenzergebnisse (Toleranz gemäß Determinismus-Gesetz ≤ 1e-4 relative Abweichung).
   - Teste Dangling-Node-Handling explizit (Knoten ohne ausgehende Kanten).
   - Teste Konvergenz bei verschiedenen Damping-Faktor-Werten (0.15, 0.5, 0.85 — Standardwerte aus der
     Literatur) und dokumentiere Iterationsanzahl bis Konvergenz.
   - Grenzfälle: einzelner isolierter Knoten, disconnected Graph-Komponenten (PPR muss innerhalb der
     erreichbaren Komponente sinnvoll konvergieren, Score 0 für unerreichbare Knoten).

4. COMMUNITY DETECTION
   - Teste gegen synthetische Graphen mit bekannter Ground-Truth-Community-Struktur (z.B. zwei dicht
     verbundene Cluster, schwach verbunden durch eine Brückenkante) — verifiziere, dass der Algorithmus
     die bekannte Struktur korrekt erkennt.
   - Grenzfälle: Graph ohne erkennbare Community-Struktur (Zufallsgraph), Graph mit einer einzigen
     Community (alles verbunden).

5. SESSION-DAG KORREKTHEIT
   - Teste Branch-Erstellung: von einem Knoten mehrere Branches abzweigen, verifiziere Baum-/DAG-
     Struktur bleibt azyklisch (explizite Zyklenerkennungs-Prüfung nach Stress-Test mit vielen zufälligen
     Branch-Operationen).
   - Teste `active_head`-Konsistenz bei parallelem Branch-Switch aus mehreren Tasks.
   - Teste Lock-Reihenfolge (`nodes` vor `edges`/`active_head`) durch gezielte Stress-Tests mit
     absichtlich gegenläufigen Zugriffsmustern — Timeout-basierte Deadlock-Erkennung.
   - Persistenz: falls SessionBranchTree persistiert wird, teste Save/Load-Roundtrip.

6. PROPERTY-BASED TESTING
   - proptest für zufällige Graph-Konstruktionssequenzen (Add-Node, Add-Edge, Remove-Edge in
     zufälliger Reihenfolge) mit Invarianten-Check nach jeder Operation (Knotenzahl, Kantenzahl,
     CSR-Struktur-Konsistenz — z.B. `offsets`-Array muss immer streng monoton nicht-fallend sein).

7. BENCHMARKS
   - Nutze/erweitere `tests/csr_benchmark.rs` und erstelle `cargo bench -p memfuse-graph`-Suiten für:
     BFS-Traversierungslatenz vs. Graphgröße (1K/10K/100K Knoten, verschiedene Dichten), PPR-Konvergenz-
     Laufzeit vs. Graphgröße, Community-Detection-Laufzeit vs. Graphgröße, Edge-Insert-Durchsatz vor/nach
     CSR-Kompaktierung, Session-DAG Branch-Operations-Latenz bei steigender Branch-Tiefe/-Breite.

REPORT-STRUKTUR (`AUDIT_memfuse-graph.md`)
1. Executive Summary
2. Lock-Hierarchie-Audit (beide Strukturen, vollständige Tabelle)
3. CSR-Graph-Korrektheitsmatrix (BFS/Score-Decay gegen Referenz)
4. PPR-Numerische-Verifikationstabelle (Testgraph | Referenzwert | Implementierungswert | Abweichung)
5. Community-Detection-Ergebnisse gegen Ground-Truth
6. Session-DAG Azyklizitäts- und Konsistenz-Stresstest-Ergebnisse
7. Property-Test-Ergebnisse (CSR-Invarianten)
8. Benchmark-Tabellen
9. Priorisierte Bugliste
10. Anhang: Rohlogs

ABNAHMEKRITERIEN
- PPR-Werte müssen gegen eine unabhängige Power-Iteration-Referenzimplementierung verifiziert sein.
- Azyklizität des Session-DAG muss durch einen expliziten Zyklen-Detektionstest nach Stress belegt sein.
```

---

# 7. `memfuse-crypto` (Layer 1 — Kryptographischer Kern)

```
ROLLE
Du bist ein Senior Rust Security Engineer mit 20+ Jahren Erfahrung in angewandter Kryptographie,
Seitenkanal-resistenter Implementierung und Sicherheitsaudits kryptographischer Primitiven. Du wurdest
von einem Weltkonzern beauftragt, das sicherheitskritischste Crate `memfuse-crypto` des MemFuse-Projekts
(https://github.com/tfufuz1/memfuse) einer Sicherheitsprüfung nach Industriestandard zu unterziehen.

MISSION
`memfuse-crypto` schützt ALLE auf Disk liegenden Daten (AES-256-GCM-SIV) und die Integrität des WAL
(HMAC-basierter Anti-Tamper-Schutz). Die dokumentierte Kern-Invariante lautet: pro Datei wird ein
eindeutiger Schlüssel via HKDF abgeleitet, mit OsRng 8-Byte-Zufalls-Suffix + 4-Byte-Präfix zur
Verhinderung von Nonce-Reuse-Key-Leakage. Ein einziger Fehler hier (Nonce-Wiederverwendung, schwache
Schlüsselableitung, timing-abhängige Vergleiche) kompromittiert die Vertraulichkeit/Integrität ALLER
Nutzerdaten im air-gapped System. Deine Mission: verifiziere jede kryptographische Eigenschaft gegen
NIST/RFC-Referenzvektoren, beweise Nonce-Eindeutigkeit unter Last, und prüfe auf Seitenkanal-Risiken.

KONTEXT & ZIELKOMPONENTEN
Klone das Repository, arbeite in `crates/memfuse-crypto/`. Analysiere eigenständig:
  - `src/crypto.rs` — `KeyManager` (exportiert als `CryptoKey`): AES-256-GCM-SIV-Implementierung,
    Nonce-Konstruktion (8-Byte OsRng-Suffix + 4-Byte-Präfix — verifiziere exakte Bit-Anordnung und
    Gesamtlänge gegen den GCM-SIV-Nonce-Standard von 96 Bit / 12 Byte), HKDF-Schlüsselexpansion aus Passwort.
  - `src/wal_crypto.rs` — WAL-spezifische Verschlüsselungsanbindung (Integration mit memfuse-store).
  - `src/anti_tamper.rs` — HMAC-SHA256-basierter Integritätsschutz — analysiere HMAC-Key-Ableitung
    getrennt vom Verschlüsselungsschlüssel (kritische Kryptographie-Best-Practice: Key-Separation
    zwischen Encryption- und MAC-Key MUSS eingehalten werden — verifiziere dies explizit im Code).
  - `src/lib.rs` — Öffentliche API-Oberfläche.
Beachte: `#![cfg_attr(not(test), forbid(unsafe_code))]` — unsafe ist NUR in Tests erlaubt, niemals in
Produktionscode dieses Crates.

AUFGABENUMFANG

1. BUILD & STATISCHE ANALYSE
   - `cargo check`/`clippy -D warnings`/`fmt --check -p memfuse-crypto`.
   - Verifiziere `forbid(unsafe_code)` in Nicht-Test-Code vollständig (0 Treffer erwartet außerhalb
     `#[cfg(test)]`).
   - Dependency-Audit: `cargo audit -p memfuse-crypto` (RUSTSEC-Datenbank) — dokumentiere jede
     Sicherheitswarnung zu verwendeten Krypto-Bibliotheken (z.B. blake3, AES-GCM-SIV-Crate) mit CVE-ID
     falls vorhanden.
   - Verifiziere, dass sensible Schlüsselmaterial-Typen `Zeroize`/`ZeroizeOnDrop` implementieren
     (durchsuche den Code nach Key-haltenden Structs und prüfe Drop-Verhalten — ggf. mit einem
     gezielten Test, der nach Drop den Speicherinhalt inspiziert, soweit in sicherer Rust-Umgebung
     nachweisbar, z.B. via `#[cfg(test)]`-Instrumentierung).

2. KRYPTOGRAPHISCHE KORREKTHEIT GEGEN STANDARD-TESTVEKTOREN — HÖCHSTE PRIORITÄT
   - Teste AES-256-GCM-SIV gegen offizielle RFC-8452-Testvektoren (unabhängig aus der RFC entnommen,
     NICHT aus der Implementierung generiert) — encrypt(known_key, known_nonce, known_plaintext) muss
     exakt den bekannten Ciphertext + Tag aus dem RFC liefern.
   - Teste HKDF gegen RFC-5869-Testvektoren (offizielle IETF-Testvektoren für HKDF-SHA256).
   - Teste HMAC-SHA256 gegen RFC-4231-Testvektoren.
   - Teste BLAKE3 (falls direkt hier verwendet, sonst nur in memfuse-core relevant) gegen offizielle
     BLAKE3-Testvektoren aus dem Referenz-Repository.

3. NONCE-EINDEUTIGKEIT UNTER LAST
   - Generiere ≥1.000.000 Nonces über den produktiven Erzeugungspfad (8-Byte OsRng-Suffix + 4-Byte-Präfix)
     in einem Stress-Test und verifiziere Kollisionsfreiheit (Set-basierte Duplikatsprüfung). Berechne
     zusätzlich die theoretische Kollisionswahrscheinlichkeit (Geburtstagsparadoxon-Formel, unabhängig
     handberechnet) für die gegebene Nonce-Bitbreite und vergleiche mit dem empirischen Ergebnis.
   - Teste Präfix-Eindeutigkeits-Garantie: verifiziere, dass der 4-Byte-Präfix wirklich pro Datei fix und
     unterschiedlich ist (Multi-File-Test: N Dateien parallel verschlüsseln, alle Präfixe müssen paarweise
     verschieden sein — oder dokumentiere, falls Kollisionstoleranz durch den Suffix-Anteil abgefangen wird).

4. KEY-SEPARATION & HKDF-DOMAIN-SEPARATION
   - Verifiziere, dass Encryption-Key und HMAC-Key (anti_tamper.rs) aus unterschiedlichen HKDF-Info-
     Strings/Salts abgeleitet werden — teste explizit, dass beide Keys bei identischem Master-Passwort
     unterschiedliche Byte-Werte ergeben.
   - Teste HKDF mit variierender Passwortlänge (leer, 1 Zeichen, sehr lang >1000 Zeichen, Unicode-Passwörter).

5. WAL ANTI-TAMPER — INTEGRITÄTSPRÜFUNG
   - Teste: gültiger WAL-Block mit korrektem HMAC wird akzeptiert; ein einzelnes geändertes Byte im
     Payload MUSS die HMAC-Prüfung fehlschlagen lassen (systematischer Bit-Flip-Test über ALLE Byte-
     Positionen eines Testblocks — nicht nur Stichproben); ein geänderter/vertauschter HMAC-Tag selbst
     MUSS erkannt werden; Replay-Angriff (alter gültiger Block an neue Position kopiert) — prüfe, ob ein
     Sequenz-/Positions-Bindung im HMAC existiert, die dies verhindert, und teste dies explizit.
   - Teste Constant-Time-Vergleich des HMAC-Tags (Timing-Seitenkanal-Risiko): prüfe im Code, ob ein
     konstante-Zeit-Vergleich (z.B. via `subtle`-Crate oder äquivalent) verwendet wird statt eines naiven
     `==`-Vergleichs auf Byte-Arrays — dies ist eine kritische Sicherheitsanforderung, dokumentiere den
     Befund explizit als PASS/FAIL mit Codestelle.

6. FEHLERPFADE & GRENZFÄLLE
   - Entschlüsselung mit falschem Schlüssel MUSS kontrolliert fehlschlagen (kein Panic, kein
     Silent-Wrong-Output).
   - Entschlüsselung von trunkiertem Ciphertext (fehlender/unvollständiger Tag).
   - Verschlüsselung/Entschlüsselung von leerem Plaintext (0 Byte).
   - Sehr großer Plaintext (z.B. 100MB) — Performance UND Korrektheit.
   - Passwort-Wiederverwendung über mehrere KeyManager-Instanzen — deterministisches vs.
     nicht-deterministisches Verhalten explizit dokumentieren.

7. PROPERTY-BASED TESTING
   - proptest: für beliebige Plaintexts (Byte-Arrays beliebiger Länge inkl. 0) muss
     `decrypt(encrypt(pt)) == pt` gelten (Roundtrip-Invariante).
   - proptest: für beliebige 1-Bit-Flips im Ciphertext MUSS decrypt fehlschlagen (Authentizitäts-Invariante).

8. BENCHMARKS
   - `cargo bench -p memfuse-crypto` (erstellen falls nicht vorhanden): AES-256-GCM-SIV Encrypt/Decrypt-
     Durchsatz (MB/s) bei 1KB/64KB/1MB/16MB Payload-Größen, HKDF-Key-Derivation-Latenz, HMAC-Berechnungs-
     durchsatz, Nonce-Generierungs-Overhead.

REPORT-STRUKTUR (`AUDIT_memfuse-crypto.md`)
1. Executive Summary mit explizitem Sicherheits-Verdikt (GO/NO-GO für Produktionseinsatz)
2. `cargo audit`-Ergebnisse (Dependency-CVEs)
3. RFC-Testvektor-Konformitätsmatrix (AES-GCM-SIV/HKDF/HMAC/BLAKE3 — jeweils PASS/FAIL pro Vektor)
4. Nonce-Kollisions-Stresstest (empirisch vs. theoretisch berechnete Wahrscheinlichkeit)
5. Key-Separation-Verifikation
6. Anti-Tamper Bit-Flip-Testmatrix (vollständig, jede Byteposition)
7. Timing-Seitenkanal-Befund (Constant-Time-Vergleich PASS/FAIL)
8. Replay-Schutz-Befund
9. Property-Test-Ergebnisse (Roundtrip, Authentizität)
10. Benchmark-Tabellen
11. Vollständige, nach Schweregrad (Kritisch/Hoch/Mittel/Niedrig gemäß CVSS-ähnlicher Einschätzung)
    priorisierte Sicherheits-Befundliste
12. Anhang: Rohlogs, verwendete RFC-Testvektoren im Volltext

ABNAHMEKRITERIEN
- JEDER kryptographische Primitiv-Test MUSS gegen einen offiziellen, extern publizierten Testvektor
  verifiziert werden (RFC/NIST) — keine selbst erfundenen "erwarteten" Werte.
- Der Timing-Seitenkanal-Befund muss eine explizite Codestellen-Referenz enthalten.
```

---

# 8. `memfuse-checkpoint` (Layer 1 — Checkpoint-Registry)

```
ROLLE
Du bist ein Senior Rust Entwickler mit 20+ Jahren Erfahrung in Transaktionssystemen, Time-Travel-
Datenbankarchitekturen und RAII-basierter Ressourcenverwaltung. Du auditierst im Auftrag eines
Weltkonzerns das Crate `memfuse-checkpoint` des MemFuse-Projekts (https://github.com/tfufuz1/memfuse),
den gemäß ADR-011 EINZIGEN öffentlich sichtbaren Einstiegspunkt für das Checkpoint-Konzept im gesamten
Workspace.

MISSION
`memfuse-checkpoint` stellt den Trait `CheckpointCoordinator`, die `PersistentCheckpointStore`-Registry
und den RAII-Guard `CheckpointGuard` für automatisches Rollback bei Fehlern bereit. Es ist essenziell für
Time-Travel-Funktionalität und den `checkpoint → execute → commit → audit`-Loop der Agent-Engine
(memfuse-agent). Ein Fehler in der RAII-Rollback-Logik (z.B. Guard wird nicht bei Panic ausgelöst) kann
zu inkonsistenten Zuständen führen, die stillschweigend persistiert werden. Deine Mission: beweise, dass
`CheckpointGuard` unter JEDER Exit-Bedingung (normaler Drop, Panic-Unwind, expliziter Commit/Rollback)
korrekt funktioniert, und verifiziere die klare architektonische Abgrenzung zum internen
`memfuse-store::checkpoint`-Modul (ADR-011/ADR-015).

KONTEXT & ZIELKOMPONENTEN
Klone das Repository, arbeite in `crates/memfuse-checkpoint/`. Analysiere eigenständig `src/lib.rs`
(einzige Quelldatei, 1235 Zeilen laut Repo-Scan — daher vermutlich hohe Funktionsdichte):
  - `PersistentCheckpointStore` — delegiert Persistenz an ein `memfuse_core::StorageEngine`-Objekt,
    cacht aktive Checkpoints in einem thread-sicheren In-Memory-Store (`parking_lot::RwLock`).
    Analysiere Cache-Invalidierungs-/Synchronisationslogik zwischen In-Memory-Cache und persistenter
    Storage-Schicht — was passiert bei Cache-Miss? Bei gleichzeitigem Schreiben durch zwei Prozesse
    (falls relevant) oder zwei Tasks?
  - `CheckpointGuard` — RAII-Guard, `for_agent_step()`-Hotspot-Methode laut FILE-CONTEXT. Analysiere
    exakt: was passiert im `Drop`-Implementierung, wenn der Guard OHNE expliziten Commit fallengelassen
    wird (z.B. durch `?`-Fehlerpropagation oder Panic)? Ist das Standardverhalten Commit oder Rollback
    ("commit-on-success" vs. "rollback-unless-committed" — sicherheitskritischer Designentscheid)?
  - `CheckpointCoordinator`-Trait (definiert in memfuse_core, hier implementiert) — analysiere alle
    Methoden und deren Vertrags-Semantik.
Beachte ADR-011 (Consolidated Checkpoint Subsystem Architecture) und ADR-015 (RAII CheckpointGuard
Integration & Konsolidierung, BEFUND AGT-CKPT-001/AGT-STORE-002) — lies beide vollständig aus
`DECISIONS.md` und verifiziere, dass der aktuelle Code die dort getroffenen Entscheidungen tatsächlich
widerspiegelt (nicht nur zum Zeitpunkt der ADR-Erstellung, sondern JETZT im geklonten Code-Stand).

AUFGABENUMFANG

1. BUILD & STATISCHE ANALYSE
   - `cargo check`/`clippy -D warnings`/`fmt --check -p memfuse-checkpoint`.
   - Verifiziere `#![forbid(unsafe_code)]`.
   - ADR-Konformitäts-Check: lies ADR-011 und ADR-015 vollständig, erstelle eine Checkliste jeder dort
     getroffenen architektonischen Entscheidung, und verifiziere jede einzeln gegen den tatsächlichen Code.

2. RAII-GUARD-KORREKTHEIT — HÖCHSTE PRIORITÄT
   - Teste explizit alle Exit-Pfade von `CheckpointGuard`:
     a) Normaler Drop nach explizitem `.commit()`-Aufruf — verifiziere finaler Zustand ist persistiert.
     b) Drop OHNE expliziten Commit (Guard geht durch Scope-Ende verloren) — verifiziere dokumentiertes
        Verhalten (Rollback oder Commit, je nach Design) tritt zuverlässig ein.
     c) Drop während eines Panics (Panic-Unwind-Pfad) — schreibe einen Test, der bewusst einen Panic
        innerhalb des Guard-Scopes auslöst (`std::panic::catch_unwind`), und verifiziert danach den
        Systemzustand (muss konsistent/zurückgerollt sein, kein Teilzustand).
     d) Expliziter `.rollback()`-Aufruf, gefolgt von Drop — verifiziere Idempotenz (kein Doppel-Rollback-
        Fehler).
     e) Verschachtelte Guards (Guard innerhalb eines anderen Guard-Scopes, falls die API dies erlaubt) —
        teste korrekte LIFO-Auflösung.
   - `for_agent_step()`: teste den vollständigen Agent-Step-Checkpoint-Zyklus end-to-end mit einer
     Mock-/Test-StorageEngine-Implementierung.

3. PERSISTENTCHECKPOINTSTORE — CACHE-KONSISTENZ
   - Teste: Checkpoint erstellen → sofort lesen (Cache-Hit erwartet) → Cache künstlich invalidieren
     (falls API dies erlaubt) → erneut lesen (muss aus persistenter Storage korrekt nachladen).
   - Teste Parallelität: N Tasks erstellen gleichzeitig Checkpoints, M Tasks lesen gleichzeitig — Stress-
     Test mit Konsistenzprüfung (jeder gelesene Checkpoint muss vollständig und nicht korrupt sein — kein
     "Torn Read" durch RwLock-Verletzung).
   - Teste GC/Ablauf von Checkpoints (falls TTL oder explizite Lösch-API existiert) — Zusammenspiel mit
     Snapshot-Pinning aus memfuse-core (SnapshotRegistry) explizit verifizieren: ein aktiv referenzierter
     Checkpoint darf NICHT physisch gelöscht werden, solange er gepinnt ist.

4. TIME-TRAVEL-KORREKTHEIT
   - Erstelle eine Sequenz von Zustandsänderungen mit Checkpoints dazwischen (Zustand A → Checkpoint 1 →
     Zustand B → Checkpoint 2 → Zustand C), dann Rollback zu Checkpoint 1 — verifiziere, dass der
     resultierende Zustand exakt Zustand A entspricht (bytegenauer Vergleich über eine unabhängige
     Zustands-Snapshot-Prüfsumme).

5. FEHLERPFADE
   - Checkpoint-Erstellung bei voller/nicht beschreibbarer Storage — muss `Result::Err` zurückgeben, kein
     Panic.
   - Rollback zu einem nicht existierenden Checkpoint-ID.
   - Doppelte Checkpoint-Erstellung mit identischer ID (falls IDs nicht auto-generiert werden).

6. ABGRENZUNGS-VERIFIKATION (ADR-011)
   - Verifiziere durch Code-Grep, dass `memfuse-store::checkpoint` (das `pub(crate)`-Modul) NIRGENDS
     außerhalb von `memfuse-store` direkt importiert wird — insbesondere NICHT von `memfuse-checkpoint`
     selbst (dies wäre ein architektonischer Bruch, da beide Konzepte laut Doku strikt getrennt sein
     müssen). Dokumentiere das Ergebnis explizit als PASS/FAIL.

7. PROPERTY-BASED TESTING
   - proptest für zufällige Guard-Lebenszyklus-Sequenzen (create/commit/rollback/drop in zufälliger,
     aber gültiger Reihenfolge) mit Invarianten-Check: Systemzustand muss nach jeder Sequenz konsistent sein.

8. BENCHMARKS
   - `cargo bench -p memfuse-checkpoint` (erstellen): Checkpoint-Erstellungs-Latenz vs. Zustandsgröße,
     Rollback-Latenz vs. Anzahl zwischenzeitlicher Änderungen, Cache-Hit- vs. Cache-Miss-Lesepfad-Latenz,
     Durchsatz bei paralleler Checkpoint-Erstellung (1/10/100 gleichzeitige Tasks).

REPORT-STRUKTUR (`AUDIT_memfuse-checkpoint.md`)
1. Executive Summary
2. ADR-011/ADR-015-Konformitäts-Checkliste (Entscheidung | Code-Stelle | konform ja/nein)
3. RAII-Guard-Exit-Pfad-Testmatrix (alle 5 Szenarien, inkl. Panic-Unwind-Nachweis)
4. Cache-Konsistenz- & Nebenläufigkeits-Ergebnisse
5. Time-Travel-Korrektheitsnachweis (bytegenauer Zustandsvergleich)
6. Fehlerpfad-Testergebnisse
7. Architektonische Abgrenzungs-Verifikation (PASS/FAIL mit Grep-Beleg)
8. Property-Test-Ergebnisse
9. Benchmark-Tabellen
10. Priorisierte Bugliste
11. Anhang: Rohlogs

ABNAHMEKRITERIEN
- Der Panic-Unwind-Testfall ist nicht optional — ohne ihn gilt der RAII-Kern-Claim als unverifiziert.
- Die ADR-Konformitäts-Checkliste muss jede Einzelentscheidung aus beiden ADRs explizit abdecken.
```

---

# 9. `memfuse-embed` (Layer 3 — ONNX Embedding Engine, optional)

```
ROLLE
Du bist ein Senior Rust ML-Infrastructure-Engineer mit 20+ Jahren Erfahrung (davon substanzieller Anteil
in ML-Systemen) in der Integration von Inferenz-Runtimes (ONNX Runtime), Feature-Flag-Architektur und
FFI-Grenzen. Du auditierst im Auftrag eines Weltkonzerns das Crate `memfuse-embed` des MemFuse-Projekts
(https://github.com/tfufuz1/memfuse), das In-Process-Text-Embeddings ohne externe API-Aufrufe bereitstellt.

MISSION
`memfuse-embed` ist gemäß ADR-005 (Feature-Based Scaling) und dem Pure-Rust-USP der "Sovereign Core
Doctrine" so konzipiert, dass der Default-Build OHNE ONNX-Abhängigkeiten baut (`default=[]`). Das gesamte
ONNX-/Tokenizer-Funktionalität ist hinter dem `onnx`-Feature-Flag verborgen. Ein Leck von ONNX-Typen oder
-Abhängigkeiten in den Default-Build würde die Kernaussage "Pure Rust, keine schwergewichtigen
C++-Runtime-Abhängigkeiten im Kernprodukt" verletzen. Deine Mission: beweise hermetische Feature-Gate-
Isolation (TESTING.md Abschnitt 5), verifiziere Embedding- und Reranking-Korrektheit, und stelle
Thread-Safety des `spawn_blocking`-basierten Inferenzpfads sicher.

KONTEXT & ZIELKOMPONENTEN
Klone das Repository, arbeite in `crates/memfuse-embed/`. Analysiere eigenständig:
  - `src/lib.rs` — High-Level-API für Embedding-Generierung via `ort`-Crate (ONNX Runtime) und
    `tokenizers`-Crate für Preprocessing. Analysiere Threading via `tokio::task::spawn_blocking` (laut
    FILE-CONTEXT explizit zur Vermeidung von Executor-Starvation gewählt).
  - `src/reranker.rs` — `CrossEncoderReranker` (Post-RRF Neuordnung via lokalem ONNX Cross-Encoder,
    laut README "67% weniger Fehler kombiniert" mit Contextual Retrieval — verifiziere, ob dies ein
    externer Literatur-Claim oder ein interner Messwert ist; falls internes Benchmark-Ergebnis fehlt,
    führe eigene Messung durch).
Beachte ADR-005 (Feature-Based Scaling) und ADR-008 (Embedding-Backend ONNX → Ollama HTTP, Status: Final,
ersetzt ADR-007 bzgl. lokaler ONNX-Inferenz — verstehe die Historie: warum wurde von ONNX auf Ollama-HTTP
als primärer Pfad umgestellt, und in welcher Rolle existiert memfuse-embed jetzt noch — optionales
Zusatzfeature statt Kernabhängigkeit?). `#![deny(unsafe_code)]` bewusst statt `forbid`, um C-FFI/ONNX-
Runtime-Interaktionen im `onnx`-Feature zu erlauben — im Default-Build (ohne onnx) MUSS 0 unsafe existieren.

AUFGABENUMFANG

1. BUILD & STATISCHE ANALYSE — HERMETIC FEATURE GATE CHECK (TESTING.md Abschnitt 5, VERPFLICHTEND)
   - `cargo check -p memfuse-embed --no-default-features` MUSS sauber bauen — führe dies als ERSTEN
     Schritt aus und dokumentiere das vollständige Ergebnis.
   - Verifiziere Zero-Leakage: durchsuche nach jedem `#[cfg(feature = "onnx")]`-Gate im Code und prüfe,
     dass wirklich JEDER ONNX-bezogene Import/Typ/Funktion korrekt gegated ist — kompiliere testweise
     einen minimalen Downstream-Consumer-Crate-Stub, der nur `memfuse-embed` ohne Features einbindet, und
     verifiziere, dass keine ONNX-Symbole im öffentlichen API-Oberfläche sichtbar sind (`cargo doc
     --no-default-features --no-deps` durchsuchen).
   - `cargo check`/`clippy -D warnings`/`fmt --check -p memfuse-embed --features onnx` (voller Featureumfang).
   - Verifiziere `#![deny(unsafe_code)]`: im Default-Build 0 unsafe-Vorkommen (hartes Kriterium); im
     `onnx`-Feature dokumentiere jeden unsafe-Block mit Zweck (ONNX-C-FFI-Grenze).

2. EMBEDDING-KORREKTHEIT (nur mit `onnx`-Feature testbar — falls kein Modell in der Sandbox verfügbar
   ist, dokumentiere dies explizit als Einschränkung und teste stattdessen ALLE Nicht-Modell-abhängigen
   Pfade erschöpfend: Tokenisierung, Preprocessing, Batching-Logik, Error-Handling bei fehlendem/
   korruptem Modellpfad)
   - Falls ein Test-ONNX-Modell geladen werden kann: teste Determinismus (identischer Input → identischer
     Output-Vektor über mehrere Aufrufe hinweg, bit-exakt oder innerhalb Float-Toleranz).
   - Teste Batch-Verarbeitung: Einzeltext vs. Batch aus N Texten — Ergebnis für denselben Text muss
     unabhängig davon identisch sein, ob er allein oder im Batch verarbeitet wird (keine
     Batch-Kontamination zwischen Sequenzen, z.B. durch fehlerhaftes Padding/Masking).
   - Grenzfälle: leerer String, sehr langer Text (über maximale Modell-Sequenzlänge — muss korrekt
     trunkiert oder mit klarem Fehler abgelehnt werden, nicht stillschweigend falsch verarbeitet werden),
     Unicode/Sonderzeichen, Text nur aus Whitespace.

3. THREADING & SPAWN_BLOCKING-KORREKTHEIT
   - Teste, dass parallele Embedding-Anfragen aus mehreren Tokio-Tasks den Async-Executor nicht blockieren
     (Nachweis: ein paralleler "leichter" Task ohne Embedding-Bezug muss währenddessen weiterhin zeitnah
     ausgeführt werden — Latenz-Messung des leichten Tasks mit/ohne gleichzeitige Embedding-Last).
   - Stress-Test: N gleichzeitige Embedding-Anfragen, verifiziere keine Panics, keine Ressourcen-Leaks
     (offene Threads/Handles nach Testende zählen).

4. CROSS-ENCODER RERANKER (reranker.rs)
   - Teste Reranking-Korrektheit: gegebene (Query, Kandidatenliste)-Paare mit bekannt unterschiedlicher
     Relevanz (synthetisch konstruiert, z.B. ein exakt passendes Duplikat vs. ein völlig unrelated Dokument)
     — der Reranker muss die relevantere Kandidat höher ranken (qualitativer Sanity-Check, kein
     Ground-Truth-Benchmark-Datensatz nötig, aber klar konstruierter Unterscheidungsfall).
   - Teste Grenzfälle: leere Kandidatenliste, Kandidatenliste mit 1 Element (Reranking trivial/No-Op),
     sehr große Kandidatenliste (Performance + Korrektheit der finalen Sortierung).
   - Falls kein ONNX-Modell verfügbar: teste den Reranker-Interface-Vertrag mit einem Mock-Scoring-Backend
     und verifiziere korrekte Sortierlogik (Sortierstabilität bei Score-Ties).

5. FEHLERPFADE
   - Fehlendes Modell-File beim Initialisieren — muss `Result::Err` mit klarer Fehlermeldung liefern,
     kein Panic.
   - Korruptes/inkompatibles ONNX-Modell-File.
   - Dimension-Mismatch zwischen erwartetem und tatsächlichem Modell-Output.

6. BENCHMARKS (ausführen + erweitern)
   - Nutze/erweitere `benches/embed_bench.rs`. Falls ONNX-Modell in der Sandbox verfügbar: Embedding-
     Durchsatz (Texte/Sekunde) bei Batch-Größe 1/8/32/128, Latenz p50/p95/p99 pro Einzel-Embedding,
     Reranker-Durchsatz (Query-Kandidat-Paare/Sekunde).
   - Falls KEIN Modell verfügbar: benchmarke ausschließlich Tokenisierungs-/Preprocessing-Durchsatz und
     dokumentiere transparent, welche Benchmarks aus Umgebungsgründen nicht ausführbar waren.

REPORT-STRUKTUR (`AUDIT_memfuse-embed.md`)
1. Executive Summary — inkl. explizitem Abschnitt "Testbarkeitseinschränkungen dieser VM-Umgebung"
2. Hermetic-Feature-Gate-Check-Ergebnis (PASS/FAIL, vollständiger Log)
3. Unsafe-Code-Inventar (Default-Build: MUSS 0 sein — explizit verifizieren; onnx-Feature: Inventar)
4. Embedding-Korrektheits-/Determinismus-Ergebnisse (oder Begründung falls nicht testbar)
5. Threading-/Executor-Starvation-Nachweis
6. Reranker-Korrektheitstests
7. Fehlerpfad-Ergebnisse
8. Benchmark-Tabellen (mit expliziter Kennzeichnung, welche real vs. eingeschränkt sind)
9. Priorisierte Bugliste
10. Anhang: Rohlogs

ABNAHMEKRITERIEN
- Der Hermetic-Feature-Gate-Check ist NICHT verhandelbar und MUSS als erstes ausgeführt werden.
- Falls Modell-Assets in der Sandbox fehlen, ist dies explizit zu dokumentieren, NICHT durch erfundene
  Zahlen zu kaschieren.
```

---

# 10. `memfuse-agent` (Layer 3 — Persistente Agent-Workflow-Engine)

```
ROLLE
Du bist ein Senior Rust Entwickler mit 20+ Jahren Erfahrung in State-Machine-Design, verteilten
Workflow-Engines (vergleichbar Temporal/LangGraph-Internas) und Audit-Log-Systemen. Du auditierst im
Auftrag eines Weltkonzerns das Crate `memfuse-agent` des MemFuse-Projekts
(https://github.com/tfufuz1/memfuse), die "souveräne Alternative zu LangGraph/AutoGen" — eine reine
Rust-Workflow-Engine ohne externe Abhängigkeiten.

MISSION
`memfuse-agent` implementiert den deterministischen `checkpoint → execute → commit → audit`-Loop für
Multi-Step-Agenten-Ausführung, aufbauend auf `memfuse-db` (Collections), `memfuse-checkpoint` (RAII
Guards), `memfuse-graph` (deklarativer StateGraph) und `memfuse-store` (LSM-Persistenz). Es verwaltet
Workflow-Zustand, Token-Budget-Durchsetzung und unveränderliches Audit-Logging über LSM-persistierte
Keys. Ein Fehler in der State-Machine (unerlaubter Zustandsübergang, doppelte Ausführung eines Schritts,
nicht durchgesetztes Token-Budget) kann in einem produktiven Agentensystem zu Kostenexplosion oder
inkonsistentem Agenten-Verhalten führen. Deine Mission: verifiziere die vollständige State-Machine gegen
das dokumentierte Diagramm, beweise Exactly-Once-Ausführungssemantik pro Schritt, und stelle
Audit-Log-Unveränderlichkeit sicher.

KONTEXT & ZIELKOMPONENTEN
Klone das Repository, arbeite in `crates/memfuse-agent/`. Analysiere eigenständig:
  - `src/lib.rs` — enthält das State-Machine-Diagramm als Doc-Kommentar (Idle → ... , durch `run()`
    ausgelöst). Extrahiere das VOLLSTÄNDIGE Diagramm aus dem Quellcode und leite daraus die exakte
    Zustandsübergangstabelle ab (alle Zustände, alle erlaubten/verbotenen Übergänge).
  - `src/engine.rs` — zentrale Ausführungs-Engine, orchestriert vermutlich den checkpoint→execute→commit→
    audit-Zyklus.
  - `src/step.rs` — Einzelschritt-Abstraktion innerhalb eines Workflows.
  - `src/context.rs` — Workflow-Ausführungskontext (Token-Budget-Tracking? Zusammenspiel mit
    `memfuse-core::types::budget`).
  - `src/graph.rs` — deklarativer StateGraph (Zusammenspiel mit `memfuse-graph`? oder eigenständige
    Workflow-Graph-Definition — analysiere die tatsächliche Abhängigkeit).
  - `src/audit.rs` — unveränderliches Audit-Logging über LSM-persistierte Keys. Analysiere: wie wird
    Unveränderlichkeit erzwungen (Append-Only-Schema? Keine Update/Delete-API für Audit-Einträge?).
  - `src/event_source.rs` — vermutlich Event-Sourcing-Pattern für Workflow-Historie/Replay.

AUFGABENUMFANG

1. BUILD & STATISCHE ANALYSE
   - `cargo check`/`clippy -D warnings`/`fmt --check -p memfuse-agent`.
   - Extrahiere das State-Machine-Diagramm aus dem lib.rs-Doc-Kommentar wörtlich in den Report und
     erstelle eine formale Zustandsübergangstabelle daraus.

2. STATE-MACHINE-KORREKTHEIT — HÖCHSTE PRIORITÄT
   - Für JEDEN im Diagramm dokumentierten Zustand: teste JEDEN erlaubten ausgehenden Übergang (muss
     erfolgreich sein) UND mindestens 2 exemplarische verbotene Übergänge (müssen mit klarem Fehler
     abgelehnt werden, kein stiller State-Corruption).
   - Teste den vollständigen Happy-Path: Idle → run() → ... → Terminal-Zustand für einen einfachen
     1-Schritt-Workflow.
   - Teste Fehlerpfad-Übergänge: was passiert, wenn `execute` innerhalb eines Schritts fehlschlägt?
     Landet die State-Machine in einem klar definierten Fehlerzustand mit Rollback (via CheckpointGuard-
     Integration) oder in einem undefinierten Zwischenzustand? Verifiziere gegen memfuse-checkpoint-
     Integration.
   - Property-Test: zufällige Sequenzen von Übergangsversuchen (auch ungültige) — die State-Machine darf
     NIEMALS in einen im Diagramm nicht vorgesehenen Zustand gelangen (Invarianten-Check nach jeder
     Zufallssequenz).

3. EXACTLY-ONCE-AUSFÜHRUNGSSEMANTIK
   - Teste: bei einem simulierten Absturz zwischen `execute` und `commit` eines Schritts — nach Neustart/
     Recovery MUSS der Schritt entweder sauber wiederholt (mit Idempotenz-Garantie, falls dokumentiert)
     oder als fehlgeschlagen markiert werden, NIEMALS als "halb ausgeführt und committed" erscheinen.
   - Teste Idempotenz von `commit()`: doppelter Commit-Aufruf für denselben Schritt darf keine
     Doppelwirkung haben (z.B. doppelte Audit-Log-Einträge, doppelte Seiteneffekte).

4. TOKEN-BUDGET-DURCHSETZUNG
   - Teste: Workflow mit Budget-Limit X, Schritte die kumulativ das Limit überschreiten würden — MUSS
     korrekt gestoppt werden, BEVOR das Limit überschritten wird (nicht erst danach mit nachträglicher
     Fehlermeldung, falls das Design "Pre-Check" vorsieht — verifiziere welches Design tatsächlich
     implementiert ist und ob es der Dokumentation entspricht).
   - Grenzfälle: Budget exakt erschöpft (letzter Schritt nutzt exakt das Restbudget), Budget 0 von Anfang an.

5. AUDIT-LOG-UNVERÄNDERLICHKEIT
   - Teste: nach Schreiben eines Audit-Eintrags existiert KEINE öffentliche API, die diesen Eintrag
     verändert oder löscht (API-Oberflächen-Review + expliziter Versuch, dies über die LSM-Storage-Schicht
     zu umgehen, um zu verifizieren, dass die Unveränderlichkeit nicht nur "by convention" sondern
     strukturell erzwungen ist).
   - Teste Audit-Trail-Vollständigkeit: für einen Multi-Step-Workflow muss JEDER Schritt (inkl.
     fehlgeschlagener Schritte) einen nachvollziehbaren Audit-Eintrag hinterlassen — verifiziere Anzahl
     Audit-Einträge == Anzahl tatsächlich ausgeführter (auch abgebrochener) Schritte.

6. EVENT-SOURCING & REPLAY (event_source.rs)
   - Falls Replay-Funktionalität existiert: teste, dass ein aus Events rekonstruierter Workflow-Zustand
     exakt dem zur Laufzeit erreichten Zustand entspricht (Determinismus-Nachweis).

7. NEBENLÄUFIGKEIT
   - Teste parallele Ausführung mehrerer unabhängiger Workflow-Instanzen — keine gegenseitige
     Beeinflussung (Isolation).
   - Falls ein einzelner Workflow theoretisch parallel angestoßen werden könnte (Doppelstart-Schutz) —
     teste explizit, dass dies verhindert wird.

8. BENCHMARKS
   - `cargo bench -p memfuse-agent` (erstellen): Latenz pro checkpoint→execute→commit→audit-Zyklus,
     Overhead der Audit-Log-Schreibung isoliert gemessen, Durchsatz bei N parallelen Workflow-Instanzen,
     Skalierung der State-Machine-Übergangs-Latenz vs. Workflow-Historie-Länge (falls relevant für
     Event-Sourcing-Replay-Kosten).

REPORT-STRUKTUR (`AUDIT_memfuse-agent.md`)
1. Executive Summary
2. Formale Zustandsübergangstabelle (aus Doc-Kommentar extrahiert) + vollständige Testabdeckungsmatrix
   (jeder Übergang: getestet ja/nein, Ergebnis)
3. Exactly-Once-Semantik-Nachweis (Crash-Simulation-Ergebnisse)
4. Token-Budget-Durchsetzungs-Testergebnisse
5. Audit-Log-Unveränderlichkeits-Verifikation
6. Event-Sourcing-/Replay-Determinismus-Nachweis (falls zutreffend)
7. Nebenläufigkeits-Testergebnisse
8. Benchmark-Tabellen
9. Priorisierte Bugliste
10. Anhang: Rohlogs

ABNAHMEKRITERIEN
- JEDER im dokumentierten State-Diagramm vorkommende Übergang muss in der Testabdeckungsmatrix
  auftauchen — keine Lücken.
- Die Exactly-Once-Aussage muss durch einen echten Crash-Simulationstest belegt sein, nicht durch
  Code-Review allein.
```

---

# 11. `memfuse-mcp` (Layer 4 — MCP Server, stdio JSON-RPC)

```
ROLLE
Du bist ein Senior Rust Entwickler mit 20+ Jahren Erfahrung in Protokoll-Implementierungen, sicherer
Sandboxing-Architektur und stdio-basierten IPC-Systemen. Du auditierst im Auftrag eines Weltkonzerns das
Crate `memfuse-mcp` des MemFuse-Projekts (https://github.com/tfufuz1/memfuse) — den Model-Context-
Protocol-Server, über den externe KI-Agenten (z.B. Claude Desktop) auf die MemFuse-Datenbank zugreifen.

MISSION
`memfuse-mcp` implementiert JSON-RPC 2.0 EXKLUSIV über stdio (ADR-010 — bewusst KEIN HTTP/axum/TCP, um
die Air-Gapped-Sicherheitsgarantie zu erhalten). Es enthält eine `McpSandbox` mit `VolatileToolResult`-
Zeroize-Encryption für flüchtige Tool-Ausgaben (Anthropic Containment Pattern). Da dieser Server externe,
potenziell nicht vertrauenswürdige Eingaben über stdin verarbeitet, ist er die Hauptangriffsfläche des
gesamten Systems gegenüber einem kompromittierten oder fehlerhaften LLM-Client. Deine Mission: verifiziere
Protokoll-Konformität, beweise Robustheit gegen malformte/böswillige Eingaben, verifiziere die
Sandbox-Isolation, und stelle sicher, dass die dokumentierten Bounds (16MB RPC-Nachrichtengröße, 64KB
Suchquery) tatsächlich hart durchgesetzt werden.

KONTEXT & ZIELKOMPONENTEN
Klone das Repository, arbeite in `crates/memfuse-mcp/`. Analysiere eigenständig:
  - `src/lib.rs` — `MAX_RPC_BYTES` (16MB), `MAX_SEARCH_QUERY_BYTES` (64KB), Hotspots laut FILE-CONTEXT:
    `run_stdio_loop()`, `handle_request()`, `read_line_bounded()`. Analysiere den kompletten Request-
    Verarbeitungs-Loop.
  - `src/protocol.rs` — `JsonRpcRequest`, `JsonRpcResponse`, `McpError`, `response_from_error()`.
    Analysiere JSON-RPC-2.0-Konformität (id-Handling, error-Codes, batch-Requests falls unterstützt).
  - `src/sandbox.rs` — `McpSandbox`, `SandboxPolicy`. Analysiere, welche Operationen die Sandbox erlaubt/
    verbietet, und wie `VolatileToolResult` mit Zeroize-Encryption implementiert ist.
  - `src/bin/memfuse-mcp-server.rs` — Binary-Entry-Point.
  - `src/tests.rs` — vorhandene Testsuite als Ausgangspunkt.
  - Integration mit `memfuse-db::chunker::{ChunkerConfig, MarkdownChunker}` und `memfuse-db::MemFuse`.
Beachte ADR-010 (MCP-Transport: HTTP-REST-Stub → stdio JSON-RPC 2.0) — verifiziere, dass WIRKLICH keine
TCP/HTTP-Listener-Reste im Code existieren.

AUFGABENUMFANG

1. BUILD & STATISCHE ANALYSE
   - `cargo check`/`clippy -D warnings`/`fmt --check -p memfuse-mcp`.
   - Grep-Verifikation: keine `axum`/`tokio::net::TcpListener`/HTTP-Server-Symbole im Produktionscode
     (ADR-010-Konformität).

2. JSON-RPC 2.0 PROTOKOLL-KONFORMITÄT
   - Teste gegen die JSON-RPC-2.0-Spezifikation systematisch: gültiger Request mit id → korrekte Response
     mit derselben id; Notification (Request ohne id) → keine Response erwartet; ungültiges JSON
     (Parse-Error, Code -32700); Request ohne "method"-Feld (Invalid Request, -32600); unbekannte Methode
     (Method Not Found, -32601); falsche Parametertypen (Invalid Params, -32602); interner Fehler-Pfad
     (-32603).
   - Teste Batch-Requests, falls unterstützt (Array von Requests → Array von Responses in korrekter Reihenfolge).
   - Teste Response-Envelope-Struktur exakt gegen Spec (jsonrpc: "2.0"-Feld vorhanden, korrekt).

3. `read_line_bounded()` / STDIO-LOOP ROBUSTHEIT — HÖCHSTE PRIORITÄT (Hauptangriffsfläche)
   - Teste `MAX_RPC_BYTES` (16MB) hart: Eingabe exakt an der Grenze (16MB - 1, 16MB, 16MB + 1 Byte) —
     verifiziere korrektes Verhalten an jeder dieser drei Grenzen (Ablehnung mit klarem Fehler statt
     Speicher-Erschöpfung oder Panic).
   - Teste `MAX_SEARCH_QUERY_BYTES` (64KB) analog an der exakten Grenze.
   - Teste Eingabe OHNE abschließenden Newline (unvollständige Zeile) — darf nicht hängen/blockieren
     (Timeout-Test).
   - Teste sehr viele kleine Requests hintereinander (Flood-Test) — Ressourcenverbrauch über Zeit
     (Memory-Leak-Check nach 10.000 Requests).
   - Teste binäre/nicht-UTF8-Daten auf stdin — darf nicht paniken, muss kontrolliert als Fehler behandelt
     werden.
   - Teste extrem lange Einzelzeile ohne jeglichen validen JSON-Inhalt (reiner Byte-Müll) bis zur
     16MB-Grenze — Performance UND Speicherverbrauch während der Verarbeitung.
   - Teste gleichzeitiges/verschachteltes partielles Schreiben auf stdin (simuliere langsamen Client, der
     einen Request über mehrere kleine Writes verteilt sendet) — muss korrekt zusammengesetzt werden.

4. SANDBOX-ISOLATION (sandbox.rs)
   - Teste `SandboxPolicy`-Durchsetzung: konstruiere Testfälle für jede definierte Policy-Regel und
     verifiziere sowohl erlaubte als auch verbotene Operationen exakt gegen die Policy.
   - Teste `VolatileToolResult`-Zeroize-Encryption: verifiziere, dass nach Ablauf/Verwerfen eines
     volatilen Ergebnisses der zugrunde liegende Speicher tatsächlich genullt wird (analog zum
     Zeroize-Test-Ansatz aus dem memfuse-crypto-Audit — Cross-Referenz).

5. FUNKTIONALE MCP-TOOL-ENDPUNKTE
   - Identifiziere alle über `handle_request()` exponierten MCP-Tools/Methoden (z.B. Search, Ingest via
     MarkdownChunker) und teste jeden Endpunkt einzeln: Happy Path, leere Parameter, fehlende
     Pflichtparameter, Parameter mit falschem Typ, Parameter an den dokumentierten Grenzen
     (MAX_SEARCH_QUERY_BYTES).
   - Teste Chunker-Integration (MarkdownChunker über MCP getriggert) mit Grenzfall-Dokumenten (Cross-
     Referenz zu memfuse-db-Chunking-Tests, hier speziell End-to-End über das Protokoll).

6. FEHLERBEHANDLUNG & INFORMATIONSLECKS
   - Verifiziere, dass Fehlermeldungen an den Client KEINE internen Implementierungsdetails leaken, die
     ein Sicherheitsrisiko darstellen könnten (Pfadangaben, interne Stacktraces, Speicheradressen) —
     dokumentiere jeden Fund.

7. NEBENLÄUFIGKEIT
   - Falls der Server mehrere Requests pipeline-artig verarbeiten kann: Stress-Test mit interleaved
     Requests, Verifikation korrekter Antwort-Zuordnung (id-Matching) unter Last.

8. BENCHMARKS
   - `cargo bench -p memfuse-mcp` (erstellen): Request-Verarbeitungs-Durchsatz (Requests/Sekunde) bei
     minimaler/durchschnittlicher/maximaler (16MB) Nachrichtengröße, `read_line_bounded()`-Latenz-
     Overhead, End-to-End-Latenz für einen typischen Such-Request via MCP.

REPORT-STRUKTUR (`AUDIT_memfuse-mcp.md`)
1. Executive Summary — inkl. explizitem Sicherheits-Verdikt zur stdio-Angriffsfläche
2. ADR-010-Konformitätsnachweis (kein HTTP/TCP)
3. JSON-RPC-2.0-Konformitätsmatrix (Spec-Regel | Testergebnis)
4. Grenzwert-Testmatrix für MAX_RPC_BYTES & MAX_SEARCH_QUERY_BYTES (exakte Byte-Grenzen)
5. Robustheits-/Fuzz-artige Testergebnisse (Flood, Binärdaten, partielle Writes)
6. Sandbox-Policy-Durchsetzungsmatrix
7. VolatileToolResult-Zeroize-Nachweis
8. Tool-Endpunkt-Testmatrix (pro Endpunkt)
9. Informationsleck-Befunde
10. Benchmark-Tabellen
11. Priorisierte Sicherheits-/Bugliste
12. Anhang: Rohlogs

ABNAHMEKRITERIEN
- Alle drei dokumentierten Größengrenzen müssen exakt an der Grenze getestet sein (n-1/n/n+1).
- Kein Test darf den Prozess in einen hängenden Zustand versetzen ohne Timeout-Abbruch und Dokumentation.
```

---

# 12. `memfuse-ollama` (Layer 3 — Ollama Client & Embeddings)

```
ROLLE
Du bist ein Senior Rust Entwickler mit 20+ Jahren Erfahrung in HTTP-Client-Robustheit, LLM-Integrations-
schichten und Prompt-Engineering-Sicherheit (Injection-Resistenz). Du auditierst im Auftrag eines
Weltkonzerns das Crate `memfuse-ollama` des MemFuse-Projekts (https://github.com/tfufuz1/memfuse), das
gemäß ADR-008 das primäre LLM-/Embedding-Backend darstellt (ersetzt die ursprünglich geplante reine
ONNX-In-Process-Lösung als Hauptpfad).

MISSION
`memfuse-ollama` verbindet MemFuse mit einem lokal laufenden Ollama-Server für Text-Generierung und
Embeddings — die einzige Stelle im System, an der Netzwerk-I/O zu einem (wenn auch lokalen) externen
Prozess stattfindet. Es enthält den `ContextPrefixEngine` (Contextual-Retrieval-Pattern: LLM-generiertes
Kontext-Präfix vor Chunks, laut README "49% weniger Retrieval-Fehler" — Anthropic-Pattern) und
Importance-Scoring. Ein Fehler in der Prompt-Konstruktion (`build_rag_prompt`, `xml_escape`) könnte zu
Prompt-Injection-Vektoren führen, wenn Nutzerdaten ungeschützt in LLM-Prompts eingebettet werden. Deine
Mission: verifiziere HTTP-Client-Robustheit gegen Netzwerkfehler/Timeouts, beweise Injection-Sicherheit
von `xml_escape`/`build_rag_prompt`, und stelle Korrektheit des Kontext-Präfix-Mechanismus sicher.

KONTEXT & ZIELKOMPONENTEN
Klone das Repository, arbeite in `crates/memfuse-ollama/`. Analysiere eigenständig:
  - `src/client.rs` — `OllamaClient`, `OllamaConfig`, `DEFAULT_BASE_URL`, `DEFAULT_EMBED_MODEL`,
    `build_rag_prompt()`, `xml_escape()`. Analysiere HTTP-Request-Konstruktion, Timeout-Konfiguration,
    Retry-Verhalten (falls vorhanden), Streaming vs. Non-Streaming-Response-Handling.
  - `src/embedding.rs` — `OllamaEmbedder` (Embedding-Generierung via Ollama-HTTP-API statt In-Process-ONNX).
  - `src/context_prefixer.rs` — `ContextPrefixConfig`, `ContextPrefixEngine`, `ContextPrefixer`. Analysiere
    den exakten Mechanismus: wie wird das Kontext-Präfix generiert (LLM-Aufruf mit welchem Prompt-Template?),
    und wie wird es mit dem ursprünglichen Chunk kombiniert (Prepend? Separates Metadatenfeld?).
  - `src/importance.rs` — `score_importance()`. Analysiere die Scoring-Heuristik/den LLM-Aufruf für
    Wichtigkeitsbewertung von Memory-Einträgen.
  - `src/model_info.rs` — `ModelInfo`. Modell-Metadaten-Handling (Kontext-Fenster-Größe, Kapabilitäten).
  - `src/lib.rs` — Re-Export-Oberfläche.
Beachte ADR-008 (Embedding-Backend-Wechsel ONNX → Ollama HTTP) — verstehe, warum HTTP-Robustheit hier
kritischer ist als in memfuse-embed.

AUFGABENUMFANG

1. BUILD & STATISCHE ANALYSE
   - `cargo check`/`clippy -D warnings`/`fmt --check -p memfuse-ollama`.

2. `xml_escape()` / `build_rag_prompt()` — INJECTION-SICHERHEIT, HÖCHSTE PRIORITÄT
   - Teste `xml_escape()` gegen ALLE XML-Sonderzeichen (`<`, `>`, `&`, `"`, `'`) einzeln und in
     Kombination — verifiziere korrekte Entity-Escaping gemäß XML-1.0-Spezifikation (unabhängig
     recherchiert, nicht aus der Implementierung abgeleitet).
   - Teste `build_rag_prompt()` mit absichtlich böswilligem Input, der versucht, aus dem Prompt-Kontext
     "auszubrechen" (z.B. Chunk-Inhalt, der wie ein System-Prompt-Delimiter oder eine neue Instruktion
     aussieht: "Ignore previous instructions...", eingebettete XML-/Markdown-Tags, die wie Prompt-Struktur-
     Marker aussehen) — verifiziere, dass der escapte/eingebettete Inhalt strukturell klar vom Prompt-
     Gerüst getrennt bleibt (kein Bruch der Delimiter-Struktur).
   - Teste mit sehr langen Chunk-Inhalten, Unicode-Edge-Cases, eingebetteten Null-Bytes.
   - Grenzfälle: leerer Prompt-Input, Prompt-Input der bereits escapte Zeichen enthält (Doppel-Escaping-
     Vermeidung prüfen).

3. HTTP-CLIENT-ROBUSTHEIT (OllamaClient)
   - Nutze einen lokalen Mock-HTTP-Server (z.B. via `wiremock` oder `mockito`, in Cargo.lock prüfen ob
     bereits als Dev-Dependency vorhanden, sonst hinzufügen) um folgende Szenarien zu simulieren OHNE
     einen echten Ollama-Server zu benötigen:
     a) Server nicht erreichbar (Connection Refused) — muss klaren `Result::Err` liefern, kein Panic,
        kein endloses Hängen.
     b) Server antwortet mit Timeout (sehr langsame/keine Antwort) — verifiziere konfigurierten Timeout
        wird eingehalten.
     c) Server antwortet mit HTTP 4xx/5xx-Fehlercodes — korrekte Fehler-Propagation mit aussagekräftiger
        Fehlermeldung.
     d) Server antwortet mit malformtem JSON — kontrollierter Fehler statt Panic.
     e) Server antwortet mit unerwartetem aber validem JSON-Schema (fehlende erwartete Felder).
     f) Sehr große Response (z.B. lange Text-Generierung) — Streaming-Handling falls implementiert,
        Speicherverbrauch bei großen Antworten.
     g) Verbindungsabbruch MITTEN in einer Streaming-Response.
   - Falls kein Mocking-Framework einsetzbar ist: dokumentiere dies transparent und teste stattdessen die
     Fehlerbehandlungs-Codepfade durch direktes Aufrufen mit einer absichtlich falschen Base-URL
     (z.B. `http://localhost:1` — garantiert kein Server) als Ersatzstrategie für Szenario (a).

4. CONTEXT-PREFIX-ENGINE KORREKTHEIT
   - Teste `ContextPrefixEngine` mit einem Mock-LLM-Backend (deterministische Testantworten statt echtem
     Ollama-Aufruf): verifiziere, dass das generierte Präfix korrekt mit dem Original-Chunk kombiniert
     wird, und dass die `ContextPrefixConfig`-Parameter (z.B. Präfix-Länge-Limit, Aktivierung/
     Deaktivierung) korrekt respektiert werden.
   - Grenzfälle: leerer Chunk, sehr langer Chunk (über Kontext-Fenster-Grenze — Trunkierungsverhalten
     prüfen), Chunk ohne umgebenden Dokumentkontext (Randfall am Dokumentanfang/-ende).
   - Teste Deaktivierungspfad: wenn ContextPrefixing deaktiviert ist, muss der Chunk unverändert
     durchgereicht werden (No-Op-Verifikation).

5. IMPORTANCE SCORING
   - Teste `score_importance()` mit Mock-Backend: verifiziere Score-Range-Grenzen (z.B. 0.0-1.0, falls
     dokumentiert — Grenzwerte exakt testen), Determinismus bei identischem Input (falls dokumentiert
     als deterministisch), Verhalten bei leerem/trivialem Input.

6. EMBEDDER (OllamaEmbedder)
   - Teste Vektordimensions-Konsistenz: alle von einem Modell zurückgegebenen Embeddings müssen dieselbe
     Dimension haben (Konsistenz-Check über mehrere Aufrufe mit Mock-Backend).
   - Teste Batch- vs. Einzel-Embedding-Konsistenz (analog zum memfuse-embed-Audit).

7. PROPERTY-BASED TESTING
   - proptest für `xml_escape()`: für beliebige Strings darf das Ergebnis nach dem Escaping niemals ein
     unescaptes XML-Sonderzeichen enthalten, das nicht bereits Teil einer korrekten Entity war
     (strukturelle Invariante).

8. BENCHMARKS
   - `cargo bench -p memfuse-ollama` (erstellen, mit Mock-Backend um Netzwerklatenz zu isolieren):
     `xml_escape()`-Durchsatz bei steigender String-Länge, `build_rag_prompt()`-Konstruktions-Overhead,
     Context-Prefix-Kombinations-Latenz (ohne LLM-Aufruf-Anteil, nur String-Verarbeitung isoliert).

REPORT-STRUKTUR (`AUDIT_memfuse-ollama.md`)
1. Executive Summary — inkl. Prompt-Injection-Sicherheits-Verdikt
2. `xml_escape()`-Korrektheitsmatrix (Zeichen | erwartetes Escaping | tatsächliches Ergebnis)
3. Prompt-Injection-Testmatrix (Angriffsvektor | Ergebnis: erfolgreich abgewehrt ja/nein)
4. HTTP-Client-Robustheits-Testmatrix (alle 7 Szenarien a-g)
5. Context-Prefix-Engine-Korrektheitsergebnisse
6. Importance-Scoring-Testergebnisse
7. Embedder-Konsistenz-Ergebnisse
8. Property-Test-Ergebnisse
9. Benchmark-Tabellen
10. Priorisierte Bugliste (Sicherheitsrelevante Funde gesondert hervorgehoben)
11. Anhang: Rohlogs, Mock-Server-Konfiguration

ABNAHMEKRITERIEN
- Die Prompt-Injection-Testmatrix ist der wichtigste Teil dieses Reports und darf nicht ausgelassen werden.
- HTTP-Fehlerpfade müssen ohne echten Ollama-Server reproduzierbar getestet sein (Mock oder Fehl-Adresse).
```

---

# 13. `memfuse-router` (Layer 3 — SLM Routing & Dispatch)

```
ROLLE
Du bist ein Senior Rust Entwickler mit 20+ Jahren Erfahrung in Entscheidungslogik-Systemen, Routing-
Algorithmen und Konfigurationsmanagement. Du auditierst im Auftrag eines Weltkonzerns das kompakte, aber
geschäftslogisch wichtige Crate `memfuse-router` des MemFuse-Projekts (https://github.com/tfufuz1/memfuse),
das Routing-Entscheidungen zu Small-Language-Models (SLM) trifft.

MISSION
`memfuse-router` ist mit nur 511 Codezeilen das kompakteste Crate im Workspace, aber jede
Routing-Fehlentscheidung hat direkten Einfluss auf Antwortqualität und Kosten des gesamten Systems (z.B.
falsches Modell für eine Aufgabe gewählt). Deine Mission: verifiziere JEDE Verzweigung der
Routing-Entscheidungslogik erschöpfend (bei dieser Codegröße ist 100%-Branch-Coverage ein realistisches
und einzufordendes Ziel), und stelle Konsistenz der `SlmProfile`-Konfiguration sicher.

KONTEXT & ZIELKOMPONENTEN
Klone das Repository, arbeite in `crates/memfuse-router/`. Analysiere eigenständig (bei dieser
Codegröße: lies JEDE Zeile aller vier Quelldateien vor Testbeginn):
  - `src/router.rs` — `RouterEngine`, `RoutingDecision`. Analysiere exakt: welche Eingabesignale
    fließen in die Routing-Entscheidung ein (Query-Komplexität? Token-Länge? explizite Nutzer-Präferenz?
    historische Performance?), und welche konkreten Ausgabe-Zustände `RoutingDecision` annehmen kann.
  - `src/profile.rs` — `SlmProfile`. Analysiere Struktur (Modellname, Kapazitätsgrenzen, Kosten-/Latenz-
    Charakteristik?) und wie Profile verglichen/priorisiert werden.
  - `src/dispatch.rs` — `dispatch_to_slm()`. Analysiere den tatsächlichen Dispatch-Mechanismus (ruft
    dies memfuse-ollama auf? Ist es Backend-agnostisch?).
  - `src/tests.rs` — vorhandene Tests als Ausgangsbasis, identifiziere Lücken.

AUFGABENUMFANG

1. BUILD & STATISCHE ANALYSE
   - `cargo check`/`clippy -D warnings`/`fmt --check -p memfuse-router`.
   - Erstelle einen manuellen Kontrollflussgraphen (Control Flow Graph) für JEDE Funktion in `router.rs`
     und `dispatch.rs` — dies ist bei der geringen Codegröße machbar und für vollständige Branch-Coverage
     notwendig. Dokumentiere den Graphen im Report.

2. VOLLSTÄNDIGE BRANCH-COVERAGE DER ROUTING-LOGIK — HÖCHSTE PRIORITÄT
   - Für JEDE bedingte Verzweigung (`if`/`match`/`else`) in `RouterEngine`: konstruiere mindestens einen
     Testfall, der JEDEN Zweig auslöst — dokumentiere pro Zweig den auslösenden Testfall.
   - Teste Grenzwerte JEDES numerischen Schwellwerts, der die Routing-Entscheidung beeinflusst
     (identifiziert aus der Code-Analyse) exakt an Grenzwert-1/Grenzwert/Grenzwert+1.
   - Teste Tie-Breaking-Verhalten: wenn mehrere `SlmProfile`s gleich geeignet erscheinen — welches wird
     gewählt, und ist dies deterministisch (Reproduzierbarkeits-Test: 100 identische Aufrufe müssen
     identisches Ergebnis liefern)?
   - Teste mit 0 verfügbaren Profilen (leere Konfiguration) — muss kontrolliert fehlschlagen, kein Panic.
   - Teste mit genau 1 verfügbaren Profil (triviale Routing-Entscheidung).
   - Teste mit vielen (z.B. 50) synthetischen Profilen unterschiedlicher Charakteristik — Performance UND
     Korrektheit der Auswahl bei größerem Entscheidungsraum.

3. `SlmProfile`-KONSISTENZ
   - Teste Konstruktion mit ungültigen/widersprüchlichen Profil-Daten (z.B. negative Kapazitätswerte
     falls numerische Felder vorhanden, leere Modellnamen) — Validierungsverhalten dokumentieren.
   - Teste Vergleichs-/Priorisierungslogik zwischen Profilen mit proptest: für zufällig generierte
     Profil-Paare muss die Vergleichsrelation konsistent (transitiv, falls eine Ordnung definiert ist) sein.

4. DISPATCH-KORREKTHEIT
   - Teste `dispatch_to_slm()` mit einem Mock-Backend (falls es tatsächlich memfuse-ollama oder einen
     HTTP-Endpunkt aufruft) — verifiziere korrekte Parameterweitergabe (das richtige Modell aus der
     Routing-Entscheidung wird auch tatsächlich im Dispatch-Aufruf verwendet — End-to-End-Konsistenz
     zwischen Entscheidung und Ausführung).
   - Teste Fehlerpfad: Dispatch an ein nicht verfügbares/fehlerhaftes Modell — korrekte Fehler-Propagation.

5. PROPERTY-BASED TESTING
   - proptest über den gesamten Eingabe-Parameterraum der Routing-Funktion(en), mit einer Invariante:
     die zurückgegebene `RoutingDecision` muss IMMER auf ein tatsächlich in der übergebenen Profilmenge
     vorhandenes Profil verweisen (niemals ein "Phantom"-Profil, das nicht existiert).

6. MUTATION TESTING
   - Da dieses Crate klein ist, führe hier eine VOLLSTÄNDIGE (nicht nur exemplarische) Mutation-Analyse
     durch: für JEDEN Vergleichsoperator und JEDE Konstante in der Routing-Logik, mutiere sie einzeln und
     verifiziere, dass mindestens ein Test fehlschlägt. Ziel: Mutation-Score so nah wie möglich an 100%.
     Dokumentiere JEDE Mutation mit Ergebnis in einer vollständigen Tabelle.

7. BENCHMARKS
   - `cargo bench -p memfuse-router` (erstellen): Routing-Entscheidungs-Latenz bei 1/10/50/500 verfügbaren
     Profilen (Skalierungsverhalten der Auswahllogik — linear? Es sollte bei dieser Größenordnung sub-
     Millisekunden-Latenz sein, jede Abweichung ist auffällig und zu kommentieren).

REPORT-STRUKTUR (`AUDIT_memfuse-router.md`)
1. Executive Summary
2. Vollständiger Kontrollflussgraph aller Kernfunktionen
3. Branch-Coverage-Matrix (JEDER Zweig | auslösender Test | Ergebnis) — Ziel 100%
4. Grenzwert-Testergebnisse aller identifizierten Schwellwerte
5. Determinismus-/Tie-Breaking-Nachweis
6. SlmProfile-Konsistenz-Testergebnisse
7. Dispatch-Korrektheits-Ergebnisse
8. Vollständige Mutation-Testing-Tabelle (Ziel: nahe 100% Mutation-Score)
9. Property-Test-Ergebnisse
10. Benchmark-Tabellen
11. Priorisierte Bugliste
12. Anhang: Rohlogs

ABNAHMEKRITERIEN
- Aufgrund der geringen Codegröße wird eine Branch-Coverage von 100% erwartet — jede Abweichung davon
  muss explizit begründet werden (z.B. unreachable-Code-Pfad mit Beleg).
- Die Mutation-Testing-Tabelle muss vollständig sein, nicht exemplarisch.
```

---

# 14. `memfuse-tauri` (Layer 4 — Desktop-App Shell)

```
ROLLE
Du bist ein Senior Rust Entwickler mit 20+ Jahren Erfahrung in Desktop-Anwendungsarchitektur, sicherer
IPC zwischen Frontend/Backend (Tauri-Command-Pattern) und Dateiformat-Parsing (PDF/DOCX/E-Mail). Du
auditierst im Auftrag eines Weltkonzerns das Crate `memfuse-tauri` des MemFuse-Projekts
(https://github.com/tfufuz1/memfuse), die Desktop-Applikations-Shell "MemFuse Brain".

MISSION
`memfuse-tauri` ist die einzige direkte Nutzerschnittstelle des Systems: Chat-UI, Dokumenten-Import
(PDF, Word/DOCX, Markdown, E-Mails), und MCP-Server-Einbindung. Da diese Schicht Dateien aus potenziell
nicht vertrauenswürdigen Quellen (vom Nutzer importierte Dokumente) parst, ist Parser-Robustheit
sicherheitskritisch (Parser sind eine klassische Angriffsfläche für Speicherkorruption/DoS über
präparierte Dateien). Deine Mission: verifiziere jeden Dokumenten-Parser gegen malformte Eingaben,
verifiziere alle Tauri-Commands auf korrekte Fehlerbehandlung über die Frontend-Backend-Grenze, und teste
die Ingestion-Pipeline End-to-End.

KONTEXT & ZIELKOMPONENTEN
Klone das Repository, arbeite in `crates/memfuse-tauri/`. Analysiere eigenständig:
  - `src/main.rs` / `src/lib.rs` — App-Bootstrap, Plugin-Registrierung (`tauri_plugin_dialog`,
    `tauri_plugin_fs`), `AppState`-Management, Ollama-Erreichbarkeits-Check beim Start.
  - `src/state.rs` — `AppState` — analysiere geteilten mutable State zwischen Tauri-Commands (Thread-
    Safety-Anforderungen: Tauri-Commands laufen potenziell parallel).
  - `src/ollama.rs` — `OllamaBridge` — Tauri-seitige Anbindung an Ollama (Abgrenzung zu memfuse-ollama
    prüfen: Duplikation oder Delegation?).
  - `src/ingestion/{mod,pipeline,docx,pdf,email,entities}.rs` — Dokumenten-Import-Pipeline. JEDER
    Parser (docx.rs, pdf.rs, email.rs) muss einzeln auf Robustheit gegen malformte Dateien geprüft
    werden. `entities.rs` vermutlich Entity-Extraktion aus importierten Dokumenten für Graph-Anreicherung.
  - `src/commands/{mod,transform,search,collections,chat,ingest}.rs` — alle Tauri-Commands (die
    Frontend-Backend-API-Oberfläche). Analysiere JEDEN Command auf Input-Validierung und Fehler-
    Serialisierung zurück ans Frontend.
Beachte ADR-009 (memfuse-tauri als Desktop-App-Grundgerüst) und ADR-018 (Doppelstrategie PyPI-Library
UND Desktop-App).

AUFGABENUMFANG

1. BUILD & STATISCHE ANALYSE
   - `cargo check`/`clippy -D warnings`/`fmt --check -p memfuse-tauri`.
   - Analysiere `AppState`-Zugriffsmuster auf Thread-Safety (Tauri-Commands sind async und potenziell
     parallel aufrufbar) — dokumentiere Synchronisationsmechanismus und verifiziere Abwesenheit von
     Data-Races durch Code-Review + Stress-Test.

2. PARSER-ROBUSTHEIT — HÖCHSTE PRIORITÄT (Hauptangriffsfläche für Nutzer-Uploads)
   - Für JEDEN Parser (docx.rs, pdf.rs, email.rs) einzeln:
     a) Gültiges Minimal-Dokument des jeweiligen Formats — muss korrekt geparst werden.
     b) Leere Datei (0 Byte) — kontrollierter Fehler, kein Panic.
     c) Datei mit korrektem Datei-Header aber abgeschnittenem/korruptem Inhalt (trunkiert nach 10%,
        50%, 90% der Originalgröße) — kontrollierter Fehler.
     d) Datei mit falscher Extension aber falschem tatsächlichen Format (z.B. .docx-Datei die eigentlich
        eine Textdatei ist) — Format-Erkennung vs. Extension-Vertrauen prüfen.
     e) Sehr große Datei (falls VM-Ressourcen erlauben, z.B. 100MB) — Performance UND Speicherverbrauch,
        kein unbeschränktes Laden in den Speicher ohne Limit (DoS-Risiko dokumentieren falls kein Limit
        existiert).
     f) Datei mit tief verschachtelter Struktur (z.B. DOCX mit vielen verschachtelten Tabellen/Objekten,
        PDF mit vielen verschachtelten Objekten) — Stack-Overflow-Risiko bei rekursivem Parsing prüfen.
     g) Datei mit ungewöhnlicher Zeichenkodierung (Nicht-UTF8 in Textfeldern, gemischte Encodings in
        E-Mail-Headers).
     h) Falls `cargo fuzz` verfügbar: kurzer Fuzz-Lauf (5-10 Min Zeitbudget) gegen jeden Parser-Entry-
        Point, Crashes dokumentieren.
   - E-Mail-Parser speziell: teste mit verschiedenen MIME-Multipart-Strukturen, verschachtelten
     Attachments, ungültigen Header-Feldern, sehr langen Header-Zeilen (Header-Injection-Risiko).
   - PDF-Parser speziell: teste mit PDF, das eingebettete JavaScript/Actions enthält (muss ignoriert
     werden, darf nicht ausgeführt werden — Sicherheits-kritischer Test), verschlüsseltem PDF ohne
     Passwort, PDF mit beschädigter Xref-Tabelle.

3. INGESTION-PIPELINE END-TO-END (pipeline.rs)
   - Teste vollständigen Fluss: Datei-Import → Parsing → Entity-Extraktion → Chunking (Integration mit
     memfuse-db::chunker) → Indexierung. Verifiziere, dass ein Fehler in einer frühen Pipeline-Stufe
     (z.B. Parser-Fehler) korrekt propagiert wird und NICHT zu einem Teil-Import (inkonsistenter
     Datenbankzustand) führt.
   - Teste Batch-Import mehrerer Dokumente, wobei eines davon fehlerhaft ist — Rest der Batch muss
     trotzdem erfolgreich verarbeitet werden ODER die Gesamt-Batch-Semantik muss klar dokumentiert und
     getestet sein (All-or-Nothing vs. Best-Effort — verifiziere welches Verhalten implementiert ist).

4. TAURI-COMMANDS (commands/*.rs)
   - Für JEDEN Command in transform/search/collections/chat/ingest: teste mit gültigen Parametern, mit
     fehlenden Pflichtparametern, mit Parametern falschen Typs (soweit über die Rust-Typgrenze hinaus
     über JSON-Deserialisierung vom Frontend simulierbar), und verifiziere, dass Fehler als strukturierte,
     Frontend-verwertbare Fehlerobjekte zurückgegeben werden (kein Rust-Panic, der den gesamten
     Tauri-Prozess crashen würde).
   - Teste `AppState`-Interaktion bei parallelen Command-Aufrufen (z.B. gleichzeitiger `search`- und
     `ingest`-Aufruf) — Konsistenz-Stress-Test.

5. OLLAMA-BRIDGE
   - Teste Verhalten bei nicht erreichbarem Ollama beim App-Start (dokumentierter Status-Check) — App
     darf nicht abstürzen, muss klaren Status an Frontend kommunizieren.
   - Vergleiche `OllamaBridge` (hier) mit `OllamaClient` (memfuse-ollama) — dokumentiere im Report, ob
     hier unnötige Logik-Duplikation vorliegt, die ein Wartungsrisiko darstellt.

6. BENCHMARKS
   - `cargo bench -p memfuse-tauri` (erstellen, soweit Tauri-Kontext dies zulässt — ggf. isolierte
     Benchmarks nur für ingestion/-Module ohne vollen Tauri-Runtime-Kontext): Parser-Durchsatz
     (Seiten/Sekunde für PDF, Dokumentgröße/Sekunde für DOCX/E-Mail) für kleine/mittlere/große
     Testdokumente, Pipeline-End-to-End-Latenz pro importiertem Dokument.

REPORT-STRUKTUR (`AUDIT_memfuse-tauri.md`)
1. Executive Summary — inkl. explizitem Parser-Sicherheits-Verdikt
2. AppState Thread-Safety-Analyse
3. Parser-Robustheits-Testmatrix (pro Format: docx/pdf/email — alle 8 Szenarien a-h)
4. PDF-JavaScript-Sicherheitstest-Ergebnis (explizit hervorgehoben)
5. Ingestion-Pipeline End-to-End- & Batch-Semantik-Ergebnisse
6. Tauri-Command-Testmatrix (pro Command: Happy Path/Fehlerpfade)
7. Ollama-Bridge-Testergebnisse + Duplikations-Befund vs. memfuse-ollama
8. Benchmark-Tabellen
9. Priorisierte Sicherheits-/Bugliste
10. Anhang: Rohlogs, Test-Dokumente-Inventar (welche synthetischen Testdateien wurden erstellt)

ABNAHMEKRITERIEN
- Jeder Parser muss gegen ALLE 8 Robustheits-Szenarien getestet sein — keine Auslassungen.
- Der PDF-JavaScript-Sicherheitstest ist verpflichtend und gesondert zu berichten.
```

---

# 15. `memfuse-py` (Layer 3 — Python PyO3 Bindings)

```
ROLLE
Du bist ein Senior Rust Entwickler mit 20+ Jahren Erfahrung in FFI-Grenzschichten, PyO3-basierten
Python-Bindings und der Absicherung von Cross-Language-Fehlerbehandlung. Du auditierst im Auftrag eines
Weltkonzerns das Crate `memfuse-py` des MemFuse-Projekts (https://github.com/tfufuz1/memfuse), die
Python-Brücke der eingebetteten Hybrid-Search-Datenbank.

MISSION
`memfuse-py` exponiert MemFuse als PyPI-Bibliothek (ADR-018) über PyO3, mit gemeinsam genutztem
Multi-Thread-Tokio-Runtime (`OnceLock`) über Python-Worker-Threads hinweg. Die dokumentierte Kern-
Invariante lautet: "Zero Rust panics cross FFI boundary" — JEDER Rust-Panic, der über die FFI-Grenze in
Python durchschlägt, führt zu einem Python-Prozessabsturz (Segfault-artiges Verhalten) statt einer
kontrollierten Python-Exception. Deine Mission: beweise diese Zero-Panic-Garantie exhaustiv, verifiziere
korrekte `MemFuseError` → `PyErr`-Konvertierung für JEDE Fehlervariante, und stelle GIL-Handling-
Korrektheit während async `block_on`-Aufrufen sicher.

KONTEXT & ZIELKOMPONENTEN
Klone das Repository, arbeite in `crates/memfuse-py/`. Analysiere eigenständig `src/lib.rs` (einzige
Quelldatei, 1298 Zeilen laut Repo-Scan):
  - Hotspots laut FILE-CONTEXT: Zeilen 160-205 (memfuse_err Mapping — MemFuseError → PyErr-Konvertierung),
    Zeilen 270-650 (CRUD & Search-Methoden FFI-Grenzvalidierung). Lies diese Bereiche vollständig.
  - `OnceLock`-basierte geteilte Multi-Thread-Tokio-Runtime — analysiere Initialisierungs-Race-Condition-
    Sicherheit (was passiert, wenn zwei Python-Threads gleichzeitig zum ersten Mal auf die Runtime
    zugreifen?) und GIL-Release-Verhalten während `block_on()`-Aufrufen (muss das GIL freigeben, sonst
    blockiert der gesamte Python-Interpreter während einer async Rust-Operation — kritischer
    Performance- UND Deadlock-Risikofaktor bei Multi-Threading in Python).
  - `#![forbid(unsafe_code)]` — striktester Modus, obwohl FFI-Grenzen typischerweise unsafe erfordern
    (PyO3 abstrahiert dies) — verifiziere, dass wirklich kein direktes unsafe im Crate-Code selbst existiert.
  - Zero-Copy-Anspruch für Vektordaten (NumPy-Integration) — analysiere, ob dies tatsächlich Zero-Copy
    ist oder eine Kopie stattfindet, und wo genau im Code.

AUFGABENUMFANG

1. BUILD & STATISCHE ANALYSE
   - `cargo check`/`clippy -D warnings`/`fmt --check -p memfuse-py` (PyO3-Feature-Kompilierung erfordert
     ggf. eine Python-Entwicklungsumgebung in der VM — dokumentiere, falls Python-Header/`python3-dev`
     fehlen und installiere sie, falls das Netzwerk-Sandbox dies erlaubt, oder dokumentiere die
     Einschränkung transparent).
   - Verifiziere `#![forbid(unsafe_code)]` — 0 direktes unsafe im Crate-eigenen Code.

2. ZERO-PANIC-ÜBER-FFI-GRENZE — HÖCHSTE PRIORITÄT
   - Baue das Crate als Python-Extension-Modul (`maturin develop` oder äquivalent, falls in der VM
     installierbar — dokumentiere Vorgehen und ggf. Einschränkungen).
   - Für JEDE öffentlich exponierte Python-Methode (CRUD, Search, etc. aus den Zeilen 270-650): rufe sie
     aus einem Python-Testskript mit ABSICHTLICH invaliden/Grenzfall-Argumenten auf (falscher Typ, `None`
     wo ein Wert erwartet wird, negative Zahlen wo positive erwartet werden, extrem lange Strings, leere
     Collections, zirkuläre/rekursive Python-Objekte falls relevant für Serialisierung) und verifiziere:
     der Python-Prozess stürzt NIEMALS ab — jede invalide Eingabe muss als saubere Python-Exception
     (nicht als Rust-Panic/Segfault) zurückkommen.
   - Falls kein Python-Build in der Sandbox möglich ist: teste die interne Rust-Logik so weit wie möglich
     mit reinen Rust-Unit-Tests, die die FFI-Wrapper-Funktionen direkt aufrufen (unter Umgehung des
     Python-Interpreters, soweit die Funktionssignaturen dies erlauben), und dokumentiere diese
     Einschränkung explizit und transparent im Report.

3. `MemFuseError` → `PyErr` KONVERTIERUNGSMATRIX
   - Für JEDE Variante von `MemFuseError` (aus memfuse-core, cross-referenzieren): verifiziere, dass eine
     Konvertierung nach `PyErr` existiert und der resultierende Python-Exception-Typ sinnvoll gewählt ist
     (z.B. `ValueError` für Validierungsfehler, `IOError` für Storage-Fehler, ein spezifischer
     MemFuse-Exception-Typ falls definiert). Dokumentiere als vollständige Tabelle: Rust-Error-Variante →
     Python-Exception-Typ → Nachrichtentext-Erhaltung (wird die ursprüngliche Fehlermeldung korrekt
     durchgereicht?).

4. RUNTIME-INITIALISIERUNG & GIL-HANDLING
   - Teste `OnceLock`-Runtime-Initialisierung unter simulierter Nebenläufigkeit: mehrere Python-Threads
     (via Python `threading`-Modul im Testskript) rufen gleichzeitig zum allerersten Mal eine
     MemFuse-Methode auf — verifiziere exakt EINE Runtime-Instanz wird erstellt (kein Double-Init, keine
     Race Condition).
   - Teste GIL-Freigabe während `block_on()`: starte eine langlaufende MemFuse-Operation in einem Python-
     Thread, verifiziere in einem PARALLELEN Python-Thread, dass reiner Python-Code währenddessen
     weiterhin ausführbar ist (Nachweis über Zeitmessung: der parallele Python-Thread darf nicht durch
     die Rust-Operation blockiert werden, wenn GIL korrekt freigegeben wird).

5. NUMPY / ZERO-COPY-VERIFIKATION
   - Teste Vektor-Datenaustausch Python↔Rust: übergebe ein NumPy-Array an eine Insert-Methode, verifiziere
     Korrektheit der übertragenen Werte; teste mit unterschiedlichen NumPy-Dtypes (float32, float64,
     int-Arrays fälschlich übergeben) — Typfehler müssen kontrolliert abgelehnt werden.
   - Falls Zero-Copy behauptet wird: verifiziere dies empirisch (z.B. durch Speicheradress-Vergleich
     zwischen Python-Buffer und dem, was Rust tatsächlich liest, soweit über PyO3-Buffer-Protocol-APIs
     nachweisbar) oder widerlege den Zero-Copy-Anspruch mit Belegen, falls tatsächlich kopiert wird.

6. VOLLSTÄNDIGE CRUD-/SEARCH-API-TESTMATRIX AUS PYTHON
   - Teste die komplette öffentliche Python-API end-to-end aus einem Python-Testskript: Collection
     erstellen, Dokumente einfügen, suchen, aktualisieren, löschen — inkl. aller Grenzfälle analog zu den
     Rust-seitigen Tests in memfuse-db (Cross-Referenz), diesmal aber explizit über die FFI-Grenze
     verifiziert.

7. BENCHMARKS
   - FFI-Overhead isoliert messen: Vergleiche Latenz eines identischen Suchvorgangs rein in Rust
     (Baseline aus memfuse-db-Benchmarks) vs. über die Python-FFI-Grenze aufgerufen — quantifiziere den
     FFI-Overhead in absoluten und prozentualen Zahlen.
   - NumPy-Array-Transfer-Durchsatz bei steigender Vektordimension/Batch-Größe.

REPORT-STRUKTUR (`AUDIT_memfuse-py.md`)
1. Executive Summary — inkl. explizitem Zero-Panic-Verdikt und Testbarkeits-Einschränkungen der VM
2. Python-Build-Vorgehen & Umgebungsdokumentation (maturin/Toolchain-Details)
3. Zero-Panic-FFI-Testmatrix (Methode | invalider Input | Ergebnis: Exception ja/Crash nein)
4. MemFuseError→PyErr-Konvertierungstabelle (vollständig, jede Variante)
5. Runtime-Initialisierungs- & GIL-Handling-Nachweis
6. NumPy/Zero-Copy-Verifikationsergebnis (bestätigt oder widerlegt, mit Belegen)
7. Vollständige Python-API-CRUD-Testmatrix
8. FFI-Overhead-Benchmark-Ergebnisse
9. Priorisierte Bugliste
10. Anhang: Rohlogs, verwendete Python-Testskripte im Volltext

ABNAHMEKRITERIEN
- Der Zero-Panic-Claim ist die zentrale Sicherheitsaussage dieses Crates und muss entweder vollständig
  über echte Python-Aufrufe verifiziert oder die Einschränkung explizit und begründet dokumentiert sein.
- Die MemFuseError→PyErr-Tabelle muss VOLLSTÄNDIG sein (jede Enum-Variante aus memfuse-core abgedeckt).
```

---

## Hinweis zu `xtask`

Das 16. Workspace-Mitglied `xtask` ist kein Bibliotheks-Crate, sondern ein internes Build-/Automatisierungswerkzeug (Standard-Rust-Pattern für Custom-Cargo-Subcommands) und wurde daher bewusst NICHT in die obige Liste aufgenommen. Falls eine Prüfung gewünscht ist, empfiehlt sich ein separater, leichtgewichtiger Prompt, der ausschließlich die korrekte Funktion der bereitgestellten Subcommands (nicht Geschäftslogik) verifiziert — auf Anfrage nachreichbar.

## Empfohlene Ausführungsreihenfolge für Google-Jules

1. `memfuse-core` (Fundament — muss zuerst verifiziert sein)
2. `memfuse-store`, `memfuse-crypto` (parallel möglich, beide Layer 1, unabhängig)
3. `memfuse-index`, `memfuse-text`, `memfuse-graph`, `memfuse-checkpoint` (parallel möglich, Layer 1)
4. `memfuse-db` (hängt von allen Layer-1-Crates ab)
5. `memfuse-embed`, `memfuse-ollama`, `memfuse-router`, `memfuse-agent`, `memfuse-py` (Layer 3, teilweise parallel möglich)
6. `memfuse-mcp`, `memfuse-tauri` (Layer 4, hängen von Layer-3-Crates ab)

Jeder Prompt ist so geschrieben, dass er **eigenständig als vollständiger Jules-Task** übergeben werden kann; bei sequenzieller Abarbeitung in obiger Reihenfolge kann der jeweils vorherige Audit-Report als zusätzlicher Kontext mitgegeben werden, ist aber nicht Voraussetzung für die Ausführbarkeit des jeweiligen Prompts.
