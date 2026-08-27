#!/usr/bin/env bash
# Schritt 5 — .gitignore korrigieren und durchsetzen
set -euo pipefail

if [ ! -d ".git" ]; then
  echo "FEHLER: Kein .git-Verzeichnis hier gefunden. Bitte im Repo-Root ausführen." >&2
  exit 1
fi

MARKER="# --- memfuse cleanup: added by 05-harden-gitignore.sh ---"

if [ -f .gitignore ] && grep -qF "$MARKER" .gitignore; then
  echo "Ergänzung bereits vorhanden, überspringe Schreiben."
else
  {
    echo ""
    echo "$MARKER"
    echo "# Lokale Umgebungs-/Cache-Verzeichnisse (nie versionieren)"
    echo ".cache/"
    echo ".local/"
    echo ".config/"
    echo ".nix-defexpr/"
    echo ".nix-profile"
    echo ""
    echo "# Build-/Lint-Logs"
    echo "*.log"
    echo "clippy*.log"
    echo "*_output.txt"
    echo ""
    echo "# Kompilierte Ad-hoc-Binaries ohne Cargo-Build-System"
    echo "/fix_unsafe"
  } >> .gitignore
  echo ".gitignore ergänzt."
fi

git add .gitignore
git commit -m "chore: harden .gitignore against local env/cache/log artifacts"

echo
echo "=== git status nach .gitignore-Härtung (sollte sauber sein) ==="
git status

echo
echo "Fertig. Nächster Schritt: 06-verify.sh"
