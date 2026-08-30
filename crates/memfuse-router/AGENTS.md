# memfuse-router — Crate-Level Agent Rules

## Critical Invariants

### Hybrid Search & Community Scoring Loop
- Queries execute hybrid search on L2 Collection (`hybrid_search_with_strategy`).
- Community assignment via `EntityId` applies 1.2x score boost to profiles sharing domain communities.
- Candidate context window is trimmed to match the target `SlmProfile` token budget using `ContextManager`.

### Routing Decision & Dispatch Strategy
- Returns `RoutingDecision` containing selected `SlmProfile` and prepared `ContextWindow`.
- Rejects routing with `MemFuseError::NotFound` if no SLM profiles are configured or no search results match.
- `dispatch_to_slm` handles stdio JSON-RPC MCP routing boundary.

## Layer Position
Layer 3. Darf importieren: memfuse-db (L2), memfuse-store (L1), memfuse-core (L0). Darf NICHT importieren: memfuse-tauri (L4).

## Nicht-offensichtliche Entscheidungen
- Pure Rust routing engine avoiding heavy external routing dependencies.
- Token budgets and relevance thresholds enforced dynamically per profile.
