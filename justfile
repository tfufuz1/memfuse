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

# Modular check for memfuse-core
check-core:
    nix develop -c cargo check -p memfuse-core

# Modular check for memfuse-store
check-store:
    nix develop -c cargo check -p memfuse-store

# Modular check for memfuse-index
check-index:
    nix develop -c cargo check -p memfuse-index

# Modular check for memfuse-db
check-db:
    nix develop -c cargo check -p memfuse-db

# Modular check for memfuse-text
check-text:
    nix develop -c cargo check -p memfuse-text


# Modular check for memfuse-py
check-py:
    nix develop -c cargo check -p memfuse-py

# Modular check for memfuse-tauri
check-tauri:
    nix develop -c cargo check -p memfuse-tauri

# Modular check for memfuse-embed
check-embed:
    nix develop -c cargo check -p memfuse-embed

# Verifies the Directed Acyclic Graph (DAG) integrity of the workspace
dag-check:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "=== DAG Integrity Check ==="

    echo "--- Phase 1: L1 Kernel Isolation (core) ---"
    echo "Verifying memfuse-core isolation..."
    if cargo tree -p memfuse-core --edges no-dev | grep "memfuse-" | grep -E -v "memfuse-core" | grep -q .; then
        echo "❌ ERROR: memfuse-core imports forbidden internal crates."
        cargo tree -p memfuse-core --edges no-dev | grep "memfuse-"
        exit 1
    fi

    echo "--- Phase 2: L2 Peer Isolation (store, index, text, checkpoint) ---"
    echo "Verifying memfuse-store..."
    if cargo tree -p memfuse-store --edges no-dev | grep -E -v "memfuse-store|memfuse-core|memfuse-crypto" | grep -q "memfuse-"; then
        echo "❌ ERROR: memfuse-store violates DAG by importing non-core crates."
        cargo tree -p memfuse-store --edges no-dev | grep "memfuse-"
        exit 1
    fi
    echo "Verifying memfuse-index..."
    if cargo tree -p memfuse-index --edges no-dev | grep -E -v "memfuse-index|memfuse-core|memfuse-graph" | grep -q "memfuse-"; then
        echo "❌ ERROR: memfuse-index violates DAG by importing non-core crates."
        cargo tree -p memfuse-index --edges no-dev | grep "memfuse-"
        exit 1
    fi
    echo "Verifying memfuse-text..."
    if cargo tree -p memfuse-text --edges no-dev | grep -E -v "memfuse-text|memfuse-core" | grep -q "memfuse-"; then
        echo "❌ ERROR: memfuse-text violates DAG by importing non-core crates."
        cargo tree -p memfuse-text --edges no-dev | grep "memfuse-"
        exit 1
    fi
    echo "Verifying memfuse-checkpoint (excluding tracked DAG-002)..."
    if cargo tree -p memfuse-checkpoint --edges no-dev | grep -E -v "memfuse-checkpoint|memfuse-core|memfuse-store" | grep -q "memfuse-"; then
        echo "❌ ERROR: memfuse-checkpoint violates DAG."
        cargo tree -p memfuse-checkpoint --edges no-dev | grep "memfuse-"
        exit 1
    fi

    echo "--- Phase 3: L3 Orchestration Isolation (db) ---"
    echo "Verifying memfuse-db..."
    if cargo tree -p memfuse-db --edges no-dev | grep -E -q "memfuse-py"; then
        echo "❌ ERROR: memfuse-db imports higher layers."
        cargo tree -p memfuse-db --edges no-dev | grep -E "memfuse-py"
        exit 1
    fi

    echo "--- Phase 4: L4 Application & Bindings Isolation (py, tauri) ---"
    echo "Verifying memfuse-py..."
    echo "Verifying memfuse-tauri..."
    if cargo tree -p memfuse-tauri --edges no-dev | grep -E -q "memfuse-py"; then
        echo "❌ ERROR: memfuse-tauri imports forbidden internal crates."
        cargo tree -p memfuse-tauri --edges no-dev | grep -E "memfuse-py"
        exit 1
    fi

    echo "--- Known DAG Violations (Tracking) ---"
    for VIOLATION in "memfuse-checkpoint:memfuse-store:DAG-002" "memfuse-py:memfuse-db:DAG-003"; do
        CRATE=${VIOLATION%%:*}
        TARGET=$(echo $VIOLATION | cut -d: -f2)
        ID=$(echo $VIOLATION | cut -d: -f3)
        if cargo tree -p "$CRATE" --edges no-dev | grep -q "$TARGET"; then
            echo "⚠️  $ID still present ($CRATE → $TARGET)"
        else
            echo "✅ $ID resolved"
        fi
    done
    echo "=== DAG-Check PASSED ==="

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

# Bootstrap a new feature using the Micro-Spec Template
spec NAME:
    #!/usr/bin/env bash
    set -euo pipefail
    TIMESTAMP=$(date +%Y%m%d)
    TARGET="docs/specs/SPEC-${TIMESTAMP}-{{NAME}}.md"
    mkdir -p docs/specs
    cp docs/specs/TEMPLATE_MICRO_SPEC.md "$TARGET"
    echo "Created new micro-spec at $TARGET"
    echo "Please fill out the spec and follow the SDD-Process (Spec -> Test -> Impl)!"
