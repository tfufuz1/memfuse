# MemFuse — Jules Session Bootstrap
> Maschinenausführbare Checkliste. Jede Session MUSS mit dieser
> Sequenz beginnen, bevor Code geschrieben oder Dateien geändert werden.

- **VETOES.md** (Root): Permanent abgelehnte oder eingeschränkt akzeptierte Features.
  Vor jeder neuen Feature-Implementierung mit "F-NN"-Bezeichnung prüfen ob ein
  Eintrag existiert. `just check-vetoes` läuft automatisch, ist aber kein Ersatz
  für manuelles Lesen vor Arbeitsbeginn an physio-*/Nucleation-artigen Features.

## Phase 0 — Session-Identität etablieren (30 Sekunden)

**Primärquelle:** Das Environment-Setup-Skript liefert SESSION_HASH und TS bereits
unter `[10/10] Session Identity`. Nutze diese Werte direkt.

Falls kein Setup-Skript gelaufen ist (z.B. manueller Start):

```bash
# SESSION-Hash generieren (verwende diesen für ALLE Tags dieser Session)
SESSION_HASH=$(date -u +%Y%m%d%H%M%S | sha256sum | head -c 8)
echo "SESSION: $SESSION_HASH"

# Aktuellen Timestamp ermitteln
TS=$(date -u +%Y-%m-%dT%H:%M:%SZ)
echo "TS: $TS"
```

**Konsistenzregel:** Ein Session-Hash MUSS für die gesamte Sitzung konsistent bleiben.
Niemals mid-session neu generieren — außer nach explizitem Neustart des Environments.

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
| ADR schreiben | `docs/decisions/` (letzte 5 ADRs lesen), `CONSTITUTION.md §Governance` |
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

## Phase 6 — Pre-Submit Gate (BLOCKIEREND — kein Submit ohne ✅)

> **Invariante:** Jede dieser Prüfungen muss explizit bestätigt sein, bevor
> `submit` aufgerufen oder ein PR erstellt wird. Bei Fehlschlag: STOP, Fix,
> Phase 6 von vorne.

```bash
# ── 6.1 REBASE-CHECK: Wurde gegen aktuellen main getestet? ──────────────────
git fetch origin main
if ! git merge-base --is-ancestor origin/main HEAD; then
    echo "❌ STOP: main hat sich weiterentwickelt seit Sessionbeginn."
    echo "   Ausführen: git rebase origin/main && cargo check --workspace --exclude memfuse-tauri"
    echo "   Danach Phase 6 erneut durchlaufen."
    exit 1
fi
echo "✅ 6.1 Branch ist aktuell gegenüber origin/main."

# ── 6.2 COMPILE-VERIFIKATION: Nicht aus Erinnerung — live ausführen ──────────
echo "→ 6.2 Compile-Gate..."
if ! cargo check --workspace --exclude memfuse-tauri --quiet; then
    echo "❌ STOP: Compile-Fehler. NICHT submitten."
    exit 1
fi
echo "✅ 6.2 Workspace kompiliert."

# ── 6.3 PHANTOM-FILE-CHECK: PR-Beschreibung vs. tatsächlicher Diff ───────────
# Liste aller in der Commit-Message behaupteten .rs-Dateien prüfen
# Ersetze DEINE_DATEILISTE durch die im PR-Body genannten Dateien.
DIFF_FILES=$(git diff --name-only origin/main...HEAD)
echo "→ 6.3 Phantom-File-Check. Dateien im Diff:"
echo "$DIFF_FILES"
echo "MANUELL PRÜFEN: Stimmen alle im PR-Body genannten Dateien mit obiger Liste überein?"
echo "Bei Abweichung: PR-Body korrigieren, NIEMALS nicht-existente Dateien behaupten."

# ── 6.4 CLAIM-RELEASE: Claim für bearbeiteten Scope freigeben ────────────────
# Ersetze CRATE_NAME durch den tatsächlich bearbeiteten Crate.
# cargo xtask claim --release --crate <CRATE_NAME>
echo "→ 6.4 Claim-Release ausführen (Crate einsetzen):"
echo "   cargo xtask claim --release --crate <DEIN_CRATE>"

# ── 6.5 TEST-SMOKE: Kein Submit ohne zumindest doc-tests ─────────────────────
echo "→ 6.5 Smoke-Test (doc-tests, schnell)..."
cargo test --doc --workspace --exclude memfuse-tauri --quiet 2>&1 | tail -5
echo "✅ 6.5 Doc-Tests bestanden."

echo ""
echo "════════════════════════════════════════════════"
echo "✅ PHASE 6 BESTANDEN — Submit erlaubt."
echo "════════════════════════════════════════════════"
```

> **Regel für Commit-Messages:** Jede in der PR-Beschreibung unter
> "Hinzugefügt" oder "Getestet" genannte Datei MUSS in `git diff --name-only
> origin/main...HEAD` erscheinen. Ausnahmen begründen, nie stillschweigend weglassen.

## Notfall-Eskalation (Prompt-Thrashing)

Wenn derselbe Compiler-Fehler nach 2 Iterationen nicht behoben ist:
1. **STOPP** — keinen weiteren Code schreiben
2. Fehler auf minimales Beispiel reduzieren
3. Fehlermeldung + Diff in Session-Log dokumentieren
4. Entwickler um explizite Instruktion bitten
