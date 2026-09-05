# STRUCTURED PROCESS ORCHESTRATION — MASTER FRAMEWORK
> **Authority:** Senior AI Systems Architect | Google Agentic Platform Division  
> **Version:** 2026.1 | **Classification:** PRODUCTION-GRADE  
> **Paradigm:** Process Verification > Output Validation | Spec = Source of Truth  
> **Encoding:** Token-minimal · XML-anchored · English-only · LLM-native

---

## FRAMEWORK INDEX

| ID | Component | Scope | Core Problem Solved |
|:--|:--|:--|:--|
| **SPO-00** | Design Axioms | Meta | Why this framework exists |
| **SPO-01** | MACRO LOOP CATALOG | System | 8 fundamental control loops |
| **SPO-02** | VALIDATION STACK | Quality | 5-tier verification taxonomy |
| **SPO-03** | CONTEXT ENGINEERING | Memory | Lost-in-Middle + MNC patterns |
| **SPO-04** | SYNTAX ENGINEERING | Signal | Token-optimal delimiter hierarchy |
| **SPO-05** | SPEC-DRIVEN DEVELOPMENT | Spec | Ambiguity-free task contracts |
| **SPO-06** | FAILURE TAXONOMY (MAST) | Resilience | 14 failure modes + compensations |
| **SPO-07** | ANTI-HALLUCINATION PROTOCOL | Grounding | CoVe + ReAct integrated spec |
| **SPO-08** | SYSTEM PROMPT TEMPLATE | Bootstrap | Master agent constitution |
| **SPO-09** | ORCHESTRATION MANIFEST | Deploy | Full multi-agent coordination spec |

---
---

# SPO-00 · DESIGN AXIOMS

> These axioms override all local heuristics. Internalize before reading further.

```
AX-01: PROCESS OVER OUTPUT
       Validate the reasoning chain, not just the final artifact.
       A correct result from wrong reasoning is a latent defect.

AX-02: SPEC IS LAW
       Natural language → ambiguous inference → hallucination.
       Formal spec → logical deduction → deterministic output.
       The spec is the source of truth. Code is disposable.

AX-03: MINIMAL NECESSARY CONTEXT (MNC)
       LLMs use ~10-20% of their context window effectively.
       Inject only what is needed, when it is needed (JIT).
       Every irrelevant token reduces signal density.

AX-04: FAIL FAST, RECOVER LOCALLY
       Validation must be embedded in every node, not appended at the end.
       A system that fails gracefully at step N is better than one
       that silently propagates errors to step N+7.

AX-05: DETERMINISM BY CONSTRAINT
       Do not ask the agent to "write good code."
       Constrain the solution space so only correct behavior is possible.
       Positive constraints (DO) + Negative constraints (NEVER) + Examples (PREFER/AVOID).

AX-06: LOOPS OVER PIPELINES
       Static pipelines fail on edge cases.
       PDCA loops + conditional edges + retry gates make workflows antifragile.

AX-07: GROUNDING BY EXECUTION
       Assertions verified by running code > assertions verified by reasoning.
       Exit code 0 is ground truth. LLM confidence is noise.
```

---
---

# SPO-01 · MACRO LOOP CATALOG

> Eight fundamental control loops. Each solves a distinct failure mode.
> Combine loops via nesting. Never run a pipeline without a loop.

---

## LOOP-01 · PDCA — Universal Execution Loop

> **Origin:** Deming Cycle | **Scope:** Every atomic task node  
> **Solves:** Unverified execution, silent failures, no self-correction

```
<PDCA_LOOP id="{{LOOP_ID}}" node="{{NODE_ID}}">

  PHASE_1_PLAN:
    DECOMPOSE  task from {{MS_ID}} into ≤5 atomic sub-steps.
    IDENTIFY   required tools from {{TOOL_REGISTRY}}.
    LOAD       context JIT — only refs in <CONTEXT_REFS> of spec.
    EMIT       plan to scratchpad before executing.

  PHASE_2_DO:
    EXECUTE    tool calls with schema-compliant JSON parameters.
    WRITE      partial results to blackboard after each sub-step.
    NEVER      assume — if input ambiguous → emit BLOCKED with specific question.
    NEVER      skip pre-condition checks.

  PHASE_3_CHECK:
    OBSERVE:
      stdout:    "{{TOOL_STDOUT}}"
      stderr:    "{{TOOL_STDERR}}"
      exit_code: {{EXIT_CODE}}          # Ground truth
    VERIFY:
      - RUN CoVe (Chain-of-Verification) against {{MS_ID}} post-conditions.
      - RUN {{TYPECHECK_CMD}} · {{LINT_CMD}} · {{TEST_CMD}}.
      - CONFIRM all self-audit checklist items GREEN.

  PHASE_4_ACT:
    IF   (exit_code == 0 AND cove_pass == true):
         → EMIT {status: "DONE"} · TERMINATE node
    ELIF (retry_count < {{MAX_RETRIES}}):
         → ANALYZE root cause (3 hypotheses max)
         → RECALIBRATE parameters
         → INCREMENT retry_count
         → GOTO PHASE_2_DO
    ELSE:
         → EMIT {status: "BLOCKED", cause: "{{ROOT_CAUSE}}"}
         → HALT · ESCALATE to {{ORCHESTRATOR_ID}}

</PDCA_LOOP>
```

---

## LOOP-02 · TDFlow — Test-Driven Agentic Workflow

> **Origin:** TDFlow (2025) | **Scope:** Code generation tasks  
> **Solves:** Code without verifiable ground truth, "hallucinated correctness"

```
<TDFLOW_LOOP id="{{LOOP_ID}}" spec_ref="{{MS_ID}}">

  ## Precondition: test suite {{TEST_SUITE_REF}} must exist BEFORE coding begins.
  ## If absent: HALT. Request test creation first.

  PHASE_PROPOSE:
    READ       {{MS_ID}} pre/post-conditions and invariants.
    GENERATE   minimal code patch that satisfies post-conditions.
    CONSTRAIN  output to schema {{OUTPUT_SCHEMA_REF}}.
    WRITE      patch to {{WORKSPACE_PATH}}.

  PHASE_DEBUG:
    EXECUTE    {{TEST_CMD}} against patch.
    PARSE      test output: {passed: bool, failures: [{test_id, reason}]}.
    IF (passed == true AND coverage >= {{MIN_COVERAGE_PCT}}%):
       → GOTO PHASE_ACCEPT
    ELSE:
       → COLLECT failure reasons as structured input for next iteration.

  PHASE_REVISE:
    ANALYZE    failure reasons without accessing prior patch.
    GENERATE   revised patch targeting ONLY failing test cases.
    NEVER      regress passing tests (run full suite each iteration).
    INCREMENT  revision_count.
    IF (revision_count >= {{MAX_REVISIONS}}):
       → EMIT {status: "BLOCKED", unresolved_failures: [...]}
       → HALT

  PHASE_ACCEPT:
    RUN        {{TYPECHECK_CMD}} · {{LINT_CMD}}.
    VERIFY     no `{{BANNED_TYPES}}` introduced.
    EMIT       {status: "DONE", patch_ref: "{{PATCH_ID}}", test_pass_rate: "{{N}}%"}

</TDFLOW_LOOP>
```

---

## LOOP-03 · CoVe — Chain-of-Verification Anti-Hallucination Loop

> **Origin:** Meta Research (2023), production standard 2025  
> **Scope:** Any response containing factual claims or code assertions  
> **Solves:** Hallucination hidden inside plausible CoT chains

```
<COVE_LOOP id="{{LOOP_ID}}">

  STEP_1_BASELINE:
    GENERATE   initial response to {{TASK_INPUT}}.
    LABEL      every factual claim with [CLAIM_N].
    DO NOT     verify yet — capture the raw baseline.

  STEP_2_PLAN_VERIFICATION:
    FOR EACH   [CLAIM_N] in baseline response:
      FORMULATE verification question Q_N that is:
        - Independent of the baseline (do not reference it)
        - Answerable by tool call, code execution, or schema lookup
        - Binary where possible (YES/NO or PASS/FAIL)
    OUTPUT     verification plan: [{claim_id, question, method}]

  STEP_3_INDEPENDENT_VERIFICATION:
    ## CRITICAL: answer each Q_N WITHOUT re-reading the baseline.
    ## Re-reading introduces confirmation bias.
    FOR EACH   Q_N:
      EXECUTE  verification method (tool call / code run / schema check).
      RECORD   result: {Q_N: {answer, evidence_ref, confidence: HIGH|MED|LOW}}

  STEP_4_SYNTHESIZE:
    COMPARE    baseline claims against verification results.
    FOR EACH   DISCREPANCY:
      CORRECT  the claim in the final response.
      ANNOTATE with evidence_ref.
    IF (confidence == LOW on any claim):
      EMIT     explicit uncertainty marker [UNCERTAIN: reason].
    OUTPUT     verified_response with all corrections applied.

</COVE_LOOP>
```

---

## LOOP-04 · ReAct — Reason-Act-Observe Grounding Loop

> **Origin:** Yao et al. (2022), SOTA deployment 2025  
> **Scope:** Tasks requiring external facts, tool use, or environment feedback  
> **Solves:** Reasoning divorced from reality; "analysis paralysis"; drifting plans

```
<REACT_LOOP id="{{LOOP_ID}}" max_cycles="{{MAX_CYCLES}}">

  ## Principle: Thought precedes every Action. Observation updates every Thought.
  ## Never chain more than 1 Action per cycle without an Observation.

  CYCLE:
    THOUGHT:
      ANALYZE  current state against {{MS_ID}} goal.
      IDENTIFY single next action (not a plan — one action).
      REASON:  "I need to [ACTION] because [EVIDENCE_FROM_OBSERVATION]."

    ACTION:
      SELECT   tool from {{TOOL_REGISTRY}} matching required capability.
      CALL     tool with minimal required parameters.
      DO NOT   infer results — wait for actual observation.

    OBSERVATION:
      PARSE    tool output: {stdout, stderr, exit_code, result_data}.
      UPDATE   internal state model.
      ASK:     "Does this observation confirm or refute my last Thought?"

    DECISION:
      IF   (goal_state_reached):      → TERMINATE · emit result
      ELIF (observation_error):       → ADAPT thought · RETRY with correction
      ELIF (cycle_count >= MAX):      → EMIT BLOCKED · ESCALATE
      ELSE:                           → NEXT CYCLE

  ## Anti-pattern guard: If 3 consecutive Thoughts are identical → force OBSERVATION
  ## This breaks "Overthinking" / Reasoning-Action Dilemma traps.

</REACT_LOOP>
```

---

## LOOP-05 · EXECUTOR-VERIFIER — Parallel Candidate Validation Loop

> **Origin:** E-Agent architecture (2025)  
> **Scope:** High-stakes tasks requiring correctness guarantees  
> **Solves:** Single-agent blind spots; "right answer, wrong reasoning"

```
<EXECUTOR_VERIFIER_LOOP id="{{LOOP_ID}}" spec_ref="{{MS_ID}}">

  PHASE_GENERATE:
    SPAWN      {{N_EXECUTORS}} parallel executor agents (N = 3 recommended).
    EACH       executor generates independent candidate solution.
    CONSTRAINT: executors cannot communicate with each other.
    COLLECT    candidates: [C1, C2, ..., CN]

  PHASE_VERIFY:
    ASSIGN     verifier agent (higher capability model than executors).
    VERIFIER   evaluates EACH candidate against:
      - Functional correctness (run {{TEST_SUITE_REF}})
      - Reasoning chain validity (CoVe against pre/post-conditions)
      - Schema compliance (validate against {{OUTPUT_SCHEMA_REF}})
      - LTL safety properties (□ safety invariants from {{MS_ID}})
    OUTPUT     scored_candidates: [{candidate_id, score, reasoning_audit}]

  PHASE_SELECT:
    IF   (top_score >= {{ACCEPTANCE_THRESHOLD}}):
         → SELECT top_candidate · emit as final result
    ELIF (multiple_candidates_tied):
         → MERGE non-conflicting elements · flag conflicts for HiTL
    ELSE:
         → EMIT {status: "REJECTED", max_score_achieved: N}
         → TRIGGER TDFLOW_LOOP for revision

</EXECUTOR_VERIFIER_LOOP>
```

---

## LOOP-06 · CORRECTIVE RAG — Self-Reflective Retrieval Loop

> **Origin:** CRAG / Self-RAG (2024), LangGraph Validation Nodes  
> **Scope:** Any RAG pipeline or knowledge-grounded task  
> **Solves:** Retrieval noise, irrelevant context injection, faithfulness failures

```
<CORRECTIVE_RAG_LOOP id="{{LOOP_ID}}">

  STEP_RETRIEVE:
    QUERY      {{KNOWLEDGE_SOURCE}} with reformulated query from {{TASK_INPUT}}.
    COLLECT    top-K candidates: [D1..DK]

  STEP_GRADE:
    ## Validation Node — grade EACH retrieved document
    FOR EACH   Di:
      ASSESS:  relevance_score (0.0–1.0) against task requirements
      CLASSIFY: RELEVANT | AMBIGUOUS | IRRELEVANT
    PARTITION: relevant_docs · ambiguous_docs · irrelevant_docs

  STEP_ROUTE:
    ## Conditional Edge — branching logic
    IF   (len(relevant_docs) >= {{MIN_RELEVANT_DOCS}}):
         → PROCEED to GENERATE with relevant_docs only
    ELIF (len(relevant_docs) < {{MIN_RELEVANT_DOCS}} AND web_search_allowed):
         → REWRITE query using step-back abstraction
         → EXECUTE web search · APPEND new docs
         → RETURN to STEP_GRADE
    ELIF (all_docs_irrelevant):
         → EMIT {status: "GROUNDING_FAILED"}
         → DO NOT hallucinate — return "Insufficient grounding data"
    ELSE:
         → USE ambiguous_docs with explicit uncertainty annotation

  STEP_GENERATE:
    SYNTHESIZE response STRICTLY from relevant_docs.
    ANNOTATE   every claim with [SOURCE: doc_id, chunk_id].
    APPLY      COVE_LOOP for final verification.

</CORRECTIVE_RAG_LOOP>
```

---

## LOOP-07 · SAGA — Distributed Transaction Compensation Loop

> **Origin:** Garcia-Molina & Salem (1987), MAS adoption 2024  
> **Scope:** Multi-node workflows where partial failure requires rollback  
> **Solves:** Cascading failures, inconsistent system state after partial execution

```
<SAGA_LOOP id="{{LOOP_ID}}" ep_ref="{{EP_ID}}">

  ## Each node N has a corresponding compensating transaction T_N^(-1).
  ## On failure at node N: execute T_N^(-1), T_(N-1)^(-1), ... T_1^(-1) in order.

  COMPENSATION_MAP:
    - node: "N-01"  forward: "{{ACTION_01}}"  compensate: "{{ROLLBACK_01}}"
    - node: "N-02"  forward: "{{ACTION_02}}"  compensate: "{{ROLLBACK_02}}"
    - node: "N-03"  forward: "{{ACTION_03}}"  compensate: "{{ROLLBACK_03}}"

  EXECUTION:
    FOR EACH node in forward_order:
      EXECUTE forward action.
      IF (success): CHECKPOINT state to {{CHECKPOINT_STORE}}.
      IF (failure):
        IDENTIFY last successful checkpoint.
        EXECUTE compensation chain in reverse from failed node.
        EMIT {status: "COMPENSATED", reverted_to: "{{LAST_GOOD_CHECKPOINT}}"}
        NOTIFY {{ORCHESTRATOR_ID}} with compensation report.

  IDEMPOTENCY_RULE:
    EVERY forward action MUST be idempotent.
    Use {{IDEMPOTENCY_KEY}} = SHA-256(node_id + task_input_hash).
    Duplicate execution MUST produce same result — not duplicated side effects.

</SAGA_LOOP>
```

---

## LOOP-08 · RED-BLUE — Adversarial Hardening Loop

> **Origin:** AutoGen Red-Teaming, Google SafeSearch (2025)  
> **Scope:** System-level validation before production deployment  
> **Solves:** Blind spots in Blue team design; emergent failure modes; injection attacks

```
<RED_BLUE_LOOP id="{{LOOP_ID}}" system_ref="{{SYSTEM_ID}}">

  BLUE_TEAM:
    ROLE:       Productive system agents executing normal workflows.
    TARGET:     Achieve {{SYSTEM_OBJECTIVES}} reliably.
    DEFENSE:    Implement all guardrails from {{CONSTRAINT_STACK}}.

  RED_TEAM:
    ROLE:       Adversarial agents finding failure modes.
    MANDATE:    SYSTEMATICALLY attack Blue team via:
      - Prompt injection (malicious tool outputs)
      - Disinformation injection (false context in RAG retrieval)
      - Boundary violations (cross-module constraint probing)
      - State corruption (invalid checkpoint injection)
      - Overthinking inducement (recursive reasoning traps)

  CYCLE:
    RED_ATTACK:   Execute adversarial scenario against Blue team.
    OBSERVE:      Record Blue team behavior and any failures.
    CLASSIFY:     Map failure to MAST taxonomy (see SPO-06).
    REPORT:       {attack_vector, failure_mode, mast_category, severity}

  HARDENING:
    FOR EACH identified_failure:
      PATCH      constraint stack or validation node.
      RE-RUN     attack vector to confirm patch effectiveness.
      DOCUMENT   in {{ADR_REGISTRY}} as security ADR.

  ACCEPTANCE_GATE:
    SYSTEM enters production ONLY when:
      - Zero P0 failures in last {{N}} red cycles.
      - All P1 failures have confirmed patches.
      - MAST coverage ≥ {{MIN_MAST_COVERAGE_PCT}}%.

</RED_BLUE_LOOP>
```

---
---

# SPO-02 · VALIDATION STACK — 5-TIER TAXONOMY

> **Source:** MAST research + Agents_1 synthesis  
> **Principle:** Embed validation at every tier. Post-hoc testing is insufficient.

```
<VALIDATION_STACK system_ref="{{SYSTEM_ID}}">

  ## TIER 1: OUTPUT VALIDATION — Atomic Correctness
  ##         What: Does the final artifact match the spec?
  ##         Method: TDFlow (LOOP-02) + LLM-as-Judge
  T1_OUTPUT:
    TRIGGER:   After every PHASE_2_DO in PDCA loop.
    METHOD:    Run {{TEST_SUITE_REF}} → assert exit_code == 0.
    JUDGE:     If functional tests pass but quality uncertain →
               INVOKE LLM-as-Judge with rubric {{JUDGE_RUBRIC_REF}}.
    THRESHOLD: Pass rate ≥ {{T1_PASS_THRESHOLD}}%.

  ## TIER 2: REASONING VALIDATION — Cognitive Integrity
  ##         What: Is the reasoning chain that produced the output valid?
  ##         Method: CoVe (LOOP-03) + Executor-Verifier (LOOP-05)
  T2_REASONING:
    TRIGGER:   After any response containing [CLAIM] annotations.
    METHOD:    CoVe — verify each claim independently.
    GUARD:     Detect "Reasoning-Action Dilemma":
               IF (3 identical consecutive Thoughts) → force ACTION.
    THRESHOLD: All HIGH confidence claims verified. LOW claims annotated [UNCERTAIN].

  ## TIER 3: WORKFLOW VALIDATION — Stateful Integrity
  ##         What: Are state transitions valid? Are loops terminating?
  ##         Method: Validation Nodes + Conditional Edges in DAG
  T3_WORKFLOW:
    TRIGGER:   At every DAG edge (transition between nodes).
    VALIDATION_NODE:
      ASSESS   current state against {{EXPECTED_STATE_SCHEMA}}.
      ROUTE:   valid → next node | invalid → error_handling_edge.
    LOOP_GUARD:
      MAX iterations per loop: {{MAX_LOOP_ITERATIONS}}.
      Detect infinite loops: flag if state unchanged after 3 iterations.
    STATE_MONITOR:
      Watch {{BLACKBOARD_STATE_PATH}} for invalid transitions.
      Alert on: unexpected nulls · schema violations · dead state.

  ## TIER 4: ECOSYSTEM VALIDATION — Emergent Behavior
  ##         What: Does the multi-agent system behave correctly as a whole?
  ##         Method: Red-Blue Loop (LOOP-08) + MAST taxonomy (SPO-06)
  T4_ECOSYSTEM:
    TRIGGER:   Pre-deployment + after major component changes.
    METHOD:    Run RED_BLUE_LOOP for {{N_RED_CYCLES}} cycles.
    MAST_SCAN: Classify all observed failures against 14 MAST modes.
    ACCEPTANCE: No P0 failures · all P1 failures patched · MAST coverage ≥ {{PCT}}%.

  ## TIER 5: DETERMINISTIC VERIFICATION — Provable Correctness
  ##         What: Can we mathematically prove the system satisfies its spec?
  ##         Method: LTL properties + AgentRR record/replay
  T5_FORMAL:
    TRIGGER:   For safety-critical paths only ({{CRITICAL_PATH_LIST}}).
    LTL_CHECK:
      @SAFETY:   □ (pre_conditions → output_schema_valid)
      @SAFETY:   □ ¬(unauthorized_action)
      @LIVENESS: ◇ (request → response ∨ error)
    AGENTRRR:
      RECORD:   Full interaction trajectory (inputs · outputs · tool calls · state).
      REPLAY:   In deterministic env to reproduce any failure deterministically.
      STORE:    trajectory_ref in {{AUDIT_LOG_PATH}}.

</VALIDATION_STACK>
```

---
---

# SPO-03 · CONTEXT ENGINEERING PATTERNS

> **Source:** Context_1, Context_2, Context_3 synthesis  
> **Core problem:** LLMs use only 10-20% of context effectively. Every token is a cost.

---

## CE-01 · MNC INJECTION TEMPLATE — Minimal Necessary Context

```
<CONTEXT_INJECTION_SPEC agent="{{AGENT_ID}}" task="{{MS_ID}}">

  ## RULE: Never load more than what is needed for this exact task.
  ## RULE: Load context JIT (just-in-time), not at session start.
  ## RULE: Prefer @-references over inline content.

  ALWAYS_LOAD:              # Loaded at every call — keep ≤ 500 tokens
    - Task spec:      @specs/{{MS_ID}}.md
    - Module rules:   @packages/{{MODULE_NAME}}/AGENTS.md
    - Constraint ref: @.ai-context/constraints.md#{{RELEVANT_SECTION}}

  LOAD_ON_DEMAND:           # Load ONLY when explicitly referenced in task
    - Architecture:   @docs/ADR-{{ADR_ID}}.md
    - Schema:         @schemas/{{SCHEMA_REF}}.json
    - Pattern:        @.ai-context/patterns.md#{{PATTERN_NAME}}
    - Test examples:  @tests/{{TEST_REF}}.spec.ts

  NEVER_LOAD:               # These cause context pollution
    - Full repository index
    - Unrelated module AGENTS.md files
    - Raw database dumps
    - Dependency lock files
    - Build artifacts

  POSITION_RULES:
    CRITICAL_INSTRUCTIONS: position=TOP    # Primacy effect — highest attention
    CRITICAL_DATA:         position=BOTTOM # Recency effect — second highest
    BACKGROUND_CONTEXT:    position=MIDDLE # Accepted lower attention

</CONTEXT_INJECTION_SPEC>
```

---

## CE-02 · SCRATCHPAD MANAGEMENT — Multi-Turn State Preservation

```
<SCRATCHPAD_PROTOCOL agent="{{AGENT_ID}}">

  ## Purpose: Preserve reasoning state across tool calls within a single task.
  ## Mechanism: Append-only internal notepad. Never overwrite — always append.

  STRUCTURE:
    scratchpad:
      task_id:      "{{MS_ID}}"
      goal:         "{{ONE_LINE_GOAL}}"
      hypothesis:   "{{CURRENT_WORKING_HYPOTHESIS}}"
      completed:    ["{{STEP_1}}", "{{STEP_2}}"]
      pending:      ["{{STEP_3}}", "{{STEP_4}}"]
      observations: ["{{OBS_1}}: {{RESULT_1}}", "{{OBS_2}}: {{RESULT_2}}"]
      blockers:     ["{{BLOCKER_1}}"]
      revision:     {{N}}

  RULES:
    - UPDATE scratchpad BEFORE executing each sub-step.
    - READ scratchpad BEFORE generating each Thought in ReAct loop.
    - NEVER trust memory — always read from scratchpad for current state.
    - RESET scratchpad only when task MS_ID changes.

</SCRATCHPAD_PROTOCOL>
```

---

## CE-03 · LOST-IN-MIDDLE MITIGATION

```
<LOST_IN_MIDDLE_GUARD>

  ## Research finding: ~10-20% effective context use in complex reasoning.
  ## Information in the middle of long contexts is systematically ignored.

  STRUCTURAL_MITIGATIONS:
    1. CHUNK_ISOLATION:
       Place each logical unit in its own XML container.
       <TASK>...</TASK> <CONTEXT>...</CONTEXT> <CONSTRAINTS>...</CONSTRAINTS>
       Never let different content types bleed into each other.

    2. PRIMACY_ANCHORING:
       The single most important instruction goes at POSITION[0].
       If it must be remembered throughout → repeat at POSITION[-1] as summary.

    3. PROGRESSIVE_DISCLOSURE:
       Do not inject all context upfront.
       Inject each piece at the moment it is needed (JIT).
       This keeps the "active" context window small and dense.

    4. ATTENTION_ANCHORS:
       Use strong structural tokens at section starts:
       <CRITICAL>, ### IMPORTANT, @REQUIRED:
       These create high attention weights at boundaries.

    5. DISTRACTORS_ELIMINATION:
       Before finalizing prompt, AUDIT for:
       - Redundant examples (pick best 1, remove rest)
       - Contradictory instructions (resolve, keep one)
       - Tangential context (remove unless directly cited in task)

</LOST_IN_MIDDLE_GUARD>
```

---
---

# SPO-04 · SYNTAX ENGINEERING REFERENCE

> **Source:** Prompts_6 synthesis — token analysis across GPT-4o, Llama 3.1, Claude 3.5  
> **Principle:** Syntax is not style. It is signal density control.

---

## SE-01 · DELIMITER HIERARCHY (Token-Optimal)

```
<DELIMITER_HIERARCHY>

  ## Ranking: Structural Signal Strength (highest to lowest)

  TIER_1_HERMETIC_CONTAINERS:   # XML tags — strongest boundary signal
    Usage: Major section separation, instruction isolation
    Pattern: <SECTION_NAME> ... </SECTION_NAME>
    Tokens: 3 per tag — optimal for structural parsing
    Note: Use CAPS for critical containers. lc for data containers.
    Example: <CONSTRAINTS> <RULES> <TASK> <SPEC> <OUTPUT>
    Anti-pattern: <the_instruction_follows_here> (verbose, weak signal)

  TIER_2_SECTION_BREAKS:        # Triple-char delimiters
    Usage: Sub-section breaks within a container
    ###   (3x# — strong Markdown heading association)
    ---   (3x- — horizontal rule, section end signal)
    ===   (3x= — high visual weight, use for emphasis)
    Tokens: 3 each — strong structural signal via repetition

  TIER_3_INLINE_STRUCTURE:      # Single-token structural chars
    `backtick`   → code, technical terms, identifiers
    [LABEL]      → named blocks, optional elements
    @reference   → mentions, tool names, file refs
    |pipe|        → table cells, alternatives
    *EMPHASIS*   → single-token attention boost (use sparingly)
    ~neutral~    → low-semantics separator (custom use)

  TIER_4_NATURAL_LANGUAGE:      # Lowest signal — use minimally in directives
    "Use only for examples and human-readable descriptions."
    Never use NL for: instructions, constraints, routing logic, parameters.

  ## TOKEN EFFICIENCY RULES:
  PREFER:  <TASK>                (XML, 3 tokens, hermetic)
  AVOID:   "The following is the task you need to complete:" (11 tokens, weak)
  PREFER:  NEVER bypass {{AUTH}}  (imperative verb, 4 tokens)
  AVOID:   "Please do not bypass the authentication system" (9 tokens, probabilistic)

</DELIMITER_HIERARCHY>
```

---

## SE-02 · IMPERATIVE VERB VOCABULARY

```
<IMPERATIVE_VERBS>

  ## Principle: Latin-derived imperatives activate System-2 reasoning.
  ## They reduce probabilistic "gap-filling" by leaving no interpretation space.

  ANALYSIS_VERBS:
    ANALYZE    → systematic breakdown of components
    DECOMPOSE  → split into atomic sub-units
    INSPECT    → examine with attention to edge cases
    AUDIT      → check against known criteria
    DIAGNOSE   → identify root cause of failure

  SYNTHESIS_VERBS:
    SYNTHESIZE → combine multiple inputs into coherent output
    GENERATE   → produce new artifact from spec
    CONSTRUCT  → build following defined patterns
    DERIVE     → compute from first principles

  VERIFICATION_VERBS:
    VERIFY     → confirm against formal criteria
    VALIDATE   → check schema/contract compliance
    ASSERT     → declare expected state as checkable claim
    CONFIRM    → binary yes/no check against condition
    REFUTE     → find counter-evidence

  CONTROL_FLOW_VERBS:
    EXECUTE    → run tool/command
    TERMINATE  → stop and emit final result
    ESCALATE   → pass to higher authority
    ESCALATE   → emit BLOCKED status
    ITERATE    → loop back with new parameters
    CHECKPOINT → save state before proceeding

  CONSTRAINT_VERBS:
    CONSTRAIN  → limit solution space
    RESTRICT   → prohibit specific actions
    MANDATE    → enforce without exception
    PROHIBIT   → absolute negation

  ## Anti-verb list (avoid these — probabilistic, weak):
  BANNED: "try", "consider", "maybe", "attempt", "think about",
          "look into", "feel free", "you can", "if possible"

</IMPERATIVE_VERBS>
```

---

## SE-03 · INSTRUCTION GRAMMAR — Canonical Prompt Structure

```
<INSTRUCTION_GRAMMAR>

  ## Canonical order: ROLE → CONTEXT → TASK → CONSTRAINTS → FORMAT → EXAMPLES

  ## TOKEN-OPTIMAL TEMPLATE:
  ---
  <ROLE>{{SPECIALIST_PERSONA_IN_ONE_LINE}}</ROLE>

  <CONTEXT>
    PROJECT: {{PROJECT_ID}}
    MODULE:  {{MODULE_NAME}}
    PHASE:   {{CURRENT_PHASE}}
    STATE:   @blackboard://{{EP_ID}}/{{NODE_ID}}/state
  </CONTEXT>

  <TASK>
    INTENT:    {{ONE_LINE_GOAL}}
    SPEC_REF:  {{MS_ID}}
    PROCEDURE: {{ORDERED_STEPS}}
  </TASK>

  <CONSTRAINTS>
    DO:    {{POSITIVE_RULES}}
    NEVER: {{ABSOLUTE_PROHIBITIONS}}
    PREFER: {{GOLD_STANDARD_REF}} over {{ANTI_PATTERN_REF}}
  </CONSTRAINTS>

  <OUTPUT>
    FORMAT:  {{json|markdown|code|structured_text}}
    SCHEMA:  {{OUTPUT_SCHEMA_REF}}
    DELIVER: {{DELIVERY_TARGET}}
  </OUTPUT>
  ---

  ## RULES:
  - Role activates relevant knowledge domain (Primacy Effect).
  - Context anchors to current state (prevents drift).
  - Task is single-intent (SRP mandate).
  - Constraints frame the solution space (DO + NEVER + PREFER pattern).
  - Output schema eliminates format ambiguity.
  - Examples: max 1 positive + 1 negative. More = In-Context Bias risk.

</INSTRUCTION_GRAMMAR>
```

---
---

# SPO-05 · SPEC-DRIVEN DEVELOPMENT — AMBIGUITY ELIMINATION PROTOCOL

> **Source:** Context_2 (Formal Spec Architect Protocol) synthesis  
> **Principle:** Vague spec → gap-filling → hallucination. Formal spec → logical deduction.

---

## SDD-01 · FAILURE CAUSALITY CHAIN (recognize and break)

```
<HALLUCINATION_CAUSALITY_CHAIN>

  ## The deterministic path from bad spec to bad code:

  VAGUE_PROSE (User Story) →
  MISSING_CONTEXT (LLM has no implicit domain knowledge) →
  PROBABILISTIC_INFERENCE (gap-filling from training data) →
  HALLUCINATION (invented details presented as fact) →
  FUNCTIONAL_DEFECT or ARCHITECTURE_VIOLATION

  ## Breaking the chain:
  AT EACH ARROW: insert a formal constraint that blocks the failure mode.

  VAGUE_PROSE:           → Replace with MICRO_SPEC (SDD-02)
  MISSING_CONTEXT:       → MNC injection (CE-01) + JIT loading
  PROBABILISTIC_INFERENCE: → LTL formal properties (SDD-03)
  HALLUCINATION:         → CoVe loop (LOOP-03) + grounding mandate
  FUNCTIONAL_DEFECT:     → TDFlow loop (LOOP-02) + test-first mandate

</HALLUCINATION_CAUSALITY_CHAIN>
```

---

## SDD-02 · MICRO-SPEC CONTRACT — SRP-Compliant Atomic Task

```
<MICRO_SPEC id="{{MS_ID}}" parent="{{EP_ID}}" version="{{SEMVER}}">

  ## SRP MANDATE: This spec has EXACTLY ONE responsibility.
  ## If you identify two responsibilities, SPLIT into two specs.

  <IDENTITY>
    COMPONENT:    {{COMPONENT_NAME}}
    MODULE:       {{MODULE_NAME}}
    RESPONSIBILITY: {{ONE_LINE_RESPONSIBILITY}}   # The "single reason to change"
  </IDENTITY>

  <INTERFACE_CONTRACT>
    ## DIP MANDATE: Code ONLY against this interface — never against implementation.
    INTERFACE_REF: @contracts/{{INTERFACE_FILE}}
    INPUT_SCHEMA:  @schemas/{{INPUT_SCHEMA}}.json
    OUTPUT_SCHEMA: @schemas/{{OUTPUT_SCHEMA}}.json
  </INTERFACE_CONTRACT>

  <PRECONDITIONS>
    ## @REQUIRE: All must be true before execution begins.
    - {{ENTITY}} exists and satisfies {{VALIDATOR}}.
    - System state: {{REQUIRED_STATE}}.
    - Permission: current_agent has role {{REQUIRED_ROLE}}.
  </PRECONDITIONS>

  <POSTCONDITIONS>
    ## @ENSURE: All must be true after successful execution.
    - {{OUTPUT_ENTITY}} persisted with schema compliance.
    - Domain event {{EVENT_NAME}} emitted on {{EVENT_BUS}}.
    - No side effects outside {{MODULE_BOUNDARY}}.
  </POSTCONDITIONS>

  <INVARIANTS>
    ## @ALWAYS: Must hold throughout execution (not just at end).
    - {{ENTITY}}.{{FIELD}} is never null.
    - Idempotency: repeat calls with {{IDEMPOTENCY_KEY}} → identical result.
    - Transaction: all writes commit atomically or roll back completely.
  </INVARIANTS>

  <LTL_PROPERTIES>
    @SAFETY:   □ (pre_conditions_valid → output_schema_valid)
    @SAFETY:   □ ¬(write_outside_{{MODULE_BOUNDARY}})
    @LIVENESS: ◇ (execution_start → done ∨ blocked_with_reason)
    @LIVENESS: ◇ (event_emitted → consumed_by_{{CONSUMER}})
  </LTL_PROPERTIES>

  <VALIDATION>
    TEST_REF:     @tests/{{TEST_SUITE_REF}}
    COVERAGE_MIN: {{MIN_COVERAGE_PCT}}%
    LOOP:         TDFLOW_LOOP (LOOP-02)
    SELF_AUDIT:
      - [ ] All preconditions tested by unit tests.
      - [ ] All postconditions asserted in integration test.
      - [ ] No {{BANNED_PATTERNS}} introduced.
      - [ ] exit_code == 0 on: lint · typecheck · test.
  </VALIDATION>

  <CONTEXT_REFS>
    ## JIT — load ONLY when needed, never preload all
    - pattern:   @.ai-context/patterns.md#{{PATTERN_NAME}}
    - adr:       @docs/ADR-{{ADR_ID}}.md
    - schema:    @schemas/{{SCHEMA_REF}}.json
  </CONTEXT_REFS>

</MICRO_SPEC>
```

---

## SDD-03 · DEPENDENCY MAP — KI-AM (Abhängigkeitsgraph)

```
<DEPENDENCY_MAP id="{{DM_ID}}" project="{{PROJECT_ID}}">

  ## Purpose: Track all inter-spec dependencies to prevent hidden coupling.
  ## Rule: Every dependency must be explicit. Implicit dependencies = bugs.

  MODULES:
    - id: "MOD-{{A}}"  specs: ["{{MS_ID_A1}}", "{{MS_ID_A2}}"]  owns: ["{{DOMAIN_A}}"]
    - id: "MOD-{{B}}"  specs: ["{{MS_ID_B1}}"]                  owns: ["{{DOMAIN_B}}"]

  DEPENDENCIES:
    ## DIP-compliant: depend on interface, never on implementation
    - from: "MOD-{{A}}"  to: "MOD-{{B}}"
      via:  "interface: @contracts/{{INTERFACE_B}}"
      type: "REQUIRED"    # REQUIRED | OPTIONAL | FORBIDDEN

  FORBIDDEN_DEPENDENCIES:
    ## Cross-boundary violations — agent MUST NOT create these
    - "MOD-{{FRONTEND}} MUST NOT import from MOD-{{DATABASE}}"
    - "MOD-{{AUTH}} MUST NOT depend on MOD-{{BILLING}} implementation"

</DEPENDENCY_MAP>
```

---
---

# SPO-06 · FAILURE TAXONOMY (MAST) — 14 MODES + COMPENSATIONS

> **Source:** MAST framework — derived from 150 agent interaction analyses  
> **Usage:** Reference during RED_BLUE_LOOP. Classify all failures here first.

```
<MAST_TAXONOMY version="2025.1">

  ## CATEGORY I: SPECIFICATION FAILURES
  ##   Root cause: Ambiguous, incomplete, or conflicting specifications.

  F-01 SPEC_AMBIGUITY:
    Signal:    Agent produces plausible but incorrect interpretation.
    Root:      Vague prose, missing formal constraints.
    Compensate: Add LTL property + concrete counter-example to spec.

  F-02 SPEC_INCOMPLETENESS:
    Signal:    Agent invents missing behavior (hallucination).
    Root:      Postconditions not fully enumerated.
    Compensate: Add explicit DEFAULT behavior for unspecified cases.

  F-03 SPEC_CONFLICT:
    Signal:    Agent oscillates between two behaviors unpredictably.
    Root:      Two constraints contradict each other.
    Compensate: Add priority order to constraint stack. Highest wins.

  F-04 CONTEXT_POLLUTION:
    Signal:    Agent applies rules from wrong module.
    Root:      Monolithic context, cross-context bleeding.
    Compensate: Enforce nearest-file cascade + module boundary declaration.

  ## CATEGORY II: INTER-AGENT MISALIGNMENT
  ##   Root cause: Breakdown in communication or coordination between agents.

  F-05 PROTOCOL_VIOLATION:
    Signal:    Agent sends/receives malformed message.
    Root:      Missing schema validation on payload.
    Compensate: Add schema assertion at every message ingestion point.

  F-06 STATE_DESYNC:
    Signal:    Two agents have conflicting views of system state.
    Root:      No atomic state update mechanism.
    Compensate: Implement blackboard with optimistic locking + version field.

  F-07 AUTHORITY_AMBIGUITY:
    Signal:    Multiple agents attempt same action simultaneously.
    Root:      No exclusive ownership declaration for resources.
    Compensate: Add OWNS: field to module spec. One owner per resource.

  F-08 CONTEXT_LOSS_HANDOFF:
    Signal:    Receiving agent lacks context from sender.
    Root:      Insufficient state in AGENT_COMM_PAYLOAD.
    Compensate: Add STATE_REFS with all relevant spec anchors to payload.

  ## CATEGORY III: TASK VERIFICATION FAILURES
  ##   Root cause: Absence or inadequacy of verification mechanisms.

  F-09 SILENT_FAILURE:
    Signal:    Agent reports DONE but postconditions not met.
    Root:      No automated post-condition verification.
    Compensate: Mandate TDFLOW_LOOP for all code tasks. Exit code is ground truth.

  F-10 REASONING_DRIFT:
    Signal:    Agent's plan diverges from reality after tool observations.
    Root:      Plan not updated after each observation.
    Compensate: Enforce REACT_LOOP. Thought must reference latest Observation.

  F-11 OVERTHINKING:
    Signal:    Agent produces analysis but no action. Infinite reasoning loop.
    Root:      Reasoning-Action Dilemma — high reasoning / low grounding.
    Compensate: Add ReAct loop guard: "3 identical consecutive Thoughts → force ACTION."

  F-12 HALLUCINATED_CORRECTNESS:
    Signal:    Agent claims tests pass without running them.
    Root:      Agent infers results instead of observing them.
    Compensate: VERIFY exit_code from actual execution. Confidence score ≠ ground truth.

  F-13 INCOMPLETE_ROLLBACK:
    Signal:    Partial failure leaves system in inconsistent state.
    Root:      No compensation transactions defined.
    Compensate: Implement SAGA_LOOP with idempotent compensating actions for every node.

  F-14 VALIDATION_ESCAPE:
    Signal:    Errors pass through all validation nodes undetected.
    Root:      Validation nodes check wrong criteria or are skipped.
    Compensate: Add T5 formal verification (LTL) for critical paths + AgentRR replay.

</MAST_TAXONOMY>
```

---
---

# SPO-07 · ANTI-HALLUCINATION PROTOCOL — INTEGRATED SPEC

> **Source:** Context_1 synthesis — all grounding mechanisms combined  
> **Principle:** Hallucination is not random. It is a predictable response to ambiguity.

```
<ANTI_HALLUCINATION_PROTOCOL agent="{{AGENT_ID}}">

  ## LAYER 1: PREVENTION (before generation)
  L1_GROUNDING_MANDATE:
    - LOAD only context directly relevant to {{MS_ID}} (MNC Injection, CE-01).
    - READ preconditions and invariants before writing a single line.
    - NEVER generate content about {{DOMAIN}} without @-referenced evidence.
    - APPLY Step-Back: before implementation, ask
      "What fundamental principle governs this?"
      Anchor answer in spec before proceeding.

  L1_SCOPE_RESTRICTION:
    RESPOND only based on: provided spec · retrieved context · tool observations.
    NEVER draw on parametric memory for facts that could have changed.
    IF unsure: emit [UNCERTAIN: reason] rather than confident guess.

  ## LAYER 2: DETECTION (during generation)
  L2_CLAIM_TAGGING:
    LABEL every factual claim in output as [CLAIM_N].
    LABEL every inferred conclusion as [INFERENCE_N].
    LABEL every assumption as [ASSUMPTION_N: explicit statement of assumption].
    NEVER present [INFERENCE] or [ASSUMPTION] as established fact.

  L2_HIGHLIGHTED_COT:
    USE XML tags to anchor each reasoning step to its evidence:
    <STEP_1 evidence="@{{SOURCE_REF}}">Reasoning text here.</STEP_1>
    Steps without evidence reference → flag as [UNGROUNDED].

  ## LAYER 3: CORRECTION (after generation)
  L3_COVE_MANDATORY:
    APPLY COVE_LOOP (LOOP-03) to ALL responses containing:
    - Claims about external state
    - Assertions about code behavior
    - Statements about system configuration
    - Any numerical data or metrics

  L3_REACT_GROUNDING:
    For any claim that can be verified by tool execution:
    PREFER running the tool over reasoning about the result.
    grounding_by_execution >> LLM_confidence_score

  ## LAYER 4: STRUCTURAL (spec-level prevention)
  L4_FORMAL_PROPERTIES:
    Every MICRO_SPEC MUST contain LTL properties.
    LTL constrains the solution space → reduces gap-filling probability.
    @SAFETY □ and @LIVENESS ◇ properties are NOT optional.

  L4_TDFLOW_MANDATE:
    Code correctness MUST be verified by test execution (exit_code == 0).
    "The tests probably pass" is not a valid completion state.
    No DONE status without observed test results.

</ANTI_HALLUCINATION_PROTOCOL>
```

---
---

# SPO-08 · MASTER SYSTEM PROMPT TEMPLATE

> **Purpose:** Bootstrap any agent in the ecosystem.  
> **Architecture:** Primacy-effect anchored · XML hermetic · MNC compliant

```markdown
---
agent_id:    "{{AGENT_ID}}"
agent_type:  "{{AGENT_TYPE}}"     # e.g. "backend-api", "orchestrator", "critic"
project_id:  "{{PROJECT_ID}}"
ep_ref:      "{{EP_ID}}"
ms_ref:      "{{MS_ID}}"
version:     "{{SEMVER}}"
---

<SYSTEM_CONSTITUTION>

  <IDENTITY>
    ROLE:     {{SPECIALIST_ROLE}}
    MISSION:  Execute {{MS_ID}} within {{MODULE_NAME}} for {{PROJECT_ID}}.
    STRATEGY: Hexagonal Architecture — isolate domain logic from adapters.
              Spec-Driven — spec is law, code is disposable artifact.
  </IDENTITY>

  <GOVERNANCE>
    HIERARCHY:     SYSTEM > SPEC > USER_REQUEST > HISTORY
    SOURCE_TRUTH:  {{MS_ID}} is the only source of truth for this task.
    SAFETY_GATE:   Privileged actions require MCP proxy + HiTL authorization.
    MEMORY_RULE:   On conflict, refer to nearest AGENTS.md in file tree.
  </GOVERNANCE>

  <COGNITIVE_FRAMEWORK>
    REASONING_MODE: ReAct (LOOP-04) for tool-dependent tasks.
                    CoVe  (LOOP-03) for all factual assertions.
    STEP_BACK:      Before implementation → abstract to governing principle.
    SCRATCHPAD:     Maintain scratchpad per CE-02 across all tool calls.
    VERIFICATION:   Grounding-by-execution > reasoning confidence.
  </COGNITIVE_FRAMEWORK>

  <OPERATIONAL_CONSTRAINTS>
    TECH_STACK:   {{PRIMARY_LANGUAGE}} · {{FRAMEWORK}} · {{DATABASE}}
    PKG_MANAGER:  {{PKG_MANAGER}}
    DO:
      - Follow patterns in @.ai-context/patterns.md.
      - Emit domain events on every state mutation.
      - Write idempotent operations using {{IDEMPOTENCY_KEY}} pattern.
    NEVER:
      - Use `{{BANNED_TYPES}}` type annotations.
      - Call external services without {{HTTP_ADAPTER}}.
      - Commit without exit_code == 0 on: lint · typecheck · test.
      - Skip precondition checks.
    PREFER: @{{GOLD_STANDARD_FILE}} over @{{ANTI_PATTERN_FILE}}
  </OPERATIONAL_CONSTRAINTS>

  <EXECUTION_PROTOCOL>
    1. READ   {{MS_ID}} in full before starting.
    2. VERIFY preconditions before executing.
    3. APPLY  PDCA_LOOP (LOOP-01) for all sub-steps.
    4. APPLY  TDFLOW_LOOP (LOOP-02) for all code changes.
    5. APPLY  COVE_LOOP (LOOP-03) for all factual assertions.
    6. EMIT   {status: DONE | BLOCKED} — never silent completion.
    7. WRITE  result to blackboard://{{EP_ID}}/{{NODE_ID}}/result.
  </EXECUTION_PROTOCOL>

  <TOOL_PERMISSIONS>
    WITHOUT_PROMPT: read · list · lint · typecheck · run_single_test
    ASK_FIRST:      install · git_push · delete · run_migrations · full_test_suite
    FORBIDDEN:      bypass_{{AUTH}} · write_to_{{PROTECTED_PATH}} · external_http_direct
  </TOOL_PERMISSIONS>

</SYSTEM_CONSTITUTION>
```

---
---

# SPO-09 · ORCHESTRATION MANIFEST — FULL MULTI-AGENT COORDINATION SPEC

```markdown
---
ep_id:       "{{EP_ID}}"
feature:     "{{FEATURE_ID}}"
version:     "{{SEMVER}}"
created:     "{{ISO_8601}}"
status:      "ACTIVE"
---

<EXECUTION_PLAN type="DAG">

  <WORKFLOW>
    ## Node structure: ID · spec_ref · agent_type · depends_on · loop_type
    NODES:
      N-01:  spec={{MS_ID_01}}  agent=schema-agent       deps=[]         loop=PDCA
      N-02:  spec={{MS_ID_02}}  agent=backend-agent      deps=[N-01]     loop=TDFLOW
      N-03:  spec={{MS_ID_03}}  agent=frontend-agent     deps=[N-01]     loop=TDFLOW
      N-04:  spec={{MS_ID_04}}  agent=integration-agent  deps=[N-02,N-03] loop=PDCA
      N-05:  spec={{MS_ID_05}}  agent=critic-agent       deps=[N-04]     loop=COVE
      N-SAGA: type=compensation  trigger=any_failure      loop=SAGA

    ## Conditional routing
    EDGES:
      N-05 → DONE      IF (cove_pass == true  AND ltl_pass == true)
      N-05 → N-02      IF (cove_pass == false AND retry_count < 3)
      N-05 → ESCALATE  IF (retry_count >= 3)
  </WORKFLOW>

  <COORDINATION>
    PATTERN:   Blackboard Architecture
    STATE_REF: blackboard://{{EP_ID}}/
    PARALLEL:  N-02 and N-03 execute concurrently (independent deps)
    SYNC_GATE: N-04 activates ONLY when N-02 AND N-03 both emit DONE
    MEMORY:    Each node writes to blackboard://{{EP_ID}}/{{NODE_ID}}/result
  </COORDINATION>

  <ERROR_HANDLING>
    RECOVERY:    SAGA_LOOP (LOOP-07) for all committed nodes on failure
    ESCALATION:  3 failed retries → emit BLOCKED to {{HUMAN_CHANNEL}}
    FALLBACK:    critic-agent N-05 triggers self-correction loop
    MAX_RUNTIME: {{MAX_RUNTIME_MINUTES}} minutes before forced TIMEOUT
  </ERROR_HANDLING>

  <VALIDATION_GATES>
    ## Gates that must pass before advancing to next tier
    AFTER_N-02: T1_OUTPUT + T2_REASONING (exit_code + CoVe)
    AFTER_N-04: T3_WORKFLOW (state transition audit)
    AFTER_N-05: T5_FORMAL (LTL properties check)
    PRE_DEPLOY: T4_ECOSYSTEM (Red-Blue loop, {{N}} cycles)
  </VALIDATION_GATES>

  <OBSERVABILITY>
    TRACE_SCHEMA:
      ep_id:       "{{EP_ID}}"
      node_id:     "{{NODE_ID}}"
      agent_id:    "{{AGENT_ID}}"
      ms_ref:      "{{MS_ID}}"
      status:      "DONE|FAILED|BLOCKED|COMPENSATED"
      loop_used:   "{{LOOP_ID}}"
      retry_count: {{N}}
      ltl_pass:    true|false
      cove_pass:   true|false
      duration_ms: {{N}}
      tool_calls:  [{{TOOL_CALL_LOG}}]
    STORE:  @{{AUDIT_LOG_PATH}}
  </OBSERVABILITY>

</EXECUTION_PLAN>
```

---
---

# SYNTHESIS MATRIX

> How every component of this framework addresses a specific failure mode.

| Failure Mode | MAST ID | Loop Deployed | Spec Pattern | Validation Tier |
|:--|:--|:--|:--|:--|
| Hallucinated correctness | F-12 | TDFLOW (L2) | LTL @SAFETY | T1 Output |
| Reasoning drift | F-10 | ReAct (L4) | Scratchpad | T2 Reasoning |
| Overthinking / paralysis | F-11 | ReAct guard (L4) | Loop guard | T2 Reasoning |
| State desync | F-06 | PDCA (L1) + SAGA | Blackboard + idempotency | T3 Workflow |
| Context pollution | F-04 | CE-01 MNC | Nearest-file cascade | T3 Workflow |
| Silent failure | F-09 | TDFlow (L2) | Exit-code mandate | T1 Output |
| Spec ambiguity | F-01 | CoVe (L3) | LTL + formal properties | T5 Formal |
| Inter-agent desync | F-08 | SAGA (L7) | AGENT_COMM_PAYLOAD schema | T3 Workflow |
| Unknown unknowns | — | Red-Blue (L8) | MAST scan | T4 Ecosystem |
| Incomplete rollback | F-13 | SAGA (L7) | Compensation map | T3 Workflow |
| RAG noise injection | — | CRAG (L6) | Relevance grading | T2 Reasoning |
| Lost-in-middle | — | CE-03 guard | Position rules + XML | T3 Workflow |

---

> **Master principle:** The agent does not need to be creative. It needs to be correct.  
> Creativity lives in the spec. Execution lives in the loops. Verification lives in the stack.
