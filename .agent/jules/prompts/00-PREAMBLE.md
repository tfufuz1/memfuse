# Jules Prompt Library — Gemeinsame Präambel

Diese Datei enthält die **Standard-Präambel** die jedem Jules Scheduled Task vorangestellt wird.
Account-spezifische Kontexte liegen in `accounts/XX-NAME.md`.

---

## PRÄAMBEL (für alle Tasks kopieren)

```
Repository: dieses Repository (bereits verbunden via Jules Dashboard)
Basis-Branch: dev
Feature-Branch: jules/[ACCOUNT]-[TASK-NAME] (Jules erstellt diesen automatisch)

═══════════════════════════════════════════════════════════════
  DIE 4 KERNSÄULEN DES SYSTEMS - WORKFLOW
═══════════════════════════════════════════════════════════════

1. SPEC-DRIVEN DEVELOPMENT (Das Blackboard-Prinzip)
   - Die `docs/specs/*.md` ist das "schwarze Brett". Niemals Code schreiben, der nicht von Specs abgedeckt ist.
   - Agent liest Invarianten aus, und aktualisiert am Ende den Status in der Spec.

2. SOVEREIGN CORE TDD-LOOP (Red -> Green -> Refactor)
   - RED: Test für Invariante / AC schreiben (muss failen).
   - GREEN: Minimale Implementierung schreiben.
   - TRIPLE-TEST-GATE: Flaky tests verhindern. Führe `just triple-test` (oder 3x nix develop -c cargo test) aus. Muss 3x grün sein.

3. COMMENT-ANCHOR KOMMUNIKATION
   - Globale Audit- und Log-Zentrale im Code: `// ANCHOR:[TYP]:[COMP-ID] — [Grund/Status]`
   - Typen: TODO, FIXME, IMPL, TEST, SAFETY (Audit-Pflicht bei unsafe!).

4. GITHUB-GATES
   - Gate 1: Lokaler Commit (Clippy, Tests, Format).
   - Gate 2: PR Open (Mapping zur Spec in der PR Description).
   - Gate 3: Merge (durch Lead Architect).

═══════════════════════════════════════════════════════════════
  SOVEREIGN CORE DOCTRINE — ABSOLUT VERBINDLICH & ZERO-PANIC
═══════════════════════════════════════════════════════════════

1. ZERO-PANIC POLICY: Produktionscode darf NIEMALS .unwrap() / .expect() enthalten!
   → Nutze den `?` Operator, propagiere Fehler über das `MemFuseError` (oder ähnliche Enum) System.

2. ASYNC ONLY I/O (Asynchrone Integrität): 
   → KEIN blockierendes `std::fs` in async Funktionen. Das wird als schwerer Architekturfehler (Bottleneck) behandelt!
   → Nutze ausschließlich `tokio::fs`.

3. ZERO UNSAFE: #![forbid(unsafe_code)] in jedem Crate.
   → Ausnahme: distance.rs (SIMD) -- muss zwingend ein SAFETY ANCHOR haben.

4. WARNINGS = ERRORS: `cargo clippy -- -D warnings` muss SAUBER sein.

5. DOC PFLICHT: jede pub struct/fn braucht /// Doc-Comment.

6. BACKWARD COMPAT CHECK: Bestehende API-Signaturen dürfen NICHT gebrochen werden, es sei denn in Spec explizit verlangt.

═══════════════════════════════════════════════════════════════
  TECH-DEBT GUARD & START DES WORKFLOWS (WP-0.0)
═══════════════════════════════════════════════════════════════

Tech-Debt Scans (Säuberungs-Gates):
Voraussetzung für alle weiteren WPs ist das bestehen von WP-0.0!

Führe aus: `just debt-audit` (falls vorhanden)

Oder manuell prüfen (Zero-Tolerance für Ausgaben):
grep -rn "\.unwrap()" crates/ --include="*.rs" | grep -v "/tests/"
grep -rn "std::fs::" crates/ --include="*.rs" | grep -v "/tests/"
```
