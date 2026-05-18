#!/bin/bash
# .agent/scripts/jules-sync-locks.sh
# Proaktives "Locking" System für Jules-Anchors v2.0
# Verhindert Merge-Konflikte durch Blockieren abhängiger Tasks

set -e

REPO_DIR="/home/freddy/Arbeitsplatz/DEV/memfuse"
cd "$REPO_DIR"

# Temporary file to store anchor states
TMP_ANCHORS=$(mktemp)

echo "=== MemFuse Jules-Lock-Manager ==="

# 1. Alle Anchors extrahieren
# Wir suchen nach JULES-ANCHOR Blöcken
grep -rn "// ⬡ @JULES-" --include="*.rs" --include="*.md" . > "$TMP_ANCHORS"

# 2. Identifiziere WIP-Tasks und deren Domains
WIP_DOMAINS=()
WIP_IDS=()

while IFS= read -r line; do
    file=$(echo "$line" | cut -d':' -f1)
    lineno=$(echo "$line" | cut -d':' -f2)
    # Hole den Task-Header und die nächsten 10 Zeilen für Metadaten
    anchor_block=$(sed -n "${lineno},$((lineno + 10))p" "$file")

    task_id=$(echo "$anchor_block" | grep -o "TODO:[A-Z]\+-[0-9]\+" | head -1 | cut -d':' -f2)
    status=$(echo "$anchor_block" | grep "STATUS:" | grep -o "[A-Z]\+" | head -2 | tail -1)

    if [ "$status" == "WIP" ]; then
        domain=$(echo "$task_id" | cut -d'-' -f1)
        WIP_DOMAINS+=("$domain")
        WIP_IDS+=("$task_id")
        echo "📍 WIP erkannt: $task_id (Domain: $domain)"
    fi
done < "$TMP_ANCHORS"

# 3. Blockiere Abhängige
if [ ${#WIP_IDS[@]} -eq 0 ]; then
    echo "✅ Keine aktiven WIP-Tasks. Alle Pfade frei."
else
    # Wir loopen erneut durch alle OPEN-Tasks und prüfen DEPS
    while IFS= read -r line; do
        file=$(echo "$line" | cut -d':' -f1)
        lineno=$(echo "$line" | cut -d':' -f2)
        anchor_block=$(sed -n "${lineno},$((lineno + 10))p" "$file")

        task_id=$(echo "$anchor_block" | grep -o "TODO:[A-Z]\+-[0-9]\+" | head -1 | cut -d':' -f2)
        status=$(echo "$anchor_block" | grep "STATUS:" | grep -o "[A-Z]\+" | head -2 | tail -1)
        deps=$(echo "$anchor_block" | grep "DEPS:" | cut -d':' -f2- | tr -d ' ')

        # Nur OPEN Tasks prüfen
        if [ "$status" == "OPEN" ]; then
            blocked=0
            # Prüfung gegen WIP_IDS
            for wip in "${WIP_IDS[@]}"; do
                if echo "$deps" | grep -q "$wip"; then
                    blocked=1
                    reason="Abhängigkeit von $wip"
                    break
                fi
            done

            # Domain-Lock Prüfung (Strategischer Schutz)
            # Wenn ein Basis-Crate (CORE, STORE) WIP ist, blockieren wir DB/PY Implementierungen
            current_domain=$(echo "$task_id" | cut -d'-' -f1)
            for wip_dom in "${WIP_DOMAINS[@]}"; do
                if [ "$wip_dom" == "CORE" ] && [ "$current_domain" != "CORE" ]; then
                    blocked=1
                    reason="Basis-Lock (CORE ist WIP)"
                elif [ "$wip_dom" == "STORE" ] && [[ "$current_domain" =~ ^(DB|PY|TEXT)$ ]]; then
                    blocked=1
                    reason="Basis-Lock (STORE ist WIP)"
                fi
            done

            if [ $blocked -eq 1 ]; then
                echo "🔒 Blockiere $task_id ($reason)"
                # STATUS von OPEN auf BLOCKED ändern
                # Wir suchen präzise nach STATUS:OPEN im Fenster
                local target_line=$(sed -n "$((lineno)),$((lineno+10))p" "$file" | grep -n "STATUS:OPEN" | cut -d':' -f1 | head -1)
                if [ -n "$target_line" ]; then
                    local real_lineno=$((lineno + target_line - 1))
                    sed -i "${real_lineno}s/STATUS:OPEN/STATUS:BLOCKED/" "$file"
                    # Grund vermerken (nach der STATUS Zeile)
                    sed -i "$((real_lineno + 1))i // LOCK-BY: $reason ($(date -u +%Y-%m-%dT%H:%M:%SZ))" "$file"
                fi
            fi
        fi
    done < "$TMP_ANCHORS"
fi

rm "$TMP_ANCHORS"
echo "=== Lock-Sync abgeschlossen ==="
