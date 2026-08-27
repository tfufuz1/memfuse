#!/usr/bin/env bash
# Schritt 6 — Verifikation
set -uo pipefail

if [ ! -d ".git" ]; then
  echo "FEHLER: Kein .git-Verzeichnis hier gefunden. Bitte im Repo-Root ausführen." >&2
  exit 1
fi

echo "=== Verbleibende .py-Dateien außerhalb crates/memfuse-py/ ==="
echo "(sollte nur noch scripts/fix_sstable.py, scripts/fix_db2.py, scripts/fix_db.py,"
echo " scripts/check_unsafe_audit.py zeigen — prüfe, ob diese noch gebraucht werden)"
find . -not -path './.git/*' -name "*.py" 2>/dev/null | grep -v "crates/memfuse-py"
echo

echo "=== Gesamtzahl getrackter Dateien jetzt ==="
git ls-files | wc -l
echo

echo "=== Größe von .git jetzt (Verlaufsdaten bleiben vorerst groß) ==="
du -sh .git
echo

echo "=== Verbleibende problematische Pfade im Index (sollte leer sein) ==="
git ls-files | grep -E "^(\.venv|crates/memfuse-py/\.venv|\.cache|\.local|\.config|\.nix-defexpr|\.nix-profile)" || echo "(keine gefunden — sauber)"
echo

if command -v cargo >/dev/null 2>&1; then
  echo "=== cargo check --workspace ==="
  cargo check --workspace
else
  echo "Hinweis: 'cargo' nicht gefunden — bitte manuell 'cargo check --workspace' ausführen,"
  echo "um sicherzustellen, dass das Projekt nach der Bereinigung weiterhin baut."
fi

echo
echo "############################################"
echo "# Verifikation abgeschlossen."
echo "#"
echo "# WICHTIG: git rm --cached entfernt Dateien nur aus dem aktuellen Commit,"
echo "# nicht aus der Historie. Das .git-Verzeichnis bleibt groß, da alte"
echo "# venv-Blobs weiterhin im Objektspeicher liegen. Eine History-Bereinigung"
echo "# (git filter-repo / BFG) erfordert einen Force-Push und Neu-Klonen aller"
echo "# Mitentwickler — das ist NICHT Teil dieses Skript-Sets und sollte"
echo "# separat und bewusst entschieden werden."
echo "############################################"
