# MemFuse SAOS — 10 Tägliche Agent-Prompts

> **System:** ANCHOR v2 State Machine
> **Regel:** Jeder Prompt wird 1x täglich unverändert ausgeführt.
> **State:** Die `// ANCHOR:` Kommentare im Code sind der dynamische Zustand.

---

## Ausführungsreihenfolge (täglich)

```
06:00  01-scan        Zustandserfassung + Debt-Erkennung
07:00  02-spec        Spezifikationen schreiben
08:00  03-red         Failing Tests schreiben (TDD Red)
09:00  04-green       Tests grün machen (TDD Green)
10:00  05-refactor    Code aufräumen
11:00  06-integrate   Cross-Crate Integration
12:00  07-validate    Triple-Test-Gate + Clippy
14:00  08-perf        Performance-Hotspots
16:00  09-security    Security-Audit
18:00  10-docs        Dokumentation + Release-Readiness
```

---

## Prompt-Dateien

Die 10 Prompts liegen in `.agent/prompts/daily/01-scan.md` bis `10-docs.md`.
Jeder Prompt ist ein vollständiger, eigenständiger System-Prompt.
