# LLM-Driven Software Engineering — Fortschrittliches Regelwerk
### Von der Idee zum Produkt mit Coding-Agenten & großen Sprachmodellen

> **Version:** 2.0 | **Paradigma:** Context-First, Loop-Driven, Failure-Aware  
> **Zielgruppe:** Senior Engineers, Tech Leads, AI-Augmented Teams

---

## Inhaltsverzeichnis

1. [Kernphilosophie & Denkmodell](#1-kernphilosophie--denkmodell)
2. [Kontext-Architektur — Das Fundament](#2-kontext-architektur--das-fundament)
3. [Planungsphase mit LLMs](#3-planungsphase-mit-llms)
4. [Architektur-Design & Entscheidungen](#4-architektur-design--entscheidungen)
5. [Der Entwicklungs-Loop](#5-der-entwicklungs-loop)
6. [Prompt-Patterns & Templates](#6-prompt-patterns--templates)
7. [Dokumentation & Inline-Kommentare](#7-dokumentation--inline-kommentare)
8. [Testing-Infrastruktur](#8-testing-infrastruktur)
9. [LLM-Schwachstellen & Gegenmaßnahmen](#9-llm-schwachstellen--gegenmaßnahmen)
10. [Agile Integration & Iteration](#10-agile-integration--iteration)
11. [Qualitätssicherung & Review-Loops](#11-qualitätssicherung--review-loops)
12. [Referenz: Prompt-Bibliothek](#12-referenz-prompt-bibliothek)

---

## 1. Kernphilosophie & Denkmodell

### 1.1 Das LLM ist kein Entwickler — es ist ein hochkompetenter Kontext-Interpreter

Ein LLM erzeugt keine Software — es vervollständigt Muster auf Basis von Kontext.
Daraus folgen drei unverhandelbare Grundsätze:

```
QUALITÄT DES OUTPUTS = f(Qualität des Kontexts × Klarheit der Constraints × Iteration)
```

**Prinzip 1 — Context is King:**
Jeder Prompt ist ein Mini-Lastenheft. Je präziser der Kontext, desto deterministischer das Ergebnis.

**Prinzip 2 — Constraint before Creation:**
Definiere zuerst, was NICHT gewünscht ist. LLMs neigen zu kreativer Übererfüllung ohne Grenzen.

**Prinzip 3 — Loop over Perfection:**
Kein einzelner Prompt liefert ein fertiges System. Iteration ist keine Schwäche — sie ist der Mechanismus.

---

### 1.2 Das CLEAR-Modell für jeden Prompt

| Buchstabe | Bedeutung | Beispiel |
|-----------|-----------|---------|
| **C** — Context | Projektstand, Technologie, Abhängigkeiten | `"Wir bauen eine REST API in FastAPI, Python 3.12, bereits implementiert: Auth-Modul"` |
| **L** — Level | Abstraktionsstufe & Detailtiefe | `"Implementiere, kein Konzept — direkt ausführbarer Code"` |
| **E** — Examples | Ein-/Ausgabe-Beispiele, existierender Code | Code-Snippet des letzten Moduls als Referenz |
| **A** — Anti-patterns | Was explizit NICHT gemacht werden soll | `"Keine globals, kein Mocking von externen Services in Unit Tests"` |
| **R** — Result format | Erwartetes Ausgabeformat | `"Nur die Funktion + Docstring, keine Erklärung drumherum"` |

---

### 1.3 Das Drei-Schichten-Modell der LLM-Zusammenarbeit

```
┌─────────────────────────────────────────────────┐
│  SCHICHT 3: STRATEGIE  (Mensch dominiert)        │
│  Architektur-Entscheidungen, Tech-Stack,         │
│  Geschäftslogik, Security-Konzept                │
├─────────────────────────────────────────────────┤
│  SCHICHT 2: TAKTIK  (Mensch + LLM kollaborieren) │
│  Modul-Design, Interface-Definitionen,           │
│  Refactoring-Strategien, Test-Planung            │
├─────────────────────────────────────────────────┤
│  SCHICHT 1: IMPLEMENTIERUNG  (LLM dominiert)     │
│  Code-Generierung, Boilerplate, Docs,            │
│  Unit Tests, Typ-Definitionen, Transformationen  │
└─────────────────────────────────────────────────┘
```

> **Fehler:** LLMs für Schicht-3-Entscheidungen verwenden ohne human oversight = größte Quelle von Architektur-Debt.

---

## 2. Kontext-Architektur — Das Fundament

### 2.1 Der MASTER-KONTEXT (Project Context File)

Erstelle zu Projektbeginn eine `PROJECT_CONTEXT.md` — die einzige Wahrheitsquelle für alle LLM-Interaktionen.

```markdown
# PROJECT_CONTEXT.md

## Projekt-Identität
- **Name:** PaymentGateway-Service
- **Domäne:** FinTech / B2B Payment Processing
- **Phase:** MVP (Sprint 3 von 6)
- **Kritikalität:** Hoch — Finanztransaktionen, PCI-DSS relevant

## Tech-Stack (FEST, nicht verhandelbar)
- Runtime: Python 3.12, FastAPI 0.110
- DB: PostgreSQL 16 via SQLAlchemy 2.0 (async)
- Queue: Redis Streams
- Auth: JWT + OAuth2 (bereits implementiert in /auth/)
- Container: Docker + Kubernetes (GKE)
- CI: GitHub Actions

## Architektur-Muster
- Hexagonale Architektur (Ports & Adapters)
- Repository Pattern für alle DB-Operationen
- Domain Events für async Kommunikation
- KEIN direktes ORM in Business Logic

## Code-Standards
- Type hints: 100% Pflicht
- Docstrings: Google-Style
- Tests: pytest, min. 80% Coverage
- Kein print() — nur structured logging (structlog)
- Error handling: Custom Exception Hierarchy (siehe /core/exceptions.py)

## Verbotene Patterns
- [ ] Keine synchronen DB-Calls im async Context
- [ ] Keine hardcodierten Credentials (ENV vars only)
- [ ] Kein catch-all `except Exception` ohne Re-raise
- [ ] Keine business logic in API-Layern

## Aktueller Sprint-Fokus
- US-001: Payment Intent erstellen
- US-002: Webhook-Verarbeitung von Stripe
- US-003: Idempotenz-Mechanismus

## Bekannte technische Schulden
- Legacy synchrone Datenbankverbindung in /legacy/db_sync.py (nicht anfassen!)
- Rate-Limiter ist TODO (Kommentar in /api/middleware.py L.45)
```

**Nutzung:** Dieser Kontext wird als ERSTES in jeden neuen Chat/Agenten-Run eingefügt.

---

### 2.2 Kontext-Hierarchie & Schichtung

```
PROJECT_CONTEXT.md          ← Immer dabei (statisch)
    │
    ├── SPRINT_CONTEXT.md   ← Aktueller Sprint (wöchentlich aktualisiert)
    │       │
    │       ├── TASK_CONTEXT.md   ← Aktuelle Aufgabe (per Session)
    │       │       │
    │       │       └── CODE_SNIPPET  ← Relevanter bestehender Code
    │       │
    │       └── DECISION_LOG.md   ← Getroffene Entscheidungen
    │
    └── FAILURE_LOG.md      ← Was bereits schief gelaufen ist (kritisch!)
```

### 2.3 Das FAILURE_LOG — Der mächtigste Kontext

```markdown
# FAILURE_LOG.md

## [2024-03-15] Async Context Manager Leak
**Was passiert:** LLM generierte DB-Sessions ohne proper cleanup
**Symptom:** Connection pool exhaustion nach 50 Requests
**Fix:** Immer `async with session_factory() as session:` Pattern
**Prompt-Regel:** "Nutze IMMER den async context manager aus /core/db.py, 
                   niemals manuelle Session-Verwaltung"

## [2024-03-18] Race Condition in Payment State Machine
**Was passiert:** LLM implementierte State Transitions ohne Locking
**Symptom:** Doppelte Buchungen unter Last
**Fix:** Optimistic Locking mit version_id Feld
**Prompt-Regel:** "Bei State-Transitions: SELECT FOR UPDATE oder 
                   optimistic locking mit version_id - Beispiel in /core/locking.py"

## [2024-03-20] Missing Idempotency Keys
**Was passiert:** Retry-Logic ohne Idempotenz-Checks
**Symptom:** Doppelte Zahlungen bei Netzwerk-Timeouts
**Prompt-Regel:** "Jeder mutierender Endpoint MUSS Idempotenz-Key 
                   aus Header X-Idempotency-Key verarbeiten"
```

> **Regel:** Das FAILURE_LOG ist IMMER Teil des Kontexts bei Implementierungs-Prompts. Es verhindert Regressionen.

---

### 2.4 Kontext-Kompression für große Projekte

Wenn der Kontext zu groß wird, verwende **strukturierte Zusammenfassungen**:

```markdown
## KOMPRIMIERTER MODUL-KONTEXT: /payments/

IMPLEMENTIERT (nicht neu generieren):
- PaymentIntent.create() → /payments/domain/intent.py:45
- PaymentRepository.save() → /payments/infrastructure/repo.py:23
- IntentCreatedEvent → /payments/events.py:12

INTERFACES (verwenden, nicht ändern):
```python
# Repository Interface — NICHT modifizieren
class PaymentRepository(Protocol):
    async def save(self, payment: Payment) -> Payment: ...
    async def find_by_id(self, id: UUID) -> Payment | None: ...
    async def find_by_idempotency_key(self, key: str) -> Payment | None: ...
```

AKTUELL FEHLEND (das soll generiert werden):
- WebhookProcessor für Stripe Events
- Idempotenz-Check vor save()
```

---

## 3. Planungsphase mit LLMs

### 3.1 Der Planungs-Loop (3 Phasen)

```
Phase 1: ENTDECKUNG          Phase 2: SPEZIFIKATION       Phase 3: VALIDIERUNG
─────────────────────        ─────────────────────        ─────────────────────
LLM als Sparringspartner →   LLM strukturiert Output  →   LLM sucht Lücken
Fragen stellen lassen        User Stories generieren       "Was fehlt hier?"
Domäne erkunden              API-Contracts entwerfen       Edge Cases finden
Risiken aufdecken            Datenmodell vorschlagen       Widersprüche prüfen
```

### 3.2 Planungs-Prompt-Sequenz

**PROMPT 1 — Domain Discovery:**
```
Ich entwickle [PRODUKT]. Stelle mir als erfahrener Domain-Experte 
genau 10 kritische Fragen, die ich beantworten muss, BEVOR ich 
anfange zu entwickeln. Fokus: Nicht-offensichtliche Anforderungen, 
Skalierungsprobleme, Sicherheitsrisiken, Integrationskomplexität.
Keine Implementierungsfragen — nur strategische Domänenfragen.
```

**PROMPT 2 — Requirements Refinement:**
```
Gegeben meine Antworten: [ANTWORTEN]

Erstelle ein strukturiertes Requirements-Dokument mit:
1. Funktionale Anforderungen (priorisiert MoSCoW)
2. Nicht-funktionale Anforderungen (Performance, Security, Skalierung)
3. Ausdrücklich OUT OF SCOPE (kritisch!)
4. Offene Fragen die noch geklärt werden müssen
5. Technische Risiken mit Eintrittswahrscheinlichkeit

Format: Markdown-Tabellen, keine Prosa-Blöcke
```

**PROMPT 3 — Gap Analysis:**
```
Reviewe das folgende Requirements-Dokument als kritischer Senior-Architect.
Identifiziere:
- Widersprüchliche Anforderungen
- Unrealistische Kombinationen (Performance + Konsistenz + Verfügbarkeit — CAP!)  
- Fehlende Error-Cases
- Nicht-spezifizierte Grenzfälle
- Implizite Annahmen die expliziert werden müssen

Sei schonungslos ehrlich. Antworte NUR mit gefundenen Problemen.
[REQUIREMENTS_DOKUMENT]
```

---

### 3.3 User Story Generation mit Akzeptanzkriterien

**Prompt-Template:**
```
Erstelle für die folgende Anforderung vollständige User Stories im Format:

ANFORDERUNG: [ANFORDERUNG]

Format für jede Story:
---
ID: US-XXX
Als [Rolle] möchte ich [Aktion] damit [Mehrwert]

Akzeptanzkriterien (Given/When/Then):
- Given [Ausgangssituation]
  When [Aktion]
  Then [Erwartetes Ergebnis]
  
Technische Notizen:
- [API-Endpoint-Vorschlag]
- [Datenbank-Implikationen]
- [Performance-Anforderungen]

Edge Cases:
- [Was wenn X?]
- [Was bei Fehler Y?]

Definition of Done:
- [ ] Unit Tests (>80% Coverage)
- [ ] Integration Test
- [ ] API-Dokumentation
- [ ] Performance-Test (<200ms p95)
---
```

---

## 4. Architektur-Design & Entscheidungen

### 4.1 Architektur-Decision-Records (ADR) mit LLMs

**Niemals** eine Architektur-Entscheidung ohne schriftliches ADR. LLMs helfen dabei enorm:

```
PROMPT: Architecture Decision Support

Ich muss entscheiden: [ENTSCHEIDUNG Z.B. "Event Sourcing vs. CRUD für Payment-History"]

Kontext:
- [Projektkontext]
- [Aktuelle Constraints]
- [Erwartetes Volumen: X Requests/s, Y GB/Tag]

Erstelle ein ADR (Architecture Decision Record) mit:
1. Status: [Proposed]
2. Kontext: Warum diese Entscheidung jetzt
3. Optionen: Genau 3 realistische Alternativen
4. Entscheidungsmatrix: Tabelle mit Kriterien (Performance, Komplexität, 
   Wartbarkeit, Team-Expertise, Time-to-Market)
5. Konsequenzen: Was wird durch diese Entscheidung schwerer/leichter
6. Risiken & Mitigationen
7. Review-Datum: Wann überprüfen wir diese Entscheidung?

Sei ausgewogen — ich will keine Meinung, ich will Fakten zur Entscheidungshilfe.
```

**Beispiel-Output-Struktur:**
```markdown
# ADR-007: Event Store für Payment-History

**Status:** Accepted  
**Datum:** 2024-03-20  
**Entscheider:** Tech Lead + Senior Dev  
**Review:** 2024-06-20 (nach 3 Monaten Produktion)

## Kontext
Payment-Historie muss unveränderlich, auditierbar und replay-fähig sein.
Aktuelle CRUD-Lösung macht Point-in-Time-Queries unmöglich.

## Entscheidung
Event Sourcing mit PostgreSQL als Event Store (kein Kafka für MVP)

## Konsequenzen
+ Vollständige Audit-Trail
+ Zeitreise-Queries möglich
- Eventual Consistency in Read-Models
- Erhöhte Komplexität bei Queries
- Team muss Event Sourcing lernen
```

---

### 4.2 Datenmodell-Design Loop

```
SCHRITT 1: Entities identifizieren
──────────────────────────────────
PROMPT: "Leite aus diesen User Stories alle Domain-Entities ab. 
         Für jede Entity: Attribute, Invarianten, Lifecycle."

         ↓

SCHRITT 2: Beziehungen & Aggregates
─────────────────────────────────────
PROMPT: "Definiere Aggregate Boundaries. Welche Entities gehören 
         zusammen? Was ist die Aggregate Root? Begründe mit DDD."

         ↓

SCHRITT 3: Datenbankschema generieren
──────────────────────────────────────
PROMPT: "Generiere SQLAlchemy 2.0 async Models für diese Aggregates.
         Constraints, Indizes, Partitionierung für [X Millionen Zeilen].
         Migrations-Skript mit Alembic."

         ↓

SCHRITT 4: Schema-Review
─────────────────────────
PROMPT: "Reviewe dieses Datenbankschema auf:
         - N+1 Query Potenzial
         - Fehlende Indizes für häufige Query-Patterns
         - Normalisierungsprobleme
         - Skalierungsprobleme bei [X GB Daten]"
```

---

## 5. Der Entwicklungs-Loop

### 5.1 Der Kern-Loop: TDD-First mit LLM

```
┌──────────────────────────────────────────────────────────────┐
│                    ENTWICKLUNGS-LOOP                          │
│                                                               │
│  1. SPEC        2. TEST FIRST    3. IMPLEMENT    4. REVIEW   │
│  ─────────      ──────────────   ───────────     ─────────   │
│  Interface  →   Tests schreiben  → Code gen.  →  LLM-Review  │
│  definieren     (LLM assistiert)   (LLM gen.)    + Human     │
│  (Mensch)       [Kontrollpunkt]    [Auto]         Review      │
│                                                    │          │
│  ◄─────────────────────────────────────────────────┘         │
│  Iteration bei Fehlern / neuen Erkenntnissen                  │
└──────────────────────────────────────────────────────────────┘
```

### 5.2 Der Interface-First Prompt

**Schritt 1:** Interface definieren BEVOR Code generiert wird.

```python
# PROMPT: Interface-Definition
"""
Definiere nur das Python Protocol/Interface für einen PaymentProcessor.
KEIN Implementierungscode.

Anforderungen:
- Zahlung verarbeiten
- Zahlung rückbuchen
- Status abfragen
- Webhook validieren

Konventionen aus PROJECT_CONTEXT.md:
- Async/await
- Type hints 100%
- Custom Exceptions aus /core/exceptions.py
- Google-style Docstrings

Ausgabe: NUR das Interface als Python-Code, nichts anderes.
"""

# Erwarteter Output:
class PaymentProcessor(Protocol):
    async def process(
        self, 
        intent: PaymentIntent,
        idempotency_key: str
    ) -> PaymentResult:
        """
        Verarbeite eine Zahlung.
        
        Args:
            intent: Das PaymentIntent-Objekt mit allen Zahlungsdaten.
            idempotency_key: Eindeutiger Key zur Duplikat-Prävention.
            
        Returns:
            PaymentResult mit Status und Provider-Referenz.
            
        Raises:
            PaymentDeclinedError: Wenn Zahlung abgelehnt wird.
            ProviderUnavailableError: Bei Provider-Ausfall (retryable).
            IdempotencyViolationError: Bei Key-Konflikt.
        """
        ...
```

**Warum Interface-First?**
- LLM kennt den "Vertrag" bevor es implementiert
- Verhindert scope creep in der Implementierung
- Ermöglicht parallele Test-Generierung
- Macht Abhängigkeiten explizit

---

### 5.3 Der Test-Generierungs-Prompt

```
PROMPT: Test-Suite generieren

Interface: [INTERFACE_CODE]
Implementierung soll: [KURZBESCHREIBUNG]

Generiere eine vollständige pytest Test-Suite mit:

1. HAPPY PATH Tests (mindestens 3 Szenarien)
2. ERROR PATH Tests (alle deklarierten Exceptions)
3. EDGE CASES:
   - Leere/None-Werte
   - Grenzwerte (0, max_int, leerer String)
   - Concurrent calls (asyncio.gather)
   
4. FIXTURES:
   - Minimale, realistische Test-Daten
   - Keine Magic Numbers — benannte Konstanten
   
5. MOCKING:
   - Mock nur externe Dependencies (DB, HTTP)
   - Kein Mocking von eigenen Domain-Objekten
   
Stil:
- Beschreibende Testnamen: test_[methode]_[szenario]_[erwartung]
- Given/When/Then als Kommentare
- Keine Logik in Tests (ein Assert pro Test bevorzugt)

KONTEXT: [PROJECT_CONTEXT relevante Teile]
FAILURE_LOG: [Bekannte Fehler die abgedeckt sein müssen]
```

---

### 5.4 Der Implementierungs-Prompt mit Constraints

```
PROMPT: Implementierung generieren

INTERFACE (NICHT modifizieren):
[INTERFACE_CODE]

FAILING TESTS (müssen zum Bestehen gebracht werden):
[TEST_CODE]

IMPLEMENTIERUNGS-CONSTRAINTS:
- Nutze StripeClient aus /infrastructure/stripe_client.py (bereits implementiert)
- Nutze PaymentRepository aus /infrastructure/repos/payment_repo.py
- Folge Hexagonaler Architektur — kein direkter HTTP-Call in Domain
- Idempotenz: Check via repository.find_by_idempotency_key() VOR Stripe-Call
- Error mapping: Stripe-Fehler → Custom Exceptions (Mapping in /core/stripe_errors.py)
- Logging: structlog mit payment_id, idempotency_key als bound vars

VERBOTEN:
- Keine neuen Dependencies einführen
- Kein direktes requests/httpx importieren
- Keine print() oder logging.basicConfig()

FAILURE_LOG (diese Fehler MÜSSEN vermieden werden):
[RELEVANT_FAILURE_LOG_ENTRIES]

Ausgabe: NUR die Implementierungsklasse, keine Tests, keine Erklärung.
```

---

### 5.5 Der Self-Review Loop

Nach jeder LLM-Generierung, diesen Review-Prompt ausführen:

```
PROMPT: Code Self-Review

Reviewe den folgenden Code als kritischer Senior Engineer.
Suche nach (priorisiert):

KRITISCH (muss behoben werden):
- [ ] Security-Vulnerabilities (Injection, Secrets, unvalidierte Inputs)
- [ ] Race Conditions / Thread Safety Probleme
- [ ] Resource Leaks (unclosed connections, files, streams)
- [ ] Unhandelte Fehlerszenarien die zu Silent Failures führen

WICHTIG (sollte behoben werden):
- [ ] Performance-Probleme (N+1 Queries, fehlende Indizes, blocking I/O)
- [ ] Verletzungen der Architektur-Regeln aus PROJECT_CONTEXT
- [ ] Fehlende oder falsche Type Hints
- [ ] Inkonsistenter Error-Handling-Stil

STYLE (optional):
- [ ] Nicht-idiomatisches Python
- [ ] Unnötige Komplexität
- [ ] Docstring-Vollständigkeit

Für jeden gefundenen Punkt:
1. Zeile/Funktion
2. Problem-Beschreibung
3. Konkreter Fix-Vorschlag als Code-Snippet

CODE:
[GENERIERTER_CODE]
```

---

## 6. Prompt-Patterns & Templates

### 6.1 Das SCAFFOLD-Pattern (für neue Module)

```
# SCAFFOLD: Neues Modul anlegen
# Anwendung: Immer wenn ein neues Feature-Modul entsteht

Du erstellst die vollständige Dateistruktur für das Modul [MODUL_NAME].

PROJEKTSTRUKTUR-VORLAGE (von anderen Modulen ableiten):
/payments/
  domain/          ← Business Logic (kein Framework)
  application/     ← Use Cases / Services  
  infrastructure/  ← DB, HTTP, Queue Adapters
  api/             ← FastAPI Router
  tests/
    unit/
    integration/

Erstelle für [MODUL_NAME]:
1. Datei-Liste mit Zweck jeder Datei (noch kein Code)
2. Abhängigkeits-Graph zwischen den Dateien
3. Welche bestehenden Module werden importiert?
4. Interfaces die nach außen exponiert werden
5. Eine leere __init__.py Struktur mit __all__ und TODO-Kommentaren

KEIN Implementierungscode — nur Struktur und Interfaces.
```

### 6.2 Das REFACTOR-Pattern

```
# REFACTOR: Bestehenden Code verbessern

KONTEXT: Dieser Code wurde unter Zeitdruck geschrieben und 
         verletzt jetzt [SPEZIFISCHE_REGELN].

BESTEHENDER CODE:
[CODE]

ZIEL DES REFACTORING:
[z.B. "Hexagonale Architektur einhalten — DB-Calls aus Domain entfernen"]

CONSTRAINTS:
- Das externe Verhalten MUSS identisch bleiben
- Bestehende Tests MÜSSEN weiterhin bestehen
- Schrittweises Refactoring bevorzugt (nicht alles auf einmal)

LIEFERE:
1. Analyse: Was ist das genaue Problem?
2. Refactoring-Plan in Schritten (Schritt 1 zuerst, dann folgende)
3. Implementierung NUR von Schritt 1
4. Welche Tests müssen angepasst werden?

Schritt 2+ erst nach Bestätigung.
```

### 6.3 Das DEBUG-Pattern

```
# DEBUG: Fehler systematisch lösen

FEHLER:
[FEHLERMELDUNG + STACKTRACE]

KONTEXT:
- Wann tritt er auf: [Szenario]
- Wann tritt er NICHT auf: [Gegenbeispiel — kritisch!]
- Letzte Änderung vor dem Fehler: [GIT DIFF oder Beschreibung]

BEREITS VERSUCHT:
- [Was wurde probiert]
- [Warum hat es nicht funktioniert]

RELEVANTER CODE:
[NUR der relevante Teil, nicht alles]

HYPOTHESEN (meine bisherigen):
1. [Hypothese 1]
2. [Hypothese 2]

Analysiere: Ist eine meiner Hypothesen korrekt?
Falls nein: Was ist deine Top-3-Hypothesen mit Begründung?
Liefere einen Test, der die wahrscheinlichste Hypothese beweist/widerlegt.
```

### 6.4 Das MIGRATION-Pattern (Datenbankänderungen)

```
# MIGRATION: Datenbankschema ändern

AKTUELLE SCHEMA-VERSION:
[AKTUELLES SCHEMA oder Alembic-Migration]

GEWÜNSCHTE ÄNDERUNG:
[WAS soll sich ändern und WARUM]

BUSINESS CONSTRAINT:
- Zero-Downtime erforderlich: [Ja/Nein]
- Bestehende Daten müssen migriert werden: [Ja/Nein + Umfang]
- Rollback muss möglich sein: [Ja/Nein]

Erstelle:
1. Alembic-Migration (up + down)
2. Datenmigrations-Skript falls nötig (idempotent!)
3. Feature-Flag-Strategie für Zero-Downtime falls erforderlich
4. Rollback-Procedure
5. Verification-Query um sicherzustellen dass Migration erfolgreich war
```

### 6.5 Das SECURITY-REVIEW-Pattern

```
# SECURITY REVIEW: Vor jedem PR-Merge

Reviewe den folgenden Code auf Security-Probleme.
Denke wie ein Angreifer. Prüfe systematisch:

INJECTION:
- SQL Injection (auch bei ORM: raw queries?)
- Command Injection
- Path Traversal

AUTHENTICATION/AUTHORIZATION:
- Werden User-Berechtigungen auf jeder Ebene geprüft?
- Sind JWTs korrekt validiert (Signatur + Expiry + Audience)?
- Horizontal Privilege Escalation möglich?

DATA EXPOSURE:
- Werden sensible Daten geloggt? (Kreditkartennummern, Tokens)
- Gibt es Information Leakage in Error Messages?

RATE LIMITING / DOS:
- Kann ein User unbegrenzt Ressourcen konsumieren?
- Gibt es teure Operationen ohne Schutz?

DEPENDENCIES:
- Neue npm/pip Packages? Bekannte CVEs?

OUTPUT: Tabellarisch — Risiko (Critical/High/Medium/Low), 
        Zeile, Beschreibung, Empfehlung
```

---

## 7. Dokumentation & Inline-Kommentare

### 7.1 Das WHY > WHAT Prinzip

```python
# SCHLECHTER Kommentar — beschreibt WAS (ist aus dem Code ersichtlich):
# Iteriere über alle Payments
for payment in payments:
    payment.mark_as_processed()

# GUTER Kommentar — erklärt WARUM (nicht aus Code ersichtlich):
# Payments müssen sequentiell (nicht parallel) verarbeitet werden,
# da jede Verarbeitung den Balance-Counter des Users aktualisiert
# und concurrent updates zu Race Conditions führen würden.
# Ticket: INFRA-445, ADR-012
for payment in payments:
    payment.mark_as_processed()
```

### 7.2 Docstring-Generierungs-Prompt

```
PROMPT: Docstrings generieren

Generiere Google-Style Docstrings für alle public Methoden 
im folgenden Code.

ANFORDERUNGEN:
- Args: Jeden Parameter mit Typ und Bedeutung (nicht nur Typ)
- Returns: Was genau zurückgegeben wird (nicht nur Typ)
- Raises: Jede Exception mit Auslösebedingung
- Example: Ein realistisches Nutzungsbeispiel
- Notes: Wichtige Implementierungsdetails die Nutzer wissen müssen
         (z.B. "Diese Methode ist nicht thread-safe")

STIL:
- Erste Zeile: Imperativ, max. 80 Zeichen, ohne Punkt am Ende
- Erkläre WARUM, nicht WAS — der Code zeigt das WAS selbst
- Keine Trivialitäten: Kein "Diese Funktion addiert zwei Zahlen"

CODE:
[CODE]
```

### 7.3 Automatisierter Dokumentations-Loop

```
SCHRITT 1: Code generieren (Docstrings minimal)
SCHRITT 2: Tests schreiben und bestanden
SCHRITT 3: Dokumentations-Prompt für vollständige Docstrings
SCHRITT 4: README-Update für das Modul
SCHRITT 5: API-Dokumentation (OpenAPI-Beschreibungen für FastAPI)
```

**README-Update-Prompt:**
```
Aktualisiere den Abschnitt [MODUL_NAME] in der README.

NEUES MODUL:
[CODE ODER BESCHREIBUNG]

README-STIL (aus bestehendem README ableiten):
[EXISTIERENDER README ABSCHNITT ALS BEISPIEL]

PFLICHTINHALTE:
- Zweck in einem Satz
- Quick Start Beispiel (copy-pastable)
- Konfigurationsoptionen als Tabelle
- Häufige Fehler und Lösungen
- Link zu relevanten ADRs

VERBOTEN:
- Marketingsprache ("powerful", "robust", "easy-to-use")
- Redundante Informationen die schon in den Docstrings stehen
```

---

## 8. Testing-Infrastruktur

### 8.1 Die Test-Pyramide mit LLM-Unterstützung

```
           /\
          /  \   E2E Tests (10%)
         /────\  → LLM: Test-Szenarien definieren
        /      \ Integration Tests (30%)
       /────────\ → LLM: Test-Daten generieren, Fixture-Setup
      /          \
     /────────────\ Unit Tests (60%)
    /              \ → LLM: Vollständige Generierung
   /────────────────\
```

### 8.2 Test-Coverage-Analyse-Prompt

```
PROMPT: Coverage-Gap-Analyse

Coverage-Report:
[PYTEST COVERAGE OUTPUT]

Source-Code:
[MODULE CODE]

Analysiere:
1. Welche Branches werden NICHT getestet?
2. Welche dieser un-getesteten Branches sind KRITISCH 
   (Fehler-Handling, Security, Business Logic)?
3. Generiere Tests NUR für die Top-5 kritischen Lücken.

Priorisierung:
- Erst: Error-Handling-Pfade
- Dann: Grenzwerte in Geschäftslogik  
- Zuletzt: Happy-Path-Variationen
```

### 8.3 Integrations-Test-Template

```python
# PROMPT: Integration Test generieren

"""
Generiere einen Integrations-Test für das folgende Szenario:
SZENARIO: Payment Intent → Stripe-Call → DB-Persistenz → Event publish

INFRASTRUKTUR:
- PostgreSQL: Testcontainer (bereits konfiguriert in conftest.py)
- Redis: Fakeredis (bereits konfiguriert)
- Stripe: Recorded HTTP-Responses in /tests/fixtures/stripe_responses/
- Kein echter Stripe-API-Call im Test!

ANFORDERUNGEN:
1. Test ist isoliert (kein Shared State mit anderen Tests)
2. Cleanup nach Test (DB-Rollback oder explicit delete)
3. Assertions auf allen Ebenen:
   - API Response Status und Body
   - DB-Zustand nach Operation
   - Publizierte Events
   - Stripe-Anfragen (wurden sie korrekt gesendet?)

FIXTURE-PATTERN (aus bestehendem Beispiel):
[EXISTIERENDES INTEGRATION TEST BEISPIEL]
"""
```

### 8.4 Property-Based Testing mit Hypothesis

```
PROMPT: Property-Based Tests

Analysiere den folgenden Code und identifiziere Invarianten 
die IMMER gelten müssen (unabhängig vom Input).

CODE:
[CODE]

Für jede Invariante:
1. Formuliere sie als Eigenschaft (z.B. "Ausgabe immer >= 0")
2. Generiere einen Hypothesis-Test der diese Eigenschaft prüft
3. Welche Edge Cases deckt Hypothesis automatisch ab?

Nutze st.composite für komplexe Domain-Objekte.
Boundary-Strategien explizit definieren (z.B. Preise: st.decimals(min_value=0.01))
```

---

## 9. LLM-Schwachstellen & Gegenmaßnahmen

### 9.1 Systematischer Überblick: Schwachstellen-Katalog

| Schwachstelle | Symptom | Gegenmaßnahme | Prompt-Technik |
|--------------|---------|---------------|----------------|
| **Halluzination von APIs** | Methoden die nicht existieren | API-Docs als Kontext beifügen | `"Nutze NUR Methoden die in [DOCS] dokumentiert sind"` |
| **Temporal Blindheit** | Veraltete Patterns/Libraries | Explizite Versionsangabe | `"FastAPI 0.110, Python 3.12 — kein 0.9x Syntax"` |
| **Kontextverlust** | Ignoriert frühere Entscheidungen | FAILURE_LOG + Decision Log | Regeln in jeden Prompt wiederholen |
| **Overengineering** | Zu komplexe Lösung für einfaches Problem | Explicit simplicity constraint | `"Einfachste mögliche Lösung — kein Premature Abstraction"` |
| **Inkonsistenz** | Verschiedene Stile im selben Codebase | Codebase-Beispiel als Kontext | Existierenden Code als Referenz beifügen |
| **Sycophancy** | Bestätigt falsche Annahmen des Users | Explizit widersprechen auffordern | `"Widersprich mir wenn ich falsch liege"` |
| **Scope Creep** | Fügt ungefragte Features hinzu | Explicit scope boundary | `"NUR das Beschriebene — keine zusätzlichen Features"` |
| **Test-Theater** | Tests die nichts testen | Tests auf Assertions prüfen | Review-Prompt: "Haben diese Tests echten Wert?" |
| **Sicherheitslücken** | XSS, Injection, Auth-Bypass | Security-Review-Prompt nach Generierung | Systematischer Security-Checklist-Prompt |
| **Race Conditions** | State nicht thread-safe | Explicit concurrency constraints | `"Ist dieser Code sicher bei 100 concurrent calls?"` |

### 9.2 Anti-Halluzinations-Protokoll

**Für Library-spezifischen Code:**
```
PROMPT: API-Verifikation

WICHTIG: Bevor du Code mit [LIBRARY] generierst:

1. Liste alle Library-Methoden die du nutzen willst
2. Gib für jede die Quelle an (Docstring, docs.library.io/...)
3. Wenn du dir nicht 100% sicher bist: SCHREIB ES HIN mit 
   "⚠️ VERIFY: Diese API muss geprüft werden"

Ich würde lieber einen Kommentar mit TODO als falschen Code.

Relevante Dokumentation:
[EINGEFÜGTE DOKUMENTATIONS-ABSCHNITTE]
```

**Praxis-Regel:** Bei unbekannten Libraries immer die Docs als Kontext einfügen:
```python
# docs_context.py — wird als Kontext-Datei genutzt
"""
SQLAlchemy 2.0 Async Session Usage (from official docs):

# Correct pattern:
async with AsyncSession(engine) as session:
    async with session.begin():
        result = await session.execute(select(User).where(User.id == user_id))
        user = result.scalar_one_or_none()

# WRONG (SQLAlchemy 1.x pattern - DO NOT USE):
session = Session(engine)  
user = session.query(User).filter_by(id=user_id).first()
"""
```

### 9.3 Anti-Sycophancy-Protokoll

LLMs neigen dazu, dem User zuzustimmen. Explizit dagegen steuern:

```
STANDARD-SUFFIX für alle Architektur-/Design-Fragen:

"Wenn du meinst, dass meine Annahme oder mein Ansatz falsch ist, 
WIDERSPRICH mir klar und begründe es. Ich bevorzuge eine direkte 
Korrektur über eine höfliche Bestätigung. Sei wie ein ehrlicher 
Senior-Kollege, nicht wie ein Yes-Man."
```

```
DEVIL'S ADVOCATE PROMPT:

Ich habe folgende Entscheidung getroffen: [ENTSCHEIDUNG]

Spiele jetzt den Advocatus Diaboli:
- Was sind die stärksten Argumente GEGEN diese Entscheidung?
- Welche Szenarien würden diese Entscheidung scheitern lassen?
- Was würde ein kritischer Tech-Lead in einem Code-Review bemängeln?

Sei so kritisch wie möglich. Überzeuge mich, dass ich falsch liege.
```

### 9.4 Konsistenz durch Canonical Examples

```markdown
# CODING_STANDARDS.md — wird als Kontext beigefügt

## Canonical Code Examples — DIESE PATTERNS IMMER NUTZEN

### Async DB Query Pattern:
```python
# ✅ RICHTIG:
async def get_payment(self, payment_id: UUID) -> Payment | None:
    async with self._session_factory() as session:
        result = await session.execute(
            select(PaymentModel).where(PaymentModel.id == payment_id)
        )
        model = result.scalar_one_or_none()
        return self._to_domain(model) if model else None

# ❌ FALSCH (mehrere Anti-Patterns):
async def get_payment(self, payment_id):
    session = Session(engine)  # Kein Context Manager
    payment = session.query(Payment).filter_by(id=payment_id).first()  # Sync
    return payment  # Session wird nie geschlossen
```

### Error Handling Pattern:
```python
# ✅ RICHTIG:
try:
    result = await stripe_client.charge(intent)
except stripe.CardError as e:
    raise PaymentDeclinedError(
        message=e.user_message,
        decline_code=e.code,
        payment_id=intent.id
    ) from e
except stripe.APIConnectionError as e:
    raise ProviderUnavailableError(
        message="Stripe nicht erreichbar",
        retryable=True
    ) from e

# ❌ FALSCH:
try:
    result = await stripe_client.charge(intent)
except Exception as e:
    print(f"Error: {e}")  # Silent failure, kein Re-raise
    return None
```
```

---

## 10. Agile Integration & Iteration

### 10.1 Der Sprint-Initialisierungs-Loop

```
SPRINT START (Montag):
─────────────────────
PROMPT 1: Sprint-Kontext aktualisieren
"Wir starten Sprint [N]. Ziele: [SPRINT_GOALS].
 Vorherige Velocity: [X Story Points].
 Technische Schulden aus letztem Sprint: [DEBT_LIST].
 
 Erstelle:
 1. Aktualisierte SPRINT_CONTEXT.md
 2. Technische Risiken für diesen Sprint
 3. Abhängigkeiten zwischen User Stories"

PROMPT 2: Aufgaben-Breakdown
"Breche US-XXX in technische Tasks auf.
 Format: Checkliste mit Schätzung in Stunden.
 Markiere Tasks die LLM-unterstützt werden können vs. 
 Tasks die menschliche Entscheidung brauchen."
```

### 10.2 Der Tages-Loop (Daily Development Cycle)

```
MORGEN (Kontext aufbauen):
──────────────────────────
1. PROJECT_CONTEXT.md laden
2. SPRINT_CONTEXT.md laden  
3. FAILURE_LOG.md laden
4. Gestrigen Code-Stand laden
→ "Ich arbeite heute an [TASK]. Zusammenfassung des Standes: [STAND]"

ENTWICKLUNG (Iterations-Loop):
───────────────────────────────
For each sub-task:
  1. Interface-Prompt
  2. Test-Prompt
  3. Implementation-Prompt
  4. Review-Prompt
  5. Integration-Test
  └─ Wenn Fehler: Debug-Prompt → zurück zu 3

ABEND (Kontext sichern):
────────────────────────
"Fasse zusammen was heute implementiert wurde.
 Format für SPRINT_CONTEXT.md Update:
 - Was ist fertig
 - Was ist noch offen
 - Neue Erkenntnisse/Entscheidungen
 - Probleme die morgen gelöst werden müssen"
```

### 10.3 Der Retrospektiven-Loop

```
SPRINT ENDE:
────────────
PROMPT: Sprint Retrospektive mit LLM

"Ich gebe dir folgende Sprint-Daten:
 - Geplante vs. fertige Story Points: [X vs. Y]
 - Code-Review-Kommentare: [LISTE]
 - Bugs gefunden: [LISTE]
 - FAILURE_LOG Einträge dieser Sprint: [EINTRÄGE]

Analysiere:
1. Welche LLM-Prompt-Patterns haben gut funktioniert?
2. Wo war der generierte Code qualitativ schwach?
3. Welche neuen Regeln sollten in FAILURE_LOG oder PROJECT_CONTEXT?
4. Prompt-Vorschläge die den nächsten Sprint verbessern?

Sei spezifisch — keine allgemeinen Ratschläge."
```

### 10.4 Kontinuierliche Verbesserung der Prompt-Qualität

**Prompt-Versionierung:**
```markdown
# PROMPT_VERSIONS.md

## payment_processor_impl_prompt

### v3 (aktuell — 2024-03-20)
Grund für Update: v2 generierte oft sync DB-Calls
Änderung: Expliziter Hinweis auf async context manager
Verbesserung: 0 sync-DB-Fehler in letzten 10 Generierungen

### v2 (deprecated — 2024-03-10)  
Problem: Idempotenz-Checks wurden vergessen
→ FAILURE_LOG Eintrag: INFRA-445

### v1 (deprecated — 2024-03-01)
Initiale Version
```

---

## 11. Qualitätssicherung & Review-Loops

### 11.1 Der Pre-Commit Review Loop

```bash
# .pre-commit-hooks / Makefile target
# Automatisiert mit git hooks

pre-llm-review:
    @echo "Sending diff to LLM for review..."
    @git diff --staged | python scripts/llm_review.py \
        --context PROJECT_CONTEXT.md \
        --failure-log FAILURE_LOG.md \
        --checklist .review_checklist.md
```

**review_checklist.md:**
```markdown
# PRE-COMMIT REVIEW CHECKLIST

Prüfe den folgenden Git-Diff auf:

## BLOCKER (kein Merge ohne Fix):
- [ ] Security-Vulnerabilities (Checklist aus Section 6.5)
- [ ] Regression von FAILURE_LOG-bekannten Problemen
- [ ] Breaking Changes an Public Interfaces ohne Version Bump
- [ ] Hardcodierte Secrets oder Credentials
- [ ] Tests entfernt ohne Ersatz

## WARNINGS (sollte vor Merge behoben werden):
- [ ] Verletzung der Architektur-Regeln aus PROJECT_CONTEXT
- [ ] Fehlende Type Hints in neuen Funktionen
- [ ] Fehlende Docstrings in Public Methods
- [ ] Coverage-Rückgang >2%

## INFO (kann in Follow-up behoben werden):
- [ ] Performance-Hinweise
- [ ] Refactoring-Möglichkeiten
- [ ] Kommentar-Qualität
```

### 11.2 Der Code-Quality-Gate-Prompt

```
PROMPT: Pull Request Review

PR-Beschreibung: [PR_DESCRIPTION]
Verlinktes Ticket: [TICKET_URL_INHALT]
Git Diff:
[GIT_DIFF]

Reviewe als Senior Engineer mit Fokus auf:

KORREKTHEIT:
- Erfüllt der Code was das Ticket beschreibt?
- Gibt es logische Fehler?
- Sind alle Edge Cases aus dem Ticket behandelt?

ARCHITEKTUR:
- Verletzt irgendetwas die Regeln in PROJECT_CONTEXT.md?
- Entstehen neue technische Schulden?
- Passt das in die bestehende Architektur?

TESTBARKEIT:
- Sind die Tests aussagekräftig?
- Testen sie wirklich das Verhalten oder nur die Implementierung?
- Gibt es kritische ungetestete Pfade?

FORMAT: 
- Jeder Punkt als GitHub-Review-Kommentar formatiert
- Zeile angeben
- Severity: [BLOCKER | IMPORTANT | SUGGESTION | NITPICK]
```

### 11.3 Performance-Profiling-Loop

```
PROMPT: Performance-Analyse

Dieser Code wird aufgerufen:
- Frequenz: [X Requests/Sekunde]
- P95 Latenz-Ziel: <[Y]ms
- Daten-Größenordnung: [Z Rows/Documents]

CODE:
[CODE]

Analysiere:
1. Zeitkomplexität jeder Operation (O-Notation)
2. Potenzielle Bottlenecks bei der angegebenen Last
3. N+1 Query Patterns
4. Unnötige Wiederholungen/Redundanzen
5. Caching-Möglichkeiten (was ist safe to cache? TTL?)

Für jeden gefundenen Bottleneck:
- Schätzung der Auswirkung (wie viel Latenz wird eingespart?)
- Konkreter Fix als Code-Snippet
- Trade-offs des Fixes
```

---

## 12. Referenz: Prompt-Bibliothek

### Schnellzugriff: Situative Prompt-Auswahl

```
SITUATION                          → PROMPT-PATTERN
────────────────────────────────────────────────────
Neues Projekt starten              → Domain Discovery (3.2) + SCAFFOLD (6.1)
Neues Feature                      → Interface-First (5.2) → Tests (5.3) → Impl (5.4)
Bug fixen                          → DEBUG-Pattern (6.3)
Performance-Problem                → Performance-Analyse (11.3)
Code refaktorieren                 → REFACTOR-Pattern (6.2)
DB-Schema ändern                   → MIGRATION-Pattern (6.4)
Security-Review vor Deploy         → SECURITY-REVIEW (6.5)
PR reviewen                        → Code-Quality-Gate (11.2)
Dokumentation schreiben            → Docstring-Prompt (7.2) + README-Update
Sprint planen                      → Sprint-Initialisierung (10.1)
Sprint abschließen                 → Retrospektive (10.3)
LLM macht Fehler immer wieder      → FAILURE_LOG erweitern + Anti-Patterns (9.4)
```

---

### 12.1 Der Universal-Kontext-Header

Dieser Header wird VOR jeden Implementierungs-Prompt gesetzt:

```
═══════════════════════════════════════════════
PROJEKT-KONTEXT (immer beachten)
═══════════════════════════════════════════════
Tech-Stack: [STACK]
Architektur: [MUSTER]
Code-Standards: Type hints, Google Docstrings, structlog
Verboten: [TOP 3 VERBOTE]
Relevante FAILURE_LOG Einträge: [1-3 RELEVANTE EINTRÄGE]
═══════════════════════════════════════════════
AUFGABE:
```

---

### 12.2 Goldene Regeln — Zusammenfassung

```
┌─────────────────────────────────────────────────────────────┐
│               DIE 10 GOLDENEN REGELN                         │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  1. KONTEXT ZUERST — Kein Prompt ohne PROJECT_CONTEXT        │
│                                                              │
│  2. FAILURE_LOG IMMER DABEI — Fehler nicht wiederholen       │
│                                                              │
│  3. INTERFACE VOR IMPLEMENTIERUNG — Vertrag zuerst           │
│                                                              │
│  4. TESTS VOR CODE — TDD auch mit LLMs                       │
│                                                              │
│  5. REVIEW NACH JEDER GENERIERUNG — Nie blind übernehmen     │
│                                                              │
│  6. VERBOTE EXPLIZIT — Was nicht sein soll, muss gesagt sein │
│                                                              │
│  7. EIN SCHRITT AUF EINMAL — Kein "baue alles auf einmal"    │
│                                                              │
│  8. KANONISCHE EXAMPLES — Stil durch Beispiele vorgeben      │
│                                                              │
│  9. DEVIL'S ADVOCATE — LLM kritisieren lassen was es baute   │
│                                                              │
│ 10. PROMPT VERSIONIEREN — Was gut funktioniert, dokumentieren│
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

---

### 12.3 Kontext-Dateien Checkliste

```
Projekt-Setup — Diese Dateien einmalig erstellen:
─────────────────────────────────────────────────
[ ] PROJECT_CONTEXT.md       ← Stack, Architektur, Standards, Verbote
[ ] CODING_STANDARDS.md      ← Canonical Code Examples
[ ] FAILURE_LOG.md           ← Leer starten, kontinuierlich füllen
[ ] ADR/                     ← Architektur-Entscheidungen
[ ] PROMPT_VERSIONS.md       ← Versionierte Prompts
[ ] .review_checklist.md     ← Pre-commit Review Regeln

Pro Sprint:
───────────
[ ] SPRINT_CONTEXT.md        ← Ziele, Fokus, aktuelle Entscheidungen

Pro Session:
────────────
[ ] TASK_CONTEXT.md          ← Spezifische Aufgabe, relevante Files
```

---

*Dieses Regelwerk ist ein lebendes Dokument. Jeder Sprint sollte neue Erkenntnisse in die entsprechenden Abschnitte — insbesondere FAILURE_LOG und Prompt-Bibliothek — einfließen lassen. Die beste Version dieses Dokuments entsteht durch das Team, nicht durch einen einzelnen Autor.*

---

> **Lizenz:** Internes Regelwerk — angepasst für das jeweilige Projektteam.  
> **Maintainer:** Rolle: Senior Context-Architekt / Tech Lead  
> **Letzte Aktualisierung:** Automatisch aus Sprint-Retrospektiven


---


**# LLM-gestütztes Softwareentwicklungs-Framework (LLM-SDF)**  
**Fortschrittliches Regelwerk für vollständige Softwareentwicklung mit LLMs & Coding Agents**

**Version:** 1.0 (agil & iterativ erweiterbar)  
**Zielgruppe:** LLM-Enthusiasten, Senior Context-Architekten, Entwickler-Teams  
**Zweck:** Ein vollständiges, praxiserprobtes Regelwerk, das LLM-Schwächen (Halluzinationen, Kontextverlust, inkonsistente Logik, begrenzte Kreativität bei Edge-Cases) systematisch minimiert und die Stärken (Geschwindigkeit, Wissensbreite, parallele Iteration) maximal ausnutzt.

---

## 1. Grundprinzipien (Fundament)

### 1.1 LLM-spezifische Schwächen & Gegenmaßnahmen
- **Halluzinationen & Falschwissen**: Immer mit `Verify & Ground` (Quellen, Tests, formale Spezifikationen).
- **Kontextverlust**: Strukturierte Context-Management (Chunking, RAG-ähnlich, Rolling Context).
- **Inkonsistente Architektur**: Zentrale "Single Source of Truth" (Architecture Decision Record + Master Prompt).
- **Fehlende Langzeitgedächtnis**: Explizites Memory-System (Project.md, Decisions.md, Lessons-Learned.md).
- **Kreativitäts-Plateaus**: Forced Exploration + Divergent → Convergent Loops.

**Kernprinzipien (MERIT-Framework)**:
- **M**odular & Atomic (kleinste sinnvolle Einheiten)
- **E**xplicit Context & Documentation
- **R**edundant Verification (multi-stage)
- **I**terative Refinement Loops
- **T**raceable & Reproducible Decisions

---

## 2. Projektplanung & Initialisierung

### 2.1 Kick-off Prompt-Struktur (Master Context)
```markdown
Du bist Senior Software Architect & Principal Engineer mit 20+ Jahren Erfahrung.

**Projektname:** [Name]
**Ziel:** [Klarer, messbarer Erfolgskriterium]
**Stakeholder:** [...]
**Nicht-Ziele:** [Scope Exclusion]

**Verfügbare Ressourcen:** [Tech-Stack, Budget, Zeit]

Erstelle:
1. Product Vision Statement (1 Satz)
2. MoSCoW-Priorisierung der Features
3. High-Level Risiko-Matrix (inkl. LLM-spezifischer Risiken)
4. Initiale Iteration Roadmap (2-Wochen-Sprints)
5. Erforderliche Context-Dateien (Project.md, Architecture.md etc.)
```

**Folge-Schritt:** LLM generiert → Mensch validiert → In `docs/` committen.

### 2.2 Context-Architektur (Dateisystem-basiert)
- `docs/Project.md` – Living Document (Vision, Glossar, Entscheidungen)
- `docs/Architecture/ADR-001.md` – Architecture Decision Records
- `prompts/` – Wiederverwendbare System-Prompts
- `context/` – Aktueller Rolling Context pro Modul
- `tests/` & `specs/`

---

## 3. Anforderungsanalyse & Spezifikation

**Technik:** Specification by Example + Gherkin + formale User Stories.

**Beispiel-Prompt für Feature-Spezifikation:**
```markdown
Erstelle eine vollständige, testbare Spezifikation für Feature X.

**Akzeptanzkriterien (Gherkin):**
Given ...
When ...
Then ...

Berücksichtige:
- Edge Cases (leere Eingaben, hohe Last, Security, Internationalisierung)
- LLM-typische Fehlerquellen (z.B. falsche Annahmen über State)
- Performance-Budgets
- Observability-Anforderungen (Logging, Metrics, Traces)
```

Output → `specs/feature-x.md` → in Tests überführen.

---

## 4. Architektur-Design

**Prinzip:** C4-Model + Domain-Driven Design + LLM-gestützte Exploration.

**Iterativer Architektur-Loop:**
1. **Divergent Phase**: Mehrere alternative Architekturen vorschlagen lassen (Prompt mit "Generate 3 fundamentally different approaches").
2. **Evaluation**: Bewertungsmatrix (Maintainability, Scalability, LLM-Implementierbarkeit, Cost).
3. **Convergent Phase**: Entscheidung + ADR schreiben.
4. **Validation**: LLM soll Code-Skelett generieren und auf Konsistenz prüfen.

**Master-Architecture-Prompt** enthält immer:
- Aktuelle C4-Diagramme (PlantUML/Text)
- Tech-Stack Constraints
- Frühere ADRs

---

## 5. Implementierung (Coding Loops)

### 5.1 Atomic Coding Cycle (pro Funktion/Komponente)
```markdown
1. Context laden (relevante Dateien + specs + ADRs)
2. Implementierungs-Prompt (sehr explizit)
3. Code generieren
4. Self-Critique Prompt
5. Test-Generierung
6. Ausführen + Fix-Loop bis grün
7. Refactoring & Documentation
8. Commit mit detaillierter Message
```

**Beispiel Implementierungs-Prompt:**
```markdown
**Context:**
- [Inhalt von relevanten Dateien via <file> Tags oder Tool-Calls]
- Aktuelle Architektur-Regeln
- Coding Standards (z.B. Clean Code, TypeScript strict)

**Aufgabe:** Implementiere Funktion `processOrder(...)`

**Constraints:**
- Keine Side-Effects außer explizit dokumentiert
- Vollständige Fehlerbehandlung
- Inline-Kommentare nur bei komplexer Logik (Warum, nicht Was)
- Performance < 50ms p95

**Output-Format:**
```ts
// Dateipfad
Code

// Tests (Jest/Vitest)
```

Danach: Selbstkritik mit separatem Prompt: "Finde alle potenziellen Bugs, Inkonsistenzen und Verbesserungen."
```

### 5.2 Schwächenbeseitigung während Coding
- **Multi-Agent Simulation**: "Du bist Reviewer A (Security), Reviewer B (Performance), Reviewer C (Maintainability)".
- **Chain-of-Verification (CoVe)**: Generiere → Beweise Korrektheit → Gegenbeispiel finden → Korrigieren.
- **Golden Tests**: Wichtige Funktionen mit LLM-generierten aber menschlich validierten Testfällen "vergolden".

---

## 6. Dokumentation & Inline Comments

**Regel:** "Documentation is code" – immer synchron halten.

- **Inline**: Nur "Warum" + komplexe Trade-offs. Kein "Was" (der Code sagt es).
- **READMEs & docs/**: LLM generiert, Mensch curatiert.
- **Auto-Doc Loops**: Nach Refactoring Prompt: "Aktualisiere alle betroffenen Dokumentationen und Kommentare konsistent."

**Prompt für Dokumentation:**
```markdown
Aktualisiere die Dokumentation für Modul X basierend auf dem neuen Code.
Stelle sicher, dass:
- Alle öffentlichen APIs dokumentiert sind
- Migration Notes vorhanden
- Architecture Diagram konsistent
```

---

## 7. Testing & Quality Infrastructure

**Testing Pyramid + Property-Based + LLM-Enhanced:**

- **Unit Tests**: LLM-generiert + menschlich erweitert
- **Integration & E2E**: Stark menschlich kuratiert
- **Property-Based Testing** (fast-check, hypothesis): Für komplexe Logik
- **LLM-as-Tester**: "Generiere 20 diverse Testfälle inkl. adversarial inputs"
- **Mutation Testing** & Coverage Gates (>90%)

**CI/CD Integration**: Jeder PR durchläuft:
1. Linter + Type Check
2. Test Suite
3. LLM-Critique (optionaler Agent: "Finde Architectural Drift")

---

## 8. Iterations- & Feedback-Loops (Agil & Dynamisch)

### 8.1 Der zentrale PDCA-Loop (Plan-Do-Check-Act) für LLMs
```
Plan:   Neue Anforderung / Bug / Refactoring → Spec + Context
Do:     LLM-generierter Change (Atomic)
Check:  Tests + Self-Critique + Human Review + Metrics
Act:    Merge / Fix-Loop / Lessons Learned ins Memory
```

**Rolling Context Management:**
- Vor jedem großen Prompt: `summarize_context.py` (Zusammenfassung der letzten Änderungen)
- Wichtige Erkenntnisse → `docs/Lessons-Learned.md`

**Sprint-Review Prompt (Ende Iteration):**
```markdown
Analysiere die letzte Iteration:
- Was lief gut (LLM-Stärken)
- Welche Fehler traten auf (Halluzinationen, Scope Creep etc.)
- Verbesserungsvorschläge für Prompt-Engineering & Context
- Update der Master-Prompts
```

---

## 9. Fortgeschrittene Techniken

- **Tree of Thoughts / Graph of Thoughts**: Für komplexe Features.
- **Self-Consistency Sampling**: Mehrere Lösungen generieren → Voting.
- **Tool-Use Orchestration**: LLM entscheidet, wann es bash, Tests, Linter etc. nutzen soll.
- **Evolving Master Prompt**: Die System-Prompts werden selbst iterativ verbessert (Meta-Prompting).
- **Human-in-the-Loop Gates**: Kritische Entscheidungen (Security, Architecture, Business Logic) immer menschlich.

---

## 10. Beispiel: Kompletter Feature-Entwicklungs-Flow

1. Feature-Idee in `Project.md`
2. Spezifikation + Gherkin
3. Architektur-Exploration → ADR
4. Atomic Implementation Loop (5-15 min pro Zyklus)
5. Tests grün + Critique
6. Refactor + Doc-Update
7. Integration in Main
8. End-to-End Validation
9. Lessons Learned

---

## 11. Anti-Patterns (was vermieden werden muss)

- Monolithische Prompts (>4k Tokens ohne Struktur)
- "Trust but not Verify"
- Kein explizites Memory → wiederholte Fehler
- Zu frühes Coding ohne klare Spec
- Ignorieren von LLM-Fehlermustern (z.B. Off-by-one, falsche Imports)

---

## 12. Skalierung & Team-Nutzung

- **Agent Roles**: Product Owner Agent, Architect Agent, Coder Agent, Tester Agent, Reviewer Agent.
- **Orchestrierung**: LangChain / CrewAI / Custom Orchestrator oder einfaches Shell-Scripting.
- **Wissensmanagement**: Vector-DB für Projekt-Knowledge (optional, aber mächtig).

---

**Dieses Framework ist bewusst lebendig.**  
Jede Iteration sollte das Dokument selbst verbessern (`docs/LLM-SDF.md` updaten).

**Nächste Schritte für dich:**
1. Dieses Dokument in dein neues Projekt kopieren.
2. `Project.md` initialisieren.
3. Ersten Master-Prompt anpassen.
4. Starte mit einem kleinen Feature und iteriere das Framework selbst.

**Viel Erfolg!** Dieses Regelwerk hat in realen Projekten die Entwicklungs-Geschwindigkeit bei gleichzeitig höherer Qualität massiv gesteigert.

---

*Erstellt als agiles Living Document – Pull Requests & Verbesserungsvorschläge willkommen.*


---


