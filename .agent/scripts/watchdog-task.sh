#!/bin/bash
# .agent/scripts/watchdog-task.sh
# Comprehensive Orchestrator-Watchdog Task Execution (AGENT:00)

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_DIR"

echo "=== WATCHDOG RUN $(date) ==="

# Phase 1: Stale WIP Anchors
echo "--- Phase 1: Stale WIP Anchors ---"
# Find files with STATUS:WIP
WIP_FILES=$(grep -rl "STATUS:WIP" . --exclude-dir=.git --exclude-dir=.agent --exclude-dir=target)

if [ -z "$WIP_FILES" ]; then
    echo "✅ No active STATUS:WIP anchors found."
else
    # Current time in seconds
    NOW=$(date +%s)
    # 8 hours in seconds
    TIMEOUT=$((8 * 3600))

    for file in $WIP_FILES; do
        echo "Checking $file..."
        # This is a simplification: assuming one anchor per file for logic demonstration
        # In a real scenario, we'd parse the specific line.
        # Looking for DATE:YYYY-MM-DD
        DATE_STR=$(grep "STATUS:WIP" "$file" | grep -oE "DATE:[0-9]{4}-[0-9]{2}-[0-9]{2}" | cut -d: -f2)
        if [ -n "$DATE_STR" ]; then
            ANCHOR_TIME=$(date -d "$DATE_STR" +%s 2>/dev/null || echo 0)
            AGE=$((NOW - ANCHOR_TIME))
            if [ "$AGE" -gt "$TIMEOUT" ]; then
                echo "⚠️ Anchor in $file is stale ($((AGE/3600))h). Resetting..."
                # Replace STATUS:WIP with STATUS:OPEN and add comment
                sed -i "/STATUS:WIP/i // WATCHDOG: Reset WIP due to timeout." "$file"
                sed -i "s/STATUS:WIP/STATUS:OPEN/g" "$file"
            fi
        fi
    done
fi

# Phase 2: Cross-Agent Deadlocks
echo "--- Phase 2: Cross-Agent Deadlocks ---"
# This requires building a dependency graph of BLOCKED anchors.
# For now, we scan and report. Logic to break cycles would be here.
BLOCKED_ANCHORS=$(grep -r "STATUS:BLOCKED" . --exclude-dir=.git --exclude-dir=.agent --exclude-dir=docs)
if [ -z "$BLOCKED_ANCHORS" ]; then
    echo "✅ No active STATUS:BLOCKED anchors found."
else
    echo "⚠️ Found BLOCKED anchors:"
    echo "$BLOCKED_ANCHORS"
    # Placeholder for cycle detection logic
fi

# Phase 3: Formal Verification Gates
echo "--- Phase 3: Formal Verification Gates ---"
# Check if Jules-02 and Jules-10 comply.
# Missing Kani/TLA+ proofs for REVIEW components (WAL, LSM, Crypto).
REVIEW_COMPONENTS=$(grep -rl "STATUS:REVIEW" crates/memfuse-store crates/memfuse-db --exclude-dir=target 2>/dev/null)

if [ -n "$REVIEW_COMPONENTS" ]; then
    echo "ℹ️ Components in REVIEW found. Enforcing ARCH:GATE-FV=OPEN."
    # Ensure gate is OPEN in crates/memfuse-core/src/lib.rs
    if ! grep -q "ANCHOR:ARCH:GATE-FV STATUS:OPEN" crates/memfuse-core/src/lib.rs; then
        echo "⚠️ Setting ARCH:GATE-FV to OPEN..."
        sed -i "s/ANCHOR:ARCH:GATE-FV STATUS:CLOSED/ANCHOR:ARCH:GATE-FV STATUS:OPEN/g" crates/memfuse-core/src/lib.rs
    else
        echo "✅ Gate already OPEN."
    fi
else
    echo "ℹ️ No components in REVIEW."
fi

# Phase 4: GitHub PR Integration
echo "--- Phase 4: GitHub PR Integration ---"
if command -v gh >/dev/null 2>&1; then
    bash "$REPO_DIR/.agent/scripts/jules-integrate.sh"
else
    echo "❌ gh CLI not found. Skipping Phase 4."
fi

echo "=== WATCHDOG RUN COMPLETE ==="
