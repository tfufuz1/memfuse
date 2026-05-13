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

# Verifies Directed Acyclic Graph (DAG) integrity of crates
dag-check:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "=== DAG Integrity Check ==="

    echo "--- [1/3] memfuse-core Isolation ---"
    if cargo tree -p memfuse-core --edges no-dev | grep -q "memfuse-store\|memfuse-db\|memfuse-index\|memfuse-text\|memfuse-checkpoint\|memfuse-py\|memfuse-orchestrator\|memfuse-runtime"; then
        echo "❌ ERROR: memfuse-core imports forbidden crates."
        exit 1
    fi
    echo "✅ memfuse-core is isolated."

    echo "--- [2/3] L2 Peer Isolation (store, index, text, checkpoint) ---"
    PEERS="memfuse-db|memfuse-index|memfuse-store|memfuse-text|memfuse-checkpoint|memfuse-py|memfuse-orchestrator|memfuse-runtime"
    for CRATE in memfuse-store memfuse-index memfuse-text memfuse-checkpoint; do
        # We allow tracked violations to NOT fail the local check yet if they are in dev-dependencies
        # but the CI check uses --edges no-dev which we also do here.
        FILTER=$(echo "$PEERS" | sed "s/$CRATE|//; s/|$CRATE//; s/$CRATE//")
        if cargo tree -p "$CRATE" --edges no-dev | grep -E -q "$FILTER"; then
            # Exceptions for tracked violations DAG-001 and DAG-002
            if [[ "$CRATE" == "memfuse-text" || "$CRATE" == "memfuse-checkpoint" ]]; then
                 if cargo tree -p "$CRATE" --edges no-dev | grep -E -v "memfuse-store" | grep -E -q "$FILTER"; then
                    echo "❌ ERROR: $CRATE imports forbidden peers (excluding tracked memfuse-store)."
                    exit 1
                 fi
            else
                echo "❌ ERROR: $CRATE imports forbidden peers."
                exit 1
            fi
        fi
    done
    echo "✅ L2 isolation verified (tracked violations skipped)."

    echo "--- [3/4] L1 Orchestration Isolation ---"
    for CRATE in memfuse-db memfuse-orchestrator memfuse-runtime; do
        if cargo tree -p "$CRATE" --edges no-dev | grep -q "memfuse-py"; then
            echo "❌ ERROR: $CRATE imports memfuse-py (Circular Dependency Risk)."
            exit 1
        fi
    done
    echo "✅ L1 isolation verified."

    echo "--- [4/4] Tracking Known Violations ---"
    for DAG in "DAG-001:memfuse-text:memfuse-store" "DAG-002:memfuse-checkpoint:memfuse-store" "DAG-003:memfuse-py:memfuse-db"; do
        ID=$(echo $DAG | cut -d: -f1)
        SRC=$(echo $DAG | cut -d: -f2)
        DST=$(echo $DAG | cut -d: -f3)
        if cargo tree -p "$SRC" --edges no-dev | grep -q "$DST"; then
            echo "⚠️  $ID still present ($SRC → $DST)"
        else
            echo "✅ $ID resolved"
        fi
    done

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
