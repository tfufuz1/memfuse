# T-05 · COVE VERIFICATION CONTRACT (Chain-of-Verification)

> **Zweck:** Erzwingt die formale 4-Phasen-Verifikation vor jedem Commit/Merge. Verhindert "Context Rot", Halluzinationen und architektonische Namenskollisionen (wie z. B. die `CompactionStrategy`-Kollision zwischen LSM-Disk-Compaction und LLM-Context-Token-Compaction).

---

```xml
<verification_contract id="{{VC_ID}}" governs="{{FEATURE_OR_PR_ID}}" version="2026.1">

  <!-- ═══════════════════════════════════════════════════════════════════════
       1. SCOPE ANCHOR (Primacy — Verhindert Scope Drift & Context Rot)
       ═══════════════════════════════════════════════════════════════════════ -->
  <scope_boundary>
    <verifies_only>{{WHAT_IS_BEING_VERIFIED}}</verifies_only>
    <excludes>{{WHAT_IS_EXPLICITLY_OUT_OF_SCOPE}}</excludes>
    <!-- CRITICAL: Verifier darf NIEMALS etwas außerhalb von <verifies_only> bewerten. -->
    <source_of_truth ref="{{SPEC_ID}}" section="acceptance_criteria" />
    <input_artifact ref="{{PR_NUMBER_OR_FILE_PATH}}" />
  </scope_boundary>

  <!-- ═══════════════════════════════════════════════════════════════════════
       PHASE 1: BASELINE GENERATION
       Unveränderter Output des Worker-Agenten.
       ═══════════════════════════════════════════════════════════════════════ -->
  <phase_1_baseline>
    <author_agent>{{AUTHOR_AGENT_ID}}</author_agent>
    <output_summary>{{DESCRIPTION_OF_GENERATED_CODE_OR_DIFF}}</output_summary>
    <artifacts>
      <artifact path="{{FILE_PATH}}" sha="{{GIT_SHA}}" />
    </artifacts>
  </phase_1_baseline>

  <!-- ═══════════════════════════════════════════════════════════════════════
       PHASE 2: VERIFICATION QUESTION GENERATION
       Fragen werden direkt aus den Acceptance Criteria (AC) abgeleitet.
       Muss zwingend binär (PASS/FAIL) beantwortbar sein.
       ═══════════════════════════════════════════════════════════════════════ -->
  <phase_2_questions>
    <vq id="VQ-01" targets_ac="AC-01">
      {{VERIFICATION_QUESTION_1}}
    </vq>
    <vq id="VQ-02" targets_ac="AC-02">
      {{VERIFICATION_QUESTION_2}}
    </vq>
    <vq id="VQ-03" targets_ac="AC-03">
      Sind alle Test-Mandate in der Spec (UT-01 bis UT-N) vorhanden und grün?
    </vq>
    <vq id="VQ-04" targets_ac="AC-04">
      Importiert die Implementierung ausschließlich erlaubte Patterns (keine internen Core-Hacks)?
    </vq>
    
    <!-- MANDATORY COVE SAFETY CHECKS -->
    <vq id="VQ-05" type="HALLUCINATION_CHECK">
      Enthält der Code Verhalten, Symbole oder Annahmen, die NICHT in der Spec spezifiziert sind?
      (Architektonische Drift-Prüfung — z.B. Namenskollisionen bei CompactionStrategy).
    </vq>
    <vq id="VQ-06" type="INVARIANT_CHECK">
      Werden ausnahmslos ALLE formalen Invarianten des Moduls eingehalten?
    </vq>
    <vq id="VQ-07" type="SAFETY_CHECK">
      Gibt es unkontrollierte Panics, unwrap() ohne Begründung oder Speicher-Lecks?
    </vq>
  </phase_2_questions>

  <!-- ═══════════════════════════════════════════════════════════════════════
       PHASE 3: INDEPENDENT VERIFICATION (Unabhängige Prüfung)
       CRITICAL: Jede Frage muss isoliert beantwortet werden.
       Keine Cross-Contamination zwischen Fragen.
       ═══════════════════════════════════════════════════════════════════════ -->
  <phase_3_verification>
    <answer id="VQ-01">
      <result>{{PASS | FAIL}}</result>
      <evidence>{{CODE_LINE_OR_TEST_LOG}}</evidence>
      <confidence>{{HIGH | MEDIUM | LOW}}</confidence>
    </answer>
    <answer id="VQ-02">
      <result>{{PASS | FAIL}}</result>
      <evidence>{{EVIDENCE}}</evidence>
      <confidence>{{HIGH | MEDIUM | LOW}}</confidence>
    </answer>
    <answer id="VQ-05">
      <result>{{PASS | FAIL}}</result>
      <evidence>{{DIFF_CHECK_RESULT}}</evidence>
      <confidence>{{HIGH | MEDIUM | LOW}}</confidence>
    </answer>
    <answer id="VQ-06">
      <result>{{PASS | FAIL}}</result>
      <evidence>{{INVARIANT_ASSERTION_RESULT}}</evidence>
      <confidence>{{HIGH | MEDIUM | LOW}}</confidence>
    </answer>
    <answer id="VQ-07">
      <result>{{PASS | FAIL}}</result>
      <evidence>{{LINT_AND_TEST_OUTPUT}}</evidence>
      <confidence>{{HIGH | MEDIUM | LOW}}</confidence>
    </answer>
  </phase_3_verification>

  <!-- ═══════════════════════════════════════════════════════════════════════
       PHASE 4: VERDICT & BOUNDED ITERATION (E-5 Enhancement)
       ═══════════════════════════════════════════════════════════════════════ -->
  <phase_4_verdict>
    <confidence_gate>
      <threshold_accept>85</threshold_accept>
      <!-- Score >= 85 AND keine CRITICAL-Fehler -> auto APPROVED. -->
      <threshold_revise>60</threshold_revise>
      <!-- 60 <= Score < 85 -> APPROVED_WITH_CONDITIONS oder REVISION_NEEDED. -->
      <threshold_reject>60</threshold_reject>
      <!-- Score < 60 OR Fail bei VQ-05/06/07 -> REJECTED. -->
    </confidence_gate>

    <iteration_control>
      <current_attempt>{{N}}</current_attempt>
      <max_attempts>4</max_attempts>
      <!-- Nach 4 Iterationen ohne Pass: Sofortige Eskalation zum menschlichen Lead -->
      <decision>{{CONTINUE | FINAL_ACCEPT | ESCALATE}}</decision>
    </iteration_control>

    <bias_guard>
      <rule>Verifier lädt NUR die Original-Spec, NIEMALS Kommentare des Autors.</rule>
      <rule>Kein PARTIAL für VQ-05, VQ-06, VQ-07 zulässig.</rule>
    </bias_guard>

    <overall_result>{{APPROVED | REJECTED | ESCALATED}}</overall_result>

    <output_envelope>
      <status>{{accepted | rejected | escalated}}</status>
      <final_artifact_hash>{{SHA256_OF_ARTIFACT}}</final_artifact_hash>
      <confidence>{{FINAL_CONFIDENCE_0_100}}</confidence>
      <attempts>{{N}}</attempts>
    </output_envelope>
  </phase_4_verdict>

</verification_contract>
```
