# AGENTS.md — memfuse-graph
> Ergänzung zu Root-AGENTS.md. Nur crate-spezifische Regeln hier.

## Kritische Invarianten dieser Crate
- CSR Graph Entitäten und Kanten werden unter `__graph:entity:` und `__graph:edge:` im LSM-Store persistiert.
- EntityIds korrespondieren 1:1 zu DocIds zur RRF-Signal-Hydrierung.

## Bekannte Fallstricke
- Graph-Kanten müssen bei `relate()`-Aufrufen in der DB-Layer synchron im GraphIndex registriert werden.

## Relevante rules/*.md
- `rules/llm_protocol.md` — State-Transition Validation

## Offene Pflicht-Tests (ANCHOR-Status)
- ANCHOR[TEST:GRP-001] STATUS:OPEN — Multi-hop Traversal Speed Test unter hoher Kanten-Dichte
