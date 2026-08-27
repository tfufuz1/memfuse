#!/usr/bin/env bash
# =============================================================================
# RESTORE_COMMANDS.sh
# Stellt die Kern-Komponenten aus memfuse-saos-agent (gelöscht in Commit
# 55a3464 am 2026-08-22) als neues Crate `memfuse-agent` wieder her.
#
# Quelle: Commit ddc4c77 (letzter Commit VOR der Löschung)
# Ziel:   crates/memfuse-agent/   (neuer, angepasster Name)
#
# Ausführung (im Repo-Root):
#   chmod +x RESTORE_COMMANDS.sh && ./RESTORE_COMMANDS.sh
# =============================================================================
set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
cd "$REPO_ROOT"

SOURCE_COMMIT="ddc4c77"
NEW_CRATE_DIR="crates/memfuse-agent"

echo "► Prüfe Commit ${SOURCE_COMMIT}…"
git cat-file -t "${SOURCE_COMMIT}" > /dev/null 2>&1 || {
  echo "FEHLER: Commit ${SOURCE_COMMIT} nicht gefunden. Bitte git fetch origin durchführen."
  exit 1
}

echo "► Erstelle Verzeichnisstruktur für ${NEW_CRATE_DIR}…"
mkdir -p "${NEW_CRATE_DIR}/src"
mkdir -p "${NEW_CRATE_DIR}/tests"

# ─── Quelldateien aus Git-History extrahieren ─────────────────────────────────
echo "► Extrahiere Quelldateien aus ${SOURCE_COMMIT}…"

git show "${SOURCE_COMMIT}:crates/memfuse-saos-agent/src/step.rs"    > "${NEW_CRATE_DIR}/src/step.rs"
git show "${SOURCE_COMMIT}:crates/memfuse-saos-agent/src/context.rs" > "${NEW_CRATE_DIR}/src/context.rs"
git show "${SOURCE_COMMIT}:crates/memfuse-saos-agent/src/graph.rs"   > "${NEW_CRATE_DIR}/src/graph.rs"
git show "${SOURCE_COMMIT}:crates/memfuse-saos-agent/src/engine.rs"  > "${NEW_CRATE_DIR}/src/engine.rs"
git show "${SOURCE_COMMIT}:crates/memfuse-saos-agent/src/audit.rs"   > "${NEW_CRATE_DIR}/src/audit.rs"
git show "${SOURCE_COMMIT}:crates/memfuse-saos-agent/src/lib.rs"     > "${NEW_CRATE_DIR}/src/lib.rs"

# ─── Test-Dateien extrahieren ─────────────────────────────────────────────────
echo "► Extrahiere Tests aus ${SOURCE_COMMIT}…"

git show "${SOURCE_COMMIT}:crates/memfuse-saos-agent/tests/e2e_integration.rs"  > "${NEW_CRATE_DIR}/tests/e2e_integration.rs"
git show "${SOURCE_COMMIT}:crates/memfuse-saos-agent/tests/workflow_tests.rs"   > "${NEW_CRATE_DIR}/tests/workflow_tests.rs"
git show "${SOURCE_COMMIT}:crates/memfuse-saos-agent/tests/agent_recovery.rs"  > "${NEW_CRATE_DIR}/tests/agent_recovery.rs"
git show "${SOURCE_COMMIT}:crates/memfuse-saos-agent/tests/persistence_test.rs"> "${NEW_CRATE_DIR}/tests/persistence_test.rs"
git show "${SOURCE_COMMIT}:crates/memfuse-saos-agent/tests/graph_integration.rs"> "${NEW_CRATE_DIR}/tests/graph_integration.rs"
git show "${SOURCE_COMMIT}:crates/memfuse-saos-agent/tests/final_state_test.rs" > "${NEW_CRATE_DIR}/tests/final_state_test.rs"
git show "${SOURCE_COMMIT}:crates/memfuse-saos-agent/tests/contract_tests.rs"  > "${NEW_CRATE_DIR}/tests/contract_tests.rs"

# ─── Cargo.toml erzeugen (angepasst: kein memfuse-sandbox, korrekter Name) ──
echo "► Erzeuge ${NEW_CRATE_DIR}/Cargo.toml…"
cat > "${NEW_CRATE_DIR}/Cargo.toml" << 'EOF'
[package]
name = "memfuse-agent"
version.workspace = true
edition.workspace = true
license.workspace = true
rust-version.workspace = true
description = "Persistent agent workflow engine for MemFuse — checkpoint/execute/audit loop"

[dependencies]
memfuse-core       = { workspace = true }
memfuse-db         = { workspace = true }
memfuse-graph      = { workspace = true }
memfuse-checkpoint = { workspace = true }
memfuse-store      = { workspace = true }
serde              = { workspace = true, features = ["derive"] }
serde_json         = { workspace = true }
tracing            = { workspace = true }
tokio              = { workspace = true, features = ["full"] }
async-trait        = { workspace = true }

[dev-dependencies]
tempfile           = { workspace = true }
tracing-subscriber = { workspace = true, features = ["env-filter"] }
EOF

# ─── Crate in workspace/Cargo.toml eintragen ──────────────────────────────────
echo "► Trage crate in workspace Cargo.toml ein…"

# Prüfe ob bereits eingetragen
if grep -q '"crates/memfuse-agent"' Cargo.toml; then
  echo "  ℹ️  memfuse-agent ist bereits in der Workspace eingetragen."
else
  # Füge nach memfuse-mcp ein (alphabetisch passend)
  sed -i 's|    "crates/memfuse-mcp",|    "crates/memfuse-mcp",\n    "crates/memfuse-agent",|' Cargo.toml
  echo "  ✓ memfuse-agent in workspace Cargo.toml eingetragen."
fi

# Füge workspace-dependency ein (nach memfuse-mcp)
if grep -q 'memfuse-agent = ' Cargo.toml; then
  echo "  ℹ️  memfuse-agent workspace dependency bereits vorhanden."
else
  sed -i 's|memfuse-mcp   = { path = "crates/memfuse-mcp"   }|memfuse-mcp   = { path = "crates/memfuse-mcp"   }\nmemfuse-agent = { path = "crates/memfuse-agent" }|' Cargo.toml
  echo "  ✓ memfuse-agent workspace dependency eingetragen."
fi

# ─── Zusammenfassung ──────────────────────────────────────────────────────────
echo ""
echo "═══════════════════════════════════════════════════════"
echo "  Extraktion abgeschlossen. Nächste Schritte:"
echo ""
echo "  1. Namens-Patches anwenden:"
echo "     grep -rl 'memfuse_saos_agent' ${NEW_CRATE_DIR}/ | \\"
echo "       xargs sed -i 's/memfuse_saos_agent/memfuse_agent/g'"
echo "     grep -rl 'memfuse-saos-agent' ${NEW_CRATE_DIR}/ | \\"
echo "       xargs sed -i 's/memfuse-saos-agent/memfuse-agent/g'"
echo ""
echo "  2. Ersten Build-Check (erwartet Kompilierfehler — normal):"
echo "     cargo check -p memfuse-agent 2>&1 | head -40"
echo ""
echo "  3. Integrations-Prompt ausführen (AGENT_INTEGRATION_PROMPT.md)"
echo "     — enthält alle nötigen API-Anpassungen."
echo "═══════════════════════════════════════════════════════"
