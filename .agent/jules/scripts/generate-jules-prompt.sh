#!/usr/bin/env bash
# generate-jules-prompt.sh — Generiert vollständigen Jules-Task-Prompt mit 3-Layer-System
#
# VERWENDUNG:
#   bash generate-jules-prompt.sh <ACCOUNT_NR> <WP> [TASK-NR]
#   bash generate-jules-prompt.sh 02 WP-1.1 3
#   bash generate-jules-prompt.sh 07           # QA Account, kein spezifisches WP
#
# Kombiniert:
#   Layer 1: Standard-Präambel (00-PREAMBLE.md)
#   Layer 2: Account-Kontext (accounts/XX-*.md)
#   Layer 3: WP-Spezifikation (docs/specs/SPEC-*-WP-X.Y-*.md)

set -euo pipefail

ACCOUNT="${1:-}"
WP="${2:-}"
TASK_NR="${3:-}"

if [ -z "$ACCOUNT" ]; then
    echo "Usage: $0 <ACCOUNT_NR> [WP-X.Y] [TASK-NR]" >&2
    echo "" >&2
    echo "Accounts:" >&2
    echo "  01  Core Guardian      07  QA Cross-Crate" >&2
    echo "  02  Store Engineer     08  Docs & Specs" >&2
    echo "  03  Index Engineer     09  Benchmarks" >&2
    echo "  04  DB Orchestrator    10  Security" >&2
    echo "  05  Text Engine        11  CI/DevOps" >&2
    echo "  06  Python Bindings    12  Integration Tester" >&2
    echo "                         13  Debt Hunter" >&2
    exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
PROMPTS_DIR="${SCRIPT_DIR}/../prompts"
ACCOUNTS_DIR="${PROMPTS_DIR}/accounts"

# ═══════════════════════════════════════════════════════════════
# Layer 0: Dynamic Repository State
# ═══════════════════════════════════════════════════════════════
echo "═══════════════════════════════════════════════════════════════"
echo "  CURRENT REPOSITORY STATE (auto-generated $(date -u +%Y-%m-%dT%H:%M:%SZ))"
echo "═══════════════════════════════════════════════════════════════"
echo ""

if [ -x "${SCRIPT_DIR}/inject-context.sh" ]; then
    bash "${SCRIPT_DIR}/inject-context.sh" 2>/dev/null || echo "(context injection skipped)"
fi
echo ""

# ═══════════════════════════════════════════════════════════════
# Layer 1: Standard-Präambel (Sovereign Doctrine)
# ═══════════════════════════════════════════════════════════════
echo "═══════════════════════════════════════════════════════════════"
echo "  LAYER 1: SOVEREIGN CORE DOCTRINE & TRIPLE-TEST-GATE"
echo "═══════════════════════════════════════════════════════════════"
echo ""

PREAMBLE="${PROMPTS_DIR}/00-PREAMBLE.md"
if [ -f "$PREAMBLE" ]; then
    # Extrahiere den Code-Block aus der Präambel
    sed -n '/^```$/,/^```$/p' "$PREAMBLE" | grep -v '^```$'
else
    echo "⚠️  Präambel nicht gefunden: $PREAMBLE" >&2
fi
echo ""

# ═══════════════════════════════════════════════════════════════
# Layer 2: Account-Kontext
# ═══════════════════════════════════════════════════════════════
ACCOUNT_FILE=$(ls "${ACCOUNTS_DIR}/${ACCOUNT}-"*.md 2>/dev/null | head -1)

if [ -n "${ACCOUNT_FILE:-}" ] && [ -f "$ACCOUNT_FILE" ]; then
    ACCOUNT_NAME=$(basename "$ACCOUNT_FILE" .md | sed "s/^${ACCOUNT}-//")
    echo "═══════════════════════════════════════════════════════════════"
    echo "  LAYER 2: ACCOUNT CONTEXT — #${ACCOUNT} ${ACCOUNT_NAME}"
    echo "═══════════════════════════════════════════════════════════════"
    echo ""
    cat "$ACCOUNT_FILE"
    echo ""
else
    echo "⚠️  Account-Datei nicht gefunden für Account ${ACCOUNT}" >&2
    echo "    Erwartet: ${ACCOUNTS_DIR}/${ACCOUNT}-*.md" >&2
fi

# ═══════════════════════════════════════════════════════════════
# Layer 3: WP Specification (falls angegeben)
# ═══════════════════════════════════════════════════════════════
if [ -n "$WP" ]; then
    WP_SPEC=$(ls "${REPO_ROOT}/docs/specs/SPEC-"*"-${WP}-"*.md 2>/dev/null | head -1)
    # Fallback: versuche mit WP ohne Bindestrich-Trenner
    if [ -z "${WP_SPEC:-}" ]; then
        WP_SLUG=$(echo "$WP" | tr '[:upper:]' '[:lower:]' | sed 's/\./-/g')
        WP_SPEC=$(ls "${REPO_ROOT}/docs/specs/SPEC-"*"${WP_SLUG}"*.md 2>/dev/null | head -1)
    fi

    if [ -n "${WP_SPEC:-}" ] && [ -f "$WP_SPEC" ]; then
        echo "═══════════════════════════════════════════════════════════════"
        echo "  LAYER 3: ATOMIC SPEC — ${WP}"
        echo "═══════════════════════════════════════════════════════════════"
        echo ""
        cat "$WP_SPEC"
        echo ""
    else
        echo "⚠️  Keine SPEC gefunden für ${WP}" >&2
        echo "    Gesucht in: ${REPO_ROOT}/docs/specs/" >&2
    fi
fi

# ═══════════════════════════════════════════════════════════════
# Done-Definition (immer am Ende)
# ═══════════════════════════════════════════════════════════════
echo "═══════════════════════════════════════════════════════════════"
echo "  DONE-DEFINITION"
echo "═══════════════════════════════════════════════════════════════"
echo ""
echo "Dieses WP ist DONE wenn und nur wenn:"
echo "  1. Alle Contract-Tests bestehen 3× hintereinander (Triple-Test)"
echo "  2. cargo clippy -- -D warnings ist grün"
echo "  3. GitHub Actions CI ist grün"
echo "  4. Kein einziger bestehender Test des Workspace ist neu rot"
echo ""
echo "Öffne den Pull Request NUR nach erfolgreichem Triple-Test."
if [ -n "${TASK_NR:-}" ]; then
    echo ""
    echo "Task-Nummer: ${TASK_NR}/15"
fi
