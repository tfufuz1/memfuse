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

MemFuse besteht aus 14 Workspace-Crates (13 Kern-Crates + 1 optionales Crate `memfuse-embed`) in einer 5-Schichten-Architektur (Layer 0–4).

Die vollständige, automatisch aktuell gehaltene Crate-Tabelle und DAG-Topologie ist in [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) dokumentiert.

## 4. Non-Obvious Decisions (would cause wrong code without this knowledge)

- **TxId generation**: ALWAYS `collection.allocate_tx()` — NEVER `SystemTime::as_nanos()`
- **fsync errors**: ALWAYS propagate with `?` — NEVER `let _ = dir.sync_all()`
- **unsafe scope**: ONLY in `memfuse-index/src/distance.rs` (SIMD), `memfuse-index/src/diskann.rs` (Mmap, ADR-017) and `memfuse-index/src/persistence.rs` (Mmap, ADR-017). Every `unsafe` block requires `// SAFETY:` proof.
- **AI-TAG[SMELL][CRITICAL]**: ALWAYS fix immediately — never just comment
- **Document chunking**: ALWAYS use `MarkdownChunker` — NEVER embed entire text as 1 vector
- **MCP transport**: stdio JSON-RPC 2.0 ONLY — axum was removed (ADR-010)
- **WAL HMAC key**: ALWAYS via `load_or_create_integrity_key()` — NEVER hardcoded
- **AI-TAG & ID Schema**: Alle neuen Tags verwenden das hash-basierte Schema `AGT-<CRATE>-<8-hex-hash>` (z.B. `AGT-STORE-a3f29c1d`). Bestehende `AGT-<CRATE>-NNN` IDs haben Bestandsschutz.
- **Tag-Zeitstempel- & Session-Pflicht**: Alle `AI-TAG`, `ANCHOR` und `REVIEW-PASS` Kommentare tragen zwingend sekundengenaue ISO-8601-UTC-Zeitstempel im Format `(TS: YYYY-MM-DDTHH:MM:SSZ)` und das `(SESSION: <8-hex-hash>)` Token (siehe `rules/tag_taxonomy.md`).

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
bereits in der Setup-Ausgabe, siehe `[9/9] Session Context Digest` und `[10/10] Session identity`):
0. SESSION-Hash aus Environment-Setup (`SESSION:<hash>`) übernehmen und für alle Tags dieser Sitzung konsistent verwenden.
1. Session-Digest aus Environment-Setup lesen (offene BLOCKER/CRITICAL Tags,
   offene ANCHORs, letzte 3 ADRs, WORKING_STATE.md-Tail)
2. Falls Digest nicht sichtbar (z.B. bei nachträglichem Reconnect):
   manuell `just session-context` ausführen (siehe justfile)

Jede Sitzung MUSS mit folgendem enden — VOR dem letzten Commit:
1. `just sync-docs` ausführen (`WORKING_STATE.md`, `docs/CHANGELOG.md`, `docs/ARCHITECTURE.md`, `docs/SOURCE_OF_TRUTH.md` werden vollständig aus Inline-Tags & Cargo-Topologie generiert).
2. `WORKING_STATE.md` enthält NULL manuell editierten Text mehr. Bei Git-Merge-Konflikten in `WORKING_STATE.md`: stets durch `just sync-docs` auflösen.
3. Falls diese Sitzung eine REINE REVIEW-Sitzung war (kein eigener Code-Beitrag, nur Prüfung fremder Arbeit): mindestens einen `REVIEW-PASS`-Eintrag mit `PRÜFER-KONTEXT: FRESH` hinterlassen, bevor `just sync-docs` läuft.
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
