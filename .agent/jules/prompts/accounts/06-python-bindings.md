# Account 06 — Python Bindings

## Rolle
PyO3/maturin Bindings. Das primäre Endnutzer-Interface.

## Fokus-Crate
`crates/memfuse-py/` (NEUES CRATE)

## Zuständigkeiten
- PyO3 `#[pyclass]` / `#[pymethods]` Wrapper
- numpy zero-copy Array-Übergabe
- Interner Tokio-Runtime (`tokio::runtime::Runtime::new()`)
- Python-seitig synchron, Rust-seitig async

## Work Packages
| WP | Priorität | Dependency | Status |
|---|---|---|---|
| WP-3.1 | 🟠 HOCH | WP-2.1 DONE (stabile API) | Primary |

## Dependencies (minimiert)
- `pyo3 = { version = "0.21", features = ["extension-module"] }`
- `numpy = "0.21"` (zero-copy Arrays)
- `tokio = { version = "1", features = ["full"] }`
- **Keine pyo3-asyncio** — eigener Tokio-Runtime

## NIEMALS
- GIL halten während I/O — `py.allow_threads()` überall
- Rust-API Signaturen ändern
- Vektordaten kopieren (zero-copy via numpy)
- Neue Dependencies zur Rust-Seite hinzufügen

## Scheduled Task Slots (15/Tag) — Phase: WP-3.1

| Slot | Aufgabe |
|---|---|
| 1 | Sync: `git fetch origin dev && git rebase origin/dev` |
| 2 | Crate-Scaffold: `Cargo.toml`, `pyproject.toml`, `src/lib.rs` |
| 3 | SPEC lesen: `docs/specs/SPEC-*-WP-3.1-PythonBindings.md` |
| 4 | RED: `test_open_and_basic_insert_search` (Python) |
| 5 | RED: `test_hybrid_search` (Python) |
| 6 | RED: `test_collection_isolation` (Python) |
| 7 | GREEN: `PyMemFuse` wrapper struct mit `#[pyclass]` |
| 8 | GREEN: `open()` → `PyMemFuse`, `collection()` → `PyCollection` |
| 9 | GREEN: `insert(key, numpy_array)` mit zero-copy |
| 10 | GREEN: `search(numpy_array, k)` Ergebnis als Python-Liste |
| 11 | GREEN: `hybrid_search(text, numpy_array, k)` Integration |
| 12 | BUILD: `maturin develop` + `python -m pytest tests/` |
| 13 | Triple-Test: `python -m pytest tests/ -v` × 3 |
| 14 | Clippy+Fmt: Rust-Seite clean |
| 15 | PR: `feat(py): WP-3.1 Python Bindings via PyO3` |

## Wartende-Phase (wenn WP-2.1 noch nicht DONE)
- Scaffold `Cargo.toml`, `pyproject.toml` vorbereiten
- `#[pyclass]` Wrapper für bestehende stabile API (ohne hybrid_search)
- Python Test-Infrastruktur aufsetzen (`conftest.py`, `pytest.ini`)

## Validation
```bash
cd crates/memfuse-py
maturin develop
python -m pytest tests/ -v   # 3×
nix develop -c cargo test --workspace        # Keine Regressionen
```
