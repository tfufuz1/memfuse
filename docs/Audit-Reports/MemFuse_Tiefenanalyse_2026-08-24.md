# MemFuse — Unabhängige Tiefenanalyse (Verifikations-Audit)

> **Hinweis zur Methodik**: Dieses Repository enthält bereits einen eigenen Audit-Bericht
> (`LLM_VIBE_CODING_AUDIT_UND_REPARATURPLAN.md`, datiert 2026-08-24), verfasst von einem
> früheren LLM-Agenten-Durchlauf im selben Projekt. Dieser Bericht wurde **nicht ungeprüft
> übernommen**. Stattdessen wurde jede seiner zentralen Behauptungen direkt am Quellcode
> nachverifiziert — mit dem Ergebnis, dass mehrere "OPEN"-markierte Findings tatsächlich
> bereits behoben sind, während andere zutreffend offen sind. Zusätzlich wurden **neue,
> bisher nicht dokumentierte Funde** identifiziert. Alle Aussagen unten sind durch konkrete
> Zeilennummern und Codezitate im tatsächlichen Repo-Zustand belegt.

---

## 1. Executive Summary

MemFuse ist architektonisch ambitioniert und in weiten Teilen ungewöhnlich reif für
LLM-generierten Code: echtes 2-Phase-Commit mit Compensating Transactions, HMAC-verkettetes
WAL, dokumentierte ADRs, eingebaute Selbst-Diagnose-Heuristiken (`is_suspicious_tx_id`) und
eine belastbare Test-Kultur. Gleichzeitig zeigt die Tiefenprüfung der untersten Schichten
mehrere **echte, noch offene Probleme**, die für ein Produkt mit dem Anspruch "souveräne
Unternehmensdaten als Kontext" und ACID-Garantie kritisch sind — insbesondere an der
Schnittstelle zwischen den soliden Kern-Crates (Layer 0–2) und der GUI-Schicht (Layer 4),
wo Zeitdruck und API-Lücken zu Kompromissen geführt haben.

**Kernbefund**: Die untersten Schichten (`memfuse-core`, `memfuse-store`, `memfuse-crypto`)
sind, bis auf einen verbleibenden Mmap-Race-Bug in `memfuse-index`, weitgehend so robust wie
behauptet. Die eigentlichen Schwachstellen liegen dort, wo Layer-2-Kern-APIs **keine sichere
öffentliche Schnittstelle** für Layer-4-Consumer (Tauri-App, MCP-Server) bereitstellen und
diese sich deshalb mit unsicheren Workarounds behelfen.

---

## 2. Abgleich mit dem bestehenden Audit-Bericht — was stimmt, was nicht

| Finding-ID (alt) | Bericht behauptet | **Verifizierter tatsächlicher Status** |
|---|---|---|
| BUG-02 (WAL HMAC hardcoded) | 🔴 OPEN | ✅ **TATSÄCHLICH BEHOBEN.** `load_or_create_integrity_key()` generiert und persistiert einen zufälligen 32-Byte-Schlüssel in `.wal_integrity_key` (0600-Rechte). `LEGACY_INTEGRITY_KEY` existiert nur noch als Migrationspfad für Alt-Datenbanken, mit Warnung. Der Bericht ist hier veraltet. |
| HIGH-05 (dupliziertes `EmbeddingProvider`) | 🟡 OPEN | ✅ **Bestätigt offen.** `memfuse-tauri/src/ingestion/pipeline.rs:17` definiert weiterhin ein eigenes Trait statt `memfuse_core::TextEmbeddingEngine` zu nutzen. |
| BUG-03 (TxId aus SystemTime) | 🔴 OPEN | ✅ **Bestätigt offen — und schlimmer als beschrieben.** Bei `EMBED_CONCURRENCY=8` parallelen Chunks kann die TxId-Kollision real auftreten, nicht nur theoretisch. Root Cause identifiziert: `Collection::next_tx` ist `pub(crate)` — es gibt **keine öffentliche API**, über die `memfuse-tauri` eine gültige TxId anfordern könnte. Das ist kein reiner Implementierungsfehler, sondern eine Design-Lücke in Layer 2. |
| BUG-05 (HNSW Lazy Validation) | 🟡 PARTIAL | ✅ **Tatsächlich vollständig behoben.** `try_new()` existiert, `new()` ist `#[deprecated]`, und alle Produktionsaufrufe (`memfuse-db`) nutzen bereits `try_new()`. Nur Testcode verwendet noch `new()` — unkritisch. |
| Silent I/O Failure (`let _ = dir.sync_all()`) | Als Findings gelistet | ✅ **Bestätigt offen, 4 Stellen.** `wal.rs:338,422,471` und `lsm.rs:125`. Bereits vom Projekt selbst mit `AI-TAG[SMELL][CRITICAL]` markiert, aber nicht behoben. |
| Unsafe Mmap in DiskANN | Als Risiko gelistet | 🟡 **Formal behoben (ADR-017 + SAFETY-Kommentar), aber die SAFETY-Begründung ist unvollständig** — siehe Abschnitt 4.1 für einen bislang nicht dokumentierten Race-Bug. |
| CSR `compact()` O(N)-Rebuild | Als Findings gelistet | ✅ **Bestätigt.** Jeder Aufruf iteriert über alle Knoten, unabhängig von der Anzahl neuer Kanten. |

**Fazit zum bestehenden Bericht**: Er ist eine gute Ausgangsbasis, aber teils veraltet
(Sicherheits-Fix bereits gelandet, dort aber noch als offen geführt) und deckt mehrere reale
Probleme in den Randbereichen (memfuse-tauri, memfuse-mcp, GUI, memfuse-embed) gar nicht ab.

---

## 3. Neue, bisher nicht dokumentierte Funde

### 3.1 KRITISCH — Race Condition zwischen `DiskANN::build()` und `::load()` via Mmap
**Datei**: `crates/memfuse-index/src/diskann.rs:340-345` (Write) vs. `:488-492` (Read)

`build()` öffnet die Indexdatei mit `.truncate(true)` und schreibt in-place auf denselben
Pfad, den `load()` per `Mmap::map()` mappt. Es gibt **kein Atomic-Rename-Pattern**
(write-to-temp + rename). Läuft `load()` (z.B. durch einen anderen Such-Thread) während ein
`build()` gerade `.truncate(true)` ausführt, wird die gemappte Region unter der Nase des
Lesers verkürzt → potenzieller SIGBUS-Crash oder undefiniertes Verhalten. Der bestehende
`// SAFETY:`-Kommentar (ADR-017) prüft nur die Gültigkeit des Dateideskriptors beim Öffnen,
nicht aber die Nebenläufigkeitsgefahr durch gleichzeitige Schreibzugriffe auf dieselbe Datei
— genau die klassische Mmap-Falle, die die eigentliche Rechtfertigung für einen "rigorous
SAFETY proof" laut CONSTITUTION.md wäre.
**Empfehlung**: `build()` muss in eine temporäre Datei schreiben und atomar per `rename()`
den Indexpfad ersetzen, damit ein aktiver `mmap()`-Reader stets eine konsistente,
unveränderliche Datei sieht (POSIX-Semantik: bereits offene FDs/Mmaps bleiben nach `rename`
auf der alten Inode gültig).

### 3.2 KRITISCH — `memfuse_insert` im MCP-Server chunked nicht, trotz gegenteiliger Doku
**Datei**: `crates/memfuse-mcp/src/lib.rs:88-100, 189-218`

Die Tool-Beschreibung verspricht "Dokument einspeichern (auto-embedding, **auto-chunking**)".
Tatsächlich wird der komplette `text` als ein einziges Embedding gespeichert — kein Aufruf
von `MarkdownChunker` o.ä. Bei langen Dokumenten (mehrere tausend Wörter) führt das zu stark
verwässerten Embeddings und damit zu schlechter Retrieval-Qualität — dem Kernversprechen des
Produkts direkt entgegengesetzt. Das ist der wichtigste funktionale Bug im gesamten Audit,
weil er die USP ("4-Signal-Fusion für optimalen LLM-Prompt-Kontext") in der über MCP
zugänglichen Insert-Route unterläuft.

### 3.3 HOCH — Fehlerhafte Fehlerpropagation in `repair_on_open`
**Datei**: `crates/memfuse-db/src/lib.rs:237-299`

Wenn eine Collection nicht repariert werden kann (`all_repairs_succeeded == false`), wird
das nur geloggt (`tracing::error!`). Die Funktion gibt trotzdem `Ok(())` zurück, und
`open_with_config()` meldet dem Aufrufer fälschlicherweise erfolgreichen Start — obwohl
nachweislich eine Collection in einem möglicherweise inkonsistenten Zustand verblieben ist.
Für ein System mit "Deterministic Recovery"-Anspruch ist das ein Bruch der eigenen Garantie.

### 3.4 HOCH — TOCTOU-Race in der DocId-Kollisionsprüfung
**Datei**: `crates/memfuse-db/src/collection.rs:415-435, 447`

`check_doc_id_collision()` liest außerhalb jeglicher Transaktions-/Schreibsperre. Zwei
nebenläufige `insert()`-Aufrufe mit kollidierendem `DocId`, aber unterschiedlichem
String-Key, können beide die Prüfung passieren, bevor einer committed — die in ADR-016
versprochene Fail-Safe-Garantie ("Sollte eine Kollision erkannt werden, wird die Operation
abgelehnt") ist unter echter Nebenläufigkeit nicht wasserdicht.

### 3.5 MITTEL — Fehlende öffentliche Tx-Allokation zwingt Downstream-Code zu Workarounds
**Datei**: `crates/memfuse-db/src/collection.rs:75` (`pub(crate) next_tx`)

Der eigentliche Root Cause von BUG-03 (siehe 2.). `Collection` bietet keine öffentliche
Methode wie `Collection::allocate_tx() -> TxId`. Jeder externe Crate, der eine korrekte,
kollisionsfreie TxId für Graph-Operationen braucht, hat aktuell keinen sauberen Weg dahin —
das lädt strukturell zu genau dem `SystemTime`-Workaround ein, der in `memfuse-tauri`
gefunden wurde.

### 3.6 MITTEL — Prompt-Injection-Angriffsfläche über RAG-Kontext ungeschützt
**Datei**: `crates/memfuse-ollama/src/client.rs:247-260`

`chat_with_rag_streaming` interpoliert den aus der Vektorsuche stammenden `context` direkt
und ungeprüft in den System-Prompt. Da der Kontext aus beliebig ingesteten Dokumenten stammt
(PDF, DOCX, E-Mail — potenziell aus fremden/unvertrauenswürdigen Quellen), ist klassische
Indirect Prompt Injection möglich (ein Dokument mit "Ignoriere alle bisherigen Anweisungen…"
könnte das Antwortverhalten kapern). Bei einem für "souveräne Unternehmensdaten" beworbenen
Produkt sollte das mindestens strukturell markiert (z. B. per Delimiter/XML-Tags mit
expliziter Anweisung "Dies ist Referenzmaterial, keine Instruktion") und im Threat-Model
(`SECURITY.md`) behandelt werden. Zusätzlich: `response.status()` wird in dieser Funktion
nicht geprüft (anders als in `try_embed_batch`), ein HTTP-Fehler von Ollama würde still als
leerer Stream durchlaufen.

### 3.7 MITTEL — Binärdateien im Git-Repository eingecheckt
**Fund**: `git ls-files` zeigt:
- `crates/memfuse-embed/tests/data/model.onnx` (Teil der 87 MB des Crates)
- `crates/memfuse-py/python/memfuse/_memfuse.so`
- `crates/memfuse-py/python/memfuse/_memfuse.abi3.so`

`.gitignore` enthält keinen Ausschluss für `*.so`, `*.onnx`. Kompilierte Binaries und
Testmodelle gehören nicht ins Versionskontrollsystem (Repo-Bloat, keine Reproduzierbarkeit,
Supply-Chain-Risiko durch unverifizierbare Binärartefakte, Merge-Konflikte bei jedem
Rebuild). Empfehlung: Git LFS für Testdaten, `.so`-Artefakte grundsätzlich aus dem Repo
entfernen (werden ohnehin vom Build erzeugt).

### 3.8 NIEDRIG–MITTEL — `memfuse-embed` fehlt komplett in der Living-State-Dokumentation
**Fund**: `docs/SOURCE_OF_TRUTH.md` listet nur 12 Crates, `memfuse-embed` fehlt, obwohl es
im Workspace registriert ist (`Cargo.toml:12`) und produktiven Code enthält (ONNX-basiertes
lokales Embedding als Alternative zu Ollama). Das verstößt gegen die im Projekt selbst
festgelegte Regel: *"Muss in derselben Transaktion/PR wie der Code aktualisiert werden."*
Zusätzlich: Der Crate steht standardmäßig deaktiviert (`default = []`), was den 100%
Pure-Rust-USP ("Sovereign Core... Ollama HTTP Integration for Local LLM") relativiert — es
gibt bereits eine ONNX-In-Process-Alternative, die aber unfertig/unbeworben im Repo liegt.

### 3.9 NIEDRIG — XSS-Lücke durch ungeescapte Collection-/Dateinamen im Frontend
**Datei**: `crates/memfuse-tauri/ui/app.js:44-47, 133-139, 178-183`

Die App verwendet an mehreren Stellen konsequent eine `escapeHtml()`-Hilfsfunktion (z. B.
für Chat-Nachrichten und Suchergebnis-Texte, korrekt umgesetzt), aber **nicht** für
Collection-Namen und Dateinamen, die via `innerHTML` gerendert werden. Bei einer lokalen
Desktop-App ist die Ausnutzbarkeit begrenzt, aber sobald Collections aus geteilten
DB-Ordnern importiert werden können, ist DOM-XSS über einen präparierten Collection-Namen
real möglich.

### 3.10 NIEDRIG — Panic-Risiko im ONNX SessionPool
**Datei**: `crates/memfuse-embed/src/lib.rs:40-46`

`SessionPool::pop()` verwendet `.expect("SessionPool exhausted, semaphore leak?")`. Zwar
durch ein Semaphore theoretisch geschützt, aber jede zukünftige Änderung, die diese
Invariante bricht (z. B. Panic zwischen `SessionGuard::new` und dem ersten `Drop`), würde
den gesamten Prozess abstürzen lassen — direkter Verstoß gegen die eigene
No-Panic-Policy für Produktionscode.

---

## 4. Bestätigte, bereits gut gelöste Kern-Mechanismen (positiv)

Zur Einordnung — nicht alles ist ein Problem. Folgende Kernmechanismen wurden geprüft und
sind tatsächlich robust implementiert:

- **2-Phase-Commit mit Compensating Transactions** (`memfuse-db/src/transaction.rs`):
  Explizite Retry-Logik (3 Versuche, 100 ms Backoff) bei fehlgeschlagenem Index-Commit nach
  erfolgreichem Storage-Commit, mit klarer Split-Brain-Warnung im Log. Ungewöhnlich
  sorgfältig für automatisch generierten Code.
- **DocId-Kollisionserkennung** (ADR-016): Sauber vom Kern in `memfuse-db::Collection`
  ausgelagert und getestet (abgesehen von der TOCTOU-Lücke unter Nebenläufigkeit, s. 3.4).
- **WAL-Integritätsschlüssel-Management**: Zufällige Schlüsselgenerierung mit
  Datei-Rechten 0600 und sauberem Legacy-Migrationspfad.
- **HNSW `try_new()`**: Vollständig auf Fail-Fast umgestellt in Produktionscode.
- **Ollama-Client Retry/Backoff & Batch-Fallback**: Sauber implementiert mit
  Längenprüfung und automatischem Sequenziell-Fallback.
- **PyO3-FFI-Schicht**: Konsequente `allow_threads`-Nutzung, kein GIL-Deadlock-Risiko
  erkennbar, panic-freie Runtime-Initialisierung.
- **CSR-Graph Selbst-Diagnose**: `is_suspicious_tx_id()`-Heuristik warnt bereits aktiv vor
  genau der Art von TxId-Bug, die in `memfuse-tauri` gefunden wurde — das Kern-Team hat das
  Problem also bereits antizipiert, nur die Downstream-Behebung fehlt noch.

---

## 5. Priorisierte Empfehlungsliste

| # | Finding | Schweregrad | Betroffene Schicht |
|---|---|---|---|
| 1 | Mmap-Race zwischen `build()`/`load()` in DiskANN (3.1) | **Kritisch** | Layer 1 (Kern) |
| 2 | Fehlendes Chunking in `memfuse_insert` (MCP) (3.2) | **Kritisch** (Produktqualität) | Layer 4 |
| 3 | `next_tx` nicht öffentlich zugänglich → TxId-Workarounds (2., 3.5) | Hoch | Layer 2 API-Design |
| 4 | Silent fsync-Failures im WAL/LSM (4 Stellen) | Hoch | Layer 1 (Kern) |
| 5 | `repair_on_open` verschluckt Fehlerzustand (3.3) | Hoch | Layer 2 |
| 6 | TOCTOU in DocId-Kollisionsprüfung (3.4) | Mittel | Layer 2 |
| 7 | Ungeschützte Prompt-Injection-Fläche im RAG-Kontext (3.6) | Mittel | Layer 3 |
| 8 | Binärartefakte im Git-Repo (3.7) | Mittel | Repo-Hygiene |
| 9 | CSR `compact()` O(N)-Rebuild bei jedem Commit | Mittel | Layer 1 |
| 10 | `memfuse-embed` fehlt in SOURCE_OF_TRUTH.md (3.8) | Niedrig–Mittel | Doku-Governance |
| 11 | Duplicated `EmbeddingProvider`-Trait (3. Tabelle) | Niedrig | Layer 4 |
| 12 | XSS via ungeescapte Namen im Frontend (3.9) | Niedrig | GUI |
| 13 | Panic-Risiko SessionPool (3.10) | Niedrig | Layer 3 (optional) |

---

## 6. Zum Ziel "marktreifes Produkt" — strukturelle Beobachtungen jenseits von Bugs

- **GUI-Reife**: Das aktuelle Frontend (`ui/app.js`, `index.html`) umfasst ca. 600 Zeilen
  Vanilla-JS ohne Komponentenframework. Für den Anspruch "extrem professionelle GUI zur
  Verwaltung des gesamten Kontextes und RAG-Systems" ist das aktuell ein Prototyp-Stand,
  kein marktreifes Interface — es fehlen u. a. Collection-Analytics/Dashboards, Vector-/
  Graph-Visualisierung, granulare Fusion-Gewichts-Einstellungen in der UI (die Backend-Logik
  dafür existiert bereits über `FusionWeights`), Nutzer-/Rechteverwaltung und ein
  strukturiertes Error-/Retry-Handling im UI selbst.
- **Governance-Lücke**: Das Projekt hat vorbildliche Prozessartefakte (CONSTITUTION.md,
  ADRs, `AI-TAG`-Marker-System), aber die Praxis zeigt, dass als "kritisch" markierte
  Findings (`AGT-AUDIT-005/006`) über mehrere Audit-Zyklen hinweg unbehoben bleiben. Ein
  tatsächlicher CI-Gate, der offene `AI-TAG[SMELL][CRITICAL]`-Marker als Build-Fehler
  behandelt, würde diese Lücke schließen.
- **Fehlende Enterprise-Grundfunktionen** für "souveräne Unternehmensdaten": kein sichtbares
  Audit-Log für Zugriffe/Änderungen, keine Mandantentrennung über Collections hinaus, keine
  Backup/Restore-Automatisierung in der GUI, keine Rate-Limits/Quotas im MCP-Server gegen
  Resource-Exhaustion durch fehlerhafte LLM-Tool-Aufrufe.

---

*Bericht erstellt durch direkte Code-Verifikation aller 13 Workspace-Crate-Verzeichnisse
(11 aktive + `memfuse-embed` + Doppelzählung durch fehlende SOT-Pflege). Alle Funde sind an
konkreten Datei-/Zeilenangaben im Repo-Zustand zum Zeitpunkt des Klonens nachvollziehbar.*
