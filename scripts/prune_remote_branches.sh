#!/usr/bin/env bash
# ZWECK: Automatisches Audit und Pruning veralteter Agenten- & Task-Branches auf origin.
# INVARIANTEN: Geschützte Branches (main/master/HEAD) werden NIEMALS gelöscht.
# STAND: TS: 2026-09-05T16:05:00Z (SESSION: 0dcb9f3b)

set -euo pipefail

DRY_RUN=true
MERGED_ONLY=false
ALL_STALE=false
DAYS_THRESHOLD=3
PROTECTED_BRANCHES=("main" "master" "HEAD")

usage() {
    cat << EOF
Verwendung: $0 [OPTIONEN]

Optionen:
  --dry-run             Simuliert das Löschen (Standardmodus, keine Löschung auf origin).
  --apply               Führt die Löschbefehle tatsächlich auf origin aus.
  --merged-only         Löscht nur direkt gemergte & 0-Diff Branches.
  --older-than DAYS     Löscht automatische Task-Branches älter als DAYS Tage (Standard: 3).
  --all-stale           Kombiniert --merged-only und --older-than (Standard 3 Tage).
  -h, --help            Zeigt diese Hilfe an.
EOF
    exit 0
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --apply)
            DRY_RUN=false
            shift
            ;;
        --dry-run)
            DRY_RUN=true
            shift
            ;;
        --merged-only)
            MERGED_ONLY=true
            shift
            ;;
        --older-than)
            DAYS_THRESHOLD="$2"
            shift 2
            ;;
        --all-stale)
            ALL_STALE=true
            shift
            ;;
        -h|--help)
            usage
            ;;
        *)
            echo "Unbekannte Option: $1"
            usage
            ;;
    esac
done

if [ "$ALL_STALE" = true ]; then
    MERGED_ONLY=false
fi

echo "=== MemFuse Remote Branch Pruning Utility ==="
echo "Modus: $( [ "$DRY_RUN" = true ] && echo "DRY-RUN (Simulation — keine Änderungen auf origin)" || echo "LIVE APPLY (Löschen auf origin)" )"
echo "Fetch & Synchronisiere Remote-Tracking-Branches..."
git fetch --prune origin >/dev/null 2>&1

NOW=$(date +%s)
CUTOFF_SECS=$((DAYS_THRESHOLD * 86400))

# 1. Direct merged branches
MERGED_TMP=$(mktemp)
git branch -r --merged origin/main | sed 's#origin/##' | tr -d ' ' | grep -v '^main$' | grep -v '^HEAD$' > "$MERGED_TMP" || true

# 2. Bulk fetch all refs with timestamps
REFS_TMP=$(mktemp)
git for-each-ref --format='%(refname:short)|%(committerdate:unix)' refs/remotes/origin | sed 's#origin/##' > "$REFS_TMP"

TOTAL_BRANCHES=$(wc -l < "$REFS_TMP")
echo "Gesamtzahl Remote-Branches auf origin: $TOTAL_BRANCHES"

TO_DELETE_MERGED=()
TO_DELETE_STALE=()

while IFS='|' read -r branch commit_ts; do
    [ -z "$branch" ] && continue
    [ "$branch" = "origin" ] && continue
    
    # Check protected
    is_protected=false
    for p in "${PROTECTED_BRANCHES[@]}"; do
        if [ "$branch" = "$p" ]; then
            is_protected=true
            break
        fi
    done
    [ "$is_protected" = true ] && continue

    # Check direct merged
    if grep -q "^${branch}$" "$MERGED_TMP"; then
        TO_DELETE_MERGED+=("$branch")
        continue
    fi

    # If merged-only mode is active, skip further unmerged checks
    if [ "$MERGED_ONLY" = true ]; then
        continue
    fi

    # Check if automated agent/task branch
    is_auto_task=false
    if [[ "$branch" =~ -[0-9]{15,}$ ]] || [[ "$branch" =~ ^audit-memfuse- ]] || [[ "$branch" =~ ^jules- ]]; then
        is_auto_task=true
    fi

    if [ "$is_auto_task" = true ]; then
        age=$((NOW - commit_ts))
        if [ "$age" -gt "$CUTOFF_SECS" ]; then
            TO_DELETE_STALE+=("$branch")
        fi
    fi
done < "$REFS_TMP"

rm -f "$MERGED_TMP" "$REFS_TMP"

COUNT_MERGED=${#TO_DELETE_MERGED[@]}
COUNT_STALE=${#TO_DELETE_STALE[@]}
TOTAL_DELETE=$((COUNT_MERGED + COUNT_STALE))

echo "--------------------------------------------------------"
echo "Befund:"
echo "  - Direkt gemergte Branches:               $COUNT_MERGED"
echo "  - Inaktive Auto-Task-Branches (>$DAYS_THRESHOLD Tage): $COUNT_STALE"
echo "  - Gesamtzahl zu löschender Branches:       $TOTAL_DELETE"
echo "--------------------------------------------------------"

if [ "$TOTAL_DELETE" -eq 0 ]; then
    echo "Keine veralteten oder gemergten Branches zum Löschen gefunden."
    exit 0
fi

ALL_DELETE=("${TO_DELETE_MERGED[@]}" "${TO_DELETE_STALE[@]}")

if [ "$DRY_RUN" = true ]; then
    echo ""
    echo "--- Vorschau der zu löschenden Branches (ersten 30) ---"
    printf '%s\n' "${ALL_DELETE[@]}" | head -n 30
    if [ "$TOTAL_DELETE" -gt 30 ]; then
        echo "... und $((TOTAL_DELETE - 30)) weitere."
    fi
    echo ""
    echo "💡 Um diese $TOTAL_DELETE Branches tatsächlich auf origin zu löschen, führe aus:"
    echo "  ./scripts/prune_remote_branches.sh --apply --older-than $DAYS_THRESHOLD"
else
    echo "Lösche $TOTAL_DELETE Branches auf origin in 50er-Batches..."
    BATCH_SIZE=50
    for ((i=0; i<TOTAL_DELETE; i+=BATCH_SIZE)); do
        batch=("${ALL_DELETE[@]:i:BATCH_SIZE}")
        echo "Lösche Batch $((i/BATCH_SIZE + 1)) / $(( (TOTAL_DELETE + BATCH_SIZE - 1) / BATCH_SIZE ))... (${#batch[@]} Branches)"
        git push origin --delete "${batch[@]}"
    done
    echo "✅ Remote-Branch-Pruning erfolgreich abgeschlossen!"
fi
