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

# DAG Integrity Check: verifiziert Architektur-Layer Isolation
dag-check:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "=== DAG Integrity Check ==="

    echo "--- [1/3] Layer 0 Isolation (Kernel) ---"
    if cargo tree -p memfuse-core --edges no-dev | grep -q "memfuse-store\|memfuse-db\|memfuse-index\|memfuse-text\|memfuse-checkpoint\|memfuse-py\|memfuse-runtime\|memfuse-orchestrator"; then
        echo "❌ ERROR: memfuse-core imports forbidden internal crates."
        exit 1
    fi
    echo "✅ memfuse-core isolation OK"

    echo "--- [2/3] Layer 1 Isolation (Peers) ---"
    FAIL=0
    if cargo tree -p memfuse-store --edges no-dev | grep -q "memfuse-db\|memfuse-index\|memfuse-text\|memfuse-checkpoint\|memfuse-py\|memfuse-runtime\|memfuse-orchestrator"; then
        echo "❌ ERROR: memfuse-store imports forbidden crates."
        FAIL=1
    fi
    if cargo tree -p memfuse-index --edges no-dev | grep -q "memfuse-db\|memfuse-store\|memfuse-text\|memfuse-checkpoint\|memfuse-py\|memfuse-runtime\|memfuse-orchestrator"; then
        echo "❌ ERROR: memfuse-index imports forbidden crates."
        FAIL=1
    fi
    if cargo tree -p memfuse-text --edges no-dev | grep -v "memfuse-store" | grep -q "memfuse-db\|memfuse-index\|memfuse-checkpoint\|memfuse-py\|memfuse-runtime\|memfuse-orchestrator"; then
        echo "❌ ERROR: memfuse-text imports forbidden crates (excluding DAG-001)."
        FAIL=1
    fi
    [ $FAIL -eq 0 ] && echo "✅ Layer 1 isolation OK (modulo DAG-001)" || exit 1

    echo "--- [3/3] Tracking Known Violations ---"
    VIOLATIONS=0
    cargo tree -p memfuse-text --edges no-dev | grep -q "memfuse-store" && { echo "⚠️ DAG-001 present (text->store)"; VIOLATIONS=$((VIOLATIONS+1)); }
    cargo tree -p memfuse-checkpoint --edges no-dev | grep -q "memfuse-store" && { echo "⚠️ DAG-002 present (checkpoint->store)"; VIOLATIONS=$((VIOLATIONS+1)); }
    cargo tree -p memfuse-py --edges no-dev | grep -q "memfuse-db" && { echo "⚠️ DAG-003 present (py->db)"; VIOLATIONS=$((VIOLATIONS+1)); }
    echo "Found $VIOLATIONS known violations."

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
