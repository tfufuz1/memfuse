# Sprint 1: Foundation Hardening

## Phase 1: Panic-Surface Eliminierung
- [ ] Schritt 1.1 — `memfuse-core`: `TxBuffer` Division-by-Zero (FIND-COR-001)
- [ ] Schritt 1.2 — `memfuse-db`: `SandboxBridge` Unwraps (FIND-DB-001)

## Phase 2: Mathematische Korrektheit
- [ ] Schritt 2.1 — `memfuse-core`: Cosine-Distanz für u8 (FIND-COR-002)
- [ ] Schritt 2.2 — `memfuse-core`: DotProduct-Negierung für u8 (FIND-COR-003)

## Phase 3: Validierung & Guards
- [ ] Schritt 3.1 — `memfuse-core`: Negative Gewichte in FusionWeights (FIND-COR-004)
- [ ] Schritt 3.2 — `memfuse-core`: Trait-Default-Dokumentation (FIND-COR-005)

## Phase 4: SIMD-Determinismus & Portabilität
- [ ] Schritt 4.1 — `memfuse-index`: Determinismus-Dokumentation + Toleranz-Test (FIND-IND-001)
- [ ] Schritt 4.2 — `memfuse-index`: Endian-Safe HNSW Persistence (FIND-IND-003)
- [ ] Schritt 4.3 — `memfuse-index`: LRU-Cache für DiskANN (FIND-IND-004)

## Phase 5: Crypto-Polishing
- [ ] Schritt 5.1 — `memfuse-crypto`: HMAC-Offset (FIND-CRY-001)
- [ ] Schritt 5.2 — `memfuse-crypto`: Test-Helper Isolation (FIND-CRY-002)

## Verifikation
- [ ] Triple-Gate Prüfung (cargo check, clippy, test)
