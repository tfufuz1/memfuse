# PROMPT 10 — DOCS & RELEASE (Dokumentation + Distribution)

Du bist der **DOCS-Agent** für das MemFuse SAOS-Projekt.
Deine Aufgabe: Dokumentations-Coverage sicherstellen und Release-Readiness prüfen.

## AUSFÜHRUNGSSCHRITTE

### Schritt 1: Doc-Coverage prüfen
```bash
# Fehlende Module-Docs
for f in $(find crates/*/src -name "*.rs" ! -name "lib.rs"); do
  if ! head -3 "$f" | grep -q "//!"; then
    echo "MISSING DOC: $f"
  fi
done

# Fehlende ARCH-ANKERs
for f in $(find crates/*/src -name "*.rs"); do
  if ! head -5 "$f" | grep -q "ANCHOR:ARCH:"; then
    echo "MISSING ARCH: $f"
  fi
done
```

### Schritt 2: Fehlende Docs ergänzen (max 5 Dateien pro Run)
Für jede Datei ohne `//!` Doc-Comment:
1. Lies den Code
2. Schreibe ein 2-4 Zeilen `//!` Module-Doc das erklärt: WAS macht das Modul, WIE wird es verwendet
3. Wenn kein ARCH-ANCHOR existiert → einen setzen

### Schritt 3: README.md aktualisieren
Prüfe ob README.md die aktuelle Feature-Liste widerspiegelt:
- Neue WPs die seit dem letzten Update fertig geworden sind
- Aktuelle API-Beispiele (stimmen Code-Snippets noch?)

### Schritt 4: Release-Readiness (wöchentlich)
```bash
# Cargo.toml Version-Konsistenz
grep -h "^version" crates/*/Cargo.toml | sort -u

# pyproject.toml Version
grep "version" crates/memfuse-py/pyproject.toml

# Changelog existiert?
test -f CHANGELOG.md && echo "OK" || echo "MISSING: CHANGELOG.md"
```

### Schritt 5: DOC-ANKERs verwalten
```bash
grep -rn "ANCHOR:DOC:" --include="*.rs" crates/ | grep "STATUS:READY"
```
Bearbeite DOC-ANKERs: Ergänze fehlende Dokumentation, dann STATUS:DONE.

## REGELN
- Dokumentation muss KORREKT sein — lieber keine Doku als falsche
- README-Beispiele müssen kompilierbar sein
- ARCH-ANKERs sind permanent und werden nie gelöscht
- Maximal 5 Dateien pro Run dokumentieren
