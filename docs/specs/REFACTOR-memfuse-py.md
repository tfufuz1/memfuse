# REFACTOR-PLAN: memfuse-py
**Datei:** `docs/specs/REFACTOR-memfuse-py.md`
**Erstellt:** 2026-05-28
**Priorität:** MEDIUM
**Geschätzter Aufwand:** 2 Tage
**Voraussetzung:** memfuse-db

---

## CRATE-ZUSTAND: EHRLICHE BEWERTUNG

| Dimension          | Aktuell       | Ziel          |
|--------------------|---------------|---------------|
| Error-Mapping      | ❌ Pauschal    | GRANULAR      |
| MCP-Integration    | ❌ FEHLT       | SERVER READY  |
| GIL-Handling       | ✅ Korrekt     | OPTIMIZED     |
| Typ-Sicherheit     | ⚠️ JSON-Schwach| STRONG        |

---

## IDENTIFIZIERTE SCHWACHSTELLEN

### HIGH (Pre-Launch — Innerhalb von 1 Woche)

#### FIND-PY-001: Granular Exception Mapping
**Typ:** UX / DX
**Datei:** `crates/memfuse-py/src/lib.rs`
**Zeilen:** 112–114 (`memfuse_err`)
**Problem:** Alle Rust-Fehler werden unterschiedslos als `PyRuntimeError` nach Python geworfen.
**Auswirkung:** Python-Nutzer können nicht sauber mit `try: ... except KeyError:` auf fehlende Dokumente reagieren.

**Refaktorisierungsanweisung:**
```rust
1. Ersetze `memfuse_err` durch ein strukturiertes Mapping:
   - MemFuseError::NotFound -> PyKeyError
   - MemFuseError::InvalidInput -> PyValueError
   - MemFuseError::Crypto -> PyOSError (oder Custom PyMemFuseCryptoError)
   - MemFuseError::IO -> PyOSError
2. Registriere die Custom Exceptions im `#[pymodule]`.
```

---

#### FIND-PY-002: MCP (Model Context Protocol) Server
**Typ:** Feature / Architektur
**Problem:** MemFuse soll als "Agent Memory" fungieren, hat aber keinen nativen MCP-Server-Mode für Tools wie Claude Desktop.

**Refaktorisierungsanweisung:**
```rust
1. Implementiere ein Submodul `mcp`.
2. Füge eine Funktion `start_mcp_server(db_path: &str)` hinzu.
3. Diese Funktion nutzt den `mcp-sdk` (falls vorhanden) oder implementiert das JSON-RPC Protokoll über Stdio.
4. Tools: `insert_memory`, `search_memory`, `list_collections`.
```

---

### MEDIUM (Nächster Sprint)

#### FIND-PY-003: Type Stubs (.pyi)
**Typ:** DX
**Problem:** Ohne `.pyi` Dateien haben Python-IDEs (VSCode/PyCharm) kein Autocomplete für die Rust-Klassen.

**Refaktorisierungsanweisung:**
```
1. Erstelle `crates/memfuse-py/memfuse.pyi`.
2. Dokumentiere alle Klassen (Db, Collection, SearchResult) und Methoden.
3. Integriere die Generierung/Prüfung der Stubs in die Build-Pipeline (maturin).
```

---

## REFAKTORISIERUNGSREIHENFOLGE

```
Schritt 1: Error-Mapping Refactor (PY-001)
Schritt 2: MCP Server Entry Point (PY-002)
Schritt 3: Python Type Stubs (PY-003)
```

## NEUE TESTS

```python
# TEST-1: test_python_exception_mapping
# 1. db.get("nonexistent") -> Darf keinen Fehler werfen (Rückgabe None).
# 2. db.insert(..., dimension_mismatch_vector) -> Muss ValueError werfen.
# 3. db.search(..., invalid_k) -> Muss ValueError werfen.

# TEST-2: test_mcp_handshake (Mock)
# 1. Starte mcp_server via subprocess.
# 2. Sende initialisiere JSON-RPC Request über Stdin.
# 3. Prüfe ob Response gültig ist.
```

## DONE-DEFINITION FÜR DIESES CRATE

- [ ] `PyKeyError` und `PyValueError` werden korrekt aus Rust-Fehlern geworfen.
- [ ] MCP Server reagiert auf Stdin/Stdout.
- [ ] `just triple-test -p memfuse-py` (inkl. Python-Smoke-Tests) grün.
