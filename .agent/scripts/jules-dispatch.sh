#!/bin/bash
# .agent/scripts/jules-dispatch.sh
# Identifiziert den nächsten Agenten anhand der "SUCCESSOR:" Tags im letzten Commit.

set -e

REPO_DIR="/home/freddy/Arbeitsplatz/DEV/memfuse"
cd "$REPO_DIR"

echo "=== MemFuse Dynamic Queue Dispatcher ==="

# Fallback values
TARGET_ID=""
INSTRUCTION=""

# Analysiere die geänderten Dateien des letzten Merges (HEAD^ zu HEAD)
# Wir suchen nach exakt dem neuen SUCCESSOR-Tag, den der Vorgänger-Agent platziert hat.
CHANGED_FILES=$(git diff --name-only HEAD^ HEAD || true)

for file in $CHANGED_FILES; do
    if [ -f "$file" ]; then
        if grep -q "SUCCESSOR: @JULES-" "$file" 2>/dev/null; then
            MATCH=$(grep "SUCCESSOR: @JULES-" "$file" | head -n 1)
            
            # Extract Account-ID (e.g. 05 from @JULES-05)
            # Nutzt grep -o um genau JULES-XX zu fangen und cut für die ID
            TARGET_ID=$(echo "$MATCH" | grep -o "@JULES-[0-9]\{2\}" | cut -d'-' -f2)
            
            # Extract Instruction (alles nach dem Gedankenstrich und Quotes)
            INSTRUCTION=$(echo "$MATCH" | sed -n 's/.*SUCCESSOR: @JULES-[0-9]\{2\} — "\(.*\)"/\1/p')
            
            if [ -z "$INSTRUCTION" ]; then
                INSTRUCTION=$(echo "$MATCH" | sed -n 's/.*SUCCESSOR: @JULES-[0-9]\{2\} — \(.*\)/\1/p')
            fi
            
            echo "SUCCESSOR-Tag gefunden in $file:"
            echo "-> Next Agent: $TARGET_ID"
            echo "-> Instruction: $INSTRUCTION"
            break
        fi
    fi
done

# Fallback: Wenn kein SUCCESSOR gefunden wurde oder etwas schiefgeht -> Debt Hunter (13) übernimmt
if [ -z "$TARGET_ID" ]; then
    echo "⚠️ Kein SUCCESSOR-Tag im letzten Commit gefunden!"
    TARGET_ID="13"
    INSTRUCTION="Stelle sicher, dass keine blockierten ANCHORs vorliegen und räume Codebase Debt auf. Prüfe ob der Conveyor-Belt gerissen ist."
    echo "-> Fallback auf Next Agent: $TARGET_ID"
fi

# Dispatch an GitHub Actions
echo "Triggering jules-invoke.yml for Account $TARGET_ID..."
gh workflow run jules-invoke.yml -f account_id="$TARGET_ID" -f instruction="$INSTRUCTION"
echo "✅ Dispatcher erfolgreich beendet."
