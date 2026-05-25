#!/bin/bash
# scripts/jules-watchdog.sh
# Orchestrator-Watchdog Automation for Jules Agents
# Identity: AGENT:00

set -e

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_DIR"

DATE_THRESHOLD=$(date -d "8 hours ago" +%s)
CURRENT_DATE_STR=$(date -u +"%Y-%m-%d %H:%M UTC")
REPORT_DATE=$(date -u +"%Y%m%d")
REPORT_FILE="docs/audit/WATCHDOG_REPORT_${REPORT_DATE}.md"

echo "--- Jules Watchdog Run: ${CURRENT_DATE_STR} ---"

# Phase 1: Stale WIP-Anchor Scan
echo "Phase 1: Scanning for stale WIP anchors..."
WIP_ANCHORS=$(grep -rn "STATUS:WIP" crates/ || true)
if [ -n "$WIP_ANCHORS" ]; then
    echo "$WIP_ANCHORS" | while read -r line; do
        FILE=$(echo "$line" | cut -d: -f1)
        LNUM=$(echo "$line" | cut -d: -f2)
        # Try to extract date
        WIP_DATE=$(echo "$line" | grep -oP "DATE:\d{4}-\d{2}-\d{2}" | cut -d: -f2)
        if [ -n "$WIP_DATE" ]; then
            WIP_TS=$(date -d "$WIP_DATE" +%s)
            if [ "$WIP_TS" -lt "$DATE_THRESHOLD" ]; then
                echo "Resetting stale WIP at $FILE:$LNUM (Date: $WIP_DATE)"
                sed -i "${LNUM}s/STATUS:WIP/STATUS:OPEN/" "$FILE"
                sed -i "${LNUM}i // WATCHDOG: Reset WIP due to timeout." "$FILE"
            fi
        fi
    done
else
    echo "No active WIP anchors found."
fi

# Phase 2: Cross-Agent Deadlocks
echo "Phase 2: Scanning for BLOCKED anchors..."
BLOCKED_ANCHORS=$(grep -rn "STATUS:BLOCKED" crates/ || true)
if [ -z "$BLOCKED_ANCHORS" ]; then
    echo "No active BLOCKED anchors found."
fi

# Phase 3: Formal Verification Gates
echo "Phase 3: Auditing Formal Verification Gates..."
REVIEW_COMPONENTS=$(grep -rn "STATUS:REVIEW" crates/memfuse-store/ crates/memfuse-crypto/ || true)
GATE_STATUS="OPEN"
if [ -n "$REVIEW_COMPONENTS" ]; then
    HAS_PROOFS=$(grep -riE "kani|TLA\+" crates/memfuse-store/ crates/memfuse-crypto/ || true)
    if [ -z "$HAS_PROOFS" ]; then
        echo "WARNING: Components in REVIEW lack formal verification. Keeping Gate OPEN."
        GATE_STATUS="OPEN"
    fi
fi

# Apply Gate Status to memfuse-core/src/lib.rs
if grep -q "ANCHOR:ARCH:GATE-FV STATUS:" crates/memfuse-core/src/lib.rs; then
    sed -i "s/ANCHOR:ARCH:GATE-FV STATUS:CLOSED/ANCHOR:ARCH:GATE-FV STATUS:OPEN/" crates/memfuse-core/src/lib.rs
fi

# Phase 4: GitHub PR Integration
echo "Phase 4: Checking for PR Integration..."
if command -v gh >/dev/null 2>&1; then
    if [ -f ".agent/scripts/jules-integrate.sh" ]; then
        bash .agent/scripts/jules-integrate.sh || echo "Integration script failed."
    fi
else
    echo "GitHub CLI (gh) not found. Skipping automated integration."
fi

echo "Watchdog run complete."
