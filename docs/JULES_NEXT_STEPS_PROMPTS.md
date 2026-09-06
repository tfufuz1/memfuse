# Prompts für Google-Jules (Post-Merge-Verifikation & Nachbereitung)

Dieses Dokument enthält vorgefertigte Prompts für Google-Jules zur Verifikation und Nachbereitung der Migration von `async-trait` auf AFIT und `BoxFuture`.

---

## Prompt 1: Main-Branch Verifikation nach Merge

```markdown
AUFGABE: Post-Merge Verifikation der async-trait Migration auf main.

Führe folgende Befehle aus und bestätige die Resultate:

1. Prp-Check für async-trait im gesamten Repository:
   grep -rl "async-trait" --include=Cargo.toml . | wc -l
   grep -rln "#\[async_trait\]" crates/ | wc -l
   (Beide Ausgaben MÜSSEN 0 sein).

2. Prp-Check für Dependency Tree:
   cargo tree | grep async-trait
   (Ausgabe MUSS leer sein).

3. Workspace-Build und Baseline-Check:
   cargo check --workspace --exclude memfuse-tauri
   cargo xtask check-unwrap-baseline
   (Beide MÜSSEN mit 0 Fehlern durchlaufen).
```

---

## Prompt 2: Dokumentations- und Benchmark-Synchronisation

```markdown
AUFGABE: Re-Sync der Architektur-Dokumente und Un-Wrap Baseline.

Führe aus:
1. cargo xtask update-unwrap-baseline
2. cargo run -p xtask -- sync-docs
3. git status und committe eventuelle Diffs mit Nachricht "docs: sync architecture and baseline post-afit-migration".
```

---

## Prompt 3: Crate-für-Crate Voll-Testlauf (CI Parallelization)

```markdown
AUFGABE: Vollständige Testlauf-Validierung aller Subsystem-Crates.

Führe schrittweise folgende Crate-Tests aus:
1. cargo test -p memfuse-core
2. cargo test -p memfuse-store --lib
3. cargo test -p memfuse-index --lib
4. cargo test -p memfuse-text
5. cargo test -p memfuse-graph
6. cargo test -p memfuse-checkpoint
7. cargo test -p memfuse-agent
8. cargo test -p memfuse-db --lib
9. cargo test -p memfuse-mcp
10. cargo test -p memfuse-ollama
11. cargo test -p memfuse-router

Bestätige, dass alle Crate-Bibliothekstests grün bleiben.
```
