# MemFuse — Zielkohärenz-Audit: Wo widerspricht sich das Projekt selbst?

> Diese Analyse beantwortet eine andere Frage als der vorherige Bug-Report: nicht "ist der
> Code korrekt", sondern **"erzählen Doku, Architektur-Entscheidungen und Code noch dieselbe
> Geschichte?"**. Methodik: Alle Grundsatzdokumente (README, AGENTS.md, CONSTITUTION.md,
> SOURCE_OF_TRUTH.md, ARCHITECTURE.md, DECISIONS.md/ADRs, strategische Roadmap) wurden
> gegeneinander und gegen den tatsächlichen Code-Zustand abgeglichen.

---

## Kernbefund in einem Satz

**Es gab mindestens zwei dokumentierte, harte Strategie-Kehrtwenden (PyPI-Library → Desktop-App
zurück zu "Sovereign Enterprise", HTTP-MCP → stdio-MCP), aber die Nachpflege der Grundsatz-
dokumente ist jedes Mal auf halbem Weg steckengeblieben** — mit der Folge, dass vier von fünf
zentralen Architekturdokumenten heute technisch falsche Aussagen über das System treffen und
die ursprünglich verworfene Positionierung nirgends formal für tot erklärt wurde.

---

## 1. Die verdeckte strategische Kehrtwende: PyPI-Library vs. Sovereign-Enterprise-Desktop-App

Das ist der wichtigste und am wenigsten offensichtliche Fund.

### Was am 19. Juli 2026 offiziell beschlossen wurde
- **`docs/memfuse_strategic_roadmap.md`** (Stand 2026-07-19, "Senior Rust Architect Review"):
  Positioniert MemFuse explizit als **"laser-fokussierte 3-in-1 Agent-Memory-Engine"** —
  *"kein Server, kein Docker"*. Zitat: *"`memfuse-py` ist der **wichtigste Vertriebskanal**!
  Python-Entwickler bauen 95% aller KI-Agenten. Ohne PyPI-Paket existiert das Produkt am
  Markt nicht."* Tauri, Desktop-App oder GUI werden in diesem gesamten Strategiedokument
  **kein einziges Mal** erwähnt (außer als Verweis auf das fremde "Claude Desktop").
- **ADR-007** (selbes Datum, 2026-07-19): Bestätigt dieselbe Linie formal als
  Architekturentscheidung — *"Richtung A (Sovereign Edge-DB) ... aber Enterprise-Vertrieb als
  Solo-Entwickler aktuell **nicht realisierbar**"* → explizit verworfen zugunsten von
  PyPI/crates.io-Distribution.

### Was einen Tag später (und seitdem) tatsächlich passiert ist
- **ADR-009** (2026-07-20, *ein Tag* nach ADR-007): Legt den Grundstein für `memfuse-tauri`
  — eine vollständige Desktop-Anwendung mit eigener GUI, Ingestion-Pipeline (PDF/DOCX/E-Mail),
  Chat-UI und Branding ("MemFuse Brain").
- Heute ist `memfuse-tauri` mit ~1.464 Zeilen Backend-Code + eigenem Frontend das
  **größte einzelne Feature-Investment** im Repo, und README.md positioniert das gesamte
  Produkt primär darüber: *"Ihr lokaler, air-gapped **Unternehmensassistent**"* — exakt die
  Positionierung, die ADR-007 als "aktuell nicht realisierbar" verworfen hatte.
- README enthält **keine `pip install`-Anleitung** — der laut ADR-007/Roadmap "wichtigste
  Vertriebskanal" ist in der aktuellen Nutzerdokumentation komplett unsichtbar, obwohl
  `crates/memfuse-py/pyproject.toml` technisch vollständig auf PyPI-Publishing vorbereitet
  ist (maturin-Build, `mcp`/`fastmcp`-Dependencies).

### Warum das ein echtes Problem ist
Es gibt **keinen ADR, der ADR-007 formal revidiert**. Laut dem eigenen Regelwerk
(`DECISIONS.md`, Kopfzeile: *"kein Agent darf eine dokumentierte Entscheidung eigenmächtig
überschreiben"*) hätte die Kursänderung zur Desktop-App-First-Strategie einen expliziten,
begründeten ADR benötigt, der ADR-007 ersetzt — analog dazu, wie ADR-008 sauber und
korrekt als *"Ersetzt ADR-007 bzgl. lokaler ONNX-Inferenz"* markiert ist. Bei der viel
größeren strategischen Frage "Library vs. Desktop-App" fehlt dieser Schritt komplett.
**Konsequenz**: Zwei sich widersprechende, beide als "final" bzw. nie widerrufen geltende
Strategiedokumente existieren gleichzeitig im Repo.

---

## 2. Der am weitesten verbreitete Einzelfehler: "axum HTTP" für memfuse-mcp

Bereits im letzten Audit gefunden, hier aber im vollen Ausmaß bestätigt: **ADR-010**
(2026-08-23) dokumentiert explizit und korrekt die Migration weg von HTTP:

> *"axum/tower-Abhängigkeiten aus `memfuse-mcp` entfernt; das Crate verwendet nur
> tokio-util + futures-util... Kein HTTP-Listener mehr."*

Der tatsächliche Code bestätigt das zu 100 % — `memfuse-mcp` ist reiner stdio-JSON-RPC-
Transport, kein `axum` in `Cargo.toml`. Trotzdem behaupten **vier von vier** geprüften
zentralen Architekturdokumenten weiterhin das Gegenteil:

| Dokument | Fundstelle | Behauptung |
|---|---|---|
| `README.md` | Crate-Liste | "MCP Server für Tool Calls" — implizit über HTTP-Endpunkte formuliert |
| `docs/SOURCE_OF_TRUTH.md` | Zeile 24, 59 | *"Standalone MCP-Server (**axum HTTP** / JSON-RPC)"* |
| `docs/ARCHITECTURE.md` | Zeile 14 | *"memfuse-mcp — Standalone MCP-Server (**axum HTTP** / JSON-RPC)"* |
| `AGENTS.md` | §2 DAG-Definition | *"Layer 4: `memfuse-mcp` (**Axum HTTP** / JSON-RPC MCP Server)"* |

Das ist deshalb besonders gravierend, weil **AGENTS.md das für alle Agenten/LLMs
verbindliche Regelwerk ist** ("Vor jeder Codeänderung MUSS das LLM folgende Dokumente
bestätigen"). Ein Agent, der sich strikt an AGENTS.md hält, würde bei der nächsten
MCP-Änderung von einer falschen Transport-Architektur ausgehen.

---

## 3. Die AGENTS.md-Regel widerspricht der eigenen ADR

**AGENTS.md §3 (NEVER-Tier)**: *"Kein `unsafe` ohne `// SAFETY:`-Beweis. `unsafe` ist
**ausschließlich** in `memfuse-index/src/distance.rs` erlaubt."*

**ADR-017** (2026-08-24, selber Tag wie viele andere Fixes) sagt wörtlich das Gegenteil:
*"Die generelle Architekturregel (...) wird für `memfuse-index/src/diskann.rs`
**erweitert**."*

Der Code folgt korrekt ADR-017 (mit sauberem SAFETY-Kommentar in `diskann.rs`). Aber
AGENTS.md — das ranghöchste Regeldokument, das laut CONSTITUTION.md sogar denselben
Review-Prozess wie Produktionscode durchlaufen soll — wurde nicht synchron aktualisiert.
Das ist ein Verstoß gegen die im Dokument selbst festgelegte Exit-Kriterien-Regel
(§7 Punkt 4: *"AGENTS.md ... bei API-/Toolchain-Änderungen aktualisiert"*).

---

## 4. "Zero-Panic" wird als erreicht deklariert, ist es aber nicht

`docs/ARCHITECTURE.md` führt eine Tabelle "Invarianten-Status" mit der Zeile:

> **Zero-Panic** | 🟢 Gehärtet | *"Fehlerbehandlung über `MemFuseError` und `?`-Operator
> propagiert."*

Das ist im Sinne von "🟢 abgeschlossen/erledigt" formuliert. Tatsächlich (siehe vorheriger
Bug-Audit) existieren weiterhin produktionsrelevante `.expect()`-Aufrufe außerhalb von
Tests, z. B. in `memfuse-embed/src/lib.rs` (`SessionPool::pop()`) und im generierten
FlatBuffers-Code. Die eigene Roadmap (`memfuse_strategic_roadmap.md`) benennt das Problem
ursprünglich sogar selbst sehr genau ("Zero-Panic-Doktrin wird durch 16+ Quelldateien mit
`.unwrap()` verletzt") — die Statusanzeige in ARCHITECTURE.md wurde optimistischer
nachgezogen, als der Code tatsächlich hergibt. "Gehärtet" suggeriert einen Endzustand, der
laut eigenem Nachweis vom letzten Audit nicht erreicht ist.

---

## 5. Crate-Zahl und Inventar sind uneinheitlich (12 vs. 11 vs. 13)

- `README.md`, `docs/SOURCE_OF_TRUTH.md`, `docs/ARCHITECTURE.md`, `AGENTS.md` sagen alle
  übereinstimmend **"12 Crates"** und listen dieselben 12 (ohne `memfuse-embed`).
- Deine ursprüngliche Aufgabenstellung sprach von **11 Crates**.
- Das tatsächliche Workspace-Verzeichnis (`crates/`) enthält **13 Unterordner**, weil
  `memfuse-embed` zusätzlich existiert, im Workspace registriert ist
  (`Cargo.toml:12`, `Cargo.toml:71`), aber in **keinem** der vier Kerndokumente als
  aktiver Layer-Crate auftaucht.
- Die strategische Roadmap selbst erklärt korrekt, warum: `memfuse-embed` sollte laut
  eigenem Beschluss *"als rein optionales Feature ausgegliedert"* werden — das ist im Code
  auch tatsächlich so umgesetzt (`default = []`-Feature-Gate). Nur wurde diese bewusste
  Randstellung nie sauber im Crate-Inventar der Kerndokumente reflektiert (weder als
  "12+1 optional" noch anderweitig), sodass die "12 Crates"-Aussage bei genauem Hinsehen
  schlicht unvollständig ist.

---

## 6. Wo Ziel und Umsetzung tatsächlich übereinstimmen (zur Einordnung)

Nicht alles ist Drift — folgende Kernaussagen halten der Prüfung stand:

- **"4-Signal-Fusion via RRF"**: Korrekt und konsistent in README, SOURCE_OF_TRUTH,
  ARCHITECTURE.md und im tatsächlichen Code (`fusion.rs`) beschrieben.
- **"Sovereign Core / Pure Rust ohne C-Deps"**: Wird eingehalten — `memfuse-embed` (die
  einzige potenzielle C-Abhängigkeit via ONNX-Runtime) ist bewusst optional/deaktiviert,
  genau wie in ADR-008 und der Roadmap beschlossen.
- **Physische Entfernung von `memfuse-cluster`, `-sandbox`, `-saos-agent`**: Vollständig
  umgesetzt, keine Referenzen mehr im Workspace — sauberster Teil der Scope-Bereinigung.
- **Deutsche Morphologie als Differenzierungsmerkmal**: Konsistent dokumentiert und
  implementiert (`memfuse-text`).
- **Ollama als primäres Embedding-Backend statt ONNX**: ADR-008 korrekt umgesetzt und in
  allen Dokumenten konsistent nachgezogen — im Gegensatz zu ADR-007/-010, hier hat die
  Nachpflege tatsächlich funktioniert.

---

## 7. Priorisierte Liste: Was jetzt geklärt werden sollte

| # | Widerspruch | Typ | Empfohlene Klärung |
|---|---|---|---|
| 1 | ADR-007 (PyPI-Library, "kein Server") vs. gelebte Realität (Tauri-Desktop-App als Hauptprodukt) | **Strategisch, ungelöst** | Menschliche Entscheidung nötig: Neuen ADR schreiben, der ADR-007 explizit ersetzt und die tatsächliche Doppelstrategie (Library **und** Desktop-App) oder eine bewusste Priorisierung festhält. |
| 2 | "axum HTTP" für memfuse-mcp in 4 Kerndokumenten, obwohl ADR-010 stdio beschlossen hat | **Reine Doku-Drift, leicht behebbar** | Textkorrektur in README, SOURCE_OF_TRUTH.md, ARCHITECTURE.md, AGENTS.md — kein Codeeingriff nötig. |
| 3 | AGENTS.md §3 widerspricht ADR-017 (unsafe-Ausnahme) | **Doku-Drift im Regelwerk selbst** | AGENTS.md-Zeile aktualisieren: "...ausschließlich in `distance.rs` **und, gemäß ADR-017 eng begrenzt, in `diskann.rs`**, erlaubt." |
| 4 | "Zero-Panic: 🟢 Gehärtet" trotz verbleibender `.expect()`-Stellen | **Übertriebene Statusangabe** | Status auf 🟡 "in Arbeit" korrigieren, bis die im vorherigen Audit gefundenen Stellen behoben sind. |
| 5 | `memfuse-embed` fehlt in allen vier Kerndokumenten trotz Workspace-Mitgliedschaft | **Unvollständiges Inventar** | Als "13. Crate, optional/Feature-gated" explizit in SOURCE_OF_TRUTH.md und ARCHITECTURE.md aufnehmen. |
| 6 | PyPI-Vertriebsweg technisch vorbereitet, aber nirgends in README dokumentiert | **Lücke zwischen Absicht und Nutzer-Doku** | Falls PyPI weiterhin strategisches Ziel ist: Installationsanleitung in README ergänzen. Falls nicht mehr: ADR-007 formal zurückziehen. |

---

*Diese Analyse beruht ausschließlich auf Textvergleich der im Repository selbst enthaltenen
Grundsatzdokumente (README.md, AGENTS.md, CONSTITUTION.md, docs/SOURCE_OF_TRUTH.md,
docs/ARCHITECTURE.md, DECISIONS.md, docs/memfuse_strategic_roadmap.md) gegen den tatsächlichen
Code-/Dependency-Zustand. Alle Zitate sind wörtlich den genannten Dateien entnommen.*
