# MemFuse — Zielarchitektur & Umsetzungsplan v9.0
## „Governance vor Struktur — warum die Reihenfolge selbst die wichtigste Entscheidung ist"

> **Dokument-Typ:** Ziel-/Soll-Architektur + verbindliche Entscheidungen + Umsetzungsreihenfolge. Ersetzt v8.0 vollständig (nicht additiv) und integriert vier seither entstandene Analysedokumente: den Governance-Audit, die GitHub-Verlaufsanalyse, sowie zwei Entscheidungsdokumente (v1 und den Live-Abgleich v2), die zwischen v8.0 und jetzt bereits Entscheidungen getroffen haben. Diese Spezifikation führt alles zu einem einzigen, konsistenten Referenzdokument zusammen.
> **Live-Abgleich für dieses Dokument:** frischer `git clone`, HEAD `6fbb3ede1` (2026-09-08, 16:21 Uhr), 1.328 Commits gesamt. Gemäß P16 (unten) ist dieser Zeitstempel bewusst nur im Kopf vermerkt, nicht in den Fließtext eingewoben — das Dokument bleibt auch nach dem nächsten Commit-Schub gültig, weil es Strukturentscheidungen trifft, keine Codezeilen zitiert.
> **Wichtigste Erkenntnis gegenüber v8.0:** v8.0 ging implizit davon aus, dass die 195 Jules-Tasks/Tag unter funktionierender Koordination laufen. Der Governance-Audit und die GitHub-Analyse zeigen mit konkreten Belegen (dreifache `ConfigFingerprint`-Implementierung, 4× ADR-070, sechs unverdrahtete xtask-Module, ein `AGENTS.md`, das nicht garantiert geladen wird), dass diese Annahme falsch war. **Konsequenz: Governance-Reparatur rückt vor die Struktur-Migration — nicht parallel dazu, sondern davor**, weil die Struktur-Migration aus v8.0 §4 exakt dieselbe Kollisionsanfälligkeit hätte wie die bereits dokumentierten Fälle, nur mit höherem Schaden pro Kollision (Modul- statt Funktionsebene).

---

## Änderungsprotokoll gegenüber v8.0

| Was | v8.0 | v9.0 |
|---|---|---|
| Reihenfolge | Struktur-Migration (§8) mit Governance nur am Rande erwähnt | Governance-Fundament (Phase A) und Claim-Mechanismus (Phase B) **zwingende Voraussetzung** vor jeder Struktur-Migration (Phase C) |
| Produktvision (§6) | Drei Optionen offen gelassen, Entscheidung an "Anhang D" delegiert | **Entschieden:** Option 1 (PyPI-Library, Position A). Mit Revisionsklausel, nicht endgültig unveränderlich |
| Prompter-Tooling (§7) | Vorschlag: Crate-Manifest-Export einführen | **Bereits umgesetzt** im Repo (`gen_prompter_data.rs`, `.jules/Memfuse-Prompter-v24.html`, `.jules/prompter-tiers.toml`) — dieses Dokument beschreibt jetzt den nächsten Ausbauschritt (Claim-Integration), nicht mehr die Grundfunktion |
| Governance-Infrastruktur | Nicht behandelt | Neuer Abschnitt §3 (vormals nicht vorhanden): xtask-Reparatur, ADR-Konsolidierung, Claim-/Lock-Mechanismus, Ambient-Kontext-Problem |
| Physio-Feature-Klassifikation (§5/D.2) | Vier Klassen (Kern/Differenzierend/Experimentell/Entfernen) | Reduziert auf zwei Klassen (Kern/Entfernen), da Vision-Entscheidung Multi-Tenant-abhängige Features eindeutig aussortiert |
| memfuse-tauri | Bedingt auf §6-Entscheidung | **Entfernung beschlossen**, 60-Tage-Frist läuft |

---

## Inhaltsverzeichnis

- **§0** Warum jetzt entscheiden statt weiter analysieren (Muster hinter allen vier Ausgangsdokumenten)
- **§1** Verbindliche Entscheidung: Produktvision
- **§2** Architekturprinzipien P1–P19 (P13–P16 aus v8.0 + neue P17–P19 aus dem Governance-Audit)
- **§3** Governance-Infrastruktur: Ist-Zustand, Zielzustand, Einzelmaßnahmen
- **§4** Ziel-Crate-Topologie (unverändert gültig aus v8.0 §3, hier referenziert)
- **§5** Modul-Migration (unverändert gültig aus v8.0 §4, hier referenziert + Ergänzungen)
- **§6** Feature-Konsolidierung (reduziert auf zwei Klassen)
- **§7** Prompter-Tooling: von "vorgeschlagen" zu "vorhanden" — nächste Ausbaustufe
- **§8** GitHub-Prozess-Hygiene (neu, aus der Verlaufsanalyse)
- **§9** Konsolidierte Umsetzungsreihenfolge (Governance + Struktur zusammengeführt)
- **§10** Laufende Session: Bewertung und Korrekturauftrag (Live-Fall Phase 0)
- **§11** Erfolgsmessung
- Anhang A: Crate-Mapping (unverändert aus v8.0)
- Anhang B: Vollständige Befundliste des Governance-Audits mit Live-Bestätigung
- Anhang C: Was du als Mensch als Nächstes tun musst
- Anhang D: Warum dieses Dokument keine Live-Code-Zeilen zitiert (P16, unverändert aus v8.0)

---

## §0 Warum jetzt entscheiden statt weiter analysieren

Vier unabhängige Analysen — die ursprüngliche Zielarchitektur (v8.0), der Governance-Audit, die GitHub-Verlaufsanalyse und zwei Entscheidungsdokumente — zeigen **dasselbe Muster auf drei verschiedenen Ebenen**:

- **Funktionsebene:** `ConfigFingerprint` wurde dreimal unabhängig implementiert, in drei verschiedenen Architekturvarianten, innerhalb von 66 Minuten (GitHub-Analyse §2.1) — mit einem konkreten, 5 Stunden lang unbemerkten Kompilierfehler als Folge.
- **Dokumentebene:** ADR-070 existiert vierfach, ADR-072 zweifach, ein Auflösungsbeschluss (ADR-060) wurde gefasst und seither zwölfmal ignoriert (Governance-Audit §2).
- **Architekturebene:** Drei Produktvisionen (PyPI-Library, Desktop-Enterprise-App, Voice-Assistent) galten laut ADR-018 gleichzeitig als „final", ohne dass eine die andere ablöste (v8.0 §6) — dieselbe Nicht-Entscheidung wie bei den ADR-Kollisionen, nur eine Abstraktionsebene höher.

**Die gemeinsame Ursache ist immer dieselbe: fehlende Entscheidung wird durch Kapazität kompensiert.** Bei ~195 Tasks/Tag löst *irgendein* Agent *irgendeine* offene Frage — nur eben mehrfach, unterschiedlich, und ohne dass die Lösungen voneinander wissen. Das ist kein Agentenfehler im engeren Sinn (jede einzelne Session hat vermutlich sinnvoll gehandelt, gegeben ihren Kontext) — es ist ein fehlendes Koordinationssystem, das diese Sessions gegeneinander absichert.

**Konsequenz für dieses Dokument:** Jede offene Frage aus v8.0 (dort als "Anhang D — offene Entscheidungen" formuliert) wird hier entschieden, nicht als Optionsliste weitergereicht. Wo echte Unsicherheit besteht, wird das benannt, aber trotzdem entschieden — mit Revisionsklausel statt Nicht-Entscheidung, denn eine Nicht-Entscheidung ist genau das, was zum aktuellen Zustand geführt hat.

---

## §1 Verbindliche Entscheidung: Produktvision

**Entscheidung: Option 1 — PyPI-Library (ADR-007-Richtung), Position A aus v8.0 §1 ("schlanker als MinnsDB, Krypto-Härtung als Alleinstellungsmerkmal").**

**Begründung:**
- Der faktische Entwicklungsmodus — ein Solo-Entwickler, der einen Agenten-Schwarm orchestriert, keine Vertriebs-/Support-Organisation — passt strukturell nicht zu einer Desktop-Enterprise-App mit Multi-Tenant-Anspruch. Diese Option braucht Vertrauen, Support-SLAs und Referenzkunden, die ein Ein-Personen-Projekt nicht glaubwürdig liefern kann, unabhängig von Codequalität.
- Position A ist in einem Satz kommunizierbar und in PyPI-Release-Zyklus-Größenordnung lieferbar. Die Enterprise-Option erfordert Vertriebszyklen, die dieses Setup nicht bedienen kann.
- Der stärkste, bereits vorhandene differenzierende Code-Bestandteil (HMAC-WAL-Kette, kryptografischer Löschbeweis) ist als Compliance-Feature innerhalb einer Library genauso vermarktbar wie innerhalb einer Enterprise-App — der Vorteil geht durch diese Entscheidung nicht verloren.
- Die DiskANN-Out-of-Core-Fähigkeit (Kern der verworfenen Position B) bleibt als **spätere Erweiterung innerhalb der Library** möglich — großer Korpus auf Disk ist ein Library-Feature, kein App-Feature. Keine verlorene Option, nur eine Sequenzierung.

**Sofortige, verbindliche Konsequenzen (P15, 60-Tage-Frist ab Merge dieser Entscheidung ins Repo als ADR):**
1. `memfuse-py` wird primäres, best-gepflegtes Grenzschicht-Crate.
2. `memfuse-tauri` wird als **superseded** in einer neuen ADR markiert (referenziert ADR-018) und **physisch entfernt** — hartes Kalenderdatum in einem Tracking-Issue, nicht nur „60 Tage" als Text (siehe Anhang C).
3. Multi-Tenant-Konzepte in `memfuse-security::kv_segment` (ehem. `memfuse-kv-bridge`, siehe v8.0 §4.1) werden auf Test-Isolation reduziert; Enterprise-Marketing-Sprache aus allen Docs entfernt.
4. **Option 3 (Voice/Jarvis):** formales VETO mit `conditional_review_due` = 6 Monate ab heute, analog zum bestehenden VETO-F02-Muster in `VETOES.md` — kein stilles Fallenlassen, sondern ein expliziter Wiedervorlage-Zeitpunkt.
5. Da `memfuse-tauri` entfernt wird, erledigt sich die in v8.0 Anhang D.4 offene Frage nach einem gemeinsamen `memfuse-bindings`-Crate: `memfuse-py` bleibt eigenständig, ein Feature-Flag-Konstrukt für ein bereits totes Ziel wäre unnötige Komplexität.

**Revisionsklausel:** Wenn nach Phase 0 (LongMemEval/LoCoMo-CI, siehe §10) der direkte Benchmark-Vergleich gegen MinnsDB zeigt, dass Out-of-Core-Fähigkeit der einzige signifikante Vorteil ist und Position A keinen messbaren Unterschied bei typischer Korpusgröße bietet, wird diese Entscheidung **explizit neu bewertet** — nicht stillschweigend unterlaufen. „Explizit neu bewerten" bedeutet: neue ADR, die diese hier referenziert und begründet widerlegt, nicht ein Wiederauftauchen von Tauri-Code in einem unauffälligen Feature-Branch.

---

## §2 Architekturprinzipien P1–P19

P1–P12 (DAG-Integrität, Zero-Panic, WAL-First, Backend-Agnostizismus, etc.) und P13–P16 (Modulgrenzen nach Verantwortung, ein Scheduler pro Subsystem, eine Vision pro Release, Dokumente als Ziele statt Zustandsprotokolle) bleiben aus v8.0 unverändert gültig. Der Governance-Audit und die GitHub-Analyse begründen drei weitere, ebenso verbindliche Prinzipien:

**P17 — Ambient-Kontext ist eine Annahme, keine Garantie, und muss explizit erzwungen werden.** Der Governance-Audit weist mit widersprüchlichen Log-Einträgen (`.jules/JULES_LOG.md` vs. `.jules/JULES_LOG_2.md`) nach, dass nicht garantiert ist, dass eine Jules-Session `AGENTS.md` automatisch lädt. Jedes Governance-Dokument, das sich auf „wird automatisch gelesen" verlässt, ist ein Single-Point-of-Failure. Ab sofort gilt: Jeder Session-Trigger-Prompt (Prompter-Tool) enthält einen expliziten Pflicht-Präfix, der `AGENTS.md` und `.jules/SESSION_BOOTSTRAP.md` per Pfad referenziert und deren Lesen zur ersten erzwungenen Aktion macht. Selbstverpflichtung in Dokumentation reicht nachweislich nicht.

**P18 — Parallele Sessions auf demselben fachlichen Bereich erfordern ein Claim, keine Hoffnung auf Zufalls-Nichtüberlappung.** Die dreifache `ConfigFingerprint`-Implementierung (GitHub-Analyse §2.1) ist der konkrete Beweis, dass „25 parallele Sessions werden schon zufällig nicht denselben Bereich anfassen" eine falsche Annahme ist. Jede Session beansprucht vor Schreibbeginn ein Claim auf den betroffenen fachlichen Bereich (nicht nur Datei-Ebene, da unterschiedliche Dateien dieselbe fachliche Fragestellung — z. B. „wo lebt der Config-Fingerprint" — betreffen können). Kein Claim-Mechanismus zu haben ist gleichbedeutend mit "Kollisionen werden akzeptiert".

**P19 — Ein beschlossener, aber nicht umgesetzter Governance-Beschluss ist schädlicher als gar keiner.** ADR-060 beschließt die Auflösung von `docs/decisions/`, wird seither zwölfmal ignoriert. `AGENTS.md` behauptet, ein existierendes Crate existiere nicht. Ein Dokument, das falsche oder überholte Aussagen mit demselben Autoritätsanspruch trägt wie korrekte, ist gefährlicher als ein Dokument, das offen als unvollständig markiert ist — weil es Vertrauen erzeugt, das die nächste Session in eine falsche Richtung lenkt. Jeder Governance-Beschluss braucht ab sofort entweder eine Umsetzungsfrist mit Gate-Durchsetzung oder wird formal zurückgezogen — ein dritter Zustand („beschlossen, aber liegen gelassen") ist nicht zulässig.

---

## §3 Governance-Infrastruktur: Ist-Zustand, Zielzustand, Einzelmaßnahmen

### §3.1 Ist-Zustand (live gegen HEAD `6fbb3ede1` verifiziert, siehe Anhang B für die vollständige Liste)

Sechs von zwölf vorhandenen xtask-Modulen sind toter Code — im Dateisystem vorhanden, aber weder per `mod` deklariert noch als Match-Arm aufrufbar: `check_duplicate_intent.rs`, `init_audit_fix.rs`, `jules_preflight.rs`, `generate_adr.rs`, `check_type_registry.rs`, `validate_pr_checklist.rs`. Darunter der wichtigste fehlende Baustein: `jules_preflight.rs` — ein fertiger, aber funktional inexistenter Preflight-Aggregator, der genau das P18-Claim-Problem lösen könnte, wenn er verdrahtet wäre.

`AGENTS.md` (per eigener Quellenhierarchie höchste Priorität) behauptet weiterhin `memfuse-kv-bridge` existiere nicht — das Crate ist vollständig implementiert und korrekt in `README.md`, `Cargo.toml`, `WORKING_STATE.md` gelistet. `docs/decisions/` enthält weiterhin 75 Dateien trotz ADR-060-Auflösungsbeschluss, darunter die vierfache ADR-070- und zweifache ADR-072-Kollision unverändert.

**Eine positive Entwicklung, live bestätigt:** `gen_prompter_data.rs` ist als einziges der zuvor kritischen xtask-Module tatsächlich verdrahtet (`mod gen_prompter_data;` + aktiver Match-Arm in `main.rs`) und erzeugt das in §7 behandelte Prompter-Manifest. Das zeigt: Die Verdrahtungslücke ist kein grundsätzliches technisches Problem — wenn ein Modul priorisiert wird, wird es auch angeschlossen. Es fehlt bisher an Priorisierung für die verbleibenden fünf, nicht an Fähigkeit.

### §3.2 Sofortmaßnahmen (Phase A, siehe §9 für Reihenfolge im Gesamtplan)

1. **`jules_preflight.rs` verdrahten** (`mod` + Match-Arm in `main.rs`) und in `justfile`/`context-gates.yml` einbinden. Priorität 1, da direkter Andockpunkt für den in §3.4 beschriebenen Claim-Mechanismus.
2. **`context-gates.yml` korrigieren:** Der aufgerufene, nicht-existente Subcommand `check-duplicate-intent` wird entweder durch Verdrahtung von `check_duplicate_intent.rs` zum Leben erweckt oder der Workflow-Schritt entfernt, bis er existiert. Ein CI-Gate, das permanent rot läuft oder als „nicht required" ignoriert wird, ist ein Gate, das nicht existiert — nur mit zusätzlichen Betriebskosten.
3. **`AGENTS.md` faktisch korrigieren** (`memfuse-kv-bridge`-Status), anschließend ein neues Gate `check_agents_md_crate_list.rs`, das bei jedem PR die Crate-Liste in `AGENTS.md` gegen die tatsächlichen `Cargo.toml`-Workspace-Mitglieder abgleicht und bei Abweichung blockiert (P19: nie wieder ein stiller Doku-Drift dieser Art).
4. **ADR-Kollisionen auflösen:** ADR-070 (4×) und ADR-072 (2×) manuell umbenennen/neu nummerieren; `generate_adr.rs` reparieren, sodass die höchste vorhandene Nummer live aus dem Dateisystem gelesen wird (nicht aus einer potenziell veralteten Zähler-Datei), und **atomar** vergeben wird (siehe §3.4 — dieselbe Infrastruktur wie der Claim-Mechanismus).
5. **ADR-060 tatsächlich umsetzen** (nicht zurückziehen — Begründung: siehe §3.3). `docs/decisions/*.md` werden 1:1 in `DECISIONS.md` überführt, Verzeichnis danach gelöscht, `generate_adr.rs` schreibt ausschließlich dorthin.
6. **Setup-Skript reparieren:** fehlenden Schritt `[4/8]` (ast-grep-Installation) ergänzen, `core.hooksPath` setzen, damit `.githooks/pre-commit` in frisch geklonten Jules-VMs überhaupt wirksam wird.
7. **Tote/riskante Skripte entscheiden:** `fix_db.py`, `fix_db2.py`, `fix_sstable.py` — **entfernen** (Begründung siehe §3.3), `check_unsafe_audit.py` — in `justfile` einbinden oder ebenfalls entfernen, falls obsolet.

### §3.3 Entschiedene Einzelfragen aus dem Governance-Audit (dortiger Abschnitt 6)

**„Soll `docs/decisions/` aufgelöst werden (ADR-060 umsetzen) oder ADR-060 zurückgezogen werden?"**
→ **ADR-060 umsetzen.** Die Existenz von vier ADR-070-Kollisionen *nach* ADR-060 ist der lebende Beweis, dass das verteilte Datei-System bei diesem Parallelitätsgrad (bis zu 25 gleichzeitige Sessions) strukturell nicht funktioniert. Ein einzelnes, append-only, per Lock-Mechanismus geschütztes Dokument ist die einzige Variante, die mit atomarer Nummernvergabe tatsächlich kollisionsfrei wird.

**„Claim-/Lock-Mechanismus: Markdown-Datei oder GitHub-Issue/Label?"**
→ **GitHub-Issue/Label-basiert.** Eine `CLAIMS.md` hat bei 25 parallelen Sessions dieselbe Race-Condition-Anfälligkeit, die sie eigentlich lösen soll (zwei Sessions committen gleichzeitig eine Änderung an derselben Claim-Datei → Merge-Konflikt statt Fehlermeldung *vor* Arbeitsbeginn). GitHub-Issues mit Labels sind bereits atomare API-Schreiboperationen, brauchen keinen eigenen Lock-Mechanismus obendrauf, und sind direkt aus `jules_preflight.rs` über die GitHub-API abfragbar.

**„`fix_db*.py`/`fix_sstable.py`: behalten mit Warnmarkierung oder entfernen?"**
→ **Entfernen.** Ungetestete Regex-Patches auf Rust-Quellcode, nirgends referenziert, sind ausschließlich Risiko — eine Session, die sie versehentlich ausführt, patcht Produktionscode blind. Falls die zugrunde liegende Fix-Logik dokumentationswürdig ist, gehört sie als Kommentar im betroffenen Code oder als ADR, nicht als ausführbares Skript im Repo-Root.

### §3.4 Claim-/Lock-Mechanismus (Phase B, Voraussetzung für parallele Struktur-Migration)

**Ziel:** Vor Arbeitsbeginn beansprucht eine Session ein Claim auf den fachlich betroffenen Bereich (nicht nur einzelne Dateipfade, siehe P18-Begründung — `ConfigFingerprint` betraf drei verschiedene Dateien in zwei verschiedenen Crates, ein reiner Datei-Lock hätte das nicht verhindert).

**Konkrete Umsetzung:** GitHub-Issue pro fachlichem Arbeitsbereich (z. B. „Claim: ConfigFingerprint / P8-Kalibrierungsschutz"), Label `claimed`, Zuweisung an die Session-ID. `jules_preflight.rs` fragt vor Session-Start per GitHub-API offene Claims ab, die den geplanten Arbeitsbereich überlappen, und bricht bei Konflikt mit einer expliziten Warnung ab, statt stillschweigend weiterzulaufen.

**Anbindung an das Prompter-Tool:** Ein einfaches Claim-Feld im Prompter (§7) prüft vor Generierung eines Prompts auf offene, überlappende Claims und warnt den Nutzer, bevor eine weitere parallele Session auf demselben Feature gestartet wird — die menschliche Kontrollinstanz, die laut GitHub-Analyse §2.7 aktuell komplett fehlt.
