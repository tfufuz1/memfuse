#!/bin/bash
# .agent/scripts/watchdog-task.sh
# Automated Orchestrator-Watchdog Task Execution (AGENT:00)

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_DIR"

# Portability: Use a variable for sed -i
if [[ "$OSTYPE" == "darwin"* ]]; then
  SED_I="sed -i ''"
else
  SED_I="sed -i"
fi

echo "=== WATCHDOG RUN $(date) ==="

# Phase 1: Stale WIP Anchors
echo "--- Phase 1: Stale WIP Anchors ---"
WIP_FILES=$(grep -rl "STATUS:WIP" . --exclude-dir=.git --exclude-dir=.agent --exclude-dir=target --exclude="WATCHDOG_REPORT_*.md")

if [ -z "$WIP_FILES" ]; then
    echo "✅ No active STATUS:WIP anchors found."
else
    # We use day-precision as proxy for 8h since timestamps are rarely present in anchors
    TODAY=$(date +%Y-%m-%d)
    for file in $WIP_FILES; do
        echo "Checking $file..."
        # Extract DATE:YYYY-MM-DD
        ANCHOR_DATE=$(grep "STATUS:WIP" "$file" | grep -oE "DATE:[0-9]{4}-[0-9]{2}-[0-9]{2}" | cut -d: -f2 | head -n 1)

        if [ -n "$ANCHOR_DATE" ] && [ "$ANCHOR_DATE" != "$TODAY" ]; then
            echo "⚠️ Anchor in $file is from $ANCHOR_DATE (Stale). Resetting..."
            # Reset STATUS:WIP to STATUS:OPEN
            $SED_I "/STATUS:WIP/i \\
// WATCHDOG: Reset WIP due to timeout." "$file"
            $SED_I "s/STATUS:WIP/STATUS:OPEN/g" "$file"
        else
            echo "ℹ️ Anchor in $file is from today or missing date format. Skipping."
        fi
    done
fi

# Phase 2: Cross-Agent Deadlocks (Circular Dependencies)
echo "--- Phase 2: Cross-Agent Deadlocks ---"
# Scan for BLOCKED anchors and their DEPS/NEEDS
BLOCKED_DATA=$(grep -rE "ANCHOR:[^ ]+ STATUS:BLOCKED" . --exclude-dir=.git --exclude-dir=.agent --exclude-dir=docs --exclude="WATCHDOG_REPORT_*.md")

if [ -z "$BLOCKED_DATA" ]; then
    echo "✅ No active STATUS:BLOCKED anchors found."
else
    # Simple detection for circular dependencies
    echo "$BLOCKED_DATA" | while read -r line; do
        ID=$(echo "$line" | grep -oE "ANCHOR:[^ ]+" | head -n 1 | cut -d: -f2)
        DEP=$(echo "$line" | grep -oE "(DEPS|NEEDS):[^ ]+" | head -n 1 | cut -d: -f2)
        FILE=$(echo "$line" | cut -d: -f1)

        if [ -n "$ID" ] && [ -n "$DEP" ]; then
            echo "Checking $ID (depends on $DEP)..."
            # Look for reverse dependency
            if grep -r "ANCHOR:$DEP STATUS:BLOCKED" . --exclude-dir=.git --exclude-dir=.agent | grep -qE "(DEPS|NEEDS):$ID"; then
                echo "⚠️ Circular deadlock detected: $ID <-> $DEP"
                echo "Breaking cycle by opening $ID in $FILE..."
                $SED_I "/ANCHOR:$ID/i \\
// WATCHDOG: Broken cyclic dependency." "$FILE"
                $SED_I "s/ANCHOR:$ID STATUS:BLOCKED/ANCHOR:$ID STATUS:OPEN/g" "$FILE"
                # Remove the dependency entry to break the chain
                $SED_I "s/DEPS:$DEP//g" "$FILE"
                $SED_I "s/NEEDS:$DEP//g" "$FILE"
            fi
        fi
    done
fi

# Phase 3: Formal Verification Gates
echo "--- Phase 3: Formal Verification Gates ---"
# LSM and Crypto components in REVIEW must have Kani/TLA+ proofs
REVIEW_COMPONENTS=$(grep -rl "STATUS:REVIEW" crates/memfuse-store crates/memfuse-db --exclude-dir=target 2>/dev/null)
PROOFS_FOUND=$(find crates/memfuse-store crates/memfuse-db -name "*.kani" -o -name "*.tla" | wc -l)

if [ -n "$REVIEW_COMPONENTS" ] && [ "$PROOFS_FOUND" -eq 0 ]; then
    echo "⚠️ REVIEW components found in critical paths but NO Kani/TLA+ proofs found."
    echo "Enforcing ARCH:GATE-FV=OPEN."
    if grep -q "ANCHOR:ARCH:GATE-FV STATUS:CLOSED" crates/memfuse-core/src/lib.rs; then
        $SED_I "s/ANCHOR:ARCH:GATE-FV STATUS:CLOSED/ANCHOR:ARCH:GATE-FV STATUS:OPEN/g" crates/memfuse-core/src/lib.rs
    fi
else
    echo "✅ No missing proofs for REVIEW components detected."
fi

# Phase 4: GitHub PR Integration
echo "--- Phase 4: GitHub PR Integration ---"
INTEGRATE_SCRIPT="$REPO_DIR/.agent/scripts/jules-integrate.sh"
if [ -f "$INTEGRATE_SCRIPT" ] && command -v gh >/dev/null 2>&1; then
    bash "$INTEGRATE_SCRIPT"
else
    echo "ℹ️ PR integration skipped (gh CLI missing or script not found)."
fi

echo "=== WATCHDOG RUN COMPLETE ==="
