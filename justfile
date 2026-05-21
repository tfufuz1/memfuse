# AGENT:11
set shell := ["bash", "-uc"]

# Execution wrapper to support Nix environments and standard environments
RUN := `if command -v nix >/dev/null; then echo "nix develop -c"; else echo ""; fi`

default:
    @just --list

# Runs the TDD Validation Loop (Red -> Green -> Refactor)
test: check
    {{RUN}} cargo nextest run --workspace || {{RUN}} cargo test --workspace

# Runs formatting, clippy and checks compilation
check:
    {{RUN}} cargo fmt --all -- --check
    {{RUN}} cargo clippy --all-targets -- -D warnings
    {{RUN}} cargo check --all-targets --workspace

# Modular check for memfuse-core
check-core:
    {{RUN}} cargo check -p memfuse-core

# Modular check for memfuse-store
check-store:
    {{RUN}} cargo check -p memfuse-store

# Modular check for memfuse-index
check-index:
    {{RUN}} cargo check -p memfuse-index

# Modular check for memfuse-db
check-db:
    {{RUN}} cargo check -p memfuse-db

# Modular check for memfuse-text
check-text:
    {{RUN}} cargo check -p memfuse-text

# Modular check for memfuse-runtime
check-runtime:
    {{RUN}} cargo check -p memfuse-runtime

# Modular check for memfuse-orchestrator
check-orchestrator:
    {{RUN}} cargo check -p memfuse-orchestrator

# Modular check for memfuse-py
check-py:
    {{RUN}} cargo check -p memfuse-py

# Modular check for memfuse-checkpoint
check-checkpoint:
    {{RUN}} cargo check -p memfuse-checkpoint

# Runs benchmarks
bench:
    {{RUN}} cargo bench --workspace

# Generates and opens documentation
doc:
    {{RUN}} cargo doc --workspace --no-deps --all-features

# Verifies the Directed Acyclic Graph (DAG) integrity of the workspace
dag-check:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "=== DAG Integrity Check ==="

    echo "--- Phase 1: L1 Kernel Isolation (core, runtime, orchestrator) ---"
    for CRATE in memfuse-core memfuse-runtime memfuse-orchestrator; do
        echo "Verifying $CRATE isolation..."
        if cargo tree -p "$CRATE" --edges no-dev | grep "memfuse-" | grep -E -v "$CRATE|memfuse-core" | grep -q .; then
            echo "❌ ERROR: $CRATE imports forbidden internal crates."
            cargo tree -p "$CRATE" --edges no-dev | grep "memfuse-"
            exit 1
        fi
    done

    echo "--- Phase 2: L2 Peer Isolation (store, index, text, checkpoint) ---"
    echo "Verifying memfuse-store..."
    if cargo tree -p memfuse-store --edges no-dev | grep -E -v "memfuse-store|memfuse-core" | grep -q "memfuse-"; then
        echo "❌ ERROR: memfuse-store violates DAG by importing non-core crates."
        cargo tree -p memfuse-store --edges no-dev | grep "memfuse-"
        exit 1
    fi
    echo "Verifying memfuse-index..."
    if cargo tree -p memfuse-index --edges no-dev | grep -E -v "memfuse-index|memfuse-core" | grep -q "memfuse-"; then
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
    if cargo tree -p memfuse-db --edges no-dev | grep -E -q "memfuse-py|memfuse-runtime|memfuse-orchestrator"; then
        echo "❌ ERROR: memfuse-db imports higher layers."
        cargo tree -p memfuse-db --edges no-dev | grep -E "memfuse-py|memfuse-runtime|memfuse-orchestrator"
        exit 1
    fi

    echo "--- Phase 4: L4 Bindings Isolation (py) ---"
    echo "Verifying memfuse-py..."
    if cargo tree -p memfuse-py --edges no-dev | grep -E -q "memfuse-runtime|memfuse-orchestrator"; then
        echo "❌ ERROR: memfuse-py violates isolation by importing L1 Kernel crates."
        cargo tree -p memfuse-py --edges no-dev | grep -E "memfuse-runtime|memfuse-orchestrator"
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
        if ! {{RUN}} cargo test --workspace; then
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

    echo "--- [1/4] .unwrap()/.expect() außerhalb von Test-Code ---"
    # Policy: Erlaubt in tests, doc-tests. In benches und Prod NUR mit // unwrap bzw. // expect Kommentar.
    UNWRAP=$(grep -HrnE "\.unwrap\(\)|\.expect\(" crates/ --include="*.rs" | while IFS=: read -r file lineno content; do
        if echo "$file" | grep -qE "_test\.rs:|/tests/|::tests::"; then continue; fi
        # Check if the line is inside a 'mod tests' block
        limit=$((lineno - 1))
        if [ "$limit" -gt 0 ] && head -n "$limit" "$file" | tac | grep -m1 -E "mod tests|#\[cfg\(test\)\]" | grep -q "mod tests"; then
            start_line=$(head -n "$limit" "$file" | grep -nE "mod tests|#\[cfg\(test\)\]" | tail -1 | cut -d: -f1)
            if ! head -n "$limit" "$file" | tail -n +$start_line | grep -q "^}"; then continue; fi
        fi
        # Check for allowed comments
        if echo "$line" | grep -qE "//.*(unwrap|expect)"; then continue; fi
        # Check for doc comments (which might contain examples with unwraps)
        if echo "$line" | grep -qE "///|//!"; then continue; fi
        echo "$line"
    done || true)
    if [ -n "$UNWRAP" ]; then
        UNWRAP_COUNT=$(echo "$UNWRAP" | wc -l)
        echo "❌ UNWRAP/EXPECT VIOLATIONS ($UNWRAP_COUNT Treffer — fix in WP-0.0):"
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
