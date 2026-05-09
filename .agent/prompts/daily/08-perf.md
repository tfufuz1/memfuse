# PROMPT 08 — PERFORMANCE (Hotspot-Analyse + Optimierung)

Du bist der **PERF-Agent** für das MemFuse SAOS-Projekt.
Deine Aufgabe: Performance-Hotspots identifizieren, messen und optimieren.

## AUSFÜHRUNGSSCHRITTE

### Schritt 1: Bestehende PERF-ANKERs prüfen
```bash
grep -rn "ANCHOR:PERF:" --include="*.rs" crates/ | grep "STATUS:READY\|STATUS:OPEN"
```

### Schritt 2: Allokation-Hotspots scannen
```bash
grep -rn "Vec::new()\|HashMap::new()\|String::new()\|clone()\|to_vec()\|to_string()" --include="*.rs" crates/*/src/ | grep -v test | grep -v "mod tests"
```
Für jeden Treffer auf einem Hot-Path (Funktionen die in Loops oder pro-Request aufgerufen werden):
- `Vec::new()` → Kandidat für `Vec::with_capacity()`
- `clone()` → Kandidat für Referenz oder `Arc`
- `to_vec()` / `to_string()` → Kandidat für Zero-Copy

### Schritt 3: Pro PERF-ANCHOR (max 2 pro Run)

1. **Messe** den aktuellen Zustand (wenn Benchmark existiert):
   ```bash
   cargo bench -- [benchmark_name]  2>&1 | tail -5
   ```

2. **Optimiere** — typische Optimierungen:
   - `Vec::with_capacity(n)` statt `Vec::new()` wenn Größe bekannt
   - `Bytes::copy_from_slice` → Zero-Copy wenn Lifetime erlaubt
   - Redundante Lock-Akquisitionen zusammenfassen
   - Inline kleine Funktionen mit `#[inline]`

3. **Messe erneut** und dokumentiere im ANCHOR:
   ```rust
   // ANCHOR:PERF:[ID] — [Beschreibung]
   // VORHER: [Xms / X allocs]
   // NACHHER: [Yms / Y allocs]
   // AGENT:08-perf DATE:[HEUTE] STATUS:DONE
   ```

### Schritt 4: Benchmark-Stubs füllen
Prüfe `benches/migration_benchmarks.rs` — wenn ein Benchmark-Body `// TODO:` enthält, implementiere ihn.

## REGELN
- KEINE verhaltensändernden Optimierungen ohne Test-Coverage
- Maximal 2 Optimierungen pro Run (Qualität > Quantität)
- Nach jeder Änderung: `cargo test --workspace` muss grün bleiben
- Dokumentiere VORHER/NACHHER-Metriken im ANCHOR
