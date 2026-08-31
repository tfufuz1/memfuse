# AGENTS.md — memfuse-core
> Ergänzung zu Root-AGENTS.md. Nur crate-spezifische Regeln hier.

## Kritische Invarianten dieser Crate
- Layer 0 Kernel Isolation: Keine I/O, kein async, keine Abhängigkeiten zu anderen Workspace-Crates.
- Trait-Abwärtskompatibilität: Neue Trait-Methoden benötigen Default-Implementierungen.

## Bekannte Fallstricke
- DocId 64-Bit Hash-Ableitung (`DocId::from_key`) wahrt u64-Kompatibilität für Indizes.

## Relevante rules/*.md
- `rules/llm_protocol.md` — Schleife 1 (Read-Before-Write für Core-API Signatures)

## Offene Pflicht-Tests (ANCHOR-Status)
- ANCHOR[TEST:CORE-001] STATUS:DONE (TS:2026-08-29T00:00:00Z) — Benchmark und Collisions-Tests für DocId Key-Trunkierung
