#!/usr/bin/env bash
# Schritt 4 — Einmalige Build-/Debug-Artefakte im Root aufräumen
set -euo pipefail

if [ ! -d ".git" ]; then
  echo "FEHLER: Kein .git-Verzeichnis hier gefunden. Bitte im Repo-Root ausführen." >&2
  exit 1
fi

LOG_FILES=(clippy.log clippy_actual.log clippy_audit.log dag_check_output.txt stabilisierung.log)

echo "=== Folgende Log-/Debug-Dateien werden entfernt ==="
for f in "${LOG_FILES[@]}"; do
  [ -e "$f" ] && echo "  - $f"
done
echo

# fix_unsafe.rs vor der Entscheidung anzeigen
if [ -f "fix_unsafe.rs" ]; then
  echo "=== Inhalt von fix_unsafe.rs (bitte prüfen) ==="
  cat fix_unsafe.rs
  echo
  echo "Ist fix_unsafe.rs ein EINMALIGES Migrations-/Fix-Skript (bereits angewendet)"
  echo "oder ein WIEDERVERWENDBARES Entwickler-Tool?"
  echo "  [1] Einmalig -> Binary + .rs-Datei löschen"
  echo "  [2] Wiederverwendbar -> .rs nach scripts/ verschieben, nur Binary löschen"
  echo "  [3] Überspringen -> nichts an fix_unsafe/fix_unsafe.rs ändern"
  read -p "Auswahl [1/2/3]: " choice
else
  choice="skip"
fi

echo
read -p "Log-Dateien (${LOG_FILES[*]}) jetzt löschen? [j/N] " confirm_logs

# Sammle zu löschende Dateien für git rm
TO_REMOVE=()
if [[ "$confirm_logs" =~ ^[jJ]$ ]]; then
  for f in "${LOG_FILES[@]}"; do
    [ -e "$f" ] && TO_REMOVE+=("$f")
  done
fi

case "$choice" in
  1)
    [ -e "fix_unsafe" ] && TO_REMOVE+=("fix_unsafe")
    [ -e "fix_unsafe.rs" ] && TO_REMOVE+=("fix_unsafe.rs")
    ;;
  2)
    mkdir -p scripts
    if [ -f "fix_unsafe.rs" ]; then
      git mv fix_unsafe.rs scripts/fix_unsafe.rs 2>/dev/null || mv fix_unsafe.rs scripts/fix_unsafe.rs
      echo "fix_unsafe.rs -> scripts/fix_unsafe.rs verschoben."
    fi
    [ -e "fix_unsafe" ] && TO_REMOVE+=("fix_unsafe")
    ;;
  3|skip)
    echo "fix_unsafe / fix_unsafe.rs wird nicht angefasst."
    ;;
  *)
    echo "Ungültige Auswahl, fix_unsafe / fix_unsafe.rs wird nicht angefasst."
    ;;
esac

if [ "${#TO_REMOVE[@]}" -eq 0 ]; then
  echo "Nichts zu entfernen. Committe eventuelle Verschiebung (falls vorhanden)."
  if ! git diff --cached --quiet 2>/dev/null; then
    git commit -m "chore: move reusable fix_unsafe.rs to scripts/"
  fi
  exit 0
fi

for f in "${TO_REMOVE[@]}"; do
  if git ls-files --error-unmatch "$f" >/dev/null 2>&1; then
    git rm --cached "$f" --quiet
  fi
  rm -f "$f"
done

git commit -m "chore: remove one-off debug/log files

Removes clippy lint output logs (clippy*.log), ad-hoc debug output
(dag_check_output.txt, stabilisierung.log), and/or the compiled
fix_unsafe binary and its ad-hoc source, per the interactive choice
made during cleanup."

echo
echo "Fertig. Nächster Schritt: 05-harden-gitignore.sh"
