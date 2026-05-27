#!/bin/bash
# .agent/scripts/gemini-squad-call.sh
# Nutzt einen der 13 Gemini-API-Keys für automatisierte Anfragen

PROMPT=$1
[ -z "$PROMPT" ] && echo "Ziele einen Prompt an." && exit 1

# Liste der Secrets-Namen
KEYS=("GEMINI_API_KEY_FRANZ" "GEMINI_API_KEY_FRED" "GEMINI_API_KEY_FS11" "GEMINI_API_KEY_FSENG" "GEMINI_API_KEY_FUZFOY" "GEMINI_API_KEY_KAISER" "GEMINI_API_KEY_MAMASPO" "GEMINI_API_KEY_ME1NS" "GEMINI_API_KEY_POSTLER" "GEMINI_API_KEY_SEFRE" "GEMINI_API_KEY_SENG" "GEMINI_API_KEY_TFUFU" "GEMINI_API_KEY_TRULLI")

# Wähle einen zufälligen Key aus der Liste (Load Balancing)
RANDOM_INDEX=$((RANDOM % 13))
SELECTED_KEY_NAME=${KEYS[$RANDOM_INDEX]}

# Hole den Key aus dem Environment (muss im Workflow gemappt sein)
API_KEY=$(eval echo "\$$SELECTED_KEY_NAME")

if [ -z "$API_KEY" ]; then
    echo "Fehler: Key $SELECTED_KEY_NAME nicht im Environment gefunden."
    exit 1
fi

echo "--- Nutze Squad-Account: ${SELECTED_KEY_NAME#GEMINI_API_KEY_} ---"
GEMINI_API_KEY=$API_KEY gemini "$PROMPT"
