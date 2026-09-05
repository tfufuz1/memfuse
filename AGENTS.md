# AGENTS.md — MemFuse Operative Agent Rules
> Version 6.0 · 2026-08-29 · Ambient context — always loaded

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
| Cross-platform CI | GitHub Actions (`test-cross-platform`) | Informativer Check auf Windows/macOS für Kern-Crates; blockiert PRs nicht, bei Rot manuell prüfen vor Release-Tag |

> **Hinweis:** Alle `just`-Rezepte funktionieren sowohl mit als auch ohne installiertes `nix` — bei fehlendem `nix` wird automatisch auf direkte `cargo`-Aufrufe zurückgefallen.

## 3. Workspace Inventory (15 Crates)

MemFuse besteht aus 15 Workspace-Crates (14 Kern-Crates + 1 optionales Crate `memfuse-embed`) in einer 5-Schichten-Architektur (Layer 0–4).

Die vollständige, automatisch aktuell gehaltene Crate-Tabelle und DAG-Topologie ist in [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) dokumentiert.

## 4. Non-Obvious Decisions (would cause wrong code without this knowledge)

- **TxId generation**: ALWAYS `collection.allocate_tx()` — NEVER `SystemTime::as_nanos()`
- **fsync errors**: ALWAYS propagate with `?` — NEVER `let _ = dir.sync_all()`
- **unsafe scope**: ONLY in `memfuse-index/src/distance.rs` (SIMD, ADR-017/ADR-034), `memfuse-index/src/diskann.rs` (Mmap, ADR-017) and `memfuse-index/src/persistence.rs` (Mmap, ADR-017). Exception: test-only unsafe in `memfuse-crypto/src/anti_tamper.rs` exclusively for Zeroize drop-semantics verification via raw pointer inspection. Production builds are unsafe-free via `#![cfg_attr(not(test), forbid(unsafe_code))]`.
- **AI-TAG[SMELL][CRITICAL]**: ALWAYS fix immediately — never just comment
- **Document chunking**: ALWAYS use `MarkdownChunker` — NEVER embed entire text as 1 vector
- **MCP transport**: stdio JSON-RPC 2.0 ONLY — axum was removed (ADR-010)
- **WAL HMAC key**: ALWAYS via `load_or_create_integrity_key()` — NEVER hardcoded
- **AI-TAG & ID Schema**: Alle neuen Tags verwenden das hash-basierte Schema `AGT-<CRATE>-<8-hex-hash>` (z.B. `AGT-STORE-a3f29c1d`). Bestehende `AGT-<CRATE>-NNN` IDs haben Bestandsschutz.
- **Tag-Zeitstempel- & Session-Pflicht**: Alle `AI-TAG`, `ANCHOR` und `REVIEW-PASS` Kommentare tragen zwingend sekundengenaue ISO-8601-UTC-Zeitstempel im Format `(TS: YYYY-MM-DDTHH:MM:SSZ)` und das `(SESSION: <8-hex-hash>)` Token (siehe `rules/tag_taxonomy.md`).
- **Trait-Default-Pflichttest**: Für jedes `pub trait` mit einer Default-Methode-Implementierung MUSS im selben PR, der einen neuen Implementor dieses Traits hinzufügt, ein Integrationstest existieren, der beweist, dass die Default-Implementierung NICHT still greift (entweder weil sie explizit überschrieben wurde, oder weil ein Test explizit den Default-Fehlerpfad als erwartetes, dokumentiertes Verhalten prüft). Referenz im Code: `capability_coverage` in `crates/memfuse-core/src/traits.rs` (prüft z.B. `VectorIndex::search_at` & `GraphIndex::traverse_at`).
- **Typ-Dopplungs-Prävention**: Vor Anlegen eines neuen Typs oder Traits: `docs/TYPE_REGISTRY.md` nach ähnlichem Namen/Zweck durchsuchen. Bei Kollision: bestehenden Typ erweitern statt Duplikat anlegen, oder Kollision explizit per ADR begründen.
- **Audit-Finding-Verifikation**: Jeder Finding aus einem extern zugelieferten Audit-Dokument oder Prompt MUSS vor Implementierung am AKTUELLEN Quellcode gegengelesen werden (siehe `.jules/AUDIT_INTAKE_PROTOCOL.md`). Falls der Finding nicht mehr zutrifft (Code bereits geändert, Test existiert bereits, Fix bereits gemerged): Finding im PR-Kommentar/Log explizit als "entkräftet" markieren mit Begründung — NICHT stillschweigend ignorieren und NICHT blind implementieren.
- **Sync-Docs Nix-Fallback**: `just sync-docs` verwendet `nix develop -c` — bei fehlendem Nix direkt `cargo xtask sync-docs` aufrufen. Beide Pfade sind in der justfile mit `||`-Fallback abgesichert.
- **Keine HTTP in memfuse-mcp**: Laut ADR-010 ausschließlich stdio JSON-RPC 2.0. Das GLOSSARY.md definierte dies fälschlicherweise als HTTP/JSON-RPC — die korrekte Definition gilt aus ADR-010 und AGENTS.md, nicht aus dem Glossar (wenn Konflikt).
- **Typ-Existenz vor Anlage prüfen**: `find crates/ -name "*.rs" | xargs grep -l "<TYPNAME>"` und `grep "<TYPNAME>" docs/TYPE_REGISTRY.md` ausführen, bevor ein neuer Typ angelegt wird.
- **ADR-Nummernvergabe**: Vor Vergabe einer neuen ADR-Nummer IMMER `grep -oP '(?<=^## ADR-)\d+' DECISIONS.md | sort -n | tail -1` live ausführen, NIEMALS eine Nummer aus einem älteren Prompt oder einer älteren Analyse übernehmen (schützt vor Duplikaten durch parallele Sessions, siehe ADR-020, ADR-046).

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
0. (Pre-Step) `.jules/SESSION_BOOTSTRAP.md` vollständig ausführen.
   Bei fehlendem Environment-Setup-Skript: SESSION_HASH manuell generieren
   via `date -u +%Y%m%d%H%M%S | sha256sum | head -c 8`.
1. SESSION-Hash aus Environment-Setup (`SESSION:<hash>`) übernehmen und für alle Tags dieser Sitzung konsistent verwenden.
2. Session-Digest aus Environment-Setup lesen (offene BLOCKER/CRITICAL Tags,
   offene ANCHORs, letzte 3 ADRs, WORKING_STATE.md-Tail)
2. Falls Digest nicht sichtbar (z.B. bei nachträglichem Reconnect):
   manuell `just session-context` ausführen (siehe justfile)

Jede Sitzung MUSS mit folgendem enden — VOR dem letzten Commit:
0. `cargo fmt --all` ausführen (nicht nur `--check`) — der Pre-Commit-Hook tut dies automatisch, aber bei Hook-Bypass (`git commit --no-verify`) MUSS dieser Schritt manuell nachgeholt werden, bevor der PR geöffnet wird.
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
| `docs/TYPE_REGISTRY.md` | Register central domain types & traits before creating new ones |
| `.jules/AUDIT_INTAKE_PROTOCOL.md` | Verifying incoming external audit findings before implementation |
| `.jules/SESSION_BOOTSTRAP.md` | Maschinenausführbare Session-Checkliste | Immer zu Beginn |
| `.jules/COMMON_LLM_ERRORS.md` | Häufige LLM-Fehler und Korrekturen | Bei Unsicherheit über Korrektheit |
| `.jules/JULES_CONTEXT.md` | Freshness automatisch geprüft via `xtask check-jules-context-freshness` (Gate 10) |
| `rules/*.md` | Domain-specific rules (SIMD safety, WAL crypto, testing, chaos testing, etc.) |
