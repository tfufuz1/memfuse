# AGENTS.md — memfuse-checkpoint
> Ergänzung zu Root-AGENTS.md. Nur crate-spezifische Regeln hier.

## Kritische Invarianten dieser Crate
- `CheckpointCoordinator` Trait (ADR-011) ist der kanonische Einstiegspunkt für Checkpoint-Management.
- RAII `CheckpointGuard` stellt automatisches Rollback bei unbehandelten Fehlern sicher (ADR-015).

## Bekannte Fallstricke
- Direkter Import von Storage-Implementierungen vermeiden; gegen `StorageEngine` Trait abstrahieren.

## Relevante rules/*.md
- `rules/async-io.md` — Transaction & Checkpoint Durability

## Offene Pflicht-Tests (ANCHOR-Status)
- ANCHOR[TEST:CKPT-001] STATUS:IN-PROGRESS AGENT:Jules — Concurrent Checkpoint Pinning & GC Exclusions
  // REVIEW-PASS[1/2] STATUS:PASS (ID: TEST:CKPT-001) (TS: 2026-09-01T23:09:00Z) (SESSION: fdf7a62e)
