set shell := ["bash", "-uc"]

default:
    @just --list

# Runs the TDD Validation Loop (Red -> Green -> Refactor)
test: check
    @if cargo nextest --version >/dev/null 2>&1; then \
        cargo nextest run --workspace; \
    else \
        cargo test --workspace; \
    fi

# Runs formatting, clippy and checks compilation
check:
    cargo fmt --all -- --check
    cargo clippy --all-targets -- -D warnings
    cargo check --all-targets --workspace

# Triple-Test-Gate: Tests müssen 3x hintereinander grün sein (DONE-Definition)
triple-test: check
    #!/usr/bin/env bash
    set -euo pipefail
    echo "=== Triple-Test-Gate ==="
    for RUN in 1 2 3; do
        echo "--- Run $RUN/3 ---"
        if ! cargo test --workspace; then
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

# Verify DAG integrity and layer isolation
dag-check:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "=== DAG Integrity Check ==="

    echo "--- [1/3] memfuse-core isolation ---"
    if cargo tree -p memfuse-core --edges no-dev | grep -q "memfuse-store\|memfuse-db\|memfuse-index\|memfuse-text\|memfuse-checkpoint\|memfuse-py\|memfuse-runtime\|memfuse-orchestrator"; then
        echo "❌ ERROR: memfuse-core imports forbidden crates."
        exit 1
    fi
    echo "✅ memfuse-core is isolated"

    echo "--- [2/3] L2 isolation (store, index, text) ---"
    if cargo tree -p memfuse-store --edges no-dev | grep -q "memfuse-db\|memfuse-index\|memfuse-text\|memfuse-py\|memfuse-runtime\|memfuse-orchestrator\|memfuse-checkpoint"; then
        echo "❌ ERROR: memfuse-store imports forbidden crates."
        exit 1
    fi
    if cargo tree -p memfuse-index --edges no-dev | grep -q "memfuse-db\|memfuse-store\|memfuse-text\|memfuse-py\|memfuse-runtime\|memfuse-orchestrator\|memfuse-checkpoint"; then
        echo "❌ ERROR: memfuse-index imports forbidden crates."
        exit 1
    fi
    if cargo tree -p memfuse-text --edges no-dev | grep -v "memfuse-store" | grep -q "memfuse-db\|memfuse-index\|memfuse-py\|memfuse-runtime\|memfuse-orchestrator\|memfuse-checkpoint"; then
        echo "❌ ERROR: memfuse-text imports forbidden crates (other than known DAG-001)."
        exit 1
    fi
    echo "✅ L2 crates are isolated"

    echo "--- [3/3] Tracked violations ---"
    VIOLATIONS=0
    if cargo tree -p memfuse-text --edges no-dev | grep -q "memfuse-store"; then
        echo "⚠️  DAG-001 still present (memfuse-text → memfuse-store)"
        VIOLATIONS=$((VIOLATIONS+1))
    fi
    if cargo tree -p memfuse-checkpoint --edges no-dev | grep -q "memfuse-store"; then
        echo "⚠️  DAG-002 still present (memfuse-checkpoint → memfuse-store)"
        VIOLATIONS=$((VIOLATIONS+1))
    fi
    if cargo tree -p memfuse-py --edges no-dev | grep -q "memfuse-db"; then
        echo "⚠️  DAG-003 still present (memfuse-py → memfuse-db)"
        VIOLATIONS=$((VIOLATIONS+1))
    fi
    echo "✅ Tracked violations: $VIOLATIONS"

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
