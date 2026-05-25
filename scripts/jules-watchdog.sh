#!/bin/bash
# scripts/jules-watchdog.sh
# Orchestrator-Watchdog Automation for Jules Agents
# Identity: AGENT:00

set -e

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_DIR"

# Config
HOURS_THRESHOLD=8
DATE_THRESHOLD=$(date -d "$HOURS_THRESHOLD hours ago" +%s)
CURRENT_DATE_STR=$(date -u +"%Y-%m-%d %H:%M UTC")
REPORT_DATE=$(date -u +"%Y%m%d")
REPORT_FILE="docs/audit/WATCHDOG_REPORT_${REPORT_DATE}.md"

mkdir -p docs/audit

# Helper for report
log_report() {
    echo "$1" >> "$REPORT_FILE"
}

# Start Report
echo "# Watchdog Audit Report — ${CURRENT_DATE_STR}" > "$REPORT_FILE"
log_report ""
log_report "## Identity"
log_report "- **Agent:** AGENT:00 (Orchestrator-Watchdog)"
log_report "- **Mandate:** Resolve deadlocks, reset stale WIP, audit FV Gates, PR integration."
log_report ""

# Phase 1: Stale WIP-Anchor Scan
log_report "## Phase 1: Stale WIP-Anchor Scan"
echo "Phase 1: Scanning for stale WIP anchors..."
WIP_ANCHORS=$(grep -rn "STATUS:WIP" crates/ || true)
if [ -n "$WIP_ANCHORS" ]; then
    echo "Scanning active WIP anchors..."
    echo "$WIP_ANCHORS" | while read -r line; do
        FILE=$(echo "$line" | cut -d: -f1)
        LNUM=$(echo "$line" | cut -d: -f2)

        WIP_DATE=$(sed -n "$((LNUM-2)),$((LNUM+5))p" "$FILE" | grep -oP "(DATE|WIP-START):\d{4}-\d{2}-\d{2}" | head -1 | cut -d: -f2)

        if [ -n "$WIP_DATE" ]; then
            WIP_TS=$(date -d "$WIP_DATE" +%s)
            if [ "$WIP_TS" -lt "$DATE_THRESHOLD" ]; then
                log_report "- **Action:** Resetting stale WIP at \`$FILE:$LNUM\` (Date: $WIP_DATE)"
                sed -i "${LNUM}s/STATUS:WIP/STATUS:OPEN/" "$FILE"
                sed -i "${LNUM}i // WATCHDOG: Reset WIP due to timeout." "$FILE"
            else
                log_report "- \`$FILE:$LNUM\` is fresh (Date: $WIP_DATE)."
            fi
        else
            log_report "- \`$FILE:$LNUM\` has no date tag. Skipping automatic reset."
        fi
    done
else
    log_report "No active WIP anchors found."
fi
log_report ""

# Phase 2: Cross-Agent Deadlocks
log_report "## Phase 2: Cross-Agent Deadlocks"
echo "Phase 2: Scanning for circular dependencies..."
BLOCKED_ANCHORS=$(grep -rn "STATUS:BLOCKED" crates/ || true)
if [ -n "$BLOCKED_ANCHORS" ]; then
    echo "Analyzing blocked anchors..."
    echo "$BLOCKED_ANCHORS" | while read -r line; do
        FILE=$(echo "$line" | cut -d: -f1)
        LNUM=$(echo "$line" | cut -d: -f2)

        ANCHOR_LINE=$(sed -n "${LNUM}p" "$FILE")
        ID=$(echo "$ANCHOR_LINE" | grep -oP "ANCHOR:[^ ]*" | cut -d: -f3)
        WP_LINE=$(sed -n "$((LNUM-2)),$((LNUM+5))p" "$FILE" | grep "WP:" | head -1)
        NEEDS=$(echo "$WP_LINE" | grep -oP "NEEDS:[^ ]*" | cut -d: -f2)

        if [ -n "$ID" ] && [ "$NEEDS" != "NONE" ] && [ -n "$NEEDS" ]; then
            for DEP_ID in ${NEEDS//+/ }; do
                CIRCULAR=$(grep -rn "ANCHOR:.*$DEP_ID" crates/ | while read -r c_line; do
                    C_FILE=$(echo "$c_line" | cut -d: -f1)
                    C_LNUM=$(echo "$c_line" | cut -d: -f2)
                    C_WP_LINE=$(sed -n "$((C_LNUM-2)),$((C_LNUM+5))p" "$C_FILE" | grep "WP:" | head -1)
                    if echo "$C_WP_LINE" | grep -q "NEEDS:.*$ID"; then
                        echo "$C_FILE:$C_LNUM"
                    fi
                done)

                if [ -n "$CIRCULAR" ]; then
                    log_report "- **DEADLOCK DETECTED:** Circular dependency between \`$ID\` and \`$DEP_ID\` (\`$CIRCULAR\`)."
                    log_report "  - **Action:** Breaking cycle. Setting \`$ID\` to OPEN."
                    sed -i "${LNUM}s/STATUS:BLOCKED/STATUS:OPEN/" "$FILE"
                    sed -i "${LNUM}s/NEEDS:$DEP_ID/NEEDS:NONE/" "$FILE"
                    sed -i "${LNUM}i // WATCHDOG: Broken cyclic dependency." "$FILE"
                fi
            done
        fi
    done
else
    log_report "No active BLOCKED anchors found."
fi
log_report ""

# Phase 3: Formal Verification Gates
log_report "## Phase 3: Formal Verification Gates"
echo "Phase 3: Auditing FV Gates..."
REVIEW_COMPONENTS=$(grep -rn "STATUS:REVIEW" crates/memfuse-store/ crates/memfuse-crypto/ || true)
GATE_STATUS="CLOSED"

if [ -n "$REVIEW_COMPONENTS" ]; then
    log_report "Components awaiting review detected in sensitive crates (store/crypto):"
    while read -r line; do
        log_report "  - $line"
    done <<< "$REVIEW_COMPONENTS"

    HAS_PROOFS=$(grep -riE "kani|TLA\+|Formal Proof" crates/memfuse-store/ crates/memfuse-crypto/ || true)

    if [ -z "$HAS_PROOFS" ]; then
        log_report "- **Finding:** Components in REVIEW lack formal verification evidence. **Opening Gate.**"
        GATE_STATUS="OPEN"
    else
        log_report "- **Finding:** Formal verification evidence detected. **Gate satisfies requirements (CLOSED).**"
        GATE_STATUS="CLOSED"
    fi
else
    log_report "No critical components in REVIEW. Gate remains **CLOSED**."
    GATE_STATUS="CLOSED"
fi

# Apply Gate Status to memfuse-core/src/lib.rs
if [ -f "crates/memfuse-core/src/lib.rs" ]; then
    CURRENT_STATE=$(grep -oP "ANCHOR:ARCH:GATE-FV STATUS:\K[A-Z]*" crates/memfuse-core/src/lib.rs || echo "UNKNOWN")
    if [ "$CURRENT_STATE" != "$GATE_STATUS" ]; then
        echo "Updating Gate Status: $CURRENT_STATE -> $GATE_STATUS"
        sed -i "s/ANCHOR:ARCH:GATE-FV STATUS:$CURRENT_STATE/ANCHOR:ARCH:GATE-FV STATUS:$GATE_STATUS/" crates/memfuse-core/src/lib.rs
        log_report "- **Action:** Updated \`ARCH:GATE-FV\` to \`$GATE_STATUS\` in \`memfuse-core/src/lib.rs\`."
    else
        log_report "- Gate is correctly set to \`$GATE_STATUS\`."
    fi
fi
log_report ""

# Phase 4: GitHub PR Integration
log_report "## Phase 4: GitHub PR Integration"
echo "Phase 4: Checking PR Integration..."
if command -v gh >/dev/null 2>&1; then
    if [ -f ".agent/scripts/jules-integrate.sh" ]; then
        log_report "- **Action:** Running Jules PR Integration script..."
        INTEGRATE_OUT=$(bash .agent/scripts/jules-integrate.sh 2>&1 || true)
        log_report "\`\`\`"
        log_report "$INTEGRATE_OUT"
        log_report "\`\`\`"
    else
        log_report "- **Error:** Integration script \`.agent/scripts/jules-integrate.sh\` not found."
    fi
else
    log_report "- **Status:** BLOCKED"
    log_report "- **Reason:** GitHub CLI (\`gh\`) not found in environment."
fi
log_report ""

# System Health Summary
log_report "## System Health Audit (Regressions)"
log_report "### Detected Blockers"
echo "Performing health audit..."
CHECK_OUT=$(cargo check --workspace 2>&1 || true)
if echo "$CHECK_OUT" | grep -q "error:"; then
    log_report "- **CRITICAL:** Workspace compilation failure detected."
    FIRST_ERR=$(echo "$CHECK_OUT" | grep -A 10 "error:" | head -20)
    log_report "\`\`\`"
    log_report "$FIRST_ERR"
    log_report "\`\`\`"
else
    log_report "- Workspace compiles successfully."
fi

VIOLATIONS=$(grep -rn "\.unwrap()" --include="*.rs" crates/ | grep -v "#\[test\]" | grep -v "#\[cfg(test)\]" | grep -v "//.*unwrap" | grep -v "_test\." | grep -v "/tests/" | head -5 || true)
if [ -n "$VIOLATIONS" ]; then
    log_report "- **CI REGRESSION:** Zero-unwrap Guard violations detected:"
    log_report "\`\`\`"
    log_report "$VIOLATIONS"
    log_report "\`\`\`"
fi

DAG_FAIL=$(cargo tree -p memfuse-store --edges no-dev | grep -q "memfuse-crypto" && echo "FAIL" || echo "PASS")
if [ "$DAG_FAIL" == "FAIL" ]; then
    log_report "- **CI REGRESSION:** DAG violation detected in \`memfuse-store\` (illegal import of \`memfuse-crypto\`)."
fi

FMT_FAIL=$(cargo fmt --all -- --check 2>&1 || echo "PASSED")
if [ "$FMT_FAIL" != "PASSED" ]; then
     log_report "- **CI REGRESSION:** Formatting violations detected (\`cargo fmt --check\` failed)."
fi

log_report ""
log_report "---"
log_report "*Report generated autonomously by Jules AGENT:00.*"

echo "Watchdog run complete. Report written to $REPORT_FILE"
