# Strategic .unwrap() & .expect() Reduction Plan

## Executive Summary & Ausgangsbefund

Die Bestandsaufnahme an `.unwrap()` / `.expect()` Vorkommen in Produktionscode verzeichnet aktuell **4.486 Baseline-Einträge** (Stand Commit `d903747e`). Historische Messungen zeigen ein stetiges Wachstum der Baseline:

| Zeitpunkt | Anzahl Baseline-Einträge |
|---|---|
| Vor ~2 Tagen | 4.324 |
| Vor ~10 Stunden | 4.377 |
| Aktuell (`d903747e`) | 4.486 |

Das bisherige Gate `check-unwrap-baseline` blockiert zwar *unautorisierte neue Unwraps*, verhindert jedoch nicht das schrittweise Aufblähen der Baseline durch `cargo xtask update-unwrap-baseline`.

Dieses Dokument etabliert eine verbindliche **Zero-Panic-Reduktionsstrategie** mit messbaren Abbauzielen, risikobasierten Prioritätsstufen und automated Governance über das neue CI-Gate `check-unwrap-baseline-trend`.

---

## 1. Governance & CI Integration

Zur Überwachung der Baseline-Entwicklung über Zeit wurden zwei zentrale Mechanismen implementiert:

1. **Automatisierte Trend-Analyse (`xtask check-unwrap-baseline-trend`)**:
   - Vergleicht bei jedem CI-Lauf den Stand von `.unwrap-baseline.json` des PR-Branches mit dem Base-Branch (`MEMFUSE_CI_BASE_REF` / `origin/main`).
   - Gruppiert Nettoveränderungen nach Crates.
   - Gibt bei **Nettowachstum in Tier-1-Crates** eine explizite Warnung aus (nicht-blockierend, um Entwicklungsflüsse nicht abrupt zu unterbrechen).

2. **Kontinuierliche Historisierung (`docs/unwrap_baseline_history.jsonl`)**:
   - Bei jedem Ausführen von `cargo xtask check-unwrap-baseline-trend` wird ein unveränderlicher Eintrag an `docs/unwrap_baseline_history.jsonl` angehängt.
   - Format: `{"date": "YYYY-MM-DD", "commit": "<hash>", "total": N, "by_tier1_crate": {"memfuse-core": N1, ...}}`

---

## 2. Risikobasierte Crate-Priorisierung (Tiering)

Panic-Risiken sind im Workspace ungleich verteilt. Ein Panic in einer zentralen Datenstruktur oder FFI-Grenze hat gravierende Systemauswirkungen (Lock-Poisoning, Prozess-Absturz), während ein Unhandled Unwrap im CLI/Tooling lediglich die lokale Session abbricht.

### Tier 1: Systemkritische Kerne (Höchste Priorität)
*Crates:* `memfuse-core`, `memfuse-crypto`, `memfuse-store`, `memfuse-index`, `memfuse-db`, `memfuse-py`, `memfuse-mcp`

- **Risiko:**
  - `memfuse-core`, `memfuse-store`, `memfuse-index`: Lock-Poisoning-Kaskaden in Multi-Threaded Execution, Dateninkonsistenz in MVCC/LSM structures.
  - `memfuse-crypto`: Sicherheitskritscher Code, Timing-Lecks, unkontrollierte Panics bei Schlüsselerzeugung/Integritätsprüfung.
  - `memfuse-py`: FFI-Grenze. Panics an PyO3-Grenzen führen zum abrupten Prozessabsturz (SIGABRT) des Python-Interpreters.
  - `memfuse-mcp`: Stdio JSON-RPC Server. Panics unterbrechen die MCP-Protokollverbindung.
- **Zielvorgabe:** Priorisierter Abbau ab Tag 1.

### Tier 2: Logik- & Integrationskomponenten (Mittlere Priorität)
*Crates:* `memfuse-graph`, `memfuse-text`, `memfuse-checkpoint`, `memfuse-agent`, `memfuse-tauri`

- **Risiko:** Vorübergehender Ausfall von Subsystemen, Desktop-UI-Resets, abgebrochene Agenten-Schritte.
- **Zielvorgabe:** Abbau nach Stabilisierung der Tier-1-Baseline.

### Tier 3: Hilfs- & Peripherie-Crates (Geringe Priorität)
*Crates:* `memfuse-embed`, `memfuse-ollama`, `memfuse-router`, `memfuse-calibration`, `memfuse-candle`, `memfuse-kv-bridge`, `xtask`, `benchmarks`

- **Risiko:** Lokale Batch- und Benchmark-Fehler, Tooling-Ausfälle.

---

## 3. Messbare Abbauziele (KPIs)

Um die bisherige Trendumkehr von *Wachstum* zu *Nettoreduktion* verbindlich umzusetzen, gelten folgende quantitative Ziele:

- **Wöchentliche Nettoreduktion Tier-1:** mindestens **−20 Einträge / Woche** in Tier-1-Crates (beginnend mit `memfuse-core` und `memfuse-crypto`).
- **Quartalsziel (Q4):** Reduktion der Gesamtzahl von 4.486 auf **< 4.000 Einträge** (entspricht ~10% Reduktion).
- **Hard Limit an FFI-/Sicherheits-Grenzen:** **0 Unwraps** in `memfuse-py` (Python FFI) und `memfuse-crypto` außerhalb dedizierter `#[cfg(test)]`-Blöcke.

---

## 4. Technische Refactoring-Muster (How-To)

Beim Abbau von `.unwrap()` / `.expect()` müssen folgende Zielmuster angewendet werden:

### Muster 1: Fehlerfortpflanzung mittels `?` und `MemFuseError`
```rust
// Vorher (Gefährlich)
let value = map.get("key").unwrap();

// Nachher (Sicher)
let value = map.get("key").ok_or_else(|| MemFuseError::InvalidInput("Key 'key' not found".into()))?;
```

### Muster 2: Sichere Mutex/RwLock-Kapselung (Lock-Poisoning-Vermeidung)
```rust
// Vorher (Gefährlich bei Panic eines anderen Threads)
let guard = self.lock.lock().unwrap();

// Nachher (Sicher)
let guard = self.lock.lock().map_err(|_| MemFuseError::InternalError("Lock poisoned".into()))?;
```

### Muster 3: `OnceLock` & Initialisierung ohne `.expect()`
```rust
// Vorher
static RE: OnceLock<Regex> = OnceLock::new();
let re = RE.get_or_init(|| Regex::new("pattern").unwrap());

// Nachher
static RE: OnceLock<Result<Regex, MemFuseError>> = OnceLock::new();
let re = RE.get_or_init(|| Regex::new("pattern").map_err(Into::into)).as_ref()?;
```

---

## 5. Status & Verweise

- **CI Trend-Gate:** `cargo run -p xtask -- check-unwrap-baseline-trend`
- **Historien-Protokoll:** `docs/unwrap_baseline_history.jsonl`
- **Crate-Einstufungen:** `.jules/prompter-tiers.toml`
