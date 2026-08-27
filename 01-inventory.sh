#!/usr/bin/env bash
# Schritt 1 — Bestandsaufnahme (rein lesend, löscht nichts)
# Ausführen im Root des lokal geklonten memfuse-Repos.
set -uo pipefail

echo "############################################"
echo "# memfuse cleanup — Schritt 1: Bestandsaufnahme"
echo "############################################"
echo

if [ ! -d ".git" ]; then
  echo "FEHLER: Kein .git-Verzeichnis hier gefunden. Bitte im Repo-Root ausführen." >&2
  exit 1
fi

echo "=== git status ==="
git status
echo

echo "=== Gesamtzahl getrackter Dateien (git ls-files | wc -l) ==="
git ls-files | wc -l
echo

echo "=== Getrackte Dateien unter crates/memfuse-py/.venv/ ==="
git ls-files | grep -c "crates/memfuse-py/.venv" || echo "0"
echo

echo "=== Getrackte Dateien unter .cache/ .local/ .config/ .nix-defexpr/ ==="
git ls-files | grep -E "^\.(cache|local|config|nix-defexpr)/" || echo "(keine gefunden)"
echo

echo "=== .nix-profile Symlink getrackt? ==="
git ls-files | grep -E "^\.nix-profile$" || echo "(nicht getrackt)"
echo

echo "=== Einmalige Build-/Log-Artefakte im Root ==="
for f in clippy.log clippy_actual.log clippy_audit.log dag_check_output.txt stabilisierung.log fix_unsafe fix_unsafe.rs; do
  if [ -e "$f" ]; then
    ls -la "$f"
  else
    echo "$f: nicht vorhanden"
  fi
done
echo

echo "=== Kompilierte .so-Dateien im legitimen Python-Bindings-Verzeichnis ==="
echo "(crates/memfuse-py/python/ — das ist KEIN venv-Pfad, wird von Schritt 2 nicht angefasst)"
find crates/memfuse-py/python -type f -name "*.so" 2>/dev/null || echo "(keine gefunden)"
echo

echo "=== Inhalt von crates/memfuse-py/python/ (zur manuellen Prüfung) ==="
find crates/memfuse-py/python -type f 2>/dev/null
echo

echo "=== Inhalt von crates/memfuse-py/src/ (zur manuellen Prüfung) ==="
find crates/memfuse-py/src -type f 2>/dev/null
echo

echo "=== Lose Python-Skripte außerhalb crates/memfuse-py/ ==="
find . -not -path './.git/*' -name "*.py" 2>/dev/null | grep -v "crates/memfuse-py" || echo "(keine gefunden)"
echo

echo "=== Aktuelle .gitignore ==="
cat .gitignore 2>/dev/null || echo "(keine .gitignore vorhanden)"
echo

echo "=== Größe des .git-Verzeichnisses (vor Bereinigung) ==="
du -sh .git
echo

echo "############################################"
echo "# Bestandsaufnahme abgeschlossen."
echo "# Prüfe die Ausgabe, bevor du 02-remove-venv.sh ausführst."
echo "############################################"
