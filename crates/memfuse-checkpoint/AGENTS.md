# AGENTS.md — memfuse-checkpoint
> Ergänzung zu Root-AGENTS.md. Nur crate-spezifische Regeln hier.

## Kritische Invarianten dieser Crate
- `CheckpointCoordinator` Trait (ADR-011) ist der kanonische Einstiegspunkt für Checkpoint-Management.
- RAII `CheckpointGuard` stellt automatisches Rollback bei unbehandelten Fehlern sicher (ADR-015).

## Bekannte Fallstricke
- Direkter Import von Storage-Implementierungen vermeiden; gegen `StorageEngine` Trait abstrahieren.

## Relevante rules/*.md
- `rules/async-io.md` — Transaction & Checkpoint Durability

## Pflicht-Tests (ANCHOR-Status)
- ANCHOR[TEST:CKPT-001] STATUS:DONE (TS:2026-08-31T21:12:57Z) (SESSION:14348074) — Concurrent Checkpoint Pinning & GC Exclusions
