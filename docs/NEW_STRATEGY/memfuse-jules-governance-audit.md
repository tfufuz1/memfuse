# MemFuse — Governance- & Automatisierungs-Audit für Google-Jules-Entwicklung
**Rolle:** Principal Senior Rust Architect Review
**Repo:** github.com/tfufuz1/memfuse · **Stand:** 2026-09-08 · Verifiziert per Live-Klon

---

## 0. Executive Summary

MemFuse wird zu ~78 % der letzten 14 Tage von parallelen Google-Jules-Sessions (bis zu 25 gleichzeitig, ~195/Tag) entwickelt. Das Repository hat ein **ambitioniertes, aber strukturell defektes** Governance-System: Es definiert Regeln, ADRs, Gates und Preflight-Checks — aber ein signifikanter Teil davon ist **nicht verdrahtet, referenziert falsche Pfade, oder widerspricht sich selbst**. Die Grundursache ist immer dieselbe: **Dokumentation und Tooling wurden von verschiedenen, nicht koordinierten Jules-Sessions verändert, ohne dass ein Gate die Konsistenz zwischen ihnen erzwingt** — genau das Problem, das die Governance eigentlich verhindern sollte.

**Kernbefund Nr. 1 (am schwersten):** `AGENTS.md` — die Datei, die per Selbstdefinition "Vorrang vor allem" hat — wird von Jules **nicht garantiert automatisch geladen** (bestätigt durch `.jules/JULES_LOG.md`, widersprochen von `.jules/JULES_LOG_2.md`). Das Verhalten ist nicht-deterministisch. Das gesamte Governance-Modell ruht auf einer falschen Laufzeit-Annahme.

**Kernbefund Nr. 2:** Das zentrale CI-Gate `context-gates.yml` ruft einen xtask-Subcommand (`check-duplicate-intent`) auf, der **im Code nicht existiert** (Modul nicht deklariert). Der Job läuft entweder permanent rot oder ist deaktiviert — in beiden Fällen ist das Kern-Gate wirkungslos.

**Kernbefund Nr. 3:** `AGENTS.md` selbst enthält eine sachlich falsche Aussage (`memfuse-kv-bridge` existiere nicht), obwohl das Crate fertig implementiert ist — das gefährlichste Dokument im Repo lügt an der Stelle, wo es am meisten vertraut wird.

Die restlichen ca. 20 Befunde unten sind Symptome desselben Musters: **Fehlende Single Source of Truth, fehlende Durchsetzung, fehlende Synchronisation zwischen parallelen Agenten.**

---

## 1. Kritisch — CI/Gates blockierend oder wirkungslos

| # | Befund | Beleg | Auswirkung |
|---|---|---|---|
| 1 | `context-gates.yml` ruft `xtask check-duplicate-intent` auf; `check_duplicate_intent.rs` ist **nicht** via `mod` deklariert, existiert nicht als Match-Arm in `main.rs` | verifiziert: `grep "^mod "` in `main.rs` listet nur 6 Module, `check_duplicate_intent` fehlt; kein Match-Arm dafür | Workflow schlägt bei jedem Push/PR fehl → entweder dauerhaft rot (blockiert Merges) oder als „nicht required" ignoriert (Gate wirkungslos) |
| 2 | Sechs xtask-Module sind toter Code — nicht deklariert, nicht kompiliert, nicht aufrufbar: `check_duplicate_intent.rs`, `init_audit_fix.rs`, `jules_preflight.rs`, `generate_adr.rs`, `check_type_registry.rs`, `validate_pr_checklist.rs` | verifiziert direkt an `main.rs`: nur `check_commit_messages`, `check_duplicate_symbols`, `check_jules_context_freshness`, `check_placeholder_refs`, `check_vetoes`, `gen_prompter_data` sind deklariert | ~50 KB fertiger Rust-Code (inkl. eines vollständigen, produktionsreifen Preflight-Aggregators `jules_preflight.rs`) ist funktional inexistent |
| 3 | `justfile` hat **kein** `jules-preflight`-Kommando, obwohl `.jules/JULES_LOG.md` es als Standardbefehl empfiehlt | grep bestätigt Fehlen | Empfohlener Workflow-Schritt existiert für Jules gar nicht |
| 4 | Die eigene "Unknown xtask command"-Fehlermeldung in `main.rs` ist veraltet und listet existierende Befehle (`check-vetoes`, `check-placeholder-refs`) nicht auf | verifiziert per Zeilenvergleich | Zusätzliche Verwirrung bei Fehlbedienung durch Agenten |
| 5 | `ast-grep` (`sg`) wird in Setup-Skript, Dockerfile und CI **nirgends installiert**, obwohl `justfile`-Schritt „Debt-Audit [4/5]" es aufruft (`rules/detect_nested_locks.yml`) | grep über Setup-Skript negativ; `justfile:177` referenziert `sg scan` | Deadlock-/Lock-Hierarchie-Check (Kernprinzip laut `CONSTITUTION.md`) läuft de facto nie, fällt still auf „übersprungen" zurück |
| 6 | Jules-Setup-Skript springt kommentarlos von `[3/8]` auf `[5/8]` — Schritt `[4/8]` fehlt vollständig | im bereitgestellten Skript sichtbar | passt exakt zur fehlenden ast-grep-Installation — vermutlich ersatzlos entfernter Schritt, nie bereinigt |
| 7 | `.githooks/pre-commit` existiert, aber `core.hooksPath` wird nirgends gesetzt (weder Setup-Skript noch `justfile` noch Docs) | grep negativ | Hook ist in jeder frisch geklonten Jules-VM wirkungslos, außer manuell konfiguriert (passiert nie) |
| 8 | `check_placeholder_refs` prüft Referenzrichtung falsch (bzw. unzureichend) für ADR-Duplikate — hätte die 4-fache ADR-070-Kollision technisch fangen können, tut es nicht | Codeprüfung des Gates | Ein vorhandenes Gate deckt genau den Fehlerfall nicht ab, der am meisten Schaden anrichtet |
| 9 | `rust-ci.yml` führt den kompletten Workspace-Testlauf **3×** vollständig aus ("Triple-Run Flaky-Detection") ohne Caching-Vorteil | Workflow-Datei, Z. 52-58 | 3× CI-Zeit/-Kosten für ein Problem, das `cargo-nextest --retries` oder gezielter Re-Run gelöster Tests eleganter löst |
| 10 | `dag-check.yml` überschneidet sich funktional mit Teilen von `context-gates.yml`/`rust-ci.yml` | Workflow-Vergleich | unnötiger Parallelitäts-Overhead, zusätzliche Runner-Kosten |
| 11 | `scripts/check_unsafe_audit.py` ist in keinem `justfile`-Target oder Workflow referenziert | grep negativ | totes Skript oder unvollständig integrierte Sicherheitsprüfung |

## 2. Kritisch — Dokumentation widerspricht Code / sich selbst

| # | Befund | Beleg |
|---|---|---|
| 12 | `AGENTS.md` (höchste Priorität laut eigener Quellenhierarchie §0.1) behauptet, `memfuse-kv-bridge` existiere nicht — tatsächlich vollständig implementiert bis Increment 2, im `Cargo.toml`, mit eigenem ADR, korrekt gelistet in `README.md`, `SOURCE_OF_TRUTH.md`, `ARCHITECTURE.md`, `WORKING_STATE.md` | Cross-Check aller Root-Docs + Cargo.toml + Crate-Verzeichnis |
| 13 | ADR-Nummernkollisionen: `ADR-070` existiert **4×**, `ADR-072` **2×** mit unterschiedlichem Inhalt | `docs/decisions/` Dateiliste |
| 14 | `ADR-060` (2026-09-04) beschließt explizit die Auflösung von `docs/decisions/` zugunsten von `DECISIONS.md` als Single Source of Truth — nie umgesetzt; danach wurden dort 12 weitere ADRs (061–072) angelegt, `xtask generate_adr.rs` schreibt hartcodiert weiterhin dorthin | Vergleich ADR-060-Inhalt vs. Dateisystem |
| 15 | `CONSTITUTION.md`: „5 Layers, 0–4" vs. `AGENTS.md`: „Layer 0–6" | Direktvergleich |
| 16 | ADR-069 verbietet biologische Metaphern/Anbieter-Branding — Typen wie `ImmunMemory`, `FreeEnergyThermostat`, Feature-Flags `physio-*` bestehen unverändert | grep im Code |
| 17 | Mehrere Docs referenzieren nicht-existente Anker `AGENTS.md §1/§3/§4` — `AGENTS.md` hat nur `##`-Überschriften, keine nummerierten Paragraphen. Betrifft `SECURITY.md`, `AUDIT_INTAKE_PROTOCOL.md`, `TYPE_REGISTRY.md`, `COMMON_LLM_ERRORS.md` | grep über alle Docs |
| 18 | `TYPE_REGISTRY.md` und weitere Docs referenzieren `crates/memfuse-db/src/collection.rs:NN` — diese Datei existiert nicht mehr, `collection.rs` ist inzwischen ein Verzeichnis (`collection/{crud,search,maintenance,...}.rs`) | Dateisystem-Check |
| 19 | `SECURITY.md` (Z. 27) dokumentiert selbstkritisch eine ungeschützte HMAC-Schlüssel-Schwachstelle im Klartext — gute Transparenz, aber laut Doku ungelöst | Dateiinhalt |
| 20 | `SCHEDULED_AUDIT.md` referenziert `.unwrap-baseline.txt`, tatsächliche Datei ist `.unwrap-baseline.json` | Dateisystem-Check |
| 21 | `TESTING.md` enthält absolute lokale Pfade des Solo-Entwicklers (`/home/freddy/Arbeitsplatz/DEV/memfuse/...`) — auf der Jules-VM garantiert broken Links | grep |
| 22 | `scripts/fix_db.py`, `fix_db2.py`, `fix_sstable.py`: rohe, ungetestete Regex-Patches direkt auf Rust-Quellcode, nirgends dokumentiert/referenziert | Codeinhalt + grep |
| 23 | `docs/GITHUB_ANALYSE.md` bestätigt unabhängig, auf Commit-Ebene, exakt dasselbe Muster: dreifache Parallel-Implementierung derselben Funktion (`ConfigFingerprint`, drei Sessions in ~66 Minuten, ADR-063) mit einem konkreten daraus resultierenden Code-Defekt (falsch verschobene Zeile im Konstruktor) | 78 % der Commits in 14 Tagen, 81 aktive Branches, 25 mit `jules-*`-Präfix, 0 Merge-Commits (nur lineare Squashes) |

## 3. Fundamentaler Governance-Denkfehler (aus Jules' eigenen Logs)

| # | Befund |
|---|---|
| 24 | `JULES_LOG.md` (Z. 26, 63-65) bestätigt: `AGENTS.md` wird **nicht** automatisch/ambient geladen — nur bei explizitem `read_file`-Aufruf, ausgelöst durch Nutzeranweisung. Widerspricht direkt `AGENTS.md`/`CONSTITUTION.md`, die „ambient, immer geladen" behaupten |
| 25 | `JULES_LOG_2.md` (Z. 24-25, 71) widerspricht Log 1: dort wurde `AGENTS.md` als 4. Tool-Call automatisch gelesen. Das Ladeverhalten ist **nicht-deterministisch**, abhängig vom individuellen Session-Verlauf |
| 26 | `JULES_LOG_2.md` (Z. 104) empfiehlt eine modulare, crate-lokale `AGENTS.md`-Struktur — diese existiert inzwischen teilweise (`crates/*/AGENTS.md`), aber `.jules/JULES_CONTEXT.md` (Z. 30-44) listet `memfuse-calibration` und `memfuse-candle` ohne Eintrag und **`memfuse-kv-bridge` fehlt komplett** in der Ladetabelle — dasselbe Vergessens-Muster wie Befund #12 |
| 27 | `SESSION_BOOTSTRAP.md` fordert „MANDATORY FIRST STEP" (Bootstrap lesen) — laut Log **nicht technisch erzwungen**, reine Selbstverpflichtung ohne Durchsetzungsmechanismus |
| 28 | `.jules/`-Ordner (inkl. der wertvollen `COMMON_LLM_ERRORS.md` und `AUDIT_INTAKE_PROTOCOL.md`) wird laut Aufgabenstellung des Nutzers **nicht standardmäßig** von Jules gelesen — das gesamte kuratierte Wissen darin ist faktisch unwirksam, solange kein expliziter Trigger es einbindet |

---

## 4. Root-Cause-Analyse

Alle Befunde reduzieren sich auf **drei** strukturelle Ursachen:

1. **Keine erzwungene Konsistenzprüfung zwischen Dokumentation, ADRs und Code.** Jedes Dokument kann unabhängig von jeder Jules-Session verändert werden; nichts vergleicht `AGENTS.md` gegen den tatsächlichen Crate-Zustand, nichts prüft ADR-Nummern vor dem Commit, nichts prüft, ob ein referenzierter Dateipfad/Anker noch existiert.
2. **Kein zentrales, erzwungenes Preflight-Gate für parallele Sessions.** `jules_preflight.rs` existiert fertig, ist aber nicht verdrahtet. Ohne ein Gate, das läuft, *bevor* eine Session zu schreiben beginnt (Claim-Mechanismus, Locking auf Feature-/ADR-Ebene), kollidieren 25 parallele Sessions zwangsläufig — belegt durch die dreifache `ConfigFingerprint`-Implementierung.
3. **Fehlende Trennung von „ambient" (garantiert geladen) und „on-demand" (nur bei Anfrage geladen) Kontext**, kombiniert mit einer falschen Selbstannahme darüber, was „ambient" tatsächlich bedeutet. Das System wurde so designt, als lese Jules `AGENTS.md` und `.jules/` immer — das stimmt nachweislich nicht.

---

## 5. Optimierungsspezifikation

### 5.1 Sofortmaßnahmen (vor der nächsten Session-Welle)

1. **xtask reparieren:** Alle sechs verwaisten Module entweder in `main.rs` verdrahten (`mod` + Match-Arm) oder aus dem Repo entfernen, falls obsolet. Priorität: `jules_preflight.rs` zuerst, da es der zentrale fehlende Baustein ist.
2. **`context-gates.yml` korrigieren:** `check-duplicate-intent` entweder implementieren/verdrahten oder aus dem Workflow entfernen, bis es existiert. CI darf nicht permanent rot oder ignoriert laufen.
3. **`AGENTS.md` faktisch korrigieren:** `memfuse-kv-bridge`-Status berichtigen; anschließend ein Gate schreiben (`check_agents_md_crate_list.rs`), das bei jedem PR `AGENTS.md`-Crate-Liste gegen `Cargo.toml`-Workspace-Mitglieder abgleicht und bei Abweichung failt.
4. **ADR-Kollisionen auflösen:** ADR-070 (4×) und ADR-072 (2×) manuell umbenennen/neu nummerieren; `generate_adr.rs` reparieren, sodass es die höchste vorhandene Nummer live aus dem Dateisystem liest (nicht aus einer möglicherweise veralteten Zähler-Datei) und **atomar** (Lock-Datei oder GitHub-Issue-Reservierung) vergibt, damit parallele Sessions nicht dieselbe Nummer ziehen.
5. **`ADR-060` entweder umsetzen oder zurückziehen:** Entweder `docs/decisions/` tatsächlich auflösen und `generate_adr.rs` auf `DECISIONS.md` umstellen, oder ADR-060 durch einen neuen ADR formal revidieren. Ein beschlossener, aber ignorierter Beschluss ist schädlicher als keiner.
6. **`ast-grep` in Setup-Skript aufnehmen** (fehlendes `[4/8]`) und `core.hooksPath` im Setup-Skript setzen (`git config core.hooksPath .githooks`).
7. **Tote/riskante Skripte entscheiden:** `fix_db.py`, `fix_db2.py`, `fix_sstable.py`, `check_unsafe_audit.py` entweder löschen (falls obsolet) oder in `justfile` einbinden + dokumentieren + testen.

### 5.2 Strukturelle Maßnahmen (Governance-Redesign)

8. **Single Source of Truth für Crate-Status erzwingen:** `WORKING_STATE.md` (bereits autogeneriert) als einzige Wahrheit über Crate-Existenz/Status deklarieren; `AGENTS.md` referenziert es statt eigene Status-Aussagen zu duplizieren. Ein xtask-Gate blockt jeden PR, der `AGENTS.md` mit widersprüchlichem Status-Text verändert.
9. **Locking-Mechanismus für parallele Sessions:** Vor Arbeitsbeginn muss eine Session ein „Claim" auf betroffene ADR-IDs/Dateien/Features setzen (z. B. via GitHub-Issue-Label oder einer `CLAIMS.md`/Lock-Datei mit Zeitstempel + Session-ID), das `jules_preflight.rs` prüft und bei Konflikt abbricht. Das ist die einzige strukturelle Lösung gegen das dreifache-Implementierung-Muster aus §2.23.
10. **„Ambient vs. On-Demand" explizit dokumentieren und technisch erzwingen:** Da nicht garantiert ist, dass Jules `AGENTS.md`/`.jules/` automatisch lädt, muss der **Session-Trigger-Prompt selbst** (das Prompter-Tool, siehe 5.3) einen Pflicht-Präfix enthalten, der `AGENTS.md` und `.jules/SESSION_BOOTSTRAP.md` explizit per Pfad referenziert und deren Lesen zur ersten Aktion macht — Selbstverpflichtung in Doku reicht nachweislich nicht.
11. **Ankersystem reparieren:** `AGENTS.md` entweder echte nummerierte `##` mit stabilen HTML-Ankern (`<a id="1">`) versehen, oder alle Docs, die `§1/§3/§4` referenzieren, auf tatsächliche Überschriften umstellen. Ein Gate (`check_broken_anchors.rs`, ggf. Erweiterung von `check_placeholder_refs.rs`) validiert das bei jedem PR.
12. **Konsolidierung der Redundanz:** `dag-check.yml` in `context-gates.yml` oder `rust-ci.yml` integrieren statt separatem Workflow; Triple-Test-Run durch `cargo-nextest --retries 2` ersetzen (spart ca. 2/3 CI-Zeit).
13. **`.jules/`-Inhalte upstreamen:** Wertvolle Inhalte aus `COMMON_LLM_ERRORS.md` und `AUDIT_INTAKE_PROTOCOL.md` in `AGENTS.md` selbst verschieben (oder per Pflicht-Include im Prompter-Tool referenzieren), da `.jules/` nachweislich nicht zuverlässig gelesen wird.
14. **Veraltete Referenzen bereinigen:** `TESTING.md`-Lokalpfade entfernen, `.unwrap-baseline.txt`→`.json` korrigieren, `collection.rs`-Zeilenreferenzen auf das neue Verzeichnis-Layout aktualisieren.

### 5.3 Workflow-/Prompter-Integration

15. **Prompter-Tool (`Memfuse-Prompter-v24.html`) erweitern:**
    - Pflichtfeld/Pflicht-Präfix, der bei jeder generierten Session-Anweisung automatisch `AGENTS.md`, `.jules/SESSION_BOOTSTRAP.md` und die relevante(n) `crates/*/AGENTS.md` explizit zum Lesen anweist (behebt Befund #24/25 strukturell, da man sich nicht auf „ambient" verlassen kann).
    - Ein einfaches Claim-Feld: vor Generierung eines Prompts prüft das Tool (idealerweise gegen die neue `CLAIMS.md`, s. Punkt 9) auf offene Claims im selben Bereich und warnt den Nutzer vor dem Start einer weiteren parallelen Session auf demselben Feature.
    - Da bereits ein `gen_prompter_data`-Modul existiert: dieses so erweitern, dass es automatisch den aktuellen `WORKING_STATE.md`-Stand injiziert, damit generierte Prompts nie auf veraltetem Crate-Status basieren.
16. **Zwei-Stufen-Modell explizit dokumentieren:** Der beschriebene Claude→Jules-Workflow (Analyse vs. Implementierung) sollte in `AGENTS.md` als offizieller Prozess festgehalten werden, inkl. der Regel, dass Jules bei Unsicherheit über Architekturentscheidungen (ADR-relevant) *keinen* neuen ADR selbst erstellt, sondern eine Draft-Notiz für die Analyse-Stufe hinterlässt — das würde ADR-Kollisionen strukturell reduzieren.

### 5.4 Priorisierte Reihenfolge

| Reihenfolge | Maßnahme | Aufwand | Wirkung |
|---|---|---|---|
| 1 | `jules_preflight.rs` verdrahten + in `context-gates.yml`/`justfile` einbinden | niedrig | sehr hoch |
| 2 | `context-gates.yml` reparieren (toten Subcommand entfernen/fixen) | niedrig | sehr hoch |
| 3 | `AGENTS.md` Crate-Status korrigieren + Gate dagegen | niedrig | sehr hoch |
| 4 | ADR-Kollisionen auflösen + atomare Nummernvergabe in `generate_adr.rs` | mittel | hoch |
| 5 | Claim-/Lock-Mechanismus für parallele Sessions | mittel-hoch | sehr hoch (verhindert #2.23-Muster) |
| 6 | Prompter-Tool Pflicht-Präfix + Claim-Warnung | niedrig | hoch |
| 7 | Ankersystem, tote Skripte, Setup-Skript-Lücke, Hooks-Pfad | niedrig je Punkt | mittel |
| 8 | CI-Konsolidierung (Triple-Run, dag-check) | niedrig | mittel (Kosten) |

---

## 6. Offene Punkte für Rückfrage

- Soll `docs/decisions/` tatsächlich aufgelöst werden (ADR-060 umsetzen), oder soll stattdessen ADR-060 zurückgezogen werden? Das ist eine Architekturentscheidung, die vor der Automatisierung von `generate_adr.rs` getroffen werden sollte.
- Soll der Claim-/Lock-Mechanismus als einfache Markdown-Datei (schnell, aber race-condition-anfällig bei echtem Parallelbetrieb) oder als GitHub-Issue-/Label-basiertes System (robuster, aber mehr API-Integration) umgesetzt werden?
- Sollen `fix_db*.py`/`fix_sstable.py` als Referenz behalten (mit klarer „NICHT AUSFÜHREN"-Markierung) oder komplett entfernt werden?
