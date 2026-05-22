#!/bin/bash
# .agent/scripts/watchdog-audit.sh
# Automated audit for stale WIP anchors and cyclic dependencies.

# set -e  # Disabled to allow script to finish even if grep finds nothing

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_DIR"

echo "--- Watchdog Audit Start ---"

# Phase 1: Stale WIP Anchors
echo "Checking Phase 1: Stale WIP Anchors..."
WIP_ANCHORS=$(grep -r "STATUS:WIP" . --exclude-dir=.git --exclude-dir=.agent --exclude-dir=target --exclude="WATCHDOG_REPORT_*.md")

if [ -z "$WIP_ANCHORS" ]; then
    echo "✅ No active STATUS:WIP anchors found."
else
    echo "⚠️ Found WIP anchors:"
    echo "$WIP_ANCHORS"
fi

# Phase 2: Cross-Agent Deadlocks
echo "Checking Phase 2: Deadlocks (STATUS:BLOCKED)..."
BLOCKED_ANCHORS=$(grep -r "STATUS:BLOCKED" . --exclude-dir=.git --exclude-dir=.agent --exclude-dir=docs --exclude-dir=target --exclude="WATCHDOG_REPORT_*.md")

if [ -z "$BLOCKED_ANCHORS" ]; then
    echo "✅ No active STATUS:BLOCKED anchors found."
else
    echo "⚠️ Found BLOCKED anchors:"
    echo "$BLOCKED_ANCHORS"
fi

# Phase 3: Formal Verification Gates
echo "Checking Phase 3: ARCH:GATE-FV..."
REVIEW_COMPONENTS=$(grep -r "STATUS:REVIEW" crates/memfuse-store crates/memfuse-db --exclude-dir=target 2>/dev/null)
GATE_STATUS=$(grep "ANCHOR:ARCH:GATE-FV" crates/memfuse-core/src/lib.rs)

if [ -n "$REVIEW_COMPONENTS" ]; then
    echo "ℹ️ Components in REVIEW found. ARCH:GATE-FV should be OPEN."
    if [[ "$GATE_STATUS" == *"STATUS:OPEN"* ]]; then
        echo "✅ Gate is correctly OPEN."
    else
        echo "❌ Gate is NOT OPEN but should be! Action required."
    fi
else
    echo "ℹ️ No components in REVIEW. ARCH:GATE-FV could be CLOSED."
fi

echo "--- Watchdog Audit End ---"
