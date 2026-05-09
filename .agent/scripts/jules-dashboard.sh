#!/bin/bash
# .agent/scripts/jules-dashboard.sh
# Blueprinted Dashboard für 13 Jules-Accounts | Health & Squad Status

set -e

REPO_DIR="/home/freddy/Arbeitsplatz/DEV/memfuse"
cd "$REPO_DIR"

# ANSI Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

# --- 1. Global Health State ---
UNWRAPS=$(grep -rn "\.unwrap()\|\.expect(" crates/ --include="*.rs" | grep -v "/tests/" | grep -v "#\[cfg(test)\]" | wc -l || echo 0)
BLOCKING=$(grep -rn "std::fs::" crates/ --include="*.rs" | grep -v "/tests/" | grep -v "mod tests" | wc -l || echo 0)
TESTS=$(cargo test --workspace -- --list 2>/dev/null | grep "::" | wc -l || echo 0)

echo -e "${BOLD}=== MemFuse Codebase Health ===${NC}"
[ "$UNWRAPS" -eq 0 ] && echo -ne "${GREEN}Zero-Panic ✅${NC} | " || echo -ne "${RED}Unwraps: $UNWRAPS ❌${NC} | "
[ "$BLOCKING" -eq 0 ] && echo -ne "${GREEN}Async-Safe ✅${NC} | " || echo -ne "${RED}Blocking-IO: $BLOCKING ❌${NC} | "
echo -e "Tests: ${CYAN}$TESTS${NC}"
echo -e "----------------------------------------"

# --- 2. Squad Status ---
echo -e "${BOLD}=== Jules Squad Dashboard (13 Accounts) ===${NC}"
CURRENT_HOUR_UTC=$(date -u +%H)
CURRENT_MIN_UTC=$(date -u +%M)
echo -e "Aktuelle Zeit (UTC): ${CYAN}${CURRENT_HOUR_UTC}:${CURRENT_MIN_UTC}${NC}"
echo -e "----------------------------------------"

scan_account() {
    local acc_id=$1
    local role=$2
    local scheduled_time=$3
    
    local pattern="⬡ @JULES-${acc_id}"
    local matches=$(grep -rn "$pattern" --include="*.rs" --include="*.md" . | grep -v "STATUS:DONE" || true)
    local done_count=$(grep -rn "$pattern" --include="*.rs" --include="*.md" . | grep "STATUS:DONE" | wc -l || echo 0)
    
    local status_color=$NC
    if [[ "$scheduled_time" == "${CURRENT_HOUR_UTC}:00" ]]; then
        status_color=$GREEN
        role="${role} ${BOLD}[ACTIVE]${NC}"
    fi

    echo -e "${status_color}Account $acc_id ($scheduled_time) - $role${NC}"
    
    if [ -z "$matches" ]; then
        echo -e "  Status: ${GREEN}Idle (Free Capacity)${NC} | Done: $done_count"
    else
        local wip_count=$(echo "$matches" | grep "STATUS:WIP" | wc -l || echo 0)
        local open_count=$(echo "$matches" | grep "STATUS:OPEN" | wc -l || echo 0)
        echo -ne "  Status: "
        [ $wip_count -gt 0 ] && echo -ne "${YELLOW}WIP: $wip_count${NC} "
        [ $open_count -gt 0 ] && echo -ne "${BLUE}OPEN: $open_count${NC} "
        echo -e "| Done: $done_count"
        
        local top_task=$(echo "$matches" | head -n 1 | cut -d':' -f3- | sed 's/.*\/\/ //')
        echo -e "  Top Task: ${top_task:0:60}..."
    fi
}

# Account Scans
scan_account "13" "Debt Hunter" "05:00"
scan_account "01" "Core Guardian" "06:00"
scan_account "02" "Store Engineer" "07:00"
scan_account "03" "Index Engineer" "08:00"
scan_account "04" "DB Orchestrator" "09:00"
scan_account "05" "Text Engine" "10:00"
scan_account "06" "Python Bindings" "11:00"
scan_account "10" "Security" "12:00"
scan_account "07" "QA Cross-Crate" "20:00"
scan_account "12" "Integration Tester" "21:00"
scan_account "09" "Benchmarks" "22:00"

echo -e "----------------------------------------"
OPEN_PRS=$(gh pr list --label jules --json number | jq '. | length' || echo 0)
[ "$OPEN_PRS" -gt 0 ] && echo -e "⚠️  ${YELLOW}$OPEN_PRS Jules PRs warten auf Integration.${NC}" || echo -e "✅ Keine offenen Jules PRs."
