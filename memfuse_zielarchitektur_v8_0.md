# MemFuse — Zielarchitektur-Spezifikation v8.0
## „Von 18 LLM-Task-Crates zu 7 Produkt-Modulen"

> **Dokument-Typ:** Ziel-/Soll-Architektur (keine Live-Code-Verifikation). Dieses Dokument beschreibt bewusst **nicht** den aktuellen Repo-Zustand — der ändert sich mehrfach täglich und jede Momentaufnahme ist binnen Stunden veraltet (siehe Anhang E für die Begründung dieser Designentscheidung). Stattdessen definiert es die Architektur, auf die refaktoriert werden soll, und liefert für jede Änderung Begründung, Abgrenzung und einen für Implementierungs-Sessions direkt konsumierbaren Auftragszuschnitt.
> **Adressat:** Diese Spezifikation ist so geschrieben, dass eine andere Claude-Instanz (oder ein menschlicher Reviewer) daraus ohne Rückfrage an den Projektinhaber konkrete Jules-Prompts, PR-Zuschnitte und Reihenfolgen ableiten kann. Jeder Abschnitt liefert: **Ist-Problem → Ziel-Zustand → Begründung → Abgrenzung/Risiko → Akzeptanzkriterium.**
> **Ersetzt:** v6.0/v7.0 („Verified Live-State"-Dokumente) werden nicht fortgeschrieben. Diese Spezifikation ist komplementär: v7.0-artige Dokumente beantworten „was ist gerade im Code", dieses Dokument beantwortet „wohin soll der Code, und warum".
> **Kontext, der diese Spezifikation prägt:** ~195 parallele/serielle Google-Jules-Tasks pro Tag sind der limitierende Engpass nicht in Rechenkapazität, sondern in **Koordinationsdisziplin**. Jede hier vorgeschlagene Änderung ist deshalb explizit so geschnitten, dass sie von einem einzelnen Jules-Task ohne Kontext-Überlauf sicher umsetzbar ist — siehe §7 für das dazugehörige Entwicklungssetup.

---

## Inhaltsverzeichnis

- **§0** Ausgangslage: Was dieses Dokument löst, was es nicht löst
- **§1** Produktthese — was „besser als MinnsDB" konkret bedeutet
- **§2** Ziel-Architekturprinzipien (ersetzt P1–P12 aus v7.0, wo abweichend)
- **§3** Ziel-Crate-Topologie: von 18 auf 7 Module
- **§4** Modul-für-Modul-Spezifikation mit Migrationspfad
- **§5** Feature-Konsolidierung: vom „Physio"-Katalog zum Produkt-Feature-Set
- **§6** Die drei Visionen — Entscheidung statt Koexistenz
- **§7** Ziel-Entwicklungssetup: Prompter-Tooling, Rollen-Trennung, Anti-Drift-Mechanismen
- **§8** Migrationsreihenfolge (Phasenplan, Jules-Task-Zuschnitt)
- **§9** Erfolgsmessung: welche Zahl entscheidet, ob das funktioniert hat
- Anhang A: Crate-Mapping-Tabelle (alt → neu, jede Datei referenziert)
- Anhang B: Was aus dem APM-41-Katalog in die neue Struktur übernommen wird
- Anhang C: Was aus VETOES.md unverändert gilt
- Anhang D: Offene Entscheidungen, die EIN Mensch treffen muss (nicht Jules)
- Anhang E: Warum dieses Dokument keine Live-Code-Zeilen zitiert

---

## §0 Ausgangslage: Was dieses Dokument löst, was es nicht löst

**Löst:**
1. Die Crate-Struktur folgt aktuell LLM-Task-Ergonomie (kleine, isoliert kompilierbare Häppchen für einzelne Jules-Sessions), nicht Produkt-Architektur (klare Schichtgrenzen, minimale Kopplung). Dieses Dokument zieht neue Grenzen nach Verantwortung statt nach Task-Größe.
2. Drei nie entschiedene, parallel weitergeführte Produktvisionen (PyPI-Library, Desktop-Enterprise-App, Voice/Jarvis-Assistent — dokumentiert in ADR-007/ADR-018 als gleichzeitig „final") binden Entwicklungskapazität ohne Fokus. Dieses Dokument erzwingt eine Entscheidung (§6).
3. Das bestehende Prompter/APM-Tooling ist wertvoll, aber an die alte 15/18-Crate-Zählung gekoppelt und wird bei jedem Refactoring erneut von Hand nachgepflegt (siehe v2 vs. v22 Drift). §7 macht dieses Tooling strukturell resistent gegen zukünftige Architekturänderungen.

**Löst nicht, und das ist bewusst:**
- Dieses Dokument ersetzt keine Benchmark-Messung. Die in früheren Sitzungen identifizierte Lücke — CI-„Regression Gate" lief gegen 2 Fixture-Fälle statt echter LongMemEval-/LoCoMo-Datensätze — ist eine Datenintegrations-Aufgabe, kein Architekturproblem, und sollte parallel, unabhängig von diesem Refactoring, umgesetzt werden. Ohne diese Zahl ist jede hier vorgeschlagene Strukturänderung zwar architektonisch begründbar, aber empirisch nicht als „besser" nachweisbar.
- Dieses Dokument trifft keine Entscheidung zwischen den drei Visionen — es liefert die Faktenlage, mit der EIN Mensch (du) die Entscheidung trifft, weil kein Jules-Task und keine Architektur-Spezifikation eine Produktentscheidung ersetzen kann (siehe Anhang D).

---

## §1 Produktthese — was „besser als MinnsDB" konkret bedeutet

**Ausgangsbeobachtung (aus vorherigen Sitzungen, hier als Prämisse übernommen):** MinnsDB gewinnt nicht durch überlegene Einzelalgorithmen, sondern durch **Fokus** — ein Binary, ein Storage-Backend (`redb`), eine klar kommunizierte Fähigkeit (bi-temporale Kanten, kaskadierende Invalidierung, Multi-Signal-Fusion). MemFuse hat in der Summe mehr Fähigkeiten, aber keine davon ist so zugeschnitten, dass sie in einem Satz kommunizierbar ist, und die Crate-Struktur macht das strukturell schlimmer, nicht besser.

**Konsequenz für diese Spezifikation: „Besser als MinnsDB" heißt NICHT "mehr Features als MinnsDB".** Es heißt, eine von zwei Positionen glaubwürdig zu besetzen:

**Position A — Schlanker als MinnsDB UND fokussiert.** Ein Single-Binary-Betriebsmodus (analog zu MinnsDB), der 80% der Nutzer bedient, mit dem Krypto-Härtungs-Alleinstellungsmerkmal (HMAC-WAL-Kette, kryptografischer Löschbeweis) als Hauptverkaufsargument für Compliance-sensible Anwendungsfälle, die MinnsDB nicht adressiert.

**Position B — Strukturell überlegen für einen Anwendungsfall, den MinnsDB nicht abdeckt.** Der wahrscheinlichste Kandidat im aktuellen Code ist die Kombination aus DiskANN-Out-of-Core-Fähigkeit (Speicherkorpora, die nicht in RAM passen) und dem Krypto-Löschbeweis — also: „Agent-Memory für sehr große, langlaufende Deployments mit Audit-Pflicht", nicht „Agent-Memory für den einzelnen Entwickler-Laptop", wo MinnsDB bereits gut genug ist.

Beide Positionen sind mit dem vorhandenen Code erreichbar. Beide sind **nicht gleichzeitig als Hauptbotschaft kommunizierbar** — das ist exakt das Muster, das die drei-Visionen-Problematik erzeugt hat. §6 verlangt eine Entscheidung zwischen A und B (oder eine explizite Sequenzierung: erst A, B als spätere Erweiterung).

**Warum 195 Jules-Tasks/Tag diese Entscheidung nicht ersetzen, sondern dringlicher machen:** Bei dieser Kapazität ist der Engpass nicht „wie schnell kann Code geschrieben werden", sondern „wie viele der geschriebenen Zeilen tragen zur gewählten Position bei". Ungerichtete Kapazität bei hoher Taktung erzeugt schneller Drift, nicht langsamer — das 1319-Commits-Muster aus dem bisherigen Verlauf (Konzeptphase Mai, Stille Juni/Juli, Explosion August/September) ist der Beleg: Kapazität wurde eingesetzt, sobald sie verfügbar war, unabhängig davon, ob eine Zielarchitektur feststand.

---

## §2 Ziel-Architekturprinzipien (ersetzt P1–P12 aus v7.0, wo abweichend)

Die bestehenden P1–P12-Prinzipien (DAG-Integrität, Zero-Panic-Doctrine, WAL-First, Backend-Agnostizismus, Kein-Cloud-Zwang, ADR-Singularität, Marketing-Code-Bindung, Kalibrierungs-Integrität, Kein-Klartext-Speicher, Reuse-vor-Neubau, Latenzbudget-Pflicht, Physio-Feature-Default-Unsichtbarkeit) sind größtenteils gut und bleiben **inhaltlich** gültig. Diese Spezifikation ergänzt sie um vier neue, die die Crate-Explosion und Vision-Drift strukturell verhindern sollen — künftig **P13–P16**:

**P13 — Modulgrenzen folgen Verantwortung, nicht Task-Ergonomie.** Eine neue Crate-Grenze ist nur zulässig, wenn sie eine der drei folgenden Bedingungen erfüllt: (a) unterschiedliches Kompilierungsziel (z. B. `memfuse-py` als FFI-Grenze mit eigenem Panic-Profil), (b) echte optionale Abschaltbarkeit ohne Funktionsverlust im Kern (z. B. GUI-Frontend), (c) unabhängige Versionierungsfähigkeit für externe Konsumenten. „Diese Datei wurde in einer eigenen Jules-Session geschrieben" ist **keine** gültige Begründung für eine Crate-Grenze — Task-Zuschnitt ist eine Prozessfrage (siehe §7), keine Architekturfrage.

**P14 — Jedes Subsystem hat genau einen Scheduler/Orchestrator.** Aktuell existieren mehrere teilüberlappende Hintergrund-Ausführungspfade für Wartungsaufgaben (Decay/Thermostat, NREM/Memory-Consolidation, Reaper, Maintenance-Scheduler, Consolidation-Executor — mindestens fünf Dateien mit ähnlicher Verantwortung, siehe §4.3). Ziel: **ein** Scheduler-Trait mit registrierbaren Tasks, keine parallel gewachsenen Einzelimplementierungen. Diese Konsolidierung ist keine Kosmetik — die K17/K18-Historie (zwei nicht-koordinierte Reaper-Pfade, ein Feature-Flag-Bug durch Doppelimplementierung) zeigt konkret, welche Fehlerklasse aus fehlender Single-Scheduler-Disziplin entsteht.

**P15 — Eine Produktvision pro Release-Branch, keine stille Koexistenz.** ADR-007 und ADR-018 haben nachweislich zwei „finale" Strategien parallel geführt, ohne dass eine die andere formal ablöste. Ab dieser Spezifikation gilt: Eine neue Produktvisions-Entscheidung macht die vorherige ADR **explizit `superseded`** (nicht nur implizit durch Nichterwähnung), und Code-Pfade der abgelösten Vision werden binnen einer definierten Frist physisch entfernt, nicht nur ungenutzt liegen gelassen (vgl. §6).

**P16 — Architekturdokumente sind Ziele, keine Zustandsprotokolle, und werden nicht mit Zustandsprotokollen vermischt.** Der wiederkehrende Fehler der v6.0/v7.0-Dokumentreihe war, Ist-Zustand und Soll-Zustand im selben Dokument zu führen — das erzeugt genau die Stunden-Halbwertszeit-Problematik, die in vorherigen Prüfungen aufgefallen ist. Ab sofort: Ist-Zustand lebt ausschließlich in autogenerierten Artefakten (`WORKING_STATE.md`, `docs/audits/`, CI-Ergebnissen). Soll-Zustand lebt in versionierten, von Menschen geschriebenen/verantworteten Dokumenten wie diesem, die sich nur bei einer echten Architekturentscheidung ändern — nicht bei jedem Commit.

---

## §3 Ziel-Crate-Topologie: von 18 auf 7 Module

### §3.1 Prinzip der Zusammenlegung

Jede Zusammenlegung unten erfüllt eine Bedingung aus P13: entweder es gibt **keine** echte Notwendigkeit für getrennte Kompilierung/Versionierung, oder die getrennten Crates haben bereits eine so enge gegenseitige Abhängigkeit (siehe die 8-von-16-Abhängigkeiten von `memfuse-db`), dass die Grenze nur Boilerplate erzeugt, aber keine Kopplung verhindert.

### §3.2 Zielbild (7 Kern-Module + 2 Grenzschicht-Module + 1 Werkzeug-Crate)

```
KERN (in-process, ein Konsument kompiliert alles zusammen):

  memfuse-foundation        [ersetzt: memfuse-core]
    Typen, Traits, Fehlerbehandlung, IPC-Schema. Keine Änderung nötig — bleibt eigenständig,
    da ALLE anderen Module hiervon abhängen (P13-Bedingung (c): unabhängige Versionierung
    als Fundament ist gerechtfertigt).

  memfuse-security           [ersetzt: memfuse-crypto + memfuse-kv-bridge]
    Begründung: memfuse-kv-bridge hat 355 LOC und existiert nur, weil es in einer eigenen
    Jules-Session entstand. Es hat keine unabhängige Konsumentenschaft (kein anderes Crate
    verwendet NUR kv-bridge ohne crypto) und keinen eigenen Kompilierungsgrund.
    Zusammenlegung macht das Sicherheits-Alleinstellungsmerkmal (HMAC-WAL, DeletionProof,
    KV-Segment-Verschlüsselung) zu EINEM auditierbaren, klar abgegrenzten Modul —
    was auch die Vermarktung als Compliance-Feature erleichtert (siehe §1, Position A/B).

  memfuse-persistence         [ersetzt: memfuse-store + memfuse-checkpoint]
    Begründung: Checkpoint/Backup ist konzeptionell Teil der Persistenzverantwortung
    („wie wird Zustand dauerhaft gemacht"), nicht eine separate Schicht. Beide haben
    identische Nutzergruppe (nur memfuse-db/memfuse-persistence-orchestrator konsumiert
    beide). Eigenständige Versionierung nicht erforderlich.

  memfuse-retrieval            [ersetzt: memfuse-index + memfuse-graph + memfuse-text]
    Begründung: Dies ist die größte, aber am besten begründbare Zusammenlegung. Vektor-Index,
    Graph und Volltext sind die drei Fusionssignale EINES Retrieval-Subsystems — sie werden
    nie unabhängig voneinander eingesetzt, ihre Datenstrukturen referenzieren einander
    bereits querweise (Graph-Kanten zeigen auf DocIds aus dem Index, BM25-Postings laufen
    durch dieselbe StorageEngine). Die aktuelle Trennung in drei Crates verhindert nicht
    Kopplung — sie versteckt sie nur hinter Cargo.toml-Grenzen. Untermodule bleiben intern
    klar getrennt (`retrieval::vector`, `retrieval::graph`, `retrieval::text`), aber als
    ein Kompilierungs-/Versionierungs-/Test-Verbund.

  memfuse-orchestrator          [ersetzt: memfuse-db, entschlackt]
    Begründung: Bleibt eigenständig als Layer-2-Fassade (P1-DAG-Regel), ABER intern radikal
    aufgeräumt: die fünf teilüberlappenden Wartungsdateien (decay_controller.rs,
    maintenance_scheduler.rs, reaper.rs, consolidation_executor.rs, memory_consolidation.rs)
    werden zu EINEM Scheduler-Modul mit registrierbaren Tasks konsolidiert (P14).
    Fusion-Logik (fusion.rs), Transaction/MVCC (transaction.rs) und Collection-API bleiben
    als klar getrennte Submodule bestehen — diese Trennung ist bereits sinnvoll und wird
    nicht angetastet.

  memfuse-inference           [ersetzt: memfuse-calibration + memfuse-ollama + memfuse-candle
                               + memfuse-router + memfuse-embed]
    Begründung: Alle fünf betreffen „wie wird ein Sprachmodell/Embedding-Modell angesprochen,
    kalibriert und geroutet". memfuse-candle (718 LOC) und memfuse-embed (1.882 LOC, bereits
    optional) sind zu klein, um eigenständige Versionierungs-/Kompilierungsgründe zu
    rechtfertigen. Feature-Flags übernehmen die Rolle, die bisher Crate-Grenzen hatten:
    `inference-candle`, `inference-onnx-embed`, `inference-ollama` als Cargo-Features EINES
    Moduls statt drei/vier separater Crates. Das behält Backend-Agnostizismus (P4) bei,
    ohne die Compile-Graph-Aufblähung.

  memfuse-agentic              [ersetzt: memfuse-agent, unverändert eigenständig]
    Begründung: Workflow-Engine mit eigener Verantwortung (Dead-Letter-Queue, Audit-Trail,
    Token-Budget) — echte fachliche Eigenständigkeit, keine Änderung nötig.

GRENZSCHICHT (bewusst als LEAF-Crates isoliert — hier ist Trennung korrekt, weil sie
optionale, unabhängig ausrollbare Konsumenten sind, nicht weil sie an unterschiedlichen
Tagen geschrieben wurden):

  memfuse-mcp                  [unverändert] — einziger externer Protokoll-Kanal.

  memfuse-bindings             [ersetzt: memfuse-py + memfuse-tauri, als Cargo-Features
                               EINES Grenzschicht-Crates ODER als zwei separate Leaf-Crates,
                               siehe §6 — abhängig von der Vision-Entscheidung]
    Begründung: Solange keine Vision-Entscheidung getroffen ist (§6), bleiben beide als
    getrennte Leaf-Crates bestehen (P13-Bedingung (a) gilt für memfuse-py: eigenes
    Panic-Profil ist ein echter Kompilierungsgrund). memfuse-tauri bleibt vorerst separat,
    ist aber der wahrscheinlichste Kandidat für vollständige Entfernung, falls Position A
    (Single-Binary) gewählt wird.

WERKZEUG:

  xtask                        [unverändert] — DAG-Check, Vetoes-Check, Duplicate-Symbol-Gate.

  memfuse-bench                [unverändert, aber CI-Anbindung überarbeiten — siehe separate,
                               bereits beauftragte Aufgabe zur LongMemEval/LoCoMo-Integration]
```

### §3.3 Ergebnis in Zahlen

| | Ist (v7.0-Stand) | Ziel |
|---|---|---|
| Kern-Crates | 12 (core, calibration, candle, checkpoint, crypto, graph, kv-bridge, text, embed, index, ollama, store) | 6 (foundation, security, persistence, retrieval, orchestrator, inference) |
| Layer-3/4-Crates | 5 (db, agent, router, tauri, bench) | 3 (orchestrator bereits oben gezählt, agentic, bench) |
| Grenzschicht | 3 (mcp, py, tauri) | 2–3, abhängig von §6-Entscheidung |
| **Gesamt Workspace-Members** | **18** | **9–10** |

Diese Reduktion ist kein Selbstzweck. Sie hat drei direkte Wirkungen: (1) `cargo check --workspace` wird schneller, weil weniger Crate-Grenzen neu kompiliert werden müssen; (2) neue Jules-Tasks bekommen automatisch größere, aber inhaltlich zusammenhängendere Kontexteinheiten statt künstlich fragmentierter; (3) Architekturverstöße wie die 8-von-16-Abhängigkeitskette von `memfuse-db` werden strukturell unmöglich, weil es diese Crates schlicht nicht mehr einzeln gibt, gegen die verstoßen werden könnte.

---

## §4 Modul-für-Modul-Spezifikation mit Migrationspfad

Jeder Unterabschnitt ist so geschnitten, dass er als **eine** Jules-Task-Sequenz (typischerweise 2–5 einzelne Tasks, siehe §8) umsetzbar ist. Format pro Modul: **Ist-Problem → Ziel → Migrationsschritte → Risiko/Abgrenzung → Akzeptanzkriterium.**

### §4.1 `memfuse-security` (aus `memfuse-crypto` + `memfuse-kv-bridge`)

**Ist-Problem:** Zwei Crates, eine davon (`kv-bridge`, 355 LOC) ist zu klein, um als eigenständiges Kompilierungsziel gerechtfertigt zu sein, und ihre gesamte fachliche Existenzberechtigung (Verschlüsselung von KV-Cache-Segmenten) hängt vollständig von `memfuse-crypto` ab. Der bekannte K16-Bug (FIFO statt LRU im `eviction_worker.rs`) und die K14-Lücke (fehlende Segment-Verschlüsselung, Increment 2) liegen beide in `kv-bridge`, sind aber sicherheitsrelevant genug, um im selben Audit-Scope wie die Kryptografie-Kernlogik zu liegen — aktuell werden sie getrennt geprüft, was das Risiko erhöht, dass eine Sicherheitsannahme in `crypto` geändert wird, ohne dass `kv-bridge` synchron mitgeprüft wird.

**Ziel:** Ein Modul `memfuse-security` mit klaren Submodulen: `security::wal_integrity` (HMAC-Kette), `security::deletion_proof`, `security::kv_segment` (vormals kv-bridge, inkl. Eviction-Policy), `security::key_management`. Ein einziger Audit-Scope, ein einziges Cargo-Package, eine einzige Versionsnummer für „das Sicherheitsversprechen von MemFuse" — was auch für die Außenkommunikation (§1, Position A) hilfreich ist: "memfuse-security ist geprüft gegen X" ist eine stärkere Aussage als "memfuse-crypto UND memfuse-kv-bridge sind getrennt geprüft".

**Migrationsschritte:**
1. Jules-Task: `memfuse-kv-bridge`-Quelldateien nach `memfuse-crypto/src/kv_segment/` verschieben (reine Dateibewegung, keine Logikänderung), Cargo.toml-Dependency-Eintrag entfernen, Re-Exports in `lib.rs` anpassen. Keine Verhaltensänderung — reines Umhängen.
2. Separater Jules-Task: Crate umbenennen `memfuse-crypto` → `memfuse-security` (Cargo.toml, alle Referenzen im Workspace, DAG-Check-Konfiguration).
3. Separater Jules-Task (NICHT mit 1./2. vermischen): den bereits bekannten K16-Fix (echtes LRU) und K14 (Segment-Verschlüsselung Increment 2) als eigene, nach der Umstrukturierung fließende Fix-Tasks — nicht während der Umstrukturierung selbst, um Audit-Diff und Verhaltens-Diff sauber trennbar zu halten (P13/Audit≠Fix-Prinzip aus dem bestehenden Prompter-Tooling, siehe §7).

**Risiko/Abgrenzung:** Reine Dateibewegung hat geringes Risiko, aber `#[cfg(feature=...)]`-Gates müssen sorgfältig migriert werden — `kv-bridge` hatte eigene Feature-Flags, die jetzt zu Sub-Features von `memfuse-security` werden müssen, nicht verschwinden dürfen.

**Akzeptanzkriterium:** `cargo check --workspace` grün, `cargo test -p memfuse-security --all-features` deckt weiterhin alle vorher in `memfuse-crypto` UND `memfuse-kv-bridge` vorhandenen Tests ab (Testanzahl vor/nach vergleichen — Abnahme ist ein Fehlschlag, nicht nur ein Hinweis).

### §4.2 `memfuse-retrieval` (aus `memfuse-index` + `memfuse-graph` + `memfuse-text`)

**Ist-Problem:** Dies ist die umfangreichste Konsolidierung (14.165 + 9.250 + 5.331 LOC nach v7.0-Zählung, real vermutlich höher). Der Grund, warum sie trotzdem sinnvoll ist: Graph-Kanten referenzieren DocIds aus dem Vektorindex (`edges_for_doc()`), BM25-Postings-Listen laufen durch dieselbe `StorageEngine`-Abstraktion, und PathRAG (`path_rag.rs`) kombiniert alle drei Signale bereits algorithmisch. Die Crate-Trennung bildet also keine echte Unabhängigkeit ab — sie verlangsamt nur Refactorings, die alle drei Signale gemeinsam betreffen (wie es beim geplanten F-09-Kohärenzbonus oder einer künftigen Neugewichtung der RRF-Fusion regelmäßig der Fall sein wird).

**Ziel:** Ein Modul `memfuse-retrieval` mit den Submodulen `retrieval::vector` (HNSW, DiskANN, SIMD-Distanzfunktionen), `retrieval::graph` (CSR, PPR, PathRAG, Community-Detection), `retrieval::text` (BM25, Tokenizer, deutsche Morphologie). Diese Submodul-Grenzen bleiben intern strikt (kein „God-Modul" — APM-30 aus dem bestehenden Anti-Pattern-Katalog bleibt anwendbar und wird explizit auf dieses neue, größere Modul angewendet, siehe Anhang B), aber sie teilen sich Kompilierungseinheit und Testlauf.

**Migrationsschritte (bewusst in kleine, unabhängig verifizierbare Schritte zerlegt, da dies das größte Einzelrisiko der gesamten Migration ist):**
1. Neues leeres Crate `memfuse-retrieval` anlegen mit den drei Submodul-Ordnern, noch ohne Inhalt — nur Cargo.toml und leere `mod.rs`-Dateien, DAG-Eintrag vornehmen.
2. Jules-Task A: `memfuse-text`-Inhalt 1:1 nach `retrieval::text` verschieben. Da `memfuse-text` die wenigsten Querabhängigkeiten hat (nur `memfuse-core`), ist dies der risikoärmste Startpunkt.
3. Jules-Task B (nach Abschluss und Grünlicht von A): `memfuse-graph`-Inhalt nach `retrieval::graph` verschieben.
4. Jules-Task C (nach Abschluss und Grünlicht von B, höchstes Risiko wegen SIMD/unsafe-Code): `memfuse-index`-Inhalt nach `retrieval::vector` verschieben. `distance.rs` mit seinen ~81 unsafe-Blöcken (ADR-017) braucht hier besondere Sorgfalt — dieser Task sollte NICHT mit einer Logikänderung kombiniert werden, nur mit der Verschiebung, und im Anschluss einen eigenen `sec`-Task (siehe bestehendes Prompter-Tooling) zur Re-Verifikation aller SAFETY-Kommentare erhalten.
5. Erst nachdem alle drei Submodule verschoben sind: alte Crates `memfuse-index`, `memfuse-graph`, `memfuse-text` aus `Cargo.toml [workspace] members` entfernen, Verzeichnisse löschen.
6. Downstream-Fix: Alle Referenzen in `memfuse-orchestrator` (ehem. `memfuse-db`) von `memfuse_index::`, `memfuse_graph::`, `memfuse_text::` auf `memfuse_retrieval::vector::`, `::graph::`, `::text::` umstellen — dies ist mit Abstand der größte Einzeldiff der gesamten Migration und sollte einen eigenen, ausschließlich dafür vorgesehenen Task erhalten.

**Risiko/Abgrenzung:** Dies ist der Schritt mit der größten Wahrscheinlichkeit für stille Regressionen, weil er den meisten bestehenden Code bewegt. Zwingend: nach jedem Einzelschritt (2–4) vollständiger Testlauf inkl. der bereits vorhandenen Concurrency-Stichproben (Tier-1-Pflicht im bestehenden Prompter-Tooling, siehe §7) — nicht erst am Ende der gesamten Sequenz.

**Akzeptanzkriterium:** Testanzahl vor/nach identisch oder höher pro verschobenem Submodul; `cargo bench -p memfuse-retrieval --bench scale_bench` liefert Werte in vergleichbarer Größenordnung wie die alten `memfuse-index`-Benchmarks (grobe Regressionsschranke, kein exakter Wert nötig, da hier keine Logik geändert wird, nur Modulgrenzen).

### §4.3 `memfuse-orchestrator` — Scheduler-Konsolidierung (P14)

**Ist-Problem:** Fünf Dateien mit überlappender Verantwortung für Hintergrund-Wartungsaufgaben: `decay_controller.rs`, `maintenance_scheduler.rs`, `reaper.rs`, `consolidation_executor.rs`, `memory_consolidation.rs`. Aus der vorherigen Live-Prüfung bekannt: `maintenance_scheduler.rs::run_tick()` orchestriert bereits mehrere dieser Pfade sequenziell, aber `reaper.rs`-Funktionen (`start_thermostat_reaper`, `start_nrem_reaper`) blieben parallel exportierte, unabhängig aufrufbare Funktionen — die Konsolidierung ist technisch begonnen, aber nicht abgeschlossen.

**Ziel:** Ein `SchedulerTask`-Trait mit registrierbaren Implementierungen, ein einziger Eintrittspunkt (`MaintenanceScheduler::run_tick()` als alleiniger Aufrufpfad), keine unabhängig aufrufbaren Reaper-Funktionen mehr im öffentlichen `lib.rs`-Export.

```rust
// Zielform (illustrativ, kein Copy-Paste-Code für Jules — als Struktur-Vorgabe zu verstehen)
pub trait SchedulerTask: Send + Sync {
    fn name(&self) -> &'static str;
    fn interval(&self) -> Duration;
    async fn run(&self, ctx: &SchedulerContext) -> Result<TaskOutcome>;
}
// decay_controller, memory_consolidation, reaper-Aufgaben werden zu SchedulerTask-Impls,
// nicht mehr zu eigenständig aufrufbaren pub fn start_*().
```

**Migrationsschritte:**
1. `SchedulerTask`-Trait in `memfuse-orchestrator` definieren, noch ohne bestehenden Code anzufassen.
2. Für jede der fünf bestehenden Dateien EINEN separaten Jules-Task: bestehende Logik unverändert in eine `SchedulerTask`-Impl einwickeln (Wrapper, keine Verhaltensänderung).
3. Erst wenn alle fünf als `SchedulerTask` registriert sind: die alten `pub fn start_thermostat_reaper()`/`start_nrem_reaper()`-Exporte entfernen und durch Scheduler-Registrierung beim Start ersetzen.
4. Der aus früherer Prüfung bekannte offene Punkt (F-03/`SynapticUpdateBuffer.flush_to_csr()`-Hook, bisher nur als Kommentar-Platzhalter vorhanden) wird NACH dieser Konsolidierung als regulärer neuer `SchedulerTask` implementiert — nicht davor, da sonst erneut ein sechster, nicht-konsolidierter Pfad entsteht.

**Risiko/Abgrenzung:** Diese Konsolidierung deckt mit hoher Wahrscheinlichkeit reale Doppelausführungs- oder Reihenfolge-Bugs auf (das ist der Sinn der Übung, siehe P14-Begründung) — das bedeutet, während dieser Migration entdeckte Verhaltensabweichungen sind erwartete Befunde, keine Fehlschläge des Migrationsplans, sollten aber als eigene `AI-TAG`-Befunde (bestehende Taxonomie) behandelt und NICHT im selben Diff mitgefixt werden (Audit≠Fix-Trennung, siehe §7).

**Akzeptanzkriterium:** Nur noch eine öffentlich aufrufbare Scheduler-Startfunktion in `memfuse-orchestrator::lib.rs`; alle fünf Wartungsaufgaben laufen nachweislich (Log-Ausgabe oder Test) über `run_tick()`.

### §4.4 `memfuse-inference` (aus `memfuse-calibration` + `memfuse-ollama` + `memfuse-candle` + `memfuse-router` + `memfuse-embed`)

**Ist-Problem:** Fünf Crates, von denen zwei (`memfuse-candle`: 718 LOC, technisch vollständig aber nicht in die Pipeline integriert; `memfuse-embed`: 1.882 LOC, bereits optional) zu klein für eigenständige Versionierung sind, und deren gemeinsamer Nenner („wie wird ein Modell angesprochen/kalibriert/geroutet") aktuell nur implizit über gemeinsame Abhängigkeit auf `memfuse-core` sichtbar ist, nicht über eine gemeinsame Modul-Fassade.

**Ziel:** Ein `memfuse-inference`-Modul mit Feature-Flags statt Crate-Grenzen für die austauschbaren Backends: `inference-ollama` (Default), `inference-candle` (optional, GGUF-Pure-Rust), `inference-onnx-embed` (optional). `memfuse-router` (Conformal Routing, Lyapunov-Drift-Wächter) und `memfuse-calibration` (Isotonic/Platt/PID/Replicator) werden Kern-Submodule ohne Feature-Gate, da sie backend-unabhängig sind und immer gebraucht werden.

**Migrationsschritte:**
1. Jules-Task: `memfuse-calibration` + `memfuse-router` zusammenlegen zu `inference::calibration` + `inference::routing` (beide klein, beide unmittelbar zusammengehörig — Kalibrierung speist direkt in Routing-Entscheidungen ein).
2. Separater Jules-Task: `memfuse-ollama` nach `inference::backend::ollama` verschieben, als Default-Feature.
3. Separater Jules-Task: `memfuse-candle` nach `inference::backend::candle` verschieben, hinter `inference-candle`-Feature — dies ist der Zeitpunkt, an dem auch die seit v6.0 offene Pipeline-Integrationslücke (Candle-Factory in `memfuse-mcp`) behoben werden sollte, aber als eigener, nachgelagerter Task, nicht innerhalb der Verschiebung.
4. Separater Jules-Task: `memfuse-embed` nach `inference::backend::onnx_embed` verschieben, hinter `inference-onnx-embed`-Feature (bereits optional, Migration ist rein strukturell).

**Risiko/Abgrenzung:** Geringstes Risiko der gesamten Migration, da die Ausgangscrates bereits klar durch Feature-Flags oder klare Funktionsgrenzen getrennt sind. Hauptaufwand ist mechanisches Verschieben und Anpassen von `use`-Pfaden.

**Akzeptanzkriterium:** `cargo build --no-default-features --features inference-ollama` und `cargo build --all-features` beide grün; keine der vier bisherigen Backend-Fähigkeiten geht verloren (Feature-Matrix-Test in CI).

### §4.5 `memfuse-persistence` (aus `memfuse-store` + `memfuse-checkpoint`)

**Ist-Problem:** Geringstes Konsolidierungsrisiko, aber ähnliches Muster wie §4.1 — `memfuse-checkpoint` (5.421 LOC) hat keinen unabhängigen Konsumenten außerhalb des Persistenz-Kontexts.

**Migrationsschritte:** Analog zu §4.1 — reine Verzeichnisverschiebung (`memfuse-checkpoint/src/*` → `memfuse-store/src/checkpoint/`), Crate-Umbenennung `memfuse-store` → `memfuse-persistence`, DAG-Eintrag aktualisieren.

**Akzeptanzkriterium:** Testanzahl vor/nach identisch; `RAII CheckpointGuard`-Semantik (P10-Fassade) bleibt einzige Backup-Schnittstelle, jetzt als `persistence::checkpoint::CheckpointGuard`.

### §4.6 Was unverändert bleibt

`memfuse-foundation` (ehem. `core`), `memfuse-agentic` (ehem. `agent`), `memfuse-mcp`, `xtask`, `memfuse-bench` erhalten in dieser Spezifikation **keine** strukturelle Änderung — sie erfüllen bereits P13 (echte fachliche oder kompilatorische Eigenständigkeit) und sollten nicht angefasst werden, nur weil gerade refaktoriert wird. Eine Migration, die mehr bewegt als nötig, erzeugt mehr Regressionsrisiko als sie Nutzen bringt (vgl. APM-40 „Premature Abstraction" aus dem bestehenden Anti-Pattern-Katalog — dieselbe Vorsicht gilt umgekehrt auch für „Premature Konsolidierung").

---

## §5 Feature-Konsolidierung: vom „Physio"-Katalog zum Produkt-Feature-Set

**Ist-Problem:** Der F-01 bis F-11 „Physio"-Katalog (Thermostat, Immunsystem, REM/NREM-Schlafphasen, Homöostat, synaptische Verstärkung, Lyapunov-Drift-Wächter) folgt biologischen Analogien als Namensgebung UND als Konzeptquelle. Die bereits durchgeführte Terminologie-Migration (siehe frühere Prüfung: `thermostat.rs` → `decay_controller.rs`, `immune.rs` → `consistency_enforcement.rs`, etc.) hat die **Namen** korrigiert, aber die Frage nicht beantwortet, ob jedes einzelne Feature einen **nachweisbaren Produktnutzen** hat oder ob manche primär deshalb existieren, weil die biologische Analogie eine elegante Implementierungsidee nahelegte.

**Ziel:** Jedes der elf Features durchläuft vor der nächsten Implementierungsrunde eine einfache Klassifikation:

| Klasse | Kriterium | Umgang |
|---|---|---|
| **Kern** | Trägt direkt zu Position A oder B aus §1 bei UND ist messbar (Benchmark-Zahl vorhanden oder geplant) | Weiterentwickeln, Priorität |
| **Differenzierend** | Trägt zu §1 bei, aber noch nicht messbar | Erst nach LongMemEval/LoCoMo-Integration (bereits separat beauftragt) priorisieren |
| **Experimentell** | Konzeptionell interessant, kein klarer Produktbezug | Hinter Feature-Flag, kein Weiterausbau ohne explizite Freigabe |
| **Kandidat für Entfernung** | Weder Produktbezug noch Messbarkeit absehbar | In eigenem ADR zur Entfernung vorschlagen |

**Grobe Einordnung basierend auf den bisherigen Prüfungen** (finale Klassifikation ist eine Produktentscheidung, siehe Anhang D, keine Jules-Aufgabe):

- **Kern:** F-11 (Lyapunov-Drift-Wächter — schützt Kalibrierungsstabilität, direkt messbar über ECE), F-08 (PID-Homöostat — Latenzbudget-Einhaltung, direkt mit P11 verknüpft), F-04 (Konsistenz-/Widerspruchsabwehr — trägt zu „belegbare Korrektheit" bei, einer der wenigen Bereiche mit echtem INV-Nachweis).
- **Differenzierend:** F-09 (Kohärenz-Bonus — aktuell durch fehlendes Feature-Flag unaktivierbar, siehe K11; erst nach Aktivierung UND Kalibrierung gegen echten Benchmark bewertbar), F-06 (Perkolations-Gesundheit).
- **Experimentell:** F-01 (Freie-Energie-Decay-Controller), F-05 (REM/Synthese-Phase), F-07 (Replikatordynamik-Fusionsgewichte) — konzeptionell jeweils nicht falsch, aber ohne klaren Bezug dazu, welches Nutzerproblem sie lösen, das MinnsDB nicht auch löst.
- **Prüfkandidat für Entfernung:** F-03 (synaptische Verstärkung — Berechnungslogik seit mehreren Dokumentversionen fertig, Integration wiederholt verschoben; wenn nach der Scheduler-Konsolidierung aus §4.3 innerhalb einer definierten Frist keine Integration erfolgt, ist die naheliegende Interpretation, dass das Feature keine hohe Priorität hat, nicht dass die Integration zufällig immer verdrängt wurde).

**Wichtig:** F-02 (Partieller HNSW-Rebuild) und F-10 (Cross-Tenant-Wissensaustausch) sind bereits über `VETOES.md` geregelt (`conditionally_accepted` mit Frist bzw. `permanent_rejected`) — diese Klassifikation hier ergänzt das Veto-System, ersetzt es nicht (siehe Anhang C).

---

## §6 Die drei Visionen — Entscheidung statt Koexistenz

**Ist-Problem, mit Beleg:** ADR-018 im Repo dokumentiert wörtlich, dass zwei Produktstrategien (PyPI-Library vs. Desktop-Enterprise-App) gleichzeitig als „final" galten, ohne dass eine ADR die andere formal ablöste. Eine dritte Vision (Voice/Jarvis-Assistent) taucht in separaten Planungsdokumenten auf, ohne im Kerncode verdrahtet zu sein. Sichtbarer Fingerabdruck im Code: `memfuse-tauri` (Desktop-GUI), `memfuse-kv-bridge`/Enterprise-Multi-Tenant-Konzepte, `memfuse-py` (Library-Bindings) liegen alle gleichrangig im selben Workspace.

**Diese Spezifikation trifft die Entscheidung nicht — sie macht die Konsequenzen jeder Option explizit, damit die Entscheidung schnell und informiert getroffen werden kann (Anhang D):**

**Option 1 — PyPI-Library (ADR-007-Richtung).** Konsequenz für die Zielarchitektur: `memfuse-bindings::tauri` wird nach der Migration vollständig entfernt (nicht nur ungenutzt gelassen). `memfuse-py` wird zum primären, best-gepflegten Grenzschicht-Crate. Enterprise-Multi-Tenant-Konzepte in `memfuse-security::kv_segment` werden auf das Nötigste für Einzelanwender-Isolation reduziert (TenantId bleibt als internes Konzept für Test-Isolation, aber Multi-Tenant-Marketing entfällt). Das passt am direktesten zu Position A aus §1 (schlanker als MinnsDB).

**Option 2 — Desktop-Enterprise-App (ADR-018-Richtung).** Konsequenz: `memfuse-bindings::tauri` wird zum Hauptprodukt ausgebaut, `memfuse-py` wird zur Nebenschnittstelle für Power-User/Automatisierung zurückgestuft (bleibt bestehen, aber ohne Priorität). Multi-Tenant-Funktionen in `memfuse-security` werden vollständig ausgebaut (Increment 2 aus K14 wird Pflicht statt optional). Das passt zu Position B aus §1 (strukturell überlegen für große, auditpflichtige Deployments).

**Option 3 — Voice/Jarvis-Assistent.** Aus den bisherigen Prüfungen: Diese Vision ist im Kerncode am wenigsten verdrahtet (nur in Planungsdokumenten). Diese Spezifikation empfiehlt **explizit, diese Option vorerst zu verwerfen** (formale VETO-Eintragung analog zu F-10, nicht nur stillschweigend fallen lassen) — nicht weil sie schlecht ist, sondern weil eine dritte gleichzeitige Vision bei bereits zwei ungeklärten die Fokussierungsproblematik nur verschärft. Ein VETO mit `conditional_review_due` (analog zum bestehenden VETO-F02-Muster) ist besser als stilles Liegenlassen, weil es einen expliziten Wiedervorlage-Zeitpunkt statt unbegrenzter Ambiguität schafft.

**Verbindlich für beide verbleibenden Optionen:** Sobald die Entscheidung getroffen ist, wird die **nicht gewählte** Option nicht nur in einer neuen ADR als „superseded" markiert (P15), sondern ihr Code wird innerhalb einer definierten Frist (Vorschlag: 60 Tage nach Entscheidung) tatsächlich aus dem Repository entfernt. Ein „vielleicht später doch"-Argument für das Liegenlassen ist nachvollziehbar, aber genau dieses Muster hat zur aktuellen Situation geführt — totes, aber kompilierendes Enterprise/Voice/Library-Konzeptcode nebeneinander ist kein neutraler Zustand, er kostet laufend Kontext in jeder Jules-Session, die versehentlich in seiner Nähe arbeitet.

---

## §7 Ziel-Entwicklungssetup: Prompter-Tooling, Rollen-Trennung, Anti-Drift-Mechanismen

### §7.1 Würdigung des Bestehenden

Die beiden vorliegenden Prompter-Werkzeuge (`memfuse-prompter-v2.html`, `Memfuse-Prompter-v22_1_.html`) sind kein Nebenprodukt, sondern die eigentliche Steuerungszentrale des gesamten Entwicklungsprozesses, und sie sind deutlich durchdachter als der reine Code-Zustand vermuten lässt:

- **Rollen-Trennung Auditor/Fixer** (`ROLE_LOCK_BLOCK` in v22): Eine Sitzung ist entweder ausschließlich Befund-Erhebung (mit verbindlichem `AI-TAG`-Kommentarsystem als einzigem Kommunikationskanal) oder ausschließlich Fix-Konsum bereits erhobener Befunde — nie beides. Das ist strukturell genau die Absicherung, die verhindert, dass ein Agent einen Bug während der Analyse „nebenbei" behebt und damit den Befund für den nächsten Reviewer verschleiert.
- **41-Punkte-Anti-Pattern-Katalog (APM-1 bis APM-41)** mit konkreten Grep-Signaturen pro Muster — von Filesystem-Atomarität über Lock-Hierarchie-Inversion bis zu FFI-Typpräzisionsverlust. Das ist eine hausgemachte statische Analyse, die inhaltlich zu großen Teilen genau die Fehlerklassen abdeckt, die in den bisherigen Live-Prüfungen als K11–K20 auffielen.
- **Tier-Gewichtung (1–3) und Domain-Profile** (`mvcc-heavy`, `crypto-core`, `ffi-boundary`, etc.), die Audit-Tiefe risikoproportional statt gleichverteilt zuweisen.
- **14 Task-Typen** mit unterschiedlichem Zyklus (audit, deep, impl, fix, test, sec, calib, review, chaos, replay, flaky, adr, deps, korrektur) — eine granularere Prozesstaxonomie, als in den meisten menschengeführten Teams existiert.

**Das eigentliche Problem ist nicht die Prozessqualität, sondern die Kopplung an eine sich ständig ändernde Crate-Liste.** Der Diff zwischen v2 (16 Crates, andere LOC-Zahlen, andere Tier-Zuordnung) und v22 (15 Crates, „KOMPONENTEN-INVENTAR GEGEN REPO VERIFIZIERT", 41 statt weniger APM-Einträge) zeigt: Jede Architekturänderung erzwingt eine manuelle Nachpflege einer 2.800-Zeilen-JavaScript-Datei. Das ist der gleiche strukturelle Fehler wie bei den v6.0/v7.0-Spezifikationsdokumenten (P16) — Ist-Zustand und Steuerungslogik sind im selben Artefakt vermischt, und das Artefakt selbst wird durch Handarbeit aktuell gehalten.

### §7.2 Zielsetup: Prompter-Konfiguration aus dem Repo generiert, nicht von Hand gepflegt

**Prinzip:** Die `CRATES`-Liste, LOC-Zahlen, Test-Counts und Tier-Zuordnungen im Prompter-Tool werden nicht mehr manuell in der HTML/JS-Datei editiert, sondern aus einer maschinenlesbaren Quelle generiert, die bei jedem `cargo xtask sync-docs`-Lauf (bereits vorhandener Mechanismus, siehe `WORKING_STATE.md`-Autogenerierung) mit aktualisiert wird.

**Konkreter Vorschlag:**
1. Ein neues, kleines `xtask`-Kommando `cargo xtask export-crate-manifest` erzeugt eine `crate_manifest.json` im Repo-Root mit: Crate-Name, Layer (aus DAG-Check-Konfiguration ableitbar), LOC (aus `wc -l` automatisiert), Test-Anzahl (aus `#[test]`-Grep automatisiert), zuletzt-geändert-Datum. Dieser Mechanismus existiert in Grundzügen bereits (der Kommentar in v22 „LOC/Tests zuletzt verifiziert: 2026-09-03" zeigt, dass die Zahlen ohnehin schon per Skript erhoben wurden, nur nicht automatisch in die HTML-Datei zurückgeschrieben).
2. Das Prompter-HTML lädt `crate_manifest.json` beim Öffnen (z. B. per `fetch()` bei lokalem Server oder als eingebettetes Build-Artefakt), statt die `CRATES`-Konstante hart zu kodieren. Tier-Zuordnung und Domain-Profile (die eine echte redaktionelle/architektonische Entscheidung sind, keine automatisch ableitbare Tatsache) bleiben weiterhin von Hand gepflegt, aber in einer eigenen, kleinen, git-versionierten Konfigurationsdatei (`prompter_tiers.json`), nicht im 2.800-Zeilen-Skript vermischt.
3. Nach der in §3/§4 beschriebenen Konsolidierung auf 9–10 Workspace-Members aktualisiert sich die Prompter-Crate-Liste damit **automatisch** beim nächsten `sync-docs`-Lauf — es ist keine manuelle v23-Nacharbeit am Prompter-Tool nötig, die bei der nächsten Architekturänderung wieder von vorn beginnt.

**Was am Prompter-Tool NICHT geändert werden soll:** Die Rollen-Trennung (Auditor/Fixer), der APM-Katalog und die Task-Typ-Taxonomie sind prozessuale, keine strukturellen Bestandteile — sie bleiben unverändert gültig, unabhängig davon, ob es 18 oder 9 Crates gibt. Anhang B klärt im Detail, welche der 41 APM-Einträge nach der Konsolidierung auf welches neue Modul umgehängt werden (die meisten bleiben 1:1 gültig, wenige wie APM-30 „God-Object-Drift" gewinnen nach der Zusammenlegung sogar an Relevanz, siehe §4.2).

### §7.3 Der 195-Tasks/Tag-Engpass: von Kapazität zu Sequenzierung

Mit ~195 Jules-Tasks/Tag ist die reine Ausführungskapazität kein limitierender Faktor mehr — die Frage ist, wie diese Kapazität sequenziert wird, damit sie nicht wie im bisherigen Verlauf (1319 Commits, davon die meisten in einer sehr kurzen Verdichtungsphase) zu unkoordiniertem parallelem Fortschritt auf unterschiedlichen, sich gegenseitig störenden Baustellen führt.

**Konkrete Empfehlung für die Task-Verteilung während der in §8 beschriebenen Migrationsphasen:**

- **Kein Migrationsschritt aus §4 läuft parallel zu einem anderen Migrationsschritt aus §4**, wenn beide dasselbe Zielmodul betreffen (z. B. dürfen nicht gleichzeitig `memfuse-text`- und `memfuse-graph`-Verschiebung nach `memfuse-retrieval` laufen — siehe die explizite Sequenzierung „nach Abschluss und Grünlicht von A" in §4.2). Sehr wohl parallelisierbar: unterschiedliche Zielmodule (§4.1-Sicherheit parallel zu §4.4-Inferenz, da beide keine gemeinsamen Dateien anfassen).
- Die verfügbare Kapazität, die NICHT für die Migration selbst gebunden ist, sollte in dieser Phase bevorzugt auf **Tier-1-Audit-Tasks** (bestehende Prompter-Taxonomie) der noch unveränderten Module gehen — das hält die Qualitätssicherung der nicht-migrierten Teile am Laufen, während die Migration selbst läuft, statt beides um dieselbe Kapazität konkurrieren zu lassen.
- **Explizit vermeiden:** neue Feature-Implementierungs-Tasks (Task-Typ `impl` in der bestehenden Taxonomie) auf Modulen, die aktuell migriert werden. Ein neues Feature in `memfuse-graph`, während `memfuse-graph` gerade nach `memfuse-retrieval` verschoben wird, verdoppelt den Merge-Konflikt-Aufwand ohne Not.

---

## §8 Migrationsreihenfolge (Phasenplan, Jules-Task-Zuschnitt)

Die Reihenfolge ist nach Risiko aufsteigend sortiert — niedrigstes Risiko zuerst, damit sich das Vorgehen (Verschieben → Testen → Umbenennen → DAG-Eintrag aktualisieren) an unkritischen Modulen bewährt, bevor es auf `memfuse-retrieval` (höchstes Risiko) angewendet wird.

**Phase 0 — Voraussetzung, parallel zu allem anderen, unabhängig von dieser Spezifikation:**
LongMemEval-/LoCoMo-CI-Integration (bereits als eigener Auftrag formuliert). Ohne diese Zahl kann am Ende der Migration nicht behauptet werden, dass das Ergebnis „besser" ist — nur „anders strukturiert". Diese Phase blockiert keine der folgenden, sollte aber vor Phase 4 (Vision-Entscheidung, §6) abgeschlossen sein, da die Vision-Entscheidung auch von Benchmark-Ergebnissen mitgetragen werden sollte.

**Phase 1 — Geringstes Risiko (§4.1, §4.5):**
1. `memfuse-security`-Konsolidierung (§4.1): kv-bridge in crypto verschieben, umbenennen.
2. `memfuse-persistence`-Konsolidierung (§4.5): checkpoint in store verschieben, umbenennen.
Diese beiden können parallel laufen (unterschiedliche Zielmodule, keine Dateiüberschneidung).

**Phase 2 — Geringes Risiko (§4.4):**
3. `memfuse-inference`-Konsolidierung: calibration+router zuerst, dann ollama, dann candle, dann embed — vier sequenzielle, aber jeweils kleine Tasks, wie in §4.4 beschrieben.

**Phase 3 — Mittleres Risiko (§4.3):**
4. Scheduler-Konsolidierung in `memfuse-orchestrator` (P14). Kann parallel zu Phase 2 laufen (unterschiedliche Module), sollte aber NICHT parallel zu Phase 4 laufen, da `memfuse-orchestrator` nach der Retrieval-Migration seine `use`-Pfade ohnehin anpassen muss — beide Änderungen im selben Modul gleichzeitig erhöhen unnötig das Konflikt-Risiko.

**Phase 4 — Höchstes Risiko (§4.2):**
5. `memfuse-retrieval`-Konsolidierung: text → graph → index, strikt sequenziell wie in §4.2 beschrieben, mit Testlauf nach jedem Einzelschritt.
6. Downstream-Fix in `memfuse-orchestrator` (Referenzen von alten Crate-Pfaden auf `memfuse_retrieval::*` umstellen).

**Phase 5 — Abschluss:**
7. Alte, jetzt leere Crate-Verzeichnisse aus `Cargo.toml` entfernen, `cargo xtask check-dag` gegen die neue 9–10-Crate-Topologie grün ziehen.
8. Prompter-Tooling-Anpassung gemäß §7.2 (Crate-Manifest-Export einführen) — dies sollte NACH Abschluss der Code-Migration erfolgen, damit das generierte Manifest den neuen Zielzustand widerspiegelt, nicht einen Zwischenstand.

**Phase 6 — Getrennt von der Struktur-Migration, kann jederzeit parallel starten:**
9. Vision-Entscheidung (§6) — dies ist eine Produktentscheidung, kein Jules-Task, und blockiert die Struktur-Migration nicht, da die in §3–§4 vorgeschlagenen Modulgrenzen für beide verbleibenden Visionsoptionen gültig bleiben. Nur die Entscheidung, welche Grenzschicht-Crates (§3.2, „Grenzschicht") langfristig bestehen bleiben, hängt von dieser Wahl ab.

---

## §9 Erfolgsmessung: welche Zahl entscheidet, ob das funktioniert hat

Diese Migration ist architektonisch gut begründbar (P13–P16, Einzelbegründungen in §4), aber „architektonisch sauberer" ist nicht dasselbe wie „besseres Produkt". Drei Prüfgrößen, in Prioritätsreihenfolge:

1. **LongMemEval-/LoCoMo-Recall-Werte vor/nach der Migration dürfen sich nicht verschlechtern** (Toleranzband wie im bestehenden CI-Gate: 0.05). Da die Migration reine Strukturverschiebung ohne Logikänderung ist (§4, durchgängiges Prinzip), ist eine Verschlechterung ein Alarmsignal für einen währenddessen eingeschlichenen Bug, keine erwartbare Nebenwirkung.
2. **`cargo check --workspace`-Zeit vor/nach.** Sollte spürbar sinken (weniger Crate-Grenzen = weniger inkrementelle Neukompilierung bei Änderungen, die mehrere ehemals getrennte Crates betreffen). Falls sie NICHT sinkt, ist das ein Hinweis, dass die neuen Modulgrenzen ähnlich fein wie vorher gezogen wurden und die Konsolidierung ihr Ziel verfehlt hat.
3. **Anzahl der Workspace-Members** (Ziel: 9–10 statt 18, siehe §3.3) — die einfachste, aber am wenigsten aussagekräftige Zahl allein; sie zählt nur, wenn 1. und 2. nicht schlechter werden.

**Die eine Zahl, die über die Grundsatzfrage „lohnt sich das Projekt gegenüber MinnsDB" entscheidet, liegt außerhalb dieser Spezifikation:** ein direkter, reproduzierbarer LongMemEval/LoCoMo-Vergleich MemFuse vs. MinnsDB, auf identischer Hardware, mit identischem Datensatz. Diese Spezifikation schafft die architektonische Voraussetzung dafür (fokussierteres, auditierbareres Produkt), ersetzt diesen Vergleich aber nicht.

---

## Anhang A: Crate-Mapping-Tabelle (alt → neu)

| Alt (18 Members) | Neu (9–10 Members) | Abschnitt |
|---|---|---|
| memfuse-core | memfuse-foundation | unverändert, nur umbenannt |
| memfuse-crypto | memfuse-security | §4.1 |
| memfuse-kv-bridge | memfuse-security (Submodul `kv_segment`) | §4.1 |
| memfuse-store | memfuse-persistence | §4.5 |
| memfuse-checkpoint | memfuse-persistence (Submodul `checkpoint`) | §4.5 |
| memfuse-index | memfuse-retrieval (Submodul `vector`) | §4.2 |
| memfuse-graph | memfuse-retrieval (Submodul `graph`) | §4.2 |
| memfuse-text | memfuse-retrieval (Submodul `text`) | §4.2 |
| memfuse-db | memfuse-orchestrator | §4.3 |
| memfuse-calibration | memfuse-inference (Submodul `calibration`) | §4.4 |
| memfuse-router | memfuse-inference (Submodul `routing`) | §4.4 |
| memfuse-ollama | memfuse-inference (Submodul `backend::ollama`) | §4.4 |
| memfuse-candle | memfuse-inference (Submodul `backend::candle`) | §4.4 |
| memfuse-embed | memfuse-inference (Submodul `backend::onnx_embed`) | §4.4 |
| memfuse-agent | memfuse-agentic | unverändert, nur umbenannt |
| memfuse-mcp | memfuse-mcp | unverändert |
| memfuse-py | memfuse-bindings::py ODER eigenständig | §6-abhängig |
| memfuse-tauri | memfuse-bindings::tauri ODER entfernt | §6-abhängig |
| memfuse-bench | memfuse-bench | unverändert |
| xtask | xtask | unverändert |

---

## Anhang B: APM-Katalog-Zuordnung nach Konsolidierung

Die meisten der 41 bestehenden Anti-Pattern-Muster (siehe v22-Prompter-Tool) bleiben nach der Modul-Konsolidierung unverändert relevant und wechseln lediglich ihren Anwendungsbereich vom alten Crate-Namen auf das neue Modul (z. B. APM-13 „Nonce-/Key-Domain-Separation", bisher primär `memfuse-crypto` zugeordnet, gilt unverändert für `memfuse-security`). Hervorzuhebende Fälle, in denen die Konsolidierung selbst neue Relevanz schafft:

- **APM-30 (God-Object-Drift):** Gewinnt an Bedeutung für `memfuse-retrieval` und `memfuse-inference` — diese Module sind nach der Zusammenlegung die größten im Workspace und damit am anfälligsten dafür, dass zentrale Structs im Lauf der Zeit fachfremde Felder akkumulieren. Empfehlung: nach Phase 4 einen dedizierten `deep`-Task (bestehende Taxonomie) mit Fokus genau auf APM-30 gegen `memfuse-retrieval` ansetzen.
- **APM-6 (Geschwister-Konsistenz):** Gewinnt an Bedeutung für die in §4.3 konsolidierten Scheduler-Tasks — nachdem fünf Implementierungen zu `SchedulerTask`-Impls vereinheitlicht wurden, ist ein Konsistenzabgleich zwischen den Impls (folgen alle demselben Fehlerbehandlungs-Muster? demselben Logging-Format?) ein naheliegender Folge-Audit.
- **Neuer Vorschlag für einen APM-42-artigen Eintrag** (das bestehende Tooling sieht bereits einen Mechanismus vor, neue APM-Muster aus Fix-Sessions vorzuschlagen, siehe `ROLE_LOCK_BLOCK`-Fixer-Text „Wurde dieser Fix als neues Muster erkennbar?"): **„Migrations-Diff-Vermischung"** — eine reine Verschiebe-/Umbenennungs-Aufgabe (wie die meisten in §4) enthält eine unabsichtliche Logikänderung. Grep-Signatur: Diff-Review, der reine Zeilenverschiebung erwartet, aber tatsächliche Zeilenänderungen (nicht nur Pfad-/Modulnamen) enthält.

---

## Anhang C: Was aus VETOES.md unverändert gilt

VETO-F02 (partieller HNSW-Rebuild, `conditionally_accepted` mit Prüffrist) und VETO-F10 (Cross-Tenant-Wissensaustausch, `permanent_rejected`) bleiben von dieser Spezifikation unberührt und vollständig in Kraft — beide Features werden in §4.2 lediglich in ein neues Modul (`memfuse-retrieval::vector` bzw. würde F-10, falls es je implementiert würde, `memfuse-security` betreffen) verschoben, ihr Veto-Status wandert unverändert mit. §6 schlägt einen dritten, neuen Veto-Eintrag für die Voice/Jarvis-Vision vor — dieser wäre eine Ergänzung, keine Änderung der bestehenden zwei Einträge.

---

## Anhang D: Offene Entscheidungen, die EIN Mensch treffen muss (nicht Jules)

Diese Spezifikation ist absichtlich so geschrieben, dass sie ohne diese Entscheidungen bereits zu >80% umsetzbar ist (die gesamte Struktur-Konsolidierung in §3/§4/§8 ist von der Vision-Frage unabhängig). Folgende Punkte kann und sollte kein Jules-Task autonom entscheiden:

1. **Welche der beiden verbleibenden Visionen aus §6** (PyPI-Library vs. Desktop-Enterprise-App) verfolgt wird — das ist eine Marktentscheidung, keine technische.
2. **Die finale Klassifikation der elf Physio-Features** aus §5 (Kern/Differenzierend/Experimentell/Entfernungskandidat) — die hier vorgeschlagene grobe Einordnung ist ein Ausgangspunkt, keine verbindliche Setzung, da sie von der Vision-Entscheidung mit abhängt (z. B. wäre F-07/Replikatordynamik für Position B relevanter als für Position A).
3. **Die 60-Tage-Frist für die Entfernung der nicht gewählten Vision** (§6) — der konkrete Zeitraum ist ein Vorschlag, keine Ableitung.
4. **Ob `memfuse-py` und `memfuse-tauri` nach der Vision-Entscheidung als ein gemeinsames `memfuse-bindings`-Crate mit Feature-Flags oder als zwei getrennte Leaf-Crates weitergeführt werden** — hängt direkt an Entscheidung 1.

---

## Anhang E: Warum dieses Dokument keine Live-Code-Zeilen zitiert

Frühere Dokumentversionen (v6.0, v7.0) verifizierten jede Aussage gegen einen konkreten HEAD-Commit-Hash. Das erzeugte Genauigkeit zum Prüfzeitpunkt, aber — wie in mehreren vorherigen Sitzungen direkt beobachtet — eine Halbwertszeit im Bereich weniger Stunden bei ~1.300 Commits über den bisherigen Projektverlauf (Spitzenwerte von über 30 Commits binnen 5 Stunden). Ein Architektur-Zieldokument, das diesen Fehler wiederholen würde, wäre spätestens bei der ersten Jules-Session, die es konsumiert, bereits gegenüber dem dann aktuellen Stand ungenau — nicht weil die Architektur falsch wäre, sondern weil einzelne zitierte Zeilennummern oder Dateinamen sich verschoben haben könnten. Diese Spezifikation zitiert deshalb ausschließlich **Strukturentscheidungen und Modulverantwortungen**, die per Definition stabiler sind als einzelne Codezeilen, und überlässt die Verifikation „ist das noch so im Code" bewusst dem jeweils ausführenden Jules-Task selbst (Schritt 1 jeder Migrationsaufgabe in §4 sollte immer ein kurzer Ist-Abgleich sein, bevor die Verschiebung beginnt).

