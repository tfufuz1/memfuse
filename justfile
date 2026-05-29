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

# Modular check for memfuse-sandbox
check-sandbox:
    nix develop -c cargo check -p memfuse-sandbox

# Modular check for memfuse-saos-agent
check-saos-agent:
    nix develop -c cargo check -p memfuse-saos-agent

# Modular check for memfuse-graph
check-graph:
    nix develop -c cargo check -p memfuse-graph

# Modular check for memfuse-crypto
check-crypto:
    nix develop -c cargo check -p memfuse-crypto

# Modular check for memfuse-py
check-py:
    nix develop -c cargo check -p memfuse-py

# Modular check for memfuse-checkpoint
check-checkpoint:
    nix develop -c cargo check -p memfuse-checkpoint

# Verifies the Directed Acyclic Graph (DAG) integrity of the workspace
dag-check:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "=== DAG Integrity Check ==="

    echo "--- Phase 1: L1 Foundation Isolation (core) ---"
    for CRATE in memfuse-core; do
        echo "Verifying $CRATE isolation..."
        if cargo tree -p "$CRATE" --edges no-dev | grep "memfuse-" | grep -v "$CRATE" | grep -q .; then
            echo "❌ ERROR: $CRATE imports forbidden internal crates."
            cargo tree -p "$CRATE" --edges no-dev | grep "memfuse-"
            exit 1
        fi
    done

    echo "--- Phase 2: L2 Engine Isolation (crypto, graph, store, index, text) ---"
    echo "Verifying memfuse-crypto..."
    if cargo tree -p memfuse-crypto --edges no-dev | grep -E -v "memfuse-crypto|memfuse-core" | grep -q "memfuse-"; then
        echo "❌ ERROR: memfuse-crypto violates DAG by importing non-core crates."
        cargo tree -p memfuse-crypto --edges no-dev | grep "memfuse-"
        exit 1
    fi

    echo "Verifying memfuse-graph..."
    if cargo tree -p memfuse-graph --edges no-dev | grep -E -v "memfuse-graph|memfuse-core" | grep -q "memfuse-"; then
        echo "❌ ERROR: memfuse-graph violates DAG by importing non-core crates."
        cargo tree -p memfuse-graph --edges no-dev | grep "memfuse-"
        exit 1
    fi

    echo "Verifying memfuse-store (permits memfuse-crypto)..."
    if cargo tree -p memfuse-store --edges no-dev | grep -E -v "memfuse-store|memfuse-core|memfuse-crypto" | grep -q "memfuse-"; then
        echo "❌ ERROR: memfuse-store violates DAG."
        cargo tree -p memfuse-store --edges no-dev | grep "memfuse-"
        exit 1
    fi

    echo "Verifying memfuse-index (permits memfuse-graph)..."
    if cargo tree -p memfuse-index --edges no-dev | grep -E -v "memfuse-index|memfuse-core|memfuse-graph" | grep -q "memfuse-"; then
        echo "❌ ERROR: memfuse-index violates DAG."
        cargo tree -p memfuse-index --edges no-dev | grep "memfuse-"
        exit 1
    fi

    echo "Verifying memfuse-text..."
    if cargo tree -p memfuse-text --edges no-dev | grep -E -v "memfuse-text|memfuse-core" | grep -q "memfuse-"; then
        echo "❌ ERROR: memfuse-text violates DAG."
        cargo tree -p memfuse-text --edges no-dev | grep "memfuse-"
        exit 1
    fi

    echo "--- Phase 3: L3 Orchestration Isolation (db) ---"
    echo "Verifying memfuse-db..."
    if cargo tree -p memfuse-db --edges no-dev | grep -E -q "memfuse-py|memfuse-sandbox|memfuse-saos-agent"; then
        echo "❌ ERROR: memfuse-db imports higher layers."
        cargo tree -p memfuse-db --edges no-dev | grep -E "memfuse-py|memfuse-sandbox|memfuse-saos-agent"
        exit 1
    fi

    echo "--- Phase 4: L4 Bindings Isolation (py) ---"
    echo "Verifying memfuse-py..."
    if cargo tree -p memfuse-py --edges no-dev | grep -E -q "memfuse-sandbox|memfuse-saos-agent"; then
        echo "❌ ERROR: memfuse-py violates isolation."
        cargo tree -p memfuse-py --edges no-dev | grep -E "memfuse-sandbox|memfuse-saos-agent"
        exit 1
    fi

    echo "--- Phase 5: Frozen/Agent Layers (checkpoint, sandbox, saos-agent) ---"
    for CRATE in memfuse-checkpoint memfuse-sandbox memfuse-saos-agent; do
        echo "Verifying $CRATE isolation (L5)..."
        if cargo tree -p "$CRATE" --edges no-dev | grep -q "memfuse-py"; then
            echo "❌ ERROR: $CRATE imports forbidden L4 bindings."
            cargo tree -p "$CRATE" --edges no-dev | grep "memfuse-py"
            exit 1
        fi
    done

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
    TARGET=".agent/specs/modules/MOD-${TIMESTAMP}-{{NAME}}.md"
    mkdir -p .agent/specs/modules
    cp .agent/specs/modules/TEMPLATE_MICRO_SPEC.md "$TARGET"
    echo "Created new micro-spec at $TARGET"
    echo "Please fill out the spec and follow the SDD-Process (Spec -> Test -> Impl)!"
