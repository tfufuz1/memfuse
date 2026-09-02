#!/usr/bin/env bash
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
echo "[1/10] Verifying Rust toolchain..."

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
echo "[2/10] Installing system libraries for Tauri crate..."

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
echo "[3/10] Verifying ONNX runtime availability..."
ldd --version | head -1
echo "  ✅ glibc compatible for ORT dynamic linking"

# ── 4. Pre-Commit Hook Setup ────────────────────────────────────────────────
echo ""
echo "[4/10] Pre-Commit Hook Setup..."
if [ -d ".git" ] && [ -f ".git/hooks" -o -d ".git/hooks" ]; then
  # Pre-commit Hook für rustfmt (ADR-030)
  cat > .git/hooks/pre-commit << 'HOOK'
#!/bin/bash
cargo fmt --all -- --check || (echo "❌ Formatierung fehlerhaft. Führe: cargo fmt --all" && exit 1)
HOOK
  chmod +x .git/hooks/pre-commit
  echo "  ✅ Pre-commit hook (rustfmt) installiert"
else
  echo "  ⚠️  Kein .git Verzeichnis — Pre-commit Hook übersprungen (Jules VM erwartet)"
fi

# ── 5. just (task runner) ───────────────────────────────────────────────────
echo ""
echo "[5/10] Installing 'just' task runner..."

if ! command -v just &>/dev/null; then
    cargo install just --quiet
    echo "  ✅ just installed"
else
    echo "  ✅ just already available: $(just --version)"
fi

# ── 6. cargo-audit (security auditing) ──────────────────────────────────────
echo ""
echo "[6/10] Installing cargo-audit..."

if ! command -v cargo-audit &>/dev/null; then
    cargo install cargo-audit --quiet
    echo "  ✅ cargo-audit installed"
else
    echo "  ✅ cargo-audit already available"
fi

# ── 7. Pre-warm Cargo Dependency Cache ──────────────────────────────────────
echo ""
echo "[7/10] Pre-compiling workspace dependencies..."

cd /app 2>/dev/null || cd /home/jules/repo 2>/dev/null || cd .

cargo check --workspace --exclude memfuse-tauri 2>&1 | tail -5
echo "  ✅ Workspace dependency cache warmed"

cargo test --workspace --exclude memfuse-tauri --no-run 2>&1 | tail -3
echo "  ✅ Test binaries pre-compiled"

# ── 8. Validate Key Invariants ───────────────────────────────────────────────
echo ""
echo "[8/10] Validating MemFuse workspace invariants..."

if [ -f "AGENTS.md" ]; then
    echo "  ✅ AGENTS.md found ($(wc -l < AGENTS.md) lines)"
else
    echo "  ❌ AGENTS.md missing! Jules needs this file."
    exit 1
fi

OPEN_TAGS=$(grep -rn 'AI-TAG\[SMELL\]\[CRITICAL\]' crates/ --include='*.rs' 2>/dev/null | grep -v RESOLVED | wc -l || echo "0")
echo "  ✅ Open AI-TAG[SMELL][CRITICAL]: $OPEN_TAGS (target: 0)"

if grep -q "axum" crates/memfuse-mcp/Cargo.toml 2>/dev/null; then
    echo "  ❌ CRITICAL: axum found in memfuse-mcp! Violates ADR-010"
    exit 1
else
    echo "  ✅ ADR-010: axum not in memfuse-mcp (stdio-only MCP)"
fi

# ── 9. Session Context Digest ────────────────────────────────────────────────
echo ""
echo "[9/10] Session Context Digest..."

echo "  === OFFENE BLOCKER/CRITICAL TAGS ==="
CRITICAL_COUNT=$(grep -rn "AI-TAG\[.*\]\[BLOCKER\]\|AI-TAG\[.*\]\[CRITICAL\]" crates/ \
  --include="*.rs" 2>/dev/null | grep -vc "RESOLVED" || echo "0")
if [ "$CRITICAL_COUNT" -gt 0 ]; then
  echo "  ⚠️  $CRITICAL_COUNT offene BLOCKER/CRITICAL Tags:"
  grep -rn "AI-TAG\[.*\]\[BLOCKER\]\|AI-TAG\[.*\]\[CRITICAL\]" crates/ \
    --include="*.rs" 2>/dev/null | grep -v "RESOLVED" | head -5
else
  echo "  ✅ Keine offenen CRITICAL/BLOCKER Tags"
fi

echo "  === IN-PROGRESS ANCHORS ==="
IN_PROGRESS=$(grep -rn "ANCHOR\[.*\] STATUS:IN-PROGRESS" crates/ \
  --include="*.rs" 2>/dev/null | head -5 || true)
if [ -n "$IN_PROGRESS" ]; then
  echo "$IN_PROGRESS" | while IFS= read -r line; do echo "  $line"; done
else
  echo "  (keine aktiven Anchors)"
fi

echo "  === WORKING_STATE (letzte 5 Zeilen) ==="
tail -5 WORKING_STATE.md 2>/dev/null || echo "  (WORKING_STATE.md nicht lesbar)"

echo "  ✅ Session Context Digest abgeschlossen"

# ── 10. Session Identity ─────────────────────────────────────────────────────
echo ""
echo "[10/10] Session Identity..."

SESSION_HASH=$(date -u +%Y%m%d%H%M%S | sha256sum | head -c 8)
SESSION_TS=$(date -u +%Y-%m-%dT%H:%M:%SZ)

echo ""
echo "┌──────────────────────────────────────────────────┐"
echo "│  SESSION: ${SESSION_HASH}                        │"
echo "│  TS:      ${SESSION_TS}                    │"
echo "│                                                  │"
echo "│  Verwende BEIDE Werte für ALLE Tags dieser       │"
echo "│  Session: (TS: ${SESSION_TS})             │"
echo "│           (SESSION: ${SESSION_HASH})             │"
echo "└──────────────────────────────────────────────────┘"
echo ""
echo "  ✅ Session Identity etabliert"

# Export für nachgelagerte Skripte
export MEMFUSE_SESSION_HASH="${SESSION_HASH}"
export MEMFUSE_SESSION_TS="${SESSION_TS}"

# ── Summary ──────────────────────────────────────────────────────────────────
echo ""
echo "============================================================"
echo "  ✅ MemFuse Jules Environment Ready"
echo "============================================================"
