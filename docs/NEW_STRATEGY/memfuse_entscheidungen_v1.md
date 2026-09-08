# MemFuse — Verbindliche Entscheidungen & Umsetzungsreihenfolge
**Rolle:** Principal Senior Rust Architect — Synthese aus Governance-Audit, GitHub-Analyse, Zielarchitektur v8.0
**Zweck:** Die drei vorliegenden Dokumente sind analytisch vollständig. Was fehlt, ist eine Entscheidung. Dieses Dokument trifft sie — explizit, begründet, und in einer Form, die direkt in Jules-Prompts übersetzbar ist.

---

## 0. Warum jetzt entscheiden statt weiter analysieren

Der GitHub-Verlauf zeigt ein einzelnes, sich wiederholendes Muster: **fehlende Entscheidung wird durch Kapazität kompensiert.** Drei Produktvisionen, sieben Zielarchitektur-Versionen, 25 parallele Sessions, keine Koordination — das ist kein Zufall, sondern die vorhersagbare Folge davon, dass bei ~195 Tasks/Tag jede unentschiedene Frage von mehreren Agenten gleichzeitig "gelöst" wird, jeweils anders. Die `ConfigFingerprint`-Dreifachimplementierung (3× in 66 Minuten) ist der Beweis am konkretesten Fall; die drei Produktvisionen sind dasselbe Muster auf Architekturebene.

**Konsequenz für dieses Dokument:** Jede offene Frage unten bekommt eine Entscheidung, keine Optionsliste. Wo eine echte Unsicherheit besteht, wird das benannt — aber es wird trotzdem entschieden, mit Revisionsklausel statt Nicht-Entscheidung.

---

## 1. Die Kernentscheidung: Produktvision (Zielarchitektur §6 / Anhang D.1)

**Entscheidung: Option 1 — PyPI-Library (ADR-007-Richtung), Position A aus §1.**

**Begründung:**
- Ihr faktischer Entwicklungsmodus — ein Solo-Entwickler, der über einen Agenten-Schwarm orchestriert, keine Vertriebs-/Support-Organisation, kein Enterprise-Sales-Motion — passt strukturell nicht zu Option 2. Eine Desktop-Enterprise-App mit Multi-Tenant-Anspruch braucht Vertrauen, Support-SLAs und Referenzkunden, die ein Ein-Personen-Projekt nicht glaubwürdig liefern kann, unabhängig von Codequalität.
- Position A ("schlanker als MinnsDB, mit Krypto-Härtung als Alleinstellungsmerkmal") ist in einem Satz kommunizierbar und in einer PyPI-Release-Zyklus-Größenordnung lieferbar. Position B (Enterprise/Audit-Deployments) erfordert Vertriebszyklen, die dieses Setup nicht bedienen kann.
- Der bereits stärkste, differenzierende Code-Bestandteil (HMAC-WAL-Kette, kryptografischer Löschbeweis) ist als Compliance-Feature *innerhalb* einer Library genauso vermarktbar wie innerhalb einer Enterprise-App — der Vorteil geht durch diese Entscheidung nicht verloren.
- Die DiskANN-Out-of-Core-Fähigkeit (Kern von Position B) bleibt als **spätere Erweiterung innerhalb der Library** möglich (großer Korpus auf Disk ist ein Library-Feature, kein App-Feature) — das ist keine verlorene Option, nur eine Sequenzierung.

**Sofortige Konsequenz (P15, verbindlich, 60-Tage-Frist ab heute):**
- `memfuse-py` wird primäres, best-gepflegtes Grenzschicht-Crate.
- `memfuse-tauri` wird als **superseded** in einer neuen ADR markiert (referenziert ADR-018) und **physisch entfernt** — Frist: 60 Tage ab Merge dieser Entscheidung ins Repo.
- Multi-Tenant-Konzepte in `memfuse-security::kv_segment` werden auf Test-Isolation reduziert; Enterprise-Marketing-Sprache aus allen Docs entfernt.
- **Option 3 (Voice/Jarvis)**: formales VETO mit `conditional_review_due` = 6 Monate ab heute, analog VETO-F02.

**Revisionsklausel:** Wenn nach Phase 0 (LongMemEval/LoCoMo-CI, siehe unten) der direkte Benchmark-Vergleich gegen MinnsDB zeigt, dass die Out-of-Core-Fähigkeit der einzige signifikante Vorteil ist (Position A bietet keinen messbaren Unterschied bei typischer Korpusgröße), wird diese Entscheidung explizit neu bewertet — nicht stillschweigend unterlaufen.

---

## 2. Weitere offene Punkte aus Anhang D — entschieden

**D.2 — Physio-Feature-Klassifikation:** Mit Vision = Option 1 (Library, Fokus Einzelanwender/Auditpflicht) gilt: Alles, was Multi-Tenant oder Enterprise-Orchestrierung voraussetzt (u. a. F-07/Replikatordynamik), wird zu **Entfernungskandidat**, nicht "experimentell". Kern bleibt nur, was den Compliance-/Krypto-USP direkt stützt oder Retrieval-Qualität verbessert. Konkrete Einzelklassifikation ist ein eigener, kleiner Folgeauftrag (Tabelle aus §5 durchgehen, pro Zeile: Kern/Entfernen — keine dritte Kategorie mehr zulassen).

**D.3 — 60-Tage-Frist:** bestätigt (siehe oben), mit hartem Kalenderdatum im Tracking-Issue, nicht nur "60 Tage" als Text.

**D.4 — `memfuse-py`/`memfuse-tauri` als ein Crate mit Feature-Flags oder getrennt:** Da `memfuse-tauri` entfernt wird, erledigt sich die Frage — `memfuse-py` bleibt eigenständig, kein gemeinsames `memfuse-bindings`-Crate nötig. Ein Feature-Flag-Konstrukt für ein bereits totes Ziel wäre unnötige Komplexität.

---

## 3. Governance-Audit — offene Punkte (Abschnitt 6) entschieden

**"Soll `docs/decisions/` aufgelöst werden (ADR-060 umsetzen) oder ADR-060 zurückgezogen werden?"**
→ **ADR-060 umsetzen.** `DECISIONS.md` wird Single Source of Truth. Begründung: Die Existenz von vier ADR-070-Kollisionen *nach* ADR-060 ist der lebende Beweis, dass das verteilte Datei-System bei diesem Parallelitätsgrad (25 Sessions) nicht funktioniert — ein einzelnes, append-only, per Lock-Mechanismus geschütztes Dokument ist die einzige Variante, die mit atomarer Nummernvergabe (siehe Maßnahme 4) tatsächlich kollisionsfrei gemacht werden kann. `docs/decisions/*.md` werden 1:1 in `DECISIONS.md` überführt, Verzeichnis danach gelöscht, `generate_adr.rs` schreibt ausschließlich dorthin.

**"Claim-/Lock-Mechanismus: Markdown-Datei oder GitHub-Issue/Label?"**
→ **GitHub-Issue/Label-basiert, nicht Markdown-Datei.** Begründung: Eine `CLAIMS.md` hat bei 25 parallelen Sessions dieselbe Race-Condition-Anfälligkeit, die gerade das Kernproblem ist (zwei Sessions committen gleichzeitig eine Änderung an derselben Datei → Merge-Konflikt statt Fehlermeldung *vor* Arbeitsbeginn). GitHub-Issues mit Labels sind bereits atomar (API-Schreiboperation), brauchen keinen eigenen Lock-Mechanismus obendrauf, und sind über die GitHub-API direkt aus `jules_preflight.rs` abfragbar. Aufwand ist höher als eine Datei, aber das ist genau der Punkt, an dem der Aufwand gerechtfertigt ist (Maßnahme mit "sehr hoher Wirkung" laut eigener Priorisierungstabelle).

**"`fix_db*.py`/`fix_sstable.py`: behalten mit Warnmarkierung oder entfernen?"**
→ **Entfernen.** Ungetestete Regex-Patches auf Rust-Quellcode, die nirgends referenziert sind, sind kein Referenzwert — sie sind ausschließlich Risiko (ein Agent, der sie versehentlich ausführt, patcht Produktionscode blind). Falls die zugrundeliegende Fix-Logik dokumentationswürdig ist, gehört sie als Kommentar in den betroffenen Code oder als ADR, nicht als ausführbares Skript im Repo-Root.

---

## 4. Konsolidierte Umsetzungsreihenfolge (governance + Struktur zusammengeführt)

Die Zielarchitektur-Migration (§8 dort) und die Governance-Reparatur (§5.4 im Audit) sind bisher zwei getrennte Pläne. Sie müssen kombiniert werden, weil die Struktur-Migration ohne funktionierende Governance genau dieselben Kollisionen produzieren wird, die der GitHub-Verlauf bereits zeigt.

**Phase A — Governance-Fundament (vor jeder Struktur-Migration, ca. 1 Woche, niedriges Risiko):**
1. `jules_preflight.rs` verdrahten + in `context-gates.yml`/`justfile` einbinden.
2. `context-gates.yml` reparieren (`check-duplicate-intent` fixen oder entfernen).
3. `AGENTS.md` faktisch korrigieren (`memfuse-kv-bridge`) + Gate `check_agents_md_crate_list.rs`.
4. ADR-Kollisionen auflösen + `DECISIONS.md`-Migration (Governance-Entscheidung Abschnitt 3 oben) + atomare Nummernvergabe.
5. Setup-Skript reparieren: `ast-grep`-Installation ergänzen (fehlendes `[4/8]`), `core.hooksPath` setzen.
6. Tote Skripte entscheiden (siehe Abschnitt 3: `fix_db*.py` entfernen).

**Phase B — Claim-/Lock-Mechanismus (Voraussetzung für parallele Struktur-Migration):**
7. GitHub-Issue/Label-Claim-System implementieren, `jules_preflight.rs` prüft dagegen.
8. Prompter-Tool (`Memfuse-Prompter-v24.html`) um Pflicht-Präfix (`AGENTS.md`, `.jules/SESSION_BOOTSTRAP.md` explizit referenzieren) und Claim-Warnung erweitern.

**Phase C — Struktur-Migration (Zielarchitektur §8, jetzt mit funktionierendem Claim-Schutz):**
9. Phase 0 (LongMemEval/LoCoMo-CI) parallel starten — unabhängig, blockiert nichts.
10. Zielarchitektur-Phasen 1–5 wie in v8.0 §8 spezifiziert (Security → Persistence → Inference → Scheduler → Retrieval), jeder Schritt durch das jetzt aktive Claim-System abgesichert.
11. Vision-Entscheidung (Abschnitt 1 oben) wird **sofort**, nicht erst in Phase 6, in Code umgesetzt: `memfuse-tauri`-Entfernung kann parallel zu Phase C laufen, da sie kein anderes Zielmodul berührt.

**Phase D — Abschluss:**
12. Prompter-Crate-Manifest-Export (§7.2 der Zielarchitektur) — erst jetzt, gegen die finale 9–10-Crate-Topologie.
13. Ankersystem, CI-Konsolidierung (Triple-Run → `cargo-nextest --retries`, `dag-check.yml`-Merge).

**Warum diese Reihenfolge und nicht die einzeln vorgeschlagenen:** Der Governance-Audit priorisiert die Reparatur der Kontroll-Mechanismen richtig, benennt aber die Struktur-Migration nicht. Die Zielarchitektur benennt die Struktur-Migration richtig, geht aber implizit davon aus, dass Koordination bereits funktioniert. Ohne Phase A+B zuerst wiederholt die Struktur-Migration selbst das `ConfigFingerprint`-Muster — nur diesmal auf Modulebene statt auf Funktionsebene, mit entsprechend größerem Schaden bei einer Kollision.

---

## 5. Was du als Mensch als Nächstes tun musst

Nur drei Aktionen sind nicht an Jules delegierbar:
1. Diese Vision-Entscheidung (Abschnitt 1) formal als ADR bestätigen — der Text oben ist als Entwurf verwendbar.
2. Das GitHub-Issue/Label-Schema für den Claim-Mechanismus final festlegen (Label-Namensschema, welches Repo-Recht Jules-Sessions dafür brauchen).
3. Die 60-Tage-Frist für `memfuse-tauri`-Entfernung als konkretes Kalenderdatum in ein Tracking-Issue eintragen.

Alles andere in Abschnitt 4 ist als Jules-Task in der bestehenden Prompter-Taxonomie ausführbar.
