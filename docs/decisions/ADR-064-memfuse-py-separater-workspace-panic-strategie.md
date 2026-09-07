# ADR-064: memfuse-py als separater Cargo-Workspace (Panic-Strategie-Isolation)

* **Datum**: 2026-09-07
* **Status**: ✅ Angenommen (bereits implementiert, dieser ADR dokumentiert nachträglich eine bestehende, korrekte Entscheidung — siehe P6-Nachpflegepflicht).

## Kontext
Der Haupt-Workspace von MemFuse setzt im Release-Profil `panic = "abort"` (Begründung: Performance-Optimierung, binäre Minimalität und deterministischer Abbruch im Server-/DB-Engine-Betrieb).
`memfuse-py` exponiert PyO3-Bindings, die an der FFI-Grenze zu CPython `catch_unwind()` nutzen müssen, um Rust-Panics als Python-Exceptions abzubilden statt den gesamten Python-Interpreter per `SIGABRT` abstürzen zu lassen (siehe Kommentar in `crates/memfuse-py/src/lib.rs`, Zeilen 219–222).
Die Panic-Strategie ist in Cargo eine Workspace-weite Einstellung — sie kann nicht pro Crate innerhalb desselben Workspace überschrieben werden.

## Entscheidung
`crates/memfuse-py/Cargo.toml` definiert ein eigenständiges `[workspace]`-Manifest und wird dadurch bewusst NICHT Mitglied des Haupt-Workspace. Dies ist kein Versehen und keine technische Schuld.

## Konsequenzen
- `cargo build --workspace` im Wurzelverzeichnis baut `memfuse-py` NICHT mit. Dies ist beabsichtigt.
- CI deckt `memfuse-py` über separate `--manifest-path`-Aufrufe ab (`cargo clippy --manifest-path crates/memfuse-py/Cargo.toml`, `cargo test --manifest-path crates/memfuse-py/Cargo.toml`, `maturin build --manifest-path crates/memfuse-py/Cargo.toml`).
- Zukünftige Bearbeiter dürfen `memfuse-py` NICHT in die `members`-Liste der Root-`Cargo.toml` aufnehmen, ohne diesen ADR explizit zu widerrufen (P6).

## Alternativen (verworfen)
- Cargo-Profil-Override pro Crate: nicht möglich, Panic-Strategie ist workspace-weit in Cargo, nicht crate-weit überschreibbar.
- `#[panic_handler]`-Custom-Handler statt Workspace-Trennung: löst das `catch_unwind()`-Problem an der FFI-Grenze nicht, da die Panic-Strategie bereits zur Kompilierzeit workspace-weit fixiert wird.
