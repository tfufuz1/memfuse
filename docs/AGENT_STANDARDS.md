# LLM Coding Agent System — Architektur & Entwicklungsstandards

> **Version:** 1.0.0  
> **Status:** Living Document  
> **Zweck:** Verbindliche Entwicklungsstandards für alle am Projekt beteiligten Coding-Agenten und Context-Architekten

---

## Inhaltsverzeichnis

1. [Systemüberblick](#systemüberblick)
2. [Spec-Driven Development (SDD)](#spec-driven-development)
3. [Test-Driven Development (TDD)](#test-driven-development)
4. [Comment-ANCHOR System](#comment-anchor-system)
5. [GitHub Workflow](#github-workflow)
6. [Agent-Rollen & Verantwortlichkeiten](#agent-rollen--verantwortlichkeiten)
7. [Arbeitspaket-Lebenszyklus](#arbeitspaket-lebenszyklus)
8. [Spec-Dateistruktur](#spec-dateistruktur)
9. [Kommunikationsprotokoll zwischen Agenten](#kommunikationsprotokoll-zwischen-agenten)
10. [Qualitätsgates](#qualitätsgates)

---

## Systemüberblick

Das System besteht aus drei Kernprinzipien, die **untrennbar** miteinander verzahnt sind:

```
Spec-Driven Development
        │
        ▼
  [Spezifikation] ──► [Implementierung] ──► [Verifikation]
        │                    │                    │
   Blackboard            TDD-Zyklus          Comment-ANCHOR
   pro Komponente       Red→Green→Refactor    Kommunikation
        │                    │                    │
        └────────────────────┴────────────────────┘
                             │
                        GitHub Flow
                   (Branch → PR → Review → Merge)
```

**Kein Code wird geschrieben, der nicht durch eine Spezifikation abgedeckt ist.**  
**Kein Code gilt als fertig, der nicht durch Tests verifiziert ist.**  
**Kein Schritt bleibt ohne ANCHOR-Kommentar dokumentiert.**

---

## Spec-Driven Development

### Grundprinzip

Jede Komponente und jede Funktion existiert zuerst als Spezifikation. Die Spezifikation ist das **Blackboard** — sie wird von allen Agenten gelesen, bearbeitet und aktualisiert. Sie ist gleichzeitig:

- **Roadmap** (was muss gebaut werden)
- **Implementierungsvertrag** (wie muss es funktionieren)
- **Statusboard** (was ist fertig, was ist offen)
- **Entscheidungslog** (warum wurde so entschieden)

### Spezifikations-Hierarchie

```
specs/
├── SYSTEM.spec.md          # Gesamtsystem-Architektur
├── components/
│   ├── auth/
│   │   ├── AUTH.spec.md    # Komponenten-Spezifikation
│   │   └── AUTH.test.md    # Test-Spezifikation
│   ├── api/
│   │   ├── API.spec.md
│   │   └── API.test.md
│   └── ...
└── decisions/
    └── ADR-001-*.md        # Architecture Decision Records
```

### Spezifikations-Template

Jede `*.spec.md` folgt exakt diesem Format:

```markdown
# [Komponentenname] Spezifikation

## Status
- Phase: [PLANNING | IMPLEMENTING | TESTING | COMPLETE]
- Agent: [aktuell zuständiger Agent / "unassigned"]
- Letzte Änderung: [Datum + Agent-ID]

## Zweck
Einzeilige Beschreibung des Hauptzwecks dieser Komponente.

## Funktionale Anforderungen
### FR-001: [Anforderungsname]
- **Beschreibung:** Was muss diese Funktion tun?
- **Input:** Erwartete Eingaben (Typ, Format, Constraints)
- **Output:** Erwartete Ausgaben (Typ, Format, Garantien)
- **Fehlerverhalten:** Was passiert bei ungültigen Inputs?
- **Status:** [ ] Offen | [x] Implementiert | [v] Getestet

### FR-002: ...

## Nicht-funktionale Anforderungen
- Performance: z.B. max. 200ms Response-Time
- Sicherheit: z.B. Input-Sanitization erforderlich
- Skalierbarkeit: z.B. thread-safe

## Abhängigkeiten
- Intern: [andere Komponenten, die benötigt werden]
- Extern: [Libraries, Services]

## Schnittstellen / API-Vertrag
```typescript
// Exakte Signaturen — VERBINDLICH
interface ComponentInterface {
  method(param: Type): ReturnType;
}
```

## Implementierungsnotizen
<!-- ANCHOR:IMPL-NOTES — Agenten tragen hier Erkenntnisse ein -->

## Offene Fragen
<!-- ANCHOR:OPEN-QUESTIONS — Ungeklärtes, blockierende Fragen -->

## Änderungsprotokoll
| Datum | Agent | Änderung |
|-------|-------|----------|
| ...   | ...   | ...      |
```

### Workflow für Spezifikationen

1. **Context-Architekt** zerlegt das System in Komponenten und erstellt initiale Specs
2. **Agent** liest die Spec vollständig vor Beginn jeder Arbeit
3. **Agent** aktualisiert den Status jeder FR nach Implementierung
4. **Agent** trägt Erkenntnisse in `IMPLEMENTIERUNGSNOTIZEN` ein
5. **Agent** markiert offene Fragen für den Context-Architekten

---

## Test-Driven Development

### Prinzip: Red → Green → Refactor

**Keine Implementierung ohne vorherigen Test.** Die Reihenfolge ist unveräußerlich:

```
1. Test schreiben (schlägt fehl — RED)
      │
      ▼
2. Minimale Implementierung (Test besteht — GREEN)
      │
      ▼
3. Code verbessern (Tests bleiben grün — REFACTOR)
      │
      ▼
4. Nächste Funktion → zurück zu 1.
```

### Test-Kategorien

#### Unit-Tests (Pflicht für jede Funktion)

```typescript
// Struktur für jeden Unit-Test:
describe('[FunctionName]', () => {
  // ANCHOR:TEST-SETUP — Initialisierung, Mocks
  
  describe('Happy Path', () => {
    it('sollte [erwartetes Verhalten] bei [Bedingung]', () => {
      // Arrange
      // Act
      // Assert
    });
  });

  describe('Fehlerbehandlung', () => {
    it('sollte Fehler werfen wenn [ungültige Bedingung]', () => {
      // ...
    });

    it('sollte graceful degradieren wenn [Edge Case]', () => {
      // ...
    });
  });

  describe('Edge Cases', () => {
    // Leere Arrays, null, undefined, Extremwerte...
  });
});
```

#### Integrations-Tests (Pflicht für Komponenten-Schnittstellen)

Testen das Zusammenspiel mehrerer Komponenten. Werden in `*.test.md` spezifiziert, bevor implementiert wird.

#### Smoke Tests (Pflicht vor jedem PR-Merge)

Ein minimaler Test-Set der kritischen User-Journeys. Dienen als schnelles Feedback-Gate im CI.

### Fehlerbehandlungs-Standard

Jede Funktion muss folgende Fehlerfälle explizit behandeln:

```typescript
// ANCHOR:ERROR-HANDLING — Pflichtstruktur für jede Funktion

function example(input: InputType): Result<OutputType, AppError> {
  // 1. Input-Validierung
  if (!isValid(input)) {
    return Err(new ValidationError('Beschreibung', { input }));
  }
  
  try {
    // 2. Hauptlogik
    const result = doWork(input);
    return Ok(result);
  } catch (error) {
    // 3. Fehler wrappen mit Kontext
    return Err(new OperationError('Kontext', { cause: error, input }));
  }
}
```

**Verboten:** `try { ... } catch (e) { console.log(e) }` ohne strukturierte Fehlerbehandlung.

### Test-Abdeckungs-Minimum

| Kategorie | Mindest-Coverage |
|-----------|-----------------|
| Statements | 90% |
| Branches | 85% |
| Functions | 100% |
| Lines | 90% |

Eine Funktion gilt erst als **fertig** wenn:
- [ ] Alle Unit-Tests grün
- [ ] Fehlerbehandlung getestet
- [ ] Edge Cases abgedeckt
- [ ] Coverage-Schwellwerte erreicht
- [ ] Spec-Status auf `[v] Getestet` aktualisiert

---

## Comment-ANCHOR System

### Grundkonzept

Comment-ANKERs sind das **indirekte Kommunikationssystem** zwischen Agenten. Sie dienen als:

- **Signposts** für nachfolgende Agenten (was hier zu tun ist)
- **Log-Einträge** (was hier getan wurde und warum)
- **Warnungen** (was hier besonders zu beachten ist)
- **Fragen** (was ungeklärt ist und Entscheidung braucht)

### ANCHOR-Syntax

```
// ANCHOR:[TYP]:[ID] — [Kurzbeschreibung]
// [Kontext-Zeile 1]
// [Kontext-Zeile 2]
// AGENT:[Agent-ID] DATE:[YYYY-MM-DD] STATUS:[OPEN|DONE|BLOCKED]
```

### ANCHOR-Typen

| Typ | Bedeutung | Wer setzt ihn | Wer liest ihn |
|-----|-----------|---------------|---------------|
| `TODO` | Noch zu implementieren | Architekt / Planer-Agent | Implementierungs-Agent |
| `FIXME` | Bekannter Bug / Schwachstelle | Jeder Agent | Nächster Agent im Kontext |
| `IMPL` | Implementierungsentscheidung dokumentiert | Implementierungs-Agent | Review-Agent |
| `TEST` | Testfall markiert / Test-Lücke | Test-Agent | Implementierungs-Agent |
| `WARN` | Kritische Warnung — nicht ändern ohne Kontext | Jeder Agent | Alle Agenten |
| `ARCH` | Architekturentscheidung | Context-Architekt | Alle Agenten |
| `PERF` | Performance-kritischer Bereich | Jeder Agent | Optimierungs-Agent |
| `SEC` | Sicherheitsrelevanter Bereich | Jeder Agent | Security-Review |
| `DEBT` | Technische Schulden — akzeptierter Kompromiss | Jeder Agent | Refactoring-Agent |
| `HANDOFF` | Übergabe-Punkt zwischen Agenten | Abgebender Agent | Übernehmender Agent |

### ANCHOR-Beispiele

```typescript
// ANCHOR:ARCH:AUTH-001 — JWT-Strategie gewählt statt Sessions
// Sessions würden Shared Storage zwischen Instanzen erfordern.
// JWT ermöglicht stateless Horizontal Scaling.
// Ablaufzeit: 15min Access Token, 7d Refresh Token.
// AGENT:context-architect DATE:2024-01-15 STATUS:DONE

async function validateToken(token: string): Promise<User> {
  
  // ANCHOR:SEC:AUTH-002 — Timing-Attack-Prävention
  // crypto.timingSafeEqual verwenden — NICHT direkten String-Vergleich!
  // Änderung nur nach Security-Review.
  // AGENT:security-agent DATE:2024-01-16 STATUS:DONE
  const isValid = await verifyJWT(token);
  
  // ANCHOR:TODO:AUTH-003 — Token-Blacklisting noch nicht implementiert
  // Bei Logout müssen invalidierte Tokens bis Ablauf gespeichert werden.
  // Lösung: Redis Set mit TTL = Token-Ablaufzeit
  // AGENT:context-architect DATE:2024-01-15 STATUS:OPEN
  
  if (!isValid) {
    throw new AuthenticationError('Token ungültig');
  }
  
  return extractUser(token);
}

// ANCHOR:PERF:AUTH-004 — Cache-Kandidat
// Diese Funktion wird bei jedem Request aufgerufen.
// DB-Lookup für User nach Token-Validierung cachen (TTL: 5min).
// ANCHOR:DEBT:AUTH-005 — Cache noch nicht implementiert, erst nach Load-Test
// AGENT:perf-agent DATE:2024-01-17 STATUS:OPEN
```

### ANCHOR-Verwaltung

ANKERs werden **niemals** kommentarlos gelöscht. Beim Abschließen:

```typescript
// ANCHOR:TODO:AUTH-003 — Token-Blacklisting implementiert ✓
// Redis-Set mit TTL implementiert in redis/tokenBlacklist.ts
// Tests: auth/tokenBlacklist.test.ts — alle 12 Tests grün
// AGENT:impl-agent DATE:2024-01-20 STATUS:DONE → Löschen nach nächstem Review
```

---

## GitHub Workflow

### Branch-Strategie

```
main (protected)
  │
  ├── develop (integration branch)
  │     │
  │     ├── feature/COMP-001-auth-jwt
  │     ├── feature/COMP-002-api-endpoints
  │     ├── fix/COMP-001-token-expiry-bug
  │     └── test/COMP-001-integration-tests
  │
  └── release/v1.0.0 (release candidates)
```

### Branch-Namenskonvention

```
[type]/[COMP-ID]-[kurze-beschreibung]

type: feature | fix | test | refactor | docs | chore
COMP-ID: aus Spezifikation (z.B. AUTH-001, API-003)
```

### Commit-Konvention (Conventional Commits)

```
[type]([scope]): [kurze Beschreibung im Imperativ]

[optionaler Body — was wurde warum geändert]

[optionale Footer]
ANCHOR-CLOSE: AUTH-003
SPEC-UPDATE: AUTH.spec.md FR-003 → DONE
Closes #42
```

**Commit-Typen:**

| Typ | Wann |
|-----|------|
| `feat` | Neue Funktion (laut Spec) |
| `fix` | Bug-Fix (referenziert FIXME-ANCHOR) |
| `test` | Test hinzugefügt/geändert |
| `refactor` | Refactoring ohne Verhaltensänderung |
| `docs` | Dokumentation / Spec-Updates |
| `chore` | Build, Dependencies, CI |
| `spec` | Spezifikationsänderungen |

### Beispiel-Commit

```
feat(auth): implementiere JWT-Token-Blacklisting

Redis-Set mit TTL für invalidierte Access Tokens.
TTL wird auf Token-Ablaufzeit gesetzt um automatische Bereinigung
sicherzustellen. Getestet mit 12 Unit-Tests, alle grün.

ANCHOR-CLOSE: AUTH-003, AUTH-005
SPEC-UPDATE: AUTH.spec.md FR-005 → [v] Getestet
Closes #38
```

### Pull Request Template

Jeder PR verwendet dieses Template (`.github/pull_request_template.md`):

```markdown
## Was wurde implementiert?
[Kurze Beschreibung der Änderungen]

## Welche Spec-Anforderungen werden erfüllt?
- [ ] COMP-ID: FR-XXX — [Anforderungsname]
- [ ] COMP-ID: FR-XXX — [Anforderungsname]

## Test-Ergebnis
- [ ] Alle neuen Tests grün
- [ ] Keine bestehenden Tests gebrochen
- [ ] Coverage-Schwellwerte eingehalten
- [ ] `npm test` lokal erfolgreich

## ANCHOR-Status
- Geschlossen: [Liste der ANKERs die DONE gesetzt wurden]
- Neu geöffnet: [Liste neuer OPEN ANKERs]
- Blockiert: [Blocker und Grund]

## Spec-Updates
- [ ] Spec-Status für implementierte FRs aktualisiert
- [ ] Implementierungsnotizen in Spec eingetragen
- [ ] ADR erstellt falls Architekturentscheidung getroffen

## Screenshot / Output (falls applicable)
```

### Branch-Schutzregeln (main & develop)

- Mindestens 1 Approval erforderlich
- CI muss grün sein (Tests + Linting + Type-Check)
- Direkte Pushes verboten
- Branch muss aktuell sein vor Merge

---

## Agent-Rollen & Verantwortlichkeiten

### Context-Architekt

**Zuständigkeit:** System-Design, Spezifikationserstellung, Qualitätssicherung

**Aufgaben:**
- Erstellt und pflegt `SYSTEM.spec.md`
- Zerlegt System in Komponenten und Arbeitspakete
- Erstellt initiale `*.spec.md` für jede Komponente
- Setzt `ARCH`-ANKERs für kritische Architekturentscheidungen
- Reviewt PRs auf Spec-Konformität
- Löst blockierte `OPEN`-ANKERs auf

**Darf nicht:**
- Code schreiben (delegiert immer an Implementierungs-Agenten)
- Spezifikationen ohne Eintrag im Änderungsprotokoll ändern

### Implementierungs-Agent

**Zuständigkeit:** Code-Implementierung nach Spec

**Aufgaben:**
- Liest Spec vollständig vor Beginn
- Schreibt Tests BEVOR Implementierung (TDD)
- Setzt `IMPL`-ANKERs für Entscheidungen
- Schließt `TODO`-ANKERs nach Implementierung
- Aktualisiert Spec-Status für jede FR
- Öffnet `HANDOFF`-ANKERs bei Übergabe

**Darf nicht:**
- Implementieren ohne Spec-Deckung
- Tests überspringen
- ANKERs löschen ohne `STATUS:DONE`

### Test-Agent

**Zuständigkeit:** Test-Spezifikation und Verifikation

**Aufgaben:**
- Erstellt `*.test.md` Spezifikationen
- Reviewt Test-Abdeckung
- Setzt `TEST`-ANKERs für fehlende Tests
- Verifiziert Edge Cases und Fehlerbehandlung

### Review-Agent

**Zuständigkeit:** Code-Review und Qualitätssicherung

**Aufgaben:**
- Prüft Code gegen Spec-Anforderungen
- Verifiziert ANCHOR-Konsistenz
- Prüft Test-Qualität (nicht nur Coverage)
- Setzt `FIXME`-ANKERs bei gefundenen Issues

---

## Arbeitspaket-Lebenszyklus

```
[SPEC READY] ──► [IN PROGRESS] ──► [TESTS WRITTEN] ──► [IMPL DONE]
                                                              │
                                                              ▼
[CLOSED] ◄── [MERGED] ◄── [PR APPROVED] ◄── [REVIEW] ◄── [GREEN]
```

### Status-Definitionen

| Status | Bedeutung | Nächster Schritt |
|--------|-----------|-----------------|
| `SPEC READY` | Spec vollständig, bereit zur Implementierung | Agent zuweisen |
| `IN PROGRESS` | Agent arbeitet aktiv daran | Tests schreiben |
| `TESTS WRITTEN` | Tests rot (RED), Implementierung beginnt | Code schreiben |
| `IMPL DONE` | Tests grün, Coverage OK | PR öffnen |
| `REVIEW` | PR geöffnet, wartet auf Review | Reviewer assigned |
| `PR APPROVED` | Review bestanden | Merge |
| `MERGED` | In develop integriert | Spec-Update |
| `CLOSED` | Vollständig abgeschlossen | — |
| `BLOCKED` | Blockiert durch Abhängigkeit/Frage | Context-Architekt |

---

## Spec-Dateistruktur

### Gesamt-Repository-Struktur

```
project-root/
├── .github/
│   ├── workflows/
│   │   ├── ci.yml              # Tests, Lint, Type-Check
│   │   └── spec-check.yml      # Validiert Spec-Konsistenz
│   └── pull_request_template.md
│
├── specs/
│   ├── SYSTEM.spec.md          # Master-Spec
│   ├── components/
│   │   └── [component]/
│   │       ├── [COMP].spec.md
│   │       └── [COMP].test.md
│   └── decisions/
│       └── ADR-[NNN]-[title].md
│
├── src/
│   └── [component]/
│       ├── index.ts
│       ├── [module].ts         # Mit ANCHOR-Kommentaren
│       └── __tests__/
│           ├── unit/
│           └── integration/
│
├── docs/
│   └── ARCHITECTURE.md
│
└── AGENTS.md                   # Aktuelle Agent-Zuweisungen & Status
```

### AGENTS.md — Echtzeit-Status

```markdown
# Agent-Zuweisungen

## Aktiv

| Agent-ID | Komponente | Arbeitspaket | Status | Blockiert? |
|----------|-----------|-------------|--------|------------|
| impl-01  | AUTH      | FR-005 JWT Blacklist | IN PROGRESS | Nein |
| test-01  | API       | FR-003 Rate Limiting | TESTS WRITTEN | Nein |

## Warteschlange

| Priorität | Komponente | Arbeitspaket | Spec bereit? |
|-----------|-----------|-------------|-------------|
| HIGH      | AUTH      | FR-006 2FA  | Ja |
| MEDIUM    | API       | FR-007 Pagination | Nein |

## Blockiert

| Agent-ID | Grund | Warte auf | ANCHOR |
|----------|-------|-----------|--------|
| impl-02  | DB-Schema unklar | ARCH-Entscheidung | ARCH:DB-001 |
```

---

## Kommunikationsprotokoll zwischen Agenten

### Direkte Kommunikation (via ANKERs)

Agenten kommunizieren **nicht** direkt miteinander — ausschließlich über ANKERs in Code und Specs. Dies garantiert:

- Vollständige Nachvollziehbarkeit im Git-History
- Kein Lost Context zwischen Sessions
- Klare Verantwortlichkeiten

### Übergabe-Protokoll (HANDOFF)

Wenn ein Agent seine Arbeit übergibt:

```typescript
// ANCHOR:HANDOFF:AUTH-007 — Übergabe an Test-Agent
// IMPLEMENTIERT: JWT-Blacklisting mit Redis
// GETESTET: Unit-Tests für Happy Path (auth/blacklist.test.ts)
// NOCH OFFEN:
//   - Integration-Tests mit echtem Redis fehlen
//   - Edge Case: Redis nicht erreichbar → Fallback?
// DATEIEN: src/auth/blacklist.ts, src/auth/blacklist.test.ts
// SPEC: AUTH.spec.md FR-005 Status → IMPL DONE
// AGENT:impl-01 DATE:2024-01-20 STATUS:OPEN → Test-Agent übernimmt
```

### Eskalations-Protokoll

Wenn ein Agent blockiert ist:

```typescript
// ANCHOR:BLOCKED:API-012 — Entscheidung erforderlich
// FRAGE: Rate-Limiting per User oder per IP?
//   - Per User: erfordert Auth-Token-Validierung im Middleware-Layer
//   - Per IP: einfacher, aber bei NAT ungenau
// IMPACT: Betrifft FR-003, FR-004, API.spec.md Schnittstellen-Design
// SOFORTIGE AKTION ERFORDERLICH — blockiert PR #45
// AGENT:impl-02 DATE:2024-01-21 STATUS:BLOCKED
// → Context-Architekt: Entscheidung in ADR dokumentieren
```

---

## Qualitätsgates

### Gate 1: Vor Implementierungsstart

- [ ] Spec für diese Komponente existiert und ist `SPEC READY`
- [ ] Alle `ARCH`-ANKERs in relevanten Dateien gelesen
- [ ] Abhängigkeiten vollständig implementiert oder gemockt
- [ ] Branch von aktuellem `develop` erstellt

### Gate 2: Vor PR-Öffnung

- [ ] Alle Tests grün (`npm test`)
- [ ] Coverage-Schwellwerte erfüllt (`npm run test:coverage`)
- [ ] Linting fehlerfrei (`npm run lint`)
- [ ] Type-Check fehlerfrei (`npm run type-check`)
- [ ] Alle neuen Funktionen haben `IMPL`-ANKERs
- [ ] Alle gelösten `TODO`-ANKERs auf `STATUS:DONE`
- [ ] Spec-Status für implementierte FRs aktualisiert
- [ ] Commit-Messages folgen Conventional Commits

### Gate 3: PR-Review

- [ ] Code entspricht der Spec-Spezifikation
- [ ] Tests testen tatsächlich das Verhalten (nicht nur Coverage)
- [ ] Fehlerbehandlung vollständig
- [ ] Keine unbehandelten `FIXME`-ANKERs
- [ ] Keine magischen Zahlen ohne Erklärung
- [ ] Performance-kritische Bereiche mit `PERF`-ANKERs markiert

### Gate 4: Vor Release

- [ ] Alle Komponenten-Specs `COMPLETE`
- [ ] Smoke-Tests grün
- [ ] Kein `OPEN`-ANCHOR vom Typ `SEC` oder `WARN`
- [ ] CHANGELOG.md aktualisiert
- [ ] Alle ADRs dokumentiert

---

## Anhang: Schnellreferenz ANCHOR-Syntax

```
// ANCHOR:[TYP]:[COMP-NNN] — [Einzeiler was/warum]
// [Erläuterung Zeile 1]
// [Erläuterung Zeile 2]
// AGENT:[agent-id] DATE:[YYYY-MM-DD] STATUS:[OPEN|DONE|BLOCKED]
```

**Typen:** `TODO` `FIXME` `IMPL` `TEST` `WARN` `ARCH` `PERF` `SEC` `DEBT` `HANDOFF` `BLOCKED`

**Status:** `OPEN` (aktiv) | `DONE` (erledigt, markiert zum Löschen) | `BLOCKED` (wartet)

---

*Dieses Dokument ist ein Living Document. Änderungen nur durch den Context-Architekten mit Eintrag im Git-Log.*
