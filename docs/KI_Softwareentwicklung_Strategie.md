# Strategie: Multi-Agenten-Softwareentwicklung mit LLMs
## Von Vibe-Coding zu Agentic Engineering

> **Grundprinzip:** Jede Aufgabe wird nicht einmal ausgeführt, sondern so lange geschleift, bis sie deterministisch korrekt ist.  
> Quelle: TDD Governance for Multi-Agent Code Generation (Hasanli et al., EASE 2026, arXiv:2604.26615)

---

## 1. Das Kernproblem: Warum naive KI-Entwicklung scheitert

Aus der Forschung (Ouyang et al., 2025) wissen wir:

- **Bis zu 75,76 % aller LLM-Outputs auf komplexen Benchmarks sind nicht reproduzierbar** — identische Prompts liefern unterschiedliche Ergebnisse, selbst bei Temperature=0
- **Fehler pflanzen sich fort**: In Multi-Agenten-Systemen eskaliert ein kleiner Logikfehler durch die gesamte Pipeline
- **Context Window als wichtigste Ressource**: Wenn der Kontext voll ist, „vergisst" das Modell frühere Instruktionen und macht mehr Fehler
- **Ohne strukturierte Governance** verhalten sich LLMs wie ein sehr schneller Junior-Entwickler ohne Code-Reviews

**Lösung:** Strukturierte Schleifen + TDD als Durchsetzungsmechanismus + spezialisierte Agenten-Rollen.

---

## 2. Das Architekturmodell: Hierarchische Schleifen

```
╔══════════════════════════════════════════════════════════════════╗
║                    STRATEGIE-EBENE                               ║
║  Human-in-the-Loop (Product Owner / Lead Dev)                    ║
║  Entscheidung: Akzeptanz, Scope, Priorisierung                   ║
╠══════════════════════════════════════════════════════════════════╣
║                    ORCHESTRATOR-EBENE                            ║
║  Haupt-Agent: Planer, Delegator, Validator                       ║
║  Schleife: Plan → Delegate → Evaluate → Accept/Retry             ║
╠══════════════════════════════════════════════════════════════════╣
║  SPEZIALISIERTE SUBAGENTEN (parallele Schleifen)                 ║
║  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐           ║
║  │Architect │ │ Coder    │ │ Tester   │ │Reviewer  │           ║
║  │(Planner) │ │(Builder) │ │(QA)      │ │(Critic)  │           ║
║  └──────────┘ └──────────┘ └──────────┘ └──────────┘           ║
╠══════════════════════════════════════════════════════════════════╣
║                    DETERMINISTISCHE EBENE                        ║
║  Test-Runner, Linter, Build-System, CI/CD                        ║
║  → Die einzige Ebene mit 100% deterministischer Aussage          ║
╚══════════════════════════════════════════════════════════════════╝
```

**Schlüsselprinzip:** Modelle *schlagen vor*, deterministische Engines *entscheiden*.  
Kein Agent committet Code, der nicht durch die deterministische Ebene validiert wurde.

---

## 3. Die vier Schleifen-Typen

### Schleife 1: Die Mikro-Schleife (Red-Green-Refactor, < 5 min)

Innerhalb eines einzelnen Coding-Agenten. Direkt aus TDD:

```
┌─────────────────────────────────────────────────────┐
│                   RED PHASE                          │
│  Test-Agent schreibt scheiternden Test               │
│  Validierung: Test MUSS rot sein (Assertion)         │
│  → Wenn Test grün ist: Test ist fehlerhaft! STOP     │
└─────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────┐
│                   GREEN PHASE                        │
│  Code-Agent schreibt minimalen Code zum Bestehen     │
│  Max. 3 Repair-Iterationen (bounded loop!)           │
│  → Bei 3x Fehler: Eskalation zum Orchestrator        │
└─────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────┐
│                 REFACTOR PHASE                       │
│  Reviewer-Agent prüft Code-Qualität                  │
│  Nur strukturelle Änderungen, keine neuen Features   │
│  Validierung: Alle Tests bleiben grün                │
└─────────────────────────────────────────────────────┘
```

**Kritisch:** Bounded Repair Loops — maximal N Versuche, dann Eskalation. Ohne diese Grenze läuft das System endlos.

---

### Schleife 2: Die Feature-Schleife (Explore-Plan-Implement-Commit, < 2 h)

Basierend auf Anthropic Claude Code Best Practices (code.claude.com/docs):

```
EXPLORE  →  PLAN  →  IMPLEMENT  →  VERIFY  →  COMMIT
   ↑                                    |
   └────────────────────────────────────┘
         (Wenn Verifikation fehlschlägt)
```

**Phasen-Details:**

| Phase | Agent(en) | Tool | Gate |
|-------|-----------|------|------|
| Explore | Architect-Agent | Plan Mode (nur lesen!) | Architektur-Dokument erstellt |
| Plan | Architect + Orchestrator | PRD + Implementierungsplan | Human Review oder Auto-Gate |
| Implement | Coder-Agent(en) | Code-Editing | Tests grün, Linter sauber |
| Verify | Tester + Reviewer | Test Suite + Code Review | 100% definierte Tests grün |
| Commit | Orchestrator | Git | CI/CD Pipeline grün |

---

### Schleife 3: Die Session-Schleife (Context Reset, < 1 Tag)

Das schwächste Glied in langen Coding-Sessions ist der sich füllende Context Window.

```
Session Start
     ↓
Kontext laden (CLAUDE.md + relevante Dateien)
     ↓
Feature-Schleife (1-3 Features)
     ↓
Context-Checkpoint: Ist Context >70% voll?
     ├─ Nein → Weiter mit nächstem Feature
     └─ Ja  → Context Reset!
              ├─ Ergebnisse dokumentieren (NOTES.md)
              ├─ Git-Commit als Checkpoint
              └─ Neue Session mit frischem Kontext starten
```

**Warum:** Laut Anthropic-Dokumentation degradiert die Performance messbar, wenn der Context Window voll läuft. Frische Sessions mit gut strukturierten CLAUDE.md-Dateien performen besser als erschöpfte lange Sessions.

---

### Schleife 4: Die Projekt-Schleife (Sprint/Epic-Ebene, Wochen)

```
Epic/Requirement
     ↓
Architecture Review (Human)
     ↓
Feature Decomposition (Orchestrator)
     ↓
Parallele Feature-Schleifen (mehrere Agenten)
     ↓
Integration Testing
     ↓
Human Acceptance
     ↓
Production Release → Monitoring → nächstes Epic
```

---

## 4. Agenten-Rollen: System Prompts & Verantwortlichkeiten

### 4.1 Der Orchestrator-Agent

**Systemrolle:** Planer, Delegator, Qualitätsgatter. Dieser Agent schreibt **keinen** Code.

```markdown
# Orchestrator System Prompt Template

Du bist ein Senior Software Engineering Orchestrator.
Deine einzige Aufgabe ist Planung, Delegation und Qualitätskontrolle.

## ABSOLUT VERBOTEN
- Schreibe selbst keinen Produktionscode
- Implementiere keine Features direkt
- Überspringe niemals die Verifikationsphase

## PFLICHTEN
1. Zerteile jede Aufgabe in atomare, unabhängige Subtasks
2. Definiere für jede Subtask explizite Akzeptanzkriterien BEVOR du delegierst
3. Validiere das Ergebnis gegen die Akzeptanzkriterien
4. Eskaliere an den Menschen bei: Sicherheitsproblemen, >3 Fehlversuchen, Scope-Änderungen

## AUSGABEFORMAT
Für jede delegierte Aufgabe:
```json
{
  "task_id": "unique-id",
  "agent": "coder|tester|reviewer|architect",
  "description": "Präzise Aufgabenbeschreibung",
  "acceptance_criteria": ["Kriterium 1", "Kriterium 2"],
  "context_files": ["file1.ts", "file2.ts"],
  "constraints": ["Keine externen Libraries", "Max 50 Zeilen"],
  "max_repair_attempts": 3
}
```
```

---

### 4.2 Der Architect-Agent (Planner)

**Systemrolle:** Analyse, Entwurf, keine Code-Implementierung.

```markdown
# Architect Agent System Prompt Template

Du bist ein erfahrener Software-Architekt im EXPLORE-ONLY Modus.

## AUFGABEN
- Analysiere den bestehenden Code OHNE Änderungen vorzunehmen
- Identifiziere alle betroffenen Dateien und Abhängigkeiten
- Erstelle einen konkreten Implementierungsplan mit Dateiliste
- Erkenne potenzielle Risiken (Breaking Changes, Race Conditions, etc.)

## AUSGABEFORMAT: Immer als strukturierter Plan
### Betroffene Dateien
- `src/auth/login.ts` → Änderung: Session-Token-Handling erweitern
- `src/types/User.ts` → Änderung: Neues Feld `oauth_provider` hinzufügen

### Implementierungsschritte (geordnet nach Abhängigkeiten)
1. Types definieren (keine Abhängigkeiten)
2. Repository-Schicht updaten (benötigt: Types)
3. Service-Schicht updaten (benötigt: Repository)
4. Tests schreiben (benötigt: alle obigen)

### Risiken
- Breaking Change: User.ts Änderung betrifft 12 Stellen

## STOPPE wenn
- Eine Anfrage unklar ist → frage nach
- Scope zu groß ist → schlage Decomposition vor
```

---

### 4.3 Der Coder-Agent (Builder)

**Systemrolle:** Implementierung, streng nach Plan.

```markdown
# Coder Agent System Prompt Template

Du bist ein präziser Implementierer. Du folgst dem Plan, du erweiterst ihn nicht.

## PFLICHTEN
- Implementiere NUR was im Plan steht
- Schreibe minimalen Code (keine vorauseilende Optimierung)
- Folge den Projektkonventionen in CLAUDE.md
- Führe nach jeder Änderung die Testsuite aus

## VERBOTEN
- Keine ungeplanten Refactorings
- Keine neuen Abhängigkeiten ohne Rückfrage
- Kein "während ich hier bin..." erweitern von Scope
- Keine Suppression von Fehlern/Warnings

## VERIFIKATIONSPFLICHT
Nach jeder Implementierung:
1. `npm run typecheck` (oder äquivalent) → muss grün sein
2. Betroffene Tests ausführen → müssen grün sein
3. Linter → muss grün sein

## BEI FEHLERN
- Versuch 1: Analysiere Fehlermeldung, fixe die Ursache
- Versuch 2: Überprüfe Annahmen, überdenke Ansatz
- Versuch 3: Erkläre das Problem detailliert, eskaliere an Orchestrator
```

---

### 4.4 Der Tester-Agent (QA)

**Systemrolle:** Tests schreiben VOR dem Code (TDD), Qualitätsgatter.

```markdown
# Tester Agent System Prompt Template

Du bist ein Quality-Assurance-Experte. Du schreibst Tests BEVOR Code existiert.

## TDD-PFLICHT (Red-Green-Refactor)
1. Schreibe zuerst den Test → er MUSS rot sein
2. Verifiziere, dass der Test rot ist (führe ihn aus!)
3. Übergib dann an Coder-Agent
4. Verifiziere nach Implementierung: Test ist grün

## TESTTYPEN (nach Priorität)
1. Unit Tests: Jede öffentliche Funktion, Edge Cases, Fehlerpfade
2. Integration Tests: API-Grenzen, Datenbankinteraktionen
3. Contract Tests: Schnittstellen zwischen Services
4. E2E Tests: Kritische User Journeys (wenige, aber wichtige)

## TESTQUALITÄT
- Kein Mocking von Implementierungsdetails, nur von externen Grenzen
- Teste Verhalten, nicht Implementierung
- AAA-Struktur: Arrange → Act → Assert
- Jeder Test genau eine Assertion (wo möglich)
- Verständliche Fehlermeldungen bei Testfehlern

## ACCEPTANCE CRITERIA MAPPING
Jeder Test muss auf ein Akzeptanzkriterium referenzieren:
```typescript
// AC-001: User kann sich mit Google OAuth einloggen
describe('Google OAuth Login', () => {
  it('should return valid session token for valid OAuth callback', ...);
  it('should reject expired OAuth tokens', ...);
  it('should handle OAuth provider errors gracefully', ...);
});
```
```

---

### 4.5 Der Reviewer-Agent (Critic)

**Systemrolle:** Kritischer Code Review, ausschließlich nach definierten Kriterien.

```markdown
# Reviewer Agent System Prompt Template

Du bist ein kritischer Code Reviewer. Dein Job ist es, Probleme zu finden.

## REVIEW-CHECKLISTE (in dieser Reihenfolge)
### Korrektheit (blockt Merge)
- [ ] Logikfehler vorhanden?
- [ ] Edge Cases unbehandelt?
- [ ] Error Handling vollständig?
- [ ] Race Conditions möglich?

### Sicherheit (blockt Merge)
- [ ] Input-Validierung korrekt?
- [ ] Keine Secrets im Code?
- [ ] SQL/Command-Injection unmöglich?
- [ ] Authentifizierung/Autorisierung korrekt?

### Wartbarkeit (empfohlen, blockt nicht)
- [ ] Verständliche Variablennamen?
- [ ] Komplexe Logik dokumentiert?
- [ ] DRY-Prinzip befolgt?
- [ ] Keine God Functions (>30 Zeilen)?

## AUSGABEFORMAT
```json
{
  "verdict": "APPROVE | REQUEST_CHANGES | BLOCK",
  "blocking_issues": [],
  "warnings": [],
  "suggestions": []
}
```

## WICHTIG: Flag only what matters
Ein Reviewer, der alles als Problem markiert, ist wertlos.
Blockiere NUR bei Korrektheit und Sicherheitsproblemen.
```

---

## 5. Dokumentations-Architektur: Die Markdown-Hierarchie

### 5.1 Verzeichnisstruktur

```
project-root/
│
├── CLAUDE.md              # Haupt-Agent-Kontext (immer geladen)
├── AGENTS.md              # Cross-Tool Standard (Codex, Copilot, etc.)
│
├── .claude/
│   ├── agents/            # Subagenten-Definitionen
│   │   ├── architect.md
│   │   ├── coder.md
│   │   ├── tester.md
│   │   └── reviewer.md
│   ├── commands/          # Wiederverwendbare Workflows als Slash-Commands
│   │   ├── new-feature.md
│   │   ├── bugfix.md
│   │   ├── refactor.md
│   │   └── review-pr.md
│   └── rules/             # Kontextspezifische Regeln (lazy-loaded)
│       ├── api-conventions.md
│       ├── database-patterns.md
│       └── testing-standards.md
│
├── docs/
│   ├── architecture/
│   │   ├── OVERVIEW.md    # System-Architektur (für LLM und Menschen)
│   │   ├── DECISIONS.md   # Architecture Decision Records (ADRs)
│   │   └── DIAGRAMS.md    # Mermaid-Diagramme
│   ├── api/               # Auto-generierte API-Dokumentation
│   └── runbooks/          # Deployment & Operations
│
├── .notes/                # Session-Notizen (nicht committed)
│   └── CURRENT_SESSION.md # Was tue ich gerade? Für Context Reset
│
└── tests/
    ├── README.md          # Test-Strategie für Agenten
    └── fixtures/          # Test-Datensätze
```

---

### 5.2 CLAUDE.md – Das Herzstück

Das CLAUDE.md ist "long-term memory" für den Agenten (Medium, Habib Mrad, 2025). Kurz, präzise, iterativ verfeinert — nie länger als nötig.

```markdown
# [PROJEKTNAME] — Agent Context

## Project Summary
[2-3 Sätze: Was macht das Projekt? Welche Technologien? Welches Problem löst es?]

## Tech Stack
- Language: TypeScript 5.x (strict mode)
- Framework: Next.js 15 (App Router)
- Database: PostgreSQL 16 mit Prisma ORM
- Testing: Vitest + Testing Library
- CI: GitHub Actions

## Architecture Rules (ALWAYS follow)
- Domain-Driven Design: Kein direkter DB-Zugriff aus Route Handlers
- Repository Pattern: Alle DB-Queries in `src/repositories/`
- Error Handling: Immer `Result<T, E>` Typ verwenden, nie throw in Business Logic
- Immutability: Alle State-Objekte sind readonly

## Code Style
- ESLint + Prettier Konfiguration in `.eslintrc.ts` — immer einhalten
- Imports: Named imports bevorzugen, keine Default-Exports außer Pages
- Naming: camelCase für Funktionen, PascalCase für Typen/Klassen

## Test-Strategie (TDD-PFLICHT)
1. Test schreiben → Test muss rot sein → Code schreiben → Test grün
2. Testdatei: `[filename].test.ts` im gleichen Verzeichnis
3. Kein Mock von internen Modulen — nur externe Services mocken
4. `npm run test:unit` nach jeder Änderung ausführen

## Workflow Constraints
- NIEMALS Code commiten ohne grüne Tests
- NIEMALS `any` als TypeScript-Typ verwenden
- NIEMALS TODO-Kommentare im Produktionscode hinterlassen
- Bei Unsicherheit: Stop und frage nach, implementiere nicht "irgendwas"

## Build & Test Commands
```bash
npm run dev          # Development Server
npm run build        # Production Build (MUSS grün sein vor Commit)
npm run test:unit    # Unit Tests (Vitest)
npm run test:e2e     # E2E Tests (Playwright)
npm run typecheck    # TypeScript Check
npm run lint         # ESLint
npm run lint:fix     # ESLint mit Auto-Fix
```

## Current Sprint / Active Work
→ Siehe `.notes/CURRENT_SESSION.md` für aktuelle Session-Details
→ Siehe `docs/architecture/DECISIONS.md` für aktuelle ADRs
```

---

### 5.3 AGENTS.md – Cross-Tool Standard

```markdown
# AGENTS.md
# Cross-tool agent configuration (Claude Code, Codex, Copilot, Cursor, etc.)

## Project Context
[Projektname] ist eine [Typ]-Anwendung für [Zweck].

## Development Standards
- Alle Änderungen erfordern Tests (TDD)
- Code muss TypeScript strict-mode-kompatibel sein
- Kein direkter Datenbankzugriff außerhalb der Repository-Schicht

## Testing Requirements
- Neue Features: Unit Tests und Integration Tests erforderlich
- Bug Fixes: Regression Test der den Bug reproduziert erforderlich
- Mindest-Coverage: 80% für neue Dateien

## Commit Standards
- Format: `type(scope): description` (Conventional Commits)
- Beispiel: `feat(auth): add Google OAuth support`
- Kein Commit ohne grüne CI

## Agent Behavior Rules
- Explore before implementing (read-only phase first)
- Never modify more than 5 files without explicit approval
- Always run the test suite before suggesting a commit
- Escalate to human when: security concerns, scope creep, 3+ failed attempts
```

---

### 5.4 Slash-Commands für wiederkehrende Workflows

**`.claude/commands/new-feature.md`**
```markdown
# New Feature Workflow

## Input
$ARGUMENTS: Feature-Beschreibung in einem Satz

## Workflow
1. **EXPLORE** (Read-Only): Analysiere bestehende Architektur
   - Lies `docs/architecture/OVERVIEW.md`
   - Identifiziere betroffene Dateien
   - Erstelle Liste der Änderungen

2. **PLAN**: Erstelle Implementation Plan
   - Liste aller zu erstellenden/ändernden Dateien
   - Abhängigkeitsreihenfolge
   - Geschätzte Testabdeckung

3. **WARTE** auf Human Approval des Plans

4. **TEST FIRST**: Schreibe failing Tests für alle Akzeptanzkriterien

5. **IMPLEMENT**: Implementiere minimal den Code, der Tests grün macht

6. **VERIFY**: 
   ```bash
   npm run typecheck && npm run lint && npm run test:unit
   ```

7. **COMMIT**: Conventional Commit mit Referenz auf Plan
```

---

## 6. TDD Governance: Die Enforcement-Regeln

Basierend auf arXiv:2604.26615 (Hasanli et al., 2026):

### 6.1 Maschinen-lesbare TDD-Manifesto (in CLAUDE.md einbetten)

```markdown
## TDD Manifesto (machine-enforceable)

### Phase Ordering (STRICT)
RULE_01: Tests MUST be written BEFORE implementation code
RULE_02: Test MUST fail (red) before implementation begins  
RULE_03: Implementation MUST be minimal to pass tests
RULE_04: Refactoring ONLY after all tests are green

### Bounded Repair Loops
RULE_05: Maximum 3 repair attempts per failing test
RULE_06: If attempt 3 fails: STOP, explain problem, escalate

### Validation Gates (BLOCKING)
RULE_07: Commit gate: ALL tests green required
RULE_08: PR gate: No regression in test suite
RULE_09: Deploy gate: Full integration test suite green

### Atomic Mutation Control
RULE_10: Change ONLY what is required for current failing test
RULE_11: No refactoring and feature addition in same commit
RULE_12: One PR = One feature/fix (no bundling)
```

### 6.2 Git Hooks als deterministische Gates

**`.git/hooks/pre-commit`** (automatisch durch Agenten eingerichtet)
```bash
#!/bin/sh
# Agent Gate: Kein Commit ohne grüne Tests

echo "🔍 Running pre-commit checks..."

# TypeScript
npm run typecheck || { echo "❌ TypeScript errors"; exit 1; }

# Linter  
npm run lint || { echo "❌ Lint errors"; exit 1; }

# Unit Tests
npm run test:unit || { echo "❌ Unit tests failed"; exit 1; }

echo "✅ All checks passed"
```

**`.github/workflows/ci.yml`** (CI als externen Validator)
```yaml
name: CI

on: [push, pull_request]

jobs:
  quality-gate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Setup
        run: npm ci
      
      - name: TypeScript
        run: npm run typecheck
      
      - name: Lint
        run: npm run lint
      
      - name: Unit Tests
        run: npm run test:unit --coverage
      
      - name: Integration Tests
        run: npm run test:integration
      
      - name: Build
        run: npm run build
      
      - name: Coverage Gate (min 80%)
        run: npm run test:coverage-check
```

---

## 7. Context Engineering: Der richtige Kontext zur richtigen Zeit

Basierend auf arXiv:2508.08322 (Haseeb, 2025):

### 7.1 Das Intent-Translation-Muster

**Niemals vage starten. Immer erst Intent präzisieren:**

```
Vage: "Füge eine Kalenderansicht hinzu"

Intent-Translated:
FEATURE: Interaktiver Kalender auf der Scheduling-Seite
TECHNISCH:
  - Neue Komponente: src/components/CalendarView.tsx
  - Neue API: GET /api/events?date=YYYY-MM&userId=
  - Neue Types: CalendarEvent, CalendarMonth
AKZEPTANZKRITERIEN:
  - AC-01: User sieht Monatsansicht mit Events
  - AC-02: User kann zwischen Monaten navigieren
  - AC-03: Events laden innerhalb 200ms (mit Skeleton)
  - AC-04: Mobile-responsive (min 320px)
CONSTRAINTS:
  - Keine neuen Libraries
  - Muss mit bestehenden User-Auth-Patterns konsistent sein
```

### 7.2 Kontextverwaltung pro Session

```markdown
# .notes/CURRENT_SESSION.md (wird pro Session aktualisiert)

## Session Ziel
[Was soll in dieser Session erreicht werden?]

## Aktueller Status
- [x] Architect-Phase abgeschlossen
- [ ] Tests geschrieben (in Arbeit)
- [ ] Implementation
- [ ] Review

## Betroffene Dateien
- src/components/CalendarView.tsx (NEU)
- src/api/events/route.ts (ÄNDERUNG)
- src/types/Calendar.ts (NEU)

## Offene Entscheidungen
- Soll Caching verwendet werden? (warte auf Antwort)

## Zuletzt Committet
- abc1234: feat(types): add CalendarEvent type definitions

## NEXT (für nächste Session)
- Integration Tests schreiben
- E2E Test für critical path
```

### 7.3 Retrieval-Augmented Context (für komplexe Projekte)

Bei großen Codebases (>50k LOC):

```
Session Start
     ↓
Vector DB durchsuchen nach relevantem Code-Kontext
(ähnliche Patterns, betroffene Tests, API-Dokumentation)
     ↓
Relevante Snippets in CLAUDE.md-Session-Block einfügen
(nicht die gesamte Codebase, nur was relevant ist)
     ↓
Agent arbeitet mit präzisem, relevantem Kontext
```

---

## 8. Parallele Agenten-Koordination

### 8.1 Was parallelisiert werden kann

```
Feature A                Feature B              Feature C
(unabhängig)             (unabhängig)           (abhängig von A)
     │                        │                      │
  Coder-1                  Coder-2              wartet auf A
     │                        │                      │
  Tester-1                 Tester-2                  │
     │                        │                      │
  Review-1                 Review-2                  │
     │                        │                      │
  Merge-A                  Merge-B ──────────────► Coder-3
```

**Regel:** Agenten in parallelen Branches dürfen nie die gleichen Dateien ändern. Der Orchestrator verwaltet File-Locks und Merge-Strategie.

### 8.2 Kommunikationsprotokoll zwischen Agenten

```json
{
  "from": "tester-agent",
  "to": "orchestrator",
  "type": "REPAIR_ESCALATION",
  "payload": {
    "task_id": "feat-auth-001",
    "attempts": 3,
    "failing_test": "src/auth/login.test.ts:42",
    "error": "Expected 401, got 500",
    "hypothesis": "JWT_SECRET env var nicht gesetzt in Test-Environment",
    "suggested_action": "Prüfe .env.test Konfiguration"
  }
}
```

---

## 9. Dokumentation für Menschen und LLMs gleichzeitig

Das Ziel: Jedes Dokument muss von beiden gelesen werden können.

### 9.1 Architecture Decision Records (ADRs)

```markdown
# ADR-001: Repository Pattern für Datenbankzugriff

## Status
Akzeptiert (2024-01-15)

## Kontext
Direkter Datenbankzugriff aus Route Handlers führt zu:
- Nicht testbarem Code (Datenbank-Mocking sehr aufwändig)
- Code-Duplikation bei mehrfacher Nutzung derselben Query
- Schwierigem Austausch der Datenbank (Vendor Lock-in)

## Entscheidung
Alle Datenbankzugriffe laufen über Repository-Klassen in `src/repositories/`.

## Konsequenzen
- (+) Einfaches Mocking in Tests: `mockUserRepository.findById = jest.fn()`
- (+) Single Source of Truth für Query-Logik
- (-) Zusätzliche Abstraktionsschicht (~1 Datei pro Entity)

## Für LLMs: Enforcement
WENN eine Route Handler-Datei direkt `prisma.user.findMany()` aufruft:
→ Das ist eine Architekturverletzung, stoppe und refaktoriere

## Beispiel (korrekte Implementierung)
[code example...]
```

### 9.2 Living Documentation durch Tests

```typescript
/**
 * @module UserAuthentication
 * @description Spezifikation für User-Auth-Verhalten
 * 
 * Diese Tests sind die verbindliche Spezifikation.
 * Der Code muss sich diesen Tests anpassen, nie umgekehrt.
 */
describe('User Authentication', () => {
  describe('Login Flow', () => {
    // AC-001
    it('should issue a session token for valid credentials', async () => {
      // Arrange: Bekannter User in der Datenbank
      // Act: Login mit korrekten Credentials
      // Assert: HTTP 200 mit session_token im Response-Body
    });

    // AC-002  
    it('should reject invalid passwords with 401', async () => { ... });

    // AC-003
    it('should lock account after 5 failed attempts', async () => { ... });
  });
});
```

---

## 10. Die Strategie in der Praxis: Onboarding eines neuen Projekts

### Phase 0: Bootstrap (1 Tag, Human-led)

```bash
# 1. Projekt-Struktur aufsetzen
mkdir my-project && cd my-project
git init

# 2. CLAUDE.md initialisieren
claude /init  # Analysiert Projekt-Struktur automatisch

# 3. Agenten-Definitionen erstellen
mkdir -p .claude/{agents,commands,rules}
# → Erstelle die 5 Agent-Markdown-Dateien aus Abschnitt 4

# 4. TDD-Framework einrichten
npm install -D vitest @testing-library/react
# → Konfiguriere Coverage-Thresholds: 80% minimum

# 5. Git Hooks einrichten
npm install -D husky lint-staged
npx husky install
# → pre-commit hook aus Abschnitt 6.2 einrichten

# 6. CI/CD aufsetzen
# → GitHub Actions Workflow aus Abschnitt 6.2
```

### Phase 1: Erstes Feature (mit dem System, nicht gegen es)

```
1. Intent Translation: Feature-Anforderung präzisieren
2. /new-feature Command: Orchestrator erstellt Plan
3. Human Review: Plan prüfen (< 5 min)
4. Tester-Agent: Failing Tests schreiben
5. Verifizieren: Tests sind rot
6. Coder-Agent: Minimale Implementation
7. Micro-Loop: Tests werden grün
8. Reviewer-Agent: Code Review
9. Deterministische Gate: pre-commit hook
10. Commit und PR
```

### Phase 2: Skalierung auf mehrere Agenten

Erst wenn Phase 1 reibungslos funktioniert, parallele Agenten einführen.

---

## 11. Häufige Fallstricke und Gegenmassnahmen

| Fallstrick | Symptom | Gegenmassnahme |
|------------|---------|----------------|
| **Scope Creep** | Agent implementiert ungeplante Features | Bounded Prompts mit expliziten Constraints; Orchestrator prüft vor Merge |
| **Test-Faking** | Agent macht Test grün durch Hardcoding | Tester-Agent prüft Test-Qualität; Reviewer prüft keine Hardcoded-Returns |
| **Context Overflow** | Agent "vergisst" frühere Instruktionen | Context-Checkpoint-Schleife; regelmäßige Session Resets |
| **Over-Engineering** | 200+ Zeilen statt 20 | Explicit Prompt: "Minimale Lösung", Reviewer flaggt Overengineering |
| **Error Suppression** | `try { ... } catch {}` ohne Handler | Linter-Regel: `no-empty-catch`; Reviewer-Checkliste |
| **Dependency Hell** | Agent installiert beliebige npm-Pakete | CLAUDE.md: "Keine neuen Dependencies ohne Rückfrage" |
| **Test-Lock-In** | Tests testen Implementierungsdetails | Tester-Agent-Prompt: "Teste Verhalten, nicht Implementierung" |

---

## 12. Referenzen (Forschungsgrundlage)

### Direkte Paper-Grundlagen

1. **TDD Governance for Multi-Agent Code Generation** — Hasanli et al., EASE 2026, arXiv:2604.26615  
   → Fundament für Abschnitte 3, 6 (Bounded Repair Loops, Red-Green-Refactor als Governance)

2. **Context Engineering for Multi-Agent LLM Code Assistants** — Haseeb, 2025, arXiv:2508.08322  
   → Fundament für Abschnitte 5, 7 (Intent Translation, Orchestrator State Machine)

3. **A Survey on Code Generation with LLM-based Agents** — Dong et al., 2025, arXiv:2508.00083  
   → Überblick Agenten-Rollen, Multi-Agent-Patterns

4. **The Rise of Agentic Testing** — 2025, arXiv:2601.02454  
   → Agentic QA, Test-Feedback-Loops

### Praktische Quellen

5. **Anthropic Claude Code Official Best Practices** — code.claude.com/docs/en/best-practices  
   → Explore-Plan-Implement-Commit Workflow, CLAUDE.md Design, Context Window Management

6. **AGENTS.md Standard** — Gist: 0xfauzi (2025)  
   → Cross-Tool Agent Configuration Standard

7. **Claude Code Best Practice Repository** — github.com/shanraisshan/claude-code-best-practice  
   → 69 praxiserprobte Tips, mit Input vom Claude Code-Entwickler Boris Cherny

### Forschungs-Kontext

8. **ALMAS: Autonomous LLM-based Multi-Agent Software Engineering** — Tawosi et al., arXiv:2510.03463  
   → Agile Rollen-Mapping für LLM-Agenten

9. **Non-determinism in LLM Code Generation** — Ouyang et al., 2025  
   → Empirische Grundlage: bis zu 75,76% Variabilität bei identischen Prompts

---

## Anhang A: Quick-Start Checkliste

```
PRE-PROJECT
[ ] CLAUDE.md erstellt und iteriert
[ ] AGENTS.md erstellt (Cross-Tool)
[ ] .claude/agents/ — 5 Agent-Definitionen  
[ ] .claude/commands/ — Mindestens: new-feature, bugfix
[ ] pre-commit Hook aktiv (TypeCheck + Lint + Tests)
[ ] CI/CD Pipeline mit Quality Gates
[ ] Coverage-Threshold konfiguriert (80%)
[ ] Architecture Overview in docs/

PER FEATURE
[ ] Intent Translation: Vage → Präzise Spec
[ ] Plan erstellt und reviewed (human)
[ ] Tests ZUERST — und rot verifiziert
[ ] Minimale Implementation
[ ] Alle Tests grün
[ ] Reviewer-Agent Code Review
[ ] Commit mit Conventional Commits

PER SPRINT
[ ] ADRs aktuell halten
[ ] Test Coverage > Threshold
[ ] Keine bekannten Security Issues
[ ] CLAUDE.md aus Erfahrungen aktualisieren (iterieren!)
```

---

*Letzte Aktualisierung: Juni 2026 | Forschungsstand: EASE 2026, arXiv Stand April 2026*
