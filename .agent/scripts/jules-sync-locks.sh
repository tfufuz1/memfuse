#!/bin/bash
# .agent/scripts/jules-sync-locks.sh
# Proaktives "Locking" System (AST-basiert) für SAOS Conveyor Belt

set -euo pipefail

REPO_DIR="/home/freddy/Arbeitsplatz/DEV/memfuse"
cd "$REPO_DIR"

echo "=== MemFuse Jules-Lock-Manager (AST ver.) ==="

if ! command -v sg > /dev/null; then
    echo "⚠️ ast-grep (sg) is not installed. Please install it with 'npm install -g @ast-grep/cli'."
    echo "Using legacy bypass for now."
    exit 0
fi

# AST-based validation of state transitions and MVCC isolation
# We verify the state using ast-grep instead of textual JULES-ANCHOR comments
echo "Running State Transition AST validation..."
if sg scan --rule rules/verify_state_transitions.yml crates/; then
    echo "✅ No logical state transition violations found."
else
    echo "❌ AST validation found state transitions that are not properly synchronized."
    echo "Failing dispatcher to prevent Context Drift."
    exit 1
fi

echo "=== Lock-Sync abgeschossen (AST-Verified) ==="
exit 0
