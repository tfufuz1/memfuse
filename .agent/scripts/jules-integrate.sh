#!/bin/bash
# .agent/scripts/jules-integrate.sh
# Automatisierte Integration von Jules-PRs

set -e

REPO_DIR=$(pwd)
cd "$REPO_DIR"

echo "--- Jules Integration Check ---"

# Suche nach offenen PRs mit dem Label 'jules'
if command -v gh &> /dev/null; then
    PRS=$(gh pr list --label jules --json number,title,state,mergeable,statusCheckRollup --jq '.[] | select(.state=="OPEN")')
else
    echo "⚠️  gh tool not found. Skipping PR integration."
    exit 0
fi

if [ -z "$PRS" ]; then
    echo "✅ Keine offenen Jules-PRs gefunden."
    exit 0
fi

# Iteriere über die PRs
echo "$PRS" | jq -c '.' | while read -r pr; do
    NUMBER=$(echo "$pr" | jq -r '.number')
    TITLE=$(echo "$pr" | jq -r '.title')
    MERGEABLE=$(echo "$pr" | jq -r '.mergeable')
    
    # Prüfe Status-Checks (GitHub Actions)
    # statusCheckRollup enthält ein Array von Checks. Wir suchen nach 'FAILURE' oder 'PENDING'.
    STATUS=$(echo "$pr" | jq -r '.statusCheckRollup[]?.status // "UNKNOWN"')
    CONCLUSION=$(echo "$pr" | jq -r '.statusCheckRollup[]?.conclusion // "UNKNOWN"')
    
    echo "Prüfe PR #$NUMBER: $TITLE"
    
    if [[ "$MERGEABLE" != "MERGEABLE" ]]; then
        echo "⚠️  PR #$NUMBER ist nicht mergeable (Konflikte?). Überspringe."
        continue
    fi

    # Prüfe ob alle Checks erfolgreich waren
    # Wir filtern nach 'COMPLETED' und 'SUCCESS'. 
    # Falls es einen 'FAILURE' gibt, brechen wir für diesen PR ab.
    FAILURES=$(echo "$pr" | jq -r '.statusCheckRollup[] | select(.conclusion=="FAILURE" or .conclusion=="CANCELLED" or .conclusion=="TIMED_OUT") | .conclusion')
    
    if [ -n "$FAILURES" ]; then
        echo "❌ PR #$NUMBER hat fehlgeschlagene Checks. Überspringe."
        continue
    fi
    
    PENDING=$(echo "$pr" | jq -r '.statusCheckRollup[] | select(.status=="IN_PROGRESS" or .status=="QUEUED" or .status=="WAITING") | .status')
    if [ -n "$PENDING" ]; then
        echo "⏳ PR #$NUMBER hat noch laufende Checks. Überspringe."
        continue
    fi

    echo "🚀 PR #$NUMBER ist bereit für die Integration. Starte Merge..."
    gh pr merge "$NUMBER" --merge --auto --delete-branch
    echo "✅ PR #$NUMBER erfolgreich integriert."
done
