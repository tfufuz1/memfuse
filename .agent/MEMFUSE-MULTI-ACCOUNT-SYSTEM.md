# MemFuse — Multi-Account Entwicklungssystem
## Continuous "Conveyor Belt" Orchestrierung · Dynamic Queue Dispatch · Squad-Koordination

> **Zweck:** Vollständige Betriebsanleitung für das 13-Account-Jules-System
> **Stand:** Mai 2026
> **Doctrine:** Continuous Development, Pipeline-Routing via ANCHOR v2, Proactive Lock Management

---

## 0. Ressourcen-Inventar & Realität

### Was du wirklich hast

| Ressource | Anzahl | Limit pro Einheit | Gesamt |
|-----------|--------|------------------|--------|
| Jules Accounts (Free) | 13 | 15 Tasks/Tag + 3 Concurrent | **195 Tasks/Tag** |
| Gemini-CLI (Browser) | 13 | Manuell, Free Tier | Supervision |
| Gemini-CLI (API Key) | 13 | AI Studio Free Tier | Automatisierung |
| Antigravity (Browser) | 13 | Manuell | Elite-Architektur |
| GitHub Account | 1 | Unbegrenzte Actions-Minuten (public repo) | CI/CD |

### Kritische Wahrheit über Jules Free Tier

Da im Free Tier tägliche Quotas (15 Tasks/Tag) existieren, muss die Orchestrierung **ereignisgesteuert (Event-Driven)** ablaufen. Statische Cron-Jobs (Schedules) führen zu Überlappungen, Ineffizienz und Leerkapazitäten.
Die Lösung ist der **Continuous Development Conveyor Belt**: Jules triggert Jules. Sobald ein PR gemerged wird, berechnet die CI den nächsten Schritt und löst den zuständigen Agenten aus.

---

## 1. Das Auto-Merge & Dispatch System (Kern-Lösung)

### Workflow 1: Auto-Merge Gate (`auto-merge-jules.yml`)

Alle Jules-Branches laden als Pull Requests in GitHub. Ein PR wird nur dann auto-gemerged, wenn das **Triple-Test-Gate** besteht:

1. **Triple Test:** 3x `cargo test --workspace` hintereinander erfolgreich.
2. **Static Invariants:** Zero `.unwrap()` und Zero `std::fs` (außerhalb `/tests/`).
3. **Security:** Autorisierter `cargo audit`.

```yaml
# Merge erfolgt per CI-Bot ("Auto-Approve" / Squash) nach Gate-Validierung.
```

### Workflow 2: Dynamic Queue Dispatcher (`jules-queue-dispatcher.yml`)

Dies ist der Herzschlag des Systems. Sobald der Auto-Merge das Triple-Test-Gate passiert hat und Code auf `develop` oder `main` landet:

1. Die CI parst die geänderten ANKER im Code (`SUCCESSOR:` Feld).
2. Das Skript `.agent/scripts/jules-dispatch.sh` berechnet die logische Abhängigkeits-Kette.
3. `.agent/scripts/jules-sync-locks.sh` wird ausgeführt, um Überschneidungen zu verhindern (z.B. Blockierung hoher Crates, solange die Basis wip ist).
4. Der Successor-Agent wird via API (z.B. `gh workflow run jules-invoke.yml`) direkt mit seinem API-Key ins Rennen geschickt.

### Workflow 3: Gemini Squad Review (`gemini-squad-review.yml`)

Ein rotierender Pool aus 13 Gemini API Keys (`gemini-squad-call.sh`) führt automatische "Senior Reviews" bei komplexen Architekturentscheidungen (z.B. neue `ANCHOR:ARCH` Tags) im PR-Prozess durch. Das garantiert Qualitätskontrolle jenseits von Compilern und Tests.

---

## 2. Account-Zuweisungen (The Squad)

### Feste Zuweisung (Agentur-Modell in `AGENTS.md`)

```
Account 01 → WP-0.0 Tech Debt Core       | Role: Core Guardian
Account 02 → WP-1.1 Compaction/Store      | Role: Store Engineer
Account 03 → WP-2.2 Quantization/Index    | Role: Index Architect
Account 04 → WP-1.2 Collections/DB        | Role: Collection Manager
Account 05 → WP-2.1 Hybrid Search/Text    | Role: Fusion Specialist
...
Account 13 → RESERVE                      | Role: Debt Hunter (Resolve CI Fails / Lock Break)
```

---

## 3. Tagesablauf (Realistischer Betrieb)

### Multi-Account Dashboard

Der Squad-Status wird zentral kontrolliert via:
`bash .agent/scripts/jules-dashboard.sh`
Das Dashboard liest Logs und ANCHOR-Status in Echtzeit.

### Deine Rolle
**Du fungierst als Context-Architekt.** Du startest die Kettenreaktion initial (z.B. Setup neuer `SPEC`-ANCHOR) oder bei Stillstand:
`gh workflow run jules-invoke.yml -f account_id=01`

Sobald ein Agent aktiv ist (`STATUS:ACTIVE`), sorgt das Continuous-Development-Modell dafür, dass er bei Abschluss den nächsten Kollegen in den Code `SUCCESSOR` schreibt. Der Auto-Merge triggert dann den nächsten. Du musst nur:

1. Gelegentlich (1x Tag) in GitHub überprüfen, ob ein PR der Pipeline das Triple-Test-Gate gerissen hat.
2. Im Fall von CI-Abbrüchen delegierst du Account 13 (Debt Hunter) auf den Fehler (z.B. `.unwrap()` eingefügt).

---

## 4. Problemlösungen & Fallstricke

### Problem: Stale Tasks (Deadlock)
`jules-sync-locks.sh` lockt Aufgaben, um Merge-Konflikte zu vermeiden. Wenn ein Agent scheitert, bleibt das Lock.
**Lösung:** Ein Fallback-Dispatcher (`jules-queue-dispatcher.yml` daily dry-run) erkennt ANKER, die älter als 8h sind (`CREATED` Datum) und setzt diese wieder auf `READY` zurück.

### Problem: Jules öffnet PR aber CI schlägt fehl
Agent A schreibt Tests. Typischerweise schlägt die CI absichtlich fehl (Red-Phase). Das Auto-Merge-Skript weigert sich den PR zu mergen.
**Lösung:** Agenten sind so gebrieft, dass sie selbst in Sub-PRs iterieren. Schlägt der PR nach 3 Versuchen fehl, wird Jules-13 als Fixer von GitHub Actions per Comment (`/jules-fix`) in den PR gerufen.

### Problem: Daily Task Limit (15/Tag) erschöpft
Das Continuous Routing beachtet Kontingente. `jules-dispatch.sh` prüft das Limit eines Accounts. Ist dieser "out of quota", schwenkt die Queue auf den alternativen Account (z.B. Reserve).

---

## 5. Erfolgsmessung (KPIs)

- **Continuous Uptime:** Die Time-to-Merge von `READY` -> `DONE` & `SUCCESSOR`-Event beträgt weniger als 1h.
- **Triple-Test-Gate:** 100% (kein PR merged ohne 3× grün).
- **Unwrap in Prod:** 0.

Dieses System läuft asynchron rund um die Uhr, gelenkt durch pure Code-Zustände.
