# AGENTS.md — MemFuse Operative Agent Rules
> Version 5.0 · 2026-08-24 · Ambient context — always loaded

## 1. Mission

MemFuse is an embedded 4-signal memory engine & RAG desktop app for local AI agents.
**Non-negotiable constraints**: Pure-Rust sovereign core (no C-deps except optional
ONNX feature-gate in memfuse-embed), no Docker, no HTTP server as production component.
All errors propagate via `MemFuseError` + `?` — zero silent failures.

## 2. Toolchain

| Action | Command | Note |
|---|---|---|
| Compile check | `cargo check --workspace --exclude memfuse-tauri` | |
| Test suite | `cargo test --workspace --exclude memfuse-tauri` | Before every commit |
| Lint + format | `just check` | Clippy + rustfmt — all style rules live here |
| Flaky detection | `just triple-test` | Runs test suite 3× |
| DAG enforcement | `just dag-check` | Layer dependency validation |
| Debt scan | `just debt-audit` | Scans unwrap/expect/std::fs |

## 3. Workspace Inventory (14 Crates)

13 active kernel crates + 1 optional crate in a 5-layer DAG:

| Layer | Crates |
|---|---|
| 0 | `memfuse-core` — shared kernel, no I/O, no async |
| 1 | `memfuse-store` (LSM), `memfuse-index` (HNSW), `memfuse-text` (BM25), `memfuse-crypto` (AES-256-GCM), `memfuse-graph` (CSR), `memfuse-checkpoint` (snapshot) |
| 2 | `memfuse-db` — orchestrator, 4-signal fusion |
| 3 | `memfuse-py` (PyO3), `memfuse-ollama` (Ollama HTTP embeddings), `memfuse-agent` (persistent agent workflow engine) |
| 4 | `memfuse-mcp` (**stdio JSON-RPC 2.0 only** — ADR-010, no HTTP), `memfuse-tauri` (desktop app) |

**Optional**: `memfuse-embed` — ONNX embeddings, feature-gated (`default = []`), Layer 3.
Pure-Rust USP preserved by keeping default features empty.

## 4. Non-Obvious Decisions (would cause wrong code without this knowledge)

- **TxId generation**: ALWAYS `collection.allocate_tx()` — NEVER `SystemTime::as_nanos()`
- **fsync errors**: ALWAYS propagate with `?` — NEVER `let _ = dir.sync_all()`
- **unsafe scope**: ONLY in `memfuse-index/src/distance.rs` (SIMD), `memfuse-index/src/diskann.rs` (Mmap, ADR-017) and `memfuse-index/src/persistence.rs` (Mmap, ADR-017). Every `unsafe` block requires `// SAFETY:` proof.
- **AI-TAG[SMELL][CRITICAL]**: ALWAYS fix immediately — never just comment
- **Document chunking**: ALWAYS use `MarkdownChunker` — NEVER embed entire text as 1 vector
- **MCP transport**: stdio JSON-RPC 2.0 ONLY — axum was removed (ADR-010)
- **WAL HMAC key**: ALWAYS via `load_or_create_integrity_key()` — NEVER hardcoded
- **AI-TAG & ID Nummernkreise**: Jedes `AI-TAG` verwendet das Schema `AGT-<CRATE>-<NNN>` (z.B. `AGT-CORE-001`, `AGT-STORE-001`, `AGT-INDEX-001`, `AGT-TEXT-001`, `AGT-CRYPTO-001`, `AGT-GRAPH-001`, `AGT-CKPT-001`, `AGT-DB-001`, `AGT-EMBED-001`, `AGT-OLLAMA-001`, `AGT-PY-001`, `AGT-TAURI-001`, `AGT-MCP-001`, `AGT-AGENT-001`).
- **Tag-Zeitstempel-Pflicht**: Alle `AI-TAG` und `ANCHOR` Kommentare tragen zwingend einen ISO-8601-UTC-Zeitstempel im Format `(TS: YYYY-MM-DDTHH:MM:SSZ)` (siehe `rules/tag_taxonomy.md`).

## 5. Judgment Boundaries

**ALWAYS** (no confirmation needed):
- Check ADR list for conflicts before any code change
- After every session: update `WORKING_STATE.md`
- Propagate all errors — never swallow with `let _ =`
- Read nearest crate-level `AGENTS.md` before editing a crate

**ASK** (require human confirmation):
- Add new external dependencies
- Write or supersede an ADR
- Change public API signatures
- Add `unsafe` code (except in approved files above)

**NEVER**:
- `let _ =` for IO ops (`sync_all`, `flush`, `write`)
- `SystemTime` for TxId generation
- HTTP in `memfuse-mcp` (stdio only, ADR-010)
- `.expect()` in production code (not `#[cfg(test)]`)
- Codebase-wide refactorings without explicit ADR

## 6. Session Protocol

Jede Sitzung MUSS mit folgendem beginnen (Environment-Skript liefert dies
bereits in der Setup-Ausgabe, siehe `[9/9] Session Context Digest`):
1. Session-Digest aus Environment-Setup lesen (offene BLOCKER/CRITICAL Tags,
   offene ANCHORs, letzte 3 ADRs, WORKING_STATE.md-Tail)
2. Falls Digest nicht sichtbar (z.B. bei nachträglichem Reconnect):
   manuell `just session-context` ausführen (siehe justfile)

Jede Sitzung MUSS mit folgendem enden — VOR dem letzten Commit:
1. `just sync-docs` ausführen (aktualisiert WORKING_STATE.md,
   docs/ARCHITECTURE.md, docs/SOURCE_OF_TRUTH.md automatisch aus Inline-Tags)
2. Diff der generierten Abschnitte prüfen — falls unerwartet groß oder
   falsch: Tags im Code korrigieren, NICHT den generierten Text von Hand
   überschreiben (sonst nächster Lauf überschreibt die Handkorrektur wieder)
3. Neuen manuellen Freitext-Eintrag in `WORKING_STATE.md` NUR außerhalb der
   `<!-- AUTO-GENERATED -->`-Marker ergänzen (z.B. Sprint-Zusammenfassung)
4. `just sync-docs-check` als letzten Schritt — muss grün sein, sonst ist
   der Commit nicht vollständig

## 7. Governance Documents (on-demand, not ambient)

| Document | Read when |
|---|---|
| `CONSTITUTION.md` | Writing ADR, API design, security changes, exit criteria |
| `docs/SOURCE_OF_TRUTH.md` | Checking crate status, inventory, architecture topology |
| `docs/ARCHITECTURE.md` | Understanding layer boundaries, invariant status |
| `DECISIONS.md` | Before any architectural change |
| `rules/*.md` | Domain-specific rules (SIMD safety, WAL crypto, testing, etc.) |
