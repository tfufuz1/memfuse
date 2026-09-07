# MemFuse — AI-Assistenten-Kontext
## Verifizierter Codestand · HEAD `36ad007a` · Stand 2026-09-07

> **Für AI-Assistenten:** Diese Datei beschreibt was TATSÄCHLICH implementiert ist,
> nicht was die Spec behauptet. Bei Widerspruch zwischen dieser Datei und Spec/README:
> Diese Datei hat Vorrang (Code-Befund > Spezifikation, §0.1 Quellenhierarchie).

---

## Crate-Topologie (verifiziert)

```
Layer 0 — Fundament:      memfuse-core Typen, Traits, Domains, Kryptographie-Basis
                          memfuse-crypto WAL-Crypto (HMAC-Chain ✅), anti_tamper.rs, crypto.rs
                          [memfuse-calibration → FEHLT, muss angelegt werden (ADR-068)]

Layer 1 — Storage-Primitiven: memfuse-store LSM-Storage, MemTable (16-Shard BTreeMap, KEIN SkipList)
                          memfuse-index HNSW (CoW-Rebuild ✅), DiskANN (read-only, persist_delta fehlt)
                          memfuse-text BM25 (IDF-Bug in bm25.rs:95-100, Fix ausstehend)
                          memfuse-embed Embedder, Reranker (Rohsigmoid, NICHT kalibriert)

Layer 2 — Graph:          memfuse-graph community.rs, csr.rs, ppr.rs, session_dag.rs
                          PathRAGEngine: FEHLT (ADR-073)
                          ImmunMemory (F-04): FEHLT

Layer 3 — Orchestrierung: memfuse-router RouterEngine, SlmProfile (KEIN ConfigFingerprint-Feld)
                          memfuse-db Kernoperationen

Layer 4 — Integration:    memfuse-kv-bridge → FEHLT komplett
                          memfuse-candle → FEHLT komplett

Layer 5 — Peripherie:     memfuse-bench 9-Dokument-Synthetik-Korpus (statistisch bedeutungslos)
                          memfuse-tauri Desktop App Shell ✅
                          memfuse-py Verzeichnis existiert, NICHT in Cargo.toml members
```

---

## Was TATSÄCHLICH implementiert ist (verifiziert ✅)

| Komponente | Status | Datei |
|---|---|---|
| WAL v3 HMAC-Chain | ✅ Produktionsreif | `wal.rs:45-80` |
| HNSW 2-Phasen-CoW-Rebuild | ✅ Produktionsreif | `hnsw.rs:1693, 1812` |
| NodesGuard Lock-Reihenfolge | ✅ Typ-erzwungen | `session_dag.rs:29` |
| Label-Propagation deterministische RNG | ✅ LCG fester Seed | `community.rs:55` |
| PPR damping=0.85 | ✅ Korrekt | `ppr.rs:133,343` |
| DiskANN Vamana 2-Pass Build | ✅ Atomares Write | `diskann.rs` |

---

## Was FEHLT und gebaut werden muss

| Komponente | Kritikalität | ADR | Sprint |
|---|---|---|---|
| `TenantId` Typ in memfuse-core | KRITISCH | ADR-066 | G0 |
| `ConfigFingerprint` in memfuse-core | KRITISCH | ADR-067 | G0 |
| `DeletionProof` in memfuse-crypto | KRITISCH | ADR-072 | G0 |
| `memfuse-calibration` Crate | KRITISCH | ADR-068 | G0 |
| BM25 IDF-Fix (bm25.rs:95-100) | SOFORT | ADR-065 | G0 |
| Reranking-Fenster max(k*3,100) | SOFORT | ADR-084 | G0 |
| DiskANN persist_delta() | SOFORT | ADR-069 | G0 |
| PathRAG Engine | Hoch | ADR-073 | H1 |
| F-01 Thermostat | Mittel | ADR-074 | H1 |
| ImportanceClassifier | Hoch | — | H1 |
| memfuse-kv-bridge | Hoch | — | H2 |
| memfuse-candle | Hoch | — | H2 |
| SleepCycle (F-05) | Mittel | ADR-077 | H3 |

---

## Bekannte Bugs (verifiziert)

### BUG-01: BM25 IDF-Implementierung (A19) — SOFORT
**Datei:** `crates/memfuse-text/src/bm25.rs:95-100`
**Problem:** `idf_arg.ln()` mit `1e-6`-Floor statt `ln(1 + (N-df+0.5)/(df+0.5))`.
**Auswirkung:** IDF ≈ 0 bei häufigen Termen. Suchresultate degenerieren.
**Fix:** Einzeiler. Keine API-Änderung.

### BUG-02: Reranking-Kandidatenfenster (A14) — SOFORT
**Datei:** `crates/memfuse-db/src/collection/search.rs:450`
**Problem:** `k * 3` statt `max(k * 3, 100)`. Mit k=10 → 30 Kandidaten → Recall@5 ≈ 0.458.
**Fix:** Einzeiler + Timeout-Wrapper.

### BUG-03: DiskANN read-only (A16) — SOFORT
**Datei:** `crates/memfuse-index/src/diskann.rs:990`
**Problem:** `VectorIndex::insert()` gibt `Err("read-only")`. Kein inkrementeller Pfad.
**Fix:** Pending-Buffer + persist_delta() (ADR-069).

### BUG-04: Reranker unkalibriert (A15) — G0
**Datei:** `crates/memfuse-embed/src/reranker.rs:314`
**Problem:** Rohes Sigmoid ohne Platt-/Isotonische Kalibrierung.
**Fix:** PlattScaler aus memfuse-calibration (nach ADR-068).

---

## Architekturprinzipien (für AI-Assistenten: Pflicht bei jeder Änderung)

**P1:** Kein Import über Layer-Grenzen. `cargo xtask check-dag` vor jedem Commit.
**P2:** `unsafe` NUR in distance.rs (SIMD), diskann.rs + persistence.rs (Mmap). Immer `// SAFETY:`.
**P3:** WAL-Commit VOR MemTable. Kein `let _ = sync_all()`.
**P8:** ConfigFingerprint-Änderung → sofortiger Kalibrierungs-Reset. Kein Warmup-Skip.
**P9:** KV-Cache-Segmente nie unverschlüsselt auf persistentem Speicher. Zeroize-on-Evict.
**P11:** Hot-Path-Operationen haben Timeout. Kein unbeschränktes Warten.
**P12:** Physio-Features per Feature-Flag deaktivierbar. Default: off.

**VETO (nie implementieren):**
- F-02: Partieller HNSW-Rebuild (Recall-Kollaps, Lock-Contention)
- F-10: Cross-Tenant-Wissensaustausch (bricht TenantId-Isolation + DSGVO Art. 17)

---

## Invarianten (bei Verletzung: Bug, kein Design)

| Invariante | Beschreibung |
|---|---|
| **INV-P3-1** | WAL-Commit VOR MemTable-Update |
| **INV-P8-1** | Fingerprint-Änderung → Kalibrierungs-Reset |
| **INV-CAL-1** | calibrated_probability() → None vor Warmup. KEIN 0.5-Fallback |
| **INV-CAL-2** | invalidate_on_config_change() setzt observations auf 0 |
| **INV-PROV-1** | sum(rrf_contributions) ≈ rrf_score (\|Δ\| < 1e-6) |
| **INV-PROV-2** | coherence_bonus immer separates Feld, nie in rrf_score gefaltet |
| **INV-TENANT-1** | TenantId(0) = SYSTEM_RESERVED, try_new(0) → Err |
| **INV-TENANT-2** | scan_prefix() gibt nur Keys dieses Tenants zurück |
| **INV-HNSW-1** | ef_construction >= M, sonst HnswConfig::validate() → Err |
| **INV-DISKANN-1** | persist_delta() behält atomares Rename-Muster |
| **INV-DELETION-1** | DeletionProof::create() nur nach physischer Layer-Bereinigung |

---

## Latenzbudgets (Hot-Path, P11)

| Operation | Budget | Fallback |
|---|---|---|
| Reranker ONNX-Inference | 500ms | RRF-Reihenfolge |
| BM25-Scoring | 50ms | Timeout + Warning |
| HNSW ef_search | 200ms | Abort + Error |
| ImportanceClassifier | 100ms | Heuristik |

---

## Verworfene Features (nicht vorschlagen)

Diese Features werden **nie** implementiert:
- F-02 Partieller HNSW-Rebuild: Recall-Kollaps, Lock-Contention unlösbar
- F-10 Cross-Tenant-Austausch: DSGVO-Compliance-Risiko, technisch unmöglich
- LLM-Importance-Score im Hot-Path: 970ms P50 vs. 58ms mit ImportanceClassifier
- Genetische Algorithmen für Hyperparameter: redundant zu F-07+F-08

---

## Nächste Prioritäten (Stand G0-Sprint)

1. `cargo test --workspace` grün halten
2. BUG-01 (BM25) + BUG-02 (Reranking) + BUG-03 (DiskANN) beheben
3. TenantId + ConfigFingerprint in memfuse-core anlegen
4. memfuse-calibration Crate erstellen
5. CONSTITUTION.md + DECISIONS.md auf aktuellem Stand halten
