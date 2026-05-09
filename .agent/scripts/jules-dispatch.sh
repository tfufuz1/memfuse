#!/bin/bash
# .agent/scripts/jules-dispatch.sh
# Berechnet die nächste Jules-Instanz in der Queue und triggert sie.

set -e

REPO_DIR="/home/freddy/Arbeitsplatz/DEV/memfuse"
cd "$REPO_DIR"

# Reihenfolge der Accounts (Queue)
QUEUE=(13 01 02 03 04 05 06 10 07 12 09)

# 1. Identifiziere den letzten erfolgreichen PR-Author
# Wir schauen uns das letzte Commit-Subject an (z.B. "Merge pull request #5 from jules/01/...")
LAST_COMMIT=$(git log -1 --pretty=%s)
LAST_AUTHOR_ID=$(echo "$LAST_COMMIT" | grep -o "jules/[0-9]\{2\}" | cut -d'/' -f2 || echo "None")

echo "Letzter erfolgreicher Jules-Account: $LAST_AUTHOR_ID"

# 2. Finde die nächste Position in der Queue
NEXT_ID=""
for i in "${!QUEUE[@]}"; do
    if [ "${QUEUE[$i]}" == "$LAST_AUTHOR_ID" ]; then
        NEXT_INDEX=$(( (i + 1) % ${#QUEUE[@]} ))
        NEXT_ID=${QUEUE[$NEXT_INDEX]}
        break
    fi
done

# Fallback: Wenn kein Jules-Account gefunden wurde, fange bei 13 (Debt Hunter) an
if [ -z "$NEXT_ID" ]; then
    NEXT_ID=13
fi

echo "Nächster Account zum Triggern: $NEXT_ID"

# 3. Triggere den Workflow für den nächsten Account
# Wir nutzen gh workflow run und übergeben die ID
gh workflow run jules-invoke.yml -f account_id="$NEXT_ID"
