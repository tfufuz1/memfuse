# MemFuse — Produkt- und Projektspezifikation

**Stand der Analyse:** 2026-08-29, verifiziert gegen Commit `73617c4` (main)
**Methodik:** Diese Spezifikation wurde ausschließlich aus dem tatsächlichen Quellcode, den
Governance-Dokumenten (`CONSTITUTION.md`, `docs/SOURCE_OF_TRUTH.md`, `DECISIONS.md`,
`WORKING_STATE.md`, `docs/TYPE_REGISTRY.md`) und den Inline-Tags im Code abgeleitet — nicht aus
Marketingtexten oder Annahmen. Wo Roadmap-Dokumente und Code-Realität auseinanderlaufen, wird das
explizit als Befund markiert (siehe Abschnitt 7).

---

## 1. Was MemFuse ist — Eine-Satz-Definition

MemFuse ist eine **eingebettete (in-process), air-gapped 4-Signal-Hybrid-Retrieval-Engine in
reinem Rust**, die Vektorsuche, BM25-Volltextsuche, Entity-Relation-Graph-Traversal und
Metadaten-Filterung in einer einzigen transaktionalen Abfrage fusioniert (Reciprocal Rank
Fusion) — nutzbar als Rust-Library, Python-Bindings, MCP-Server oder Tauri-Desktop-App
("MemFuse Brain").

**Was es nicht ist** (Stand heute, verifiziert): kein verteiltes System, kein Cloud-Service, kein
Multi-Tenant-System, keine Ontologie-Engine, kein RBAC-System, keine "Palantir-Alternative" —
diese Dinge sind ausschließlich als Phase-4-Roadmap-Punkte dokumentiert, nicht als Code
vorhanden.

---

## 2. Architektur — Ist-Zustand (verifiziert)

### 2.1 Crate-DAG (14 Workspace-Crates, 4 Layer)

```
Layer 0:  memfuse-core        (6.725 LOC)  — Typen, Traits, Fehler-Enum, keine Abhängigkeiten
Layer 1:  memfuse-store       (9.720 LOC)  — LSM-Tree, WAL V3 (HMAC-gebunden), SSTables
          memfuse-index       (7.139 LOC)  — HNSW, SIMD-Distanzen, SQ8-Quantisierung
          memfuse-text        (3.531 LOC)  — BM25, Inverted Index, deutsche Morphologie
          memfuse-crypto      (1.142 LOC)  — AES-256-GCM, HMAC-Chaining
          memfuse-graph       (3.822 LOC)  — CSR-Graph, PPR, Community Detection, Session DAG
          memfuse-checkpoint  (1.051 LOC)  — Async Checkpointing, RAII CheckpointGuard
Layer 2:  memfuse-db          (10.256 LOC) — Collections, 4-Signal-Fusion (RRF), Multi-Step Engine
Layer 3:  memfuse-agent       (1.648 LOC)  — Persistenter Agent-Workflow (checkpoint/execute/audit)
          memfuse-embed       (986 LOC, optional/feature-gated) — ONNX Embeddings, Cross-Encoder
          memfuse-ollama      (2.287 LOC)  — Ollama-Client, ContextPrefixEngine
          memfuse-py          (915 LOC)    — PyO3 Python-FFI-Bindings
Layer 4:  memfuse-mcp         (1.758 LOC)  — MCP stdio JSON-RPC 2.0 Server, McpSandbox
          memfuse-tauri       (2.441 LOC)  — Desktop-App-Shell ("MemFuse Brain")
```

**Kritische Beobachtung zur Gewichtung**: `memfuse-store` (9.720 LOC) und `memfuse-db` (10.256
LOC) sind die mit Abstand größten Crates. `memfuse-agent` (1.648 LOC) — der Crate, der die
"Cognitive OS"-Vision tragen müsste — ist um den Faktor 6 kleiner als die reine Storage-Schicht.
Das bestätigt: **MemFuse ist heute primär eine ausgereifte Retrieval-Datenbank, kein
Reasoning-/Agent-System.** Das ist keine Schwäche — Storage-First ist der richtige Aufbau —, aber
es muss bei jeder Produktkommunikation berücksichtigt werden.

### 2.2 Kern-Datenfluss (4-Signal-Fusion)

Eine Anfrage an `hybrid_search()` durchläuft:
1. **Vektorsignal**: HNSW-Suche (SIMD-beschleunigte Kosinus-/Euklidische/Dot-Product-Distanz,
   optional SQ8-quantisiert) gegen `memfuse-index`.
2. **Lexikalisches Signal**: BM25 gegen den Inverted Index in `memfuse-text` (inkl. deutscher
   Morphologie-Normalisierung).
3. **Graph-Signal**: CSR-Graph-Traversal in `memfuse-graph` — Entity-Relation-Nachbarschaft,
   optional Personalized PageRank (`PersonalizedPageRank`-Strategie) für Multi-Hop-Retrieval.
4. **Metadaten-Filter**: Prädikat-basierte Filterung (`MetadataFilter`: Eq, Ne, In, Range,
   Contains, And, Or) als Vorfilter oder Nachfilter, je nach Collection-Größe (Post-Filtering für
   große Collections dokumentiert in `collection.rs`).

Alle vier Signale werden per **Reciprocal Rank Fusion (RRF)** zu einem Ranking vereint. Danach
optional: **Cross-Encoder-Reranking** (ONNX, `bge-reranker-base`, feature-gated über
`memfuse-embed`) und **Importance-/Decay-Filterung** (`effective_score()`, siehe ADR ohne
Nummer vor ADR-028 — Filterung, KEIN Re-Ranking, um RRF-Skaleninvarianz zu erhalten).

### 2.3 Transaktionalität & Konsistenz

- **MVCC-Snapshot-Isolation** über alle vier Indextypen (Storage, HNSW, Text, Graph).
- **Vollständiges 4-Index-2-Phasen-Commit** (2PC) — laut Commit `26a1b79` ("Full 4-index 2PC
  transaction commit and rollback") inzwischen über ALLE vier Indizes implementiert. Das war der
  kritischste Befund (C-1) aus dem allerersten Audit dieses Projekts — **inzwischen behoben**,
  nicht mehr nur teilweise (Storage+HNSW) wie ursprünglich.
- **WAL V3**: HMAC-Kette bindet jetzt `tx_id` selbst kryptografisch ein (ADR-029/WAL), verhindert
  `tx_id`-Manipulation bei Dateisystemzugriff durch Angreifer, mit automatischer, abwärtskompatibler
  Migration von V1/V2-WAL-Dateien.
- **RAII `CheckpointGuard`**: automatischer WAL-Rollback bei Drop, kürzlich gehärtet
  ("Harden CheckpointGuard RAII safety and manifest atomicity", Commit `bf23145`).

### 2.4 Sicherheit & Isolation

- **Encryption at Rest**: AES-256-GCM-SIV, HMAC-Chaining (`memfuse-crypto`).
- **MCP-Sandbox**: `McpSandbox` per Default aktiv. `SandboxPolicy` erlaubt DB-Reads standardmäßig,
  DB-Writes und Code-Execution sind Opt-in. Tool-Outputs werden AES-256-GCM-SIV-verschlüsselt und
  bei Drop gezeroized.
- **SIMD-Sicherheit**: Precondition-Assertions für SIMD-Distanzfunktionen wurden gehärtet (Commit
  `3b5f645`, "enforce SIMD preconditions") — einer der 4 zuvor offenen CRITICAL-Tags aus
  `WORKING_STATE.md` ist damit potenziell geschlossen (siehe Abschnitt 7 zur Verifikationspflicht).

---

## 3. Governance-System — der eigentliche USP

Was MemFuse strukturell von praktisch jedem vergleichbaren Solo-Entwickler-Projekt unterscheidet,
ist nicht (nur) der Code, sondern das **Entwicklungssystem selbst**:

| Dokument | Funktion |
|---|---|
| `CONSTITUTION.md` | Verbindliche Kernprinzipien: Sovereign Core, Zero-Panic, Triple-Test-Gate, CI-verifizierte Statusindikatoren |
| `docs/SOURCE_OF_TRUTH.md` | Living-State-Dokument für Produktstrategie & Roadmap |
| `WORKING_STATE.md` | 100 % autogeneriert aus Inline-Tags (`cargo xtask sync-docs`), Session-zu-Session-Handoff |
| `DECISIONS.md` | Chronologisches, append-only ADR-Log (aktuell 30 ADRs) |
| `docs/TYPE_REGISTRY.md` | Zentrales Typ-/Trait-Register gegen Namenskollisionen |
| `.jules/AUDIT_INTAKE_PROTOCOL.md` | Pflicht zur Verifikation externer Audit-Befunde am aktuellen Code vor Implementierung |
| `.githooks/pre-commit` | Automatischer `cargo fmt`-Zwang vor jedem Commit |
| `rules/simd_safety.md`, `rules/tag_taxonomy.md` | Domänenspezifische Detailregeln |

Das Tag-System (`AI-TAG[KATEGORIE][SCHWEREGRAD]`, sekundengenaue Zeitstempel, hash-basierte IDs,
verpflichtendes Mehrfach-Session-Review `REVIEW-PASS[N/M]`) ist ungewöhnlich rigoros für ein
Ein-Personen-Projekt und wurde in den letzten Tagen mehrfach gehärtet (ADR-028, ADR-029). Dieses
System — nicht ein einzelnes Feature — ist der Grund, warum ein KI-entwickeltes Projekt dieser
Größe (57.000+ LOC über 14 Crates) überhaupt kohärent bleibt.

---

## 4. Positionierung & Zielmarkt (Stand heute, aus README/SOURCE_OF_TRUTH übernommen)

MemFuse positioniert sich explizit **nicht** als Ersatz für Cloud-Vektordatenbanken (Qdrant,
Pinecone), sondern als neue Kategorie: **lokales Cognitive Operating System für LLM-Agenten** —
in-process, air-gapped, Pure Rust.

| Kriterium | MemFuse | Mem0 | Zep/Graphiti | Chroma+ES+Neo4j |
|---|---|---|---|---|
| Air-gapped | ✅ | ❌ | ❌ | ✅ |
| 4-Signal-Fusion | ✅ | ❌ | Teilweise | Extern (geklebt) |
| Pure Rust | ✅ | ❌ | ❌ | ❌ |
| MCP-nativ | ✅ | ❌ | ❌ | ❌ |
| Contextual Retrieval | ✅ | ❌ | ❌ | ❌ |
| Session DAG | ✅ | ❌ | ❌ | ❌ |
| Kein Docker | ✅ | ❌ | ❌ | ❌ |

**Wichtiger Vorbehalt**: Diese Tabelle ist eine Feature-Gegenüberstellung, kein empirischer
Benchmark. Es existiert (Stand heute) noch keine tatsächlich ausgeführte Vergleichsmessung gegen
Mem0/Zep/Chroma (siehe Abschnitt 6, Benchmark-Suite ist Phase-4-Roadmap-Punkt).

**Realistischer Zielkunde** (aus vorheriger Strategiediskussion abgeleitet, nicht aus dem Code):
KMU/Kanzleien/Behörden im DACH-Raum mit Datenschutz-/Air-Gap-Anforderungen, die lokale
Dokumenten-RAG ohne Cloud-Abhängigkeit und ohne Docker-Betriebsaufwand benötigen — **nicht**
Verteidigungs-/Drohnen-Beschaffung (dafür fehlen Zertifizierungen, die einem Solo-Entwickler
faktisch verschlossen sind) und **nicht** direkter Wettbewerb zu verteilten Cloud-Vektor-DBs.

---

## 5. Das gewünschte Endprodukt nach vollständiger Roadmap-Umsetzung

Basierend auf `README.md`/`docs/SOURCE_OF_TRUTH.md`, Roadmap-Abschnitt "Cognitive Operating
System":

### Phase 1 — RAG-Fundament (✅ abgeschlossen)
LSM-Storage mit MVCC/WAL/Crash-Recovery, HNSW mit SIMD, BM25 mit deutscher Morphologie,
CSR-Wissensgraph, 4-Signal-Fusion, Contextual Retrieval (Anthropic-Pattern), Cross-Encoder
Reranking, Multi-Step Query Engine (OpenAI-o-Series-Pattern), Context Compaction (Grok-Pattern),
Session-DAG-Branching (Grok-Pattern), MCP-Sandbox-Isolation, Desktop-App + MCP-Server +
Python-Bindings.

### Phase 2 — Cognitive Memory (Ziel: Q4 2026 laut Doku)
- Kognitive Gedächtnistypen als explizite Collection-Typen: Episodic / Semantic / Procedural /
  Working Memory.
- Temporaler Wissensgraph: bi-temporale Zeitachsen (Validitätszeit + Transaktionszeit).
- Memory Importance Score (LLM-bewertet, Generative-Agents-Pattern).
- Recency-Decay-Funktion für episodische Relevanz.

**Verifizierter Befund**: `ImportanceScore` und `DecayFunction` existieren bereits als
Zero-Panic-Typen in `memfuse-core/src/types/importance.rs`, inklusive `effective_score()`-Filterung
in `Collection`. Phase 2 ist damit in Teilen bereits Code, nicht nur Planung — siehe Abschnitt 7.

### Phase 3 — Selbstorganisierung (Ziel: Q1 2027 laut Doku)
- Memory Consolidation (automatische Zusammenfassung veralteter Chunks).
- Personalized PageRank (PPR) für Multi-Hop-Graph-Retrieval.
- Community Detection für semantische Cluster.
- A-MEM-Zettelkasten-Pattern (explizite Querverweise zwischen Memories).

**Verifizierter Befund**: PPR (`personalized_page_rank()`) und Community Detection
(`detect_communities()`, `run_community_detection()`) sind bereits implementiert und über die
öffentliche API von `Collection` erreichbar (`collection.rs:1465`, `:1795`). Der jüngste Commit
(`73617c4`, "enhance PPR robustness, community persistence") härtet diese Funktionalität sogar
weiter. Phase 3 ist also — entgegen der Checkbox-Darstellung in README/SOURCE_OF_TRUTH — bereits
überwiegend Code, nicht nur Planung.

### Phase 4 — Enterprise (Ziel: Q2 2027 laut Doku)
- OAuth 2.0 für MCP-Server.
- RBAC und Multi-Tenant-Isolation.
- Audit-Trail mit unveränderlichen Logs.
- Benchmark-Suite vs. Mem0, Zep/Graphiti, MemOS.

**Verifizierter Befund**: Für keinen dieser vier Punkte existiert Code. Das ist tatsächlich noch
vollständig Zukunft — hier ist die Roadmap-Doku akkurat.

### Das Endprodukt in einem Satz

Nach vollständiger Umsetzung aller vier Phasen wäre MemFuse ein **air-gapped, Pure-Rust
Cognitive Operating System für lokale KI-Agenten**: eine eingebettete Datenbank, die nicht nur
Dokumente hybrid durchsucht, sondern Gedächtnis aktiv verwaltet (Wichtigkeit, Verfall,
Konsolidierung, bi-temporale Historie), sich selbst organisiert (Cluster, Zusammenfassungen,
Querverweise) und in Enterprise-Umgebungen mit Zugriffskontrolle und Audit-Fähigkeit betrieben
werden kann — bei gleichzeitigem Verzicht auf Server/Docker/Cloud-Abhängigkeit.

---

## 6. Qualitäts-Gates & Definition of Done (Ist-Zustand)

Aus `CONSTITUTION.md` / `docs/SOURCE_OF_TRUTH.md`, Abschnitt "Qualitäts-Gates":

1. `cargo check --workspace` — Typsystem/Kompilierbarkeit.
2. `cargo test --workspace` — vollständige Testsuite.
3. `just check` — Formatierung + Clippy (`-D warnings`).
4. `just triple-test` — Triple-Run Flaky-Test-Detektor.
5. `cargo xtask check-review-coverage` (CI Gate 8, ADR-028) — erzwingt Mehrfach-Session-Review
   (2 bzw. 3 unabhängige `REVIEW-PASS`-Einträge) vor jeder `STATUS:DONE`-Markierung.

**Zero-Panic-Invariante**: Laut `SOURCE_OF_TRUTH.md` Stand 🟡 "In Arbeit" — namentlich benannte
offene `.expect()`-Stellen in `SessionPool::pop()`/`push()` (memfuse-embed) und `snapshot.rs`
(memfuse-core). Dieser Status wird laut eigener Doku-Regel ausschließlich durch CI gesetzt, nicht
durch manuelle Einschätzung — ein Muster, das direkt der zuvor identifizierten "Verifizieren statt
behaupten"-Lektion entspricht.

**Bekannte offene Tags (Stand `WORKING_STATE.md`, 2026-08-27, vor den jüngsten Commits)**: 4 Tags
mit CRITICAL/MINOR-Schweregrad, zwei davon SECURITY-CRITICAL (SIMD-Preconditions,
WAL-Integritätsschlüssel-Erstellung). Beide wurden laut Commit-Historie (`3b5f645`, WAL-V3-ADR)
zwischenzeitlich bearbeitet — **ungeprüft, ob `WORKING_STATE.md` das inzwischen widerspiegelt**
(siehe nächster Abschnitt).

---

## 7. Kritische Befunde dieser Analyse — Roadmap-Code-Drift

Dies ist der wichtigste Abschnitt für dich als Projektverantwortlichen, weil er zeigt, wo Dokument
und Code auseinanderlaufen — in beide Richtungen:

| # | Befund | Richtung der Abweichung | Handlungsempfehlung |
|---|---|---|---|
| 1 | PPR + Community Detection sind vollständig implementiert und über die öffentliche API nutzbar, aber README/SOURCE_OF_TRUTH führen sie noch als unmarkierte Checkbox unter "Phase 3, Q1 2027" | **Code ist der Doku voraus** | Roadmap-Checkboxen aktualisieren — sonst wird intern und extern der Fortschritt systematisch unterschätzt |
| 2 | `ImportanceScore`/`DecayFunction`/`effective_score()`-Filterung (Phase-2-Feature) existieren bereits als Zero-Panic-Typen mit eigenem ADR | **Code ist der Doku voraus** | Ebenfalls in Roadmap-Status korrigieren |
| 3 | `WORKING_STATE.md` (Stand 2026-08-27) listet 4 offene CRITICAL/MINOR-Tags; neuere Commits (`3b5f645`, WAL-V3) adressieren mindestens 2 davon inhaltlich | **Unklar, ob Doku aktuell ist** | Vor nächster Session `just sync-docs` ausführen und verifizieren, dass die 4 Tags tatsächlich auf 0–2 gesunken sind, nicht raten |
| 4 | Zwei ADR-Einträge tragen identisch die Nummer "ADR-029" (WAL-V3 und Governance-Härtung) | **Doku-interne Inkonsistenz** | Einen der beiden Einträge in `DECISIONS.md` umnummerieren (z.B. Governance-Härtung → ADR-031), sonst bricht künftige Referenzierbarkeit |
| 5 | Positionierungstabelle (Abschnitt 4) behauptet Vorteile gegenüber Mem0/Zep/Chroma, aber keine tatsächliche Benchmark-Ausführung liegt vor (Phase-4-Punkt, noch nicht gestartet) | **Doku ist der Empirie voraus** | Claims als "architektonisch, noch nicht empirisch validiert" kennzeichnen, bis die Benchmark-Suite (bereits als Prompt vorbereitet) tatsächlich läuft |

**Warum das wichtig ist**: Befund 1 und 2 sind eigentlich gute Nachrichten — dein Projekt ist
weiter fortgeschritten, als die eigene Roadmap-Darstellung suggeriert. Aber Befund 3–5 zeigen,
dass die Selbstauskunft des Projekts (Roadmap-Status, ADR-Nummerierung, Wettbewerbsvergleich)
nicht in jedem Fall mit dem verifizierten Ist-Zustand übereinstimmt — genau das Muster, das das
`AUDIT_INTAKE_PROTOCOL.md` verhindern soll, hier aber projektintern (nicht extern zugeliefert)
auftritt. Empfehlung: `just sync-docs` regelmäßiger und die Roadmap-Checkboxen als Teil des
Session-Ende-Protokolls (nicht nur `WORKING_STATE.md`) explizit mit-pflegen.

---

## 8. Offene strategische Fragen (unverändert seit letzter Analyse, noch nicht beantwortet)

- Zielkunden-Persona schärfen: Desktop-App-Nutzer, Library-Integrator (Rust/Python-Crate) und
  MCP-Server-Betreiber sind drei verschiedene Käufer mit unterschiedlichen Anforderungen — aktuell
  parallel bedient, ohne erkennbare Priorisierung.
- Monetarisierungsmodell: MIT/Apache-2.0-Lizenz erlaubt beliebiges Forken — Erlösquelle (Support,
  Enterprise-Features, Hosting) ist noch nicht festgelegt.
- Reale RAM-/Skalierungsgrenze: noch nicht empirisch gemessen (Benchmark-Prompt liegt vor, aber
  laut dieser Analyse noch nicht ausgeführt).
- Air-Gap als hartes Muss vs. Nice-to-have für die Zielkundschaft — entscheidet, ob langfristig
  ISO-27001/BSI-Grundschutz-Nachweise nötig werden.

---

## 9. Zusammenfassung für Entscheidungsträger

MemFuse ist heute eine **technisch überdurchschnittlich solide, aktiv und schnell weiterentwickelte
eingebettete Hybrid-Retrieval-Engine** mit einem für ein Solo-Entwickler-Projekt außergewöhnlich
disziplinierten Governance-System. Die Kernrisiken liegen nicht mehr primär im Code (die
kritischsten architektonischen Lücken aus früheren Audits — unvollständiges 2PC, FFI-Fehlersemantik,
HNSW-Deref-Antipattern, SIMD-Preconditions — sind laut Commit-Historie inzwischen behoben), sondern:

1. in der **Selbstauskunfts-Genauigkeit** des Projekts (Roadmap-Status hinkt dem Code teils
   hinterher, teils ist der Code der Doku voraus — Abschnitt 7),
2. im **fehlenden empirischen Nachweis** der behaupteten Wettbewerbsvorteile,
3. in der **noch ungeklärten Monetarisierungs- und Zielkundenstrategie**.

Der technische Kern ist reif genug, um jetzt an Positionierung, Messbarkeit und Geschäftsmodell zu
arbeiten — nicht mehr primär an Grundstabilität.
