# MemFuse — Audit Intake Verification Protocol (`AUDIT_INTAKE_PROTOCOL.md`)

> **Regel (AGENTS.md §4)**: Jeder Befund ("Finding") aus einem extern zugelieferten Audit-Dokument, Prompt oder Review-Bericht MUSS vor der Implementierung am AKTUELLEN Quellcode verifiziert werden.

---

## 📋 Verifikations-Ablauf (Schritt-für-Schritt)

### Schritt 1: FINDING STRUKTURIERT ERFASSEN
Erfasse das externe Finding mit allen relevanten Metadaten:
- Finding ID (z.B. `AUDIT-2026-09-001`)
- Schweregrad (`CRITICAL`, `HIGH`, `MEDIUM`, `LOW`)
- Betroffene Datei & Zeile (z.B. `crates/memfuse-db/src/collection/relate.rs:123`)

### Schritt 2: MATCHING GEGEN AKTUELLEN CODE (Automatisierte Verifikation)
Führe vor der Implementierung den Verifikations-Befehl aus:
```bash
cargo xtask audit-verify AUDIT-2026-09-001 \
  --file crates/memfuse-db/src/collection/relate.rs \
  --line 123
```

Mögliche Ergebnisse:
- **`VALID`**: Finding ist aktuell und nicht behoben -> Fahre mit Schritt 3 fort.
- **`ALREADY_FIXED`**: Finding wurde bereits behoben -> Markiere in Audit-Review.
- **`SUPERSEDED`**: Datei/Stelle existiert nicht mehr -> Dokumentieren.
- **`FALSE_POSITIVE`**: Finding trifft nicht zu -> Explizit begründen.

### Schritt 3: AI-TAG CREATION + TRACING
Bei Status `VALID`: Erstelle einen nachverfolgbaren `AI-TAG` an der betroffenen Quellcodestelle:
```rust
// AI-TAG[SECURITY][CRITICAL] Race condition in snapshot rollback (Audit Finding)
// ID:       AGT-DB-a3f29c1d
// TS:       2026-08-29T09:14:07Z
// SESSION:  a3f29c1d
// STATUS:   OPEN
// AUDIT_ID: AUDIT-2026-09-001
// BEFUND:   relate() function reads collection state without synchronization
// RISIKO:   Concurrent flush() can write WAL while relate() reads, causing stale state
// EMPFEHLUNG: Acquire RelateGuard before state inspection (ADR-023)
```

### Schritt 4: RESOLUTION TRACKING
Nach erfolgreicher Implementierung und Validierung des Fixes:
```rust
// RESOLVED: AUDIT-2026-09-001 — relate() now uses RelateGuard (TS: 2026-08-29T10:15:00Z)
```

### Schritt 5: MULTI-REVIEWER VALIDATION & LOGGING
Logge den Abschluss des Audit-Reviews über die CLI:
```bash
cargo xtask audit-review AUDIT-2026-09-001 --status pass --note "Fix validated via stress tests"
```

---

## 🚫 Anti-Patterns (Verboten)

- ❌ **Blind-Implementierung**: Codeänderungen vornehmen, ohne `cargo xtask audit-verify` ausgeführt und die Datei geprüft zu haben.
- ❌ **Stilles Ignorieren**: Einen unzutreffenden Finding einfach wegzulassen, ohne im Audit-Review zu erklären, warum er unzutreffend war.
- ❌ **Copy-Paste von Stale-Audit-Prompts**: Veraltete Audit-Texte unbereinigt in neue Aufgaben-Prompts übernehmen.
