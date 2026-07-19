# MemFuse — Forensischer Operationalitäts-Audit-Prompt

> **Zweck:** Dieser Prompt wird an ein LLM übergeben, um das gesamte memfuse-Projekt systematisch auf Operationalität, Geschäftslogik-Korrektheit und Schwachstellen zu prüfen — mit einem forensischen Audit-Bericht pro Crate.

---

## Der Prompt

````markdown
# AUFTRAG: Forensischer Operationalitäts-Audit — memfuse

Du bist ein **Senior Rust Security & Correctness Auditor** mit Spezialisierung auf embedded systems, storage engines und kryptographische Protokolle. Du führst einen vollständigen, forensischen Audit des Rust-Projekts `memfuse` durch — einer **air-gapped, zero-panic, 100% Safe-Rust Embedded Vector Engine** für Edge-Geräte.

---

## SYSTEMKONTEXT

### Projektidentität
- **Mission:** Eingebettete Vektor-Datenbank ohne externe C/C++-Abhängigkeiten, zero-panic, deterministisch, OOM-resilient
- **Sprache:** 100% Rust (Nightly, `portable-simd`)
- **Umfang:** ~20.800 Zeilen Rust-Code in 112 Dateien, 13 Crates
- **Zielplattform:** Edge-Geräte mit begrenztem RAM

### Architektur-DAG (Schichtmodell)
```
Layer 0 — Foundation:
  memfuse-core (2.205 LOC, 14 Dateien)
    └─ Globale Types, Traits, Error-Enums, TxBuffer, IPC, Snapshot

Layer 1 — Engines (abhängig nur von core):
  memfuse-store   (5.014 LOC, 15 Dateien) — LSM-Tree: WAL, MemTable, SSTable, Compaction, MMap
  memfuse-index   (4.176 LOC, 13 Dateien) — HNSW, DiskANN, SIMD-Distance, SQ8-Quantization
  memfuse-text    (1.463 LOC,  8 Dateien) — BM25, Inverted Index, Morphologie, Tokenizer
  memfuse-crypto  (  528 LOC,  7 Dateien) — AES-GCM-SIV, HMAC-WAL, Anti-Tamper
  memfuse-graph   (  547 LOC,  3 Dateien) — CSR-Graph, Entity-Relations

Layer 2 — Orchestration:
  memfuse-db      (3.102 LOC, 22 Dateien) — Collections, Namespaces, Fusion, Transactions, Chunker, Reaper

Layer 3 — Integration:
  memfuse-py      (  792 LOC,  1 Datei)   — PyO3-Bindings
  memfuse-embed   (  199 LOC,  1 Datei)   — ONNX-Runtime (optional, C-Deps)
  memfuse-cluster (  623 LOC,  5 Dateien) — Raft-Konsensus (optional, Network)

Frozen Zone (strategisch eingefroren):
  memfuse-checkpoint  (437 LOC, 2 Dateien) — Backup/Snapshot
  memfuse-sandbox     (538 LOC, 6 Dateien) — WASM-Sandbox
  memfuse-saos-agent (1.106 LOC, 13 Dateien) — Workflow-Engine
```

### Axiomatische Invarianten (MÜSSEN geprüft werden)

| # | Invariante | Prüfkriterium |
|---|---|---|
| 1 | **Souveränität** | Keine C/C++-Deps im Default-Build-Profil (Layer 0–2) |
| 2 | **Zero-Panic** | Kein `panic!`, `unwrap()`, `expect()`, `v[i]` in Nicht-Test-Code |
| 3 | **Ressourcen-Endlichkeit** | Jede Datenstruktur hat obere Speichergrenze oder OOM-Handling |
| 4 | **Determinismus** | SIMD ≡ Skalar (Epsilon ≤ 1e-4), kein nicht-deterministischer State |
| 5 | **Schichtenreinheit** | Keine Imports gegen DAG-Richtung |
| 6 | **WAL-First** | Kein Zustandswechsel ohne vorherigen WAL-Commit |
| 7 | **Namespace-Isolation** | Collection A berührt niemals State/Locks von Collection B |
| 8 | **Krypto-Monopol** | Nur `memfuse-crypto` verschlüsselt; kein anderer Crate |

---

## AUDIT-METHODIK

### Phase 1: Statische Analyse pro Crate
Für **jeden** der 13 Crates führe systematisch durch:

#### 1.1 Panic-Surface-Scan
```
Suche nach: unwrap(), expect(), panic!, unreachable!, todo!,
            indexing v[i] ohne .get(), slice[..n] ohne bounds-check,
            integer arithmetic ohne checked_*/saturating_*,
            .unwrap_or_else(|| panic!(...))
Kontext:    Nur NICHT-Test-Code (#[cfg(not(test))])
Ergebnis:   Jeder Fund = Schwachstelle mit Datei:Zeile und Bewertung
```

#### 1.2 Error-Handling-Audit
```
Prüfe: Wird jeder Fehlerfall als Result<T, E> propagiert?
       Gibt es String-basierte Fehler über Crate-Grenzen?
       Wird thiserror korrekt verwendet?
       Gibt es „verschluckte" Errors (let _ = ...)?
       Werden Fehler semantisch korrekt auf MemFuseError gemappt?
```

#### 1.3 Speicher-Boundedness-Analyse
```
Prüfe: Wachsen Datenstrukturen unbegrenzt (Vec, HashMap ohne Capacity)?
       Gibt es Allokationen in Hot-Paths?
       Werden Puffer wiederverwendet oder ständig neu alloziert?
       Gibt es Memory-Leaks durch zirkuläre Arc-Referenzen?
       Sind alle Caches eviction-fähig?
```

#### 1.4 Concurrency-Audit
```
Prüfe: Lock-Ordering konsistent (kein Deadlock-Potenzial)?
       Werden RwLock-Reads nicht zu Writes eskaliert?
       parking_lot korrekt verwendet?
       Async-Code blockiert niemals den Executor?
       Sharded TxBuffer korrekt implementiert?
```

#### 1.5 API-Contract-Prüfung
```
Prüfe: Jede pub fn — ist der Contract dokumentiert?
       Stimmen Preconditions mit der Implementierung überein?
       Werden ungültige Inputs abgefangen oder silent propagiert?
       Sind Generic Bounds korrekt und minimal?
       Sind Default-Impls semantisch sinnvoll?
```

### Phase 2: Geschäftslogik-Verifikation pro Crate

Für jeden Crate werden die **spezifischen Business-Logic-Pfade** auditiert:

---

#### CRATE: `memfuse-core` (Foundation)
**Module:** `error.rs`, `traits.rs`, `types/`, `tx_buffer.rs`, `snapshot.rs`, `ipc/`

| Prüfpunkt | Was genau prüfen |
|---|---|
| Error-Taxonomie | Sind alle Varianten von `MemFuseError` MECE? Gibt es überlappende oder fehlende Fehlerkategorien? |
| TxBuffer | Sharding-Logik korrekt? Race Conditions bei concurrent Writes? Flush-Semantik (partial flush möglich → Datenverlust?) |
| Trait-Defaults | Haben Default-Impls der Storage/Index-Traits sinnvolles Verhalten oder sind sie stille No-Ops die Bugs maskieren? |
| Types/Domain | Sind `CollectionId`, `NamespaceId` etc. typsicher (Newtype Pattern) oder rohe Strings? |
| Types/Filter | Filter-AST korrekt? Edge Cases (leere Filter, verschachtelte AND/OR, ungültige Felder)? |
| Types/Budget | Memory-Budget-Enforcement: Wird das Budget tatsächlich durchgesetzt oder nur empfohlen? |
| Snapshot | Snapshot-Konsistenz: Kann ein Snapshot einen inkonsistenten Zustand einfrieren? |
| IPC/FlatBuffers | Schema korrekt? Backward-Kompatibilität? Malformed-Input-Handling? |

---

#### CRATE: `memfuse-store` (LSM-Tree Storage Engine)
**Module:** `wal.rs`, `memtable.rs`, `sstable.rs`, `compaction.rs`, `lsm.rs`, `mmap.rs`, `checkpoint.rs`

| Prüfpunkt | Was genau prüfen |
|---|---|
| WAL Integrity | CRC32-Prüfung korrekt implementiert? Partial-Write-Recovery? HMAC-Chain ungebrochen? Was passiert bei korruptem WAL-Segment? |
| WAL Replay | Replayed Entries identisch mit Original? Idempotenz bei doppeltem Replay? |
| MemTable | Concurrent Insert/Read korrekt? Kapazitätsgrenzen enforced? Flush-Trigger deterministisch? |
| SSTable | Format-Validierung beim Lesen? Bloom-Filter false-positive Rate akzeptabel? Key-Range-Overlapping bei Level > 0? |
| Compaction | Correctness: Werden Tombstones korrekt propagiert? Werden Live-Reads während Compaction blockiert? Stale-Data nach Compaction möglich? |
| LSM Orchestration | Level-Sizing korrekt (ratio-based)? Write-Amplification messbar? Read-Amplification begrenzt? |
| MMap | Sichere Handhabung von Memory-Mapped Files? Korrekte Umgang mit truncated/corrupted Files? |
| Checkpoint | Atomarität: Kann Checkpoint einen halb-geschriebenen Zustand persistieren? Recovery von fehlgeschlagenem Checkpoint? |
| Encryption Integration | Korrekte Delegierung an `memfuse-crypto`? Klartext nie auf Disk ohne Encryption? |

---

#### CRATE: `memfuse-index` (HNSW Vector Index)
**Module:** `hnsw.rs`, `diskann.rs`, `distance.rs`, `quantize.rs`, `persistence.rs`

| Prüfpunkt | Was genau prüfen |
|---|---|
| HNSW Build | Graph-Aufbau korrekt (Navigable Small World Invariante)? Layer-Zuweisung randomisiert aber reproduzierbar? |
| HNSW Search | K-NN-Suche gibt korrekte Ergebnisse? Recall-Rate bei verschiedenen ef-Werten? Edge Case: leerer Index, 1 Element, identische Vektoren? |
| HNSW Delete | Werden gelöschte Knoten korrekt aus allen Layern entfernt? Phantom-Kanten möglich? Graph-Connectivity nach Delete? |
| SIMD Distance | Cosine/Euclidean/Dot: Mathematisch korrekt? NaN/Inf-Handling? Zero-Vector? SIMD ≡ Skalar? |
| SQ8 Quantization | Quantisierungs-Fehlerschranke bekannt und getestet? Encoding/Decoding roundtrip korrekt? |
| DiskANN | Korrektheit der Graph-basierten Suche auf Disk? I/O-Fehlerbehandlung? |
| Persistence | Index-Serialisierung/Deserialisierung roundtrip-korrekt? Korrupte Index-Datei → graceful Error? |
| Concurrency | Concurrent Insert/Search korrekt? Lock-Granularität optimal? |

---

#### CRATE: `memfuse-text` (BM25 Volltext-Suche)
**Module:** `bm25.rs`, `inverted.rs`, `tokenizer.rs`, `morphology.rs`

| Prüfpunkt | Was genau prüfen |
|---|---|
| BM25 Scoring | Mathematisch korrekte Formel (k1, b Parameter)? Division by Zero bei leeren Dokumenten? IDF korrekt? |
| Inverted Index | Posting-Listen korrekt maintainted bei Insert/Delete/Update? Concurrent Access sicher? |
| Tokenizer | Unicode-korrekt? Edge Cases: leerer String, nur Whitespace, emoji-only, mixed scripts? |
| Morphology | Stemming/Normalisierung korrekt für deutsche/englische Texte? False-Stem problematisch? |
| Metadata Updates | Tombstone-Semantik korrekt? Können Updates zu Ghost-Entries führen? |
| Document Frequency | DF/TF zählen korrekt bei Deletes aktualisiert? Stale Statistics möglich? |

---

#### CRATE: `memfuse-crypto` (Encryption at Rest)
**Module:** `crypto.rs`, `wal_crypto.rs`, `anti_tamper.rs`

| Prüfpunkt | Was genau prüfen |
|---|---|
| AES-GCM-SIV | Korrekte Nonce-Generierung (niemals wiederverwendet)? Key-Rotation möglich? |
| HKDF Key Derivation | Korrekte Context-Separation pro Datei/Namespace? Salt-Management sicher? |
| WAL HMAC Chain | Chain ungebrochen verifizierbar? Tamper-Detection zuverlässig? False Positives bei legitimen Änderungen? |
| Anti-Tamper | Welche Angriffsvektoren werden abgedeckt? Gap-Analyse: was fehlt? |
| Zeroize | Werden Schlüssel nach Gebrauch korrekt zeroized? Stack residuals? |
| Namespace Isolation | Kann ein Namespace-Key Daten eines anderen Namespace entschlüsseln? |
| Error Handling | Entschlüsselungsfehler → klar von I/O-Fehlern unterscheidbar? |

---

#### CRATE: `memfuse-graph` (CSR Graph)
**Module:** `csr.rs`

| Prüfpunkt | Was genau prüfen |
|---|---|
| CSR Builder | Graph-Aufbau korrekt? Duplizierte Edges handled? Self-Loops? |
| BFS/Traversal | Korrekte Traversierung? Cycle-Detection? Unreachable Nodes? |
| Transactional Edges | Edge Insert/Delete unter Concurrent Access korrekt isoliert? |
| Memory Bounds | Kann der Graph unbegrenzt wachsen? Kapazitätsgrenzen enforced? |

---

#### CRATE: `memfuse-db` (Orchestration Layer)
**Module:** `collection.rs`, `namespace.rs`, `fusion.rs`, `transaction.rs`, `chunker.rs`, `filter.rs`, `reaper.rs`, `context.rs`

| Prüfpunkt | Was genau prüfen |
|---|---|
| Collection CRUD | Create/Read/Update/Delete vollständig und korrekt? Race Conditions bei concurrent Create mit gleichem Namen? |
| Namespace Isolation | Sind Namespaces vollständig isoliert? Kann ein Query Namespace-Grenzen überschreiten? |
| RRF Fusion | Reciprocal Rank Fusion mathematisch korrekt? Edge Cases: leere Resultsets, identische Scores, k=0? |
| Transaction Isolation | Snapshot-Isolation korrekt? Dirty Reads möglich? Lost Updates möglich? Phantom Reads? |
| Atomic Commit | Commit über LSM + HNSW + BM25 atomar? Partial-Failure → Rollback vollständig? |
| Chunker | Text-Chunking korrekt? Überlappung/Segmentierung deterministisch? Edge Cases: leerer Text, sehr langer Text? |
| Filter Engine | Filter-Prädikate korrekt evaluiert? Short-Circuit-Optimierung? Ungültige Filter → klarer Fehler? |
| Reaper | Garbage Collection: werden tote Daten zuverlässig bereinigt? Kann der Reaper Live-Daten löschen? |
| Context Management | Kontexte korrekt isoliert? Memory-Leaks bei langlebigen Kontexten? |
| Multi-Signal Search | Werden alle 4 Signale (Vector, BM25, Graph, Metadata-Filter) korrekt kombiniert? |

---

#### CRATE: `memfuse-py` (Python Bindings)
**Module:** `lib.rs`

| Prüfpunkt | Was genau prüfen |
|---|---|
| Fassadenreinheit | Enthält `memfuse-py` eigene Geschäftslogik oder nur Delegation? |
| Error Mapping | Werden alle `MemFuseError`-Varianten korrekt auf Python-Exceptions gemappt? |
| GIL-Handling | Wird die GIL korrekt released bei blocking Rust-Ops? |
| Numpy-Integration | Werden Numpy-Arrays korrekt in Rust-Vektoren konvertiert (Zero-Copy wo möglich)? |
| Type Safety | Python-seitige Typfehler → klare Fehlermeldungen oder Silent Corruption? |
| Memory Safety | Können Python-Objekte Rust-Lifetime-Grenzen überleben → dangling refs? |

---

#### CRATE: `memfuse-embed` (ONNX Embedding)
**Module:** `lib.rs`

| Prüfpunkt | Was genau prüfen |
|---|---|
| Model Loading | Fehlerbehandlung bei fehlendem/korruptem Modell? |
| Batch Processing | Korrekte Batch-Dimension? OOM bei zu großem Batch? |
| Feature Guard | Ist die C-Dependency (ort) korrekt hinter einem Feature-Flag isoliert? |
| Normalization | Werden Embeddings normalisiert (L2)? Konsistenz mit externen Tools? |

---

#### CRATE: `memfuse-cluster` (Raft Consensus)
**Module:** `lib.rs`, `node.rs`, `network.rs`, `storage.rs`

| Prüfpunkt | Was genau prüfen |
|---|---|
| Raft Integration | Korrekte openraft-API-Nutzung? Log-Compaction? Snapshot-Übertragung? |
| Network Layer | TLS korrekt konfiguriert? Timeout-Handling? Partition-Tolerance? |
| Storage Backend | Konsistenz zwischen Raft-Log und lokaler Storage-Engine? |
| Split-Brain | Wird Split-Brain-Szenario korrekt behandelt? |

---

#### CRATE: `memfuse-checkpoint` (Frozen Zone)
**Module:** `lib.rs`

| Prüfpunkt | Was genau prüfen |
|---|---|
| Snapshot Atomarität | Kann ein halb-geschriebener Snapshot zum Zeitpunkt des Crash korrupt sein? |
| Concurrency | Concurrent Snapshot mit Live-Writes: konsistent? |
| Recovery | Restore eines Checkpoints → System-Zustand identisch mit Snapshot-Zeitpunkt? |

---

#### CRATE: `memfuse-sandbox` (Frozen Zone)
**Module:** `sandbox.rs`, `airgap.rs`, `host_functions.rs`

| Prüfpunkt | Was genau prüfen |
|---|---|
| WASM Isolation | Sind Host-Functions korrekt eingeschränkt? Kann WASM-Guest auf Host-Filesystem zugreifen? |
| Resource Limits | Fuel/Memory-Limits konfiguriert und enforced? |
| Air-Gap | Ist der Air-Gap wirklich dicht? Keine Seitenkanäle (Timing, Memory)? |

---

#### CRATE: `memfuse-saos-agent` (Frozen Zone)
**Module:** `engine.rs`, `step.rs`, `context.rs`, `audit.rs`, `graph.rs`

| Prüfpunkt | Was genau prüfen |
|---|---|
| Workflow Engine | Step-Transitions korrekt? Fehlzustände möglich (stuck workflows)? |
| Audit Trail | Lückenlose Protokollierung? Manipulationssicher? |
| Recovery | Workflow-Recovery nach Crash: Zustand konsistent? |
| Graph Integration | Task-Graph korrekt aufgebaut? Zyklische Dependencies erkannt? |

---

### Phase 3: Cross-Crate-Analyse

| Prüfpunkt | Was genau prüfen |
|---|---|
| DAG-Integrität | Cargo.toml-Dependencies gegen Architektur-DAG validieren. Jeder Import gegen die Schichtrichtung = Finding. |
| Error-Propagation | Werden Fehler über Crate-Grenzen korrekt übersetzt? Gehen Fehlerdetails verloren? |
| Feature-Flag-Isolation | Aktivieren optionaler Features (embed, cluster) ungewollte C-Deps im Default-Profil? |
| Shared-State | Gibt es globalen State (lazy_static, once_cell) der Namespace-Isolation verletzt? |
| Version-Konsistenz | Workspace-Dependencies konsistent? Gibt es Version-Mismatches? |

### Phase 4: Test-Coverage-Analyse

| Prüfpunkt | Was genau prüfen |
|---|---|
| Fehlende Tests | Welche pub-Funktionen haben NULL Testabdeckung? |
| Edge-Case-Lücken | Welche kritischen Edge Cases (leere Inputs, Overflow, Concurrent Access) fehlen in Tests? |
| Integration Gaps | Welche Cross-Crate-Pfade sind nicht durch Integration-Tests abgedeckt? |
| Regression-Risk | Welche Änderungen könnten bestehende Tests bestehen, aber trotzdem Fehler einführen? |

---

## BERICHTSFORMAT

### Pro Crate ein vollständiger Bericht in folgender Struktur:

```markdown
# Forensischer Audit-Bericht: [CRATE-NAME]

## 1. Executive Summary
- Gesamtbewertung: 🟢 Clean | 🟡 Warnung | 🔴 Kritisch
- Anzahl Findings: X Kritisch, Y Mittel, Z Niedrig
- Gesamteindruck in 3 Sätzen

## 2. Crate-Steckbrief
- LOC: ...
- Module: ...
- Abhängigkeiten (eingehend/ausgehend): ...
- Feature-Flags: ...

## 3. Invarianten-Compliance

| Invariante | Status | Evidence |
|---|---|---|
| Zero-Panic | ✅/❌ | Datei:Zeile oder "Clean" |
| ... | ... | ... |

## 4. Findings

### FIND-[CRATE]-[NR]: [Titel]
- **Severity:** 🔴 Kritisch / 🟡 Mittel / 🟢 Niedrig
- **Kategorie:** Panic-Surface | Logic-Error | Memory-Boundedness | Concurrency | Security | API-Contract | Missing-Test
- **Datei:** `path/to/file.rs`
- **Zeile(n):** L123–L145
- **Beschreibung:** Was genau ist das Problem?
- **Impact:** Was kann im schlimmsten Fall passieren?
- **Proof of Concept:** Eingabe/Sequenz die das Problem triggert
- **Empfohlene Behebung:** Konkreter Code-Fix oder Architektur-Vorschlag
- **Aufwand:** S/M/L

### FIND-[CRATE]-[NR+1]: ...
(Alle Findings auflisten, keine Zusammenfassungen)

## 5. Test-Gap-Analyse

| Funktion/Modul | Testabdeckung | Fehlende Szenarien |
|---|---|---|
| `fn batch_insert()` | ❌ Keine | Concurrent insert, OOM, Empty batch |
| ... | ... | ... |

## 6. Empfehlungen (priorisiert)
1. [Kritisch] ...
2. [Mittel] ...
3. [Niedrig] ...
```

---

## ABSCHLUSS-BERICHT (nach allen Crates)

```markdown
# Gesamtbericht: MemFuse Forensischer Audit

## 1. Executive Summary
- Gesamtanzahl Findings über alle Crates
- Top-5 kritischste Schwachstellen
- Architektur-Gesundheit (DAG, Schichtenreinheit)

## 2. Heatmap

| Crate | Panic | Logic | Memory | Concurrency | Security | Tests | Gesamt |
|---|---|---|---|---|---|---|---|
| core | 🟢 | 🟡 | 🟢 | 🟢 | 🟢 | 🟡 | 🟢 |
| store | ... | ... | ... | ... | ... | ... | ... |
| ... | | | | | | | |

## 3. Cross-Crate Findings
(Probleme die nur im Zusammenspiel der Crates sichtbar werden)

## 4. Priorisierte Roadmap zur Behebung
Phase 1 (sofort): Kritische Findings
Phase 2 (1 Woche): Mittlere Findings
Phase 3 (Backlog): Niedrige Findings

## 5. Metriken
- Gesamte Findings: X
- Davon Invarianten-Verletzungen: X
- Test-Coverage-Lücken: X Funktionen ohne Tests
- Estimated Technical Debt: X Personentage
```

---

## AUSFÜHRUNGSHINWEISE

1. **Vollständigkeitspflicht:** Jede `pub fn` und jeder `pub struct` in jedem Crate muss analysiert werden. "Sieht gut aus" ist kein Audit-Ergebnis.

2. **Quellcode lesen:** Der Audit basiert auf dem tatsächlichen Quellcode, nicht auf Dokumentation oder README-Behauptungen. Wenn die Docs sagen "Zero-Panic verifiziert", muss der Auditor das beweisen oder widerlegen.

3. **Reihenfolge:** Auditiere bottom-up entlang des DAGs:
   - Erst: `memfuse-core`
   - Dann Layer 1: `memfuse-crypto` → `memfuse-store` → `memfuse-graph` → `memfuse-index` → `memfuse-text`
   - Dann: `memfuse-db`
   - Dann: `memfuse-py`, `memfuse-embed`, `memfuse-cluster`
   - Zuletzt: Frozen Zone (`checkpoint`, `sandbox`, `saos-agent`)
   - Abschluss: Cross-Crate-Analyse + Gesamtbericht

4. **Keine Abkürzungen:** Auch wenn ein Crate "nur" 200 LOC hat (z.B. `memfuse-embed`), wird der vollständige Bericht erstellt. Kleine Crates können große Angriffsflächen haben.

5. **Beweise statt Behauptungen:** Jedes Finding enthält exakte Datei- und Zeilenreferenz. Jede "Clean"-Bewertung muss begründet werden.

6. **Frozen Zone beachten:** Die Frozen-Zone-Crates (`checkpoint`, `sandbox`, `saos-agent`) werden auditiert aber Behebungen nur empfohlen, nicht gefordert — es sei denn, es handelt sich um Sicherheitslücken.

7. **Test-Code ist nicht immun:** Auch `#[cfg(test)]`-Code wird auf Korrektheit geprüft — falsche Tests sind schlimmer als fehlende Tests, da sie falsche Sicherheit geben.

8. **Pro Crate ein separater Bericht:** Erstelle 13 einzelne Berichte + 1 Gesamtbericht = 14 Dokumente. Kein "zusammengefasstes" Format.
````

---

## Nutzungsanleitung

### Empfohlene Ausführung

Da das Projekt ~20.800 Zeilen Rust-Code umfasst, wird empfohlen:

1. **Modell-Wahl:** Ein LLM mit großem Kontextfenster (Gemini 2.5 Pro, Claude Opus, etc.)
2. **Chunking-Strategie:** Pro Crate einen separaten Session/Prompt:
   - Lade den gesamten `src/`-Ordner des Crates + Tests + [Cargo.toml](file:///home/freddy/Arbeitsplatz/DEV/memfuse/Cargo.toml)
   - Füge den crate-spezifischen Abschnitt des Prompts ein
   - Verlange den Einzelbericht

3. **Gesamtbericht:** Nachdem alle 13 Crate-Berichte vorliegen, lade alle Berichte in eine Session und verlange den Gesamtbericht mit Cross-Crate-Analyse.

### Datei-Beschaffung pro Crate

```bash
# Beispiel: Alle Sources für memfuse-store sammeln
fd -e rs . crates/memfuse-store/ | sort

# Output in Prompt einfügen:
for f in $(fd -e rs . crates/memfuse-store/ | sort); do
  echo "=== $f ==="
  cat "$f"
  echo ""
done
```

### Estimated Audit-Dauer
- Pro Crate: 15–45 Minuten (je nach LOC)
- Cross-Crate-Analyse: 30 Minuten
- **Gesamt: ~6-8 Stunden**
