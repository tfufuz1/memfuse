#!/usr/bin/env bash
# Schritt 2 — Python-venv entfernen (crates/memfuse-py/.venv/)
# Führe 01-inventory.sh zuerst aus und prüfe die Ausgabe manuell,
# bevor du dieses Skript laufen lässt.
set -euo pipefail

if [ ! -d ".git" ]; then
  echo "FEHLER: Kein .git-Verzeichnis hier gefunden. Bitte im Repo-Root ausführen." >&2
  exit 1
fi

VENV_PATH="crates/memfuse-py/.venv"

if [ ! -e "$VENV_PATH" ] && ! git ls-files --error-unmatch "$VENV_PATH" >/dev/null 2>&1; then
  echo "Hinweis: $VENV_PATH scheint bereits entfernt/nicht getrackt zu sein. Nichts zu tun."
  exit 0
fi

echo "=== Zeige zuerst, was in crates/memfuse-py/python/ und crates/memfuse-py/src/ liegt ==="
echo "(dies bleibt erhalten — nur .venv/ wird gelöscht)"
find crates/memfuse-py/python -type f 2>/dev/null
find crates/memfuse-py/src -type f 2>/dev/null
echo

read -p "Sind die obigen Dateien legitime Projektdateien und soll nur .venv/ gelöscht werden? [j/N] " confirm
if [[ ! "$confirm" =~ ^[jJ]$ ]]; then
  echo "Abgebrochen."
  exit 1
fi

echo "=== Entferne $VENV_PATH aus dem Git-Index ==="
git rm -r --cached "$VENV_PATH" --quiet

echo "=== Entferne $VENV_PATH aus dem Arbeitsverzeichnis ==="
rm -rf "$VENV_PATH"

echo "=== Committe die Änderung ==="
git commit -m "chore: remove accidentally committed Python venv

Removes crates/memfuse-py/.venv/ (~350MB, ~5829 files) which is a
complete Python virtual environment with third-party packages
(numpy, pip, setuptools, mcp, httpx, uvicorn, pytest, compiled .so
files, and vendored Fortran/C source) that was accidentally checked
into version control. This is a runtime/dev artifact and must never
be tracked in git.

Legitimate project files under crates/memfuse-py/python/ and
crates/memfuse-py/src/ (PyO3 bindings) are untouched."

echo
echo "Fertig. Nächster Schritt: 03-remove-env-artifacts.sh"
