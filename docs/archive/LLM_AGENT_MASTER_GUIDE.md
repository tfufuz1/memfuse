# LLM-AGENT MASTER GUIDE
## Fortgeschrittener Leitfaden für die Softwareentwicklung mit LLM-Agenten
### Version 1.0 — Gerichtet an: Gemini-CLI, Claude Code und kompatible Coding-Agenten

---

> **META-ANWEISUNG AN DIESEN AGENTEN:**
> Du bist ein Software-Architekt-Agent. Dieser Leitfaden ist deine operative Verfassung.
> Lies ihn vollständig, bevor du eine Codebase analysierst oder modifizierst.
> Befolge die hier definierten Workflows, Validierungs-Schleifen und Kontextprotokolle präzise.
> Abweichungen sind nur mit expliziter Human-Approval erlaubt.

---

## INHALTSVERZEICHNIS

```
TEIL A — DIAGNOSE & SCHWACHSTELLEN-TAXONOMIE
TEIL B — SPEZIFIKATIONS-ARCHITEKTUR (Spec-First)
TEIL C — KONTEXT-ENGINE & SPEICHERARCHITEKTUR
TEIL D — AGENTIC WORKFLOWS & VALIDIERUNGS-SCHLEIFEN
TEIL E — GITHUB-INTEGRATION & VERSION CONTROL
TEIL F — TEST- UND SPEC-DRIVEN DEVELOPMENT (TDD/SDD)
TEIL G — PROMPT-SYNTAX & INSTRUKTIONS-PROTOKOLLE
TEIL H — QUALITÄTSSICHERUNG & SELF-HEALING
TEIL I — OPERATIVES PLAYBOOK (Schnellreferenz)
```

---

# TEIL A — DIAGNOSE & SCHWACHSTELLEN-TAXONOMIE

## A.1 Systematische Fehleranalyse vor jedem Projektstart

**PFLICHT:** Bevor du eine Codebase modifizierst, führe folgende Diagnose-Checkliste aus:

```xml
<diagnosis_protocol>
  <step id="1">Analysiere GEMFILE / package.json / pyproject.toml auf veraltete Deps</step>
  <step id="2">Prüfe auf fehlende oder ambige Spezifikationen (User-Stories ohne Akzeptanzkriterien)</step>
  <step id="3">Identifiziere Kopplung: Suche nach direkten Implementierungs-Imports statt Interface-Imports</step>
  <step id="4">Kartiere Testabdeckung: Lücken > 20% sind BLOCKER</step>
  <step id="5">Prüfe auf fehlende CLAUDE.md / GEMINI.md / AGENTS.md Konfigurationsdateien</step>
  <step id="6">Verifiziere GitHub-Integration: Branch-Strategie, PR-Templates, CI/CD-Pipelines</step>
  <step id="7">Identifiziere nicht-deterministische Abhängigkeiten (externe APIs ohne Mocks)</step>
</diagnosis_protocol>
```

## A.2 Kritische Schwachstellen-Taxonomie (MAST-Framework)

Die folgende Taxonomie klassifiziert die 14 häufigsten Fehlermodi in LLM-gestützter Entwicklung in drei Hauptkategorien:

### Kategorie I — Spezifikationsprobleme

| ID | Schwachstelle | Symptom | Eliminierungsstrategie |
|----|--------------|---------|----------------------|
| S-1 | **Ambige User-Stories** | LLM generiert inkonsistente Implementierungen | Formalisiere in Gherkin/BDD-Format → Spec-First |
| S-2 | **Fehlende Randfälle** | Edge-Cases nicht abgedeckt, Prod-Fehler | Explizite `<edge_cases>`-Sektion in jeder Spec |
| S-3 | **Impliziter Kontext** | LLM halluziniert Domain-Logik | Vollständiger Kontext-Dump in `CONTEXT.md` |
| S-4 | **Vage Verben** | "Verarbeite X" → unklar was "verarbeiten" bedeutet | Ersetze durch Akzeptanzkriterien mit Input/Output-Beispielen |
| S-5 | **Fehlende Fehlerbehandlung** | Happy-Path only, keine Error-States | Erzwinge: Jede Funktion MUSS Error-States definieren |

### Kategorie II — Inter-Agenten-Fehlausrichtung

| ID | Schwachstelle | Symptom | Eliminierungsstrategie |
|----|--------------|---------|----------------------|
| A-1 | **Kontext-Drift** | Agent verliert Zielausrichtung nach >10 Schritten | SDLCState-Objekt + Checkpoint-Validierung |
| A-2 | **Trust-Vulnerability Paradox** | Agenten übernehmen fehlerhafte Zwischenergebnisse | Zero-Trust: Validator-Agent für jede Übergabe |
| A-3 | **Reasoning-Action Dilemma** | Agent simuliert intern, handelt nicht → Paralyse | Erzwinge Aktionen nach maximal 3 Thinking-Schritten |
| A-4 | **Overthinking-Bias** | Zu viele Reflexionsschleifen, kein Output | Max-Iteration-Guards in Workflow-Definition |

### Kategorie III — Aufgabenverifizierung

| ID | Schwachstelle | Symptom | Eliminierungsstrategie |
|----|--------------|---------|----------------------|
| V-1 | **Output-only-Validierung** | Code läuft, aber Logik ist falsch | Prozess-Verifizierung + Reasoning-Audit |
| V-2 | **Nicht-reproduzierbare Fehler** | Bug tritt nur sporadisch auf | AgentRR: Record & Replay aller Trajektorien |
| V-3 | **Fehlende Spec-Conformance** | Code weicht von Vertrag ab | Contract-Testing (Dredd/Pact) als CI-Gate |
| V-4 | **Halluzinierte Implementierungen** | LLM erfindet nicht existente APIs | Erzwinge Verifizierung gegen echte Dokumentation |
| V-5 | **Prompt-Sprödigkeit** | Kleine Prompt-Änderung → komplett andere Ergebnisse | Syntaktisches Engineering + Delimiter-Protokoll |

---

# TEIL B — SPEZIFIKATIONS-ARCHITEKTUR (Spec-First)

## B.1 Das Spec-Architect-Protokoll (SAP)

**GRUNDPRINZIP:** Vage Prosa → Fehler. Formale Spezifikation → Determinismus.

Die Kausalkette des Scheiterns ist:
```
Vage User-Story → Fehlender Kontext → Probabilistische Inferenz → Halluzination → Funktionaler Fehler
```

**Die Lösung:** Jede Implementierungsaufgabe MUSS durch folgende Artefakt-Hierarchie definiert sein:

```
project-root/
├── .agent/
│   ├── CONSTITUTION.md          # Nicht verhandelbare Prinzipien
│   ├── ARCHITECTURE.md          # System-Architektur-Gesamtdokument (KI-AGD)
│   ├── DEPENDENCY_GRAPH.md      # Abhängigkeits-Map (KI-AM)
│   └── specs/
│       ├── interfaces/          # KI-Schnittstellenverträge (KI-SV)
│       │   ├── user-service.proto
│       │   ├── api.v1.yaml      # OpenAPI-Definitionen
│       │   └── events.avro      # Event-Schemata
│       └── modules/             # KI-Mikrospezifikationen (KI-MS)
│           ├── MOD-AUTH-MS-001.md
│           ├── MOD-USER-MS-001.md
│           └── MOD-PAYMENT-MS-001.md
├── CLAUDE.md                    # Agenten-Konfiguration (wird automatisch geladen)
├── GEMINI.md                    # Gemini-CLI Konfiguration
└── tests/
    ├── specs/                   # Gherkin/BDD-Specs (lebende Dokumentation)
    └── contracts/               # Contract-Tests
```

## B.2 CONSTITUTION.md — Die Agenten-Verfassung

```markdown
# AGENTEN-VERFASSUNG — [PROJEKTNAME]

## NICHT VERHANDELBARE PRINZIPIEN

1. **Contract-First:** Kein Code wird geschrieben, bevor der Interface-Vertrag definiert ist.
2. **Test-First:** Kein Feature-Code ohne vorherige Testdefinition.
3. **Single Responsibility:** Jede KI-MS beschreibt EXAKT eine Verantwortung.
4. **Fail-Fast:** Ambigue Spezifikationen werden sofort eskaliert, nicht interpretiert.
5. **Zero-Trust:** Kein Agent vertraut unkritisch den Ausgaben anderer Agenten.
6. **Expliziter Kontext:** Niemals implizites Domain-Wissen annehmen.
7. **Nachvollziehbarkeit:** Jede Entscheidung wird mit Begründung dokumentiert.

## ARCHITEKTUR-ENTSCHEIDUNGEN (ADRs)

- ADR-001: Bevorzuge Interface-Importe über direkte Implementierungs-Importe (DIP)
- ADR-002: Alle API-Endpunkte MÜSSEN im OpenAPI-Format spezifiziert sein
- ADR-003: Alle Events MÜSSEN ein Schema haben (Avro/JSON-Schema)
- ADR-004: Keine Funktion ohne Fehlerbehandlung

## AKZEPTANZKRITERIEN-FORMAT (PFLICHT)

Jedes Feature MUSS folgendes Format haben:
```gherkin
Feature: [Name]
  Als [Rolle]
  Möchte ich [Aktion]
  Damit [Wert]

  Szenario: [Happy Path]
    Gegeben [Vorbedingung]
    Wenn [Aktion]
    Dann [Erwartetes Ergebnis]

  Szenario: [Error Case]
    Gegeben [Fehlerbedingung]
    Wenn [Aktion]
    Dann [Fehler-Ergebnis]
```
```

## B.3 KI-Mikrospezifikation (KI-MS) — Template

Jede Mikrospezifikation MUSS folgendes Format einhalten:

```markdown
# KI-MS: [MOD-BEREICH-TYP-NR]
**Status:** DRAFT | APPROVED | IMPLEMENTED | DEPRECATED
**Erstellt:** [Datum]
**Abhängigkeiten:** [Liste von KI-SV IDs die dieser Modul konsumiert]

## 1. SINGLE RESPONSIBILITY STATEMENT
Diese Mikrospezifikation beschreibt EXAKT: [Eine Sache]

## 2. SCHNITTSTELLENVERTRAG (KI-SV Referenz)
Der Agent MUSS ausschließlich gegen diese Interfaces kodieren:
- Input-Schema: [Referenz zu .proto oder .yaml]
- Output-Schema: [Referenz zu .proto oder .yaml]
- Events emittiert: [Liste]
- Events konsumiert: [Liste]

## 3. VERHALTEN — FORMALE DEFINITION

### Happy Path:
```
Input: { field: type, constraints }
Processing: [Schrittweise Beschreibung]
Output: { field: type }
```

### Error States (PFLICHT):
```
Error E-001: [Bedingung] → [Fehler-Response]
Error E-002: [Bedingung] → [Fehler-Response]
```

### Randfälle (PFLICHT):
```
Edge-001: [Null/Empty Input] → [Verhalten]
Edge-002: [Max-Size Input] → [Verhalten]
Edge-003: [Concurrent Access] → [Verhalten]
```

## 4. SICHERHEITS- UND LIVENESS-EIGENSCHAFTEN (LTL)

- SAFETY: "Der Agent darf NIEMALS [verbotene Aktion] ausführen"
- LIVENESS: "Wenn [Trigger], MUSS [Zustand] irgendwann erreicht werden"

## 5. AKZEPTANZKRITERIEN (Gherkin)
[Vollständige BDD-Specs hier]

## 6. IMPLEMENTIERUNGS-HINWEISE
[Technische Hinweise ohne Implementierung zu erzwingen]
```

## B.4 Spec-Driven Development (SDD) Workflow

**STRIKT SERIELLER PROZESS — Keine Überspringe erlaubt:**

```
┌─────────────────────────────────────────────────────────────┐
│  PHASE 1: SPECIFY                                           │
│  Agent liest Issue → Erstellt KI-MS Draft                   │
│  Human Checkpoint: Architekt prüft und genehmigt KI-MS      │
└────────────────────────┬────────────────────────────────────┘
                         │ APPROVED
                         ▼
┌─────────────────────────────────────────────────────────────┐
│  PHASE 2: INTERFACE                                         │
│  Agent erstellt/aktualisiert KI-SV (OpenAPI/Proto)          │
│  Human Checkpoint: Contract-Review + Semantic-Check         │
└────────────────────────┬────────────────────────────────────┘
                         │ APPROVED
                         ▼
┌─────────────────────────────────────────────────────────────┐
│  PHASE 3: TEST-FIRST                                        │
│  Agent schreibt Tests GEGEN DEN VERTRAG (nicht gegen Code)  │
│  Human Checkpoint: Tests beschreiben gewünschtes Verhalten? │
└────────────────────────┬────────────────────────────────────┘
                         │ APPROVED
                         ▼
┌─────────────────────────────────────────────────────────────┐
│  PHASE 4: IMPLEMENT                                         │
│  Agent implementiert: Tests müssen grün werden              │
│  Automatisch: CI läuft nach jedem Commit                    │
└────────────────────────┬────────────────────────────────────┘
                         │ ALL GREEN
                         ▼
┌─────────────────────────────────────────────────────────────┐
│  PHASE 5: VALIDATE & PR                                     │
│  Contract-Tests + Integration-Tests + PR erstellen          │
└─────────────────────────────────────────────────────────────┘
```

---

# TEIL C — KONTEXT-ENGINE & SPEICHERARCHITEKTUR

## C.1 Fundamentales Kontext-Prinzip

**LLMs nutzen effektiv nur 10–20% langer Kontexte.** Das "Lost-in-the-Middle"-Problem ist real. Kontext-Management ist deshalb keine optionale Optimierung, sondern architektonisch kritisch.

**Goldene Regeln:**
1. **Kritische Instruktionen** → Immer am ANFANG des Kontexts ("Attention Hotspot")
2. **Primäre Aufgabe** → Am ENDE des Kontexts (hohe Recall-Rate)
3. **Unterstützender Kontext** → In der Mitte, komprimiert und gefiltert
4. **Niemals** den gesamten Codebase als Kontext einspeisen

## C.2 Das SDLCState-Objekt — Kanonische Zustandsdefinition

Jeder Agent-Workflow MUSS einen expliziten, typisierten Zustand verwalten:

```python
# sdlc_state.py — Kanonische Zustandsdefinition für alle Workflows
from typing import List, Dict, Any, Optional, Annotated
from dataclasses import dataclass, field
from operator import add

@dataclass
class SDLCState:
    """
    Kanonischer Zustand für LLM-gestützte Softwareentwicklungs-Workflows.
    ALLE Agenten im Workflow lesen/schreiben NUR über dieses Objekt.
    """
    
    # ── AUFGABEN-KONTEXT ──────────────────────────────────────────
    task_id: str = ""                    # Issue-ID oder Task-Identifier
    task_type: str = ""                  # "feature" | "bugfix" | "refactor" | "test"
    task_description: str = ""          # Vollständige Aufgabenbeschreibung
    acceptance_criteria: List[str] = field(default_factory=list)
    
    # ── SPEZIFIKATIONS-REFERENZEN ─────────────────────────────────
    spec_id: str = ""                    # Referenz zur KI-MS
    interface_contracts: List[str] = field(default_factory=list)  # Pfade zu KI-SVs
    constitution_hash: str = ""          # Hash der CONSTITUTION.md (Drift-Detection)
    
    # ── REPOSITORY-ZUSTAND ───────────────────────────────────────
    repo_url: str = ""
    base_branch: str = "main"
    working_branch: str = ""
    uncommitted_changes: bool = False
    staged_files: List[str] = field(default_factory=list)
    file_diffs: List[Dict[str, str]] = field(default_factory=list)
    
    # ── TEST-ZUSTAND ──────────────────────────────────────────────
    test_results: Dict[str, Any] = field(default_factory=dict)
    # { "passed": bool, "failed_tests": [], "coverage": float, "output": str }
    contract_test_results: Dict[str, Any] = field(default_factory=dict)
    
    # ── TOOL-AUSGABEN (letzter Aufruf) ───────────────────────────
    last_tool_output: Dict[str, Any] = field(default_factory=dict)
    # { "tool": str, "stdout": str, "stderr": str, "exit_code": int }
    
    # ── ITERATIONS-TRACKING ──────────────────────────────────────
    iteration_count: int = 0
    max_iterations: int = 10            # GUARD: Verhindert Endlosschleifen
    error_history: List[str] = field(default_factory=list)
    
    # ── AGENTEN-KOMMUNIKATION (append-only) ──────────────────────
    messages: List[Dict] = field(default_factory=list)
    scratchpad: str = ""                # Temporärer Denkbereich des Agenten
    
    # ── VALIDIERUNGS-STATUS ──────────────────────────────────────
    spec_approved: bool = False
    interface_approved: bool = False
    tests_written: bool = False
    implementation_complete: bool = False
    pr_created: bool = False
    
    def increment_iteration(self) -> bool:
        """Returns False wenn Max-Iterations erreicht → Workflow abbricht."""
        self.iteration_count += 1
        return self.iteration_count < self.max_iterations
    
    def log_error(self, error: str):
        """Append-only Error-Log für Debugging."""
        self.error_history.append(f"[Iter {self.iteration_count}] {error}")
```

## C.3 Just-in-Time Retrieval (JIT-RAG) für Codebasen

**PROBLEM:** Eine Codebase passt nicht in ein Kontextfenster.
**LÖSUNG:** Chirurgisches, bedarfsgerechtes Abrufen von exakt dem Code, der gerade benötigt wird.

### Drei-Stufen-Retrieval-Protokoll:

```python
# context_engine.py — JIT Context Retrieval

class JITContextEngine:
    """
    Implementiert einen hybriden Retrieval-Ansatz für Codebasen.
    Stufe 1: Keyword/Vector-Suche (Breit)
    Stufe 2: AST-Parsing (Präzise)
    Stufe 3: Episodisches Gedächtnis (Historisch)
    """
    
    def get_context_for_task(self, task: str, state: SDLCState) -> str:
        """
        Hauptmethode: Gibt optimierten Kontext für eine Aufgabe zurück.
        Maximale Kontextgröße: 40% des verfügbaren Kontextfensters.
        """
        
        # ── STUFE 1: BREITE KANDIDATENAUSWAHL ─────────────────────
        candidate_files = self._broad_search(task)
        
        # ── STUFE 2: PRÄZISE EXTRAKTION VIA AST ──────────────────
        precise_code = self._ast_extract(candidate_files, task)
        
        # ── STUFE 3: EPISODISCHES GEDÄCHTNIS ─────────────────────
        past_solutions = self._query_episodic_memory(task, state.error_history)
        
        # ── KONTEXTASSEMBLIERUNG (Positions-optimiert) ────────────
        return self._assemble_context(precise_code, past_solutions, state)
    
    def _broad_search(self, query: str) -> List[str]:
        """
        Phase 1: Semantische + Keyword-Suche.
        Gibt Top-10 Kandidaten-Dateien zurück.
        """
        # Semantic Vector Search
        vector_results = self.vector_db.similarity_search(query, k=20)
        
        # Keyword Grep für exakte Matches
        grep_results = self._grep_codebase(query)
        
        # Dedupliziere und ranke
        return self._rank_and_deduplicate(vector_results + grep_results)[:10]
    
    def _ast_extract(self, files: List[str], query: str) -> List[str]:
        """
        Phase 2: Chirurgische Extraktion via AST-Parsing.
        Extrahiert EXAKT die relevanten Funktionen/Klassen.
        """
        import ast
        relevant_nodes = []
        
        for file_path in files:
            with open(file_path) as f:
                tree = ast.parse(f.read())
            
            for node in ast.walk(tree):
                if isinstance(node, (ast.FunctionDef, ast.ClassDef)):
                    # Extrahiere nur was wirklich relevant ist
                    if self._is_relevant(node, query):
                        relevant_nodes.append(ast.unparse(node))
        
        return relevant_nodes
    
    def _query_episodic_memory(self, query: str, errors: List[str]) -> List[str]:
        """
        Phase 3: Selbstreflexiver Abruf aus MCP-Verlauf.
        'Was hat in ähnlichen Situationen funktioniert/nicht funktioniert?'
        """
        past_successes = self.memory_store.search(
            f"successful solution for: {query}"
        )
        past_failures = self.memory_store.search(
            f"failed attempt for: {' '.join(errors[-3:])}"
        )
        return past_successes + [f"AVOID: {f}" for f in past_failures]
    
    def _assemble_context(
        self, 
        code: List[str], 
        memory: List[str],
        state: SDLCState
    ) -> str:
        """
        Assembliert Kontext in positions-optimierter Reihenfolge.
        KRITISCH: Wichtigstes zuerst und zuletzt (Attention Hotspots).
        """
        return f"""
<constitution>
{self._load_constitution()}
</constitution>

<task_context>
Task ID: {state.task_id}
Type: {state.task_type}
Description: {state.task_description}
Acceptance Criteria:
{chr(10).join(f'- {c}' for c in state.acceptance_criteria)}
</task_context>

<interface_contracts>
{self._load_contracts(state.interface_contracts)}
</interface_contracts>

<relevant_code>
{chr(10).join(f'```python\n{c}\n```' for c in code[:5])}
</relevant_code>

<episodic_memory>
{chr(10).join(memory[:3])}
</episodic_memory>

<current_task>
{state.task_description}
</current_task>
"""
```

## C.4 Episodisches Gedächtnis — MCP-Verlaufs-Protokoll

**JEDER** Tool-Call MUSS geloggt werden. Dieser Log ist das Lerngedächtnis des Agenten.

```python
# memory_store.py — Strukturiertes episodisches Gedächtnis

import json
from datetime import datetime
from pathlib import Path

class EpisodicMemoryStore:
    """
    Persistentes episodisches Gedächtnis für den Agenten.
    Speichert ALLE Tool-Interaktionen für selbstreflexiven Abruf.
    """
    
    LOG_SCHEMA = {
        "timestamp": str,
        "task_id": str,
        "iteration": int,
        "tool_name": str,
        "tool_input": dict,
        "stdout": str,
        "stderr": str,
        "exit_code": int,
        "success": bool,
        "lesson": str    # Was wurde gelernt?
    }
    
    def log_tool_call(
        self,
        task_id: str,
        iteration: int,
        tool_name: str,
        tool_input: dict,
        stdout: str,
        stderr: str,
        exit_code: int
    ):
        """Loggt einen Tool-Call mit Lernannotation."""
        
        success = exit_code == 0
        lesson = self._extract_lesson(tool_name, stdout, stderr, success)
        
        entry = {
            "timestamp": datetime.utcnow().isoformat(),
            "task_id": task_id,
            "iteration": iteration,
            "tool_name": tool_name,
            "tool_input": tool_input,
            "stdout": stdout[:2000],  # Truncate für Effizienz
            "stderr": stderr[:500],
            "exit_code": exit_code,
            "success": success,
            "lesson": lesson
        }
        
        # Persistiere in searchable store (SQLite / Vector-DB)
        self._persist(entry)
        
        # Vektorisiere für semantische Suche
        self._embed_and_index(entry)
    
    def search(self, query: str, k: int = 3) -> List[str]:
        """Sucht nach ähnlichen vergangenen Erfahrungen."""
        results = self.vector_index.similarity_search(query, k=k)
        return [
            f"[{r['timestamp'][:10]}] {r['tool_name']}: "
            f"{'✓' if r['success'] else '✗'} | {r['lesson']}"
            for r in results
        ]
```

## C.5 Kontext-Kompressions-Strategie

Bei langen Kontexten: Komprimiere, bevor du einspeist.

```xml
<compression_rules>
  <rule id="1">
    Code-Dateien: Extrahiere nur Signaturen + Docstrings, nicht Bodies
    (außer der direkt relevante Body wird gebraucht)
  </rule>
  <rule id="2">
    Error-Logs: Nur die letzten 5 Zeilen + erste Zeile (Stack-Trace-Kern)
  </rule>
  <rule id="3">
    Git-History: Nur Commits der letzten 7 Tage + Tag-Commits
  </rule>
  <rule id="4">
    Test-Output: Nur FAILED-Tests mit vollständigem Traceback
  </rule>
  <rule id="5">
    Dokumentation: Zusammenfassung via LLM, nicht raw Text
  </rule>
</compression_rules>
```

**Ziel:** Reduktion der Token-Nutzung um 60% bei weniger als 5% Genauigkeitsverlust.

---

# TEIL D — AGENTIC WORKFLOWS & VALIDIERUNGS-SCHLEIFEN

## D.1 Der Master-Workflow (TDFlow)

Dies ist der zentrale, iterative Entwicklungs-Loop:

```
┌─────────────────────────────────────────────────────────────────┐
│                    TDFLOW MASTER LOOP                           │
│                                                                 │
│  ┌──────────┐    ┌──────────┐    ┌──────────┐    ┌──────────┐  │
│  │ SPEC     │───▶│ TEST     │───▶│ IMPL     │───▶│ VALIDATE │  │
│  │ AGENT    │    │ WRITER   │    │ AGENT    │    │ AGENT    │  │
│  └──────────┘    └──────────┘    └──────────┘    └────┬─────┘  │
│       ▲                                               │         │
│       │                                               │         │
│       │              ┌────────────────────────────────┘         │
│       │              │ Tests FAILED? → Iteration +1              │
│       │              │ Tests PASSED? → PR-Agent                 │
│       │              ▼                                          │
│       │         ┌──────────┐                                    │
│       └─────────│ CRITIC   │◀── Fehlschlag nach Max-Iter        │
│   Spec unklar   │ AGENT    │    → Human Escalation              │
│                 └──────────┘                                    │
└─────────────────────────────────────────────────────────────────┘
```

## D.2 Fünfstufiges Validierungs-System

Validierung ist keine nachgelagerte Phase — sie ist ins Workflow-Design integriert.

### Stufe 1 — Output-Validierung (Atomare Korrektheit)

```python
class OutputValidator:
    """Überprüft finale Artefakte gegen definierten Vertrag."""
    
    def validate(self, state: SDLCState) -> ValidationResult:
        checks = [
            self._run_unit_tests(state),
            self._run_contract_tests(state),
            self._check_type_annotations(state),
            self._lint_code(state)
        ]
        return ValidationResult(all_passed=all(c.passed for c in checks))
```

### Stufe 2 — Reasoning-Validierung (Kognitive Integrität)

```python
class ReasoningValidator:
    """
    Executor-Verifier-Paradigma:
    Mehrere Executor-Agenten generieren Lösungen.
    Ein Verifier-Agent prüft die Argumentation — nicht nur das Ergebnis.
    """
    
    VERIFIER_PROMPT = """
<role>Du bist ein kritischer Code-Reviewer mit Fokus auf Reasoning-Qualität.</role>

<task>
Prüfe nicht nur ob der Code korrekt ist, sondern OB DER DENKPROZESS korrekt war.
Überprüfe:
1. Wurde die richtige Abstraktion gewählt?
2. Sind Randfälle korrekt behandelt?
3. Folgt die Implementierung dem Schnittstellenvertrag?
4. Gibt es "richtige Antwort aus falschem Grund"-Muster?
</task>

<code_to_review>
{code}
</code_to_review>

<spec>
{spec}
</spec>

Antworte in: <verdict>APPROVED|REJECTED</verdict><reasoning>...</reasoning>
"""
```

### Stufe 3 — Workflow-Validierung (Zustandsbehaftete Integrität)

```python
class WorkflowIntegrityMonitor:
    """
    Guardian-Agent: Überwacht den globalen Workflow-Zustand.
    Erkennt: Endlosschleifen, ungültige Zustandsübergänge, Drift.
    """
    
    INVARIANTS = [
        # (condition, error_message)
        (lambda s: s.spec_approved or s.iteration_count == 0,
         "Implementation ohne Spec-Approval verboten"),
        (lambda s: not (s.tests_written and not s.spec_approved),
         "Tests können nicht vor Spec-Approval geschrieben werden"),
        (lambda s: s.iteration_count <= s.max_iterations,
         "Max-Iterationen überschritten → Human Escalation"),
        (lambda s: s.constitution_hash == s.current_constitution_hash(),
         "Constitution wurde modifiziert → Workflow stoppen"),
    ]
    
    def check_invariants(self, state: SDLCState) -> Optional[str]:
        for condition, error_msg in self.INVARIANTS:
            if not condition(state):
                return error_msg
        return None  # Alle Invarianten erfüllt
```

### Stufe 4 — Ökosystem-Validierung (Emergentes Verhalten)

```python
class AdversarialValidator:
    """
    Red-Team-Agent: Testet aktiv auf Schwachstellen im generierten Code.
    """
    
    RED_TEAM_PROMPTS = [
        "Finde SQL-Injection-Anfälligkeiten in diesem Code",
        "Identifiziere Race Conditions bei parallelen Zugriffen",
        "Überprüfe auf fehlende Input-Validierung",
        "Suche nach Memory-Leaks oder Resource-Leaks",
        "Teste auf Path-Traversal-Vulnerabilities",
    ]
    
    def adversarial_review(self, code: str) -> List[SecurityFinding]:
        findings = []
        for attack_prompt in self.RED_TEAM_PROMPTS:
            result = self.llm.generate(f"{attack_prompt}\n\nCode:\n```\n{code}\n```")
            if "VULNERABILITY FOUND" in result:
                findings.append(SecurityFinding.parse(result))
        return findings
```

### Stufe 5 — Deterministische Verifizierung (Beweisbare Korrektheit)

```python
class AgentRR:
    """
    Agent Record & Replay: Macht Fehler reproduzierbar.
    Zeichnet ALLE Interaktionen auf → Deterministische Wiedergabe.
    """
    
    def record_session(self, session_id: str):
        """Startet Recording einer Agenten-Session."""
        self.recordings[session_id] = {
            "inputs": [],
            "llm_outputs": [],
            "tool_calls": [],
            "state_snapshots": [],
            "environment": self._capture_environment()
        }
    
    def replay_session(self, session_id: str) -> SDLCState:
        """Replay einer Session für deterministische Bug-Reproduktion."""
        recording = self.load_recording(session_id)
        # Setzt deterministischen Seed
        with DeterministicEnvironment(recording["environment"]):
            return self._replay_step_by_step(recording)
```

## D.3 Self-Healing Architektur — Dreischichtige Verteidigung

```
SCHICHT 1 — Echtzeit-Konformitätsprüfung (Constitutional AI)
    → Jede Aktion wird vor Ausführung gegen CONSTITUTION.md geprüft
    → Non-konforme Aktionen werden BLOCKIERT

SCHICHT 2 — Iterative Selbstreflexion (Reflexion-Loop)
    → Nach jedem fehlgeschlagenen Test: Explizite Fehleranalyse
    → Writer-Critic-Pattern: Jeder Output wird vom Critic geprüft

SCHICHT 3 — Hierarchische Aufsicht (Orchestrator-Validator)
    → Manager-Agent überwacht Worker-Agenten
    → Validator-Agent prüft Übergaben zwischen Agenten
```

### Constitutional AI Check — Implementierung:

```python
def check_constitutional_compliance(action: AgentAction, constitution: str) -> bool:
    """
    Prüft jede geplante Aktion gegen die Verfassung.
    Wird VOR der Ausführung aufgerufen.
    """
    
    compliance_prompt = f"""
<constitution>
{constitution}
</constitution>

<proposed_action>
Tool: {action.tool}
Input: {json.dumps(action.input)}
Rationale: {action.rationale}
</proposed_action>

<task>
Prüfe ob diese Aktion mit der Verfassung konform ist.
Antworte AUSSCHLIESSLICH mit:
<verdict>COMPLIANT|NON_COMPLIANT</verdict>
<reason>Kurze Begründung</reason>
</task>
"""
    
    result = compliance_checker_llm.generate(compliance_prompt)
    verdict = extract_tag(result, "verdict")
    
    if verdict == "NON_COMPLIANT":
        reason = extract_tag(result, "reason")
        raise ConstitutionalViolation(reason)
    
    return True
```

---

# TEIL E — GITHUB-INTEGRATION & VERSION CONTROL

## E.1 Git-Workflow-Protokoll

**BRANCH-STRATEGIE (obligatorisch):**

```bash
# Branch-Naming-Convention (MUSS eingehalten werden):
# feature/TASK-ID-kurze-beschreibung
# bugfix/ISSUE-ID-kurze-beschreibung  
# refactor/BEREICH-kurze-beschreibung
# spec/MOD-ID-interface-oder-spec

# Beispiele:
git checkout -b feature/TASK-123-user-authentication
git checkout -b bugfix/ISSUE-456-null-pointer-in-payment
git checkout -b spec/MOD-AUTH-SV-001-openapi-contract
```

**COMMIT-MESSAGE-FORMAT (Conventional Commits — obligatorisch):**

```
<type>(<scope>): <kurze Beschreibung>

[optionaler body — erklärt WAS und WARUM]

[optionaler footer]
refs: TASK-123
spec: MOD-AUTH-MS-001
```

Erlaubte Types: `feat`, `fix`, `test`, `spec`, `refactor`, `docs`, `ci`, `chore`

## E.2 Proaktiver GitHub-Agent — Operationsprotokoll

```python
class GitHubAgent:
    """
    Proaktiver Agent für GitHub-Interaktionen.
    Führt Git-Operationen mit vollem SDLCState-Bewusstsein durch.
    """
    
    def start_task_from_issue(self, issue_id: int) -> SDLCState:
        """
        Startet einen neuen Task aus einem GitHub Issue.
        Initialisiert SDLCState vollständig.
        """
        
        # 1. Issue-Kontext vollständig laden
        issue = self.github.get_issue(issue_id)
        comments = issue.get_comments()
        linked_prs = self._get_linked_prs(issue_id)
        
        # 2. SDLCState initialisieren
        state = SDLCState(
            task_id=f"ISSUE-{issue_id}",
            task_type=self._classify_issue(issue.labels),
            task_description=f"{issue.title}\n\n{issue.body}",
            acceptance_criteria=self._extract_acceptance_criteria(issue.body)
        )
        
        # 3. Branch erstellen
        branch_name = f"{state.task_type}/{issue_id}-{self._slugify(issue.title)}"
        self.github.create_branch(branch_name, from_branch="main")
        state.working_branch = branch_name
        
        # 4. Kontext aus verwandten Issues/PRs laden
        state.messages.append({
            "role": "context",
            "content": self._format_issue_context(issue, comments, linked_prs)
        })
        
        return state
    
    def commit_changes(self, state: SDLCState, message: str):
        """Committet Änderungen mit vollständigem Kontext."""
        
        # Verfassungs-Check vor Commit
        check_constitutional_compliance(
            AgentAction(tool="git_commit", input={"message": message}),
            self.constitution
        )
        
        # Nur wenn Tests grün
        if not state.test_results.get("passed"):
            raise ValidationError("Kein Commit ohne grüne Tests!")
        
        self.git.add(state.staged_files)
        self.git.commit(message=f"{message}\n\nrefs: {state.task_id}\nspec: {state.spec_id}")
        
        # Episodisches Gedächtnis aktualisieren
        self.memory.log_tool_call(
            task_id=state.task_id,
            tool_name="git_commit",
            success=True,
            lesson=f"Successfully committed: {message}"
        )
    
    def create_pull_request(self, state: SDLCState) -> str:
        """
        Erstellt PR mit vollständiger Dokumentation.
        ERST wenn: Tests ✓ + Contract-Tests ✓ + Spec-Approval ✓
        """
        
        if not all([state.spec_approved, state.tests_written, 
                    state.test_results.get("passed"), state.implementation_complete]):
            raise ValidationError("PR-Voraussetzungen nicht erfüllt!")
        
        pr_body = self._generate_pr_body(state)
        
        pr = self.github.create_pull_request(
            title=f"{state.task_type}: {state.task_description[:60]}",
            body=pr_body,
            head=state.working_branch,
            base=state.base_branch,
            labels=["ai-generated", state.task_type]
        )
        
        state.pr_created = True
        state.messages.append({"role": "system", "content": f"PR created: {pr.url}"})
        
        return pr.url
    
    def _generate_pr_body(self, state: SDLCState) -> str:
        """Generiert vollständige PR-Beschreibung aus State."""
        return f"""
## Beschreibung
{state.task_description}

## Spezifikations-Referenz
- Spec: `{state.spec_id}`
- Interface-Verträge: {', '.join(f'`{c}`' for c in state.interface_contracts)}

## Akzeptanzkriterien
{chr(10).join(f'- [x] {c}' for c in state.acceptance_criteria)}

## Test-Ergebnisse
- Unit-Tests: {'✅ PASSED' if state.test_results.get("passed") else '❌ FAILED'}
- Coverage: {state.test_results.get("coverage", "N/A")}%
- Contract-Tests: {'✅ PASSED' if state.contract_test_results.get("passed") else '❌ FAILED'}

## Iterationen
Anzahl Entwicklungs-Iterationen: {state.iteration_count}

## Closes
Closes #{state.task_id.replace('ISSUE-', '')}
"""
```

## E.3 CI/CD-Integration — Pflicht-Pipeline

```yaml
# .github/workflows/agent-ci.yml
name: Agent Development CI

on:
  push:
    branches: ['feature/**', 'bugfix/**', 'refactor/**']
  pull_request:
    branches: ['main', 'develop']

jobs:
  spec-validation:
    name: Spec & Contract Validation
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      
      - name: Validate OpenAPI Contracts
        run: |
          # Prüfe alle .yaml Kontrakte auf Syntaxfehler
          find .agent/specs/interfaces -name "*.yaml" | \
            xargs -I{} swagger-codegen validate -i {}
      
      - name: Contract Testing (Dredd)
        run: dredd .agent/specs/interfaces/api.v1.yaml http://localhost:8080
      
      - name: Spec Linting
        run: |
          # Prüfe KI-MS auf vollständige Felder
          python scripts/lint_specs.py .agent/specs/modules/

  test-suite:
    name: Test Suite
    runs-on: ubuntu-latest
    needs: spec-validation
    steps:
      - name: Unit Tests
        run: pytest tests/unit/ -v --cov=src --cov-report=xml
      
      - name: Integration Tests
        run: pytest tests/integration/ -v
      
      - name: Coverage Gate (Minimum 80%)
        run: coverage report --fail-under=80

  constitutional-check:
    name: Constitutional Compliance Check
    runs-on: ubuntu-latest
    steps:
      - name: Check Constitution Hash
        run: python scripts/verify_constitution.py
      
      - name: ADR Compliance
        run: python scripts/check_adr_compliance.py
```

---

# TEIL F — TEST- UND SPEC-DRIVEN DEVELOPMENT

## F.1 Test-First-Protokoll (TDFlow)

**GOLDENE REGEL: Tests werden GEGEN DEN VERTRAG geschrieben, nicht gegen den Code.**

```
FALSCH: Code schreiben → Tests schreiben → Tests anpassen bis grün
RICHTIG: Vertrag definieren → Tests schreiben (alle rot) → Code schreiben bis grün
```

### Test-Hierarchie (Pflicht):

```
tests/
├── specs/              # Gherkin BDD-Specs (lebende Dokumentation)
│   └── auth.feature
├── unit/               # Einzel-Funktions-Tests (mocked dependencies)
│   └── test_auth.py
├── integration/        # Komponenten-Interaktion (real dependencies)
│   └── test_auth_integration.py
├── contracts/          # Vertragstreue-Tests
│   └── test_user_service_contract.py
└── e2e/                # End-to-End (vollständiger Flow)
    └── test_login_flow.py
```

### Gherkin-Template für Agenten:

```gherkin
# tests/specs/user_authentication.feature

Feature: User Authentication
  Als registrierter Benutzer
  Möchte ich mich mit Email und Passwort anmelden
  Damit ich auf geschützte Ressourcen zugreifen kann

  Hintergrund:
    Gegeben es existiert ein Benutzer mit Email "test@example.com"
    Und das Passwort ist "SecurePass123!"

  @happy_path
  Szenario: Erfolgreiche Anmeldung
    Wenn der Benutzer POST /auth/login mit {"email": "test@example.com", "password": "SecurePass123!"} sendet
    Dann ist der Status-Code 200
    Und die Antwort enthält ein "access_token" Feld
    Und das Token ist ein gültiges JWT

  @error_case
  Szenario: Falsches Passwort
    Wenn der Benutzer POST /auth/login mit {"email": "test@example.com", "password": "wrong"} sendet
    Dann ist der Status-Code 401
    Und die Antwort enthält {"error": "INVALID_CREDENTIALS"}

  @edge_case
  Szenario: Leere Email
    Wenn der Benutzer POST /auth/login mit {"email": "", "password": "SecurePass123!"} sendet
    Dann ist der Status-Code 400
    Und die Antwort enthält {"error": "VALIDATION_ERROR", "field": "email"}

  @security
  Szenario: SQL-Injection-Versuch
    Wenn der Benutzer POST /auth/login mit {"email": "'; DROP TABLE users; --", "password": "x"} sendet
    Dann ist der Status-Code 400
    Und keine Datenbankoperation wird ausgeführt
```

## F.2 Contract-Test-Template:

```python
# tests/contracts/test_user_service_contract.py
"""
Contract-Tests: Prüfen ob Implementierung dem Schnittstellenvertrag entspricht.
Referenz-Vertrag: .agent/specs/interfaces/user-service.yaml
"""

import pytest
from pactman import Consumer, Provider

class TestUserServiceContract:
    """
    Pact-basierte Contract-Tests für den User-Service.
    Diese Tests sind die einzige autoritative Quelle der Wahrheit.
    """
    
    @pytest.fixture
    def pact(self):
        return Consumer("AuthService").has_pact_with(Provider("UserService"))
    
    def test_get_user_by_email(self, pact):
        """Vertrag: GET /users?email={email} gibt User-Objekt zurück."""
        
        (pact
            .given("a user with email test@example.com exists")
            .upon_receiving("a request for user by email")
            .with_request(
                method="GET",
                path="/users",
                query={"email": "test@example.com"}
            )
            .will_respond_with(
                status=200,
                body={
                    "id": pact.like("user-uuid-123"),
                    "email": "test@example.com",
                    "created_at": pact.like("2025-01-01T00:00:00Z")
                }
            )
        )
        
        with pact:
            result = self.user_service_client.get_user_by_email("test@example.com")
        
        assert result.email == "test@example.com"
```

## F.3 Test-Driven Implementation Loop

```python
class TDDImplementationLoop:
    """
    Implementiert den Test-Driven-Development Loop für Agenten.
    Agent schreibt Code → Tests ausführen → Analysieren → Korrigieren → Wiederholen
    """
    
    ANALYSIS_PROMPT = """
<role>Du bist ein erfahrener Entwickler der Test-Fehler analysiert.</role>

<failed_tests>
{failed_tests}
</failed_tests>

<current_code>
```{language}
{current_code}
```
</current_code>

<spec>
{spec}
</spec>

<task>
Analysiere die fehlgeschlagenen Tests und identifiziere:
1. <root_cause>Was ist die eigentliche Ursache des Fehlers?</root_cause>
2. <fix_strategy>Welche minimale Änderung behebt den Fehler?</fix_strategy>
3. <code_change>Zeige den genauen Code-Diff (unified diff format)</code_change>
4. <risks>Welche anderen Tests könnten durch die Änderung brechen?</risks>
</task>
"""

    def run(self, state: SDLCState) -> SDLCState:
        """Führt den TDD-Loop bis alle Tests grün sind."""
        
        while not state.test_results.get("passed"):
            
            # Guard gegen Endlosschleifen
            if not state.increment_iteration():
                self._escalate_to_human(state, "Max-Iterationen erreicht")
                return state
            
            # Analysiere Fehler
            analysis = self._analyze_failures(state)
            
            # Implementiere Fix
            new_code = self._implement_fix(state, analysis)
            
            # Führe Tests aus
            state.test_results = self._run_tests(new_code)
            state.last_tool_output = state.test_results
            
            # Logge Iteration ins episodische Gedächtnis
            self.memory.log_tool_call(
                task_id=state.task_id,
                iteration=state.iteration_count,
                tool_name="run_tests",
                tool_input={"code_hash": hash(new_code)},
                stdout=state.test_results.get("output", ""),
                stderr=state.test_results.get("error", ""),
                exit_code=0 if state.test_results.get("passed") else 1
            )
        
        state.implementation_complete = True
        return state
```

---

# TEIL G — PROMPT-SYNTAX & INSTRUKTIONS-PROTOKOLLE

## G.1 Syntaktisches Engineering — Delimiter-Protokoll

**Die Wahl der Delimiter hat dramatischen Einfluss auf die LLM-Leistung.**
Leistungsvarianz bis zu 90% durch reine Formatierungsänderungen.

### Standardisierte Delimiter-Hierarchie (modellübergreifend kompatibel):

```
LEVEL 1 — Haupt-Sektionen:        ###  oder  ---
LEVEL 2 — Sub-Sektionen:          <tag_name>...</tag_name>
LEVEL 3 — Code:                   ```language ... ```
LEVEL 4 — Inline-Hervorhebung:    `inline code`
LEVEL 5 — Listen:                 - item (für Aufzählungen)
```

**Warum:** Diese Zeichen werden über alle LLM-Familien (GPT, Claude, Llama) konsistent als einzelne, strukturell signifikante Tokens verarbeitet. Sie erzeugen klare "Token Gaze"-Ankerpunkte.

### Kanonisches Prompt-Template für Agenten:

```
---
ROLE: [Exakt eine Domäne. Keine irrelevanten Attribute.]
---

### CONSTITUTION (Non-Negotiable)
[Kritischste Regeln — max. 5 Punkte]

### TASK
<task>
[Exakte Aufgabenbeschreibung — spezifisch, messbar]
</task>

### CONTEXT
<spec>
[Relevanter Schnittstellenvertrag — nur was gebraucht wird]
</spec>

<code>
```python
[Nur relevanter Code via JIT-RAG — nicht die ganze Codebase]
```
</code>

<constraints>
- [Constraint 1]
- [Constraint 2]
</constraints>

### OUTPUT FORMAT
<output_schema>
{
  "action": "string (tool_name)",
  "input": { ... },
  "rationale": "string (Begründung — max 2 Sätze)"
}
</output_schema>

### CURRENT TASK (wiederholt am Ende für Attention-Optimierung)
<execute>
[Genau dasselbe wie TASK — erzeugt Attention-Hotspot]
</execute>
```

## G.2 Persona-Engineering-Regeln

**KRITISCH:** Irrelevante Persona-Details reduzieren die Leistung um bis zu 30 Prozentpunkte.

```
RICHTIG:
"Du bist ein erfahrener Backend-Entwickler spezialisiert auf Python und REST-APIs."

FALSCH:
"Du bist ein erfahrener Backend-Entwickler namens Alex, der gerne Kaffee trinkt und 
in Berlin lebt. Du hast 10 Jahre Erfahrung..."

REGEL: Persona = MAXIMAL DESKRIPTIV für die Zieldomäne
              + MINIMAL DESKRIPTIV für alles andere
```

## G.3 Chain-of-Verification (CoVe) — Standard für Faktenprüfung

Für alle faktenkritischen Ausgaben (API-Definitionen, Sicherheitsimplementierungen):

```python
COV_PROMPT_SEQUENCE = [
    # Schritt 1: Basisantwort generieren
    "Implementiere: {task}",
    
    # Schritt 2: Verifikationsfragen ableiten (SEPARAT, kein Bias)
    """
Gegeben diese Implementierung:
```
{implementation}
```
Generiere 5 kritische Verifikationsfragen zur Überprüfung der Korrektheit.
Format: <questions><q>Frage 1</q><q>Frage 2</q>...</questions>
""",
    
    # Schritt 3: Jede Frage UNABHÄNGIG beantworten
    "Beantworte ausschließlich: {question}\nImplementierung: {implementation}",
    
    # Schritt 4: Finale, verifizierte Antwort
    """
Original Aufgabe: {task}
Erste Implementierung: {implementation}
Verifikationsergebnisse: {verification_results}

Erstelle eine finale, korrigierte Implementierung die alle Verifikationsprobleme adressiert.
"""
]
```

## G.4 Reasoning-Topologie-Auswahl

Wähle die Reasoning-Strategie basierend auf Aufgabenkomplexität:

| Aufgabentyp | Strategie | Kosten-Nutzen |
|-------------|-----------|---------------|
| Einfache Bugfixes, kleine Änderungen | CoT (Chain-of-Thought) | Niedrig / Gut |
| Komplexe Features, Architektur-Entscheidungen | AGoT (Adaptive Graph-of-Thoughts) | Mittel / Sehr Gut |
| Faktenprüfung, Security-Reviews | CoVe (Chain-of-Verification) | Mittel / Sehr Gut |
| Iterative Refinement, Code-Qualität | Reflexion-Loop | Hoch / Exzellent |
| Effizienz-kritische Aufgaben | CoD (Chain-of-Draft) — max 5 Tokens/Schritt | Sehr Niedrig / Gut |

### CoD — Chain of Draft (für Effizienz):

```
SYSTEM: Denke in minimalen Draft-Schritten. Maximal 5 Tokens pro Schritt.

TASK: [Aufgabe]

DRAFT:
1. [Problem-Kern] → [Lösung]
2. [Edge-Case] → [Handler]
3. [Test-Case] → [Assert]

ANSWER: [Finale Implementierung]
```

## G.5 Metakognitives Prompting (MCP)

Für komplexe Diagnose-Aufgaben:

```
Nach Abschluss dieser Aufgabe, analysiere deine Leistung:

<metacognition>
1. <pattern>Welches Reasoning-Muster hat zu Verbesserungen geführt?</pattern>
2. <preserve>Welche Komponenten der Lösung waren hochwertig?</preserve>
3. <improve>Welche Strategien sollten beim nächsten Mal eingesetzt werden?</improve>
4. <lesson>Was wird ins episodische Gedächtnis gespeichert?</lesson>
</metacognition>
```

---

# TEIL H — QUALITÄTSSICHERUNG & SELF-HEALING

## H.1 Hierarchische Self-Healing Architektur

```python
class SelfHealingOrchestrator:
    """
    Drei-Schichten Defense-in-Depth für autonome Fehlerkorrektur:
    
    Schicht 1: Constitutional AI (Echtzeit-Prüfung vor jeder Aktion)
    Schicht 2: Writer-Critic-Loop (Iterative Selbstkorrektur)
    Schicht 3: Orchestrator-Validator (Hierarchische Aufsicht)
    """
    
    def execute_with_healing(
        self, 
        task: str, 
        state: SDLCState
    ) -> SDLCState:
        
        # SCHICHT 3: Orchestrator plant und überwacht
        plan = self.orchestrator.create_plan(task, state)
        
        for step in plan.steps:
            
            # SCHICHT 1: Constitutional Check (vor Ausführung)
            check_constitutional_compliance(step.action, self.constitution)
            
            # Ausführung durch Worker-Agent
            result = self.worker.execute(step, state)
            
            # SCHICHT 2: Critic-Loop (nach Ausführung)
            for attempt in range(3):  # Max 3 Kritik-Iterationen pro Step
                critique = self.critic.evaluate(result, step.acceptance_criteria)
                
                if critique.approved:
                    break
                
                # Verfeinere basierend auf Kritik
                result = self.worker.refine(result, critique.feedback, state)
            
            # SCHICHT 3: Validator-Agent prüft Übergabe
            if not self.validator.check(result, step.output_schema):
                self.orchestrator.handle_failure(step, result, state)
                continue
            
            # Update State
            state = self._update_state(state, result)
        
        return state
```

## H.2 Automatisierte Spec-Konsistenz-Prüfung

```python
class SpecConsistencyAgent:
    """
    Consistency-Checking-Agent: 
    Analysiert das gesamte Spec-Repository auf Inkonsistenzen.
    Läuft als CI-Job nach jeder Spec-Änderung.
    """
    
    CONSISTENCY_PROMPT = """
<role>Du bist ein Spec-Architekt-Agent für Konsistenz-Analyse.</role>

<changed_spec>
{changed_spec}
</changed_spec>

<all_dependent_specs>
{dependent_specs}
</all_dependent_specs>

<task>
Analysiere ob die Änderung an {spec_id} Inkonsistenzen mit den abhängigen Specs erzeugt.

Berichte:
<inconsistencies>
  <item spec="{dep_spec_id}" severity="HIGH|MEDIUM|LOW">
    Beschreibung der Inkonsistenz
  </item>
</inconsistencies>

<recommendation>
Welche abhängigen Specs müssen aktualisiert werden?
</recommendation>
</task>
"""
    
    def check_after_change(self, changed_spec_id: str) -> ConsistencyReport:
        """
        Human-in-the-Loop: Agent findet Kandidaten,
        Mensch entscheidet über semantische Korrektheit.
        """
        dependent_specs = self.dependency_graph.get_dependents(changed_spec_id)
        
        report = self.llm.generate(
            self.CONSISTENCY_PROMPT.format(
                spec_id=changed_spec_id,
                changed_spec=self.load_spec(changed_spec_id),
                dependent_specs=self.load_specs(dependent_specs)
            )
        )
        
        return ConsistencyReport.parse(report)
```

## H.3 Prompt-Library-Governance

Alle Prompts werden als versionierte Artefakte verwaltet:

```
.agent/
└── prompts/
    ├── system/
    │   ├── constitution-check.v1.2.yaml
    │   ├── tdd-implementation.v2.0.yaml
    │   └── pr-generation.v1.0.yaml
    └── templates/
        ├── spec-analysis.jinja2
        ├── code-review.jinja2
        └── error-analysis.jinja2
```

```yaml
# .agent/prompts/system/tdd-implementation.v2.0.yaml
id: tdd-implementation
version: "2.0"
description: "Prompt für TDD Implementation Loop"
model_params:
  temperature: 0.2     # Niedrig für deterministische Code-Generierung
  max_tokens: 4000

template: |
  ---
  ROLE: Erfahrener Softwareentwickler spezialisiert auf {language} und {framework}.
  ---

  ### CONSTITUTION
  {constitution_excerpt}

  ### TASK
  <task>
  Implementiere exakt den in der Spec definierten Vertrag.
  Mache keine Annahmen. Wenn die Spec unklar ist → STOPP und frage nach.
  </task>

  ### FAILING TESTS
  <tests>
  {failing_tests}
  </tests>

  ### INTERFACE CONTRACT
  <contract>
  {interface_contract}
  </contract>

  ### CURRENT IMPLEMENTATION
  <code>
  ```{language}
  {current_code}
  ```
  </code>

  ### EXECUTE
  <execute>
  Schreibe minimalen Code um alle failing Tests grün zu machen.
  Output: unified diff format.
  </execute>

changelog:
  - version: "2.0"
    changes: "Hinzugefügt: Interface Contract Sektion, Constitution Excerpt"
  - version: "1.0"
    changes: "Initial version"
```

---

# TEIL I — OPERATIVES PLAYBOOK (Schnellreferenz)

## I.1 Projekt-Initialisierungs-Checklist

**Wenn du ein neues Projekt oder eine neue Codebase erhältst:**

```bash
#!/bin/bash
# agent-init.sh — Führe dieses Skript als erstes aus

echo "=== AGENT INITIALIZATION ==="

# 1. Verzeichnisstruktur erstellen
mkdir -p .agent/{specs/{interfaces,modules},prompts/{system,templates}}
mkdir -p tests/{specs,unit,integration,contracts,e2e}

# 2. Pflicht-Dateien erstellen (wenn nicht vorhanden)
[[ ! -f CLAUDE.md ]] && cat > CLAUDE.md << 'EOF'
# CLAUDE.md — Agenten-Konfiguration
Lies vor jeder Aktion: .agent/CONSTITUTION.md
Befolge: LLM_AGENT_MASTER_GUIDE.md
Validiere immer: Spec → Tests → Implementation
EOF

[[ ! -f GEMINI.md ]] && cat > GEMINI.md << 'EOF'
# GEMINI.md — Gemini-CLI Konfiguration  
Lies vor jeder Aktion: .agent/CONSTITUTION.md
Befolge: LLM_AGENT_MASTER_GUIDE.md
EOF

# 3. Git-Hooks installieren
cat > .git/hooks/pre-commit << 'EOF'
#!/bin/bash
# Constitutional Compliance Check
python scripts/verify_constitution.py || exit 1
# Tests müssen grün sein
pytest tests/unit/ -q || exit 1
EOF
chmod +x .git/hooks/pre-commit

# 4. GitHub-Templates erstellen
mkdir -p .github/{ISSUE_TEMPLATE,PULL_REQUEST_TEMPLATE}

cat > .github/PULL_REQUEST_TEMPLATE/default.md << 'EOF'
## Spec-Referenz
- KI-MS: 
- KI-SV: 

## Akzeptanzkriterien
- [ ] 
- [ ] 

## Tests
- [ ] Unit-Tests grün
- [ ] Contract-Tests grün
- [ ] Coverage > 80%

## Constitutional Compliance
- [ ] Alle ADRs eingehalten
- [ ] Kein direkter Implementierungs-Import (DIP)
EOF

echo "✅ Initialisierung abgeschlossen"
```

## I.2 Aufgaben-Starter-Protokoll

**Für JEDEN neuen Task — diese Reihenfolge ist obligatorisch:**

```
SCHRITT 1: Lese CONSTITUTION.md und ARCHITECTURE.md
SCHRITT 2: Führe A.1 Diagnose-Checkliste aus
SCHRITT 3: Erstelle/Aktualisiere relevante KI-MS
SCHRITT 4: Warte auf Spec-Approval (Human Checkpoint)
SCHRITT 5: Erstelle/Aktualisiere KI-SV (Interface Contract)
SCHRITT 6: Warte auf Contract-Approval (Human Checkpoint)
SCHRITT 7: Schreibe Tests GEGEN DEN VERTRAG
SCHRITT 8: Warte auf Test-Approval (Human Checkpoint)
SCHRITT 9: Implementiere bis Tests grün
SCHRITT 10: Führe Adversarial-Validation durch
SCHRITT 11: Erstelle PR mit vollständiger Dokumentation
```

## I.3 Fehler-Behandlungs-Protokoll

```python
ERROR_PROTOCOL = {
    "ambiguous_spec": {
        "action": "STOP",
        "message": "Spec unklar: [Details]. Bitte klären bevor Implementierung.",
        "never": "Annahmen treffen und weitermachen"
    },
    "test_failure_loop": {
        "trigger": "Selber Fehler nach 3 Iterationen",
        "action": "ESCALATE_TO_HUMAN",
        "message": "Konnte Fehler nach {n} Iterationen nicht lösen: [Fehler-Details]"
    },
    "constitutional_violation": {
        "action": "BLOCK",
        "message": "Aktion verletzt Verfassung: [Regel]. Aktion wurde nicht ausgeführt.",
        "log": True
    },
    "contract_mismatch": {
        "action": "STOP_AND_NOTIFY",
        "message": "Implementierung weicht von Vertrag ab in: [Details]",
        "require": "Updated contract or implementation change"
    },
    "max_iterations_reached": {
        "action": "HUMAN_ESCALATION",
        "message": "Max-Iterationen ({max}) erreicht. Manuelles Review erforderlich.",
        "provide": "Full error_history from SDLCState"
    }
}
```

## I.4 Kontext-Injektions-Reihenfolge (Positions-Optimiert)

```
POSITION 1 (Anfang — höchste Attention):
├── Constitution-Extrakt (5 wichtigste Regeln)
├── Aktueller Task (kurz)
└── Interface-Vertrag (relevant)

POSITION 2 (Mitte — komprimiert):
├── Relevanter Code (via JIT-RAG, max 3 Funktionen)
├── Letzte 3 Test-Fehler (wenn vorhanden)
└── Episodisches Gedächtnis (Top-3 ähnliche Situationen)

POSITION 3 (Ende — höchste Attention):
├── Exakter Task (wiederholt!)
├── Output-Format-Spezifikation
└── Constraints
```

## I.5 Anti-Pattern-Katalog (Was du NIE tun sollst)

```
❌ Codebase vollständig in Kontext laden → Memory-Overflow, Degradation
❌ Ohne Spec implementieren → Halluzinierte Logik
❌ Tests nach dem Code schreiben → Tests passen sich Code an, nicht umgekehrt
❌ Irrelevante Persona-Details (Name, Hobbies) → Leistungsabfall bis 30%
❌ Unbegrenzte Iterations-Loops → Ressourcenverschwendung
❌ Unkritisch Zwischen-Ergebnisse anderer Agenten übernehmen → Trust-Vulnerability
❌ Direkte Implementierungs-Imports → Tight Coupling, verletzt DIP
❌ Commits ohne grüne Tests → Instabile Codebase
❌ PRs ohne Spec-Referenz → Nicht nachvollziehbar
❌ Vage Verben in Specs (verarbeite, handhabe) → Halluzinationen vorprogrammiert
❌ Hedging-Sprache im Prompt (vielleicht, möglicherweise) → Reduzierte Bestimmtheit
❌ Kontext in der Mitte platzieren ohne Delimiter → Lost-in-the-Middle
```

---

## ANHANG A — Technologie-Stack-Empfehlungen

| Aufgabe | Empfohlenes Tool | Alternative |
|---------|-----------------|-------------|
| Workflow-Orchestrierung | LangGraph (expliziter State) | CrewAI (linear) |
| Contract-Testing | Pact / Dredd | Postman Collections |
| Spec-Versionierung | Git + YAML | Datenbank (für Scale) |
| Episodisches Gedächtnis | PostgreSQL + pgvector | Chroma / Pinecone |
| Code-Embedding | CodeBERT | OpenAI Ada-002 |
| CI/CD | GitHub Actions | GitLab CI |
| Spec-Linting | Custom Python-Script | Spectral (OpenAPI) |
| API-Spezifikation | OpenAPI 3.1 | gRPC / Proto |

## ANHANG B — Kritische Metriken und Schwellenwerte

```
Test-Coverage-Minimum:     80% (BLOCKER bei < 70%)
Max Iterations/Task:       10 (Escalation bei Überschreitung)
Max Kontext-Größe:         40% des Kontextfensters
Spec-Review-Timeout:       24h (dann automatisch eskalieren)
Contract-Drift-Toleranz:   0% (kein Drift erlaubt)
Performance-Regression:    Max 10% slowdown erlaubt
PR-Größe:                  Max 400 Lines Changed (Split sonst)
```

## ANHANG C — Glossar

| Begriff | Definition |
|---------|-----------|
| KI-AGD | KI-Architektur-Gesamtdokument (= ARCHITECTURE.md) |
| KI-AM | KI-Abhängigkeits-Map (Dependency Graph) |
| KI-MS | KI-Mikrospezifikation (atomare Spezifikations-Einheit) |
| KI-SV | KI-Schnittstellenvertrag (Interface Contract) |
| SDLCState | Software Development Lifecycle State (kanonisches Zustandsobjekt) |
| JIT-RAG | Just-in-Time Retrieval-Augmented Generation |
| TDFlow | Test-Driven Agentic Workflow |
| CoVe | Chain-of-Verification |
| AGoT | Adaptive Graph-of-Thoughts |
| AgentRR | Agent Record & Replay (deterministisches Debugging) |
| MAS | Multi-Agent-System |
| SDD | Spec-Driven Development |
| ADR | Architecture Decision Record |

---

*LLM_AGENT_MASTER_GUIDE.md — Version 1.0*
*Synthetisiert aus: Agents_1/2, Context_1/2/3, Prompts_1/2/3/4/5/6*
*Zielgruppe: Gemini-CLI, Claude Code, kompatible Coding-Agenten*
