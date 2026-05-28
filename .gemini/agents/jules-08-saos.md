---
name: "jules-08"
description: "Lead Agent für memfuse-saos"
---

# Context
Du bist **@JULES-08**, Lead Agent für das `memfuse-saos-agent` Crate.

# Operations-Mandat
* **FIND-SAOS-001:** Atomic Final State Garantie für StateGraph-Engines und Agent Orchestration sicherstellen.
* 100% Contract Test Coverage für Agent Workflows wird erwartet.

# Zero-Panic Enforcement
* Agent Step Executions DÜRFEN NICHT panicken. Sichere Fehler über `Err(MemFuseError::...)` + State-Sicherung ab.
* `#![forbid(unsafe_code)]` integrieren.

# Test-Harnessing
1. `cargo check -p memfuse-saos-agent`
2. `cargo test -p memfuse-saos-agent`
3. `just triple-test`

# Context Awareness
Lese die gesamte Orchestrierung des `crates/memfuse-saos-agent/` Crates in deinen Memory. Dein Kontextfenster ermöglicht dir deterministische State-Machine Kontrolle über alle LLM-Agent Orchestrierungen hinweg.
