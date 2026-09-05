# STRUCTURED PROCESS ORCHESTRATION SPECIFICATION
## Cognitive Architecture as Code — Master Reference v2.0

> **Classification:** Senior Architecture Artefact  
> **Scope:** Agentic Coding · Spec-Driven Development · Workflow Orchestration · Systems Thinking  
> **Paradigma:** Deterministische Prozesssteuerung durch hierarchische Constraint-Kaskaden

---

## EXECUTIVE SUMMARY

Traditionelles Prompt-Crafting ist für autonome Multi-Agenten-Systeme strukturell unzureichend. Die Fehlerklassen — **Halluzination**, **Context Rot**, **Architectural Drift**, **Infinite Loops**, **Cross-Context Bleeding** — entstehen nicht durch mangelnde Modellkapazität, sondern durch nicht-deterministische Informationsumgebungen.

Diese Spezifikation definiert das vollständige Rahmenwerk für **deterministische Prozesssteuerung** auf zwei Abstraktionsebenen:

- **Makro-Ebene:** Orchestrierungszyklen, Feedback-Schleifen, Workflow-DAGs, Eskalationsketten
- **Mikro-Ebene:** Atomare Ausführungseinheiten, Zustandsprüfungen, Verifikationskontrakte, Gedächtnisarchitektur

Das Ergebnis ist eine **„Cognitive Architecture as Code"** — ein System, das nicht _hofft_, dass das Modell die richtige Entscheidung trifft, sondern den Lösungsraum durch formale Constraints so einschränkt, dass fehlerhafte Outputs strukturell unmöglich werden.

---

## INHALTSSTRUKTUR

```
TEIL I   — SYSTEMARCHITEKTUR & DESIGNPRINZIPIEN
TEIL II  — MAKRO-MUSTER: Orchestrierungszyklen
TEIL III — MIKRO-MUSTER: Atomare Ausführungseinheiten
TEIL IV  — SPEC-DRIVEN DEVELOPMENT FRAMEWORK
TEIL V   — AGENTIC CODING PATTERNS
TEIL VI  — SYSTEMS THINKING: Feedback & Emergenz
TEIL VII — MASTER EXECUTION FLOW & TEMPLATE MAP
```

---

# TEIL I — SYSTEMARCHITEKTUR & DESIGNPRINZIPIEN

## 1.1 Fundamentale Fehlerklassen & ihre strukturellen Ursachen

| Fehlerklasse | Ursache | Strukturelle Lösung |
|:---|:---|:---|
| **Halluzination** | LLM füllt Wissenslücken probabilistisch | Formale Constraints → logische Deduktion statt Inferenz |
| **Context Rot** | Token-Degradation bei > 8K Token | Minimal Necessary Context (MNC) Injection |
| **Architectural Drift** | Fehlende globale Invarianten | System Constitution + Hierarchische Kaskade |
| **Infinite Loops** | Fehlende Bounded Iteration | Confidence Gates + max_attempts Enforcement |
| **Cross-Context Bleeding** | Monolithische Konfiguration | Nearest-File-Präzedenz + Layer-Scoping |
| **Silent Error Propagation** | Keine Zwischenzustandsprüfung | PDCA-Checkpoint nach jedem Tool-Call |
| **Spec Ambiguity** | Prosa-basierte Anforderungen | LTL-Formalisierung + Graph-of-Thoughts |

## 1.2 Kernprinzipien der Architektur

### Prinzip 1 — Minimal Necessary Context (MNC)

Ein Agent erhält ausschließlich den Kontext, der für sein aktuelles Verzeichnis und seine aktuelle Aufgabe notwendig ist. Jeder zusätzliche Token erhöht die Wahrscheinlichkeit von „Lost-in-the-Middle"-Degradation nicht-linear.

```
MNC-Formel: context_load = f(task_scope) ≠ f(project_scope)
```

### Prinzip 2 — Deterministischer Constraint-Stack

Verhalten wird nicht durch Empfehlungen, sondern durch dreistufige Constraints gesteuert:

```
INHIBIT  → absolute Verbote (verletzbar = kritischer Fehler)
MANDATE  → absolute Gebote (verletzbar = kritischer Fehler)
PREFER   → Präferenzen (Qualitätssignal, nicht blockierend)
```

### Prinzip 3 — Design-by-Contract auf allen Ebenen

Jede Schnittstelle — zwischen Agenten, zwischen Modulen, zwischen Schichten — ist durch explizite Pre/Post-Conditions und Invarianten definiert. Keine Implementierung ohne vorherigen Vertrag.

### Prinzip 4 — Bounded Execution

Jede Schleife, jeder Retry, jede Selbstkorrektur hat eine mathematisch definierte Obergrenze. Loops beyond 4 iterations degradieren Outputqualität empirisch nachweisbar.

### Prinzip 5 — Verifiable Outputs

Jedes Artefakt trägt einen SHA-256-Hash. Jede Verifikation arbeitet gegen den Spec-Hash, nicht gegen die Interpretation des Autors. Outputs sind Beweise, keine Meinungen.

---

# TEIL II — MAKRO-MUSTER: Orchestrierungszyklen

## 2.1 Der OODA-Loop auf Orchestrierungsebene

Der OODA-Loop (Observe–Orient–Decide–Act) ist das primäre Kontrollparadigma für den Orchestrator-Agenten. Im Gegensatz zur klassischen Militär-Anwendung operiert er auf Spec-Artefakten, nicht auf situativen Beobachtungen.

```
┌─────────────────────────────────────────────────────────────┐
│                    OODA ORCHESTRATION LOOP                  │
│                                                             │
│  OBSERVE ──────→ ORIENT ──────→ DECIDE ──────→ ACT         │
│     │                │              │            │          │
│  Load T-08        Load T-06      Select        Execute      │
│  LTM State        MNC Context    Agent         T-04 DAG     │
│  Read T-07        Evaluate       Manifest      Dispatch     │
│  Checkpoints      Constraints    T-02          T-03 ACP     │
│     │                │              │            │          │
│     └────────────────┴──────────────┴────────────┘          │
│                         FEEDBACK                            │
│              T-05 Verification Gate                         │
│              T-07 Metacognitive Checkpoint                  │
└─────────────────────────────────────────────────────────────┘
```

**Kritischer Unterschied zu naivem Agenten-Looping:** Jede OODA-Phase ist durch ein Artefakt-Schema gebunden. Der Agent `OBSERVES` nicht frei — er lädt definierte State-Deltas. Er `DECIDES` nicht intuitiv — er matcht gegen Agent-Manifeste. Dies eliminiert die „Gap-Filling"-Halluzinationsklasse vollständig.

## 2.2 Der PDCA-Zyklus auf Task-Ebene

Plan–Do–Check–Act ist die Ausführungsschleife für jeden Worker-Agenten auf Mikro-Ebene. Er ist nicht optional — er ist die einzige erlaubte Ausführungsform.

```
┌──────────────────────────────────────────────────────────────┐
│                    PDCA EXECUTION CYCLE                      │
│                                                              │
│   PLAN ─────────────────────────────────────────────────┐   │
│   Load T-01 Micro-Spec                                   │   │
│   Verify Pre-Conditions                                  │   │
│   Load T-08 STM Scratchpad                               │   │
│   Activate relevant T-06 Context Layer                   │   │
│        │                                                 │   │
│        ↓                                                 │   │
│   DO ─────────────────────────────────────────────────┐ │   │
│   Execute Logic Graph Node N                          │ │   │
│   Tool-Call → Result                                  │ │   │
│        │                                              │ │   │
│        ↓                                              │ │   │
│   CHECK ──────────────────────────────────────────┐   │ │   │
│   T-07 Metacognitive Checkpoint                   │   │ │   │
│   Post-Condition satisfied? Y→ next node          │   │ │   │
│   Invariant violated? Y→ ESCALATE                 │   │ │   │
│   Confidence < threshold? Y→ bounded retry        │   │ │   │
│        │                                          │   │ │   │
│        ↓                                          │   │ │   │
│   ACT ─────────────────────────────────────────┐  │   │ │   │
│   PASS → T-07 commit state, next node          │  │   │ │   │
│   RETRY → mutated strategy (NOT identical)     │  │   │ │   │
│   ESCALATE → T-03 INCIDENT → Orchestrator      │  │   │ │   │
│   COMPLETE → T-08 LTM commit (conf ≥ 80)       │  │   │ │   │
│        └──────────────────────────────────────────┘   │ │   │
│        └──────────────────────────────────────────────┘ │   │
│        └────────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────────────────┘
```

**Kritische Regel:** Retries **müssen** sich vom Originalversuch unterscheiden. Identische Retries sind ein expliziter INHIBIT-Verstoß. Ein Retry ohne Strategieänderung ist kein Retry — es ist ein deterministischer Fehler.

## 2.3 Der Verifikations-Zyklus (CoVe 4-Phasen-Gate)

Chain-of-Verification ist die Qualitätssicherungsschleife vor jedem Merge oder State-Commit. Sie operiert auf dem Produkt-Artefakt, nicht auf der Intention des Autors.

```
┌───────────────────────────────────────────────────────────────┐
│              CoVe VERIFICATION GATE (T-05)                    │
│                                                               │
│  PHASE 1: Baseline Response                                   │
│  Agent generates output against T-01 spec                    │
│                                                               │
│  PHASE 2: Verification Questions (VQ-01 bis VQ-07)            │
│  Verifier generates checklist WITHOUT reading agent output    │
│  Derives VQs ONLY from T-01 spec + T-03 interface contract   │
│                                                               │
│  PHASE 3: Independent Verification                            │
│  Verifier answers each VQ independently                       │
│  PASS | FAIL only — no PARTIAL for VQ-05, VQ-06, VQ-07       │
│  Bias-Guard: VQs answered in isolation (no adjacent context)  │
│                                                               │
│  PHASE 4: Confidence Gate + Iteration Control                 │
│  conf ≥ 85 + 0 CRITICAL → AUTO APPROVED                       │
│  60 ≤ conf < 85 | MEDIUM → APPROVED_WITH_CONDITIONS           │
│  conf < 60 | CRITICAL → REJECTED → PDCA retry                │
│  N > max_attempts | any CRITICAL VQ FAIL → ESCALATE           │
│                                                               │
│  OUTPUT: SHA-256(verified_artifact) + confidence + attempts   │
└───────────────────────────────────────────────────────────────┘
```

## 2.4 Der Hierarchische Kaskaden-Zyklus (Context Lifecycle)

Der Context Lifecycle beschreibt, wie Informationen in das Agenten-System fließen, verarbeitet werden und wieder verschwinden — ohne dass irrelevanter Kontext akkumuliert.

```
┌───────────────────────────────────────────────────────────────┐
│           CONTEXT LIFECYCLE — Hierarchische Kaskade           │
│                                                               │
│  BOOTSTRAP                                                    │
│  Root AGENTS.md → T-06 lexicon.md → Agent Manifeste T-02     │
│                                                               │
│  ACTIVATION (per Task)                                        │
│  Orchestrator identifiziert Arbeitsverzeichnis                │
│  Nearest-File lookup → spezifischste AGENTS.md gewinnt       │
│  JIT context load: load_when condition evaluated              │
│  MNC injection: nur relevante Dokumente → STM (T-08)          │
│                                                               │
│  EXECUTION                                                    │
│  Agent arbeitet ausschließlich gegen geladenen Kontext        │
│  Tool-Calls → T-07 Checkpoint → State-Delta update            │
│                                                               │
│  DEACTIVATION                                                 │
│  STM gecleart nach Task-Completion (FIFO-Scratchpad)          │
│  LTM commit nur bei confidence ≥ 80 (T-08 commit rules)       │
│  Spekulative Schlüsse INHIBITED für LTM                       │
│                                                               │
│  ROOT   /AGENTS.md ────── globale Axiome                      │
│    └── /packages/api/AGENTS.md ── backend constraints         │
│          └── /packages/api/auth/AGENTS.md ── feature rules    │
│  Winner: immer die nächstliegende Datei                       │
└───────────────────────────────────────────────────────────────┘
```

---

# TEIL III — MIKRO-MUSTER: Atomare Ausführungseinheiten

## 3.1 Der Graph-of-Thoughts (GoT) Execution Node

Jede atomare Task wird nicht als freier Prompt, sondern als deterministischer Graph modelliert. Jeder Knoten hat exakt einen Eingang, eine Operation und einen Ausgang.

```xml
<logic_graph>
  <node id="1" type="VALIDATE">
    <input>$task_input</input>
    <operation>ASSERT_SCHEMA(interface_contract_id)</operation>
    <output>$validated_input</output>
    <on_fail>ABORT → T-03 INCIDENT</on_fail>
  </node>
  <node id="2" type="TRANSFORM">
    <input>$validated_input</input>
    <operation>EXECUTE_BUSINESS_LOGIC</operation>
    <output>$transformed_state</output>
    <on_fail>RETRY (max=3, backoff=EXPONENTIAL) → ESCALATE</on_fail>
  </node>
  <node id="3" type="PERSIST">
    <input>$transformed_state</input>
    <operation>COMMIT_TO_TARGET(database_table)</operation>
    <output>$commit_receipt</output>
    <on_fail>ABORT → ROLLBACK → T-03 INCIDENT</on_fail>
  </node>
  <node id="4" type="VERIFY">
    <input>$commit_receipt</input>
    <operation>ASSERT_POST_CONDITIONS</operation>
    <output>$verification_result</output>
    <on_pass>COMPLETE → T-08 LTM commit</on_pass>
    <on_fail>ESCALATE → human_gate</on_fail>
  </node>
</logic_graph>
```

**Design-Rationale:** GoT-Knoten eliminieren das „Multi-Step Reasoning Collapse"-Phänomen, bei dem LLMs spätere Schritte mit früherem Kontext kontaminieren. Jeder Knoten ist epistemisch isoliert.

## 3.2 Der Metacognitive Checkpoint (T-07)

Nach **jedem** Tool-Call führt der Agent einen strukturierten Selbst-Check durch. Dies ist keine optionale Reflexion — es ist ein verifikationspflichtiges Gate.

```xml
<metacognitive_checkpoint>
  <step_id>{{current_node_id}}</step_id>
  <timestamp>{{iso_8601}}</timestamp>

  <!-- 1. Pre/Post Contract Verification -->
  <contract_check>
    <pre_condition_satisfied>{{TRUE | FALSE}}</pre_condition_satisfied>
    <post_condition_satisfied>{{TRUE | FALSE}}</post_condition_satisfied>
    <invariant_violated>{{FALSE | VIOLATION_DESCRIPTION}}</invariant_violated>
  </contract_check>

  <!-- 2. State Delta Recording -->
  <state_delta>
    <tool_called>{{tool_name}}</tool_called>
    <input_hash>{{SHA256_OF_INPUT}}</input_hash>
    <output_summary>{{MAX_50_TOKEN_SUMMARY}}</output_summary>
    <output_hash>{{SHA256_OF_OUTPUT}}</output_hash>
    <confidence>{{0-100}}</confidence>
  </state_delta>

  <!-- 3. Anomaly Detection -->
  <anomaly_detection>
    <unexpected_output>{{FALSE | DESCRIPTION}}</unexpected_output>
    <spec_deviation>{{FALSE | DEVIATION_TYPE}}</spec_deviation>
    <action>{{CONTINUE | RETRY_WITH_MUTATION | ESCALATE | ABORT}}</action>
  </anomaly_detection>

  <!-- 4. Strategy Adaptation (mandatory for retries) -->
  <strategy_adaptation>
    <original_approach>{{IF_RETRY: describe original}}</original_approach>
    <mutated_approach>{{MUST differ from original — identical retries INHIBITED}}</mutated_approach>
    <mutation_rationale>{{WHY this mutation addresses the failure}}</mutation_rationale>
  </strategy_adaptation>

</metacognitive_checkpoint>
```

## 3.3 Die Deterministische Fehler-Matrix

Fehler sind keine Ausnahmen — sie sind definierte Zustandsübergänge mit spezifizierten Antwortpfaden.

```yaml
error_matrix:
  
  # Klasse 1: Transiente Fehler — temporal, automatisch behebbar
  - code_range: [429, 503, 504]
    class: TRANSIENT
    agent_action: RETRY_EXPONENTIAL_BACKOFF
    params:
      max_attempts: 3
      base_ms: 500
      multiplier: 2
      # Sequenz: 500ms → 1000ms → 2000ms
    on_exhaustion: ESCALATE_TO_ORCHESTRATOR

  # Klasse 2: Logische Fehler — Spec-Verletzung, Strategiemutation nötig
  - code_range: [400, 422]
    class: LOGICAL
    agent_action: ABORT_CURRENT_APPROACH
    params:
      mutation_required: true
      reload_spec: true
    on_exhaustion: ESCALATE_TO_HUMAN_GATE

  # Klasse 3: Fatale Fehler — sofortiger Abbruch, keine Retries
  - code_range: [401, 403, 500]
    class: FATAL
    agent_action: ABORT_AND_ESCALATE
    params:
      create_incident: true
      rollback_state: true
    on_trigger: IMMEDIATE_ESCALATION

  # Klasse 4: Invarianten-Verletzung — architektonischer Fehler
  - type: INVARIANT_VIOLATION
    class: ARCHITECTURAL
    agent_action: HALT_ALL_DEPENDENT_AGENTS
    params:
      notify: [ORCHESTRATOR, HUMAN_GATE]
      quarantine_output: true
```

## 3.4 Die Memory Architecture (STM / LTM Separation)

```
┌─────────────────────────────────────────────────────────────┐
│                  MEMORY ARCHITECTURE (T-08)                  │
│                                                              │
│  SHORT-TERM MEMORY (STM)          LONG-TERM MEMORY (LTM)    │
│  ┌────────────────────────┐       ┌───────────────────────┐  │
│  │ Scope: Current Task    │       │ Scope: Cross-Session  │  │
│  │ Format: CoD Scratchpad │       │ Format: Semantic KB   │  │
│  │ Retention: Task only   │       │ Retention: Permanent  │  │
│  │ Eviction: FIFO         │       │ Eviction: Explicit    │  │
│  │                        │       │                       │  │
│  │ WRITES: every node     │       │ WRITES: only when:    │  │
│  │ READS: current agent   │       │  • confidence ≥ 80    │  │
│  │ CLEARS: task complete  │       │  • verified artifact  │  │
│  │                        │       │  • no speculation     │  │
│  └────────────────────────┘       └───────────────────────┘  │
│                                                              │
│  INHIBIT: Speculative conclusions → LTM                      │
│  INHIBIT: Unverified outputs → LTM                           │
│  MANDATE: SHA-256 hash on every LTM commit                   │
│  MANDATE: Source_spec_id on every LTM entry                  │
└─────────────────────────────────────────────────────────────┘
```

**Kritische Regel:** LTM ist kein Protokoll — es ist eine **kuratierte Wissensbasis**. Jeder unkontrollierte LTM-Write ist eine Halluzinations-Quelle, weil Agenten spekulative Schlüsse als gesicherte Fakten persistieren.

---

# TEIL IV — SPEC-DRIVEN DEVELOPMENT FRAMEWORK

## 4.1 Das Contract-First-Paradigma

Spec-Driven Development inverts die klassische Reihenfolge. Kein Agent schreibt eine Zeile Code, bevor der Vertrag formalisiert ist.

```
KLASSISCH:                    SPEC-DRIVEN:
User Story                    Interface Contract (T-01 E-1)
    ↓                              ↓
Implementation            Micro-Specification (T-01)
    ↓                              ↓
Tests                      Test-Driven Spec (TDS assertions)
    ↓                              ↓
Documentation              Formal Verification (CoVe Gate)
                                   ↓
                           Implementation (bounded by spec)
                                   ↓
                           Automated spec-compliance check
```

**Struktureller Vorteil:** Wenn die Spec das primäre Artefakt ist und Code ephemer wird (Disposable Software), kann jeder Defekt auf eine Spec-Lücke zurückgeführt werden — nicht auf Implementierungskapriolen.

## 4.2 Formale Spezifikationsebenen

### Ebene 1 — BDD-Gherkin (Stakeholder Interface)

Für die Kommunikation mit nicht-technischen Stakeholdern. Begrenzte Präzision, maximale Lesbarkeit.

```gherkin
Feature: Payment Processing
  Scenario: Successful transaction
    Given a valid payment request with amount 100.00 EUR
    When the payment service processes the request
    Then the transaction status SHALL be SUCCESS
    And the ledger balance SHALL decrease by 100.00 EUR
    And a confirmation event SHALL be emitted within 500ms
```

### Ebene 2 — Micro-Specification (Agent Interface)

Maschinenlesbarer Kontrakt für Worker-Agenten. Replaces vage User Stories durch präskriptive Constraints.

```xml
<micro_specification>
  <artifact_id>MS-PAYMENT-TX-001</artifact_id>
  <parent_plan>EP-CHECKOUT-003</parent_plan>

  <contract_frame>
    <pre_conditions>
      <assert>INPUT.amount > 0 AND INPUT.currency IN [EUR, USD, GBP]</assert>
      <assert>INPUT.payment_method.verified == TRUE</assert>
      <assert>SYSTEM.payment_gateway.status == HEALTHY</assert>
    </pre_conditions>
    <post_conditions>
      <assert>OUTPUT.transaction.status == SUCCESS</assert>
      <assert>LEDGER.balance_delta == -INPUT.amount</assert>
      <assert>DOMAIN_EVENT.PaymentConfirmed EMITTED</assert>
    </post_conditions>
    <invariants>
      <assert>LEDGER.total_debits == LEDGER.total_credits AT ALL TIMES</assert>
      <assert>TRANSACTION.idempotency_key UNIQUE GLOBALLY</assert>
    </invariants>
  </contract_frame>

  <quality_gates>
    <nfr id="PERF_1">
      <metric>LATENCY_P95_MS</metric>
      <threshold_operator>&lt;</threshold_operator>
      <value>500</value>
    </nfr>
    <nfr id="SEC_1">
      <metric>IDEMPOTENCY_ENFORCEMENT</metric>
      <requirement>DUPLICATE_REQUESTS_RETURN_ORIGINAL_RESULT</requirement>
    </nfr>
  </quality_gates>
</micro_specification>
```

### Ebene 3 — Lineare Temporale Logik (Formal Verification Layer)

Für kritische Systemgarantien, die über Szenariotests hinausgehen.

```
# Safety Properties (niemals verletzt werden)
□ (payment_initiated → ◇ (payment_committed ∨ payment_failed))
# "Immer gilt: Jede initiierte Zahlung wird schließlich 
#  entweder committed oder als Fehler quittiert"

□ ¬ (payment_committed ∧ ¬ ledger_updated)
# "Niemals ist eine Zahlung committed, ohne dass das Ledger
#  aktualisiert wurde"

# Liveness Properties (werden schließlich eintreten)
◇ (payment_initiated → payment_responded)
# "Jede initiierte Zahlung erhält schließlich eine Antwort"

# Idempotenz-Invariante
□ (request_id = x → response = f(x) FOR ALL TIME)
# "Dieselbe Request-ID liefert immer dasselbe Ergebnis"
```

**Überlegenheit über BDD:** LTL beweist zeitliches Verhalten über den gesamten Ausführungspfad, nicht nur für isolierte Szenarien. Race Conditions und Deadlocks werden auf Spezifikationsebene ausgeschlossen, bevor eine Zeile Code existiert.

## 4.3 Der Spec-Lifecycle

```
┌─────────────────────────────────────────────────────────────────┐
│                       SPEC LIFECYCLE                             │
│                                                                  │
│  DRAFT                                                           │
│  Stakeholder BDD → Architekt Micro-Spec → Formale LTL-Prüfung   │
│  Konsistenzcheck: Widersprüche in Specs? → zurück zu DRAFT       │
│                                                                  │
│  APPROVED                                                        │
│  SHA-256 Hash der Spec-Datei                                     │
│  Versionierung in VCS                                            │
│  Referenzierung in T-04 Orchestration Plan                       │
│                                                                  │
│  ACTIVE                                                          │
│  Agenten laden Spec-Hash bei Task-Start                          │
│  T-05 Verifier prüft gegen Spec-Hash (nicht Impl.)              │
│  Spec-Drift Detection: hash(spec_at_write) ≠ hash(spec_now)     │
│                                                                  │
│  DEPRECATED                                                      │
│  Explizite Deprecation → alle abhängigen Specs notifiziert       │
│  Keine Implementation darf gegen deprecated Spec codieren        │
└─────────────────────────────────────────────────────────────────┘
```

---

# TEIL V — AGENTIC CODING PATTERNS

## 5.1 Die Drei-Rollen-Architektur

Jedes produktive Agenten-System benötigt genau drei Rollen. Überlappungen erzeugen Cross-Context Bleeding.

```
┌────────────────────────────────────────────────────────────────┐
│                  DREI-ROLLEN-ARCHITEKTUR                        │
│                                                                 │
│  ORCHESTRATOR                                                   │
│  Lädt: System Constitution (T-06 root), alle Agent-Manifeste   │
│  Verantwortlich: DAG-Planung (T-04), Task-Dispatch (T-03)       │
│  INHIBIT: Code generieren, direkte Tool-Calls auf Domäne        │
│  MANDATE: Risk Register führen, Eskalationskette verwalten      │
│                                                                 │
│         ↕ ACP Message (T-03) mit trace_id                       │
│                                                                 │
│  WORKER AGENT                                                   │
│  Lädt: Nearest AGENTS.md, relevante Micro-Specs (T-01)          │
│  Verantwortlich: GoT-Knoten ausführen, T-07 Checkpoints         │
│  INHIBIT: Globale Architekturentscheidungen treffen             │
│  MANDATE: Pre/Post-Conditions prüfen, STM führen                │
│                                                                 │
│         ↕ Output-Artefakt mit SHA-256                           │
│                                                                 │
│  VERIFIER AGENT                                                 │
│  Lädt: Original Spec (T-01), Interface Contract — NICHT Output  │
│  Verantwortlich: CoVe 4-Phasen-Gate (T-05)                      │
│  INHIBIT: Autor-Intention lesen, adjacent VQ-Antworten lesen    │
│  MANDATE: PASS/FAIL binär, Confidence-Score, Hash auf Ergebnis  │
└────────────────────────────────────────────────────────────────┘
```

## 5.2 Der Agent-Manifest (T-02) als Identity Contract

Ein Agent ohne explizites Manifest ist ein Agent ohne Grenzen — und damit eine Architektur-Katastrophe.

```xml
<agent_manifest>
  <id>AGENT-BE-AUTH-001</id>
  <role>WORKER</role>
  <domain>backend.authentication</domain>
  <version>1.2.0</version>

  <capabilities>
    <can>IMPLEMENT TypeScript Node.js services</can>
    <can>WRITE PostgreSQL queries via Prisma ORM</can>
    <can>EXECUTE unit and integration tests via Jest</can>
    <can>CALL tool:file_write, tool:bash_execute, tool:test_runner</can>
  </capabilities>

  <constraint_stack>
    <inhibit priority="CRITICAL">
      GENERATE_CODE_WITHOUT_SPEC_REFERENCE
      USE_ANY_TYPESCRIPT_TYPE
      CALL_EXTERNAL_APIS_WITHOUT_INTERFACE_CONTRACT
      COMMIT_TO_LTM_WITH_CONFIDENCE_BELOW_80
    </inhibit>
    <mandate priority="CRITICAL">
      PRE_POST_CONDITION_CHECK_ON_EVERY_NODE
      T07_CHECKPOINT_AFTER_EVERY_TOOL_CALL
      SHA256_HASH_ALL_OUTPUTS
      EXPLICIT_ERROR_HANDLING_NO_SILENT_CATCH
    </mandate>
    <prefer>
      FUNCTIONAL_OVER_CLASS_BASED_COMPONENTS
      EARLY_RETURNS_OVER_NESTED_CONDITIONS
      NAMED_EXPORTS_OVER_DEFAULT_EXPORTS
    </prefer>
  </constraint_stack>

  <context_access>
    <!-- Welche Kontextebenen dieser Agent laden darf -->
    <allowed_layers>
      <layer>global.architectural_axioms</layer>
      <layer>backend.conventions</layer>
      <layer>auth.feature_rules</layer>
    </allowed_layers>
    <denied_layers>
      <layer>frontend.*</layer>
      <layer>devops.infrastructure</layer>
    </denied_layers>
  </context_access>

  <tool_harness>
    <!-- Grounding-by-Execution: Agent verifiziert empirisch -->
    <verify_command>pnpm run typecheck</verify_command>
    <verify_command>pnpm run lint</verify_command>
    <test_command>pnpm run test:unit --coverage</test_command>
    <success_criteria>
      <typecheck_errors>0</typecheck_errors>
      <lint_errors>0</lint_errors>
      <test_coverage_min_pct>85</test_coverage_min_pct>
    </success_criteria>
  </tool_harness>

</agent_manifest>
```

## 5.3 Der Task Orchestration DAG (T-04)

Ein Workflow ist kein linearer Plan — er ist ein gerichteter azyklischer Graph mit expliziten Parallelisierungsebenen und Merge-Strategien.

```xml
<task_orchestration_plan>
  <plan_id>PLAN-CHECKOUT-FEATURE-003</plan_id>
  <trace_id>{{UUID}}</trace_id>

  <task_graph>

    <!-- Layer 0: Sequential (Spec muss zuerst existieren) -->
    <layer id="0" execution="SEQUENTIAL">
      <task id="T0-SPEC">
        <agent_manifest_ref>AGENT-ARCHITECT-001</agent_manifest_ref>
        <spec_ref>T-01: MS-CHECKOUT-SPEC</spec_ref>
        <output_artifact>checkout_spec.xml</output_artifact>
        <retry_policy>
          <max_attempts>2</max_attempts>
          <backoff>LINEAR</backoff>
          <on_exhaustion>ESCALATE_TO_HUMAN</on_exhaustion>
        </retry_policy>
      </task>
    </layer>

    <!-- Layer 1: Parallel (unabhängige Implementierungen) -->
    <layer id="1" execution="PARALLEL">
      <task id="T1-BACKEND">
        <depends_on>T0-SPEC</depends_on>
        <agent_manifest_ref>AGENT-BE-AUTH-001</agent_manifest_ref>
        <retry_policy>
          <max_attempts>3</max_attempts>
          <backoff>EXPONENTIAL</backoff>
          <base_ms>500</base_ms>
        </retry_policy>
      </task>
      <task id="T1-FRONTEND">
        <depends_on>T0-SPEC</depends_on>
        <agent_manifest_ref>AGENT-FE-REACT-001</agent_manifest_ref>
        <retry_policy>
          <max_attempts>3</max_attempts>
          <backoff>EXPONENTIAL</backoff>
          <base_ms>500</base_ms>
        </retry_policy>
      </task>
      <merge_strategy type="UNION">
        <conflict_resolution>ESCALATE_TO_ORCHESTRATOR</conflict_resolution>
      </merge_strategy>
    </layer>

    <!-- Layer 2: Sequential (Integration nach Parallelisierung) -->
    <layer id="2" execution="SEQUENTIAL">
      <task id="T2-INTEGRATION">
        <depends_on>T1-BACKEND, T1-FRONTEND</depends_on>
        <agent_manifest_ref>AGENT-VERIFIER-001</agent_manifest_ref>
        <!-- CoVe Gate: Verifikation vor jedem Merge -->
        <verification_gate>T-05</verification_gate>
      </task>
    </layer>

  </task_graph>

  <risk_register>
    <risk id="R-01">
      <description>Spec ambiguity causing parallel agent divergence</description>
      <probability>MEDIUM</probability>
      <impact>HIGH</impact>
      <mitigation>T-05 verification gate before merge, VOTE merge strategy</mitigation>
    </risk>
  </risk_register>

  <escalation_chain>
    <level_1>ORCHESTRATOR auto-retry with mutation</level_1>
    <level_2>VERIFIER_MODEL independent assessment</level_2>
    <level_3>HUMAN_GATE blocking decision required</level_3>
  </escalation_chain>

</task_orchestration_plan>
```

---

# TEIL VI — SYSTEMS THINKING: Feedback & Emergenz

## 6.1 Systemische Feedback-Schleifen

Ein Agenten-System ist kein linearer Prozess — es ist ein dynamisches System mit verstärkenden und dämpfenden Rückkopplungsschleifen.

### Verstärkende Schleife (R1) — Spec-Qualitätsspirale

```
Gute Spec → Präzise Implementierung → Hohe Verifikations-Confidence
    ↑                                           ↓
LTM-Commit verfeinert Spec-Templates ← Wertvolles LTM-Wissen
```

**Managementstrategie:** Diese Schleife aktiv fördern. Jedes erfolgreiche Artefakt sollte Spec-Templates verbessern.

### Dämpfende Schleife (B1) — Halluzinations-Korrektur

```
Halluzination erkannt → T-05 REJECTED → T-07 Anomaly Flag
    ↑                                           ↓
INHIBIT-Stack erweitert ← Ursachenanalyse → Pattern identifiziert
```

**Managementstrategie:** Bounded Iteration sicherstellen. Diese Schleife darf nicht >4 Zyklen laufen.

### Dämpfende Schleife (B2) — Context Bloat Prevention

```
Context wächst → MNC-Prüfung schlägt an → JIT-Deaktivierung
    ↑                                           ↓
Nur relevanter          STM cleared ← Task complete
Kontext aktiv
```

### Systemische Pathologien & ihre Korrekturen

| Pathologie | Systemisches Symptom | Strukturelle Korrektur |
|:---|:---|:---|
| **Runaway Amplification** | Agent verbessert immer gleichen Output | Bounded Iteration + Confidence Gate |
| **Oscillation** | Agent wechselt zwischen zwei Lösungen | VOTE merge strategy + human gate |
| **Stagnation** | Agent findet keine Verbesserung mehr | ESCALATE nach max_attempts |
| **Context Avalanche** | Kontext wächst unkontrolliert | STM FIFO-Eviction + JIT loading |
| **Spec Drift** | Specs und Code divergieren | SHA-256 hash matching |

## 6.2 Emergenz-Kontrolle in Multi-Agenten-Systemen

Emergente Fehler entstehen, wenn die Interaktion zwischen Agenten unerwartete Systemzustände produziert, die kein einzelner Agent verursacht hätte.

```
EMERGENZ-KONTROLLMECHANISMEN

1. Transaktionale ACP Messages (T-03)
   Jede Agent-Kommunikation trägt trace_id + timestamp + hash.
   Kein Zustandsübergang ohne explizites ACP-Paket.
   Vollständige Audit-Trail ermöglicht Emergenz-Ursachenverfolgung.

2. Idempotenz-Enforcement
   Alle Operationen müssen bei identischer Request-ID dasselbe
   Ergebnis liefern. Verhindert inkonsistente Systemzustände
   durch Retry-Kaskaden.

3. Distributed State Isolation
   Kein Agent modifiziert gemeinsamen State direkt.
   State-Deltas werden transaktional via Orchestrator koordiniert.
   Optimistic Locking mit conflict resolution via VOTE.

4. Invarianten-Monitoring (T-07)
   Globale Systemeinvarianten werden nach JEDEM Tool-Call geprüft.
   Eine Invariantenverletzung stoppt ALLE abhängigen Agenten sofort.
   "Fail Fast" verhindert Fehler-Propagation im DAG.

5. Human Gate als finale Schranke
   Jede Eskalationskette hat als letzten Knoten ein Human Gate.
   Kein autonomes System darf eine Eskalation über Level 2 hinaus
   autonom auflösen.
```

## 6.3 Systemisches Qualitätsmodell

```
┌─────────────────────────────────────────────────────────────────┐
│              SYSTEMISCHES QUALITÄTSMODELL                        │
│                                                                  │
│  INPUT QUALITÄT                                                  │
│  • Spec-Präzision (LTL > BDD > Prosa)                           │
│  • Context-Relevanz (MNC Ratio)                                  │
│  • Constraint-Vollständigkeit (INHIBIT + MANDATE Coverage)       │
│                  ↓                                               │
│  PROZESS-INTEGRITÄT                                              │
│  • PDCA-Disziplin (Kein Skip von CHECK-Phasen)                   │
│  • Bounded Execution (max_attempts respected)                    │
│  • Design-by-Contract (Pre/Post/Invariant)                       │
│                  ↓                                               │
│  OUTPUT QUALITÄT                                                 │
│  • Verifikations-Confidence (≥ 85 für Auto-Approve)              │
│  • Spec-Compliance-Rate (Anzahl PASS / VQs total)                │
│  • Iterations-Effizienz (Gute Outputs in ≤ 2 Zyklen)            │
│                  ↓                                               │
│  SYSTEMEVOLUTION                                                 │
│  • LTM-Qualität (nur verified artifacts committet)               │
│  • Spec-Template-Reife (verstärkende R1-Schleife)                │
│  • INHIBIT-Stack-Vollständigkeit (aus Fehlerhistorie lernen)     │
└─────────────────────────────────────────────────────────────────┘
```

---

# TEIL VII — MASTER EXECUTION FLOW & TEMPLATE MAP

## 7.1 Vollständiger System-Execution Flow

```
═══════════════════════════════════════════════════════════════════
                    MASTER EXECUTION FLOW v2.0
═══════════════════════════════════════════════════════════════════

PROJECT BOOTSTRAP
─────────────────
T-06 lexicon.md (Context API)
  → T-02 Agent Manifests (Identity Contracts)
    → T-01 Interface Contracts (Spec-First)
      → System Constitution validated
        → Agent Manifests loaded per domain

FEATURE CYCLE (pro Epic)
─────────────────────────
T-04 Orchestration Plan (DAG)
  │
  ├── OODA Loop: Orchestrator
  │     OBSERVE: T-08 LTM state, T-07 pending checkpoints
  │     ORIENT:  T-06 MNC injection, nearest AGENTS.md
  │     DECIDE:  T-02 Agent Manifest match, risk register
  │     ACT:     T-03 ACP dispatch, trace_id assigned
  │
  ├── PDCA Loop: Worker Agent (per DAG node)
  │     PLAN:   T-01 spec load, T-08 STM init, pre-conditions
  │     DO:     GoT node execution, tool-call
  │     CHECK:  T-07 metacognitive checkpoint
  │     ACT:    CONTINUE | RETRY(mutated) | ESCALATE
  │
  ├── CoVe Gate: Verifier Agent (pre-merge)
  │     Phase 1: Baseline output available
  │     Phase 2: VQs from spec (NOT from output)
  │     Phase 3: Independent binary answers
  │     Phase 4: Confidence gate → APPROVED | REJECTED | ESCALATED
  │
  └── T-08 LTM Commit: nur bei confidence ≥ 80, verified artifact

ERROR / ANOMALY PATH
─────────────────────
T-07 anomaly detected
  → T-03 INCIDENT message (trace_id, severity)
    → T-04 escalation_chain
      → Level 1: Orchestrator auto-retry with mutation
      → Level 2: Verifier Model independent assessment  
      → Level 3: HUMAN GATE — blocking, no autonomous resolution

═══════════════════════════════════════════════════════════════════
```

## 7.2 Complete Template Map v2.0

```
═══════════════════════════════════════════════════════════════════
                    TEMPLATE SYSTEM v2.0
═══════════════════════════════════════════════════════════════════

SPECIFICATION LAYER
  T-01  Micro-Specification         Atomic SRP-enforced module spec
  T-01+ Contract Frame              Pre/Post/Invariant Design-by-Contract
  T-01+ Logic Graph (GoT)           Deterministic execution nodes
  T-01+ Error Matrix                Error class → deterministic agent action
  T-01+ NFR Contract                Measurable performance gates
  T-01+ TDS Assertions              Test-Driven Specification (Given/When/Then)
  LTL   Formal Verification         Temporal safety + liveness properties

CONTEXT LAYER  
  T-06  Context Hierarchy           Cascade, lexicon, JIT loading
  T-06+ Nearest-File Principle      Specificity wins, MNC injection
  T-06+ Constraint Stack            INHIBIT / MANDATE / PREFER separation
  T-06+ Architectural Axioms        Global invariants, tech-stack constitution

AGENT IDENTITY LAYER
  T-02  Agent Manifest              Identity, capabilities, hard rules
  T-02+ INHIBIT Stack               Negative constraint activation vectors
  T-02+ Context Access Rules        Layer-level access control
  T-02+ Tool Harness                Grounding-by-Execution commands

COMMUNICATION LAYER
  T-03  ACP Message                 RACE+ format, CoD trace, trace_id
  T-03+ ACP Envelope                Priority, hash, requires_ack
  T-04  Task Orchestration DAG      Parallel layers, state machine
  T-04+ Retry + Merge               Node-level resilience + merge strategies
  T-04+ Risk Register               Probability/Impact/Mitigation
  T-04+ Escalation Chain            3-level human gate

VERIFICATION LAYER
  T-05  CoVe Verification Contract  4-phase gate, binary VQ answers
  T-05+ Confidence Gate             85/60 thresholds, auto-approve logic
  T-05+ Bounded Iteration           max_attempts, bias guard
  T-05+ Output Integrity Envelope   SHA-256, spec_version_ref

RUNTIME LAYER
  T-07  Metacognitive Checkpoint    Per-node PDCA, contract check, anomaly
  T-07+ Strategy Adaptation         Mutation-enforced retries
  T-08  Memory Architecture         STM/LTM separation, commit rules
  T-08+ LTM Commit Rules            Confidence threshold, spec-anchoring

═══════════════════════════════════════════════════════════════════
```

## 7.3 Design-Prinzipien Kreuzreferenz

| Prinzip | Quelle | Implementiert in |
|:---|:---|:---|
| Nearest-File / MNC Injection | Template-AGENTS.md §1, §3-4 | T-06 Cascade, JIT loading |
| INHIBIT / MANDATE Constraint Stack | Template-AGENTS.md §3 | T-06, T-02, alle Manifeste |
| Architectural Axioms / Constitution | TEMPLATES.md §1 | T-06 Root, System Constitution |
| Design-by-Contract (Pre/Post/Inv) | Teil IV dieser Spec | T-01 Contract Frame, T-07 |
| Graph-of-Thoughts Nodes | TEMPLATES.md §2 | T-01 Logic Graph |
| Deterministic Error Matrix | TEMPLATES.md §3 | T-01 Error Matrix, T-04 |
| NFR als messbare Variablen | TEMPLATES.md §2 | T-01 NFR Contract |
| Metacognitive PDCA Checkpoint | TEMPLATES.md §5 | T-07 |
| STM / LTM Separation | Teil III §3.4 | T-08 |
| ACP trace_id / Integrity Hash | TEMPLATES.md §4 | T-03 Envelope |
| Exponential Backoff per Node | Teil III §3.3 | T-04 Retry Policy |
| Bounded Iteration + Bias Guard | Teil II §2.3 | T-05 Confidence Gate |
| Tool-Harnessing / PDCA Grounding | Template-AGENTS.md §5 | T-06 Tool Harness, T-07 |
| Confidence Threshold Gates | Teil II §2.3 | T-05 Phase 4 Verdict |
| SHA-256 Output Integrity | Übergreifend | T-03, T-05, T-07, T-08 |
| Cross-Context Bleeding Prevention | Template-AGENTS.md §1, §3 | T-06 Layer Scoping, T-02 |
| LTL Formal Verification | Teil IV §4.2 | Spec Layer, vor Implementation |
| OODA Orchestration Loop | Teil II §2.1 | Orchestrator Execution Model |
| Drei-Rollen-Architektur | Teil V §5.1 | Agent Manifest Role Separation |
| Emergenz-Kontrolle | Teil VI §6.2 | ACP Idempotenz, Distributed Isolation |

---

## ANHANG A — Verzeichnisstruktur-Referenz

```
/ (Root)
├── AGENTS.md                      ← Global Architecture Constitution
├── .ai-context/
│   ├── lexicon.md                 ← Context API (T-06 Master Index)
│   ├── rules.md                   ← Metadata-driven priorities
│   ├── security.md                ← Security constraints (INHIBIT)
│   ├── patterns.md                ← Reusable code patterns
│   └── error-playbook.md          ← Error Matrix cross-reference
├── .agents/
│   ├── orchestrator-manifest.xml  ← T-02 Orchestrator
│   ├── worker-be-manifest.xml     ← T-02 Backend Worker
│   ├── worker-fe-manifest.xml     ← T-02 Frontend Worker
│   └── verifier-manifest.xml      ← T-02 Verifier
├── specs/
│   ├── contracts/                 ← T-01 Interface Contracts
│   ├── micro-specs/               ← T-01 Micro Specifications
│   ├── ltl/                       ← Formal LTL Specifications
│   └── bdd/                       ← Gherkin Stakeholder Specs
└── packages/
    ├── api/
    │   ├── AGENTS.md              ← Backend-specific constraints
    │   └── .ai-context/
    │       └── db-schema.md       ← JIT-loaded DB context
    └── web/
        ├── AGENTS.md              ← Frontend-specific constraints
        └── .ai-context/
            └── component-library.md ← JIT-loaded UI context
```

---

## ANHANG B — Entscheidungsbaum: Welches Muster wann?

```
AUFGABE EMPFANGEN
      │
      ▼
  Scope definiert?
  ├── NEIN → T-06 Context load → T-01 Spec-first
  └── JA
        │
        ▼
    Einzelner Agent oder Multi-Agent?
    ├── SINGLE → PDCA Loop direkt (T-07 Checkpoints)
    └── MULTI
          │
          ▼
      Abhängigkeiten zwischen Tasks?
      ├── NEIN (unabhängig) → T-04 Parallel Layer
      └── JA
            │
            ▼
          Reihenfolge deterministisch?
          ├── JA → T-04 Sequential Layers mit Dependencies
          └── BEDINGT → T-04 DAG mit State Machine Gates
                │
                ▼
            Konfliktrisiko bei Merge?
            ├── NIEDRIG → UNION merge
            ├── MITTEL → SYNTHESIZE merge
            └── HOCH → VOTE merge + CoVe Gate (T-05)
```

---

_STRUCTURED PROCESS ORCHESTRATION SPEC v2.0_  
_Synthese aus: Template-AGENTS.md · TEMPLATES.md · SOTA Agentic Research 2024–2025_  
_Revision Trigger: LTL-to-XML Compiler Integration · GoT Complexity Classifier · LTM Backend Spec_
