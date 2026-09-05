# T-07 · METACOGNITIVE STATE CHECKPOINT (PDCA Loop)

> **Zweck:** Löst das Problem der *Silent Error Propagation* bei autonomen Agenten. Nach JEDEM Tool-Call oder Zwischenschritt persistiert der Agent diesen Checkpoint, BEVOR er zum nächsten Graph-Knoten übergeht.  
> **Kernregel:** Identische Retries sind strikt verboten. Jeder Retry MUSS eine deklarierte Strategie-Mutation (`<strategy_adaptation>`) enthalten.

---

```xml
<checkpoint id="MSC-{{WORKFLOW_ID}}-NODE{{N}}-ATT{{M}}" workflow_ref="{{WORKFLOW_ID}}" version="2026.1">

  <!-- ═══════════════════════════════════════════════════════════════════════
       1. PLAN (Was beabsichtigt war — gesetzt VOR dem Aufruf)
       ═══════════════════════════════════════════════════════════════════════ -->
  <plan>
    <current_node>{{CURRENT_TASK_NODE}}</current_node>
    <intended_action>{{INTENDED_ACTION}}</intended_action>
    <spec_ref id="{{SPEC_ID}}" section="{{SECTION}}" />
    <tool_called>{{TOOL_NAME_OR_CLI_COMMAND}}</tool_called>
    <inputs_hash>{{SHA256_OF_INPUTS}}</inputs_hash>
  </plan>

  <!-- ═══════════════════════════════════════════════════════════════════════
       2. DO (Tatsächlicher Output des Tools — ungefiltert)
       ═══════════════════════════════════════════════════════════════════════ -->
  <observation>
    <raw_stdout><![CDATA[{{RAW_STDOUT}}]]></raw_stdout>
    <raw_stderr><![CDATA[{{RAW_STDERR}}]]></raw_stderr>
    <exit_code>{{EXIT_CODE}}</exit_code>
    <duration_ms>{{DURATION_MS}}</duration_ms>
    <output_hash>{{SHA256_OF_OUTPUT}}</output_hash>
  </observation>

  <!-- ═══════════════════════════════════════════════════════════════════════
       3. CHECK (Metakognitive Selbstreflexion gegen die Spezifikation)
       ═══════════════════════════════════════════════════════════════════════ -->
  <metacognition>
    <evaluation>
      <against_spec ref="{{SPEC_ID}}" section="acceptance_criteria" />
      <critique>{{OBJECTIVE_CRITIQUE_AGAINST_SPEC}}</critique>
      <status>{{PASS | FAIL | PARTIAL | ANOMALY}}</status>
      <anomaly_detected>{{true | false}}</anomaly_detected>
      <confidence>{{0-100}}</confidence>
    </evaluation>

    <!-- Design-by-Contract Prüfung -->
    <contract_verification>
      <preconditions_held>{{true | false}}</preconditions_held>
      <postconditions_met>{{true | false}}</postconditions_met>
      <invariants_intact>{{true | false}}</invariants_intact>
    </contract_verification>
  </metacognition>

  <!-- ═══════════════════════════════════════════════════════════════════════
       4. ACT (Zustandsübergang & Strategie-Mutation bei Fehler)
       ═══════════════════════════════════════════════════════════════════════ -->
  <action_decision>
    <decision>{{ADVANCE | RETRY_WITH_MUTATION | ESCALATE}}</decision>
  </action_decision>

  <!-- Nur bei status != PASS: Identische Wiederholungen sind verboten! -->
  <strategy_adaptation>
    <failure_classification>{{TRANSIENT | LOGICAL | FATAL | ARCHITECTURAL}}</failure_classification>
    <previous_hypothesis>{{WHY_PREVIOUS_ATTEMPT_FAILED}}</previous_hypothesis>
    <mutated_hypothesis>{{NEW_APPROACH_OR_DIFF}}</mutated_hypothesis>
    <mutation_rationale>{{WHY_NEW_APPROACH_FIXES_ROOT_CAUSE}}</mutation_rationale>
  </strategy_adaptation>

</checkpoint>
```
