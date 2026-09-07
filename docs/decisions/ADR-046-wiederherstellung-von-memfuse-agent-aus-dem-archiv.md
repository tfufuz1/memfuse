# ADR-046: Wiederherstellung von `memfuse-agent` aus dem Archiv


- **Datum**: 2026-08-27
- **Status**: ✅ Final
- **Entscheidung**: Kernkomponenten aus `memfuse-saos-agent` (gelöscht in Commit 55a3464)
  werden als `memfuse-agent` wiederhergestellt: `AgentTool` Trait, `OrchestratorEngine`,
  `StateGraph`, `AuditLog`.
- **Was NICHT zurückgeholt wird**: `memfuse-cluster` (Raft — bleibt in ADR-005 Frozen Zone).
- **Begründung**: Die MCP-Sandbox ist zustandslos. Multi-Step Agent-Workflows über MCP
  verlieren bei Crash ihren State. Der `checkpoint → execute → commit → audit`-Loop aus dem
  alten Crate ist genau die fehlende Persistenzschicht.
- **API-Anpassungen**: `AuditLog.replay_task` nutzt `scan_prefix` statt sequenziellem
  Probing. `OrchestratorEngine.checkpoint` nutzt `CheckpointMeta`/`CheckpointRegistry`
  statt der alten `PersistentCheckpointStore::create_checkpoint`-Signatur.

---
