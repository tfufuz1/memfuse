# MemFuse — Jules Session Bootstrap
> Maschinenausführbare Checkliste. Jede Session MUSS mit dieser
> Sequenz beginnen, bevor Code geschrieben oder Dateien geändert werden.

## Phase 0 — Session-Identität etablieren (30 Sekunden)

```bash
# 1. SESSION-Hash generieren (verwende diesen für ALLE Tags dieser Session)
SESSION_HASH=$(date -u +%Y%m%d%H%M%S | sha256sum | head -c 8)
echo "SESSION: $SESSION_HASH"

# 2. Aktuellen Timestamp ermitteln
TS=$(date -u +%Y-%m-%dT%H:%M:%SZ)
echo "TS: $TS"
```

## Phase 1 — Offene Kritische Issues prüfen (60 Sekunden)

```bash
# BLOCKER und CRITICAL Tags — bei Fund: STOP, zuerst beheben
echo "=== BLOCKER/CRITICAL AI-TAGs ==="
grep -rn "AI-TAG\[.*\]\[BLOCKER\]\|AI-TAG\[.*\]\[CRITICAL\]" crates/ \
  --include="*.rs" | grep -v "RESOLVED" || echo "  ✅ Keine"

# Offene ANCHORS mit IN-PROGRESS Status
echo "=== IN-PROGRESS ANCHORS ==="
grep -rn "ANCHOR\[.*\] STATUS:IN-PROGRESS" crates/ \
  --include="*.rs" || echo "  (keine)"

# WORKING_STATE.md lesen (autogeneriert, immer aktuell)
echo "=== WORKING STATE ==="
head -50 WORKING_STATE.md
```

## Phase 2 — Toolchain verifizieren (30 Sekunden)

```bash
# Verifiziere Build-Grundlage (ohne Nix-Shell zuerst probieren)
cargo check --workspace --exclude memfuse-tauri 2>&1 | tail -5

# Falls cargo nicht im PATH: Rust-Toolchain aktivieren
# source "$HOME/.cargo/env" && cargo check --workspace --exclude memfuse-tauri
```

## Phase 3 — Aufgaben-spezifischen Kontext laden

Lade basierend auf der Aufgabe:

| Aufgabe-Typ | Zu lesende Dateien |
|-------------|-------------------|
| Code in `memfuse-store/*` | `crates/memfuse-store/AGENTS.md`, `rules/wal_crypto.md`, `rules/async-io.md` |
| Code in `memfuse-index/*` | `crates/memfuse-index/AGENTS.md`, `rules/simd_safety.md` |
| Code in `memfuse-db/*` | `crates/memfuse-db/AGENTS.md` |
| Neue Dependency | `rules/dependencies.md` → Cargo.lock prüfen → crates.io verifizieren |
| Neue API-Oberfläche | `CONSTITUTION.md`, `docs/TYPE_REGISTRY.md` |
| ADR schreiben | `DECISIONS.md` (letzte 5 ADRs lesen), `CONSTITUTION.md §Governance` |
| Tests schreiben | `rules/testing.md`, `rules/test_quality.md` |
| unsafe Code | `rules/simd_safety.md` — NUR in approved files (AGENTS.md §4) |
| Crypto/WAL | `rules/wal_crypto.md` → WAL-First-Regel verifizieren |

## Phase 4 — Pre-Write-Check (vor JEDER Code-Änderung)

```bash
# API-Halluzinations-Schutz: Signatur vor Nutzung verifizieren
# Beispiel: Bevor du eine Methode auf Collection aufrufst:
grep -n "pub fn <METHODE>" crates/memfuse-db/src/collection.rs

# Typ-Dopplungs-Schutz: Typ-Register prüfen
grep "<TYPNAME>" docs/TYPE_REGISTRY.md

# DAG-Prüfung: Keine Layer-Verletzung
# Layer 0 darf nicht von Layer 1+ importieren, etc.
```

## Phase 5 — Session-Ende (VOR letztem Commit)

```bash
# 1. Format & Lint (Formatierung erzwingen + Clippy/Check)
cargo fmt --all
just check

# 2. DAG-Integrität & Tech-Debt Audit
just dag-check
just debt-audit

# 3. Tests
just test

# 4. Sync-Docs (generiert WORKING_STATE.md, CHANGELOG, etc.)
just sync-docs

# 5. Finaler Check
just sync-docs-check
```

## Notfall-Eskalation (Prompt-Thrashing)

Wenn derselbe Compiler-Fehler nach 2 Iterationen nicht behoben ist:
1. **STOPP** — keinen weiteren Code schreiben
2. Fehler auf minimales Beispiel reduzieren
3. Fehlermeldung + Diff in Session-Log dokumentieren
4. Entwickler um explizite Instruktion bitten
