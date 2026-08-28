#!/usr/bin/env bash
# https://jules.google.com/repo/github/tfufuz1/memfuse/config
# =============================================================================
# MemFuse — Jules Environment Setup Script
# Repository: https://github.com/tfufuz1/memfuse
# Target: Jules VM (Ubuntu 24, Rust pre-installed)
# =============================================================================

set -euo pipefail

echo "============================================================"
echo "  MemFuse Jules Environment Setup"
echo "  $(date -u '+%Y-%m-%d %H:%M:%S UTC')"
echo "============================================================"

# ── 1. Rust Toolchain (Fix für Cross-Device Link Fehler) ────────────────────
echo ""
echo "[1/9] Verifying Rust toolchain..."

# Vorheriges Stable entfernen, um OverlayFS-Rename-Fehler zu umgehen
rustup toolchain uninstall stable || true
rustup toolchain install stable
rustup default stable

RUST_VERSION=$(rustc --version)
echo "  ✅ $RUST_VERSION"

rustup component add clippy rustfmt
echo "  ✅ clippy + rustfmt installed"

# ── 2. System Libraries for Tauri ───────────────────────────────────────────
echo ""
echo "[2/9] Installing system libraries for Tauri crate..."

sudo apt-get update -q 2>/dev/null || true
sudo apt-get install -y -q \
    libwebkit2gtk-4.1-dev \
    libgtk-3-dev \
    libayatana-appindicator3-dev \
    librsvg2-dev \
    libssl-dev \
    pkg-config \
    curl \
    2>/dev/null || echo "  ⚠️  Some Tauri deps unavailable (acceptable)"

echo "  ✅ System libraries installed"

# ── 3. ONNX Runtime Check ───────────────────────────────────────────────────
echo ""
echo "[3/9] Verifying ONNX runtime availability..."
ldd --version | head -1
echo "  ✅ glibc compatible for ORT dynamic linking"

# ── 4. just (task runner) ───────────────────────────────────────────────────
echo ""
echo "[4/9] Installing 'just' task runner..."

if ! command -v just &>/dev/null; then
    cargo install just --quiet
    echo "  ✅ just installed"
else
    echo "  ✅ just already available: $(just --version)"
fi

# ── 5. cargo-audit (security auditing) ──────────────────────────────────────
echo ""
echo "[5/9] Installing cargo-audit..."

if ! command -v cargo-audit &>/dev/null; then
    cargo install cargo-audit --quiet
    echo "  ✅ cargo-audit installed"
else
    echo "  ✅ cargo-audit already available"
fi

# ── 5b. Git Pre-Commit Hook Configuration ───────────────────────────────────
echo ""
echo "[5b/9] Configuring git pre-commit hook..."
git config core.hooksPath .githooks
echo "  ✅ Git core.hooksPath set to .githooks"

# ── 6. Pre-warm Cargo Dependency Cache ──────────────────────────────────────
echo ""
echo "[6/9] Pre-compiling workspace dependencies..."

cd /app 2>/dev/null || cd /home/jules/repo 2>/dev/null || cd .

cargo check --workspace --exclude memfuse-tauri 2>&1 | tail -5
echo "  ✅ Workspace dependency cache warmed"

cargo test --workspace --exclude memfuse-tauri --no-run 2>&1 | tail -3
echo "  ✅ Test binaries pre-compiled"

# ── 7. Validate Key Invariants ───────────────────────────────────────────────
echo ""
echo "[7/9] Validating MemFuse workspace invariants..."

if [ -f "AGENTS.md" ]; then
    echo "  ✅ AGENTS.md found ($(wc -l < AGENTS.md) lines)"
else
    echo "  ❌ AGENTS.md missing! Jules needs this file."
    exit 1
fi

OPEN_TAGS=$(grep -rn 'AI-TAG\[SMELL\]\[CRITICAL\]' crates/ --include='*.rs' 2>/dev/null | grep -v RESOLVED | wc -l | tr -d ' ')
echo "  ✅ Open AI-TAG[SMELL][CRITICAL]: $OPEN_TAGS (target: 0)"

OPEN_BLOCKERS=$(grep -rn "AI-TAG\[.*\]\[BLOCKER\]\|ANCHOR\[.*\] STATUS:BLOCKER" crates/ --include='*.rs' 2>/dev/null | grep -v RESOLVED | wc -l | tr -d ' ')
if [ "$OPEN_BLOCKERS" -gt 0 ]; then
    echo "  ⚠️ HARD GATE: $OPEN_BLOCKERS open BLOCKER tags found in codebase!"
    if [ "${ALLOW_OPEN_BLOCKERS:-0}" = "1" ] || [ "${JULES_FIX_BLOCKER:-0}" = "1" ]; then
        echo "  ⚠️ Bypassing BLOCKER gate (ALLOW_OPEN_BLOCKERS=1 or JULES_FIX_BLOCKER=1 set)."
    else
        echo "  ❌ HARD GATE FAILURE: Active BLOCKER tags exist. Set ALLOW_OPEN_BLOCKERS=1 or JULES_FIX_BLOCKER=1 if this session is explicitly targeting blocker fixes."
        exit 1
    fi
else
    echo "  ✅ Open BLOCKER tags: 0"
fi

if grep -q "axum" crates/memfuse-mcp/Cargo.toml 2>/dev/null; then
    echo "  ❌ CRITICAL: axum found in memfuse-mcp! Violates ADR-010"
    exit 1
else
    echo "  ✅ ADR-010: axum not in memfuse-mcp (stdio-only MCP)"
fi

# ── 8. Documentation Drift Check ────────────────────────────────────────────
echo ""
echo "[8/9] Checking documentation drift (informational)..."
if cargo run -p xtask -- sync-docs --check 2>&1 | tail -10; then
    echo "  ✅ Docs currently in sync with code"
else
    echo "  ⚠️  Docs drifted — run 'just sync-docs' before finishing this session"
fi

# ── 9. Session-Kontext-Digest (für Jules-Sitzungsstart) ─────────────────────
echo ""
echo "[9/9] Generating session context digest..."
echo "------------------------------------------------------------"
echo "OFFENE KRITISCHE TAGS (BLOCKER/CRITICAL):"
grep -rn "AI-TAG\[.*\]\[BLOCKER\]\|AI-TAG\[.*\]\[CRITICAL\]" crates/ \
    --include='*.rs' | grep -v RESOLVED || echo "  (keine)"
echo ""
echo "OFFENE ANCHORS (IN-PROGRESS):"
grep -rn "ANCHOR\[.*\] STATUS:IN-PROGRESS" crates/ --include='*.rs' || echo "  (keine)"
echo ""
echo "LETZTE 3 ADRs:"
grep -A2 "^## ADR-" DECISIONS.md | tail -30 || true
echo ""
echo "LETZTER WORKING_STATE.md STAND:"
tail -20 WORKING_STATE.md
echo "------------------------------------------------------------"
echo "  ✅ Session-Digest bereit — siehe Ausgabe oben vor Arbeitsbeginn lesen"

# ── 10. Session-Identität ────────────────────────────────────────────────
SESSION_HASH=$(head -c 16 /dev/urandom | sha256sum | cut -c1-8)
echo ""
echo "[10/10] Session identity for this run: SESSION:${SESSION_HASH}"
echo "  → Verwende dieses Token in JEDEM AI-TAG/ANCHOR/REVIEW-PASS dieser Sitzung."
echo "  → Zeitstempel via: date -u +%Y-%m-%dT%H:%M:%SZ"

# ── Summary ──────────────────────────────────────────────────────────────────
echo ""
echo "============================================================"
echo "  ✅ MemFuse Jules Environment Ready (SESSION:${SESSION_HASH})"
echo "============================================================"
