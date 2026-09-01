# MemFuse — Google-Jules Crate-Audit-Prompts, Runde 2 (Vertiefung & Dekomposition)

**Repository:** `https://github.com/tfufuz1/memfuse`
**Basis dieser Runde:** Vollständige Auswertung von `docs/audits/AUDIT_*.md` (Ergebnisse der Runde-1-Prompts aus `docs/audits/Prompts-Jules-Audits.md`) plus einer unabhängigen Zweitmeinungs-Analyse ("Gemini-Analyse", im Auftraggeber-Prompt zitiert), die für ein vollständig LLM-generiertes ~67k-LOC-Rust-Projekt eine Reihe von **hypothetischen, typischen LLM-Fehlerklassen** benennt (SIMD-Alignment/CPU-Feature-Detection, mmap-TOCTOU, fehlendes fsync, Tombstone-Resurrection, HMAC-Truncation, CSR-Mutations-Kollaps, PPR-Sink-Holes, Unicode-Byte-Slicing-Panics, exponentielle Kompositazerlegung, Lock-Ordering-Deadlocks, Read-Uncommitted-Fenster bei Vektor-/Graph-Suche u.a.).

## Ergebnis des Abgleichs (Runde 1 vs. Gemini-Hypothesen)

| Gemini-Hypothese | Status laut Runde-1-Audit / Code-Verifikation | Konsequenz für Runde 2 |
|---|---|---|
| Fehlendes `fsync` im WAL → Silent Data Loss | `memfuse-store`-Audit fand **5 kritische Bugs** in Compaction/MemTable/SSTable/Snapshot-Pfad (bereits behoben) — fsync selbst nicht explizit widerlegt | **Erneut prüfen**, gezielt fsync-Pfad isoliert verifizieren (Runde 1 hat es nicht explizit als eigenen Prüfpunkt geführt) |
| Tombstone-Resurrection bei Compaction | Bug 1 (CRITICAL, RESOLVED): "Compaction Tombstone Masking via `TOMBSTONE_BIT`" — **bestätigt und bereits gefixt** | **Regressionstest verlangen**, nicht erneut suchen |
| SIMD ohne CPU-Feature-Detection → Illegal Instruction | Runde-1-Index-Audit fand andere kritische Bugs (DiskANN Recall 39,5%, mmap-Slice-Panics), CPU-Feature-Detection nicht explizit isoliert verifiziert | **Gezielt nachprüfen** in Runde 2 |
| mmap TOCTOU / unaligned cast | Bug-02/03 (HIGH, RESOLVED): Direct-Slice-Indexing-Panics in `persistence.rs`/`diskann.rs` bei verkürzten Dateien — **bestätigt, bereits gefixt** | Regressionstest + **TOCTOU-Race-Test** (Datei wird *während* des mmap-Zugriffs verkürzt) fehlt noch — **nachfordern** |
| CSR-Mutations-Kollaps (O(\|E\|) pro Kante) | Graph-Audit fand echten Compaction-Bug (`offsets.len()` Inkonsistenz), Delta-Graph-Architektur nicht explizit auf Kosten-Komplexität vermessen | **Komplexitäts-Benchmark nachfordern** (Runde 1 hat Korrektheit, nicht Skalierungskosten pro Insert vermessen) |
| PPR Sink-Holes / Dangling Nodes | Nicht mit dieser Tiefe im Runde-1-Report behandelt | **Neu, gezielt prüfen** |
| Unicode-Byte-Slicing-Panics | Nicht explizit gegen Grapheme-Grenzen getestet, nur Tokenizer-Fuzzing allgemein | **Neu, gezielt prüfen** |
| Exponentielle Kompositazerlegung / DoS | Morphologie-Corpus zeigt 41/45 PASS, aber 3 lange Komposita (`donaudampfschifffahrtsgesellschaftskapitaen` etc.) wurden **nicht gesplittet** (FAIL) — das ist ein Korrektheits-*und* ein potenzielles Performance-Signal (Backtracking-Abbruch?) | **Root-Cause-Analyse + DoS-Timing-Test nachfordern** |
| 2PC-Deadlocks zwischen Sub-Engines | `memfuse-db`-Audit bestätigt Lock-Hierarchie-Konformität (Tabellen-Audit) und 10.000+ Op Stresstest ohne Deadlock — **aber**: Zettelkasten-Displacement-Zyklen (`Supersedes`-Relation) wurden nicht geprüft | **Neu, gezielt prüfen** |
| Fehlende Snapshot-Isolation bei Vektor-/Graph-Suche (nur LSM hat MVCC) | Nicht im Runde-1-Report widerlegt oder bestätigt | **Kritisch, neu prüfen** |
| `commit_mutex` als globaler Schreib-Flaschenhals | Bestätigt im Code vorhanden (`lsm.rs`) als bewusstes Serialisierungs-Primitiv — Frage ist, ob es **zu grobgranular** ist | **Durchsatz-Benchmark unter Schreiblast nachfordern** |
| Router (`memfuse-router`) verletzt Layer-DAG durch Import aus `memfuse-mcp` | **Im aktuellen Code-Stand nicht nachweisbar** (kein `memfuse_mcp`-Import in `memfuse-router/src/`) — vermutlich bereits behoben oder Fehleinschätzung | **Nur Regressionscheck**, kein Schwerpunkt |
| `Cargo.lock` nicht eingecheckt | **Im aktuellen Repo-Stand eingecheckt** (`git ls-files` bestätigt `Cargo.lock`) | Erledigt, nur Reproduzierbarkeits-Regressionscheck |
| `sanitize_prompt_input()` mit trivialer Denylist | Funktion unter diesem Namen im aktuellen Code **nicht auffindbar** — evtl. umbenannt oder Fehleinschätzung Gemini | **Prompt-Injection-Härtung dennoch neu und breiter prüfen** (memfuse-ollama, memfuse-tauri) |

**Schlussfolgerung:** Ein erheblicher Teil der von Gemini benannten Fehlerklassen wurde durch Runde 1 bereits **empirisch bestätigt und behoben** (5× memfuse-store, 3× memfuse-index, 1× memfuse-graph). Das ist ein starkes Signal, dass die verbleibenden, noch nicht mit derselben Tiefe geprüften Hypothesen (Snapshot-Isolation bei Vektor/Graph, PPR-Sink-Holes, Unicode-Panics, CSR-Insert-Komplexität, Zettelkasten-Zyklen, DoS via Kompositazerlegung, fsync-Isolation, mmap-TOCTOU-Race) mit **mindestens derselben Wahrscheinlichkeit reale, noch unentdeckte Bugs sind**. Runde 2 zerlegt daher die fünf komplexesten Crates in fokussierte Sub-Prompts, von denen jeder GENAU EINE dieser Risikoklassen bis zur Erschöpfung verfolgt.

## Format-Hinweis für Google-Jules (Gemini-Basis)

Da Jules auf einem Gemini-Modell basiert, sind die folgenden Prompts konsequent in klar nummerierten, kurzen, unzweideutigen MD-Abschnitten mit expliziten Imperativen ("Führe aus...", "Verifiziere...", "Liefere...") strukturiert, vermeiden verschachtelte Bedingungssätze, und geben JEDES Mal einen exakten Dateinamen für den Ergebnis-Report vor. Jeder Sub-Prompt referenziert explizit die relevante Fundstelle aus dem Runde-1-Report als Kontext-Anker, damit Jules nicht bei Null anfängt.

---

## Wie diese Runde zu benutzen ist

1. Jeder Sub-Prompt ist **eigenständig** ausführbar und für genau **eine Komponente/Risikoklasse** eines Crates formuliert (max. Fokussierung statt eines einzigen breiten Prompts).
2. Reports werden als `docs/audits/round2/AUDIT_<crate>_<thema>.md` erwartet.
3. Reihenfolge innerhalb eines Crates ist wie gelistet abzuarbeiten; Crates untereinander sind parallelisierbar.
4. Jeder Prompt fordert **Regressionstests für bereits behobene Runde-1-Bugs** als Fixpunkt, BEVOR neue Angriffsflächen exploriert werden — damit kein Fix versehentlich durch nachfolgende Änderungen wieder bricht.

---

# A. `memfuse-db` — Dekomposition in 4 Sub-Prompts

## A.1 `memfuse-db` / Zettelkasten-Displacement & Relation-Zyklen

```
ROLLE
Senior Rust Datenbank-Architekt, spezialisiert auf Graph-Constraint-Validierung in relationalen/
dokumentbasierten Systemen. Du vertiefst den bestehenden Audit von `memfuse-db` (siehe
docs/audits/AUDIT_memfuse-db.md, Abschnitt 3-4, Status: Lock-Hierarchie & Fusion bereits BESTANDEN)
um einen von Runde 1 NICHT abgedeckten Risikobereich.

MISSION
Eine unabhängige Zweitmeinungs-Analyse hypothetisiert folgendes Fehlerbild: Wenn Dokument A per
Relation (z.B. `Supersedes`/"ersetzt") auf Dokument B verweist, und B (evtl. durch eine spätere
Operation) wiederum auf A zurückverweist, entsteht ein Zyklus in der Relations-Graph-Struktur. Falls
die Post-RRF-Filterung oder die Graph-Traversierung (`collection/relate.rs`, `memfuse-graph`-Integration)
diesen Zyklus nicht erkennt, kann eine Suchanfrage, die diesem Zyklus folgt, in eine Endlosschleife
geraten (Hänger/Denial-of-Service der gesamten Instanz, da vermutlich unter einem Lock aus der
dokumentierten Lock-Hierarchie ausgeführt).

AUFGABENUMFANG
1. Analysiere `crates/memfuse-db/src/collection/relate.rs` vollständig: identifiziere JEDE Stelle, an
   der beim Einfügen einer Relation (insbesondere "ersetzt"/"supersedes"-artige Relationstypen, falls
   vorhanden — recherchiere den exakten Relationstyp-Katalog im Code, verlasse dich nicht auf die
   Gemini-Vermutung) eine Zyklenprüfung stattfinden könnte oder müsste.
2. Falls KEINE explizite DAG-Validierung beim Einfügen existiert: konstruiere einen minimalen Testfall
   (2 Dokumente A, B mit wechselseitiger Relation; danach 3 Dokumente A→B→C→A) und beobachte das
   tatsächliche Verhalten bei nachfolgender Traversierung/Suche mit einem HARTEN Timeout (z.B. 5
   Sekunden via `tokio::time::timeout`). Ein Timeout-Auslösen gilt als bestätigter Bug.
3. Falls eine Zyklenprüfung existiert: teste sie erschöpfend — 2-Knoten-Zyklus, 3-Knoten-Zyklus,
   10-Knoten-Zyklus, "fast"-Zyklus (der keiner ist, darf NICHT fälschlich abgelehnt werden),
   Selbstreferenz (Dokument verweist auf sich selbst).
4. Prüfe zusätzlich das Verhalten der Relations-API beim LÖSCHEN eines Dokuments, das Teil einer
   Relationskette ist — entstehen "hängende" Relationen (Referenzen auf gelöschte DocIds), und wie
   reagiert die Traversierung darauf (Panic? Endlosschleife durch fehlerhafte Weiterverfolgung? Sauberer
   Stop?).
5. Falls ein echter Hänger/Bug gefunden wird: dokumentiere exakte Reproduktionsschritte, schlage einen
   minimalinvasiven Fix vor (z.B. Visited-Set in der Traversierung, harte Tiefenbegrenzung als
   Sofortmaßnahme), implementiere ihn, und verifiziere mit demselben Testfall, dass der Fix greift.

REPORT: docs/audits/round2/AUDIT_memfuse-db_relation-cycles.md
STRUKTUR: 1) Executive Summary (Bug bestätigt Ja/Nein), 2) Relationstyp-Katalog-Inventar,
3) Zyklen-Testmatrix, 4) Lösch-/Hängende-Referenz-Testergebnisse, 5) Fix-Beschreibung (falls
angewendet) + Vorher/Nachher-Testbeweis, 6) Anhang Rohlogs.

ABNAHMEKRITERIUM
Jeder Traversierungstest MUSS mit explizitem Timeout laufen — ein Test ohne Timeout, der "einfach lange
lief", ist keine gültige Verifikation eines Endlosschleifen-Verdachts.
```

## A.2 `memfuse-db` / Snapshot-Isolation bei Vektor- und Graph-Signalen (Read-Uncommitted-Fenster)

```
ROLLE
Senior Rust Datenbank-Architekt mit Spezialisierung auf MVCC-Isolationslevel-Verifikation.

MISSION
Der Runde-1-Audit hat MVCC/Snapshot-Isolation für den LSM-Pfad (`memfuse-store`) bestätigt und
Lock-Hierarchie-Konformität in `memfuse-db` nachgewiesen — ABER: keiner der bisherigen Reports hat
explizit verifiziert, ob eine laufende 4-Signal-Suche (HNSW + BM25 + CSR-Graph + Filter) tatsächlich
gegen EINEN konsistenten Snapshot über ALLE VIER Signale hinweg läuft, oder ob Vektor-/Graph-Signal
direkt gegen den aktuellen In-Memory-Zustand suchen (kein Pinning), während nur der Text-/LSM-Pfad
snapshot-isoliert ist. Falls letzteres zutrifft, kann eine Suchanfrage inkonsistente Ergebnisse liefern
(ein Dokument erscheint im BM25-Ergebnis in einer alten Version, im HNSW-Ergebnis aber bereits mit
einem parallel committeten Update — "Split-Brain-Read").

AUFGABENUMFANG
1. Analysiere den Suchpfad in `crates/memfuse-db/src/collection/search.rs` und `fusion.rs`: für JEDES
   der 4 Signale, identifiziere exakt, GEGEN WELCHEN Snapshot/Zustand die jeweilige Sub-Engine
   (`memfuse-index::hnsw`, `memfuse-text::bm25`, `memfuse-graph::csr`, Metadaten-Filter) tatsächlich
   liest — wird ein `SnapshotRegistry`-Handle (aus `memfuse-core`) an alle vier durchgereicht, oder
   lesen HNSW/CSR direkt den "aktuellen" In-Memory-Zustand ohne Pinning?
2. Konstruiere folgenden Testfall: Starte eine lang laufende Suchanfrage (künstlich verlangsamt, z.B.
   durch einen Test-Hook oder eine sehr große Collection). WÄHREND diese läuft, führe parallel ein
   Update/Delete auf einem Dokument aus, das Teil des erwarteten Suchergebnisses ist (in mindestens 2
   der 4 Signale relevant, z.B. Vektor- UND Text-Repräsentation ändern sich). Prüfe nach Abschluss der
   Suche: ist das Ergebnis konsistent (alte ODER neue Version durchgängig über alle Signale), oder
   gemischt (Signal A liefert alte Version, Signal B liefert neue Version für dasselbe Dokument)?
3. Wiederhole Test 2 mit 100 parallelen Iterationen (Stress-Variante) um eine niedrige, aber reale
   Race-Window-Wahrscheinlichkeit statistisch zu erfassen (ein einzelner sauberer Lauf beweist nichts).
4. Falls eine Inkonsistenz nachgewiesen wird: klassifiziere Schweregrad (kosmetisch vs. korrektheits-
   kritisch für nachgelagerte Agenten-Entscheidungen) und dokumentiere exakt, welches Signal die
   fehlende Isolation aufweist.
5. Prüfe außerdem das offizielle README/DECISIONS.md-Statement zu Isolationsgarantien — vergleiche die
   dort gemachten Zusicherungen wortwörtlich mit dem empirischen Testergebnis; jede Diskrepanz ist ein
   Dokumentations-Bug, auch wenn kein Korrektheits-Bug vorliegt.

REPORT: docs/audits/round2/AUDIT_memfuse-db_cross-signal-isolation.md
STRUKTUR: 1) Executive Summary (Isolationslücke bestätigt Ja/Nein, pro Signal), 2) Code-Pfad-Analyse
(Snapshot-Handle-Weiterreichung pro Signal, mit Zeilenverweisen), 3) Race-Test-Ergebnisse (Einzellauf +
100-Iterationen-Stress), 4) Dokumentations-Abgleich, 5) Empfohlener Fix (Snapshot-Pinning-Vorschlag
falls Lücke bestätigt), 6) Anhang Rohlogs.

ABNAHMEKRITERIUM
Eine Aussage "Isolation ist gegeben" ist NUR gültig, wenn sie durch den 100-Iterationen-Stresstest
gedeckt ist, nicht durch reinen Code-Review.
```

## A.3 `memfuse-db` / 2PC-Fehlerpfad-Erschöpfung & Kompensations-Rollback unter Fault-Injection

```
ROLLE
Senior Rust Transaktionssystem-Ingenieur mit Fokus auf Fault-Injection-Testing verteilter Commits.

MISSION
Runde 1 hat den Happy-Path und EINEN Fehlerfall (HNSW-Staging-Fehler) der 2PC-Kompensationslogik
bestätigt (`test_4_index_atomic_rollback_on_vector_failure`, siehe AUDIT_memfuse-db.md Abschnitt 4).
Ein einziger getesteter Fehlerpfad bei VIER beteiligten Sub-Engines (HNSW, BM25/Text, CSR-Graph, LSM)
ist bei Weitem nicht erschöpfend. Deine Mission: erzwinge einen Fehler an JEDER der vier möglichen
Stufen des 2PC-Ablaufs, einzeln UND in Kombination, und beweise vollständige Kompensation in jedem Fall.

AUFGABENUMFANG
1. Analysiere `crates/memfuse-db/src/transaction.rs` und identifiziere die exakte Reihenfolge, in der
   die vier Sub-Engines innerhalb einer Insert/Update/Delete-Transaktion angesprochen werden (Staging-
   Reihenfolge und Commit-Reihenfolge — sind diese identisch oder unterschiedlich?).
2. Baue Test-Double/Fault-Injection-Hooks (z.B. via Trait-Mocking oder gezielte Fehler-Rückgabe an
   definierten Injection-Punkten), die einen kontrollierten Fehler auslösen an JEDER der folgenden
   Stufen (mindestens):
   a) HNSW-Staging schlägt fehl (bereits durch Runde 1 abgedeckt — als Regressionstest wiederholen).
   b) BM25/Text-Staging schlägt fehl NACHDEM HNSW-Staging bereits erfolgreich war.
   c) CSR-Graph-Staging schlägt fehl NACHDEM HNSW+Text bereits erfolgreich waren.
   d) LSM-Commit (finale Persistenz-Stufe) schlägt fehl NACHDEM alle drei Such-Signale bereits
      committed/gestaged wurden (kritischster Fall — Rollback muss ALLE drei bereits abgeschlossenen
      Signal-Schreibvorgänge zurücknehmen).
   e) Ein simulierter Prozessabsturz GENAU zwischen Stufe (c) und (d) — verifiziere über `repair_on_open`
      (bereits in Runde 1 für einen Fall getestet — hier mit allen vier möglichen Absturzpunkten
      wiederholen).
3. Für JEDEN der 5 Fälle: verifiziere nach dem simulierten Fehler/Absturz, dass die Collection in einem
   vollständig konsistenten Zustand ist — kein Dokument ist in einem Signal sichtbar und in einem
   anderen nicht (partielle Sichtbarkeit über Signale hinweg gilt als kritischer Bug).
4. Teste zusätzlich Multi-Dokument-Batch-Transaktionen (`insert_many`) mit Fehler beim 50%-Punkt des
   Batches — Ist die Semantik All-or-Nothing für den GESAMTEN Batch, oder werden bereits erfolgreiche
   Einzeldokumente committed (Best-Effort)? Verifiziere, welches Verhalten TATSÄCHLICH implementiert
   ist, und ob dies mit der Dokumentation übereinstimmt.

REPORT: docs/audits/round2/AUDIT_memfuse-db_2pc-fault-injection.md
STRUKTUR: 1) Executive Summary, 2) Staging-/Commit-Reihenfolge-Diagramm (aus Code extrahiert),
3) Fault-Injection-Testmatrix (5 Fälle × Konsistenz-Ergebnis), 4) Batch-Transaktions-Semantik-Befund,
5) Priorisierte Bugliste, 6) Anhang Rohlogs.

ABNAHMEKRITERIUM
Alle 5 Fehlerinjektionspunkte müssen einzeln nachgewiesen sein — "sollte analog funktionieren" ist
keine gültige Ersatzverifikation für einen nicht tatsächlich ausgeführten Testfall.
```

## A.4 `memfuse-db` / RRF-Performance unter Realistischer Ergebnismenge (Eager-Materialization-Verdacht)

```
ROLLE
Senior Rust Performance-Ingenieur, Fokus Speicherkomplexität von Ranking-Algorithmen.

MISSION
Runde 1 hat RRF-Latenz bei kleinen synthetischen Datensätzen gemessen (~12,78 µs End-to-End-Latenz),
aber keinen Benchmark bei GROSSEN Einzelsignal-Trefferzahlen (z.B. 100.000 Treffer pro Signal vor
Fusion) durchgeführt. Die Gemini-Analyse hypothetisiert, dass eine naive RRF-Implementierung ALLE
Treffer jedes Signals vollständig materialisiert und sortiert (`Vec::sort_by` über die volle Menge)
statt Top-K-Vorfilterung zu nutzen, was bei großen Collections zu linear explodierendem Speicher-
/Latenz-Verhalten führt.

AUFGABENUMFANG
1. Analysiere `crates/memfuse-db/src/fusion.rs`: bestimmt die Implementierung, wie viele Elemente
   JEDES Signal VOR der Fusion zurückliefert (unbegrenzt oder bereits Top-K-begrenzt durch die
   jeweilige Sub-Engine?), und ob die Fusion selbst auf der VOLLEN Menge oder einer bereits
   reduzierten Menge arbeitet.
2. Baue eine Test-Collection mit 100.000 / 500.000 Dokumenten (soweit VM-Ressourcen erlauben, sonst mit
   der größtmöglichen praktikablen Größe, und extrapoliere transparent).
3. Führe eine breite Suchanfrage aus, die in JEDEM der 4 Signale eine große Trefferzahl erzeugt (z.B.
   ein sehr häufiger BM25-Term, ein Vektor nahe dem Zentroid der Verteilung für viele HNSW-Nachbarn),
   und miss: (a) Peak-RSS-Speicherverbrauch während der Fusion, (b) End-to-End-Latenz, (c) ob die
   Latenz linear, superlinear oder konstant mit der Trefferzahl pro Signal skaliert (Messung bei
   1K/10K/100K/500K simulierter Trefferzahl pro Signal, falls die Sub-Engines dies überhaupt zulassen —
   andernfalls dokumentiere die tatsächliche harte Obergrenze als Ergebnis).
4. Falls superlineares Wachstum nachgewiesen wird: identifiziere die exakte Codezeile der
   Vollständig-Materialisierungs-/Sortierlogik und schlage eine Top-K-Heap-basierte Alternative vor
   (nicht zwingend implementieren, aber technisch konkret spezifizieren).

REPORT: docs/audits/round2/AUDIT_memfuse-db_rrf-scaling.md
STRUKTUR: 1) Executive Summary, 2) Code-Pfad-Analyse (volle vs. Top-K-Materialisierung),
3) Skalierungs-Benchmark-Tabelle (Trefferzahl × Latenz × Peak-RSS), 4) Wachstumsklassen-Einordnung
(konstant/linear/superlinear, mit Kurven-Fit falls möglich), 5) Verbesserungsvorschlag,
6) Anhang Rohlogs/Diagrammdaten.

ABNAHMEKRITERIUM
Die Skalierungsaussage muss auf mindestens 4 gemessenen Datenpunkten unterschiedlicher Größenordnung
beruhen, nicht auf Extrapolation aus einem einzigen Messpunkt.
```

---

# B. `memfuse-index` — Dekomposition in 4 Sub-Prompts

## B.1 `memfuse-index` / CPU-Feature-Detection-Vollständigkeit (Illegal-Instruction-Risiko)

```
ROLLE
Senior Rust Low-Level-Performance-Ingenieur, Spezialgebiet Hardware-Dispatch und Cross-CPU-Portabilität.

MISSION
Runde 1 (`AUDIT_memfuse-index.md`) hat drei kritische Bugs behoben (DiskANN-Recall, zwei mmap-Slice-
Panics), aber CPU-Feature-Detection (`is_x86_feature_detected!`) wurde nicht als expliziter,
eigenständiger Prüfpunkt behandelt. Ein fehlender oder unvollständiger Feature-Check vor Ausführung
eines AVX-512/AVX2-Intrinsic-Blocks führt bei Ausführung auf einer CPU ohne diese Extension zu einer
"Illegal Instruction"-Signal (SIGILL) — einem harten Prozessabsturz, den Rust's Panic-Handling NICHT
abfangen kann (kein `catch_unwind` wirksam, da es sich um ein OS-Signal, nicht um einen Rust-Panic
handelt).

AUFGABENUMFANG
1. Durchsuche `crates/memfuse-index/src/distance.rs` vollständig nach JEDEM Vorkommen eines
   `unsafe`-Blocks, der AVX-512- oder AVX2-Intrinsics (`_mm512_*`, `_mm256_*`) verwendet. Erstelle eine
   vollständige Liste: Zeile | Intrinsic-Familie | umgebende Feature-Check-Bedingung (falls vorhanden).
2. Für JEDEN gefundenen Intrinsic-Block: verifiziere, dass er NUR erreichbar ist, wenn zuvor
   `is_x86_feature_detected!("avx512f")` bzw. `"avx2"` (oder das jeweils korrekte jeweilige Feature-Flag
   — exakt prüfen, nicht raten) zur LAUFZEIT (nicht nur zur Kompilierzeit via `target_feature`) geprüft
   wurde, UND dass bei negativem Ergebnis zuverlässig auf den Skalar-Pfad zurückgefallen wird.
3. Ermittle die CPU-Features der Jules-VM (`cat /proc/cpuinfo | grep flags`, `lscpu`) und dokumentiere
   sie vollständig im Report — dies bestimmt, welche Pfade in dieser Umgebung überhaupt real getestet
   werden können.
4. Falls die VM AVX-512 NICHT unterstützt (wahrscheinlich in vielen Cloud-VMs): teste zusätzlich, ob
   der Code bei einer VM, die zwar AVX2 aber NICHT AVX-512 hat, korrekt auf AVX2 statt AVX-512
   dispatcht (nicht versucht, AVX-512-Instruktionen auszuführen) — dies ist der in dieser Sandbox
   tatsächlich verifizierbare Grenzfall und MUSS abgedeckt werden.
5. Falls möglich (Feature-Flags des Rust-Compilers erlauben dies teilweise): baue eine Cross-Compilation
   oder ein Laufzeit-Feature-Masking-Experiment (z.B. `RUSTFLAGS` ohne `target-feature=+avx2` UND einen
   Test, der `std::arch::is_x86_feature_detected!` künstlich false zurückgeben lässt, falls die API dies
   über ein Test-Double erlaubt) um explizit zu verifizieren, dass der Skalar-Fallback-Pfad bei
   deaktivierter Hardware-Erkennung fehlerfrei denselben Korrektheitsstandard wie der SIMD-Pfad liefert
   (Cross-Referenz zu den bereits in Runde 1 verifizierten SIMD-vs-Skalar-Korrektheitsdaten).
6. Dokumentiere ausdrücklich als Executive-Summary-Punkt: "Auf dieser Test-VM WURDE / WURDE NICHT der
   AVX-512-Pfad tatsächlich ausgeführt" — eine unvollständige Testabdeckung mangels VM-Hardware ist
   KEIN Fehlschlag des Audits, muss aber transparent als Lücke benannt werden, NICHT verschwiegen.

REPORT: docs/audits/round2/AUDIT_memfuse-index_cpu-feature-detection.md
STRUKTUR: 1) Executive Summary inkl. VM-Hardware-Deckungslücken-Statement, 2) Vollständiges
Intrinsic-Block-Inventar mit Feature-Check-Nachweis (Tabelle), 3) VM-CPU-Feature-Dump, 4) Fallback-Pfad-
Verifikation, 5) Priorisierte Bugliste (JEDER Intrinsic-Block ohne nachweisbaren Runtime-Check ist ein
CRITICAL-Fund), 6) Anhang Rohlogs.

ABNAHMEKRITERIUM
Jeder einzelne Intrinsic-Block ohne nachweisbaren, unmittelbar umgebenden Runtime-Feature-Check gilt
als unverifiziert und ist als CRITICAL zu melden, unabhängig davon, ob er in der Praxis "wahrscheinlich"
sicher ist.
```

## B.2 `memfuse-index` / mmap-TOCTOU-Race unter aktivem Dateizugriff (DiskANN)

```
ROLLE
Senior Rust Systems Engineer, Spezialgebiet Memory-Mapped-I/O-Sicherheit.

MISSION
Runde 1 hat zwei mmap-bezogene Panics behoben (Bug-02, Bug-03: Direct-Slice-Indexing bei bereits
verkürzten Dateien, JETZT auf sicheres `.get()` umgestellt). Das behebt den STATISCHEN Fall (Datei ist
BEREITS beim Öffnen zu kurz). NICHT getestet wurde der DYNAMISCHE Fall: eine Datei wird VERKÜRZT oder
GELÖSCHT, WÄHREND ein aktives mmap darauf besteht (Time-of-Check-to-Time-of-Use). Dies ist ein
Betriebssystem-Ebene-Risiko, das durch reine `.get()`-Bounds-Checks im Rust-Code NICHT abgedeckt wird,
da das OS bei Zugriff auf eine ge-mmap-te, aber nachträglich verkürzte Datei je nach Plattform ein
SIGBUS-Signal auslöst (nicht abfangbar durch normales Rust-Error-Handling).

AUFGABENUMFANG
1. Analysiere den Lebenszyklus des Mmap-Handles in `crates/memfuse-index/src/diskann.rs` und
   `persistence.rs`: wird die zugrunde liegende Datei nach dem `mmap()`-Aufruf jemals von einem anderen
   Teil des Systems (Compaction, Reindexierung, Nutzer-Löschung) modifiziert, während der Index aktiv im
   Speicher gemappt bleibt?
2. Konstruiere einen kontrollierten Test: öffne einen DiskANN-Index (mmap aktiv), starte eine
   Hintergrund-Task, die die zugrunde liegende Datei nach einer kurzen Verzögerung TRUNKIERT (mit
   `std::fs::File::set_len()` auf eine kleinere Größe), während gleichzeitig aus einem anderen Task
   aktiv Suchanfragen gegen den gemappten Index laufen. Beobachte das Prozessverhalten (sauberer Fehler,
   Hänger, oder Prozessabsturz via SIGBUS).
3. Prüfe, ob im Code irgendein Schutzmechanismus existiert (Exklusiv-Lock auf Datei-Ebene während aktiver
   mmap-Nutzung, Datei-Versionierung/Copy-on-Write, Advisory-Locks via `flock`) — falls JA, verifiziere
   dessen Wirksamkeit mit demselben Testaufbau; falls NEIN, ist dies ein bestätigter struktureller Fund.
4. Wiederhole den Test mit vollständigem Löschen der Datei statt Truncation (auf Linux bleibt die Datei
   durch den offenen File-Descriptor typischerweise bestehen bis Unmap — verifiziere dieses Verhalten
   für diese konkrete Codebasis empirisch, nicht nur theoretisch).
5. Dokumentiere das Ergebnis mit maximaler Präzision, da ein SIGBUS-Crash in einer Produktivumgebung dem
   kompletten Ausfall des air-gapped Agentensystems gleichkäme — dies ist einer der höchsten
   Risikopunkte des gesamten Audits.

REPORT: docs/audits/round2/AUDIT_memfuse-index_mmap-toctou.md
STRUKTUR: 1) Executive Summary mit explizitem Risiko-Verdikt, 2) Mmap-Lebenszyklus-Analyse (Code-
Pfade), 3) TOCTOU-Race-Testergebnis (Truncation-Szenario), 4) TOCTOU-Race-Testergebnis (Deletion-
Szenario), 5) Vorhandene Schutzmechanismen-Bewertung, 6) Konkreter Absicherungsvorschlag (z.B.
Advisory-Lock, Referenzzählung, Copy-on-Compact-Strategie), 7) Anhang Rohlogs (inkl. Signal-/Exit-Codes
bei Crash, falls aufgetreten).

ABNAHMEKRITERIUM
Falls ein Prozessabsturz reproduziert wird, MUSS dies als CRITICAL-Fund unabhängig von der subjektiven
Eintrittswahrscheinlichkeit im echten Betrieb gemeldet werden — "unwahrscheinlich, dass ein Nutzer
gleichzeitig löscht" ist keine gültige Abwertung eines reproduzierten Crashs.
```

## B.3 `memfuse-index` / Kompositazerlegungs-Root-Cause via Morphologie-Grenzfall (Cross-Crate: memfuse-text-Bezug für Recall-Wirkung)

```
ROLLE
Senior Rust Search-Quality-Ingenieur — dieser Sub-Prompt ist bewusst hier bei memfuse-index verortet
(nicht memfuse-text), weil untersucht wird, wie ein Text-seitiger Recall-Verlust sich auf die
4-Signal-Fusion-Gesamtqualität auswirkt, für die memfuse-index das dominante Signal liefert. Für die
reine morphologische Root-Cause-Analyse siehe den korrespondierenden Sub-Prompt C.3 unter memfuse-text.

MISSION
Der Runde-1-Text-Audit (AUDIT_memfuse-text.md) fand: 41/45 deutsche Komposita korrekt zerlegt, aber 3
lange/komplexe Komposita (`donaudampfschifffahrtsgesellschaftskapitaen`, `softwareentwicklungskontext`,
`systemadministrator`) blieben UNGESPLITTET. Deine Aufgabe hier: quantifiziere den TATSÄCHLICHEN
Retrieval-Qualitätsschaden dieses Bugs im Kontext der 4-Signal-Fusion — ist dies rein kosmetisch (weil
HNSW/Vektorsuche den Bedeutungsverlust kompensiert), oder korrektheits-kritisch (weil bei rein
lexikalischen/Fachbegriff-Anfragen ohne guten Vektor-Treffer der BM25-Ausfall das Gesamtergebnis
sichtbar verschlechtert)?

AUFGABENUMFANG
1. Baue ein synthetisches Test-Corpus von 20 Dokumenten, von denen jeweils mehrere die 3 bekannt
   fehlerhaften Komposita (und mind. 5 weitere lange, neu konstruierte Komposita zur Verallgemeinerung,
   z.B. Fachbegriffe aus Recht/Finance/IT) in unterschiedlichem Kontext enthalten.
2. Führe End-to-End-Hybridsuchen über `memfuse-db` mit Queries durch, die exakt nach Teilbegriffen
   dieser Komposita suchen (z.B. Query "Kapitän" soll das Dokument mit
   "donaudampfschifffahrtsgesellschaftskapitaen" finden), und miss: erscheint das relevante Dokument
   überhaupt im Top-10-Ergebnis, und an welcher Rang-Position — mit und ohne (falls über Konfiguration
   deaktivierbar) BM25-Signal-Beitrag.
3. Quantifiziere den Rangverlust: Rang-Position mit funktionierendem Split vs. Rang-Position bei
   tatsächlichem (fehlerhaftem) Verhalten — falls der Split über eine Testkonfiguration erzwungen werden
   kann, andernfalls über einen manuell vor-gesplitteten Vergleichsindex.
4. Bewerte auf dieser empirischen Basis den Schweregrad NEU (nicht nur "Morphologie-Bug", sondern
   "Morphologie-Bug mit gemessenem Recall-Impact von X Rangplätzen bei Y% der Testqueries").

REPORT: docs/audits/round2/AUDIT_memfuse-index_compound-split-recall-impact.md
STRUKTUR: 1) Executive Summary mit quantifiziertem Impact, 2) Testcorpus-Beschreibung,
3) Rang-Positions-Vergleichstabelle (Query × Rang-mit-Split × Rang-ohne-Split × Delta),
4) Neubewertung des Schweregrads, 5) Empfehlung (Priorität für Fix in memfuse-text hoch/mittel/niedrig
basierend auf gemessenem Impact), 6) Anhang Rohlogs.

ABNAHMEKRITERIUM
Die Schweregrad-Neubewertung muss auf gemessenen Rangpositionsdaten beruhen, nicht auf einer
qualitativen Einschätzung.
```

## B.4 `memfuse-index` / HNSW Fine-Grained-Locking-Verdacht unter Schreiblast (Insert-Skalierung)

```
ROLLE
Senior Rust Concurrency-Ingenieur, Spezialgebiet Lock-Granularität in Graph-Datenstrukturen.

MISSION
Runde 1 hat HNSW-Korrektheit (Recall) und Concurrency-Stresstests ohne Panics bestätigt, aber KEINE
explizite Messung der Schreibdurchsatz-Skalierung bei steigender Parallelität durchgeführt. Die
Gemini-Analyse hypothetisiert, dass ein grobgranularer globaler `RwLock` über den gesamten HNSW-Graphen
(statt feingranularem Node-Level-Locking) den Schreibdurchsatz bei parallelen Inserts auf
Single-Thread-Niveau degradieren lässt — ein reiner Performance-Verdacht, der aber bei einem
"produktionsreif verifizierten" Kern-Suchindex laut README eine belastbare empirische Antwort verdient.

AUFGABENUMFANG
1. Analysiere `crates/memfuse-index/src/hnsw.rs`: identifiziere die exakte Lock-Granularität beim
   Insert-Pfad (ein einziger Lock über die gesamte Graphstruktur? Pro-Layer-Locks? Pro-Knoten-Locks?
   Optimistic-Concurrency ohne Lock?).
2. Miss den Insert-Durchsatz (Inserts/Sekunde) bei 1, 2, 4, 8, 16 parallelen Tokio-Tasks (oder OS-Threads,
   je nachdem was die Implementierung tatsächlich nutzt), auf einem FIXEN Ausgangsindex von z.B. 50.000
   bereits vorhandenen Vektoren, jeweils N=1.000 zusätzliche parallele Inserts pro Lauf.
3. Berechne den Skalierungsfaktor: Durchsatz(N Threads) / Durchsatz(1 Thread) — ein Wert nahe 1
   unabhängig von N bestätigt den Global-Lock-Verdacht (keine Parallelisierung); ein Wert nahe N (bis zu
   Diminishing Returns durch CPU-Kernanzahl) widerlegt ihn.
4. Falls der Verdacht bestätigt wird: identifiziere im Code die exakte Lock-Scope-Grenze, die dies
   verursacht, und schätze den Aufwand/die Machbarkeit einer feingranulareren Sperr-Strategie ab (ohne
   sie zwingend zu implementieren, aber mit konkretem technischem Vorschlag, z.B. Sharding der
   Graphknoten auf mehrere unabhängige Locks, oder lock-freie Skip-List-Struktur pro Layer).
5. Dokumentiere zusätzlich das Verhalten bei GEMISCHTER Last (parallele Inserts UND parallele Suchen
   gleichzeitig) — verschlechtert sich die Such-Latenz messbar während hoher Insert-Last (Lock-
   Contention zwischen Lesern und Schreibern)?

REPORT: docs/audits/round2/AUDIT_memfuse-index_hnsw-lock-granularity.md
STRUKTUR: 1) Executive Summary (Global-Lock-Verdacht bestätigt/widerlegt, mit Skalierungsfaktor),
2) Lock-Granularitäts-Code-Analyse, 3) Skalierungs-Benchmark-Tabelle (Threads × Durchsatz × Faktor),
4) Gemischte-Last-Ergebnisse, 5) Verbesserungsvorschlag (falls zutreffend), 6) Anhang Rohlogs/Diagrammdaten.

ABNAHMEKRITERIUM
Die Skalierungsaussage muss auf mindestens 5 Messpunkten (1/2/4/8/16 Threads) beruhen; ein einzelner
Vergleich (1 vs. N) ist nicht ausreichend, um lineares vs. sub-lineares Wachstum zu unterscheiden.
```

---

# C. `memfuse-store` — Dekomposition in 4 Sub-Prompts

## C.1 `memfuse-store` / fsync-Durability-Isolationstest (Silent-Data-Loss-Hypothese)

```
ROLLE
Senior Rust Storage-Engine-Ingenieur, Spezialgebiet POSIX-Durability-Garantien und Crash-Consistency.

MISSION
Runde 1 hat 5 kritische Bugs in Compaction/MemTable/SSTable/Snapshot-Pfad gefunden und behoben sowie
Hard-Process-Kill-Simulationen und 9.712 Bit-Flip-Fault-Injections durchgeführt (siehe
AUDIT_memfuse-store.md, Executive Summary) — ein beeindruckendes Ergebnis. ABER: keiner der
dokumentierten Testfälle beschreibt EXPLIZIT einen isolierten fsync-Verifikationstest im Sinne von "wird
`fsync`/`sync_data` tatsächlich VOR der Erfolgsmeldung an den Client aufgerufen, oder landet der
WAL-Eintrag nur im OS-Page-Cache?". Dies ist eine Unterscheidung, die selbst ein Hard-Process-Kill NICHT
zwingend aufdeckt (ein Kill des Rust-Prozesses lässt den OS-Page-Cache meist noch bestehen — nur ein
tatsächlicher Stromausfall/Kernel-Crash würde das Fehlen von fsync sichtbar machen). Dein Auftrag: prüfe
GEZIELT die fsync-Aufruf-Disziplin selbst, nicht nur deren Endergebnis nach Prozess-Kill.

AUFGABENUMFANG
1. Durchsuche `crates/memfuse-store/src/wal.rs` nach JEDEM Aufruf von `sync_all()`, `sync_data()`, oder
   äquivalenten Low-Level-Aufrufen (`libc::fsync`, `File::sync_all`). Erstelle ein vollständiges
   Inventar: Zeile | Aufrufart | wird VOR oder NACH der Erfolgsrückmeldung an den aufrufenden Client-Code
   ausgeführt?
2. Verifiziere durch Code-Lesen die exakte Reihenfolge: `write()` → `fsync()` → `Result::Ok`-Rückgabe an
   Aufrufer, ODER `write()` → `Result::Ok`-Rückgabe → (später/asynchron/gar nicht) `fsync()`. Letzteres
   wäre ein bestätigter Durability-Bug unabhängig vom Ergebnis jedes Crash-Tests.
3. Baue einen Test mit `strace`/`ltrace` (falls in der Sandbox verfügbar) oder alternativ einen
   Test-Hook/Instrumentierungspunkt im Testcode, der die tatsächliche Aufrufreihenfolge von
   `write`-Syscalls und `fsync`-Syscalls für eine WAL-Append-Operation protokolliert und verifiziert,
   dass `fsync` GARANTIERT vor der `commit()`-Erfolgsrückgabe erfolgt ist.
4. Prüfe außerdem, ob `fsync` KONFIGURIERBAR ist (z.B. ein Performance-Modus, der `fsync` überspringt) —
   falls ja, verifiziere den DEFAULT-Wert dieser Konfiguration und ob dieser Default sicher ist
   (produktionssicher = fsync aktiv per Default).
5. Miss zusätzlich den Performance-Overhead von fsync (Vergleich Append-Durchsatz MIT vs. OHNE fsync,
   falls deaktivierbar) — dies liefert dem Auftraggeber eine informierte Kosten-Nutzen-Abwägung.

REPORT: docs/audits/round2/AUDIT_memfuse-store_fsync-durability.md
STRUKTUR: 1) Executive Summary (fsync-Disziplin bestätigt korrekt/fehlerhaft), 2) Vollständiges
fsync-Aufruf-Inventar, 3) Reihenfolge-Verifikation (write→fsync→Ok Nachweis), 4) Syscall-Trace-Ergebnis
(falls strace verfügbar) oder Alternative-Instrumentierungs-Ergebnis, 5) Konfigurierbarkeits-Analyse
inkl. Default-Sicherheits-Bewertung, 6) Performance-Overhead-Messung, 7) Anhang Rohlogs.

ABNAHMEKRITERIUM
Eine reine Aussage "Crash-Test war erfolgreich" ist NICHT ausreichend als Nachweis korrekter
fsync-Disziplin — der syscall-Reihenfolge-Nachweis (Punkt 3) ist der eigentliche Kernbeweis dieses Audits.
```

## C.2 `memfuse-store` / HMAC-Truncation-Attack & Metadaten-Bindung im WAL (Cross-Crate mit memfuse-crypto)

```
ROLLE
Senior Rust Security-Ingenieur, Spezialgebiet Authenticated-Encryption-Integration in Storage-Engines.

MISSION
Der bereits durchgeführte `memfuse-crypto`-Audit (Runde 1, siehe docs/audits/AUDIT_memfuse-crypto.md)
prüfte die kryptographischen Primitiven isoliert. Die Gemini-Analyse äußert eine spezifischere Sorge:
falls der HMAC/Anti-Tamper-Schutz im WAL NUR über den Payload-Inhalt berechnet wird, NICHT aber über
Block-Metadaten wie Länge und Sequenznummer, können vollständige, aber gültig-signierte WAL-Blöcke
UMSORTIERT oder am ENDE ABGESCHNITTEN werden (Truncation-Attack), ohne dass die HMAC-Prüfung dies
erkennt — jeder einzelne Block bleibt für sich betrachtet "gültig signiert", nur die Reihenfolge/
Vollständigkeit der Sequenz wird nicht geschützt.

AUFGABENUMFANG
1. Analysiere `crates/memfuse-crypto/src/anti_tamper.rs` UND die Aufrufstelle in
   `crates/memfuse-store/src/wal.rs` GEMEINSAM: was genau fließt exakt in die HMAC-Eingabe ein? Nur der
   Payload? Payload + Länge? Payload + Länge + Sequenznummer/Position? Payload + Länge + Sequenznummer +
   vorheriger-Block-HMAC (Chaining)?
2. Konstruiere folgenden Angriffstest: schreibe eine gültige WAL-Sequenz aus mindestens 5 Blöcken.
   Manipuliere DANACH die Datei auf Byte-Ebene außerhalb der normalen API:
   a) Vertausche Block 2 und Block 4 (beide bleiben für sich gültig signiert) — muss bei Recovery/Read
      erkannt werden.
   b) Entferne den letzten Block komplett (Truncation) — muss erkannt werden, sofern die Semantik
      "vollständige committete Sequenz" verspricht.
   c) Dupliziere Block 3 (kopiere ihn ein zweites Mal in die Sequenz ein, sog. Replay innerhalb
      derselben Datei) — muss erkannt werden.
   d) Extrahiere Block 3 aus dieser WAL-Datei und füge ihn in eine ANDERE, aber strukturell ähnliche
      WAL-Datei (unterschiedliche Datei-ID/Encryption-Kontext) ein — Cross-File-Replay, muss erkannt
      werden (Bezug zur Key-Separation-pro-Datei-Invariante aus dem memfuse-crypto-Audit).
3. Für JEDEN der 4 Angriffe (a-d): führe den Recovery-/Lesepfad aus und dokumentiere exakt, ob er
   erkannt und abgelehnt wird, STILL FEHLSCHLÄGT (Datenverlust ohne Fehlermeldung), oder — im
   schlimmsten Fall — STILL AKZEPTIERT wird (manipulierte/vertauschte/wiederholte Daten werden als
   gültig geladen).
4. Falls eine Lücke gefunden wird: schlage konkret vor, welche zusätzlichen Felder in die HMAC-Eingabe
   integriert werden müssten (Sequenznummer, Datei-eindeutige-ID, Block-Position, HMAC-Chaining zum
   Vorgänger-Block), um den jeweiligen Angriff zu vereiteln.

REPORT: docs/audits/round2/AUDIT_memfuse-store_wal-hmac-binding.md
STRUKTUR: 1) Executive Summary mit Sicherheits-Verdikt, 2) HMAC-Eingabe-Zusammensetzungs-Analyse
(Code-Beleg), 3) Angriffs-Testmatrix (a-d, jeweils mit Ergebnis: erkannt/still fehlgeschlagen/still
akzeptiert), 4) Konkreter Härtungsvorschlag, 5) Anhang Rohlogs inkl. Hex-Dumps der manipulierten Bereiche.

ABNAHMEKRITERIUM
"Still akzeptiert" bei irgendeinem der 4 Angriffe ist ein CRITICAL-Sicherheitsfund und muss als solcher
unabhängig von der praktischen Ausnutzbarkeit im Air-Gapped-Kontext gemeldet werden.
```

## C.3 `memfuse-store` / Write-Amplification & Bloom-Filter-Vorhandensein unter realistischem Workload

```
ROLLE
Senior Rust Storage-Performance-Ingenieur, Spezialgebiet LSM-Tree-Tuning.

MISSION
Runde 1 hat Korrektheits-Bugs behoben, aber keine dedizierte Write-Amplification-Messung unter einem
REALISTISCHEN Workload (Mix aus Insert/Update/Delete über Zeit mit mehreren Compaction-Zyklen)
durchgeführt, und nicht explizit verifiziert, ob Bloom-Filter für Punktabfragen (Point-Lookups)
überhaupt existieren — deren Fehlen laut Gemini-Hypothese zu Read-Amplification führt (jede
Punktabfrage muss potenziell ALLE SSTables lesen).

AUFGABENUMFANG
1. Durchsuche `crates/memfuse-store/src/sstable.rs` und `compaction.rs` nach Bloom-Filter-Strukturen
   (Suchbegriffe: "bloom", "Bloom", ggf. verwandte probabilistische Filterstrukturen wie Cuckoo-Filter).
   Falls VORHANDEN: gehe zu Schritt 2. Falls NICHT vorhanden: dokumentiere dies als bestätigten
   Performance-Fund und quantifiziere den Impact in Schritt 3 direkt (jede Punktabfrage liest
   nachweislich alle relevanten SSTables).
2. Falls Bloom-Filter vorhanden: miss die empirische False-Positive-Rate über einen Datensatz von
   mindestens 100.000 Schlüsseln (bekannte vorhandene Schlüssel als True-Positives, 100.000 bekannt NICHT
   vorhandene Schlüssel zur False-Positive-Messung) und vergleiche gegen die theoretisch konfigurierte
   Ziel-Rate.
3. Baue einen realistischen Workload-Simulator: 100.000 Inserts, gefolgt von 20.000 Updates (Overwrite
   bestehender Schlüssel) und 10.000 Deletes, verteilt über mehrere Batches mit dazwischenliegenden
   expliziten oder automatisch getriggerten Compaction-Zyklen (mind. 3 Zyklen). Miss: (a) Summe aller
   tatsächlich auf Disk geschriebenen Bytes über den gesamten Workload, (b) Summe der logisch
   gespeicherten (finalen) Bytes am Ende — Write-Amplification-Faktor = (a)/(b).
4. Miss zusätzlich Read-Amplification für Punktabfragen: für 1.000 zufällige Punktabfragen nach dem
   obigen Workload, zähle die tatsächliche Anzahl gelesener SSTable-Blöcke/-Dateien pro Abfrage
   (instrumentieren, falls keine eingebaute Metrik existiert) und bilde den Durchschnitt.
5. Vergleiche beide Faktoren gegen Literatur-Referenzwerte für produktionsreife LSM-Engines (RocksDB:
   Write-Amplification typischerweise 10-30x je nach Konfiguration, Read-Amplification mit Bloom-Filtern
   nahe 1 pro relevanter Ebene) und ordne das Ergebnis ein.

REPORT: docs/audits/round2/AUDIT_memfuse-store_write-read-amplification.md
STRUKTUR: 1) Executive Summary (Bloom-Filter vorhanden Ja/Nein, WA-/RA-Faktoren), 2) Bloom-Filter-
Code-Inventar + False-Positive-Rate-Messung (falls vorhanden), 3) Write-Amplification-Workload-Ergebnis,
4) Read-Amplification-Ergebnis, 5) Literatur-Vergleich & Einordnung, 6) Optimierungsvorschläge,
7) Anhang Rohlogs.

ABNAHMEKRITERIUM
Die Write-Amplification-Zahl muss auf tatsächlich gezählten Bytes (nicht geschätzt) über einen
mehrstufigen Compaction-Workload beruhen.
```

## C.4 `memfuse-store` / Async-I/O-Blocking-Verifikation (Executor-Starvation-Risiko)

```
ROLLE
Senior Rust Async-Runtime-Ingenieur, Spezialgebiet Tokio-Executor-Verhalten unter Blocking-I/O.

MISSION
Die Gemini-Analyse hypothetisiert: falls Datei-I/O-Operationen (insbesondere Block-Level-Random-Access
auf SSTables) direkt in der Tokio-Async-Runtime ausgeführt werden statt korrekt via
`tokio::task::spawn_blocking` ausgelagert zu werden, blockiert dies den GESAMTEN Async-Executor
(inklusive des MCP-Servers `memfuse-mcp`, der auf demselben Runtime-Kontext läuft), was dazu führt, dass
das System unter I/O-Last nicht mehr auf externe Anfragen reagiert. Der FILE-CONTEXT-Kommentar in
`crates/memfuse-store/src/lib.rs` behauptet explizit die Einhaltung dieser Regel ("tokio::fs für
Metadaten/Lifecycle, std::fs::File ausschließlich innerhalb spawn_blocking für Block-Level Random-
Access") — dies wurde bisher NICHT empirisch verifiziert, nur als Doku-Statement übernommen.

AUFGABENUMFANG
1. Durchsuche das GESAMTE `memfuse-store`-Crate nach JEDEM Vorkommen von `std::fs::File`-Operationen
   (`read`, `write`, `seek`, etc.) UND verifiziere für JEDES Vorkommen, ob es sich innerhalb eines
   `tokio::task::spawn_blocking`-Closures befindet. Erstelle eine vollständige Tabelle: Datei:Zeile |
   Operation | innerhalb spawn_blocking (Ja/Nein).
2. Für JEDES Vorkommen mit "Nein": dies ist ein bestätigter struktureller Bug — dokumentiere als
   CRITICAL.
3. Führe einen empirischen Beweistest durch (unabhängig vom Code-Review, da Code-Review menschliche/KI-
   Fehleinschätzung nicht ausschließt): starte eine große, garantiert blockierende synchrone
   Datei-Operation (z.B. sehr große sequenzielle SSTable-Lese-/Schreiboperation, mehrere hundert MB)
   über die öffentliche `memfuse-store`-API. WÄHREND diese läuft, starte auf DEMSELBEN Tokio-Runtime-
   Kontext einen zweiten, unabhängigen "leichten" Task (z.B. `tokio::time::sleep(Duration::from_millis(1))`
   in einer Schleife mit Zeitstempel-Protokollierung). Miss die tatsächliche Ausführungslatenz des
   leichten Tasks während der schweren I/O-Operation läuft — bei korrektem `spawn_blocking`-Einsatz
   bleibt sie nahe der erwarteten Sleep-Dauer (Executor bleibt frei); bei fehlerhafter Blockierung
   schnellt sie auf die Dauer der I/O-Operation hoch (Executor-Starvation nachgewiesen).
4. Wiederhole Test 3 mit einer Tokio-Runtime-Konfiguration mit NUR 1 Worker-Thread (`#[tokio::main(
   flavor = "current_thread")]` oder äquivalent in einem isolierten Test-Binary), da Starvation-Effekte
   bei Multi-Worker-Runtimes durch andere freie Threads maskiert werden können und der Single-Thread-Fall
   den Bug unzweideutig sichtbar macht.
5. Falls ein echtes Blocking-Problem gefunden wird: identifiziere die exakte(n) verursachende(n)
   Codestelle(n) aus Schritt 1-2 und verifiziere, dass sie tatsächlich für den in Schritt 3-4 gemessenen
   Effekt verantwortlich sind (z.B. durch temporäres Auskommentieren/Instrumentieren zur Isolation der
   Ursache).

REPORT: docs/audits/round2/AUDIT_memfuse-store_async-blocking.md
STRUKTUR: 1) Executive Summary (Blocking-Bug bestätigt/widerlegt), 2) Vollständiges std::fs::File-
Aufruf-Inventar mit spawn_blocking-Nachweis, 3) Executor-Starvation-Empirischer-Testbeweis (Multi-
Worker), 4) Executor-Starvation-Empirischer-Testbeweis (Single-Worker, schärfster Test), 5) Ursachen-
Isolation, 6) Priorisierte Bugliste, 7) Anhang Rohlogs (Latenz-Zeitreihen des leichten Tasks).

ABNAHMEKRITERIUM
Das Code-Review-Inventar (Schritt 1-2) ALLEIN ist nicht ausreichend — der empirische Single-Worker-Test
(Schritt 4) ist der entscheidende Beweis und muss in jedem Fall durchgeführt werden, auch wenn Schritt
1-2 keine offensichtlichen Verstöße findet (versteckte transitive Blocking-Aufrufe in Abhängigkeiten
sind möglich).
```

---

# D. `memfuse-graph` — Dekomposition in 3 Sub-Prompts

## D.1 `memfuse-graph` / CSR-Insert-Komplexität — Delta-Graph-Kosten-Verifikation

```
ROLLE
Senior Rust Graph-Datenstruktur-Ingenieur, Spezialgebiet amortisierte Komplexitätsanalyse.

MISSION
Runde 1 hat einen echten strukturellen Bug in `GraphInner::compact` gefunden und behoben (Offsets/
Reverse-Map-Inkonsistenz). Das bestätigt: die Pending-Edges→CSR-Kompaktierungs-Architektur EXISTIERT
bereits als Delta-Graph-Muster (gemäß Code-Kommentaren). Die Gemini-Hypothese eines naiven "jede Kante
kopiert sofort das gesamte CSR-Array um" scheint dadurch bereits strukturell widerlegt — ABER: die
tatsächliche AMORTISIERTE Kosten-Charakteristik (wie oft wird tatsächlich kompaktiert, und wie teuer ist
JEDE Kompaktierung in Relation zur Graphgröße) wurde in Runde 1 nicht quantitativ vermessen. Deine
Mission: liefere die fehlende empirische Komplexitätskurve.

AUFGABENUMFANG
1. Analysiere `crates/memfuse-graph/src/csr.rs`: identifiziere den exakten Trigger-Mechanismus für eine
   Pending-Edges→CSR-Kompaktierung (fixe Anzahl Pending-Edges? Zeitbasiert? Bei jedem Lesezugriff?
   Größenverhältnis Pending zu Committed?).
2. Miss die Latenz EINER EINZELNEN Kompaktierung bei unterschiedlicher Graphgröße (1K/10K/100K/1M
   bereits committete Kanten, jeweils mit einer fixen Anzahl z.B. 1000 Pending-Edges) — bestätige oder
   widerlege empirisch, ob die Kompaktierungskosten mit der GESAMTEN Graphgröße skalieren (was den
   "jede Kante kopiert alles um"-Verdacht bestätigen würde) oder nur mit der Anzahl der Pending-Edges
   (was die Delta-Architektur als wirksam bestätigen würde).
3. Miss den AMORTISIERTEN Insert-Durchsatz über eine lange Sequenz von 1 Million sequenziellen
   Kanten-Inserts in einen anfangs leeren Graphen (inkl. aller dabei ausgelösten Kompaktierungen) —
   berechne die durchschnittliche Zeit pro Insert und die Verteilung (die meisten Inserts sollten sehr
   schnell sein mit periodischen Latenz-Spitzen bei Kompaktierung — quantifiziere Spitzenhöhe und
   -häufigkeit als Histogramm/Perzentil-Tabelle p50/p95/p99/p99.9/max).
4. Bewerte, ob die p99.9/max-Latenz-Spitzen für den Einsatzzweck (interaktive Agenten-Anfragen, die laut
   README niedrige Latenz erwarten) akzeptabel sind, oder ob sie ein reales Nutzungsrisiko darstellen
   (z.B. eine Agenten-Anfrage, die zufällig während einer großen Kompaktierung landet, erlebt eine
   Latenz-Spitze von X ms/s).

REPORT: docs/audits/round2/AUDIT_memfuse-graph_csr-insert-complexity.md
STRUKTUR: 1) Executive Summary (Delta-Architektur-Wirksamkeit bestätigt, mit Kennzahlen),
2) Kompaktierungs-Trigger-Mechanismus (Code-Beleg), 3) Kompaktierungskosten-vs-Graphgröße-Tabelle,
4) Amortisierter-Durchsatz Perzentil-Tabelle, 5) Latenzspitzen-Risikobewertung für interaktive Nutzung,
6) Anhang Rohlogs/Histogrammdaten.

ABNAHMEKRITERIUM
Die Aussage "Delta-Graph-Architektur ist wirksam" ist nur gültig, wenn Schritt 2 empirisch zeigt, dass
Kompaktierungskosten NICHT proportional zur GESAMTEN Graphgröße wachsen.
```

## D.2 `memfuse-graph` / PPR Sink-Hole & Teleportations-Vektor-Korrektheit unter Dangling-Nodes

```
ROLLE
Senior Rust Numerischer-Algorithmen-Ingenieur, Spezialgebiet Markov-Ketten-basierte Ranking-Verfahren.

MISSION
Der Runde-1-Graph-Audit hat PPR grundsätzlich gegen eine Power-Iteration-Referenz verifiziert (siehe
AUDIT_memfuse-graph.md), aber laut der Gemini-Analyse ist der GEFÄHRLICHSTE PPR-Fehlerfall spezifisch
der Umgang mit "Dangling Nodes" (Knoten ohne ausgehende Kanten — sog. Sink-Holes): wird deren
akkumulierter Score bei jeder Iteration korrekt über einen Teleportations-Mechanismus wieder auf den
gesamten Graphen verteilt (mathematisch korrekte PPR-Definition), sammelt sich der Score sonst
unkontrolliert bei diesen Knoten und degradiert die Relevanz ALLER anderen Ergebnisse gegen Null. Falls
der Runde-1-Test dies nicht mit einem GEZIELTEN Dangling-Node-Graphen geprüft hat (nur allgemeine
Testgraphen), ist dies eine Lücke.

AUFGABENUMFANG
1. Prüfe zuerst den Runde-1-Report (AUDIT_memfuse-graph.md, PPR-Abschnitt): enthielt der dortige
   Testgraph explizit einen oder mehrere Dangling Nodes? Falls JA, war der Score-Erhalt (Summe aller
   PPR-Scores über den Graphen = 1, bzw. das erwartete Normierungs-Target) explizit verifiziert? Falls
   NEIN zu einer der beiden Fragen, gilt diese Lücke als bestätigt und Schritt 2ff. sind durchzuführen.
2. Konstruiere einen Testgraphen mit exakt EINEM Dangling Node inmitten eines ansonsten gut verbundenen
   10-Knoten-Graphen, führe PPR aus, und verifiziere: (a) konvergiert der Algorithmus überhaupt
   (endliche Iterationsanzahl bis zum Konvergenzkriterium)? (b) ist die Summe aller finalen PPR-Scores
   über alle Knoten gleich 1.0 (oder dem korrekten Normierungs-Zielwert, je nach exakter PPR-Variante —
   dies unabhängig aus der Literatur verifizieren) innerhalb einer engen Toleranz? (c) erhalten die
   NICHT-Dangling-Knoten weiterhin sinnvoll differenzierte, von Null verschiedene Scores (kein
   "Score-Kollaps" auf nahezu 0 für alle außer dem Dangling Node)?
3. Wiederhole mit einem EXTREMEN Testfall: 90% aller Knoten in einem 20-Knoten-Graphen sind Dangling
   Nodes (nur 10% haben ausgehende Kanten) — dies ist der Stresstest für die Teleportations-Logik.
4. Wiederhole mit einer GRUPPE zusammenhängender Dangling Nodes (3 Knoten ohne ausgehende Kanten, die
   selbst aber von anderen Knoten referenziert werden) — verifiziere, dass dies nicht zu einem anderen
   Fehlerbild führt als ein einzelner isolierter Dangling Node.
5. Falls in irgendeinem der Fälle Score-Erhalt verletzt wird oder ein "Score-Kollaps"-Muster
   nachgewiesen wird: identifiziere die exakte Codezeile der Teleportations-/Normierungslogik in
   `crates/memfuse-graph/src/ppr.rs` als Root Cause.

REPORT: docs/audits/round2/AUDIT_memfuse-graph_ppr-dangling-nodes.md
STRUKTUR: 1) Executive Summary (Lücke aus Runde 1 bestätigt Ja/Nein, neue Testergebnisse),
2) Runde-1-Abdeckungs-Review, 3) Score-Erhalt-Testmatrix (3 Testgraphen: 1 Dangling / 90% Dangling /
Dangling-Gruppe), 4) Root-Cause-Analyse (falls Bug gefunden), 5) Priorisierte Bugliste, 6) Anhang Rohlogs.

ABNAHMEKRITERIUM
Score-Erhalt (Summe = Zielwert) muss für JEDEN der 3 Testgraphen explizit numerisch nachgewiesen werden,
nicht nur qualitativ ("Werte sehen plausibel aus").
```

## D.3 `memfuse-graph` / BFS-Explosion an Hub-Knoten (Branching-Limit-Verifikation)

```
ROLLE
Senior Rust Graph-Traversierungs-Ingenieur, Spezialgebiet Ressourcen-Begrenzung in unbeschränkten
Traversierungsalgorithmen.

MISSION
Die Gemini-Analyse warnt vor unbegrenzter BFS-Explosion an "Hub-Knoten" (Knoten mit sehr hoher
Kantenzahl, z.B. ein häufig referenziertes zentrales Dokument in einem Wissensgraphen) trotz eines
dokumentierten globalen `MAX_SEARCH_K`-Limits — die Sorge ist, dass dieses Limit die FINALE Ergebnismenge
begrenzt, aber NICHT die ZWISCHENZEITLICHE Speicherbelegung während der Traversierung selbst begrenzt,
falls kein zusätzliches Branching-Limit PRO TIEFENEBENE existiert.

AUFGABENUMFANG
1. Analysiere die BFS-Implementierung in `crates/memfuse-graph/src/csr.rs`: wird `MAX_SEARCH_K` als
   Abbruchkriterium NACH jedem vollständig abgearbeiteten Level angewendet (Ergebnismenge kann
   zwischenzeitlich das Level-große Vielfache von K erreichen, bevor abgebrochen wird), oder als hartes
   Limit WÄHREND der Queue-Befüllung (Queue wächst nie über K hinaus)?
2. Konstruiere einen synthetischen "Hub-Graphen": ein zentraler Knoten mit 100.000 ausgehenden Kanten zu
   100.000 Blattknoten, plus einen Startknoten, der über wenige Hops den Hub erreicht. Starte eine BFS
   von diesem Startknoten mit einem KLEINEN `MAX_SEARCH_K` (z.B. 50) und miss die tatsächliche
   Peak-Queue-Größe/Peak-Speicherverbrauch WÄHREND der Traversierung (nicht nur die finale
   Ergebnisgröße).
3. Vergleiche Peak-Speicherverbrauch bei Hub-Größe 1K / 10K / 100K / 1M ausgehenden Kanten (bei
   konstantem kleinen `MAX_SEARCH_K`) — bei korrekter Implementierung sollte der Peak-Speicherverbrauch
   WEITGEHEND UNABHÄNGIG von der Hub-Größe sein (durch K begrenzt); bei fehlerhafter Implementierung
   sollte er linear mit der Hub-Größe wachsen.
4. Falls ein Wachstum nachgewiesen wird: miss zusätzlich die Wall-Clock-Latenz bei denselben
   Hub-Größen, um zu bestätigen, dass auch die REINE CPU-Zeit (nicht nur Speicher) durch die Hub-Größe
   dominiert wird — dies ist der eigentliche DoS-Vektor (eine einzelne böswillig oder organisch
   entstandene Hub-Struktur im Nutzerdaten-Wissensgraphen verlangsamt jede Traversierungsanfrage, die
   sie durchquert).
5. Falls kein Branching-Limit-pro-Tiefenebene existiert: schlage konkret eine Implementierung vor
   (z.B. harte Obergrenze an untersuchten Kanten pro Knoten während der Expansion, mit
   Sampling/Priorisierung nach Kantengewicht für die verbleibenden Kanten).

REPORT: docs/audits/round2/AUDIT_memfuse-graph_hub-node-bfs-explosion.md
STRUKTUR: 1) Executive Summary (Explosion bestätigt/widerlegt), 2) BFS-Abbruch-Mechanismus-Code-
Analyse, 3) Peak-Speicher-vs-Hub-Größe-Tabelle, 4) Latenz-vs-Hub-Größe-Tabelle, 5) DoS-Risikobewertung,
6) Härtungsvorschlag, 7) Anhang Rohlogs.

ABNAHMEKRITERIUM
Sowohl Speicher- ALS AUCH Latenz-Skalierung müssen gemessen werden — ein Nachweis nur einer der beiden
Dimensionen ist unvollständig, da ein DoS-Angriff über beide Vektoren wirken kann.
```

---

# E. `memfuse-text` — Dekomposition in 3 Sub-Prompts

## E.1 `memfuse-text` / Root-Cause der Kompositazerlegungs-Fehlschläge & Rekursions-/Backtracking-DoS-Analyse

```
ROLLE
Senior Rust Computerlinguistik-Ingenieur mit Spezialisierung auf String-Segmentierungsalgorithmen
(dynamische Programmierung, Finite State Transducers, Trie-basierte Verfahren).

MISSION
Runde 1 (AUDIT_memfuse-text.md) fand: 41/45 Komposita korrekt gesplittet, aber die 3 längsten/
komplexesten Testfälle (`donaudampfschifffahrtsgesellschaftskapitaen` [45 Zeichen],
`softwareentwicklungskontext`, `systemadministrator`) blieben KOMPLETT UNGESPLITTET (nicht etwa falsch,
sondern gar nicht zerlegt). Dieses Muster — korrekte Ergebnisse bei kurzen/mittleren Wörtern, aber
KOMPLETTES Scheitern (nicht graduelle Verschlechterung) bei den längsten Wörtern — ist ein klassisches
Symptom für einen Algorithmus, der bei einer bestimmten Eingabelänge/Komplexität an eine interne
Grenze stößt (Rekursionstiefen-Limit? Backtracking-Suchraum-Explosion mit stillem Timeout/Abbruch?
Wörterbuch-Lookup-Fehlschlag bei bestimmten Teilstring-Längen?). Deine Mission: finde die exakte
Ursache, nicht nur das Symptom.

AUFGABENUMFANG
1. Lies `crates/memfuse-text/src/morphology.rs` (`GermanCompoundSplitter`) vollständig und rekonstruiere
   den Algorithmus als Pseudocode/Kontrollflussgraph: ist es ein rekursiver Backtracking-Ansatz (probiere
   jede mögliche Segmentierungsgrenze, rekursiv für den Rest), ein DP-Ansatz (Memoization-Tabelle über
   Teilstring-Grenzen), oder ein Wörterbuch-Longest-Match-Greedy-Ansatz?
2. Instrumentiere den Splitter (temporäre Debug-Logging-Einfügung oder Nutzung eines Profilers) und führe
   ihn auf den 3 bekannt fehlschlagenden Wörtern aus. Protokolliere: Anzahl rekursiver Aufrufe /
   Iterationen, ob ein internes Limit (z.B. maximale Rekursionstiefe, maximale Anzahl Versuche, Timeout)
   existiert und dabei erreicht wird, und den exakten Punkt, an dem der Algorithmus aufgibt und das
   Originalwort unverändert zurückgibt.
3. Baue eine Reihe von Zwischenlängen-Testwörtern (synthetisch konstruierte, garantiert im Wörterbuch
   vorhandene Bestandteile, aber mit wachsender Gesamtlänge/Teilwortzahl: 10, 15, 20, 25, 30, 35, 40, 45
   Zeichen bzw. 2, 3, 4, 5, 6 Teilwörter) um die EXAKTE Schwelle zu bestimmen, an der das Verhalten von
   "korrekt gesplittet" zu "komplett ungesplittet" kippt — ist es eine Zeichenlängen-Schwelle oder eine
   Teilwortanzahl-Schwelle?
4. Miss die tatsächliche Ausführungszeit des Splitters für Wörter VOR und AN der gefundenen Schwelle —
   steigt die Latenz vor dem Kipppunkt bereits exponentiell/stark superlinear an (was auf
   Backtracking-Explosion mit einem harten Abbruch-Failsafe hindeuten würde), oder bleibt sie konstant
   niedrig bis zum abrupten Fehlschlag (was eher auf ein hartkodiertes Größen-/Tiefenlimit hindeuten
   würde)?
5. Konstruiere zusätzlich einen GEZIELTEN DoS-Testfall: ein synthetisches Pseudo-Wort von 200+ Zeichen
   Länge, bestehend aus wiederholten, im Wörterbuch vorhandenen kurzen Silben in einer Kombination, die
   MAXIMALE Segmentierungs-Ambiguität erzeugt (viele verschiedene gültige Zerlegungsmöglichkeiten) — miss
   CPU-Zeit und Speicherverbrauch. Falls dies zu einer unverhältnismäßig langen Laufzeit (>1 Sekunde) oder
   hohem Speicherverbrauch führt, ist dies ein bestätigter DoS-Vektor gegen den Ingestion-/Such-Pfad
   (jede Nutzereingabe mit einem solchen Pseudo-Wort würde den Text-Indexierungs-Thread blockieren).
6. Schlage auf Basis der Root-Cause-Analyse einen konkreten Fix vor: falls Backtracking ohne Memoization
   die Ursache ist, empfiehl einen DP-basierten Ansatz mit garantiert polynomieller statt exponentieller
   Zeitkomplexität (Trie- oder FST-gestützt, wie in der Gemini-Analyse vorgeschlagen) und beschreibe die
   Umstellung konkret genug, dass ein nachfolgender Reparatur-Task sie direkt umsetzen kann.

REPORT: docs/audits/round2/AUDIT_memfuse-text_compound-split-root-cause.md
STRUKTUR: 1) Executive Summary (Root Cause klar benannt: Rekursionslimit / Backtracking-Timeout /
Wörterbuch-Lücke / sonstiges), 2) Algorithmus-Kontrollflussgraph-Rekonstruktion, 3) Instrumentierungs-
Ergebnisse für die 3 bekannten Fehlschläge, 4) Schwellenwert-Bestimmungs-Tabelle (Länge/Teilwortzahl ×
Erfolg/Fehlschlag × Latenz), 5) Gezielter DoS-Testfall-Ergebnis (CPU-Zeit, Speicher, Verdikt), 6)
Konkreter Fix-Vorschlag mit Komplexitätsklassen-Begründung, 7) Anhang Rohlogs/Instrumentierungsdaten.

ABNAHMEKRITERIUM
Die Root-Cause-Aussage muss durch die Instrumentierungsdaten aus Schritt 2 direkt belegt sein ("der
Algorithmus bricht bei Rekursionstiefe X mit Rückgabe des Originalworts ab" ist ein gültiger Beleg;
"vermutlich ein Tiefenlimit" ohne Instrumentierungsnachweis ist es nicht).
```

## E.2 `memfuse-text` / Unicode-Grapheme-Grenzen-Panic-Erschöpfung (Byte- vs. Char-Slicing)

```
ROLLE
Senior Rust Text-Verarbeitungs-Ingenieur, Spezialgebiet UTF-8-sichere String-Manipulation.

MISSION
Runde 1 hat allgemeine Tokenizer-Robustheit gegen Unicode getestet (proptest-Fuzzing gegen Panics laut
Prompt-Vorgabe), aber die Gemini-Analyse benennt einen SEHR SPEZIFISCHEN, in Rust besonders häufigen
LLM-Fehler: die Verwendung von Byte-Index-basiertem String-Slicing (`&text[a..b]` oder
`String::split_at(n)`) an Positionen, die NICHT auf eine UTF-8-Zeichengrenze fallen — dies verursacht in
Rust einen GARANTIERTEN Panic (nicht Undefined Behavior, aber einen harten Prozessabbruch des
aufrufenden Threads), sobald die Slicing-Position mitten in einem Mehrbyte-Zeichen liegt (z.B. deutsche
Umlaute sind 2-Byte-UTF-8-Sequenzen, viele Emojis sind 4-Byte-Sequenzen). Falls das allgemeine
proptest-Fuzzing aus Runde 1 nicht GEZIELT auf mehrbyte-lastige Eingaben mit knappen Slicing-Grenzen
ausgerichtet war, kann dieser spezifische Bug-Typ unentdeckt geblieben sein.

AUFGABENUMFANG
1. Durchsuche das GESAMTE `memfuse-text`-Crate (alle 5 Dateien: bm25.rs, inverted.rs, morphology.rs,
   tokenizer.rs, lib.rs) systematisch nach JEDEM Vorkommen von Byte-Index-basiertem String-Slicing:
   `&s[...]`, `.split_at(`, `.get(a..b)` auf `&str`/`String`, direkte Byte-Index-Arithmetik die für
   nachfolgendes Slicing verwendet wird. Erstelle ein vollständiges Inventar mit Zeilenverweisen.
2. Für JEDES gefundene Vorkommen: verifiziere, ob die verwendeten Indizes GARANTIERT von einer
   UTF-8-sicheren Quelle stammen (z.B. von `.char_indices()`, einer Unicode-Segmentierungs-Bibliothek wie
   `unicode-segmentation`, oder von ASCII-only-vorverarbeiteten Daten mit expliziter Prüfung), oder ob sie
   aus potenziell unsicherer Arithmetik stammen (z.B. feste Offsets, Byte-Längen-Berechnungen ohne
   Zeichengrenzen-Bewusstsein).
3. Konstruiere für JEDES als potenziell unsicher eingestufte Vorkommen einen gezielten Testfall mit einer
   Eingabe, die an genau der kritischen Position ein Mehrbyte-UTF-8-Zeichen enthält (deutsche Umlaute
   ä/ö/ü/ß als 2-Byte-Fälle, Emoji als 4-Byte-Fall, kombinierte diakritische Zeichen als Grapheme-
   Cluster-Fall der aus MEHREREN Unicode-Codepoints besteht aber visuell EIN Zeichen ist — z.B. "é" als
   Kombination aus "e" + Combining-Acute-Accent) und führe die betroffene Funktion aus.
4. Baue zusätzlich eine breite proptest-Suite, die GEZIELT (nicht wie in Runde 1 allgemein) Strings mit
   HOHER Dichte an Mehrbyte-Zeichen generiert (nicht durchmischt mit viel ASCII, sondern überwiegend aus
   dem Bereich U+00C0-U+017F [lateinische Erweiterung, deckt deutsche Umlaute ab] und U+1F300-U+1FAFF
   [Emoji-Bereich]) und JEDE öffentliche Funktion des Crates gegen 10.000+ solcher Eingaben ausführt,
   Panics als harten Fehlschlag wertend.
5. Für jeden gefundenen Panic: dokumentiere exakte Reproduktion (minimale Eingabe, die den Panic
   auslöst — falls proptest "shrinking" unterstützt, nutze dies für die minimalste Reproduktion) und
   schlage den korrekten UTF-8-sicheren Ersatz vor (`.char_indices()`, `.chars().nth()`,
   `unicode-segmentation`-Crate für Grapheme-Cluster-Bewusstsein wo nötig).

REPORT: docs/audits/round2/AUDIT_memfuse-text_unicode-slicing-panics.md
STRUKTUR: 1) Executive Summary (Anzahl gefundener echter Panics, Schweregrad), 2) Vollständiges
Byte-Slicing-Inventar mit Sicherheitseinstufung, 3) Gezielte Grenzfall-Testmatrix (Umlaut/Emoji/
Grapheme-Cluster × betroffene Funktion × Ergebnis), 4) Proptest-Fuzzing-Ergebnisse (10.000+ Iterationen,
Anzahl Fehlschläge, minimierte Counterexamples), 5) Priorisierte Bugliste mit konkreten Fix-Vorschlägen
pro Fund, 6) Anhang Rohlogs.

ABNAHMEKRITERIUM
JEDES als "potenziell unsicher" eingestufte Slicing-Vorkommen aus Schritt 2 MUSS in Schritt 3 einen
tatsächlichen Testfall erhalten haben — keine Auslassungen aufgrund subjektiver "sieht sicher aus"-
Einschätzung.
```

## E.3 `memfuse-text` / Allokations-Overhead-Audit (Zero-Copy-Verstoß-Hypothese)

```
ROLLE
Senior Rust Performance-Ingenieur, Spezialgebiet Speicherallokations-Profiling.

MISSION
Die Gemini-Analyse hypothetisiert massive Heap-Allokationen durch übermäßiges `.clone()`/`.to_string()`
in der Tokenisierungs-/Indexierungs-Pipeline statt Zero-Copy-Referenzen (`&'a str`/`Cow<str>`), was laut
Hypothese dazu führt, dass das System mehr Zeit im globalen Allocator verbringt als in der eigentlichen
BM25-Berechnung. Dies ist eine konkret falsifizierbare Performance-Hypothese, die direkt mit einem
Allocation-Profiler verifiziert werden kann.

AUFGABENUMFANG
1. Durchsuche `crates/memfuse-text/src/{tokenizer,morphology,inverted,bm25}.rs` nach JEDEM Vorkommen von
   `.to_string()`, `.to_owned()`, `.clone()` auf String-/Vec-Typen, `String::from()`, `format!()`
   innerhalb von Hot-Path-Funktionen (Tokenisierung pro Wort, Indexierung pro Dokument, Score-Berechnung
   pro Query-Term). Erstelle ein Inventar mit Häufigkeitseinschätzung (wird dies pro Wort, pro Dokument,
   oder einmalig pro Aufruf ausgeführt?).
2. Nutze, falls in der Sandbox verfügbar, einen Allocation-Profiler (`dhat`/`heaptrack`/`valgrind
   --tool=massif`, oder als Rust-natives Werkzeug den `dhat`-Crate als temporäre Dev-Dependency) um für
   einen realistischen Indexierungs-Workload (z.B. 10.000 Dokumente mit je ~500 Wörtern deutschen
   Fließtexts) die GESAMTZAHL der Heap-Allokationen und die dabei verbrachte Zeit im Allocator zu messen.
3. Falls kein Allocation-Profiler installierbar ist: baue einen minimalinvasiven Custom-Global-Allocator
   (via `#[global_allocator]` mit einem zählenden Wrapper um `std::alloc::System` in einem isolierten
   Test-Binary) der Allokationsanzahl und -Gesamtgröße protokolliert, und führe denselben Workload damit
   aus.
4. Berechne: Allokationen pro indexiertem Wort (Ziel-Richtwert für eine gut optimierte Zero-Copy-
   Pipeline: deutlich unter 1 Allokation pro Wort im Durchschnitt, da viele Tokens direkt als Slices des
   Originaldokuments referenzierbar sein sollten) und vergleiche gegen den gemessenen Wert.
5. Identifiziere aus dem Inventar (Schritt 1) die 3 Codestellen mit dem VERMUTLICH größten Beitrag zum
   gemessenen Allokationsvolumen (basierend auf Aufrufhäufigkeit × Datengröße) und schlage konkret vor,
   wie sie auf `&str`/`Cow<str>`-basierte Zero-Copy-Alternativen umgestellt werden könnten — mit
   Begründung, warum dies in jedem einzelnen Fall (Lifetime-Anforderungen, Ownership-Struktur des
   umgebenden Codes) tatsächlich machbar ist, nicht nur theoretisch wünschenswert.
6. Miss zum Vergleich den GESAMTEN Indexierungsdurchsatz (Dokumente/Sekunde) für denselben Workload, um
   dem Auftraggeber eine konkrete Kosten-Nutzen-Einschätzung zu liefern: wie viel Prozent der
   Gesamtlaufzeit entfällt schätzungsweise auf Allokation (basierend auf typischen Allocator-Kosten pro
   Allokation multipliziert mit der gemessenen Anzahl)?

REPORT: docs/audits/round2/AUDIT_memfuse-text_allocation-overhead.md
STRUKTUR: 1) Executive Summary (Allokationen/Wort-Kennzahl, Einordnung gegen Zielwert),
2) Allokations-Hot-Path-Inventar, 3) Profiling-Methodik & Rohergebnisse, 4) Top-3-Optimierungskandidaten
mit Machbarkeitsbegründung, 5) Durchsatz-Baseline & geschätzter Optimierungs-Impact,
6) Anhang Rohlogs/Profiler-Output.

ABNAHMEKRITERIUM
Die Allokationszahl muss aus einem tatsächlich ausgeführten Profiling-Lauf stammen (Tool-Output oder
Custom-Allocator-Zähler im Anhang referenziert), nicht aus einer Schätzung anhand des Code-Reviews allein.
```

---

# F. Kompakte Zusatzprüfungen für die übrigen 10 Crates (Round-2-Delta, kein volles Re-Audit)

Die folgenden Punkte sind gezielte Ergänzungen zu bereits durchgeführten Runde-1-Audits, keine vollständigen Neuformulierungen — sie greifen ausschließlich Lücken auf, die durch den Gemini-Abgleich sichtbar wurden und die Runde 1 nicht mit ausreichender Tiefe behandelt hat. Jeder Punkt kann als eigenständiger, kurzer Jules-Prompt verwendet werden.

```
F.1 memfuse-core — TxId-Allocation-Base-Ranges unter absichtlicher Erschöpfungssimulation
Ziel: Verifiziere ADR-028 (System- vs. Collection-Transaktions-Ranges) nicht nur auf Monotonie
(bereits in Runde 1 geprüft), sondern auf das Verhalten AN der Grenze zwischen beiden Ranges — was
passiert, wenn die System-Range knapp vor der Collection-Range-Grenze steht und weiter alloziert wird?
Kollision oder kontrollierter Fehler? Erstelle einen Testfall, der die Range-Grenze künstlich nah an
den aktuellen Allocation-Zeiger setzt (via Test-Hook oder Reflection über interne Test-APIs) und die
Grenzüberschreitung erzwingt.
Report: docs/audits/round2/AUDIT_memfuse-core_txid-range-boundary.md

F.2 memfuse-router — Regressionscheck der Layer-DAG-Konformität + Determinismus bei SlmProfile-Änderung
zur Laufzeit
Ziel: Bestätige per Grep, dass memfuse-router weiterhin KEINE memfuse-mcp-Typen importiert (Regression
zur Gemini-Hypothese). Zusätzlich NEU: teste, was passiert, wenn SlmProfile-Konfiguration WÄHREND
aktiver Routing-Entscheidungen anderer paralleler Aufrufe verändert wird (Hot-Reload-Konsistenz) — ein
Aspekt, den der fokussierte Runde-1-Prompt für dieses kleine Crate nicht behandelt hat.
Report: docs/audits/round2/AUDIT_memfuse-router_dag-and-hotreload.md

F.3 memfuse-ollama / memfuse-tauri — Erweiterte Prompt-Injection-Denylist-Härtungsprüfung
Ziel: Auch wenn `sanitize_prompt_input()` unter diesem exakten Namen nicht auffindbar war, durchsuche
BEIDE Crates nach JEDER Stelle, die Nutzer-/Dokumenteninhalt vor der Einbettung in einen LLM-Prompt
filtert oder validiert (andere Namenskonventionen möglich). Falls eine Denylist-basierte Filterung
(Blacklist verbotener Phrasen/Muster) gefunden wird: konstruiere mindestens 15 bekannte Prompt-
Injection-Umgehungstechniken (Unicode-Homoglyphen, Base64-Kodierung der Injection, Zero-Width-Space-
Einschleusung zwischen Denylist-Wörtern, Mehrsprachigkeit/Übersetzung der Injection-Phrase) und
verifiziere, ob jede davon die Denylist umgeht.
Report: docs/audits/round2/AUDIT_memfuse-ollama-tauri_prompt-injection-hardening.md

F.4 memfuse-checkpoint — Nebenläufiger Time-Travel unter gleichzeitigem Rollback zweier Agenten-Sessions
Ziel: Runde 1 hat Time-Travel-Korrektheit für EINE Sequenz geprüft. Erweitere um: zwei unabhängige
Agenten-Sessions (unterschiedliche Namespace/Collection) führen GLEICHZEITIG Checkpoint-Erstellung und
Rollback durch — verifiziere vollständige Isolation (keine Session beeinflusst die Checkpoint-Historie
der anderen).
Report: docs/audits/round2/AUDIT_memfuse-checkpoint_concurrent-sessions.md

F.5 memfuse-agent — Token-Budget-Race zwischen parallelen Schritten derselben Workflow-Instanz
Ziel: Falls ein Workflow parallele Schritt-Ausführung zulässt (verifizieren!), teste, ob zwei Schritte,
die GLEICHZEITIG das verbleibende Budget beanspruchen, zusammen das Gesamtbudget überschreiten können
(klassisches Read-Modify-Write-Race auf einem Budget-Zähler ohne atomare Operation).
Report: docs/audits/round2/AUDIT_memfuse-agent_budget-race.md

F.6 memfuse-mcp — Slowloris-artiger Verbindungsaufbau über stdio (partielle Requests über sehr lange Zeit)
Ziel: Runde 1 prüfte Flood- und Binärdaten-Angriffe. Ergänze: ein "Slowloris"-artiger Angriff, bei dem
ein Request-Byte alle 100ms über mehrere Minuten gesendet wird (statt einmalig oder in schnellen
Bursts) — bindet dies dauerhaft Ressourcen (Buffer, Task) ohne Fortschritt, und existiert ein
Inaktivitäts-Timeout, der solche hängenden Verbindungen abbricht?
Report: docs/audits/round2/AUDIT_memfuse-mcp_slowloris.md

F.7 memfuse-embed / memfuse-db (Cross-Crate) — Reranker-Score-Manipulation durch adversariale Chunk-Inhalte
Ziel: Konstruiere Chunk-Inhalte, die speziell darauf ausgelegt sind, den Cross-Encoder-Reranker-Score
künstlich zu maximieren, OHNE inhaltlich relevant zu sein (z.B. Wiederholung des Query-Textes selbst
im Chunk als "Keyword-Stuffing für Reranker") — quantifiziere, wie leicht das finale Ranking durch
solche adversarialen Inhalte manipulierbar ist (Relevanz für Dokumenten-Ingestion aus potenziell
manipulierten/böswilligen Quelldokumenten).
Report: docs/audits/round2/AUDIT_memfuse-embed-db_reranker-adversarial.md

F.8 memfuse-py — GIL-Freigabe-Regressionstest unter Python-3.12/3.13-Sub-Interpreter-Modus (falls
zutreffend)
Ziel: Falls PyO3-Version und Zielumgebung Sub-Interpreter unterstützen, verifiziere, dass die geteilte
OnceLock-Tokio-Runtime unter mehreren Python-Sub-Interpretern (nicht nur Threads) korrekt funktioniert
oder zumindest sauber mit einer klaren Fehlermeldung ablehnt, statt undefiniert zu interagieren.
Report: docs/audits/round2/AUDIT_memfuse-py_subinterpreter.md
```

---

## Priorisierungsempfehlung für die Ausführung von Runde 2

Angesichts der bereits in Runde 1 nachgewiesenen hohen Trefferquote realer kritischer Bugs (9 bestätigte, behobene Bugs über 3 Crates) ist zu erwarten, dass ein relevanter Anteil der Runde-2-Prompts ebenfalls echte Befunde liefert. Empfohlene Abarbeitungsreihenfolge nach Risikograd (Sicherheits-/Korrektheitsrelevanz zuerst, dann Performance):

1. **C.2** (WAL-HMAC-Truncation-Attack) — Sicherheit, air-gapped Integritätsgarantie
2. **B.2** (mmap-TOCTOU) — Stabilität, Prozessabsturz-Risiko
3. **B.1** (CPU-Feature-Detection) — Stabilität, Prozessabsturz-Risiko
4. **A.2** (Cross-Signal-Snapshot-Isolation) — Korrektheit, Kernversprechen des Systems
5. **A.1** (Zettelkasten-Zyklen) — Korrektheit/Verfügbarkeit, DoS-Risiko
6. **C.1** (fsync-Durability) — Datensicherheit
7. **E.2** (Unicode-Slicing-Panics) — Stabilität
8. **E.1** (Kompositazerlegungs-DoS) — Stabilität + Korrektheit
9. **D.2** (PPR-Dangling-Nodes) — Korrektheit der Ranking-Qualität
10. **D.3** (BFS-Hub-Explosion) — Verfügbarkeit/DoS
11. **A.3** (2PC-Fault-Injection-Erschöpfung) — Korrektheit
12. Alle übrigen (C.3, C.4, D.1, B.3, B.4, A.4, E.3, F.1–F.8) — Performance-/Qualitäts-Vertiefung, nachrangig aber nicht optional.
