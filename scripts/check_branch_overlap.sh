#!/usr/bin/env bash
# Prüft ob ein Branch Dateien berührt, die auch in offenen jules-*-Branches geändert wurden.
# Verwendung: bash scripts/check_branch_overlap.sh <CURRENT_BRANCH>
# Gibt Warnungen aus aber bricht nicht ab (advisory only).

set -euo pipefail

CURRENT_BRANCH="${1:-$(git rev-parse --abbrev-ref HEAD)}"
BASE="origin/main"
MAX_BRANCHES=30  # Performance-Grenze

# Ref-Resolution: Falls CURRENT_BRANCH kein lokaler Ref ist (z.B. in CI head_ref "jules-..."),
# prüfe ob "origin/$CURRENT_BRANCH" existiert, sonst verwende "HEAD".
if git rev-parse --verify "$CURRENT_BRANCH" >/dev/null 2>&1; then
    CURRENT_REF="$CURRENT_BRANCH"
elif git rev-parse --verify "origin/$CURRENT_BRANCH" >/dev/null 2>&1; then
    CURRENT_REF="origin/$CURRENT_BRANCH"
else
    CURRENT_REF="HEAD"
fi

echo "=== Branch-Overlap-Check für: $CURRENT_BRANCH ($CURRENT_REF) ==="

# Eigene geänderte Dateien
CURRENT_FILES=$(git diff --name-only "$BASE"..."$CURRENT_REF" 2>/dev/null || true)
if [ -z "$CURRENT_FILES" ]; then
    echo "ℹ️ Keine Dateiänderungen im Branch — kein Check nötig."
    exit 0
fi

# Alle offenen jules-* Remote-Branches (exkl. current)
REMOTE_BRANCHES=$(git branch -r --sort=-committerdate 2>/dev/null | grep -E "origin/jules-|origin/feat/|origin/fix/" \
    | grep -v "$CURRENT_BRANCH" | head -$MAX_BRANCHES || true)

OVERLAP_FOUND=false

while IFS= read -r branch; do
    branch=$(echo "$branch" | xargs)  # trim whitespace
    [ -z "$branch" ] && continue
    BRANCH_FILES=$(git diff --name-only "$BASE"..."$branch" 2>/dev/null || true)
    [ -z "$BRANCH_FILES" ] && continue

    # Schnittmenge berechnen
    OVERLAP=$(comm -12 \
        <(echo "$CURRENT_FILES" | sort) \
        <(echo "$BRANCH_FILES" | sort) 2>/dev/null || true)

    if [ -n "$OVERLAP" ]; then
        echo "⚠️  OVERLAP mit Branch: $branch"
        echo "   Gemeinsame Dateien:"
        echo "$OVERLAP" | sed 's/^/   - /'
        OVERLAP_FOUND=true
    fi
done <<< "$REMOTE_BRANCHES"

if [ "$OVERLAP_FOUND" = true ]; then
    echo ""
    echo "⚠️  WARNUNG: Dieser Branch überschneidet sich mit offenen Branches."
    echo "   Koordination empfohlen, bevor Arbeit beginnt."
    echo "   (Kein harter Fehler — advisory only)"
else
    echo "✅ Kein Overlap mit anderen offenen Branches."
fi
