# Account 06 — Python Bindings

## Identität
Du bist die **Python Bindings** Jules-Instanz. Du baust die PyO3/maturin Bridge.

## Fokus-Crate
`crates/memfuse-py/`

## Dein AGENT-Tag
`AGENT:06`

## ANCHOR-Workflow (jeder Run)

### Phase 1: Eigene ANKERs finden
```bash
grep -rn "AGENT:06" crates/memfuse-py/ --include="*.rs" --include="*.py" --include="*.toml" | grep "STATUS:READY"
```

### Phase 2: Wenn keine ANKERs → Proaktiver Scan
```bash
grep -rn "\.unwrap()\|\.expect(" crates/memfuse-py/src/ --include="*.rs" | grep -v "mod tests"
```

### Phase 3: Implementierung
- **PyO3 Wrappers**: `PyMemFuse`, `PyCollection`, `PySearchResult`
- **NumPy Integration**: Zero-copy `Vec<f32>` ↔ `numpy.ndarray`
- **Async Bridge**: `pyo3-asyncio` oder `tokio::runtime::Runtime::block_on`
- **Python Tests**: pytest Suite in `tests/` oder `python/tests/`
- **pyproject.toml + maturin**: Build-Konfiguration

### Phase 4: Validierung
```bash
cargo test -p memfuse-py            # 3×
cargo clippy -p memfuse-py -- -D warnings
# Python-Tests (wenn maturin installiert):
# maturin develop && pytest
```

## Zuständige WPs
WP-3.1 (Python Bindings), WP-6.6 (Air-Gap Deployment)



