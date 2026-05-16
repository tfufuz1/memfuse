#!/bin/bash
# .agent/scripts/jules-watchdog.sh
# Orchestrator-Watchdog logic for MemFuse

set -e

REPO_DIR="${1:-/app}"
cd "$REPO_DIR"

echo "--- [Watchdog] Phase 1: Resetting Stale WIP/ACTIVE Anchors ---"
# Current date for 8-hour timeout: 2026-05-16
# We look for STATUS:WIP or STATUS:ACTIVE that are older than today (simplification for the script)
# Realistic implementation would parse the date properly.
STALE_ANCHORS=$(grep -rnE "STATUS:(WIP|ACTIVE)" --include="*.rs" --include="*.toml" --include="*.md" . | grep -v "00-watchdog.md" | grep -v "2026-05-16" || true)

if [ -n "$STALE_ANCHORS" ]; then
    echo "Found potentially stale anchors:"
    echo "$STALE_ANCHORS"
    # In a real automated run, we would sed/replace here.
    # For now, we report them.
else
    echo "✅ No stale anchors found."
fi

echo "--- [Watchdog] Phase 2: Cross-Agent Deadlock Detection ---"
BLOCKED_ANCHORS=$(grep -rn "STATUS:BLOCKED" --include="*.rs" --include="*.toml" --include="*.md" . | grep -v "AGENT_STANDARDS.md" || true)

if [ -n "$BLOCKED_ANCHORS" ]; then
    echo "Found blocked anchors - checking for cycles:"
    echo "$BLOCKED_ANCHORS"
    # Simple cycle detection would go here.
else
    echo "✅ No deadlocks detected."
fi

echo "--- [Watchdog] Phase 3: Formal Verification Gate Status ---"
if grep -q "ARCH:GATE-FV STATUS:OPEN" AGENTS.md; then
    echo "⚠️  GATE-FV is OPEN (Merge Blocked). Reason: Missing Formal Verification."
else
    echo "✅ GATE-FV is CLOSED (Merges allowed)."
fi

echo "--- [Watchdog] Phase 4: Integration Trigger ---"
if [ -f ".agent/scripts/jules-integrate.sh" ]; then
    if command -v gh &> /dev/null; then
        bash .agent/scripts/jules-integrate.sh
    else
        echo "ℹ️  gh cli missing, skipping automated integration."
    fi
else
    echo "❌ jules-integrate.sh not found."
fi

echo "--- [Watchdog] Run Complete ---"
