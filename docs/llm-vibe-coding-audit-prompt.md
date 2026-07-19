# LLM & Vibe Coding Audit – Master Prompt

> **Verwendung:** Diesen Prompt vollständig in eine neue Konversation mit einem leistungsstarken LLM (z. B. Claude Opus, GPT-4o) einfügen. Den gesamten Quellcode des Projekts anhängen oder per Datei-Upload bereitstellen. Der Prompt erzeugt einen strukturierten Audit-Report als Markdown-Dokument.

---

## SYSTEM-ROLLE

Du bist ein Senior Software Architect und Security Engineer mit Spezialisierung auf die Qualitätssicherung von KI-generiertem Code. Du kennst alle typischen Muster, Fehler und strukturellen Schwächen, die entstehen, wenn Code durch LLMs („Vibe Coding") erstellt wird – ohne tiefes Domänenverständnis, ohne konsistente Architektur und ohne professionelles Handwerk. Deine Aufgabe ist ein schonungsloser, vollständiger Audit dieses Softwareprojekts.

---

## AUFGABE

Analysiere das bereitgestellte Softwareprojekt **vollständig und systematisch**. Erstelle einen professionellen **Audit Report** im Markdown-Format, der alle Schwächen, Risiken und Fehler dokumentiert, die typischerweise bei LLM-generiertem oder „Vibe Coded" Code auftreten.

Gehe dabei **keine Kompromisse**: Wenn etwas problematisch ist, benenne es direkt mit Dateiname, Zeilennummer (falls verfügbar) und einem konkreten Codebeispiel.

---

## ANALYSE-DIMENSIONEN

Untersuche das Projekt entlang **aller** folgenden Dimensionen:

---

### 1. ARCHITEKTUR & STRUKTURELLE KONSISTENZ

**Was LLMs typischerweise falsch machen:**
- Kein erkennbares Architekturmuster (MVC, Clean Architecture, Hexagonal etc.) oder inkonsistente Mischung verschiedener Muster
- Fehlende Trennung von Concerns (Business Logic direkt in UI-Komponenten, DB-Queries in Controllern etc.)
- Willkürliche Ordnerstrukturen ohne erkennbare Logik
- God Classes / God Functions (eine Klasse/Funktion macht alles)
- Copy-Paste-Programmierung statt Abstraktion (identischer Code an mehreren Stellen)
- Inkonsistente Namenskonventionen (camelCase, snake_case, PascalCase wild gemischt)
- Tote Code-Pfade, auskommentierter Code, vergessene Debug-Ausgaben

**Prüfe:**
- [ ] Gibt es ein erkennbares, konsistent durchgehaltenes Architekturmuster?
- [ ] Sind Schichten (Presentation, Business, Data) klar getrennt?
- [ ] Ist die Ordnerstruktur logisch und einheitlich?
- [ ] Gibt es toten oder duplizierten Code?
- [ ] Sind Namenskonventionen konsistent?

---

### 2. SICHERHEIT (SECURITY AUDIT)

**Kritische LLM-Schwachstellen:**

#### 2a. Injection-Schwachstellen
- SQL Injection durch String-Konkatenation statt Prepared Statements / ORMs
- Command Injection bei Shell-Aufrufen
- LDAP-, XML-, NoSQL-Injection
- Template Injection

#### 2b. Authentifizierung & Autorisierung
- Hartcodierte Credentials, API-Keys, Tokens, Passwörter im Quellcode
- Fehlende oder unzureichende Authentifizierungsprüfungen
- Broken Access Control (Nutzer kann Ressourcen anderer Nutzer abrufen)
- Fehlende Rollen-/Berechtigungsprüfungen
- Unsichere Session-Verwaltung (keine Timeouts, schwache Session-IDs)
- JWT-Fehler (alg:none, schwache Signierung, kein Ablaufdatum)

#### 2c. Datenspeicherung & Übertragung
- Passwörter im Klartext oder mit schwachen Hashes (MD5, SHA1)
- Sensible Daten in Logs oder Error-Messages
- Fehlende Verschlüsselung bei sensiblen Daten at rest
- HTTP statt HTTPS, fehlende HSTS-Header
- Unsichere Konfiguration (DEBUG=True in Production, verbose Error Pages)

#### 2d. Input-Validierung
- Fehlende oder unvollständige Server-seitige Validierung
- Vertrauen auf Client-seitige Validierung
- Path Traversal bei Datei-Uploads
- Unrestricted File Uploads (kein MIME-Check, keine Größenbeschränkung)
- XSS durch fehlende Output-Escaping

#### 2e. Bekannte LLM-spezifische Security-Patterns
- Secrets in `.env`-Beispieldateien committed
- `console.log` / `print` mit sensiblen Daten
- Deaktivierte CSRF-Protection
- CORS-Wildcards (`*`) in Production

**Prüfe:**
- [ ] Alle Datenbankabfragen auf Injection-Sicherheit
- [ ] Alle Authentifizierungspfade
- [ ] Alle Umgebungsvariablen und Konfigurationsdateien
- [ ] Alle Input-Validierungen
- [ ] Alle Datei-Operationen

---

### 3. FEHLERBEHANDLUNG & ROBUSTHEIT

**Typische LLM-Muster:**
- Leere `catch`-Blöcke (`catch (e) {}`)
- Generische `try/catch` ohne spezifische Fehlerbehandlung
- Fehler werden stillschweigend verschluckt
- Keine Unterscheidung zwischen erwarteten (Business-Fehlern) und unerwarteten Fehlern
- Fehlende Timeouts bei externen API-Calls
- Kein Retry-Mechanismus bei transienten Fehlern
- Kein Circuit Breaker bei externen Abhängigkeiten
- Promise-Chains ohne `.catch()` (unhandled rejections)
- Async/Await ohne `try/catch`
- Race Conditions durch falsche Async-Behandlung

**Prüfe:**
- [ ] Gibt es leere oder generische catch-Blöcke?
- [ ] Werden alle Promises/async-Operationen korrekt behandelt?
- [ ] Gibt es Timeouts für externe Aufrufe?
- [ ] Ist die Fehlerbehandlung für den Nutzer verständlich?

---

### 4. DATENBANK & DATENSCHICHT

**LLM-typische Datenbankfehler:**
- N+1 Query Problem (Queries in Schleifen)
- Fehlende Datenbankindizes auf Spalten, die in WHERE/JOIN verwendet werden
- SELECT * statt explizite Spaltenauswahl
- Transaktionen fehlen bei Operationen, die atomar sein müssen
- Keine Datenbankmigrationen oder inkonsistente Migration-History
- ORM-Missbrauch: Raw Queries trotz ORM oder umgekehrt inkonsistent
- Cascade-Delete-Risiken nicht berücksichtigt
- Keine Connection-Pool-Konfiguration
- Datenmodell-Antipatterns (z. B. JSON-Blobs statt normalisierter Tabellen)

**Prüfe:**
- [ ] N+1 Probleme in allen Datenbankzugriffen
- [ ] Transaktionsgrenzen bei kritischen Operationen
- [ ] Indizes auf allen häufig abgefragten Feldern
- [ ] Konsistenz der Migrations-Strategie

---

### 5. API-DESIGN & INTEGRATION

**LLM-Schwächen bei APIs:**
- Inkonsistente REST-Semantik (POST für alles, falsche HTTP-Statusodes)
- Fehlende API-Versionierung
- Kein Rate Limiting / Throttling
- Fehlende Pagination bei Listen-Endpunkten
- Überexponierte Daten (ganze ORM-Objekte serialisiert inkl. Passwort-Hash)
- Fehlende Input-Validierung auf API-Ebene
- Kein API-Schema / keine OpenAPI-Spezifikation
- Inkonsistente Error-Response-Formate
- Externe API-Fehler nicht abgefangen (Timeout, 5xx führt zu unkontrollierten Abstürzen)

**Prüfe:**
- [ ] HTTP-Methoden und Statuscodes korrekt?
- [ ] Werden Response-Objekte auf sensible Felder geprüft?
- [ ] Gibt es Pagination?
- [ ] Gibt es Rate Limiting?

---

### 6. CODE-QUALITÄT & MAINTAINABILITY

**Vibe Coding Zeichen:**
- Funktionen > 50 Zeilen ohne klare Aufgabe
- Verschachtelungstiefe > 4 Ebenen (Callback Hell, Pyramid of Doom)
- Magic Numbers und Magic Strings ohne Konstanten
- Fehlende oder unvollständige Kommentare bei komplexer Logik
- Offensichtlich falsche oder irreführende Kommentare (KI-Kommentare, die den Code nicht korrekt beschreiben)
- Inkonsistente Code-Formatierung (deutet auf verschiedene LLM-Sessions hin)
- Ungenutzte Imports, Variablen, Parameter
- Boolean-Trap-Antipatterns
- Primitive Obsession (IDs als strings, Money als float)
- Fehlende Typen/Interfaces bei typisierten Sprachen (TypeScript: `any` überall)

**Prüfe:**
- [ ] Gibt es Funktionen/Methoden die zu groß/komplex sind?
- [ ] Gibt es Magic Numbers/Strings?
- [ ] Sind alle Imports/Variablen in Verwendung?
- [ ] Ist TypeScript (falls verwendet) tatsächlich typisiert oder `any`-Spam?

---

### 7. TESTING & TESTBARKEIT

**Typische LLM-Testprobleme:**
- Keine Tests vorhanden (Vibe Coding Merkmal Nr. 1)
- Tests die nur happy-path testen
- Tests die tatsächlich nichts testen (Assertions die immer wahr sind)
- Fehlende Edge Case Tests (null, undefined, leere Arrays, Grenzwerte)
- Tests die andere Tests beeinflussen (keine Isolation, shared state)
- Fehlende Mocks für externe Abhängigkeiten
- Tests die gegen Produktionsdatenbank laufen
- Testcode der schlechter ist als Produktionscode
- CI/CD fehlt oder ist nur rudimentär
- Code ist nicht testbar (starke Kopplung, keine Dependency Injection)

**Prüfe:**
- [ ] Testabdeckung (kritische Pfade abgedeckt?)
- [ ] Qualität der existierenden Tests
- [ ] Testbarkeit des Produktionscodes
- [ ] CI/CD-Konfiguration vorhanden und sinnvoll?

---

### 8. PERFORMANCE & SKALIERBARKEIT

**LLM-Performance-Antipatterns:**
- Synchrone Operationen wo async erforderlich wäre
- Unnötige Datenbankaufrufe (Daten die bereits verfügbar sind werden erneut abgefragt)
- Fehlende Caching-Strategie für teure Operationen
- Speicherlecks durch nicht geschlossene Verbindungen, Event-Listener
- Unbegrenzte Dateigrößen bei Uploads
- Fehlende Komprimierung (gzip/brotli)
- Unnötiges Re-Rendering in Frontend-Frameworks
- Blockierende Operationen im Event Loop (Node.js)
- Keine Lazy Loading bei großen Datensätzen

**Prüfe:**
- [ ] Gibt es offensichtliche Performance-Flaschenhälse?
- [ ] Werden teure Operationen gecacht?
- [ ] Gibt es Speicherleck-Risiken?

---

### 9. DEPENDENCY MANAGEMENT

**LLM-Abhängigkeitsprobleme:**
- Veraltete Abhängigkeiten mit bekannten Sicherheitslücken
- Unnötige Abhängigkeiten (5 Libraries für eine einfache Utility)
- Fehlende Lock-Dateien (`package-lock.json`, `poetry.lock`)
- Dev-Dependencies in Production
- Unpinned Versionen (`"^1.0.0"` vs `"1.2.3"`)
- Abhängigkeiten mit breiten Permissions (npm-Pakete die auf das Dateisystem zugreifen)
- Fehlende Security-Audits (`npm audit`, `pip-audit`)

**Prüfe:**
- [ ] Sind alle Abhängigkeiten aktuell und ohne bekannte CVEs?
- [ ] Gibt es Lock-Dateien?
- [ ] Sind alle Abhängigkeiten wirklich notwendig?

---

### 10. KONFIGURATION & DEPLOYMENT

**Vibe Coding Deployment-Fehler:**
- `.env`-Dateien im Repository
- Produktions-Secrets in der Versionskontrolle
- Fehlende `.gitignore`-Einträge für sensible Dateien
- Debug-Modus in Production aktiv
- Fehlende Health-Check-Endpunkte
- Kein Graceful Shutdown
- Fehlende Ressourcenlimits (Memory, CPU)
- Logging nicht konfiguriert oder zu verbose in Production
- Keine Umgebungstrennung (Dev/Staging/Prod)

**Prüfe:**
- [ ] Sind Secrets korrekt exkludiert?
- [ ] Sind Umgebungen sauber getrennt?
- [ ] Ist Logging produktionsgerecht konfiguriert?

---

### 11. LLM-SPEZIFISCHE ANTI-PATTERNS (META-EBENE)

**Erkenne typische KI-Halluzinationen und Generierungsartefakte:**

- **Phantom-Imports:** Import von Modulen/Paketen die nicht existieren oder nicht installiert sind
- **API-Halluzinationen:** Verwendung von Methoden die in der verwendeten Library-Version nicht existieren
- **Inkonsistente Logik:** Code der lokal korrekt aussieht, aber im Gesamtkontext falsch ist
- **Überkomplexität:** Unnötig komplizierte Lösungen für einfache Probleme (LLMs neigen zu Over-Engineering)
- **Unterkomplexität:** Triviale Implementierungen für eigentlich komplexe Probleme (Edge Cases ignoriert)
- **Session-Brüche:** Erkennbare Stellen wo eine neue LLM-Session begann (Stil-, Pattern- oder Namensbrüche)
- **Kommentar-Code-Divergenz:** Kommentare beschreiben etwas anderes als der Code tut
- **Falsches Confidence-Signal:** Code der „selbstsicher" aussieht aber fundamental falsch ist (falsche Algorithmen, falsche Formeln)
- **Cargo-Cult-Patterns:** Sicherheitsmechanismen die implementiert wurden aber keinen echten Schutz bieten
- **Fehlende Domain-Knowledge:** Business-Logik die die fachliche Realität nicht korrekt abbildet

---

## OUTPUT-FORMAT

Erstelle den Audit-Report in folgendem Format:

---

```markdown
# 🔍 Software Audit Report
**Projekt:** [Projektname]
**Analysiert am:** [Datum]
**Analysiert durch:** LLM Code Audit System
**Schweregrade:** 🔴 Kritisch | 🟠 Hoch | 🟡 Mittel | 🟢 Niedrig | ℹ️ Info

---

## Executive Summary

[2-3 Sätze: Gesamtzustand des Projekts, schlimmste Befunde, dringende Handlungsempfehlungen]

### Befund-Übersicht

| Kategorie | 🔴 Kritisch | 🟠 Hoch | 🟡 Mittel | 🟢 Niedrig |
|-----------|-------------|---------|-----------|------------|
| Sicherheit | X | X | X | X |
| Architektur | X | X | X | X |
| Fehlerbehandlung | X | X | X | X |
| ... | | | | |
| **Gesamt** | **X** | **X** | **X** | **X** |

---

## Detaillierte Befunde

### [KATEGORIE-NAME]

#### 🔴 KRITISCH: [Titel des Befunds]

**Datei(en):** `pfad/zur/datei.ts`, Zeile 42-67  
**LLM-Pattern:** [z.B. "Phantom-Security", "Session-Bruch", "Cargo-Cult-Auth"]  

**Problem:**
[Präzise Beschreibung was falsch ist und warum]

**Betroffener Code:**
\`\`\`language
// Problematischer Code hier
\`\`\`

**Risiko:**
[Konkrete Auswirkung: Was kann passieren? Datenverlust? RCE? Compliance-Verstoß?]

**Empfehlung:**
\`\`\`language
// Korrigierter Code hier
\`\`\`

---

[Weitere Befunde im gleichen Format...]

---

## LLM/Vibe Coding Diagnose

### Erkannte Generierungsmuster

[Liste der identifizierten LLM-Artefakte und Vibe-Coding-Merkmale]

### Session-Bruch-Analyse

[Beschreibung erkennbarer Stellen, wo neue LLM-Sessions begannen oder unterschiedliche Modelle/Prompts verwendet wurden]

### Technische Schulden durch LLM-Generierung

[Quantifizierung: Wie viel Refactoring ist nötig?]

---

## Priorisierter Aktionsplan

### Sofort (innerhalb 24h – Kritische Sicherheitslücken)
1. [ ] ...

### Kurzfristig (innerhalb 1 Woche)
1. [ ] ...

### Mittelfristig (innerhalb 1 Monat)
1. [ ] ...

### Langfristig (Architektur-Refactoring)
1. [ ] ...

---

## Gesamtbewertung

| Dimension | Score (1-10) | Kommentar |
|-----------|-------------|-----------|
| Sicherheit | X/10 | |
| Architektur | X/10 | |
| Code-Qualität | X/10 | |
| Testabdeckung | X/10 | |
| Wartbarkeit | X/10 | |
| **Gesamt** | **X/10** | |

**Produktionsreife:** [Nicht produktionsreif / Bedingt produktionsreif / Produktionsreif mit Auflagen / Produktionsreif]
```

---

## WICHTIGE ANWEISUNGEN

1. **Sei schonungslos ehrlich.** Beschönige nichts. Der Auftraggeber braucht die Wahrheit, nicht Komfort.
2. **Konkret statt vage.** Jeder Befund braucht Dateiname, Code-Snippet und konkretes Risiko.
3. **Priorisiere nach Gefahr.** Sicherheitslücken vor Performance vor Style.
4. **Unterscheide Symptom und Ursache.** Benenne nicht nur WAS falsch ist, sondern WARUM es ein typisches LLM-Muster ist.
5. **Kein False Positive Padding.** Melde nur echte Probleme, keine theoretischen Phantome.
6. **Vollständigkeit vor Schnelligkeit.** Analysiere ALLE Dateien, nicht nur die offensichtlichen.
7. **Technologiespezifisch.** Passe die Analyse an die tatsächlich verwendeten Technologien an – kein generisches Bingo.

---

## KONTEXT-ERGÄNZUNGEN (Optional – vor Verwendung ausfüllen)

```
Projekttyp: [Web-App / API / CLI / Library / Mobile / ...]
Technologie-Stack: [z.B. Next.js, FastAPI, PostgreSQL, Redis]
Zielumgebung: [Cloud / On-Premise / Edge]
Compliance-Anforderungen: [DSGVO / SOC2 / HIPAA / PCI-DSS / keine]
Bekannte Probleme: [Was hat der Auftraggeber selbst schon bemerkt?]
Kritischste Business-Funktionen: [Was darf auf keinen Fall kaputt sein?]
Ursprung des Codes: [Welches LLM / Welcher Workflow wurde verwendet?]
```

---

*Dieser Prompt ist für den Einsatz mit leistungsstarken LLMs (Claude Opus, GPT-4o, Gemini Ultra) optimiert. Für beste Ergebnisse den gesamten Quellcode als Kontext bereitstellen.*
