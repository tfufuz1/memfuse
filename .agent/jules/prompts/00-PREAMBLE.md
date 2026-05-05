# Jules Prompt Library — Gemeinsame Präambel

Diese Datei enthält die **Standard-Präambel** die jedem Jules Scheduled Task vorangestellt wird,
sowie account-spezifische Kontexte.

---

## PRÄAMBEL (für alle Tasks kopieren)

```
Repository: dieses Repository (bereits verbunden via Jules Dashboard)
Basis-Branch: dev
Feature-Branch: jules/[TASK-NAME] (Jules erstellt diesen automatisch)

═══════════════════════════════════════════════════════════════
  SOVEREIGN CORE DOCTRINE — ABSOLUT VERBINDLICH
═══════════════════════════════════════════════════════════════

1. ZERO PANIC: Jede Funktion die fehlschlagen kann gibt Result<T, MemFuseError>
   → Kein .unwrap(), kein .expect() in Produktionscode
   → Nur ? Operator oder explizites match

2. ZERO UNSAFE: #![forbid(unsafe_code)] in jedem Crate
   → Ausnahme: crates/memfuse-index/src/distance.rs (SIMD, mit Kommentar)

3. ASYNC ONLY I/O: kein blockierendes std::fs in async fn
   → tokio::fs überall

4. WARNINGS = ERRORS: cargo clippy -- -D warnings MUSS sauber sein

5. DOC PFLICHT: jede pub struct/fn braucht /// Doc-Comment

6. SPEC FIRST: Lies docs/specs/SPEC-*-[WP-NAME].md vor der Implementierung

═══════════════════════════════════════════════════════════════
  TRIPLE-TEST-GATE (PFLICHT vor PR-Öffnung)
═══════════════════════════════════════════════════════════════

Führe diese 4 Kommandos aus. ALLE müssen erfolgreich sein:

  cargo fmt --all
  cargo clippy --all-targets --workspace -- -D warnings
  cargo test --workspace   ← Run 1
  cargo test --workspace   ← Run 2
  cargo test --workspace   ← Run 3

Wenn Run 1 oder 2 fehlschlägt: FIX ZUERST, dann neu starten.
Öffne den PR NUR wenn alle 3 Runs grün sind.

═══════════════════════════════════════════════════════════════
  TECH-DEBT GUARD (bei jeder Änderung prüfen)
═══════════════════════════════════════════════════════════════

grep -rn "\.unwrap()" crates/ --include="*.rs" | grep -v "/tests/"
→ Erwartung: KEINE AUSGABE

grep -rn "std::fs::" crates/ --include="*.rs" | grep -v "/tests/"  
→ Erwartung: KEINE AUSGABE außer ggf. einmalige Initialisierungen

═══════════════════════════════════════════════════════════════
```

---

## Account-Kontexte

### Account 01 — memfuse-core
```
Dein Fokus: crates/memfuse-core/
Zuständig für: Typen, Traits (Storage, Index), Error-Typen, Snapshot-Registry, TxBuffer
NIEMALS: LSM, HNSW, oder DB-Facade direkt ändern
```

### Account 02 — memfuse-store
```
Dein Fokus: crates/memfuse-store/
Zuständig für: LsmStorage, MemTable, SSTable, WAL, Compaction
Kritischstes Crate: Datenverlust hier = kompletter Systemausfall
NIEMALS: .unwrap() auf Datei-I/O — immer ? mit aussagekräftigem MemFuseError
```

### Account 03 — memfuse-index
```
Dein Fokus: crates/memfuse-index/
Zuständig für: HnswIndex, Distance-Functions (SIMD), Quantization, DiskANN
unsafe-Budget: NUR in src/distance.rs, MUSS kommentiert sein
```

### Account 04 — memfuse-db
```
Dein Fokus: crates/memfuse-db/
Zuständig für: MemFuse Facade, Collection, Hybrid Search Orchestrierung
Backward-Compat-Guard: ALLE bestehenden 11 Contract-Tests müssen weiterhin grün sein
```

### Account 05 — memfuse-text
```
Dein Fokus: crates/memfuse-text/ (NEUES CRATE)
Zuständig für: BM25, Inverted Index, Tokenizer
Dependency-Limit: Maximal 2 neue externe Dependencies (unicode-segmentation + bincode)
```

### Account 06 — memfuse-py
```
Dein Fokus: crates/memfuse-py/ (NEUES CRATE)
Zuständig für: PyO3 Bindings, maturin Build
Build-Validation: maturin develop && python -m pytest tests/
```

### Account 07 — QA Cross-Crate
```
Dein Fokus: ALLE Crates (read-only Analyse + Fixes)
Aufgabe: Integration Tests, Regressionen finden und fixen
Öffne FIX-PRs: "fix(CRATE): [kurze Beschreibung]"
```

### Account 08 — Docs & Specs
```
Dein Fokus: docs/, README.md, AGENTS.md, crates/*/AGENTS.md
Aufgabe: Sync Dokumentation mit aktuellem Code
NIEMALS: Produktionscode ändern
```

### Account 09 — Benchmarks
```
Dein Fokus: benches/, cargo bench
Aufgabe: Performance-Regressionen melden (Issues), NICHT fixen
Labels: performance-regression, benchmark
```

### Account 10 — Security
```
Dein Fokus: crates/memfuse-store/src/crypto.rs, alle wal/sstable Paths
Zuständig für: AES-256-GCM, HKDF, Nonce-Management
Crypto-Regel: KEIN self-made Crypto — nur auditierte Crates (aes-gcm, hkdf, sha2)
```
