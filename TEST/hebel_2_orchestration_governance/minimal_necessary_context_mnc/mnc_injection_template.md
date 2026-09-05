# CE-01 · MNC INJECTION TEMPLATE — Minimal Necessary Context

> **Axiom AX-03:** LLMs verarbeiten nur 10–20 % ihres Kontextfensters mit maximaler Zuverlässigkeit. Jedes irrelevante Token verdünnt die Signaldichte ("Context Rot").  
> **Prinzip:** Inject only what is needed, when it is needed (Just-In-Time Context Loading).

---

```xml
<context_packet id="CP-{{TASK_ID}}-{{STEP_NUM}}" generated_at="{{TIMESTAMP}}">

  <!-- ═══════════════════════════════════════════════════════════════════════
       TIER 1: ANCHOR CONTEXT (Always present — max 500 tokens)
       ═══════════════════════════════════════════════════════════════════════ -->
  <system_anchor>
    <constitution_ref ref="CONSTITUTION.md" hash="{{SHA256}}" />
    <domain>{{DOMAIN}}</domain>
    <role>{{AGENT_ROLE}}</role>
    <task_id>{{TASK_ID}}</task_id>
    <task_goal>{{SINGLE_SENTENCE_GOAL}}</task_goal>
  </system_anchor>

  <!-- ═══════════════════════════════════════════════════════════════════════
       TIER 2: TARGET SPECIFICATION (Strictly scoped — max 1500 tokens)
       ═══════════════════════════════════════════════════════════════════════ -->
  <spec_context>
    <spec_id>{{SPEC_ID}}</spec_id>
    <allowed_files>
      <file>{{PRIMARY_FILE_TO_MODIFY}}</file>
      <file>{{TEST_FILE_FOR_MODIFICATION}}</file>
    </allowed_files>
    <invariants>
      <!-- Only the invariants relevant to this task, NOT all 50 invariants -->
      <invariant id="{{INV_ID}}">{{INVARIANT_TEXT}}</invariant>
    </invariants>
    <acceptance_criteria>
      <ac id="{{AC_ID}}">{{CRITERION_TEXT}}</ac>
    </acceptance_criteria>
  </spec_context>

  <!-- ═══════════════════════════════════════════════════════════════════════
       TIER 3: INTERFACE BOUNDARIES (Signatures only — no implementations)
       ═══════════════════════════════════════════════════════════════════════ -->
  <interface_boundary>
    <!-- NEVER inject whole crates; only inject public function and struct definitions -->
    <signature file="crates/memfuse-core/src/types.rs">
      pub struct DocumentId(pub u64);
      pub struct ScoredResult { pub id: DocumentId, pub score: f32 }
    </signature>
  </interface_boundary>

  <!-- ═══════════════════════════════════════════════════════════════════════
       TIER 4: JIT WORKSPACE STATE (Last step diff or error only)
       ═══════════════════════════════════════════════════════════════════════ -->
  <jit_state>
    <last_test_output status="{{PASS|FAIL}}">
      <![CDATA[{{RELEVANT_COMPILER_ERROR_OR_PANIC_SNIPPET}}]]>
    </last_test_output>
  </jit_state>

</context_packet>
```
