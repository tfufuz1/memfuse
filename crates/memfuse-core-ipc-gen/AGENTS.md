# AGENTS.md — memfuse-core-ipc-gen

## 1. Zweck & Architekturrolle

Auto-generated FlatBuffers IPC code for MemFuse Core (Layer 0).

## 2. Modul-Karte

| Datei | Verantwortung |
|---|---|
| `lib.rs` | Re-exports generated FlatBuffers modules |
| `memfuse_generated.rs` | Auto-generated FlatBuffers IPC bindings |

## 3. Kritische Invarianten

1. **Layer 0 Placement**: Pure schema bindings with no upstream dependencies.
2. **Auto-generated**: Modifying `memfuse_generated.rs` directly is prohibited; update the FlatBuffers schema instead.

## 4. Public API Quick-Reference

FlatBuffers generated types and structs.

## 5. Anti-Patterns & LLM-Fallstricke

Do not edit `memfuse_generated.rs` manually.

## 6. Concurrency & Lock-Hierarchie

No locks. Pure data serialization types.

## 7. Cross-Crate-Schnittstellen & DAG-Grenzen

Used by `memfuse-core`.

## 8. Relevante ADRs & Rules

ADR-045 (IPC Schema).
