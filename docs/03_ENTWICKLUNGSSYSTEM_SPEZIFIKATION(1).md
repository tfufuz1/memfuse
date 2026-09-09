# MemFuse — Spezifikation des Entwicklungssystems (v2, erweitert)

**Verifiziert gegen:** frischer `git clone https://github.com/tfufuz1/memfuse`, HEAD `33226e2` (09.09.2026, im Rahmen dieser Session — ≈ 40 Commits neuer als der in v1 referenzierte Stand `0be90d8`).
**Scope:** Dieses Dokument beschreibt nicht *was* MemFuse ist (→ `02_PROJEKT_SPEZIFIKATION.md`), sondern *wie* der Code entsteht: das Zusammenspiel aus CI-Gates, Google-Jules-Agenten, dem MemFuse-Prompter (v25), einem zweiten, manuell kuratierten Prompt-Erzeugungspfad (den „isolierten Fix-Prompts", neu in v2, Abschnitt 6) und der Governance-Doku-Kette. Jede Aussage ist gegen den tatsächlichen Repo-Inhalt geprüft.

---

## 0. Änderungen gegenüber v1 (Änderungsprotokoll)

| # | Änderung | Beleg |
|---|---|---|
| 1 | HEAD-Referenz von `0be90d8` auf `33226e2` aktualisiert. | `git log` |
| 2 | Gate-Anzahl in `context-gates.yml` von 14 auf **15** korrigiert: neues **Gate 15** (`check-doc-references`, Validierung von Dateipfad-Referenzen in Governance-Dokumenten) ist seit v1 hinzugekommen. | `.github/workflows/context-gates.yml` Zeile 125f. |
| 3 | Vollständige xtask-Kommando-Inventur (18 Subkommando-Dateien statt der in v1 nur beispielhaft genannten vier) — neuer Abschnitt 3.3. | `xtask/src/*.rs` |
| 4 | Neuer CI-Workflow `nucleation-recall-history.yml` (täglich, 30-Tage-Recall-Stabilitätsmessung für `VETO-F02`) sowie das dazugehörige xtask-Subkommando `check-recall-stability` — beide in v1 unbekannt. | `.github/workflows/`, `VETOES.md#VETO-F02` |
| 5 | Neuer Abschnitt 6: vollständige, mikrofeingranulare Spezifikation eines **zweiten Prompt-Erzeugungspfads**, der parallel zum Prompter (v25) existiert — die „isolierten Fix-Prompts" (manuell aus vertieften Stabilisierungsanalysen abgeleitet, nicht über `gen-prompter-data` generiert). Dieser Pfad war in v1 nicht dokumentiert, ist aber zum Zeitpunkt von v2 der aktive Mechanismus, über den zehn laufende Härtungsmaßnahmen an Google Jules verteilt werden. | `MemFuse_Jules_Fix_Prompts_*.md`, Live-Verifikation gegen HEAD |
| 6 | Neuer Abschnitt 7: dokumentiertes Beispiel für Abweichung zwischen Prompt-Spezifikation und tatsächlicher Jules-Implementierung (Prompt B: `LimitExceeded`-Fehler spezifiziert, stiller Cap implementiert) als Beleg für eine bislang ungeschlossene Lücke im Regelkreis aus Abschnitt 5. | `02_PROJEKT_SPEZIFIKATION.md` §3.6/§7, Live-Grep |
| 7 | `docs/GITHUB_HISTORY.md`-Anhang um die PR-Reihe `#1876`–`#1895` ergänzt (Phase 7 Fortsetzung: Scan-Limits, tenant-faire Eviction, RRF-NaN-Härtung, WAL-TOCTOU-Fix, Budget-Drift-Metrik, commit_mutex-Erzwingung, WAL-HMAC-Rollback-Race-Test). | `git log --oneline` |

---

## 1. Das Gesamtbild in einem Satz

MemFuse wird von einem Solo-Architekten plus einem Schwarm autonomer **Google-Jules**-Coding-Agenten entwickelt; Prompts erreichen die Agenten über **zwei parallele, komplementäre Pfade** — (a) den **MemFuse-Prompter** (clientseitiges HTML-Tool, generiert crate-/aufgabenspezifische Session-Prompts aus dem Live-Workspace-Inventar) und (b) **manuell kuratierte, isolierte Fix-Prompts** (aus vertieften menschlichen/LLM-gestützten Stabilisierungsanalysen abgeleitet, direkt copy-paste-fertig, mit expliziter Parallelisierungs-/Merge-Reihenfolge-Anleitung, neu in v2 als Abschnitt 6 spezifiziert); beide Pfade zwingen den Agenten in einen `PLAN→ACT→VERIFY→REFLECT`-Loop mit eingebetteter Anti-Pattern-Matrix bzw. expliziten Akzeptanzkriterien; das Ergebnis (PR) wird von einer Kaskade aus **15 CI-Gates** (`context-gates.yml`) plus Test-/Bench-/Chaos-/Recall-Stabilitäts-Workflows geprüft, bevor es in `main` landet — und die gesamte Historie wird in `docs/GITHUB_HISTORY.md` fortlaufend als Audit-Trail dokumentiert.

Die Systeme bilden eine geschlossene Feedback-Schleife (Prompter-Zweig) plus einen zweiten, lose gekoppelten Feedback-Pfad (Fix-Prompt-Zweig, der denselben CI-Gates unterliegt, aber nicht aus `gen-prompter-data` gespeist wird):

```
 Prompter (v25)                     Isolierte Fix-Prompts (manuell/analysegestützt)
        │  generiert Session-Prompt          │  aus Stabilisierungsanalyse abgeleitet,
        │  mit Scope + APM-Katalog            │  ROLLE/KONTEXT/PROBLEM/AUFGABE/
        │                                     │  NICHT-ZIELE/AKZEPTANZKRITERIEN-Template
        ▼                                     ▼
              Google-Jules-Agent (Ausführung)
                    PLAN → ACT → VERIFY → REFLECT, produziert Commits + PR
                                  │
                                  ▼
    CI-Gates (context-gates.yml [15 Gates] + rust-ci.yml + bench.yml + chaos.yml
              + nucleation-recall-history.yml)
                    automatisierte Ablehnung bei Verstoß, erzwingt Doku-Sync
                                  │
                                  ▼
    Governance-Doku (AGENTS.md, DECISIONS.md, WORKING_STATE.md, GITHUB_HISTORY.md, VETOES.md)
                    wird bei GRÜN automatisch/manuell aktualisiert → nächste Session lädt diesen Stand
                    └──────────────► zurück zum Prompter (gen-prompter-data liest genau diese Doku)
                    └──────────────► zurück zum Fix-Prompt-Autor (nächste Analyserunde verifiziert
                                       gegen den TATSÄCHLICHEN Merge-Stand, nicht gegen die Absicht
                                       der vorherigen Prompt-Runde — s. Abschnitt 7)
```

Der Kreis schließt sich für den Prompter-Zweig über `cargo xtask gen-prompter-data`. Für den Fix-Prompt-Zweig gibt es **keinen äquivalenten automatisierten Rückkanal** — jede neue Analyserunde (N1–N5 → B/E1/E2/D1/D2 → F/G/H) wurde von Hand gegen einen frischen `git clone`/Live-Grep neu verifiziert (s. Abschnitt 6.1). Dies ist ein struktureller Unterschied zum Prompter-Kreislauf und in Abschnitt 7 als offene Lücke dokumentiert.

---

## 2. Google-Jules: Rolle, Betriebsmodell, Beschränkungen

*(unverändert gegenüber v1, gegen HEAD `33226e2` erneut verifiziert — keine strukturellen Änderungen am Claim-System oder der Kontext-Ladeordnung festgestellt)*

### 2.1 Was Jules in diesem Projekt ist
Google Jules ist der autonome Coding-Agent, der den überwiegenden Teil der Commits erzeugt — sichtbar an der Autorenzeile `google-labs-jules[bot]` in praktisch der gesamten Commit-Historie, inklusive aller in Abschnitt 6/7 dieses Dokuments referenzierten Commits (`f29c398` bis `33226e2`). Der menschliche Architekt agiert als Reviewer/Entscheider für nicht-delegierbare Aufgaben (Vision-ADRs, Fristen, Claim-Schema-Festlegung, **sowie — neu belegt in v2 — die Autorschaft der isolierten Fix-Prompts selbst**, die eine tiefere, mehrstufige Analyse als ein einzelner Prompter-Task voraussetzen), nicht als primärer Code-Autor.

### 2.2 Betriebsmodus: flüchtige VM, eingeschränktes Netzwerk
- **Flüchtige Umgebung:** Jede Session läuft in einer VM ohne Persistenz über die Session hinaus.
- **Eingeschränktes Netzwerk:** Egress ist auf Git-Hosts und Paket-Registries beschränkt, keine freie Web-Recherche möglich.
- **Kein `nix develop`:** `nix` existiert in der Umgebung nicht; `just`-Rezepte laufen über den Cargo-Fallback.

### 2.3 Parallelität mehrerer Jules-Sessions: das Claim-System
**Mechanismus (`cargo xtask claim --crate <CRATE> --issue <TASK_ID> [--dry-run]`):**
- Primärpfad: GitHub-Issue/Label-basiert (`claim:<crate>`-Label über `curl` gegen `api.github.com`).
- Fallback: Lokale `.jules/claims.json` (`ClaimEntry { krate, issue, timestamp, session_id, active }`), nur wirksam ohne `GITHUB_TOKEN`.
- Zweck: Verhindert, dass zwei parallele Agenten denselben Scope gleichzeitig bearbeiten, ohne zentralen Lock-Server.

**Ergänzung v2:** Der Fix-Prompt-Zweig (Abschnitt 6) implementiert eine **funktional äquivalente, aber manuelle** Variante desselben Prinzips: Jede Fix-Prompt-Datei enthält eine explizite Konflikt-Matrix (Datei/Funktion-Überlappung, Sende-Reihenfolge), die dieselbe Kollisionsklasse adressiert wie das Claim-System, aber **ohne** GitHub-API-Reservierung — die Koordination erfolgt stattdessen durch die dokumentierte, vom menschlichen Architekten vorgegebene Sende-Reihenfolge (s. Abschnitt 6.2).

### 2.4 Kontext-Ladeordnung (`.jules/JULES_CONTEXT.md`) und Frischeprüfung
Unverändert: 4-Schritt-Ladeordnung (`.jules/JULES_CONTEXT.md`/`SESSION_BOOTSTRAP.md` → `AGENTS.md` → `WORKING_STATE.md` → `DECISIONS.md`), durchgesetzt durch **Gate 10** (`check-jules-context-freshness`).

**Ergänzung v2 — dokumentierte Doku-Drift trotz Gate 10:** Trotz aktiver Frischeprüfung wurde in `02_PROJEKT_SPEZIFIKATION.md` §0 Punkt 4 eine konkrete Inkonsistenz nachgewiesen: Das Root-`AGENTS.md` (Stand `b448084`) referenziert `crates/memfuse-kv-bridge/` als eigenständiges Crate, obwohl dieser Code-Bestandteil bereits seit mehreren Commits in `crates/memfuse-crypto/src/kv_segment/` konsolidiert ist; `WORKING_STATE.md` (vollständig autogeneriert!) listet dasselbe Crate unter dem abweichenden Namen `memfuse-security`. Dies zeigt, dass Gate 10 die *Existenz* einer frischen `AGENTS.md`-Ladeordnung prüft, nicht aber die *inhaltliche Korrektheit* jeder einzelnen Crate-Referenz darin — eine Lücke, die im Prinzip durch das ohnehin vorhandene Gate 15 (`check-doc-references`, s. Abschnitt 3.2) geschlossen werden könnte, sofern dessen Prüftiefe Crate-Pfad-Referenzen in Fließtext einschließt (nicht abschließend verifizierbar ohne Einsicht in `xtask/src/check_doc_references.rs`'s exakte Regex-Logik).

---

## 3. Der MemFuse-Prompter (v25): Aufbau und Funktionsweise

*(inhaltlich unverändert gegenüber v1; alle Kernaussagen — Engine-vs-Daten-Trennung, `gen-prompter-data`-Datenfluss, Drift-Resistenz seit v23, 14 Task-Typen in 4 Kategorien, APM-Katalog mit 42 Patterns/13 Kategorien, ROLE-LOCK, PLAN→ACT→VERIFY→REFLECT-Loop, GitHub-Kontext-Injektion, Inventar-Frische-UI, Nachbereitungspflichten — wurden gegen HEAD `33226e2` erneut geprüft und bestätigt gefunden. Für den vollständigen Text siehe v1 §2; hier nur die Delta-relevanten Ergänzungen.)*

### 3.1 Verhältnis zum neuen Fix-Prompt-Zweig (Abschnitt 6)
Der Prompter bleibt der Erzeugungsweg für **tägliche, crate-gebundene Standardaufgaben** (`AUDIT`, `DEEP`, `IMPL`, `FIX`, `TEST`, `SEC`, `REVIEW`, `ADR`, `DEPS`, `FIX-GOV`, `CALIB`, `FLAKY`, `CHAOS`, `REPLAY`). Die isolierten Fix-Prompts (Abschnitt 6) sind kein Ersatz dafür, sondern ein **ergänzender, spezialisierter Pfad für mehrstufige, quer über mehrere Analyserunden verfolgte Härtungskampagnen**, bei denen die Konflikt-/Reihenfolge-Planung über mehrere Prompts hinweg wichtiger ist als die Wiederverwendbarkeit einer generischen Task-Vorlage. Beide Pfade münden in denselben CI-Gate-Stack (Abschnitt 4) und denselben Governance-Doku-Zyklus (Abschnitt 5).

---

## 4. CI-Gates: die automatisierte Durchsetzungsschicht

### 4.1 Workflow-Übersicht (`.github/workflows/`) — aktualisiert
| Workflow | Auslöser | Zweck |
|---|---|---|
| `context-gates.yml` | `push`, `pull_request` | **15** Governance-/Struktur-Gates (Detail unten) |
| `rust-ci.yml` | vermutlich `push`/`pull_request` | `cargo nextest run --locked --workspace --exclude memfuse-tauri --retries 2` |
| `bench.yml` | vermutlich zeitgesteuert/PR | LongMemEval-/LoCoMo-Benchmarks, „Retrieval Quality Regression Gate" |
| `chaos.yml` | vermutlich zeitgesteuert | Chaos-Engineering-Läufe (Disk-Full, Kill-9, etc.) |
| `scheduled-audit.yml` | zeitgesteuert (cron) | Automatisierter Anstoß periodischer Audit-Sessions |
| `prune-branches.yml` | vermutlich zeitgesteuert | Branch-Proliferation-Reduktion |
| `publish-pypi.yml` | vermutlich Release-Tag | Veröffentlichung von `memfuse-py` |
| `tauri-release.yml` | vermutlich Release-Tag | **Noch aktiv trotz Deprecation** — physischer Rückbau von ADR-077 weiterhin nicht abgeschlossen (bestätigt, `crates/memfuse-tauri` unverändert vorhanden) |
| **`nucleation-recall-history.yml`** *(neu seit v1)* | vermutlich täglich (cron) | 30-Tage-Recall-Stabilitätsmessung für `F-02`-Tombstone-Pruning (`VETO-F02`); schreibt nach `benchmarks/results/nucleation_recall_history.jsonl`, geprüft via `cargo xtask check-recall-stability`; Voraussetzung für die Freigabe des `physio-nucleation`-Feature-Flags nach dem `2026-10-07`-Review-Datum |

### 4.2 `context-gates.yml` — die 15 Gates im Detail (aktualisiert: +1 gegenüber v1)
| Gate | Prüfung | Mechanismus |
|---|---|---|
| Vorlauf | Duplicate-Symbol-Check | `xtask check-duplicate-symbols` |
| — | DAG-Integrität | `xtask check-dag` |
| — | `AGENTS.md`-Crate-Integrität | `xtask check-agents-integrity` |
| **Gate 12** | Duplicate-PR-Intent-Detection | `xtask check-duplicate-intent`, liest `MEMFUSE_PR_BODY` |
| **Gate 1** | Keine ungelösten `AI-TAG[...][CRITICAL\|BLOCKER]` | `grep`-basiert, ignoriert `RESOLVED`-Suffix |
| **Gate 2** | Keine neuen `.unwrap()`/`.expect()` | `xtask check-unwrap-baseline` gegen `.unwrap-baseline.json` |
| **Gate 3** | Kein `let _ =` bei IO (sync/flush/write) | `grep`-basiert |
| **Gate 4** | Kein `axum` in `memfuse-mcp` | `grep` gegen `Cargo.toml`, erzwingt ADR-010 |
| **Gate 6** | Keine `TODO` ohne AI-TAG-Grammatik | `grep`-basiert |
| **Gate 7** | ISO-8601-Tag-Validierung | `xtask validate-tags` |
| **Gate 5** | Doku-Drift-Check | `xtask sync-docs`, dann `git diff` gegen `WORKING_STATE.md`, `docs/ARCHITECTURE.md`, `docs/SOURCE_OF_TRUTH.md` |
| **Gate 8** | Mehrfach-Session-Review-Abdeckung | `xtask check-review-coverage` |
| **Gate 9** | Doku-Konsistenz | `xtask check-consistency` |
| **Gate 13** | Keine Platzhalter-Referenzen in Governance-Dokumenten | `xtask check-placeholder-refs` |
| **Gate 10** | Jules-Context-Frische | `xtask check-jules-context-freshness` |
| **Gate 11** | `AGENTS.md`-Template-Validierung | Bash-`grep`, erzwingt 8 Pflichtsektionen |
| **Gate 15** *(neu seit v1)* | Validierung von Dateipfad-Referenzen in Dokumenten | `xtask check-doc-references` — prüft vermutlich, dass in Governance-/Analyse-Dokumenten referenzierte Dateipfade (`crates/<crate>/src/<file>.rs` u. ä.) tatsächlich im Repo existieren; **relevant für die in Abschnitt 2.4 dokumentierte `memfuse-kv-bridge`-Drift**, sofern das Gate Prosa-Referenzen (nicht nur strukturierte Tabellen) erfasst |
| **Gate 14** | Jules-Preflight-Aggregator | `xtask jules-preflight --fast`, `MEMFUSE_CI=true` |

**Bemerkenswertes Strukturmerkmal (bestätigt, unverändert):** Die Nummerierung folgt weiterhin nicht der Ausführungsreihenfolge im YAML — Gate 15 wurde am Ende der Datei (nach Gate 11, vor Gate 14) eingefügt, was das bereits in v1 beschriebene Muster inkrementellen, nicht neu durchnummerierten Wachstums der Gate-Liste bestätigt.

### 4.3 xtask als gemeinsame Laufzeit-Basis — vollständige Kommando-Inventur (neu in v2)
Alle Gates (außer reinen `grep`-Inline-Schritten) laufen über dasselbe `xtask`-Binary. Die vollständige Liste der Subkommando-Implementierungen in `xtask/src/` (18 Dateien, gegenüber v1, das nur vier beispielhaft nannte):

| Datei | Vermutete Funktion (aus Dateiname/Gate-Zuordnung) |
|---|---|
| `bench_gate.rs` | Benchmark-/Retrieval-Quality-Regression-Gate-Logik (`bench.yml`) |
| `check_agents_integrity.rs` | `AGENTS.md`-Crate-Integritätsprüfung |
| `check_commit_messages.rs` | Commit-Message-Formatvalidierung *(neu seit v1 identifiziert)* |
| `check_doc_references.rs` | **Gate 15**, Dateipfad-Referenz-Validierung |
| `check_duplicate_intent.rs` | **Gate 12**, semantische PR-Überlappungserkennung |
| `check_duplicate_symbols.rs` | Vorlauf-Gate, Symbol-Duplikat-Erkennung |
| `check_jules_context_freshness.rs` | **Gate 10** |
| `check_placeholder_refs.rs` | **Gate 13** |
| `check_recall_stability.rs` | 30-Tage-Recall-Stabilität für `VETO-F02` *(neu seit v1)* |
| `check_type_registry.rs` | Vermutlich Typ-Registrierungs-/Namenskonventionsprüfung *(neu seit v1 identifiziert)* |
| `check_vetoes.rs` | Prüfung gegen `VETOES.md`-Register (Keyword-Trigger, Frist-Überwachung 14 Tage vor `review_date`) |
| `claim.rs` | Claim-System (Abschnitt 2.3) |
| `gen_prompter_data.rs` | Prompter-Datenbrücke (Abschnitt 3) |
| `generate_adr.rs` | ADR-Boilerplate-Generierung für `DECISIONS.md` *(neu seit v1 identifiziert)* |
| `init_audit_fix.rs` | Vermutlich Bootstrap für `AUDIT`/`FIX`-Task-Typen (Anlegen von `docs/audits/`-Grundgerüst) *(neu seit v1 identifiziert)* |
| `jules_preflight.rs` | **Gate 14** |
| `main.rs` | CLI-Entry-Point, Subcommand-Dispatch |
| `validate_pr_checklist.rs` | Vermutlich Prüfung der in Abschnitt 3, Punkt „Nachbereitungspflichten" beschriebenen PR-Body-Anforderungen (`Fixes #<N>`, APM-Bezug, Audit-Referenz) *(neu seit v1 identifiziert)* |

**Konsequenz:** Jede Governance-Regel ist genau einmal in Rust implementiert und wird von CI, Prompter und lokalem Entwickler-Workflow identisch ausgeführt. Die in v1 dokumentierte Beobachtung „kein YAML-vs-Code-Duplikationsrisiko für die Prüflogik selbst, wohl aber für die Gate-*Reihenfolge*" bleibt mit Gate 15 als weiterem Beleg bestätigt.

---

## 5. GitHub-Verlauf: Was die Historie über das System selbst aussagt

*(Kernaussagen aus v1 — Phasenverlauf 22.08.–07.09.2026, Governance als eigene Entwicklungslinie ab Phase 5, durchgängige `google-labs-jules[bot]`-Autorenschaft — bleiben unverändert gültig. Ergänzung:)*

### 5.1 Fortsetzung der Historie bis HEAD `33226e2`
Die zehn PRs `#1876`–`#1895` (s. `git log --oneline`, direkt vor und einschließlich der in Abschnitt 6/7 behandelten Fix-Prompt-Commits) setzen das in v1 beschriebene Muster fort: thematisch konsistente, isolierte Änderungen mit Audit-Log-Updates (`memfuse-checkpoint: expand unit test suite and update audit log`, `memfuse-text: deep audit, anti-mirroring tests`), was belegt, dass der Prompter- und der Fix-Prompt-Zweig **im selben Commit-Strom** landen und denselben Nachbereitungskonventionen (Audit-Log-Referenz, s. Abschnitt 3) folgen, obwohl sie unterschiedliche Ursprungs-Prompt-Mechanismen haben.

---

## 6. Der Fix-Prompt-Zweig: Isolierte, analysegestützte Härtungs-Prompts (neu in v2)

### 6.1 Zweck und Abgrenzung zum Prompter
Neben dem Prompter (Abschnitt 3) existiert ein zweiter, in v1 nicht dokumentierter Mechanismus zur Prompt-Erzeugung: **isolierte Fix-Prompts**, die aus vertieften, mehrstufigen Stabilisierungsanalysen des Gesamtsystems abgeleitet werden (Quelldokumente außerhalb des Repos: `MemFuse_Vertiefte_Stabilisierungsanalyse_2026-09-09.md`, `MemFuse_Stabilisierung_Folgerunde_2026-09-09.md`, `MemFuse_Stabilisierungsanalyse_Runde3_2026-09-09.md`). Diese Prompts werden **nicht** über `gen-prompter-data`/den Prompter generiert, sondern von Hand (bzw. durch eine separate LLM-Analysesession) verfasst, gegen einen exakt referenzierten Commit-Hash verifiziert, und **direkt und unverändert** an Google Jules gesendet.

**Charakteristische Eigenschaften gegenüber Prompter-generierten Task-Prompts:**
1. **Punktgenaue Code-Verifikation:** Jeder Prompt zitiert exakte Zeilennummern, Funktionssignaturen und Code-Ausschnitte, verifiziert gegen einen konkreten Commit-Hash zum Erstellungszeitpunkt (z. B. „main @ `149e827c`, 09.09.2026, 19:30 Uhr" für N1–N5).
2. **Explizite Merge-Konflikt-Choreografie:** Jede Prompt-Sammlung beginnt mit einer Tabelle (Datei(en), berührte Funktion(en), Konfliktrisiko mit anderen Prompts derselben und vorheriger Runden) und einer daraus abgeleiteten empfohlenen Sende-Reihenfolge (parallel-sicher vs. sequenziell-notwendig).
3. **Rollenbasierte Prompt-Struktur** (festes Template, s. 6.3).
4. **Cross-Runden-Abhängigkeiten:** Spätere Runden (Folgerunde: B/E1/E2/D1/D2; Runde 3: F/G/H) verifizieren explizit gegen den *tatsächlich gemergten* Stand vorheriger Runden, nicht gegen deren *ursprüngliche Prompt-Spezifikation* — dokumentiert am Beispiel Prompt F, das explizit vermerkt: „Prompt B wurde real anders implementiert (stiller Cap auf `MAX_SCAN_RESULTS` statt eines `LimitExceeded`-Fehlers), und F baut exakt auf diesem tatsächlichen Stand auf" (s. Abschnitt 7).

### 6.2 Drei Analyserunden im Überblick

| Runde | Datei | Verifizierter `main`-Stand bei Erstellung | Prompts | Voraussetzung |
|---|---|---|---|---|
| 1 | `MemFuse_Jules_Fix_Prompts_N1-N5.md` | `149e827c` (09.09.2026, 19:30 Uhr) | N1, N2, N3, N4, N5 | — (Basisrunde) |
| 2 („Folgerunde") | `MemFuse_Jules_Fix_Prompts_Folgerunde.md` | `b5a68b97` (09.09.2026, 19:42 Uhr) | B, E1, E2, D1, D2 | N1–N5 zum Sendezeitpunkt bereits in Bearbeitung/gemergt |
| 3 („Runde 3") | `MemFuse_Jules_Fix_Prompts_Runde3_F-G-H.md` | `f29c3987` (09.09.2026, 19:55 Uhr) | F, G, H | N1–N5 und B/E1/E2/D1/D2 bereits in Bearbeitung/teilweise gemergt; **F zusätzlich hart abhängig vom tatsächlich gemergten B-Ergebnis** |

**Intra-Runden-Konfliktmatrix (Zusammenfassung, Details in den jeweiligen Dateien):**

| Prompt | Datei(en) | Konfliktrisiko | Empfohlene Reihenfolge |
|---|---|---|---|
| N1, N2, N4 | `fusion.rs`, `wal.rs`, neue Testdatei | Keines untereinander | Sofort parallel |
| N3, N5 | `lsm.rs` (`commit()`) | Gering–hoch (überlappende Funktion) | N3 zuerst, N5 danach (oder parallel mit manuell auflösbarem Konflikt) |
| B, D1, D2 | eigene Crates/Dateien | Keines untereinander | Sofort parallel |
| E1, E2 | `kv_segment/store.rs` (`evict_lru_fair()`) | Hoch (gleiche Funktion) | E1 zwingend vor E2 |
| G | `diskann.rs` | Gering mit D2 (unterschiedliche Zeilenbereiche) | Optional sequenziell nach D2 |
| F | `traits/mod.rs`, `lsm.rs`, `crud.rs` | Muss nach B gemergt werden | Nach B-Merge senden |
| H | `fusion.rs` | Muss nach N1 gemergt werden | Nach N1-Merge senden |

### 6.3 Das Fix-Prompt-Template (verbindliche Struktur)
Jeder isolierte Fix-Prompt folgt exakt derselben sieben-teiligen Struktur — dies ist ein de-facto-Standard für diese Prompt-Klasse, unabhängig vom Prompter-eigenen Template (Abschnitt 3):

1. **ROLLE:** Definiert eine spezifische Senior-Engineer-Persona mit exakt den für die Aufgabe relevanten Fachgebieten (z. B. „Senior Rust Engineer mit tiefgehender Expertise in numerischer Robustheit, IEEE-754-Semantik und Information-Retrieval-Systemen" für N1).
2. **KOMPETENZEN, DIE DU ANWENDEST** *(optional, in ca. der Hälfte der Prompts explizit ausformuliert)*: Stichpunktartige Liste konkreter Techniken/Patterns.
3. **KONTEXT:** Repository, Branch, exakte Datei(en) mit LOC-Angabe, exakte Funktionssignatur(en) mit Suchstring zur Lokalisierung.
4. **PROBLEM:** Wörtlich zitierter, gegen den Commit verifizierter Ist-Code-Ausschnitt, gefolgt von einer präzisen technischen Analyse der Fehlerklasse (oft mit Analogie zu einem bereits im selben Repository etablierten korrekten Muster, z. B. „vergleiche `hnsw.rs`, das bereits `is_nan() || is_infinite()` prüft").
5. **AUFGABE — exakte Implementierungsschritte:** Nummerierte, oft mit vollständigem Ziel-Code-Diff versehene Schritte, inklusive Bedingungsverzweigungen für den Fall abweichender Vorbefunde (z. B. „falls ein externer Aufrufer gefunden wird, nimm KEINE Sichtbarkeitsänderung vor, sondern..."). Schließt in der Regel die Pflicht zur Erstellung mindestens eines benannten Regressionstests ein.
6. **NICHT-ZIELE (explizit NICHT ändern):** Harte Scope-Grenze — verhindert das in `03_ENTWICKLUNGSSYSTEM_SPEZIFIKATION.md` v1 beschriebene „Mitkorrigieren außerhalb des beabsichtigten Scopes".
7. **AKZEPTANZKRITERIEN:** Prüfbare, meist mit `cargo build`/`cargo test`-Bezug formulierte Abschlussbedingungen.

**Sicherheitsventil-Muster (wiederkehrend über mehrere Prompts):** Mehrere Prompts (N4, N5, D1) enthalten eine explizite Eskalationsanweisung für den Fall, dass die Implementierung während der Bearbeitung einen *bereits bestehenden, aktiven* Fehler aufdeckt, der aber außerhalb des Prompt-Scopes liegt: „Ändere AUF KEINEN FALL selbstständig Produktionscode [...], dokumentiere den Befund [...], damit dies als separater, dringlicherer Fix-Prompt behandelt werden kann." Dies ist funktional identisch zum ROLE-LOCK-Prinzip des Prompters (Audit ≠ Fix, Abschnitt 3), hier aber als Inline-Klausel statt als Tool-Feature realisiert.

### 6.4 Fehlende Automatisierung: Rückkanal in die Governance-Doku
Im Gegensatz zum Prompter-Zweig (`gen-prompter-data` liest denselben Live-Workspace, den auch die Gates prüfen) existiert für den Fix-Prompt-Zweig **kein** äquivalentes Tooling, das die Analyseergebnisse einer Runde automatisch in `AGENTS.md`/`WORKING_STATE.md`/`VETOES.md` zurückspiegelt. Der Statusabgleich erfolgt stattdessen — wie in `02_PROJEKT_SPEZIFIKATION.md` §7 demonstriert — durch **manuelle Live-Verifikation** gegen einen frischen Checkout zu Beginn jeder neuen Analyserunde. Dies ist funktional robust (jede Runde verifiziert unabhängig gegen die Wahrheit im Code, nicht gegen Annahmen aus Sekundärdokumenten), aber **nicht** durch CI erzwungen — es gibt kein Gate, das prüft, ob eine in einer Fix-Prompt-Datei als "empfohlen" markierte Sende-Reihenfolge tatsächlich eingehalten wurde, oder ob ein als "offen" dokumentierter Befund inzwischen anderweitig behoben wurde.

---

## 7. Bestätigte Lücke im Regelkreis: Prompt-Spezifikation vs. tatsächliche Implementierung

Bereits in v1 als offene Bruchstelle benannt (ADR-Deprecation-Fristen ohne CI-Überwachung) und in v2 um einen zweiten, konkreten Beleg erweitert:

**Fall Prompt B → Prompt F:** Prompt B (Folgerunde) spezifizierte für `scan()`/`scan_prefix()` explizit einen harten Fehlerfall bei Limit-Überschreitung (`MemFuseError::LimitExceeded`, kein stilles Abschneiden — s. AKZEPTANZKRITERIEN in der Originaldatei: „Kein stilles Abschneiden des Ergebnisses (Truncation ohne Fehler)"). Die tatsächliche, von Google Jules produzierte Implementierung (verifiziert gegen HEAD `33226e2`, Commit `f29c398`) realisiert stattdessen einen **stillen Cap** auf `MAX_SCAN_RESULTS = 10_000` ohne Fehlerrückgabe bei implizitem Limit. Prompt F (Runde 3) musste diese Abweichung explizit vermerken und seine eigene Spezifikation darauf aufbauen, statt auf der ursprünglichen B-Spezifikation.

**Bewertung:** Dies zeigt, dass selbst ein sehr detailliertes AKZEPTANZKRITERIEN-Feld im Fix-Prompt-Template (Abschnitt 6.3) keine Garantie für spezifikationstreue Umsetzung durch den Agenten ist — die nachgelagerten CI-Gates prüfen *Struktur*-Eigenschaften (Duplicate Symbols, DAG, Unwrap-Baseline, Doku-Sync, etc.), nicht aber *funktionale Äquivalenz* zwischen Prompt-AKZEPTANZKRITERIEN und tatsächlichem Verhalten. Der einzige faktisch wirksame Korrekturmechanismus ist die **nächste manuelle Analyserunde**, die die Abweichung entdeckt und dokumentiert — kein automatisiertes Gate schließt diese Lücke. Diese Beobachtung ergänzt die in v1 §5 benannte offene Bruchstelle (zeitbasierte Architektur-Entscheidungen ohne Gate-Überwachung) um eine zweite Kategorie: **inhaltliche Spezifikations-Konformität ohne Gate-Überwachung.**

---

## 8. Zusammenspiel: geschlossener Regelkreis, verifiziert (aktualisiert)

| Verbindung | Nachweis im Repo | Status v2 |
|---|---|---|
| Prompter → Jules | Prompt-Text zwingt Ladeordnung, Claim-Aufruf, `FILE-CONTEXT`-Pflicht | Unverändert bestätigt |
| Fix-Prompts → Jules *(neu)* | Sieben-teiliges Template (Abschnitt 6.3), manuelle Konflikt-Choreografie statt Claim-System | Neu dokumentiert, funktional aber nicht CI-integriert |
| Jules → CI-Gates | Jeder Jules-Commit muss `context-gates.yml` (**15** Gates) + `rust-ci.yml` grün durchlaufen | Gate-Zahl aktualisiert |
| CI-Gates → Governance-Doku | Gate 5 (`sync-docs`) erzwingt Aktualität von `WORKING_STATE.md`/`ARCHITECTURE.md`/`SOURCE_OF_TRUTH.md` | Unverändert; **erfasst nachweislich nicht** Prosa-Referenzen in `AGENTS.md` (s. Abschnitt 2.4) |
| Governance-Doku → Prompter | `gen-prompter-data` liest denselben Live-Workspace, den auch die Gates prüfen | Unverändert bestätigt |
| Governance-Doku → Fix-Prompt-Autor *(neu)* | Kein automatisierter Kanal — manuelle Live-Verifikation pro Analyserunde | Neu dokumentiert als offene Lücke (Abschnitt 6.4) |
| GitHub-Historie → alle | `docs/GITHUB_HISTORY.md` dokumentiert rückblickend jeden Teil dieses Kreises | Fortgeschrieben bis PR `#1895` |

**Offene Bruchstellen im Kreis (v1 bestätigt + v2 neu):**
1. *(v1, weiterhin offen)* ADR-Deprecation-Fristen (z. B. `memfuse-tauri`-Entfernung, Ziel ≈ 07.11.2026) werden von keinem der 15 Gates überwacht — nur Veto-Fristen (`check-vetoes`).
2. *(v2, neu)* Inhaltliche Spezifikations-Konformität zwischen einem Fix-Prompt-AKZEPTANZKRITERIEN-Katalog und der tatsächlichen Jules-Implementierung wird von keinem Gate automatisiert geprüft — nur durch die nächste manuelle Analyserunde retrospektiv entdeckt (Abschnitt 7).
3. *(v2, neu)* Der Fix-Prompt-Zweig hat keinen zu `gen-prompter-data` äquivalenten Rückkanal in die Governance-Doku (Abschnitt 6.4).

---

## Anhang: Bezugsquellen dieser Analyse (v2)
- `.github/workflows/context-gates.yml`, `rust-ci.yml`, `nucleation-recall-history.yml` (Gate-/Workflow-Definitionen, Live-Repo, HEAD `33226e2`)
- `xtask/src/*.rs` (18 Subkommando-Dateien, vollständige Inventur Abschnitt 4.3)
- `.jules/JULES_CONTEXT.md`, `.jules/claims.json`, `.jules/prompter-data.json`, `.jules/prompter-tiers.toml`
- `VETOES.md` (Veto-Register inkl. `VETO-F02`-Recall-Stabilitäts-Verweis)
- `DECISIONS.md` (ADR-Reihe bis `ADR-078`)
- `docs/GITHUB_HISTORY.md` (Phasen 1–7, fortgeschrieben)
- `Memfuse-Prompter-v24.html` (Prompter-UI/-Logik, im Tool selbst als v25 bezeichnet)
- `MemFuse_Jules_Fix_Prompts_N1-N5.md`, `MemFuse_Jules_Fix_Prompts_Folgerunde.md`, `MemFuse_Jules_Fix_Prompts_Runde3_F-G-H.md` (Fix-Prompt-Zweig, Abschnitt 6/7)
- `git log` HEAD `33226e2` (frischer Clone dieser Session, 09.09.2026), plus direkte Grep-Verifikation der in Abschnitt 6/7 und in `02_PROJEKT_SPEZIFIKATION.md` §7 genannten Code-Zustände (`evict_lru_fair`, `LayerCleanupProof`, `scan_bounded`, `HeapEntry::cmp`, `is_finite()`-Guards in `diskann.rs`)
