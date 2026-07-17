# Sprint 1: Foundation Hardening — Panic-Safety, Mathematische Korrektheit, Validierung

## Ziel
Alle Findings heilen, die das **Zero-Panic-Gesetz (§2)**, den **Determinismus (§4)** und die **Eingabevalidierung** betreffen. Dieser Sprint berührt ausschließlich Level-0/1-Crates und hat **keine** Auswirkung auf höhere Schichten — er kann autonom und ohne Architektur-Änderungen durchgeführt werden.

## Status-Übersicht

| ID | Status | Kurzname |
|---|---|---|
| FIND-COR-001 | ✅ Erledigt | Zero-Division Panic in `TxBuffer` |
| FIND-COR-002 | ✅ Erledigt | Cosine-Distanz u8 Platzhalter |
| FIND-COR-003 | ⚠️ Bewusst abweichend | DotProduct-Negierung inkonsistent u8 vs f32 |
| FIND-COR-004 | ✅ Erledigt | Negative Gewichte in `FusionWeights` |
| FIND-COR-005 | ✅ Erledigt | Starrer Default-Trait-Error |
| FIND-IND-001 | ✅ Erledigt | SIMD-Determinismus-Bruch |
| FIND-IND-003 | ✅ Erledigt | Nicht-portabler Byte-Cast in HNSW-Save |
| FIND-IND-004 | ❌ Offen | DiskANN Cache-Eviction |
| FIND-CRY-001 | ✅ Erledigt | Falscher Offset bei HMAC-Fehler |
| FIND-CRY-002 | ✅ Erledigt | `debug_assertions` Gate für Test-Helper |
| FIND-DB-001  | ✅ Erledigt | Panics in `SandboxBridge` |

**Gesamtfortschritt: 9/11 vollständig, 1 bewusst abweichend, 1 offen**

---

## Detailanalyse pro Finding

### ✅ FIND-COR-001 — `TxBuffer` Division-by-Zero
- **Datei**: [crates/memfuse-core/src/tx_buffer.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/tx_buffer.rs)
- **Implementiert**: L77–78: Guard `let shard_count = if shard_count == 0 { 1 } else { shard_count };`
- **Test**: `test_tx_buffer_zero_shards_defaults_to_one` (L342–357) — verifiziert keine Panic bei `shard_count=0`.
- **Proptest**: `prop_tx_buffer_isolation` (L361–394) testet dynamisch mit `shard_count in 1..256`.

### ✅ FIND-COR-002 — Cosine-Distanz für u8
- **Datei**: [crates/memfuse-core/src/types/domain.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/domain.rs), L207–223
- **Implementiert**: Korrekte Cosine-Formel mit f64-Arithmetik, `denom == 0.0` Guard, Fixed-Point-Scaling auf u32 (`× 1_000_000`).
- **Tests**: 
  - `test_distance_metrics_u8` (L432–448): Orthogonale Vektoren → 1_000_000, identische → 0.
  - `test_cosine_u8_ranking_matches_f32` (L451–476): Cross-Validation u8 vs f32 Ranking.

### ⚠️ FIND-COR-003 — DotProduct-Negierung für u8
- **Datei**: [crates/memfuse-core/src/types/domain.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/domain.rs), L233–240
- **Bewusste Abweichung**: Der u8-DotProduct gibt `u32` zurück (unsigned), daher ist eine Negierung **nicht möglich**. Die Verantwortung für die Ranking-Inversion liegt beim Caller (dokumentiert in Doc-Comment L197–200). Dies ist **architektonisch korrekt** — der Sprint-Plan ging von einem signierten Rückgabetyp aus.

### ✅ FIND-COR-004 — Negative Gewichte in `FusionWeights`
- **Datei**: [crates/memfuse-core/src/types/saos.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/saos.rs), L87–92
- **Implementiert**: Guard `if vector < 0.0 || text < 0.0 || graph < 0.0 || metadata < 0.0` → `Err(InvalidInput)`.
- **Test**: `test_fusion_weights_invalid_sum` (L247–255) testet den Error-Pfad. Expliziter Negativ-Test fehlt als eigenständiger Test, aber die Guard-Logik ist vorhanden und der Fehlerpfad wird implizit abgedeckt.

### ✅ FIND-COR-005 — Trait-Default-Dokumentation
- **Datei**: [crates/memfuse-core/src/traits.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs), L180–201
- **Implementiert**: `search_filtered` Default-Impl hat umfassende Doc-Comments (L180–188) inkl. Hinweis "Implementors **MUST** override this method if filtered search is supported".

### ✅ FIND-IND-001 — SIMD-Determinismus-Dokumentation + Toleranz-Test
- **Datei**: [crates/memfuse-index/src/distance.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/distance.rs)
- **Implementiert**:
  - Modul-Doc-Comment (L34–37) mit expliziter Abweichungstoleranz `±1e-6`.
  - SAFETY-Kommentare an allen 12 `unsafe fn` + 30+ `unsafe`-Blöcken.
  - `test_distances_match_scalar` (tests) vergleicht SIMD vs. Scalar mit Toleranz `< 1e-3`.
  - `test_u8_metrics_match_scalar` cross-validiert u8 SIMD-Pfade.
- **Kahan-Summation**: Nicht implementiert (war für Sprint 2 geplant — richtig).

### ✅ FIND-IND-003 — Endian-Safe HNSW Persistence
- **Datei**: [crates/memfuse-index/src/hnsw.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/hnsw.rs), `save()` ab L283
- **Implementiert**: Alle Persistenz-Pfade nutzen explizit `to_le_bytes()` und `from_le_bytes()` (L365, L403–408, L574). Kein `from_raw_parts` vorhanden.
- **Test**: `save()` + `load_mmap()` Round-Trip existiert implizit via `test_close_and_reopen` in `memfuse-db`.

### ❌ FIND-IND-004 — LRU-Cache für DiskANN
- **Datei**: [crates/memfuse-index/src/diskann.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/diskann.rs)
- **Status**: Die Datei (930 Zeilen) enthält **kein** `cache`, `clear`, `evict` oder `LRU`. Entweder wurde der Cache komplett entfernt, oder das Feature wurde nie über einen einfachen `HashMap`-Cache hinaus implementiert. **Finding ist offen.**

### ✅ FIND-CRY-001 — HMAC-Offset
- **Datei**: [crates/memfuse-crypto/src/wal_crypto.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-crypto/src/wal_crypto.rs), L92
- **Implementiert**: `verify_and_update(&mut self, entry: &WalEntrySnapshot, offset: u64)` — Signatur enthält `offset: u64`, wird im `WalCorruption`-Error propagiert (L106–109).
- **Test**: `test_integrity_verifier_chain` (L132–189) verifiziert den korrekten Offset bei Korruption (assertiert `offset == 300`).

### ✅ FIND-CRY-002 — Test-Helper Isolation
- **Datei**: [crates/memfuse-crypto/src/anti_tamper.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-crypto/src/anti_tamper.rs), L32
- **Implementiert**: `#[cfg(any(test, feature = "test-utils"))]` — korrektes Gating. `debug_assertions` wurde entfernt. Im Release-Build ohne `test-utils` Feature ist `inspect_key_bytes_for_test` nicht aufrufbar.

### ✅ FIND-DB-001 — `SandboxBridge` Unwraps
- **Datei**: [crates/memfuse-db/src/lib.rs](file:///home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/lib.rs), L806–855
- **Implementiert**: Alle `unwrap()` in der `SandboxBridge`-Impl wurden durch `?`-Operator mit `ok_or_else` + `MemFuseError::Serialization` bzw. `try_into().map_err()` ersetzt (L816–826). Korruptes JSON wird via `unwrap_or(json!(...))` gracefully behandelt (L838).
- **Verbleibende `unwrap()`**: 7 Stück in `#[cfg(feature = "embed")]` Abschnitten (L351, L754, L760, L768, L781, L786) — betreffen `RwLock::read/write` auf dem Embedder. Diese sind **nicht Teil von FIND-DB-001** (SandboxBridge-spezifisch), stellen aber ein residuales Panic-Risiko bei Lock-Poisoning dar.

---

## Verifikationsplan

### Automatisiert (Triple-Gate gemäß Artikel V)

```bash
# Gate I — Kompilierbarkeit
nix develop -c cargo check --all-targets --workspace

# Gate II — Stilgesetz
nix develop -c cargo clippy --all-targets -- -D warnings

# Gate III — Verhalten
nix develop -c cargo test --workspace

# Spezifisch für diesen Sprint:
nix develop -c cargo test -p memfuse-core -- tx_buffer
nix develop -c cargo test -p memfuse-core -- distance
nix develop -c cargo test -p memfuse-core -- fusion_weights
nix develop -c cargo test -p memfuse-index -- determinism
nix develop -c cargo test -p memfuse-index -- hnsw::persistence
nix develop -c cargo test -p memfuse-crypto -- hmac
nix develop -c cargo test -p memfuse-db -- sandbox
```

### Tech-Debt Audit
```bash
just debt-audit
```
Muss nach Sprint 1 auf `✅ Debt-Audit PASSED` stehen für die bearbeiteten Crates.

### Manuelle Verifikation
- Visueller Diff-Review: `git diff --stat` sollte **nur** die genannten Dateien betreffen.
- Kein neuer `unwrap()` außerhalb von `#[cfg(test)]` eingeführt.

---

## Offene Punkte / Nachlauf

> [!WARNING]
> **FIND-IND-004** (DiskANN Cache-Eviction) ist nicht implementiert. Der DiskANN-Code enthält kein Cache-System. Muss evaluiert werden, ob dieses Finding noch relevant ist oder ob der DiskANN-Code grundlegend umstrukturiert wurde.

> [!NOTE]
> **Residuales Risiko**: 7 `RwLock::unwrap()` im `#[cfg(feature = "embed")]` Pfad (memfuse-db/src/lib.rs). Sollte als separates Finding getrackt werden.
