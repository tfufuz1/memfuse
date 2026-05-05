#!/usr/bin/env bash
# generate-jules-prompt.sh — Generiert vollständigen Jules-Task-Prompt mit dynamischem Kontext
# VERWENDUNG: bash generate-jules-prompt.sh WP-1.1 > /tmp/jules-prompt.txt
#
# Kombiniert:
#   1. Dynamischen API-Kontext (frisch extrahiert)
#   2. Standard-Präambel (aus 00-PREAMBLE.md)
#   3. WP-spezifischen Prompt-Body (aus prompts/wp-*.md)

set -euo pipefail

WP="${1:-}"
if [ -z "$WP" ]; then
    echo "Usage: $0 WP-X.Y" >&2; exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
PROMPTS_DIR="${SCRIPT_DIR}/../prompts"

# 1. Dynamischer API-Kontext
echo "═══════════════════════════════════════════════════════════════"
echo "  CURRENT REPOSITORY STATE (auto-generated, not from prompt)"
echo "═══════════════════════════════════════════════════════════════"
echo ""
bash "${SCRIPT_DIR}/inject-context.sh"
echo ""

# 2. Standard-Präambel
echo "═══════════════════════════════════════════════════════════════"
echo "  SOVEREIGN CORE DOCTRINE & TRIPLE-TEST-GATE"
echo "═══════════════════════════════════════════════════════════════"
echo ""
# Extrahiere nur den Präambel-Block (zwischen den ═══ Markierungen)
cat "${PROMPTS_DIR}/00-PREAMBLE.md" | grep -A 200 "PRÄAMBEL" | head -60
echo ""

# 3. WP-Spezifikation
WP_SLUG=$(echo "$WP" | tr '[:upper:]' '[:lower:]' | sed 's/\./-/g')
WP_SPEC="${REPO_ROOT}/docs/specs/SPEC-*-${WP}.md"
if ls $WP_SPEC 2>/dev/null | head -1 | grep -q "."; then
    SPEC_FILE=$(ls $WP_SPEC 2>/dev/null | head -1)
    echo "═══════════════════════════════════════════════════════════════"
    echo "  ATOMIC SPEC: ${WP}"
    echo "═══════════════════════════════════════════════════════════════"
    echo ""
    cat "$SPEC_FILE"
    echo ""
fi

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
