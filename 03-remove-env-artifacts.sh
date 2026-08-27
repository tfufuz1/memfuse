#!/usr/bin/env bash
# Schritt 3 — Lokale Umgebungs-/Nix-Reste entfernen
# .cache/ .local/ .config/ .nix-defexpr/ und .nix-profile-Symlinks
#
# WICHTIG: Diese Ordnernamen können an MEHREREN Stellen im Repo auftauchen,
# nicht nur im Root (z.B. auch unter crates/memfuse-py/.cache/ als Teil
# der pip-Cache-Struktur, unabhängig von der bereits entfernten .venv/).
# Dieses Skript sucht daher rekursiv im gesamten getrackten Baum.
set -euo pipefail

if [ ! -d ".git" ]; then
  echo "FEHLER: Kein .git-Verzeichnis hier gefunden. Bitte im Repo-Root ausführen." >&2
  exit 1
fi

# Verzeichnisnamen, die überall im Baum als Ganzes entfernt werden sollen
DIR_NAMES=(".cache" ".local" ".config" ".nix-defexpr")
# Symlink-/Datei-Namen, die überall im Baum entfernt werden sollen
LEAF_NAMES=(".nix-profile")

echo "=== Suche getrackte Vorkommen im gesamten Repo (nicht nur Root) ==="
MATCHES=()
for name in "${DIR_NAMES[@]}"; do
  while IFS= read -r line; do
    [ -n "$line" ] && MATCHES+=("$line")
  done < <(git ls-files | grep -E "(^|/)${name//./\\.}/" | sed -E "s#((^|.*/)${name//./\\.})/.*#\1#" | sort -u)
done
for name in "${LEAF_NAMES[@]}"; do
  while IFS= read -r line; do
    [ -n "$line" ] && MATCHES+=("$line")
  done < <(git ls-files | grep -E "(^|/)${name//./\\.}$")
done

# Deduplizieren
if [ "${#MATCHES[@]}" -gt 0 ]; then
  readarray -t MATCHES < <(printf '%s\n' "${MATCHES[@]}" | sort -u)
fi

if [ "${#MATCHES[@]}" -eq 0 ]; then
  echo "Keine getrackten .cache/.local/.config/.nix-defexpr/.nix-profile-Pfade gefunden."
  exit 0
fi

echo "Folgende Pfade werden entfernt (git rm -r --cached + rm -rf):"
for m in "${MATCHES[@]}"; do
  echo "  - $m"
done
echo

read -p "Fortfahren und diese Pfade entfernen? [j/N] " confirm
if [[ ! "$confirm" =~ ^[jJ]$ ]]; then
  echo "Abgebrochen."
  exit 1
fi

for m in "${MATCHES[@]}"; do
  echo "Entferne $m aus dem Index..."
  git rm -r --cached "$m" --quiet 2>/dev/null || git rm --cached "$m" --quiet 2>/dev/null || true
  if [ -e "$m" ] || [ -L "$m" ]; then
    rm -rf "$m"
  fi
done

echo "=== Verbleibende Prüfung: irgendwo noch getrackte Treffer? ==="
git ls-files | grep -E "(^|/)\.(cache|local|config|nix-defexpr)/|(^|/)\.nix-profile$" || echo "(sauber)"

echo "=== Committe die Änderung ==="
git commit -m "chore: remove local nix/cache environment artifacts

Removes .cache/ (pip cache, nix sentry crash logs, vendored
libonnxruntime.a), .local/ (browser binaries: firefox, chromium,
brave, etc.), .config/ (mimeapps.list), .nix-defexpr/ (nix channel
links), and .nix-profile symlinks (point to absolute, machine-specific
paths). Searched and removed across the whole tracked tree, not just
the repo root, since these artifacts appeared in multiple locations
(e.g. also under crates/memfuse-py/). None of these are portable
project files."

echo
echo "Fertig. Nächster Schritt: 04-remove-loose-artifacts.sh"
