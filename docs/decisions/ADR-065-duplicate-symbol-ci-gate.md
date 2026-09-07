# ADR-065: Duplicate Symbol CI-Gate zur Prävention von Merge-Kollisionen

* **Status:** Akzeptiert
* **Datum:** 2026-09-07
* **Kontext / Auslöser:** P0-Build-Blocker nach parallelen Commits (`307df50` und `eb0e3ef`), bei denen zwei unabhängige Branches dieselben Top-Level-Konstanten (`DISKANN_FOOTER_MAGIC`, `DISKANN_INTEGRITY_KEY`) in `crates/memfuse-index/src/diskann.rs` einfügten. Da die Diffs nicht überlappten, erzeugte Git keinen Merge-Konflikt, führte jedoch zu E0428-Kompilierfehlern.

## Entscheidung
Es wird ein leichtgewichtiges, regex-basiertes Pre-Build Gate `cargo run -p xtask -- check-duplicate-symbols` eingeführt und in die CI-Pipeline (`.github/workflows/rust-ci.yml` und `.github/workflows/context-gates.yml`) integriert.

## Funktionsweise
1. **Quelltext-Analyse ohne Compiler-Durchlauf:** Scannt `.rs`-Dateien (aus `git diff` oder Workspace) mit Regex-Mustern auf Zeilenebene nach Top-Level-Deklarationen (`const`, `static`, `struct`, `enum`, `fn`, `trait`, `type`).
2. **Top-Level Isolation:** Berücksichtigt nur Deklarationen ohne führende Einrückung und außerhalb von `impl`- / Struct-Blöcken (`brace_depth == 0`), um False Positives bei gleichnamigen Methoden unterschiedlicher Typen zu vermeiden. Wildcards (`const _`) werden ignoriert.
3. **Feature-Gate Sensitivität:** Unterscheidet Symbole unter abweichenden `#[cfg(...)]`-Attributen.
4. **Fast Pre-Build Gate:** Läuft in CI **vor** `cargo check` / `cargo build`, um Fehler unmittelbar mit präzisen Datei:Zeile-Angaben zu melden.

## Grenzen & Einschränkungen
- Kein Ersatz für `cargo check` / `cargo build`, sondern schnelles Vorab-Gate.
- Makro-generierte Symbole werden nicht expandiert (bewusste Entscheidung gegen Voll-Parsing via `syn` für maximale Ausführungsgeschwindigkeit).
