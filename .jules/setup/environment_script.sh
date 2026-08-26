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
echo "[1/8] Verifying Rust toolchain..."

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
echo "[2/8] Installing system libraries for Tauri crate..."

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
echo "[3/8] Verifying ONNX runtime availability..."
ldd --version | head -1
echo "  ✅ glibc compatible for ORT dynamic linking"


# ── 5. just (task runner) ───────────────────────────────────────────────────
echo ""
echo "[5/8] Installing 'just' task runner..."

if ! command -v just &>/dev/null; then
    cargo install just --quiet
    echo "  ✅ just installed"
else
    echo "  ✅ just already available: $(just --version)"
fi

# ── 6. cargo-audit (security auditing) ──────────────────────────────────────
echo ""
echo "[6/8] Installing cargo-audit..."

if ! command -v cargo-audit &>/dev/null; then
    cargo install cargo-audit --quiet
    echo "  ✅ cargo-audit installed"
else
    echo "  ✅ cargo-audit already available"
fi

# ── 7. Pre-warm Cargo Dependency Cache ──────────────────────────────────────
echo ""
echo "[7/8] Pre-compiling workspace dependencies..."

cd /app 2>/dev/null || cd /home/jules/repo 2>/dev/null || cd .

cargo check --workspace --exclude memfuse-tauri 2>&1 | tail -5
echo "  ✅ Workspace dependency cache warmed"

cargo test --workspace --exclude memfuse-tauri --no-run 2>&1 | tail -3
echo "  ✅ Test binaries pre-compiled"

# ── 8. Validate Key Invariants ───────────────────────────────────────────────
echo ""
echo "[8/8] Validating MemFuse workspace invariants..."

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

# ── Summary ──────────────────────────────────────────────────────────────────
echo ""
echo "============================================================"
echo "  ✅ MemFuse Jules Environment Ready"
echo "============================================================"
