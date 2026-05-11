#!/bin/bash
# .agent/scripts/jules-watchdog.sh
# Watchdog Orchestrator (Phase 1-4)

set -e

# Configuration
REPO_DIR="/app"
TIMEOUT_HOURS=8
DEADLOCK_AGE_DAYS=7
NOW_TS=$(date +%s)
NOW_DATE=$(date -u +"%Y-%m-%d")

cd "$REPO_DIR"

echo "=== JULES WATCHDOG RUN: $(date) ==="

# --- Phase 1: Stale WIP/ACTIVE Anchors ---
echo "--- Phase 1: Stale Anchor Scan ---"
WIP_FILES=$(grep -rlE "STATUS:(WIP|ACTIVE)" --include="*.rs" --include="*.md" --include="*.toml" . || true)

for file in $WIP_FILES; do
    # Skip template files in .agent/
    if [[ "$file" == *".agent/workflows/"* ]] || [[ "$file" == *".agent/prompts/"* ]]; then
        continue
    fi

    echo "Checking $file..."
    # Extract DATE or CREATED
    DATE_VAL=$(grep -m 1 -E "DATE:|CREATED:" "$file" | grep -oE "[0-9]{4}-[0-9]{2}-[0-9]{2}")
    if [ -n "$DATE_VAL" ]; then
        FILE_TS=$(date -d "$DATE_VAL" +%s)
        AGE_SECONDS=$((NOW_TS - FILE_TS))
        AGE_HOURS=$((AGE_SECONDS / 3600))

        if [ $AGE_HOURS -ge $TIMEOUT_HOURS ]; then
            echo "⚠️  Anchor in $file is stale ($AGE_HOURS hours). Resetting..."
            # Reset STATUS to OPEN and add comment
            sed -i "s/STATUS:WIP/STATUS:OPEN/g" "$file"
            sed -i "s/STATUS:ACTIVE/STATUS:OPEN/g" "$file"
            # Insert comment above the ANCHOR line
            sed -i "/ANCHOR:/i // WATCHDOG: Reset WIP due to timeout." "$file"
        fi
    fi
done

# --- Phase 2: Deadlock Detection ---
echo "--- Phase 2: Deadlock Scan ---"
BLOCKED_FILES=$(grep -rl "STATUS:BLOCKED" --include="*.rs" --include="*.md" --include="*.toml" . || true)

for file in $BLOCKED_FILES; do
    if [[ "$file" == *".agent/workflows/"* ]] || [[ "$file" == *".agent/prompts/"* ]] || [[ "$file" == *"docs/AGENT_STANDARDS.md"* ]]; then
        continue
    fi

    echo "Analyzing BLOCKED anchor in $file..."
    # Simple cycle detection for A -> B -> A
    ANCHOR_ID=$(grep -oE "ANCHOR:[A-Z]+:[^ ]+" "$file" | cut -d: -f3)
    NEEDS_ID=$(grep -oE "NEEDS:[^ ]+" "$file" | cut -d: -f2)

    if [ "$NEEDS_ID" != "NONE" ] && [ -n "$NEEDS_ID" ]; then
        # Find where NEEDS_ID is defined
        DEP_FILE=$(grep -rl "ANCHOR:[A-Z]\+:$NEEDS_ID" . || true)
        if [ -n "$DEP_FILE" ]; then
            # Check if DEP_FILE also needs ANCHOR_ID
            if grep -q "NEEDS:$ANCHOR_ID" "$DEP_FILE"; then
                echo "🛑 Deadlock detected: $ANCHOR_ID <-> $NEEDS_ID"
                echo "Breaking deadlock in $file..."
                sed -i "s/STATUS:BLOCKED/STATUS:OPEN/g" "$file"
                sed -i "s/NEEDS:$NEEDS_ID/NEEDS:NONE/g" "$file"
                sed -i "/ANCHOR:/i // WATCHDOG: Broken cyclic dependency." "$file"
            fi
        fi
    fi
done

# --- Phase 3: Formal Verification Gate ---
echo "--- Phase 3: Formal Verification Gate ---"
# Identify changed LSM/Crypto files
# (Mocking detection as we don't have real git history in sandbox for this run)
# Components: memfuse-store (LSM), distance.rs (SIMD)
CHANGED_CRITICAL=$(find crates/memfuse-store/src -name "*.rs" -mmin -60 || true)

if [ -n "$CHANGED_CRITICAL" ]; then
    for f in $CHANGED_CRITICAL; do
        # Check if kani or tla exists for this component
        BASENAME=$(basename "$f" .rs)
        if ! grep -rq "kani" . && ! grep -rq "tla" .; then
             echo "🔒 Missing FV for $f. Locking Gate."
             # We assume ARCH:GATE-FV is in AGENTS.md for this implementation
             if ! grep -q "ARCH:GATE-FV" AGENTS.md; then
                 echo -e "\n// ANCHOR:ARCH:GATE-FV — Formal Verification Gate\n// AGENT:00 DATE:$NOW_DATE STATUS:OPEN\n// DONE: All changed LSM components have Kani harnesses." >> AGENTS.md
             else
                 sed -i "/ARCH:GATE-FV/s/STATUS:DONE/STATUS:OPEN/" AGENTS.md
             fi
        fi
    done
fi

# --- Phase 4: Integration ---
echo "--- Phase 4: PR Integration ---"
if command -v gh &> /dev/null; then
    bash "$REPO_DIR/.agent/scripts/jules-integrate.sh"
else
    echo "ℹ️  'gh' tool missing. Skipping PR integration."
fi

echo "=== WATCHDOG RUN COMPLETE ==="
