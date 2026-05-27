# Autonomous Squad Status Matrix Index

This directory contains the split Status Matrices for all 13 Jules Agents.
Following LLM context engineering best practices, we avoid a single monolithic state file.
Instead, each LLM maintains its own context and implementation status matrix here.

**Agents:**
- [01 - Core Guardian (memfuse-core)](./AGENT_01_CORE_GUARDIAN_STATUS.md)
- [02 - Store Engineer (memfuse-store)](./AGENT_02_STORE_ENGINEER_STATUS.md)
- [03 - Index Master (memfuse-index)](./AGENT_03_INDEX_MASTER_STATUS.md)
- [04 - Collection Architect (memfuse-db)](./AGENT_04_COLLECTION_ARCHITECT_STATUS.md)
- [05 - Text Analyst (memfuse-text)](./AGENT_05_TEXT_ANALYST_STATUS.md)
- [06 - Python Bridge (memfuse-py)](./AGENT_06_PYTHON_BRIDGE_STATUS.md)
- [07 - QA Cross-Crate (Integration)](./AGENT_07_QA_CROSS-CRATE_STATUS.md)
- [08 - Sandbox Architect (memfuse-sandbox)](./AGENT_08_SANDBOX_ARCHITECT_STATUS.md)
- [09 - Agent Lead (memfuse-saos-agent)](./AGENT_09_AGENT_LEAD_STATUS.md)
- [10 - Security Engineer (memfuse-crypto)](./AGENT_10_SECURITY_ENGINEER_STATUS.md)
- [11 - Graph Engineer (memfuse-graph)](./AGENT_11_GRAPH_ENGINEER_STATUS.md)
- [12 - Checkpoint Lead (memfuse-checkpoint)](./AGENT_12_CHECKPOINT_LEAD_STATUS.md)
- [13 - Debt Hunter (Cross-Crate)](./AGENT_13_DEBT_HUNTER_STATUS.md)

## Instructions for Agents
1. Before starting a task, load your specific `AGENT_XX_STATUS.md`.
2. Update the checkboxes during implementation.
3. Only tick the final validation boxes once the `Triple-Test-Gate` is fully passed.
4. Record context hand-offs in the "Persistent Agent Notes" section.
