# GitHub als Kommunikationssystem für Structured Process Orchestration (SPO)

> **Architektur-Spezifikation v1.0** | Senior Systemarchitekt | März 2026  
> *GitHub as Brain · Blackboard · Trigger*

---

## Inhaltsverzeichnis

1. [Executive Summary](#1-executive-summary)
2. [Konzeptionelle Grundlage: Das Drei-Rollen-Modell](#2-konzeptionelle-grundlage-das-drei-rollen-modell)
3. [Architekturübersicht](#3-architekturübersicht)
4. [GitHub as Brain – Wissensspeicher & Entscheidungsgedächtnis](#4-github-as-brain)
5. [GitHub as Blackboard – Gemeinsames Kommunikationssubstrat](#5-github-as-blackboard)
6. [GitHub as Trigger – Ereignis- und Automatisierungsschicht](#6-github-as-trigger)
7. [Agenten-Kommunikationsprotokoll](#7-agenten-kommunikationsprotokoll)
8. [Nachvollziehbare Chronik & Entscheidungsdokumentation](#8-nachvollziehbare-chronik--entscheidungsdokumentation)
9. [Standards & Konventionen](#9-standards--konventionen)
10. [Repository-Struktur & Dateiorganisation](#10-repository-struktur--dateiorganisation)
11. [Rollen & Verantwortlichkeiten der Agenten](#11-rollen--verantwortlichkeiten-der-agenten)
12. [Sicherheit, Governance & Guardrails](#12-sicherheit-governance--guardrails)
13. [Implementierungs-Roadmap](#13-implementierungs-roadmap)
14. [Referenzen & Best Practices](#14-referenzen--best-practices)

---

## 1. Executive Summary

Dieses Dokument beschreibt ein vollständiges **GitHub-zentriertes Kommunikationssystem** für die strukturierte Prozess-Orchestrierung (SPO) in Softwareentwicklungsprojekten, in dem jeder Agent – menschlich oder KI-gesteuert – **proaktiv** mit GitHub interagiert.

GitHub übernimmt dabei drei fundamentale Rollen gleichzeitig:

| Rolle | GitHub-Primitive | Funktion |
|---|---|---|
| **Brain** | Wiki, Discussions, ADRs, `agents.md` | Persistentes Projektgedächtnis, Regeln, Entscheidungen |
| **Blackboard** | Issues, PR-Kommentare, Projects | Gemeinsamer Zustandsraum für asynchrone Kooperation |
| **Trigger** | Actions, Webhooks, Labels, Events | Aktivierungsschicht für automatisierte Agentenreaktionen |

Das System basiert auf dem **Blackboard Pattern** aus der klassischen KI-Systemarchitektur, adaptiert für das moderne GitHub-Ökosystem mit MCP-Integration, Agentic Workflows und strukturierter Issue-Kommunikation.

---

## 2. Konzeptionelle Grundlage: Das Drei-Rollen-Modell

### 2.1 Das Blackboard Pattern

Das Blackboard Pattern ist ein etabliertes Architekturmuster für kollaborative, multi-agenten Problemlösung:

```
┌─────────────────────────────────────────────────────┐
│                    BLACKBOARD                        │
│  (Gemeinsamer Wissensspeicher / GitHub Repository)  │
│                                                      │
│  ┌──────────┐  ┌──────────┐  ┌──────────────────┐  │
│  │  Issues  │  │   PRs    │  │    Discussions   │  │
│  └──────────┘  └──────────┘  └──────────────────┘  │
└─────────────────────────────────────────────────────┘
         ▲              ▲              ▲
         │   lesen/     │   schreiben  │
         ▼              ▼              ▼
  ┌────────────┐ ┌────────────┐ ┌────────────┐
  │  Agent A   │ │  Agent B   │ │  Agent C   │
  │(Architect) │ │(Developer) │ │ (QA/Test)  │
  └────────────┘ └────────────┘ └────────────┘
```

### 2.2 Prinzipien der proaktiven Agentenkommunikation

Jeder Agent **muss** folgende Grundprinzipien einhalten:

1. **Proaktivität**: Agenten initiieren Kommunikation, warten nicht auf direkte Anfragen
2. **Verlinkung**: Jede Nachricht referenziert relevante Issues, PRs, Commits oder Personen per `#ID` / `@mention`
3. **Atomizität**: Eine Nachricht = eine klar abgrenzbare Information oder Entscheidung
4. **Zustandspflege**: Agenten aktualisieren den Projektstatus nach jeder Aktion
5. **Nachvollziehbarkeit**: Alle Entscheidungspfade werden explizit dokumentiert

---

## 3. Architekturübersicht

```
┌─────────────────────────────────────────────────────────────────────┐
│                    GITHUB REPOSITORY ECOSYSTEM                       │
│                                                                      │
│  ┌─────────────┐    ┌──────────────┐    ┌────────────────────────┐ │
│  │    BRAIN    │    │  BLACKBOARD  │    │       TRIGGER          │ │
│  │             │    │              │    │                        │ │
│  │ • Wiki      │    │ • Issues     │    │ • GitHub Actions       │ │
│  │ • agents.md │◄──►│ • PR Reviews │◄──►│ • Agentic Workflows    │ │
│  │ • ADRs      │    │ • Projects   │    │ • Webhooks             │ │
│  │ • Decisions │    │ • Milestones │    │ • Label-Trigger        │ │
│  │ • CODEOWNERS│    │ • Comments   │    │ • Schedule-Trigger     │ │
│  └─────────────┘    └──────────────┘    └────────────────────────┘ │
│         ▲                  ▲                       ▲               │
│         └──────────────────┴───────────────────────┘               │
│                             │                                        │
│                    MCP-Server-Layer                                  │
│         ┌───────────────────┴──────────────────────┐               │
│         │          GitHub MCP Server               │               │
│         │  (100+ Tools für Issues, PRs, Actions)   │               │
│         └───────────────────────────────────────────┘               │
│                             │                                        │
│         ┌───────────────────┴──────────────────────┐               │
│   ┌─────┴───┐  ┌────────────┐  ┌─────────────┐    │               │
│   │Agent:   │  │ Agent:     │  │ Agent:      │    │               │
│   │Architect│  │ Developer  │  │ QA / Ops    │    │               │
│   └─────────┘  └────────────┘  └─────────────┘    │               │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 4. GitHub as Brain

### 4.1 `agents.md` – Das Gehirn-Manifest

Jedes Repository **muss** eine `agents.md`-Datei im Root-Verzeichnis enthalten. Sie ist die zentrale Instruktionsdatei für alle Agenten.

**Pflichtstruktur:**

```markdown
# agents.md – Agenten-Instruktionen für [Projektname]

## Projekt-Kontext
[Kurzbeschreibung des Projekts, Domäne, Ziele]

## Agentenrollen & Zuständigkeiten
- **Architect-Agent**: Verantwortlich für ADRs, Issue #Architektur, Wiki-Pflege
- **Dev-Agent**: Feature-Branches, PR-Erstellung, Code-Kommentare
- **QA-Agent**: Test-Issues, Review-Kommentare, Qualitätsgates
- **Ops-Agent**: Deployment-Issues, Monitoring-Alerts, Incident-Kommunikation

## Kommunikationsregeln (PFLICHT)
1. Jedes Issue muss mit Kontext-Links eröffnet werden
2. Jede Entscheidung erhält ein ADR in `/docs/adr/`
3. Status-Updates erfolgen via Issue-Kommentar, nicht per Chat
4. Alle @mentions sind bewusst und zweckgebunden

## Verlinkungs-Pflichten
- Feature-Issues → verlinken auf Architektur-Issue
- PRs → verlinken auf implementiertes Issue (`Closes #ID`)
- ADRs → verlinken auf auslösendes Issue oder PR

## Verbotene Aktionen
- Direkte Commits auf `main` ohne PR
- Issues ohne Labels eröffnen
- Entscheidungen außerhalb des GitHub-Systems treffen
```

### 4.2 Architecture Decision Records (ADRs)

Alle architektonischen Entscheidungen werden als ADRs versioniert:

**Dateiort:** `docs/adr/ADR-NNNN-kurztitel.md`

**Pflichttemplate:**

```markdown
# ADR-0042: [Entscheidungstitel]

**Status:** [Proposed | Accepted | Deprecated | Superseded by ADR-XXXX]  
**Datum:** YYYY-MM-DD  
**Entscheider:** @agent-architect, @lead-dev  
**Auslöser:** Issue #234, PR #189  

## Kontext
Welches Problem oder welche Anforderung hat diese Entscheidung ausgelöst?

## Entscheidung
Was wurde entschieden?

## Begründung
Warum wurde diese Option gewählt?

## Abgewogene Alternativen
| Option | Vorteile | Nachteile |
|--------|----------|-----------|
| Option A | ... | ... |
| Option B (gewählt) | ... | ... |

## Konsequenzen
- Positive Auswirkungen
- Risiken & technische Schulden

## Verlinkungen
- Ausgelöst durch: #234
- Implementiert in: PR #301
- Superseded by: (leer)
```

### 4.3 GitHub Wiki als lebendiges Wissensarchiv

```
Wiki-Struktur:
├── Home                          ← Projekt-Übersicht & Navigation
├── Architecture/
│   ├── System-Overview           ← Systemarchitektur
│   ├── Component-Map             ← Komponentenkarte
│   └── Data-Flow                 ← Datenflüsse
├── Processes/
│   ├── Development-Workflow      ← Entwicklungsprozess
│   ├── Release-Process           ← Release-Ablauf
│   └── Incident-Response         ← Störungsmanagement
├── Agents/
│   ├── Agent-Roster              ← Alle aktiven Agenten & Rollen
│   ├── Communication-Protocol    ← Diese Spezifikation
│   └── Decision-Log              ← Entscheidungschronik
└── Standards/
    ├── Coding-Standards          ← Code-Richtlinien
    ├── Issue-Templates           ← Issue-Vorlagen
    └── PR-Checklist              ← PR-Checkliste
```

---

## 5. GitHub as Blackboard

### 5.1 Issues als primäre Kommunikationseinheit

Issues sind das Herzstück des Blackboard-Systems. **Jeder kommunikative Akt eines Agenten beginnt mit einem Issue oder einem Issue-Kommentar.**

#### Issue-Typen & Label-System

```
ISSUE-TYPEN (durch Labels kodiert):
┌─────────────────────────────────────────────────────────────────┐
│ type:feature     │ Neue Funktionalität                          │
│ type:bug         │ Fehler & Regressions                         │
│ type:decision    │ Entscheidungsbedarf (→ erzeugt ADR)          │
│ type:risk        │ Risiko-Meldung durch Agent                   │
│ type:blocker     │ Blockierendes Problem                        │
│ type:spike       │ Technische Untersuchung / Prototyp           │
│ type:ops         │ Betrieb & Infrastruktur                      │
│ type:communication│ Agenten-Koordination                        │
└─────────────────────────────────────────────────────────────────┘

STATUS-LABELS (Zustandsmaschine auf dem Blackboard):
┌─────────────────────────────────────────────────────────────────┐
│ status:new        │ Neu eingestellt, noch nicht bewertet        │
│ status:triaged    │ Priorisiert, Zuständigkeit geklärt          │
│ status:in-progress│ Aktiv bearbeitet                            │
│ status:blocked    │ Wartet auf Abhängigkeit                     │
│ status:review     │ In Überprüfung                              │
│ status:done       │ Abgeschlossen                               │
└─────────────────────────────────────────────────────────────────┘

PRIORITÄT:
  priority:critical | priority:high | priority:medium | priority:low

AGENTEN-ZUWEISUNG:
  agent:architect | agent:developer | agent:qa | agent:ops
```

#### Pflicht-Struktur für jeden Issue

```markdown
## 📋 Kontext
<!-- Warum wird dieses Issue erstellt? Welches Problem wird gelöst? -->

## 🎯 Ziel / Acceptance Criteria
- [ ] Kriterium 1
- [ ] Kriterium 2

## 🔗 Verlinkungen
- **Übergeordnetes Issue:** #
- **Abhängige Issues:** #, #
- **Referenzierter ADR:** ADR-NNNN (falls vorhanden)
- **Referenzierter PR:** # (nach Implementierung)

## 📊 Kontext-Informationen
- **Betroffene Komponente:** 
- **Umgebung:** dev | staging | prod
- **Erstellt von Agent:** @

## 📝 Entscheidungshistorie
<!-- Alle getroffenen Entscheidungen in diesem Issue chronologisch -->
```

### 5.2 Pull Requests als Wissenstransfer

PRs sind nicht nur Code-Reviews – sie sind **strukturierte Wissenstransfer-Dokumente**.

**PR-Template (`.github/pull_request_template.md`):**

```markdown
## 🔗 Verknüpfte Issues
Closes #[Issue-Nummer]
Related to #[weitere Issues]

## 📋 Zusammenfassung der Änderungen
<!-- Was wurde geändert und warum? -->

## 🏗️ Architektonische Auswirkungen
- [ ] Keine Architekturänderungen
- [ ] ADR wurde erstellt/aktualisiert: [ADR-NNNN]
- [ ] CODEOWNERS wurden informiert

## ✅ Selbst-Checkliste (Agent)
- [ ] Tests hinzugefügt/aktualisiert
- [ ] Dokumentation aktualisiert
- [ ] Issue-Status auf `status:review` gesetzt
- [ ] Relevante Agenten via @mention benachrichtigt
- [ ] Breaking Changes in Changelog dokumentiert

## 📊 Test-Nachweis
<!-- Screenshots, Testergebnisse, Metriken -->

## 🤖 Agenten-Notizen
<!-- Automatisch generierte Informationen durch AI-Agenten -->
```

### 5.3 GitHub Projects als Blackboard-Dashboard

```
PROJECT BOARD – Spaltenstruktur (Zustandsmaschine):

┌──────────┬────────────┬─────────────┬──────────┬──────────┬──────────┐
│  Backlog │  Triaged   │ In Progress │ Blocked  │ Review   │   Done   │
├──────────┼────────────┼─────────────┼──────────┼──────────┼──────────┤
│ #Issue   │ #Issue     │ #Issue      │ #Issue   │ #PR      │ #Issue   │
│ (NEU)    │ (bewertet) │ (aktiv)     │ (wartet) │ (geprüft)│ (fertig) │
└──────────┴────────────┴─────────────┴──────────┴──────────┴──────────┘

AUTOMATISCHE REGELN:
- Issue mit `status:in-progress` → Spalte "In Progress"
- PR geöffnet, linked zu Issue → Issue in "Review"
- PR gemergt → Issue in "Done", automatisch schließen
- Label `status:blocked` → Spalte "Blocked", @mention auf Blocker-Issue
```

---

## 6. GitHub as Trigger

### 6.1 Event-Getriebene Agentenaktivierung

GitHub Actions bildet die reaktive Schicht des Systems. Agenten werden durch **präzise definierte Events** aktiviert.

#### Trigger-Matrix

```
EVENT                        →  AKTIVIERTER AGENT           →  AKTION
─────────────────────────────────────────────────────────────────────
issues.opened                →  Triage-Agent                →  Labeln, Assignen, ADR prüfen
issues.labeled (type:risk)   →  Architect-Agent             →  Risikobewertung kommentieren
pull_request.opened          →  QA-Agent                    →  Automated Review starten
pull_request.review_requested→  Reviewer-Agent              →  Review-Checkliste kommentieren
push to main                 →  Ops-Agent                   →  Deployment-Issue öffnen
issue_comment (@agent:X)     →  Agent X                     →  Direktnachricht verarbeiten
schedule (täglich 08:00)     →  Status-Agent                →  Daily-Summary in Discussion
milestone.closed             →  Release-Agent               →  Release-Issue & Changelog
workflow_run.failure         →  Ops-Agent                   →  Incident-Issue öffnen
```

### 6.2 GitHub Agentic Workflows (Technical Preview 2026)

Workflows werden in **natürlicher Sprache als Markdown** verfasst:

**`.github/workflows/triage-agent.md`:**

```markdown
# Issue Triage Agent

## Trigger
Aktiviere diesen Workflow wenn ein neues Issue geöffnet wird.

## Ziel
Analysiere das neue Issue und stelle sicher, dass es vollständig
triagiert ist: Labels gesetzt, Zuständigkeit geklärt, Verlinkungen
zu bestehenden Issues hergestellt.

## Schritte
1. Lese den Issue-Inhalt und klassifiziere den Typ
2. Setze passende Labels (type:*, priority:*, agent:*)
3. Prüfe ob ähnliche Issues bereits existieren und verlinke diese
4. Weise den Issue dem zuständigen Agenten zu
5. Kommentiere mit einem Triage-Summary und nächsten Schritten
6. Aktualisiere das Project Board

## Ausgabe (Safe Outputs)
- Issue-Labels setzen
- Issue kommentieren mit Triage-Summary
- Issue assignen
```

### 6.3 IssueOps-Muster: Issues als Trigger

Das **IssueOps-Pattern** nutzt Issues direkt als Steuerungsbefehle:

```markdown
MUSTER: Slash-Commands in Issue-Kommentaren

/deploy staging           → Ops-Agent startet Deployment nach Staging
/review @agent:qa         → QA-Agent wird zum Review aktiviert  
/adr create               → Architect-Agent erstellt ADR-Entwurf
/risk assess              → Risk-Agent führt Risikobewertung durch
/status update            → Status-Agent aktualisiert Projekt-Dashboard
/spike investigate        → Dev-Agent öffnet Spike-Issue
```

**Implementierung via GitHub Actions:**

```yaml
# .github/workflows/issueops.yml
name: IssueOps Command Router
on:
  issue_comment:
    types: [created]

jobs:
  route-command:
    runs-on: ubuntu-latest
    steps:
      - name: Parse Command
        id: parse
        run: |
          COMMENT="${{ github.event.comment.body }}"
          if [[ "$COMMENT" == /deploy* ]]; then
            echo "command=deploy" >> $GITHUB_OUTPUT
          elif [[ "$COMMENT" == /adr* ]]; then
            echo "command=adr" >> $GITHUB_OUTPUT
          fi
      
      - name: Trigger Agent via Repository Dispatch
        uses: peter-evans/repository-dispatch@v3
        with:
          event-type: ${{ steps.parse.outputs.command }}-requested
          client-payload: |
            {
              "issue_number": "${{ github.event.issue.number }}",
              "actor": "${{ github.event.comment.user.login }}",
              "args": "${{ github.event.comment.body }}"
            }
```

---

## 7. Agenten-Kommunikationsprotokoll

### 7.1 Das RACE-Format für Issue-Kommentare

Jeder Agenten-Kommentar folgt dem **RACE-Format**:

```
R – Role:     Welcher Agent schreibt? [Agent-Rolle in eckigen Klammern]
A – Action:   Was wurde getan oder entschieden?
C – Context:  Warum? Verlinkung auf relevante Issues/ADRs/PRs
E – Expect:   Was wird als nächstes erwartet? Von wem?
```

**Beispiel-Kommentar eines QA-Agenten:**

```markdown
**[QA-Agent]** Automatischer Test-Report

**Action:** Unit-Tests für Komponente `AuthService` abgeschlossen.
12/12 Tests grün. Integration-Tests ausstehend.

**Context:** Implementierung aus #234 (Feature: OAuth-Integration),
basierend auf Architekturentscheidung ADR-0018.
Test-Suite: [Link zu CI-Run]

**Expect:** @agent:ops bitte Staging-Deployment starten.
Integration-Tests benötigen Live-Environment.
Deadline: 2026-03-10 EOD

---
*Status-Update: Label `status:review` → `status:integration-test`*
```

### 7.2 Verlinkungsprotokoll (PFLICHT)

**Jede Agentennachricht muss mindestens eine der folgenden Verlinkungen enthalten:**

```
VERLINKUNGSTYPEN:
┌──────────────────────────────────────────────────────────────────┐
│ Closes #NNN      │ Issue wird durch diese Aktion geschlossen     │
│ Fixes #NNN       │ Bug wird durch diese Aktion behoben           │
│ Related to #NNN  │ Inhaltlicher Bezug                            │
│ Blocked by #NNN  │ Abhängigkeit/Blocker                          │
│ Supersedes #NNN  │ Ersetzt vorherigen Issue                      │
│ See ADR-NNNN     │ Referenz auf Architekturentscheidung          │
│ @mention         │ Direkte Adressierung eines Agenten/Persons    │
└──────────────────────────────────────────────────────────────────┘
```

### 7.3 Asynchrone Nachrichten-Kaskade

```
KOMMUNIKATIONSKASKADE (vollständig verlinkt):

Produktmanager erstellt Feature-Anfrage
  └── Issue #300 [type:feature, priority:high]
      ├── Architect-Agent kommentiert Architekturanalyse → ADR-0051 erstellt
      │   └── ADR-0051 (PR #301) referenziert Issue #300
      ├── Dev-Agent öffnet Sub-Issue #302 (Backend)
      │   ├── PR #310 "Closes #302, Related to #300"
      │   └── PR #310 Review-Kommentar → @agent:qa aktiviert
      ├── Dev-Agent öffnet Sub-Issue #303 (Frontend)
      │   └── PR #315 "Closes #303, Related to #300"
      ├── QA-Agent kommentiert Test-Plan in #300
      │   └── Test-Issue #320 "Related to #300"
      └── Ops-Agent kommentiert Deployment-Plan
          └── Issue #325 "Deployment für #300, nach PR #310 + #315"

VOLLSTÄNDIGE CHRONIK jederzeit via: Issues #300 Timeline
```

---

## 8. Nachvollziehbare Chronik & Entscheidungsdokumentation

### 8.1 Decision Log als lebendiges Dokument

**`docs/decisions/DECISION-LOG.md`** – automatisch aktualisiert:

```markdown
# Entscheidungschronik

| Datum | ID | Entscheidung | Entscheider | Status | Links |
|-------|-----|--------------|-------------|--------|-------|
| 2026-03-01 | ADR-0051 | OAuth2 statt API-Keys | @architect | Accepted | #300, PR#301 |
| 2026-02-28 | ADR-0050 | PostgreSQL für Session-Store | @architect, @ops | Accepted | #287 |
| 2026-02-20 | ADR-0049 | Microservices-Schnitt Auth/User | @architect | Accepted | #270, #271 |
```

### 8.2 Daily Context Summary (Automatisiert)

**GitHub Actions Schedule → GitHub Discussion:**

```yaml
# .github/workflows/daily-summary.yml
name: Daily Status Summary
on:
  schedule:
    - cron: '0 7 * * 1-5'  # Mo-Fr, 07:00 UTC

jobs:
  create-summary:
    runs-on: ubuntu-latest
    steps:
      - name: Generate Summary via Copilot Agent
        # Agent analysiert: offene Issues, gestrige PRs,
        # neue ADRs, blockierte Items, anstehende Deadlines
        # und postet strukturierten Tagesbericht in Discussions
```

**Output-Format der Daily Summary:**

```markdown
## 📊 Daily Stand-Up Summary – 2026-03-07

### ✅ Gestern abgeschlossen
- PR #315 gemergt: Frontend OAuth-Integration (#303) – @dev-agent
- ADR-0051 akzeptiert: OAuth2-Entscheidung – @architect-agent

### 🔄 Heute in Arbeit
- #302: Backend OAuth-Service (In Progress, @dev-agent, 70%)
- #320: Integration-Test-Suite (In Progress, @qa-agent)

### 🚫 Blockierungen
- #325: Staging-Deployment wartet auf #302 (ETA: heute 16:00)

### ⚠️ Risiken & Aufmerksamkeit
- PR #310 seit 2 Tagen ohne Review → @architect-agent bitte prüfen

### 📅 Kommende Deadlines
- Sprint-Ende: 2026-03-10 (3 Tage)
- Release v2.1: 2026-03-15 (8 Tage)
```

### 8.3 Unveränderliche Audit-Chronik

GitHub bietet eine **native, unveränderliche Chronik** durch:

- **Commit-History:** Jede Code-Änderung mit Autor, Zeitstempel, verknüpftem Issue
- **Issue-Timeline:** Vollständige Ereignisfolge (Labels, Assignments, Cross-References)
- **PR-Review-History:** Alle Review-Kommentare, Genehmigungen, Ablehnungen
- **Audit-Log (Enterprise):** Alle Organisations-Aktionen inkl. Agenten-Aktivitäten

---

## 9. Standards & Konventionen

### 9.1 Branch-Naming-Convention

```
SCHEMA: [typ]/[issue-nummer]-[kurzbeschreibung]

BEISPIELE:
feature/300-oauth-integration
bugfix/321-session-timeout
spike/298-performance-analysis
ops/325-staging-deployment
adr/0051-oauth-decision
```

### 9.2 Commit-Message-Standard (Conventional Commits)

```
SCHEMA: [typ]([scope]): [beschreibung] (#[issue])

PFLICHTFELDER:
- typ: feat | fix | docs | refactor | test | chore | ci | adr
- scope: betroffene Komponente/Modul
- issue: verlinktes Issue (PFLICHT)

BEISPIELE:
feat(auth): implement OAuth2 token refresh (#302)
fix(session): resolve timeout on inactive users (#321)
docs(adr): add ADR-0051 OAuth2 decision (#300)
test(auth): add integration tests for OAuth flow (#320)

BREAKING CHANGES:
feat(api)!: redesign authentication endpoints (#300)

BREAKING CHANGE: /auth/login signature changed,
see ADR-0051 and migration guide in #300
```

### 9.3 Label-Taxonomie (vollständig)

```yaml
# .github/labels.yml
labels:
  # Typ
  - name: "type:feature"
    color: "0075ca"
  - name: "type:bug"
    color: "d73a4a"
  - name: "type:decision"
    color: "e4e669"
  - name: "type:risk"
    color: "B60205"
  - name: "type:spike"
    color: "c5def5"
  - name: "type:ops"
    color: "f9d0c4"
  - name: "type:communication"
    color: "bfd4f2"
  
  # Status
  - name: "status:new"
    color: "ffffff"
  - name: "status:triaged"
    color: "ededed"
  - name: "status:in-progress"
    color: "fbca04"
  - name: "status:blocked"
    color: "e11d48"
  - name: "status:review"
    color: "7057ff"
  - name: "status:done"
    color: "0e8a16"
  
  # Priorität
  - name: "priority:critical"
    color: "B60205"
  - name: "priority:high"
    color: "e4e669"
  - name: "priority:medium"
    color: "0075ca"
  - name: "priority:low"
    color: "cfd3d7"
  
  # Agent-Zuweisung
  - name: "agent:architect"
    color: "5319e7"
  - name: "agent:developer"
    color: "0075ca"
  - name: "agent:qa"
    color: "006b75"
  - name: "agent:ops"
    color: "e4e669"
```

### 9.4 Kommunikations-SLA

```
ANTWORTZEITEN (Agenten-SLA):
┌──────────────────────────────────────────────────────────────────┐
│ priority:critical  │ ≤ 30 Minuten     │ Sofortiger Trigger      │
│ priority:high      │ ≤ 4 Stunden      │ Automatischer Reminder  │
│ priority:medium    │ ≤ 1 Werktag      │ Daily Summary           │
│ priority:low       │ ≤ 3 Werktage     │ Weekly Summary          │
│ Blockade-Meldung   │ ≤ 1 Stunde       │ @mention + Label        │
└──────────────────────────────────────────────────────────────────┘
```

---

## 10. Repository-Struktur & Dateiorganisation

```
REPOSITORY ROOT
├── agents.md                          ← PFLICHT: Agenten-Manifest & Instruktionen
├── CODEOWNERS                         ← Automatische Review-Zuweisung
├── CHANGELOG.md                       ← Automatisch generiert
│
├── .github/
│   ├── workflows/                     ← GitHub Actions (Trigger-Schicht)
│   │   ├── triage-agent.md            ← Agentic Workflow: Auto-Triage
│   │   ├── daily-summary.yml          ← Scheduled: Daily Stand-Up
│   │   ├── pr-review-agent.md         ← Agentic Workflow: Auto-Review
│   │   ├── issueops.yml               ← Slash-Command Router
│   │   ├── risk-monitor.yml           ← Kontinuierliche Risikobewertung
│   │   └── release-agent.md           ← Agentic Workflow: Release
│   ├── ISSUE_TEMPLATE/
│   │   ├── feature.md                 ← Feature-Request Template
│   │   ├── bug.md                     ← Bug-Report Template
│   │   ├── decision.md                ← Entscheidungs-Template
│   │   ├── risk.md                    ← Risiko-Meldung Template
│   │   └── spike.md                   ← Spike-Investigation Template
│   ├── pull_request_template.md       ← PR-Template
│   └── copilot-instructions.md        ← Copilot-Kontext für das Projekt
│
├── docs/
│   ├── adr/                           ← Architecture Decision Records
│   │   ├── ADR-0001-record-format.md  ← ADR über ADRs (Meta)
│   │   └── ADR-NNNN-[titel].md
│   ├── decisions/
│   │   └── DECISION-LOG.md            ← Konsolidiertes Entscheidungsregister
│   ├── architecture/
│   │   ├── system-overview.md
│   │   └── component-map.md
│   └── runbooks/                      ← Ops-Runbooks (agent-lesbar)
│
└── src/                               ← Quellcode
```

---

## 11. Rollen & Verantwortlichkeiten der Agenten

### 11.1 Agenten-Roster

```
┌──────────────────────────────────────────────────────────────────────┐
│ AGENT              │ PROAKTIVE PFLICHTEN                             │
├────────────────────┼─────────────────────────────────────────────────┤
│ 🏗️ Architect-Agent │ • ADR bei jeder arch. Entscheidung erstellen    │
│                    │ • Risiko-Issues proaktiv öffnen                 │
│                    │ • Wiki-Architekturseiten aktuell halten         │
│                    │ • PR-Reviews für strukturelle Änderungen        │
├────────────────────┼─────────────────────────────────────────────────┤
│ 💻 Dev-Agent       │ • Feature-Branches mit verlinkten Issues        │
│                    │ • PR-Beschreibungen nach Template               │
│                    │ • Sub-Issues für komplexe Features erstellen    │
│                    │ • Blocker sofort mit Label melden               │
├────────────────────┼─────────────────────────────────────────────────┤
│ 🧪 QA-Agent        │ • Test-Plan in Feature-Issue kommentieren       │
│                    │ • Test-Coverage-Issues bei Unterschreitung      │
│                    │ • Review-Kommentare mit Testergebnissen         │
│                    │ • Qualitäts-Report im wöchentlichen Rhythmus    │
├────────────────────┼─────────────────────────────────────────────────┤
│ ⚙️ Ops-Agent       │ • Deployment-Issue vor jedem Release            │
│                    │ • Monitoring-Alert → sofort Incident-Issue      │
│                    │ • Infrastruktur-Änderungen als Issues           │
│                    │ • Post-Mortem nach Incidents                    │
├────────────────────┼─────────────────────────────────────────────────┤
│ 🎯 Triage-Agent    │ • Alle neuen Issues innerhalb SLA labeln        │
│                    │ • Duplikate erkennen und verlinken              │
│                    │ • Zuständigkeiten klären und assignen           │
│                    │ • Daily Summary generieren                      │
└──────────────────────────────────────────────────────────────────────┘
```

### 11.2 Human-in-the-Loop Pflichtpunkte

Trotz maximaler Automatisierung gibt es **unveränderliche menschliche Kontrollpunkte**:

```
MENSCHLICHE ENTSCHEIDUNG PFLICHT bei:
  1. Merge in main/production-Branch
  2. Akzeptieren eines ADR (status: Accepted)
  3. Schließen eines priority:critical Issues
  4. Änderungen an agents.md oder Kommunikationsregeln
  5. Incident-Klassifikation als P0/P1
  6. Release-Freigabe
```

---

## 12. Sicherheit, Governance & Guardrails

### 12.1 Permission-Modell für Agenten

```
GITHUB PERMISSIONS (Least Privilege Principle):
┌──────────────────────────────────────────────────────────────────┐
│ AGENTIC WORKFLOW (Standard)                                      │
│   Read:  Repository, Issues, PRs, Actions                       │
│   Write: Issue-Kommentare, Labels (via Safe Outputs)             │
│   NEVER: Direkter Push, Branch-Deletion, Settings               │
├──────────────────────────────────────────────────────────────────┤
│ GITHUB ACTIONS (Workflow)                                        │
│   Read:  Code, Issues                                            │
│   Write: Pull Requests erstellen, Issues kommentieren           │
│   NEVER: Merge ohne Review, Secrets-Zugriff ohne Approval        │
└──────────────────────────────────────────────────────────────────┘
```

### 12.2 CODEOWNERS für kritische Pfade

```
# .github/CODEOWNERS

# Architektur-Dokumente → Architect-Freigabe Pflicht
/docs/adr/           @architect-lead
/agents.md           @architect-lead @project-lead

# Security-relevanter Code → Security-Review Pflicht
/src/auth/           @security-team @architect-lead

# Infrastruktur → Ops-Freigabe Pflicht
/.github/workflows/  @ops-team @architect-lead
/infra/              @ops-team
```

### 12.3 Branch-Protection-Rules

```yaml
# Pflicht-Regeln für main:
branch_protection:
  main:
    required_status_checks:
      - "ci/tests"
      - "ci/lint"
      - "agent/security-scan"
    required_pull_request_reviews:
      required_approving_review_count: 1
      dismiss_stale_reviews: true
      require_code_owner_reviews: true
    enforce_admins: false          # Auch Admins brauchen PRs
    restrictions: null
    allow_force_pushes: false      # NIEMALS Force-Push auf main
    allow_deletions: false
```

---

## 13. Implementierungs-Roadmap

### Phase 1: Fundament (Woche 1–2)
- [ ] Repository-Struktur gemäß Spezifikation aufsetzen
- [ ] `agents.md` erstellen und mit Team abstimmen
- [ ] Label-System vollständig konfigurieren
- [ ] Issue-Templates implementieren
- [ ] PR-Template implementieren
- [ ] Branch-Protection-Rules aktivieren
- [ ] CODEOWNERS konfigurieren
- [ ] Erstes ADR (ADR-0001: Kommunikationssystem) erstellen

### Phase 2: Automatisierung (Woche 3–4)
- [ ] Triage-Agent als GitHub Action implementieren
- [ ] Daily-Summary-Workflow implementieren
- [ ] IssueOps-Command-Router implementieren
- [ ] Project-Board mit Automatisierungsregeln aufsetzen
- [ ] GitHub MCP-Server integrieren

### Phase 3: Agentic Workflows (Woche 5–6)
- [ ] PR-Review-Agent als Agentic Workflow
- [ ] Risk-Monitor-Workflow implementieren
- [ ] Release-Agent implementieren
- [ ] SLA-Monitoring und Reminder aktivieren

### Phase 4: Optimierung (laufend)
- [ ] ADR-Qualität messen und Templates verfeinern
- [ ] Agent-Performance-Metriken etablieren
- [ ] Quarterly Review: Kommunikationsqualität
- [ ] Kontinuierliches Onboarding neuer Agenten

---

## 14. Referenzen & Best Practices

### Direkt angewendete Patterns

| Pattern | Quelle | Anwendung |
|---------|--------|-----------|
| Blackboard Pattern | KI-Systemarchitektur (classic) | Issues als gemeinsamer Zustandsraum |
| IssueOps | GitHub Best Practices 2025 | Slash-Commands als Agenten-Trigger |
| Conventional Commits | commitlint.io | Verlinktes Commit-Format |
| ADR (Architecture Decision Records) | Michael Nygard | Entscheidungsdokumentation |
| CODEOWNERS | GitHub Docs | Automatische Review-Zuweisung |
| Agentic Workflows | GitHub Next (Technical Preview 2026) | Natural Language Automation |
| Agent HQ | GitHub Universe 2025 | Zentrales Agenten-Steuerungskonzept |
| RACE-Format | SPO Best Practices | Strukturierte Agentennachrichten |

### Weiterführende Ressourcen

- [GitHub Agentic Workflows (Technical Preview)](https://github.blog/changelog/2026-02-13-github-agentic-workflows-are-now-in-technical-preview/)
- [How to write a great agents.md](https://github.blog) — Lessons from 2,500+ repositories
- [Microsoft Multi-Agent Reference Architecture](https://github.com/microsoft/multi-agent-reference-architecture)
- [Blackboard Pattern mit MCP](https://medium.com/@dp2580/building-intelligent-multi-agent-systems-with-mcps-and-the-blackboard-pattern)
- [GitHub Agent HQ — Universe 2025](https://github.blog/news-insights/company-news/welcome-home-agents/)
- [Four Design Patterns for Event-Driven Multi-Agent Systems](https://www.confluent.io/blog/event-driven-multi-agent-systems/)

---

*Dokument-Version: 1.0 | Stand: März 2026 | Erstellt gemäß SPO-Architektur-Standards*  
*Nächste Review: Quarterly (Juni 2026)*  
*Zuständig: @architect-lead | Genehmigt durch: [Projektleitung]*
