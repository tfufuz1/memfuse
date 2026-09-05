# CoVe-Gates (T-05) vor jedem Pull Request

## 1. Problemstellung bei 60–100 Commits/Tag
Bei der rasanten Weiterentwicklung durch autonome Agenten drohen zwei fundamentale Risiken:
1. **Context Rot:** Agenten verlieren bei großen Prompt-Kontexten den Fokus und halluzinieren Code, der mit bestehenden Strukturen bricht.
2. **Architectural Drift & Namenskollisionen:** Ein Paradebeispiel ist die bereits in MemFuse aufgetauchte Kollision von `CompactionStrategy`:
   - In `memfuse-db/src/context_compaction.rs`: `CompactionStrategy` bezeichnet die LLM-Token-Kompaktierung (StatusToken, Truncate, Summarize).
   - In `chimera-storage` bzw. einem zukünftigen LSM-Storage-Backend in MemFuse: `CompactionStrategy` bezeichnet die Festplatten-LSM-Compaction (SizeTiered, Leveled).

Ohne formale CoVe-Gates wird ein solcher Konflikt erst zur Laufzeit oder bei späten Refactorings entdeckt.

## 2. Die Lösung: 4-Phasen CoVe-Gate (T-05)
Das Gate besteht aus 4 Phasen:
1. **Phase 1 (Baseline):** Der unveränderte Code des Entwicklers/Agenten wird isoliert.
2. **Phase 2 (Verification Questions):** Fragen werden direkt aus den Akzeptanzkriterien der Spezifikation generiert. Zwingend enthalten sind VQ-05 (Hallucination Check), VQ-06 (Invariant Check) und VQ-07 (Safety Check).
3. **Phase 3 (Independent Verification):** Ein Verifier-Agent beantwortet jede Frage isoliert mit Code-Evidenz.
4. **Phase 4 (Verdict & Bounded Iteration E-5):**
   - Score $\ge 85$: Auto-Approval.
   - Score $< 60$ oder Fail bei VQ-05/06/07: Sofortiger Rejection-Block.
   - Maximal 4 Iterationen vor menschlicher Eskalation.

## 3. Enthaltene Dateien
- [`cove_verification_contract_t05.md`](./cove_verification_contract_t05.md): Vollständige Spezifikation des T-05 Vertrags.
- [`cove_pr_gate_workflow.yml`](./cove_pr_gate_workflow.yml): Einsatzbereite GitHub Actions Workflow-Datei für `.github/workflows/cove_pr_gate.yml`.
