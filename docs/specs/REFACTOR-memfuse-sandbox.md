# REFACTOR-PLAN: memfuse-sandbox
**Datei:** `docs/specs/REFACTOR-memfuse-sandbox.md`
**Erstellt:** 2026-05-28
**Priorität:** HIGH
**Geschätzter Aufwand:** 3 Tage
**Voraussetzung:** memfuse-core, memfuse-db

---

## CRATE-ZUSTAND: EHRLICHE BEWERTUNG

| Dimension          | Aktuell       | Ziel          |
|--------------------|---------------|---------------|
| Isolation (CPU/Mem)| ✅ Exzellent   | VERIFIED      |
| Host-Interaktion   | ❌ Nur Stubs   | FULLY LINKED  |
| Air-Gap Sicherheit | ⚠️ Mock-Status | HARDENED      |
| Exception-Mapping  | ✅ Gut         | VERIFIED      |

---

## IDENTIFIZIERTE SCHWACHSTELLEN

### BLOCKING (Release-Blocker — SOFORT)

#### FIND-SBX-001: Skeleton Host Functions
**Typ:** Funktionalität
**Datei:** `crates/memfuse-sandbox/src/host_functions.rs`
**Zeilen:** 38–67
**Problem:** `db_search`, `db_insert` und `db_get` sind leere Skelette, die immer `0` zurückgeben. Der Sandbox fehlt der Rückkanal zum MemFuse-Kern.
**Auswirkung:** WASM-Tools können keine Daten lesen oder speichern.
**Sovereign Core Verstoß:** Funktionalität.

**Refaktorisierungsanweisung:**
```rust
1. Definiere ein Interface für den Host-Rückkanal (z.B. über Channels oder einen Trait).
2. Implementiere die Speicher-Bridge:
   - WASM-Memory-Pointer in Host-Strings/Vektoren auflösen.
   - Aufruf von `memfuse_db` Facade.
   - Ergebnisse zurück in den WASM-Heap schreiben.
3. Nutze `wasmtime::Linker::func_wrap2_async` (oder ähnlich), um die async DB-Aufrufe sauber zu integrieren.
```

---

### HIGH (Pre-Launch — Innerhalb von 1 Woche)

#### FIND-SBX-002: Air-Gap Mock Verifier
**Typ:** Sicherheit
**Datei:** `crates/memfuse-sandbox/src/airgap.rs`
**Zeilen:** 89–102
**Problem:** `AirGapVerifier::verify` gibt statisch `Ok(true)` zurück, ohne die Umgebung tatsächlich zu prüfen.
**Auswirkung:** Ein Agent könnte unbemerkt Sockets öffnen (z.B. wenn die Sandbox-Config fehlerhaft ist), und der Verifier würde keine Alarm schlagen.

**Refaktorisierungsanweisung:**
```rust
1. Implementiere reale Checks für Linux:
   - Scanne `/proc/self/fd` auf offene Sockets (AF_INET/AF_INET6).
   - Prüfe `ping -c 1 8.8.8.8` (oder äquivalent via syscall) auf Nichterreichbarkeit.
   - Verifiziere, dass keine DNS-Resolver in `/etc/resolv.conf` aktiv sind (optional/streng).
```

---

## REFAKTORISIERUNGSREIHENFOLGE

```
Schritt 1: Real Host Functions (SBX-001)
Schritt 2: Real Air-Gap Verification (SBX-002)
```

## NEUE TESTS

```rust
// TEST-1: test_sandbox_db_interaction
// 1. Inserte Doc "X" in Host-DB.
// 2. Führe WASM aus, das `db_get("X")` aufruft.
// 3. Verifiziere, dass das WASM-Modul den korrekten Inhalt zurückerhält.

// TEST-2: test_airgap_leak_detection
// 1. Öffne absichtlich einen TCP-Socket im Test-Case.
// 2. Rufe `AirGapVerifier::verify` auf.
// 3. Muss `Err` zurückgeben.
```

## DONE-DEFINITION FÜR DIESES CRATE

- [ ] Host-Funktionen erlauben CRUD auf der DB.
- [ ] Air-Gap Verifier detektiert offene Sockets.
- [ ] `just triple-test -p memfuse-sandbox` 3× grün.
