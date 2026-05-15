# AGENT:11
set shell := ["bash", "-uc"]

default:
    @just --list

# Runs the TDD Validation Loop (Red -> Green -> Refactor)
test: check
    nix develop -c cargo nextest run --workspace || nix develop -c cargo test --workspace

# Runs formatting, clippy and checks compilation
check:
    nix develop -c cargo fmt --all -- --check
    nix develop -c cargo clippy --all-targets -- -D warnings
    nix develop -c cargo check --all-targets --workspace

# Modular check targets
check-core:
    nix develop -c cargo check -p memfuse-core

check-store:
    nix develop -c cargo check -p memfuse-store

check-index:
    nix develop -c cargo check -p memfuse-index

check-db:
    nix develop -c cargo check -p memfuse-db

check-text:
    nix develop -c cargo check -p memfuse-text

check-runtime:
    nix develop -c cargo check -p memfuse-runtime

check-orchestrator:
    nix develop -c cargo check -p memfuse-orchestrator

check-checkpoint:
    nix develop -c cargo check -p memfuse-checkpoint

check-py:
    nix develop -c cargo check -p memfuse-py

# DAG Integrity Check (Kernel & Peer Isolation)
dag-check:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "=== DAG Integrity Check ==="
    for CRATE in memfuse-core memfuse-runtime memfuse-orchestrator; do
      echo "Verifying $CRATE (L3 Kernel Isolation)..."
      if cargo tree -p "$CRATE" --edges no-dev | grep "memfuse-" | grep -v "$CRATE" | grep -q .; then
        echo "❌ ERROR: $CRATE imports forbidden internal crates."
        cargo tree -p "$CRATE" --edges no-dev | grep "memfuse-"
        exit 1
      fi
    done
    echo "Verifying memfuse-store (L2 Peer Isolation)..."
    if cargo tree -p memfuse-store --edges no-dev | grep -E -q "memfuse-db|memfuse-index|memfuse-text|memfuse-checkpoint|memfuse-py"; then
      echo "❌ ERROR: memfuse-store imports L2 peers or higher layers."
      exit 1
    fi
    echo "Verifying memfuse-index (L2 Peer Isolation)..."
    if cargo tree -p memfuse-index --edges no-dev | grep -E -q "memfuse-db|memfuse-store|memfuse-text|memfuse-checkpoint|memfuse-py"; then
      echo "❌ ERROR: memfuse-index imports L2 peers or higher layers."
      exit 1
    fi
    echo "Verifying memfuse-text (L2 Peer Isolation)..."
    if cargo tree -p memfuse-text --edges no-dev | grep -E -q "memfuse-db|memfuse-index|memfuse-checkpoint|memfuse-py"; then
      echo "❌ ERROR: memfuse-text imports forbidden crates (non-tracked)."
      exit 1
    fi
    echo "Verifying memfuse-checkpoint (L2 Peer Isolation)..."
    if cargo tree -p memfuse-checkpoint --edges no-dev | grep -E -q "memfuse-db|memfuse-index|memfuse-text|memfuse-py"; then
      echo "❌ ERROR: memfuse-checkpoint imports L2 peers (non-tracked)."
      exit 1
    fi
    echo "✅ DAG Integrity Check PASSED"

# Triple-Test-Gate: Tests müssen 3x hintereinander grün sein (DONE-Definition)
triple-test: check
    #!/usr/bin/env bash
    set -euo pipefail
    echo "=== Triple-Test-Gate ==="
    for RUN in 1 2 3; do
        echo "--- Run $RUN/3 ---"
        if ! nix develop -c cargo test --workspace; then
            echo "❌ FAILED on run $RUN/3. Fix all failures before this WP is DONE."
            exit 1
        fi
    done
    echo "✅ Triple-Test-Gate PASSED (3/3)"

# Tech-Debt Audit: scannt nach .unwrap(), unsafe, std::fs in Produktionscode
debt-audit:
    #!/usr/bin/env bash
    set -euo pipefail
    FAIL=0
    echo "=== Tech-Debt Audit ==="

    echo "--- [1/4] .unwrap() außerhalb von Test-Code ---"
    UNWRAP=$(grep -rn "\.unwrap()" crates/ --include="*.rs" \
        | grep -v "_test\.rs:" \
        | grep -v "/tests/" \
        | grep -v "::tests::" \
        | grep -v "//.*unwrap" \
        || true)
    if [ -n "$UNWRAP" ]; then
        UNWRAP_COUNT=$(echo "$UNWRAP" | wc -l)
        echo "❌ UNWRAP VIOLATIONS ($UNWRAP_COUNT Treffer — fix in WP-0.0):"
        echo "$UNWRAP" | head -15
        FAIL=1
    else echo "✅ Kein .unwrap() in Produktionscode"; fi

    echo "--- [2/4] unsafe außerhalb distance.rs ---"
    UNSAFE=$(grep -rn "unsafe " crates/ --include="*.rs" \
        | grep -v "crates/memfuse-index/src/distance\.rs" \
        | grep -v "#\[allow(unsafe_code)\]" \
        | grep -v "//.*unsafe" \
        || true)
    if [ -n "$UNSAFE" ]; then
        echo "❌ UNSAFE VIOLATIONS:"; echo "$UNSAFE"; FAIL=1
    else echo "✅ Kein unsafe außerhalb distance.rs"; fi

    echo "--- [3/4] std::fs in Produktionscode (Soft-Warning) ---"
    STDFS=$(grep -rn "std::fs::" crates/ --include="*.rs" \
        | grep -v "/tests/" | grep -v "mod tests" || true)
    if [ -n "$STDFS" ]; then
        echo "⚠️  std::fs:: Treffer (nach tokio::fs migrieren):"
        echo "$STDFS"
    else echo "✅ Kein std::fs:: in Produktionscode"; fi

    echo "--- [4/4] Lock-Hierarchy & Async-Safety (AST Analysis) ---"
    # Prüfe auf verschachtelte Locks (potenzielle Deadlocks) mittels ast-grep
    if command -v sg > /dev/null; then
        if sg scan --rule rules/detect_nested_locks.yml crates/; then
            echo "❌ Graceful Deadlock Risiko erkannt! Verschachtelte Locks gefunden:"
            FAIL=1
        else
            echo "✅ Keine kritischen Deadlock-Zustände im AST gefunden."
        fi
    else
        echo "⚠️  ast-grep (sg) nicht installiert, überspringe AST-Lock-Analyse."
    fi

    echo "--- [5/5] Security & Audit ---"
    if cargo audit --version &>/dev/null 2>&1; then
        cargo audit || echo "⚠️ Audit warnings — manuell prüfen"
    else
        echo "⚠️ cargo-audit nicht installiert: cargo install cargo-audit"
    fi

    if [ $FAIL -eq 1 ]; then
        echo ""; echo "❌ Debt-Audit FAILED — WP-0.0 zuerst abschließen!"; exit 1
    fi
    echo ""; echo "✅ Debt-Audit PASSED"

# Bootstrap a new feature using the Atomic Spec Template
spec NAME:
    #!/usr/bin/env bash
    set -euo pipefail
    TIMESTAMP=$(date +%Y%m%d)
    TARGET="docs/specs/SPEC-${TIMESTAMP}-{{NAME}}.md"
    mkdir -p docs/specs
    cp docs/specs/TEMPLATE_ATOMIC_SPEC.md "$TARGET"
    echo "Created new atomic spec at $TARGET"
    echo "Please fill out the spec and follow the TDD-Loop!"
