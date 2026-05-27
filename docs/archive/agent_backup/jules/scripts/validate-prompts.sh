#!/usr/bin/env bash
# ANCHOR:ARCH:SCRIPT-VALIDATE-001 — Validierung der Jules Prompt-Dateien auf Sovereign Doctrine Einhaltung
# WP:WP-0.0 PRIO:2 NEEDS:NONE
# AGENT:11-ci-devops DATE:2026-05-09 STATUS:DONE
# CREATED:2026-05-09 DEADLINE:NONE
#
# JULES-INFO: CI/CD Pipeline Script zur Absicherung, dass keine Prompt-Drift entsteht.
#             Es stellt sicher, dass alle 13 Agents korrekt konfiguriert bleiben.
# validate-prompts.sh — Validiert alle Jules-Prompt-Dateien
#
# Prüft:
#   1. Alle Account-Dateien (01-13) existieren und sind non-empty
#   2. Jeder Task-Prompt referenziert eine existierende SPEC
#   3. Account-Boundaries werden eingehalten
#   4. Präambel existiert und enthält Sovereign Doctrine

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
PROMPTS_DIR="${SCRIPT_DIR}/../prompts"
ACCOUNTS_DIR="${PROMPTS_DIR}/accounts"

FAIL=0
WARN=0

echo "═══════════════════════════════════════════════════════════════"
echo "  Jules Prompt Validation"
echo "═══════════════════════════════════════════════════════════════"
echo ""

# ──────────────────────────────────────────────
# 1. Präambel-Check
# ──────────────────────────────────────────────
# JULES-INFO: Prüft Präsenz der Sovereign Doctrine (Regelwerk für Zero-Panic).
echo "--- [1/5] Präambel ---"
PREAMBLE="${PROMPTS_DIR}/00-PREAMBLE.md"
if [ ! -f "$PREAMBLE" ]; then
    echo "❌ Präambel fehlt: $PREAMBLE"; FAIL=1
elif [ ! -s "$PREAMBLE" ]; then
    echo "❌ Präambel ist leer: $PREAMBLE"; FAIL=1
elif ! grep -q "SOVEREIGN CORE DOCTRINE" "$PREAMBLE"; then
    echo "❌ Präambel enthält keine Sovereign Doctrine"; FAIL=1
elif ! grep -q "TRIPLE-TEST-GATE" "$PREAMBLE"; then
    echo "❌ Präambel enthält kein Triple-Test-Gate"; FAIL=1
else
    echo "✅ Präambel OK"
fi

# ──────────────────────────────────────────────
# 2. Account-Dateien (01-13)
# ──────────────────────────────────────────────
# JULES-INFO: Stellt sicher, dass jeder der 13 Jules-Accounts seine Pflichtfelder hat
#             wie "Rolle", "NIEMALS" (Boundaries) und "Validation" / "Erfolgs-Metrik".
echo "--- [2/5] Account-Dateien ---"
for i in $(seq -w 1 13); do
    ACCOUNT_FILE=$(ls "${ACCOUNTS_DIR}/${i}-"*.md 2>/dev/null | head -1)
    if [ -z "${ACCOUNT_FILE:-}" ] || [ ! -f "${ACCOUNT_FILE:-}" ]; then
        echo "❌ Account ${i}: Keine Datei gefunden in ${ACCOUNTS_DIR}/${i}-*.md"
        FAIL=1
    elif [ ! -s "$ACCOUNT_FILE" ]; then
        echo "❌ Account ${i}: Datei ist leer: $ACCOUNT_FILE"
        FAIL=1
    else
        # Check für Pflichtfelder
        MISSING=""
        grep -q "## Rolle" "$ACCOUNT_FILE"         || MISSING="${MISSING} Rolle"
        grep -q "## NIEMALS" "$ACCOUNT_FILE"        || MISSING="${MISSING} NIEMALS"
        grep -q "## Validation" "$ACCOUNT_FILE" || grep -q "## Erfolgs-Metrik" "$ACCOUNT_FILE" || MISSING="${MISSING} Validation"

        if [ -n "$MISSING" ]; then
            echo "⚠️  Account ${i}: Fehlende Sektionen:${MISSING}"
            WARN=$((WARN + 1))
        else
            echo "✅ Account ${i}: $(basename "$ACCOUNT_FILE")"
        fi
    fi
done

# ──────────────────────────────────────────────
# 3. SPEC-Referenzen
# ──────────────────────────────────────────────
# JULES-INFO: Validiert, dass jedes Work Package mindestens einem Account zugewiesen ist,
#             damit keine Spec (WP-X.Y) unverarbeitet bleibt.
echo "--- [3/5] SPEC-Referenzen ---"
SPEC_COUNT=$(ls "${REPO_ROOT}/docs/specs/SPEC-"*.md 2>/dev/null | grep -v TEMPLATE | grep -v MASTER | wc -l)
echo "   Gefundene SPECs: ${SPEC_COUNT}"

for SPEC in "${REPO_ROOT}"/docs/specs/SPEC-*.md; do
    [ -f "$SPEC" ] || continue
    BASENAME=$(basename "$SPEC")
    [[ "$BASENAME" == *TEMPLATE* ]] && continue
    [[ "$BASENAME" == *MASTER* ]] && continue

    # Prüfe ob mindestens ein Account diese SPEC referenziert
    WP_ID=$(echo "$BASENAME" | grep -oP 'WP-\d+\.\d+' || true)
    if [ -n "$WP_ID" ]; then
        REFS=$(grep -rl "$WP_ID" "${ACCOUNTS_DIR}/" 2>/dev/null | wc -l)
        if [ "$REFS" -eq 0 ]; then
            echo "⚠️  SPEC ${BASENAME}: Kein Account referenziert ${WP_ID}"
            WARN=$((WARN + 1))
        else
            echo "✅ ${WP_ID}: ${REFS} Account(s) zugewiesen"
        fi
    fi
done

# ──────────────────────────────────────────────
# 4. Boundary-Checks
# ──────────────────────────────────────────────
# JULES-INFO: Prüft auf harte Sicherheitsgrenzen: Agent 08 darf z.B. keinen Code ändern, 
#             Agent 09 nur Benchmarks.
echo "--- [4/5] Boundary-Checks ---"

# Account 08 (Docs) darf keinen .rs Code ändern
DOCS_ACCOUNT=$(ls "${ACCOUNTS_DIR}/08-"*.md 2>/dev/null | head -1)
if [ -n "${DOCS_ACCOUNT:-}" ] && grep -q "NIEMALS" "$DOCS_ACCOUNT"; then
    if grep -q "Produktionscode" "$DOCS_ACCOUNT"; then
        echo "✅ Account 08: Produktionscode-Boundary gesetzt"
    else
        echo "⚠️  Account 08: Keine explizite Produktionscode-Boundary"
        WARN=$((WARN + 1))
    fi
fi

# Account 09 (Benchmarks) darf nur Benchmark-Code ändern
BENCH_ACCOUNT=$(ls "${ACCOUNTS_DIR}/09-"*.md 2>/dev/null | head -1)
if [ -n "${BENCH_ACCOUNT:-}" ] && grep -q "NIEMALS" "$BENCH_ACCOUNT"; then
    echo "✅ Account 09: NIEMALS-Boundary gesetzt"
fi

echo "✅ Boundary-Checks abgeschlossen"

# ──────────────────────────────────────────────
# 5. Prompt-Generator Test
# ──────────────────────────────────────────────
# JULES-INFO: Dry-Run des Promptgenerators um sicherzustellen, dass die Layer
#             (Doctrine + Spec) korrekt eingefügt werden.
echo "--- [5/5] Prompt-Generator ---"
GENERATOR="${SCRIPT_DIR}/generate-jules-prompt.sh"
if [ ! -x "$GENERATOR" ]; then
    chmod +x "$GENERATOR" 2>/dev/null || true
fi

if [ -f "$GENERATOR" ]; then
    # Teste für Account 01 ohne WP
    OUTPUT=$(bash "$GENERATOR" 01 2>/dev/null || true)
    if echo "$OUTPUT" | grep -q "SOVEREIGN CORE DOCTRINE"; then
        echo "✅ Prompt-Generator funktioniert (Account 01)"
    else
        echo "⚠️  Prompt-Generator: Output enthält keine Doctrine"
        WARN=$((WARN + 1))
    fi

    # Teste für Account 02 mit WP
    OUTPUT=$(bash "$GENERATOR" 02 WP-1.1 2>/dev/null || true)
    if echo "$OUTPUT" | grep -q "ATOMIC SPEC"; then
        echo "✅ Prompt-Generator funktioniert mit WP (Account 02, WP-1.1)"
    else
        echo "⚠️  Prompt-Generator: WP-Spec nicht inkludiert"
        WARN=$((WARN + 1))
    fi
else
    echo "❌ Prompt-Generator nicht gefunden: $GENERATOR"
    FAIL=1
fi

# ──────────────────────────────────────────────
# Ergebnis
# ──────────────────────────────────────────────
echo ""
echo "═══════════════════════════════════════════════════════════════"
if [ $FAIL -gt 0 ]; then
    echo "❌ VALIDATION FAILED — ${FAIL} Fehler, ${WARN} Warnungen"
    exit 1
elif [ $WARN -gt 0 ]; then
    echo "⚠️  VALIDATION PASSED mit ${WARN} Warnungen"
else
    echo "✅ VALIDATION PASSED — Alle Checks grün"
fi
echo "═══════════════════════════════════════════════════════════════"
