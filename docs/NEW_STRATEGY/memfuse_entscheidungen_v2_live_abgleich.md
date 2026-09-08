# MemFuse — Plan-Update nach Live-Repo-Abgleich (2026-09-08)
**Methodik:** Frischer `git clone` von `github.com/tfufuz1/memfuse` gegen HEAD verifiziert. Ergänzt v1 (Entscheidungsdokument), ersetzt es nicht.

---

## 1. Live-Verifikation: Governance-Zustand unverändert gegenüber Audit

Alle drei im letzten Audit benannten Kernbefunde sind am aktuellen HEAD **1:1 noch vorhanden** — nichts davon wurde in der Zwischenzeit repariert:

| Befund | Live-Check | Ergebnis |
|---|---|---|
| xtask-Module unverdrahtet | `grep "^mod " xtask/src/main.rs` | Nur 6 von 12 vorhandenen `.rs`-Dateien deklariert (`jules_preflight.rs`, `check_duplicate_intent.rs`, `init_audit_fix.rs`, `check_type_registry.rs`, `validate_pr_checklist.rs`, `generate_adr.rs` weiterhin tot) |
| `AGENTS.md` Falschaussage | `grep "kv-bridge" AGENTS.md` | Zeile 70: `` `memfuse-kv-bridge` \| FEHLT \| Crate existiert nicht im Repository `` — Crate liegt aber unter `crates/memfuse-kv-bridge/` vollständig vor |
| `bench.yml` läuft gegen Fixtures | `benchmarks/results/current_metrics.json` | `total_cases: 2` bei beiden Datensätzen — exakt der im Audit zitierte Zustand |

**Neu seit dem letzten Blick:** Ein Commit `docs(governance): add AI dev system review and specification (#1773)` ist auf `main` gelandet — das ist vermutlich genau die von dir vorgelegte Spezifikation selbst, die jetzt als Dokumentation im Repo liegt, aber noch **nicht in Code umgesetzt** wurde. Das bestätigt das immer gleiche Muster: Governance wird dokumentiert, aber die Umsetzung bleibt ein separater, bisher nicht gestarteter Schritt.

**80 aktive Remote-Branches** weiterhin vorhanden (Stichprobe bestätigt u. a. weiterhin offene, thematisch überlappende Branches wie `diskann-flush-threshold-benchmark-…`) — die Branch-Proliferation aus der GitHub-Analyse ist nicht abgeklungen.

---

## 2. Die laufende Jules-Session: Bewertung des vorgelegten Transcripts

Das transcript zeigt eine Jules-Session, die **exakt Phase 0** aus der Zielarchitektur-Migration bearbeitet (LongMemEval/LoCoMo-CI von Fixtures auf echte Datensätze). Das ist strukturell die richtige Priorität — Phase 0 sollte ohnehin parallel zu allem anderen laufen. Kein passender Branch dafür ist bisher im Remote sichtbar; die Session läuft also noch lokal/ungepusht.

**Was gut läuft:**
- Datensatzquellen korrekt identifiziert, Parser-Fix für heterogene JSON-Typen (`answer`/`turn.content` als `serde_json::Value` statt `String`) ist eine plausible, notwendige Korrektur — reale Datensätze sind selten so sauber typisiert wie Fixtures.
- Lokale Ausführung beider Benchmarks bereits verifiziert, bevor CI angefasst wird — richtige Reihenfolge.

**Drei konkrete Abweichungen, die vor dem Merge korrigiert werden müssen:**

1. **Falsche/abweichende Datenquelle für LongMemEval.** Der Auftrag verlangt `longmemeval_s.jsonl` vom offiziellen Repo (`xiaowu0162/longmemeval`, per README auch so dokumentiert). Jules verwendet stattdessen `xiaowu0162/longmemeval-cleaned` (HuggingFace-Mirror, „cleaned"-Variante, 265 MB, andere Datei: `longmemeval_s_cleaned.json`). Das ist **nicht dieselbe Datei** — eine "cleaned" Variante kann andere Antwort-Normalisierung, entfernte Edge-Cases oder abweichende Fragen-/Antwortzahlen haben. Konsequenz: Die daraus erzeugte `baseline_metrics.json` wäre gegen einen anderen Datensatz kalibriert, als das Auftragsdokument und die eigene README (die weiterhin `longmemeval_s.jsonl` vom Original-Repo nennt) beschreiben. Wenn diese Baseline unkorrigiert gemerged wird, driften Auftragsbeschreibung, README und tatsächlich verwendete Daten sofort wieder auseinander — dasselbe Muster wie bei `AGENTS.md`/Code.
   → **Entscheidung:** Falls die "cleaned"-Variante bewusst gewählt wurde (z. B. weil das Original-Repo kein direktes Rohdatei-Downloadlink bietet, sondern Git-LFS/Skript erfordert), muss das explizit im PR-Text und in der README begründet und die README-Anleitung (Abschnitt A, Zeile 32–34) entsprechend aktualisiert werden — nicht stillschweigend abweichen. Sonst: auf die im Auftrag genannte Originalquelle zurückgehen.
2. **Nur `_s`-Variante, keine Entscheidung zu `_m`.** README erwähnt `longmemeval_s.jsonl` **oder** `longmemeval_m.json` als Optionen. Die Session hat sich stillschweigend für `_s` entschieden, ohne das zu vermerken. Für eine CI-Baseline ist das in Ordnung (kleinere Variante, günstiger), sollte aber als bewusste Entscheidung im PR-Text stehen, damit ein späterer Agent nicht denkt, `_m` sei vergessen worden und es "nachträgt".
3. **265 MB Download pro CI-Lauf ist der im Original-Auftrag explizit benannte Risikofall** ("falls die Datensätze zu groß für direktes Checkout/Cache sind, dokumentiere einen alternativen Weg"). Das Transcript zeigt bisher **keinen** `actions/cache`-Schritt, nur die Absicht dazu ("Add step to download… Configure actions/cache…" ist noch offen/"Working"). Das ist der wichtigste noch fehlende Teil — ohne Cache lädt jeder CI-Lauf (bei 195 Sessions/Tag potenziell sehr häufig, auch wenn `bench.yml` nur bei Push/PR auf `main`/`develop` läuft) 265 MB neu, was CI-Zeit und ggf. HuggingFace-Rate-Limits strapaziert.

**Freigabe-Empfehlung für diese Session:** Nicht stoppen — technisch auf dem richtigen Weg —, aber vor Merge: Punkt 1 (Datenquelle) explizit klären/dokumentieren, Punkt 3 (Caching) fertigstellen, dann erst `baseline_metrics.json` final erzeugen. Eine Baseline, die vor dem Caching-Fix erzeugt wird, ist ohnehin nur ein Zwischenstand.

---

## 3. Präzisierter Gesamtplan (ersetzt Abschnitt 4 aus v1)

Da Phase 0 (Benchmarks) bereits aktiv läuft, wird der Plan neu sequenziert: **Phase 0 läuft weiter parallel**, Governance-Fundament (bisher Phase A) bleibt unabhängig davon die Voraussetzung für alles, was danach an Struktur-Migration beginnt.

**Sofort, parallel zueinander (keine gemeinsamen Dateien):**
- **Spur 1 (läuft bereits):** Benchmark-Session zu Ende bringen — mit den drei Korrekturen aus Abschnitt 2 vor dem Merge.
- **Spur 2 (neu starten):** Governance-Fundament aus v1 Phase A (xtask verdrahten, `context-gates.yml` reparieren, `AGENTS.md`-Kv-Bridge-Fakt korrigieren, ADR-Kollisionen lösen, Setup-Skript-Lücke schließen). Diese Spur berührt keine Benchmark-Dateien, kann also risikofrei gleichzeitig laufen.

**Danach, sequenziell (Voraussetzung: Spur 2 abgeschlossen):**
- Claim-/Lock-Mechanismus (v1 Phase B) — erst jetzt sinnvoll, weil `jules_preflight.rs` als Andockpunkt existieren muss.
- Struktur-Migration (v1 Phase C) — jetzt zusätzlich mit einer **echten** Benchmark-Baseline aus Spur 1 absicherbar (v8.0 §9, Erfolgsmessung Punkt 1 wird dadurch überhaupt erst prüfbar, nicht nur ein Platzhalter-Wert bei `total_cases: 2`).

**Wichtige Korrektur gegenüber v1:** In v1 war Phase 0 als "kann jederzeit parallel starten" beschrieben, aber nachrangig behandelt. Live-Check zeigt: Sie läuft bereits und ist weiter fortgeschritten als die Governance-Reparatur. Das ändert die Priorität nicht (beide sind weiterhin parallel, unabhängige Dateien), aber es bedeutet: **Spur 2 sollte jetzt als nächste Session gestartet werden**, damit sie nicht hinter Spur 1 zurückfällt, bevor die Struktur-Migration beginnen kann.

---

## 4. Nächste konkrete Aktion für dich

1. Der laufenden Jules-Session eine kurze Korrektur-Anweisung geben: Datenquelle für LongMemEval klären (Original vs. „cleaned"-Mirror, README entsprechend anpassen) und Caching fertigstellen, **bevor** `baseline_metrics.json` final committet wird.
2. Eine zweite, unabhängige Session für Spur 2 (Governance-Fundament, v1 Phase A) parallel starten — sie kollidiert mit keiner Datei aus der Benchmark-Session.
