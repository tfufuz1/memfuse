# AGENT:11
set shell := ["bash", "-uc"]

default:
    @just --list

# Runs the TDD Validation Loop (Red -> Green -> Refactor)
test: check
    #!/usr/bin/env bash
    set -euo pipefail
    if command -v nix &> /dev/null && nix develop -c true &> /dev/null; then
        RUNNER="nix develop -c"
    else
        RUNNER=""
    fi
    $RUNNER cargo nextest run --workspace 2>/dev/null || $RUNNER cargo test --workspace

# Runs formatting, clippy and checks compilation
check:
    #!/usr/bin/env bash
    set -euo pipefail
    if command -v nix &> /dev/null && nix develop -c true &> /dev/null; then
        RUNNER="nix develop -c"
    else
        RUNNER=""
    fi
    $RUNNER cargo fmt --all -- --check
    $RUNNER cargo clippy --all-targets -- -D warnings
    $RUNNER cargo check --all-targets --workspace

# Modular check for memfuse-core
check-core:
    nix develop -c cargo check -p memfuse-core || cargo check -p memfuse-core

# Modular check for memfuse-store
check-store:
    nix develop -c cargo check -p memfuse-store || cargo check -p memfuse-store

# Modular check for memfuse-index
check-index:
    nix develop -c cargo check -p memfuse-index || cargo check -p memfuse-index

# Modular check for memfuse-db
check-db:
    nix develop -c cargo check -p memfuse-db || cargo check -p memfuse-db

# Modular check for memfuse-text
check-text:
    nix develop -c cargo check -p memfuse-text || cargo check -p memfuse-text

# Sync documentation from inline tags and cargo topology
sync-docs:
    nix develop -c cargo xtask sync-docs || cargo xtask sync-docs

# Verifies if documentation is in sync with code without making changes
sync-docs-check:
    nix develop -c cargo xtask sync-docs --check || cargo xtask sync-docs --check

# Verifies multi-session review coverage for completed anchors
check-review-coverage:
    nix develop -c cargo xtask check-review-coverage || cargo xtask check-review-coverage

# Verifies internal documentation consistency (e.g. crate counts)
check-consistency:
    nix develop -c cargo xtask check-consistency || cargo xtask check-consistency

# Zeigt alle Context-Tags als NDJSON (filterbar nach Crate, Severity, Status)
context-tags *ARGS:
    cargo xtask context-tags {{ARGS}}

# Zeigt offene kritische AI-TAGs und ANCHORs an — primärer manueller
# Weg, den aktuellen Governance-Status einer Session zu prüfen.
# Für einen strukturierten NDJSON-Export aller Tags steht `just context-tags` zur Verfügung.
session-context:
    #!/usr/bin/env bash
    echo "OFFENE KRITISCHE TAGS:"
    grep -rn "AI-TAG\[.*\]\[BLOCKER\]\|AI-TAG\[.*\]\[CRITICAL\]" crates/ --include='*.rs' | grep -v RESOLVED || echo "  (keine)"
    echo ""
    echo "OFFENE ANCHORS:"
    grep -rn "ANCHOR\[.*\] STATUS:IN-PROGRESS" crates/ --include='*.rs' || echo "  (keine)"

# Modular check for memfuse-py
check-py:
    nix develop -c cargo check --manifest-path crates/memfuse-py/Cargo.toml || cargo check --manifest-path crates/memfuse-py/Cargo.toml

# Modular check for memfuse-tauri
check-tauri:
    nix develop -c cargo check -p memfuse-tauri || cargo check -p memfuse-tauri

# Modular check for memfuse-embed
check-embed:
    nix develop -c cargo check -p memfuse-embed || cargo check -p memfuse-embed

# Verifies the Directed Acyclic Graph (DAG) integrity of the workspace
dag-check:
    nix develop -c cargo xtask check-dag || cargo xtask check-dag

# Triple-Test-Gate: Tests müssen 3x hintereinander grün sein (DONE-Definition)
triple-test: check
    #!/usr/bin/env bash
    set -euo pipefail
    if command -v nix &> /dev/null && nix develop -c true &> /dev/null; then
        RUNNER="nix develop -c"
    else
        RUNNER=""
    fi
    echo "=== Triple-Test-Gate ==="
    for RUN in 1 2 3; do
        echo "--- Run $RUN/3 ---"
        if ! $RUNNER cargo test --workspace; then
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
        | grep -v "/tests\.rs:" \
        | grep -v "/benches/" \
        | grep -v "benches\.rs:" \
        | grep -v "memfuse_generated\.rs:" \
        | grep -v "::tests::" \
        | grep -v "//.*unwrap" \
        | grep -v "// expect" \
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
