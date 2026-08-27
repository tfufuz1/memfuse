#!/usr/bin/env bash
# Wrapper: Alle memfuse-Cleanup-Skripte nacheinander starten
# Aufruf im Root des geklonten memfuse-Repos:
#   bash run-all-cleanup.sh

set -euo pipefail

if [ ! -d ".git" ]; then
  echo "FEHLER: Kein .git-Verzeichnis hier gefunden." >&2
  echo "Bitte dieses Skript im Root des memfuse-Repos ausführen." >&2
  exit 1
fi

# Suchpfad für die Cleanup-Skripte
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRIPTS=(
  "01-inventory.sh"
  "02-remove-venv.sh"
  "03-remove-env-artifacts.sh"
  "04-remove-loose-artifacts.sh"
  "05-harden-gitignore.sh"
  "06-verify.sh"
)

echo "############################################"
echo "# memfuse Cleanup — Start"
echo "############################################"
echo "Repo Root: $(pwd)"
echo "Script Dir: $SCRIPT_DIR"
echo

for script in "${SCRIPTS[@]}"; do
  script_path="$SCRIPT_DIR/$script"
  if [ ! -f "$script_path" ]; then
    echo "FEHLER: $script nicht gefunden in $SCRIPT_DIR" >&2
    exit 1
  fi
done

echo "Gefundene Skripte:"
for script in "${SCRIPTS[@]}"; do
  echo "  ✓ $script"
done
echo

read -p "Alle Skripte starten? [j/N] " confirm
if [[ ! "$confirm" =~ ^[jJ]$ ]]; then
  echo "Abgebrochen."
  exit 0
fi

for script in "${SCRIPTS[@]}"; do
  script_path="$SCRIPT_DIR/$script"
  echo
  echo "############################################"
  echo "# Starte: $script"
  echo "############################################"
  bash "$script_path"
  echo
  read -p "Weiter zum nächsten Schritt? [j/N] " continue_confirm
  if [[ ! "$continue_confirm" =~ ^[jJ]$ ]]; then
    echo "Abgebrochen bei $script. Bisherige Commits bleiben bestehen."
    exit 0
  fi
done

echo
echo "############################################"
echo "# Alle Schritte abgeschlossen!"
echo "############################################"
echo
echo "git log:"
git log --oneline -6
