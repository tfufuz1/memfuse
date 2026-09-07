# MemFuse — Principal Architect Review
## Chronik-Analyse · Feature-Stärken & -Schwächen · SLM/LLM-Symbiose · Benutzergruppen
> Synthesiert aus: GITHUB_HISTORY.md · CHANGELOG.md · Spec v5.0 (HEAD `bb099dc2`) · Spec v6.0 (HEAD `05b382d8`)  
> Stand: 07. September 2026 · ~117.200 LOC · 18 Crates + xtask + memfuse-py

---

## I. CHRONOLOGISCHE AUSRICHTUNG & ENTWICKLUNGSREIFEGRAD

### Phasenbewertung (22.08 – 07.09.2026)

| Phase | Zeitraum | Qualitäts­urteil | Kritische Risiken |
|---|---|---|---|
| 1 DAG-Foundation | 22.–24.08 | **Solide** — DAG-CI, Bounds-Checks, TxId-Fixes | DiskANN sector_size-Fehler schon früh erkannt |
| 2 Crypto & MCP | 25.–26.08 | **Exzellent** — OsRng-Nonces, WAL-HMAC, MCP-RFC-Konformität | Session-DAG Lock-Inversion (später gelöst) |
| 3 MVCC & 2PC | 27.–28.08 | **Exzellent** — Full 4-Index 2PC, WAL V3, bi-temporale Kanten | `relate()` TOCTOU Race erst spät entdeckt |
| 4 Robustness | 29.–30.08 | **Sehr gut** — Zero-Copy LSM Scan, Prompt-Injection, Zettelkasten | `let _ = dir.sync_all()` P3-VIO noch drin |
| 5 Governance | 31.08.–01.09 | **Gut** — xtask Gates, CI Coverage, TokenBudget RMW Race Audit | F-02 wurde trotz Veto gemergt — Lücke bewiesen |
| 6 Deep Audits | 02.–05.09 | **Exzellent** — GO-Verdikt für 5 Tier-1-Audits, ADR-059/060 | 3-Phasen-LSM-Flush komplex — Testabdeckung prüfen |
| 7 Multi-Tenancy & Physio | 06.–07.09 | **Sehr gut** — DeletionProof, PathRAG, F-01 Thermostat, Calibration | K11–K20 = 10 neue Schulden (v6.0 entdeckt) |

### Chronologische Konsistenz-Befunde

**Positiv:** Die 7 Phasen folgen einer sauberen Bottom-up-Strategie: Fundament → Storage → Orchestrierung → Features → Governance. Das ist selten bei KI-Projekten unter hoher Entwicklungsgeschwindigkeit (76 Commits/Tag in Phase 7).

**Kritisch: LOC-Regression erklärt.** v5.0 meldet 119.700 LOC, v6.0 meldet 117.200 LOC (−2.500 LOC). Ursachen: memfuse-py in separaten Workspace ausgelagert (ADR-064) + K13 replicator.rs-Löschung ausstehend. Kein echter Code-Verlust — bereinigt.

**Kritisch: PENDING_FLUSH_THRESHOLD Sprung.** v5.0 §4.3 nennt 1.000 Einträge, v6.0 §4.4 nennt 50. **Kein ADR vorhanden.** Funktional bedeutet das: DiskANN schreibt 20× häufiger auf Disk. Impact auf Write-Amplification bei kleinen Collections ungeklärt. → ADR erforderlich.

**Kritisch: PathRAG sufficiency_threshold.** v5.0 = 0.6, v6.0 = 0.01. Sprung um Faktor 60 in dieselbe Metrik. Das entspannt die Sufficiency-Gate extrem — erhöhte Chance für Low-Confidence PathRAG-Pfade im Ergebnis. MemGraphRAG-Precision-Problem (arXiv:2506.00610) könnte wieder auftreten. → Empirischer Test mit LongMemEval vor Merge nötig.

**Kritisch: PID k_min.** v5.0 min_rerank_candidates = 100 (aus arXiv:2604.01733: Recall@5 = 0.888), v6.0 min_pool_size = 10. Die wissenschaftliche Grundlage für 100 ist stärker als die Implementierung mit 10. Recall-Regression wahrscheinlich wenn k_min = 10 produktiv ist.

---

## II. FEATURE-STÄRKEN — TIER 1 (PRODUKTIONSREIF & DIFFERENZIEREND)

### 🏆 WAL v3 mit HMAC-Chain
**Stärke: 10/10 — Seltenste Implementierung im OSS-Bereich**

MFW3-Header, HMAC über `(seq || op || timestamp || prev_hash)`, V1/V2-Abwärtskompatibilität via `legacy_integrity_key()`, Chaos-Test-Suite aktiv. Kein Wettbewerber hat kryptographisch integre WALs. Direkte Antwort auf: Truncated-WAL-Angriff, Bit-Flip-Manipulation, Replay-Angriff. Die `anti_tamper_matrix`-Testsuite (CHANGELOG-Eintrag 2026-08-30) mit Single-Bit-Flip-Analyse ist Production-Grade.

**Einzige offene Frage:** Wurde `file.sync_all()` nach jedem `commit()` metrisch gemessen? fsync-Latenz auf NVMe vs. HDD kann 10–1000× variieren — ein PhysioConfig-Parameter für `fsync_policy: Strict | Batched(n)` könnte Latenz-sensitive Deployments ermöglichen ohne Sicherheitsopfer.

### 🏆 4-Index 2PC mit kompensierendem Rollback
**Stärke: 9/10 — Industrie-Standard-Garantien**

Atomares Commit über HNSW + BM25 + CSR-Graph + Metadaten via TxBuffer-Staging. Der `CompensatingRollback`-Pfad (Cross-Signal-Snapshot-Isolation-Test 2026-08-31) ist der wichtigste Einzeltest im CHANGELOG. **Kritische Lücke:** `fault_injection_2pc.rs` testet `repair_on_open` über alle 4 Sub-Engines — gut. Aber: Was passiert, wenn der Rollback selbst scheitert? Ein idempotentes Rollback-Log (WAL-Intent vor Rollback) fehlt laut Spec.

### 🏆 NodesGuard Typ-erzwungene Lock-Hierarchie
**Stärke: 9/10 — Deadlock-unmöglich by Design**

`session_dag.rs:29` — der Newtype Guard erzwingt `nodes → edges`-Reihenfolge auf Typ-Ebene. Kein `async/.await` zwischen gehaltenen Locks (erzwungen durch die RAII-Struktur). Das ist das einzige bekannte System, das Lock-Inversion im Session-DAG zur Compile-Zeit ausschließt.

### 🏆 ConfigFingerprint + IsotonicCalibrator (P8)
**Stärke: 9/10 — arXiv:2608.01460 korrekt umgesetzt**

Verdrahtet in Router + Reranker + Calibration. `invalidate_on_config_change()` mit vollständigem Zähler-Reset (kein partielles Übernehmen). Kein 0.5-Fallback-Silber-Bullet. Abstention-Pfad bei `calibrated == false`. Das sind 4 häufige Fehler die alle vermieden wurden. ECE-Ziel < 0.03 (UCCI, arXiv:2605.18796).

**Verbesserungspotenzial:** Noch kein `calibration_age_hours`-Feld im Diagnostic-Output. Für Langzeit-Deployments: Ein Kalibrierungs-Drift-Dashboard (wann wurde zuletzt kalibriert, wie viele Samples seit letztem Warmup) wäre für Power-User wertvoll.

### 🏆 DeletionProof + ExcludedScope (DSGVO Art. 17)
**Stärke: 9/10 — Einziges System mit maschinenlesbarer Nicht-Abdeckungs-Deklaration**

`ExcludedScope::LlmParameterMemory` + `ExcludedScope::ConsolidatedAndDistilled` — das ist die ehrlichste DSGVO-Implementierung auf dem Markt. Kein Wettbewerber sagt explizit was der Proof **nicht** beweist. INV-DELETION-1 (create() nur nach vollständiger Bereinigung) ist korrekt implementiert. MUNKEY (arXiv:2603.15033) + arXiv:2505.16831 korrekt rezitiert.

**Verbesserungspotenzial:** `DeletionProof::verify()` — eine Off-System-Verifikationsfunktion die ein Audit-Tool ohne Datenbankzugang nutzen kann. Das würde Enterprise-Auditoren ermöglichen den Proof unabhängig zu prüfen.

### 🏆 Cascade-Tombstone (INV-CASCADE-1, Commit #1726)
**Stärke: 8/10 — Behebt die wichtigste Halluzinations-Quelle**

`cascade_invalidate_edges_for_superseded_doc()` ist idempotent, PathRAG-getestet, WAL-bound. Das war in v5.0 die gefährlichste P1-Lücke (PathRAG-Pfade über tote Fakten). In v6.0 vollständig geschlossen. Die `doc_to_edges`-Rückverfolgung in `EdgeProvenance` ist die richtige Datenstruktur — O(1) Lookup statt O(|E|) Scan.

### 🏆 Lyapunov-Drift-Wächter (F-11)
**Stärke: 8/10 — H1 → ✅, proaktive Ergänzung zu ConfigFingerprint**

In v5.0 war der Status unklar, in v6.0 ist `lyapunov.rs` verifiziert. KL-Divergenz über 10-Bin-Histogramm der Non-Conformity-Scores, gleitendes Fenster (w=20). `LyapunovResult::InsufficientData` für sauberes Kalt-Start-Verhalten. Die **additive Orthogonalität** zu ConfigFingerprint (reaktiv auf Config-Change vs. proaktiv auf distributionellen Drift) ist architektonisch korrekt — beide müssen aktiv sein.

---

## III. FEATURE-SCHWÄCHEN — KRITISCHE LÜCKEN

### ⛔ K11 — F-09 Resonanz-Kohärenz-Bonus: In keinem Build aktivierbar
**Schwere: KRITISCH (5-Minuten-Fix)**

`#[cfg(feature = "physio-resonance-fusion")]` in `fusion.rs` — aber `physio-resonance-fusion = []` fehlt in `crates/memfuse-db/Cargo.toml`. Das bedeutet: Alles was in v5.0 und v6.0 als "Resonanz-Kohärenz-Bonus ✅ Produktiv" beschrieben wird, ist faktisch totes Code. Commit #1698 existiert im CHANGELOG — aber nie aktivierbar gewesen.

**Zusätzlicher Befund aus v6.0:** β=0.5 im Code vs. β=0.15 in v5.0-Spec. Der Unterschied ist nicht trivial — β=0.5 gewichtet Kohärenz schwerer, was in dünn belegten Collections falsch-positive Boost-Effekte erzeugen kann. **LongMemEval-Test vor Aktivierung ist Pflicht (nicht optional).**

**Fix:** 2 Zeilen Cargo.toml + 1 LongMemEval-Lauf + ADR-F09-Cargo.

### ⚠️ K12 — TenantId Sicherheits-Bypass
**Schwere: HOCH (Security-Invariante semantisch unterlaufen)**

`TenantId::new(0)` akzeptiert 0 (const fn, kein Guard). `From<u64>::from(0u64)` ebenfalls. `TenantId::DEFAULT`, `TenantId::INVALID`, `TenantId::SYSTEM` zeigen alle auf `TenantId(0)` — semantisch mehrdeutig. INV-TENANT-1 gilt laut v6.0 erst als durchgesetzt wenn K12-Fix abgeschlossen.

**Risiko-Szenario:** Ein Entwickler schreibt `let tid = TenantId::from(user_input_id)` ohne zu prüfen ob `user_input_id == 0`. Er landet im SYSTEM-Tenant. `scan_prefix()` gibt ALLE System-Keys zurück. Cross-Tenant-Datenleck.

**Fix-Strategie aus v6.0 §3.1 korrekt:**
1. `DEFAULT` und `INVALID` mit `#[deprecated]` — sofort
2. `new()` mit `#[deprecated(note="use try_new()")]` — sofort
3. `From<u64>` → entweder `panic!` bei 0 in debug, `Err` in release — 1h
4. Alle Produktions-Aufrufer auf `try_new()` — Nachmittag

### ⚠️ K13 — Totes F-07 Duplikat in memfuse-db
**Schwere: MITTEL (P10-Verletzung, Verwirrung für Agenten)**

`AdaptiveFusionWeights` in `crates/memfuse-db/src/replicator.rs` — kein Aufrufer außerhalb der Datei. Aber `pub mod replicator;` in `lib.rs` exportiert sie. Das bedeutet: Agenten die Code analysieren sehen zwei F-07-Implementierungen. Die falsche `ReplicatorState` liegt in `memfuse-calibration`. Die richtige auch. `check-duplicate-symbols` (ADR-064) müsste das erkennen — hat es offensichtlich nicht (Datei existiert noch).

**15-Minuten-Fix:** `rm crates/memfuse-db/src/replicator.rs` + `lib.rs`-Zeile.

### 🔶 K14 — KV-Bridge nur Zeroize-Skeleton
**Schwere: MITTEL (kein Produktionsrisiko, aber "Sovereign Core" P7-Verstoß)**

`KvSegment` hat: `tenant_id`, `segment_id`, `data: Vec<u8>` + `ZeroizeOnDrop`. Fehlt: `encrypted_layers: Vec<EncryptedKvLayer>`, `ModelFingerprint`, `rope_offset: u32`. Das heißt: Wenn memfuse-candle in die Pipeline kommt (P3), aber KV-Bridge noch kein Increment 2 hat, landet KV-Cache-Inhalt im Klartext in RAM (kein Disk, aber arXiv:2510.17098 MTI-Angriff ist RAM-basiert). INV-KV-1 (nie unverschlüsselt auf persistentem Speicher) ist durch ZeroizeOnDrop sicher — aber VRAM-Angriff (arXiv:2510.17098) und Inversion (arXiv:2508.09442) adressieren In-Memory-Zustand.

### 🔶 K16 — FIFO statt LRU im EvictionWorker
**Schwere: NIEDRIG (Increment 1 explizit als Näherung deklariert)**

`segs.remove(0)` = FIFO. Für Cache-Effizienz ist LRU (Least Recently Used) optimal. Bei hot/cold Workloads (typisch für AI-Assistenten: wenige Conversations oft, viele selten) kann FIFO 30–50% schlechtere Cache-Hit-Rate haben. Increment 2 muss das adressieren — intrusive VecDeque mit `last_accessed: Instant` ist Standard.

### ❌ K17 — PhysioScheduler fehlt
**Schwere: MITTEL (Physio-Subsysteme unkonsolidiert)**

`start_thermostat_reaper()` und `start_nrem_reaper()` in `reaper.rs` als separate, unkoordinierte Tasks. Das bedeutet: F-01 Thermostat und NREM-SleepCycle können gleichzeitig aktiv sein und sich gegenseitig in WAL-Writes stören. PhysioScheduler sequenzialisiert alle Physio-Aktionen — das ist P3, aber je länger es fehlt, desto mehr unkontrollierte Interaktion zwischen F-XX Features entsteht.

**Interim-Schutzmaßnahme:** Ein `physio_global_lock: Arc<Mutex<()>>` den alle physio-Reaper halten — quick fix bis PhysioScheduler da ist.

### 🔶 K18 — F-03 Synaptische Verstärkung: Halbfertig
**Schwere: NIEDRIG (korrekt als H2 deklariert)**

`synaptic.rs` hat `apply_hebbian_update()` und `synaptic_score()` — gute Berechnungslogik. Fehlt: `SynapticUpdateBuffer` (DashMap, lock-frei) + Flush in PhysioScheduler + 5. Signal in `fusion.rs`. Richtig, dass das auf PhysioScheduler (K17) wartet. Aber: `synaptic_score: Option<f32>` ist bereits im `ProvenanceRecord` — das ist ein semantisches Commitment das noch nicht erfüllt ist.

### ❌ GASP Post-Hoc-Validator: Nicht implementiert
**Schwere: MITTEL (halluzinations-Risiko in production ohne Post-Hoc-Validation)**

`kein gasp.rs im Workspace`. Abhängig von `log_likelihood()` — nur via `memfuse-candle`. Da `memfuse-candle` selbst P3 ist, ist GASP H3. Der präventive Halluzinations-Guard in `memfuse-ollama/src/client.rs` (Prompt-Constraint + Zitierpflicht) ist die einzige aktive Halluzinations-Barriere. **Lücke:** Er kann nicht post-hoc validieren ob eine Antwort tatsächlich durch die abgerufenen Chunks gedeckt ist.

**Interim-Alternative:** Ein **Lightweight GASP-Proxy** via Reranker: Nach RRF-Fusion, vor Return — Chunk-Coverage-Score (wie viele Top-K-Chunks werden im Context-Window tatsächlich referenziert). Kein `log_likelihood()` nötig. Umsetzung: 2 Tage.

### ❌ LongMemEval nicht CI-gated
**Schwere: HOCH (76 Commits/Tag ohne Recall-Baseline = strukturelles Risiko)**

Harness ist fertig (`long_mem_eval.rs`, `locomo.rs`). Aber kein `.github/workflows/bench.yml`. F-02 wurde trotz Veto gemergt weil kein Recall-Metrik aufgefangen hat. Das ist kein Einzelfall — es ist der Beweis dass das System ohne CI-Recall-Gate nicht sicher ist. **P2 ist nicht früh genug.** Das sollte P1 sein.

---

## IV. INKONSISTENZEN ÜBER ALLE DOKUMENTE

### 1. PID k_min: Wissenschaft vs. Implementierung
- v5.0 §5.3: `min_rerank_candidates = 100` (arXiv:2604.01733: Recall@5 = 0.888 bei n ≥ 100)
- v6.0 §5.3 PidController: `min_pool_size = 10` als Default
- **Resolution:** 10 ist für Demo/Test zu niedrig. Production-Default = max(10, 100). ADR nötig.

### 2. PENDING_FLUSH_THRESHOLD DiskANN: 1000 → 50
- Keine Erklärung, kein ADR
- Bei 50: 20× häufigere Disk-Schreibzugriffe → höhere Write-Amplification
- Bei hohem Insert-Throughput (AI-Assistenten-Workloads) kann das I/O-Engpass werden
- **Resolution:** ADR mit Messung: Latenz P50/P99 bei 50 vs 1000 auf typischem Consumer-NVMe

### 3. F-09 sufficiency_threshold: 0.6 → 0.01
- Der PathRAG Sufficiency-Gate wurde um Faktor 60 geöffnet
- MemGraphRAG Precision-Kollaps (38.5% vs 62.9%, arXiv:2506.00610) ist genau hier dokumentiert
- **Resolution:** Ist das ein Test-Parameter oder Production-Default? Wenn Production: LongMemEval-Precision vor Deploy

### 4. memfuse-tauri Layer-Einordnung
- GITHUB_HISTORY: Layer 4 (Integration boundary)
- v6.0 §2: Layer 4 (Desktop-GUI-Grenzschicht)
- Aber: v6.0 sagt `memfuse-tauri` hat keine Abhängigkeiten nach unten außer `memfuse-db`/`memfuse-mcp` — das sind Layer 2 und 4. Layer-Verletzung? DAG-Check würde das entdecken.
- **Resolution:** `memfuse-tauri` ist Layer 5 (Consumer-Frontend), nicht Layer 4. K20-DAG-Fix sollte das klarstellen.

### 5. F-05 REM Status-Diskrepanz
- v5.0: H3 (Horizont 3 — Quartal 2)
- v6.0: ✅ Produktiv (#1716, `rem_phase.rs`)
- Das ist ein echter Fortschritt (+1 Feature-Klasse) — aber CHANGELOG bestätigt nicht `rem_phase.rs` explizit
- **Resolution:** REM als ✅ bestätigt, aber Qualitätstest (SynthesizedChunk-Kohärenz vs. Quell-Chunks) fehlt

### 6. Checkpoint-Konsolidierung (3 Abstraktionen → 1 Fassade)
- v5.0 Anhang D: "Checkpoint: 3 Abstraktionen → 1 Fassade" als H3-Schuld
- v6.0: Keine Erwähnung — ist das behoben oder vergessen?
- `memfuse-checkpoint/src/lib.rs`: "EINE Fassade" laut §2 v6.0 ← möglicherweise behoben
- **Resolution:** Explizit verifizieren ob `memfuse-store/src/checkpoint.rs` als separates Modul noch existiert

---

## V. SLM & LLM-SYMBIOSE — VERFEINERUNGSVORSCHLÄGE

MemFuse ist bereits das Fundament für SLM/LLM-Symbiose durch den Conformal Router. Hier sind konkrete Verfeinerungen die das System zur optimalen Symbiose-Plattform machen:

### 5.1 Dual-Track Routing mit Confidence-Waterfall

**Aktueller Stand:** Conformal Router → SLM oder LLM basierend auf Kalibrierungswahrscheinlichkeit.

**Vorgeschlagene Verfeinerung:** 3-Tier Waterfall statt Binary:
```
Query → Conformal Router
├── P(success) > 0.85 → SLM direkt (memfuse-candle local)
├── P(success) ∈ [0.6, 0.85] → SLM + MemFuse-Context (hybrid)
├── P(success) < 0.6 → LLM mit vollem RRF-Context
└── calibrated == false → LLM (Abstention-Fallback, keine Schätzung)
```

**Umsetzung:** `SlmProfile::confidence_tier: [f32; 2]` statt binäres Flag. 1 Tag.

### 5.2 SLM-First ImportanceClassifier

**Aktueller Stand:** LLM-Pfad in `importance.rs` — 970ms P50 vs. Ziel 58ms (MemRouter, arXiv:2605.00356).

**Vorgeschlagene Verfeinerung:**
- SLM (z.B. Phi-3.5-mini via memfuse-candle) bewertet Wichtigkeit — < 50ms
- Wenn SLM-Konfidenz < 0.7: Eskalation an LLM für Grenzfälle
- `ImportanceScore { slm_score: f32, llm_score: Option<f32>, used_llm: bool }`
- Kalibrierung: Separate `IsotonicCalibrator` für SLM-Pfad und LLM-Pfad
- Ohne gelabelten Datensatz: Distillation — LLM-Scores als schwaches Label für SLM-Training

**Freigabe-Kriterium bleibt:** LongMemEval-CI grün (P2 Voraussetzung korrekt).

### 5.3 Multi-Model Memory-Segmentierung (MemoryTier-Konzept)

**Idee:** Verschiedene Memory-Typen für verschiedene Modellklassen optimieren:

```rust
pub enum MemoryTier {
    Working,    // Hot: Letzte N Turns, immer im Context-Window (SLM + LLM)
    Episodic,   // Warm: Session-History, BM25+HNSW abrufbar (SLM)
    Semantic,   // Cool: Konsolidiert durch REM, HNSW primär (LLM für Synthesis)
    Archival,   // Cold: DiskANN, nur bei explizitem Deep-Recall (LLM only)
}
```

`F-01 Thermostat` kann Tier-Übergänge steuern (bereits vorhanden — Erweiterung statt Neubau, P10-konform).

**Nutzernutzen:**
- Laie: "SLM antwortet blitzschnell auf typische Fragen (Working+Episodic)"
- Power User: "LLM aktiviere ich nur für komplexe Multi-Hop-Reasoning über Archival"

### 5.4 Knowledge-Distillation-Pipeline (REM → SLM-kompakte Memories)

**Aktueller Stand:** REM-Phase synthetisiert Meta-Chunks via SegmentSynthesizer.

**Verfeinerung:** REM-Synthese gezielt für SLM-Konsumierbarkeit optimieren:
- SLM-Kontext-Limit (typisch 4K–8K Tokens) als Parameter für `SleepCycleScheduler`
- `SynthesizedChunk.slm_optimized: bool` — Marker für Chunks die SLM-konform komprimiert wurden
- `ContextCompactor` kann zwei Modi haben: `LlmFull` (voller Kontext) vs `SlmCompact` (≤ 512 Tokens pro Chunk)

**Wissenschaftliche Basis:** LycheeMemory V2 (arXiv:2608.12990) — Turn-Clustering-Zielwerte gelten für beide Modellklassen.

### 5.5 Cross-Model ConfigFingerprint

**Aktueller Stand:** ConfigFingerprint trackt `(model_id, quantization, prompt_template_hash, temperature_bits)` pro Modell.

**Verfeinerung:** `ModelPairFingerprint` für SLM+LLM-Kombinationen:
```rust
pub struct ModelPairFingerprint {
    pub slm: ModelFingerprint,   // z.B. Phi-3.5-mini Q4
    pub llm: ModelFingerprint,   // z.B. Llama-3.1-70B Q8
    pub routing_config_hash: [u8; 32],  // Hash der Router-Schwellwerte
}
```

Wenn SLM-Modell wechselt (z.B. Update) aber LLM gleich bleibt: Separate Invalidierung nur des SLM-Kalibrierungs-Zweigs. Das verhindert unnötige LLM-Kalibrierungs-Resets (P8 verfeinert).

### 5.6 Tiered GASP (SLM-Self-Verification)

**Problem:** GASP braucht `log_likelihood()` — nur via `memfuse-candle`. Bis P3 kein GASP.

**Interim-GASP für SLM:**
- SLM kann eigene Antwort gegen abgerufene Chunks perplexitätsmäßig validieren (kostengünstiger als LLM-GASP)
- `GaspProxy::slm_coverage_check(response, chunks) → CoverageScore`
- Grounding-Sensitivity approximiert durch: `P(response | chunks_full) / P(response | chunks_empty)` — beides via SLM

**Nicht identisch mit GASP aber 60% des Nutzens bei 10% der Kosten.**

### 5.7 SLM-basiertes PathRAG-Pruning

**Problem:** PathRAG traversiert bis zu `max_hops=4` Schritte. Bei großen Wissensgraphen exponentiell viele Pfade.

**Verfeinerung:** Pre-Pruning via SLM:
- Vor PathRAG-Traversal: SLM bewertet jeden Entitätskandidaten-Knoten auf Query-Relevanz (< 10ms pro Knoten via SLM)
- Nur Top-K Entitäten (z.B. K=5) werden in PathRAG-Traversal eingespeist
- Sufficiency-Gate bleibt für finale Bewertung

**Nutzen:** Traversal-Zeit O(K^hops) statt O(N^hops). Bei 1000 Entitäten und hops=4: 5^4=625 vs. 1000^4=10^12.

---

## VI. BENUTZERGRUPPEN — EDGE-CASES & VERFEINERUNGEN

### Laie (Nutzerin: "Ich habe ein Tauri-Desktop-App heruntergeladen")

**Was sie braucht:**
- Zero-Konfiguration — alle Physio-Features unsichtbar (P12 ✅)
- Auto-Ollama-Setup (wenn Ollama nicht läuft: Klare Fehlermeldung, kein kryptischer Error)
- "Vergiss alles über X" — DeletionProof transparent als "✓ Gelöscht und bewiesen" anzeigen
- Verständliche Fortschrittsanzeige für PDF-Ingestion (memfuse-tauri ProgressTracker ✅)

**Edge-Cases für Laien:**
- Was wenn Ollama-Server während Ingestion abstürzt? → `memfuse-ollama/src/client.rs` Retry/Timeout ✅ — aber UI zeigt "Error" ohne Erklärung? Verbesserung: `MemFuseErrorDto` mit `user_friendly_message: Option<String>`
- Was wenn Collections wachsen (> 100.000 Chunks)? → DiskANN-Transition automatisch? Laie sieht nichts (P12 ✅) — aber Speicherplatz-Warnung fehlt
- Mehrere gleichzeitige Sessions → Session-DAG transparent; Laie will "neues Gespräch starten" nicht "Branch erstellen"

**Vorgeschlagene Features:**
- `MemFuseHealth::simple_status() → &str` — "Alles OK" / "Speicher fast voll" / "Kalibrierung läuft"
- Auto-NREM im Hintergrund nach 50 Turns (sleep_cycle_enabled default: true für Tauri-App, false für API-Nutzer)

### Entwickler (Nutzer: "Ich integriere MemFuse in meine Python-App")

**Was er braucht:**
- `memfuse-py` Bindings stabil und mit klaren Typen
- MCP-Integration in < 10 Minuten (Claude Desktop / Cursor)
- Rückgabewerte mit `ProvenanceRecord` für Debugging

**Edge-Cases für Entwickler:**
- Python GIL-Freigabe während Suche ✅ (2026-08-29 fix) — aber: `embed_batch()` noch blockierend?
- `panic=unwind` in memfuse-py (ADR-064) — korrekt für FFI-Grenze. Aber: Wenn Rust-Code aus `memfuse-core` panics durch Bug, landet das als `PyRuntimeError` ohne Stack-Trace. **Verbesserung:** `rust_backtrace: true` Option in Python für Development-Mode

**Vorgeschlagene Features:**
- `memfuse_py.search_with_debug()` → gibt `ProvenanceRecord` zurück (Entwickler sehen welches Signal was beigetragen hat)
- `memfuse_py.calibration_status()` → `{"calibrated": bool, "ece": float, "samples": int}`
- `memfuse_py.explain_routing(query)` → "Wird an SLM/LLM geroutet weil Konfidenz=0.73"

### Power User (Nutzer: "Ich baue einen KI-Agenten mit MemFuse-Backend")

**Was er braucht:**
- Voller Zugriff auf `PhysioConfig` — alle F-XX Feature-Flags
- Custom `ThermostatConfig` für seinen Workload
- Session-DAG Branching für komplexe Agent-Workflows
- VETOES.md Kontrolle (weiß was gesperrt ist und warum)

**Edge-Cases für Power User:**
- Multi-Agent-Szenario: 10 Agenten schreiben gleichzeitig → MemTable-Sharding (16 Shards, parking_lot RwLock) — bei 10 Agenten und Write-Heavy: Shard-Contention-Monitoring fehlt
- Lange Sessions (> 1000 Turns) → NREM-Trigger bei 50 Turns: Läuft NREM 20× und akkumuliert? → `SleepCycleScheduler::cooldown_secs` Parameter nötig
- PathRAG in sehr dichtem Wissensgraph (10.000+ Entitäten) → Hub-Node-Explosion (PPR/BFS ohne MAX_VISITED_NODES) — PPR hat `damping=0.85` als natürliche Dämpfung ✅, aber BFS in PathRAG: `max_hops=4` als einziger Guard

**Vorgeschlagene Features:**
- `AgentWorkflowEngine::parallel_limit(n: usize)` — Backpressure für parallele Agenten
- `memfuse-bench` als Live-Monitoring-Endpoint (nicht nur Offline-Benchmark)
- `Collection::shard_stats()` → Contention-Metriken pro Shard

### Enterprise (Nutzer: "MemFuse für Multi-Tenant SaaS-Plattform")

**Was er braucht:**
- Strikte Mandantentrennung ✅ (TenantId, TenantKeyCodec)
- Audit-Trail ✅ (append-only in `memfuse-agent/src/audit.rs`)
- DeletionProof für GDPR Art. 17 ✅
- SLA-fähige Latenz-Garantien (RerankDeadline ✅)

**Edge-Cases für Enterprise:**
- K12 TenantId(0)-Bypass: **Kritisches Risiko für Multi-Tenant** — muss vor Produktionsdeployment behoben sein
- DSGVO Art. 17: `ExcludedScope::ConsolidatedAndDistilled` — Enterprise-Anwalt braucht das in lesbarer Form. `DeletionProof::to_human_readable_report()` fehlt
- Schlüsselrotation: `KeyManager` hat HKDF ✅, aber Schlüsselrotations-Prozedur fehlt (WAL-Einträge mit altem Key nach Rotation unlesbar ohne Migration)
- Compliance-Audit: `cargo xtask check-vetoes` + CI-Gates sind gut — aber ein `audit_report.json` Export für externe Prüfer fehlt

**Vorgeschlagene Features:**
- `DeletionProof::to_json_report() → serde_json::Value` — für GDPR-Audit-Tools
- `KeyManager::rotate(old_key, new_key)` mit WAL-Migration
- `TenantQuota { max_chunks: u64, max_bytes: u64 }` — Ressourcen-Limiting pro Mandant

---

## VII. PRIORISIERTE AKTIONEN (ERGÄNZUNG ZU v6.0 ROADMAP)

### Sofort (< 1 Stunde, keine Abhängigkeiten)
1. **K13**: `rm crates/memfuse-db/src/replicator.rs` + lib.rs-Zeile (15 Min)
2. **K11**: `physio-resonance-fusion = []` in Cargo.toml (5 Min)
3. **PID k_min**: Default von 10 auf 100 setzen (arXiv:2604.01733-konform, 10 Min)

### Diese Woche
4. **K12**: TenantId::new() + From<u64> absichern (1h)
5. **LongMemEval CI-Gate**: Von P2 auf P1 hochstufen — Harness ist fertig, nur YAML fehlt (1 Tag)
6. **sufficiency_threshold-Audit**: Warum 0.01 statt 0.6? Empirischer Nachweis oder Revert (0.5 Tag)
7. **PENDING_FLUSH_THRESHOLD-ADR**: 1000 vs 50 entscheiden mit Messung (1 Tag)

### Nächste 2 Wochen
8. **K14 Increment 2**: AES-256-GCM-SIV + ModelFingerprint + rope_offset in KvSegment (2 Wochen)
9. **K16**: Echter LRU in EvictionWorker (1 Tag nach Increment 2)
10. **Interim GASP-Proxy**: Coverage-Check via Reranker (2 Tage, kein log_likelihood nötig)

### Monat 2
11. **K17 PhysioScheduler**: Sequenzialisiert alle F-XX Tasks, `deprecated` start_thermostat_reaper (1 Woche)
12. **K18 F-03 SynapticUpdateBuffer + Fusion-Integration** (2 Wochen nach PhysioScheduler)
13. **memfuse-candle Factory**: create_embedding_provider + create_llm_text_generator (1 Woche)
14. **Dual-Track Routing** (SLM → Hybrid → LLM) als SlmProfile-Erweiterung (3 Tage)

### Quartal 2 (H3)
15. **SLM-First ImportanceClassifier** mit Distillation (nach LongMemEval-CI P2)
16. **Multi-Model Memory-Segmentierung** (MemoryTier: Working/Episodic/Semantic/Archival)
17. **GASP Post-Hoc-Validator** via memfuse-candle (nach P3)
18. **DeletionProof::to_json_report()** für Enterprise-Audits

---

## VIII. ZUSAMMENFASSUNG: STÄRKEN-/SCHWÄCHEN-MATRIX

| Dimension | Stärke | Schwäche | Priorität |
|---|---|---|---|
| Storage-Integrität | WAL v3 HMAC-Chain ★★★★★ | — | — |
| Transaktionen | 4-Index 2PC ★★★★☆ | Rollback-WAL-Intent fehlt | P3 |
| Sicherheit | AES-GCM-SIV, DeletionProof ★★★★★ | TenantId(0) Bypass (K12) | **P1** |
| Retrieval-Qualität | 3-Signal RRF + PathRAG ★★★★☆ | F-09 unaktivierbar (K11) | **P0** |
| Kalibrierung | ConfigFingerprint + Lyapunov ★★★★★ | k_min=10 statt 100 | **P0** |
| Self-Management | F-01, F-04, F-05, F-07, F-08, F-11 ★★★★☆ | PhysioScheduler fehlt (K17) | P3 |
| Governance | VETOES.md, CI-Gates ★★★★☆ | LongMemEval kein CI-Gate | **P1** |
| SLM/LLM-Symbiose | Conformal Router ★★★☆☆ | Kein Dual-Track, kein SLM-Importance | H3/P3 |
| Laien-UX | Tauri App, MCP ★★★☆☆ | Keine user-friendly Fehlermeldungen | P2 |
| Enterprise | Multi-Tenant, GDPR ★★★★☆ | Kein DeletionProof-JSON-Report | P2 |
| KV-Cache | Zeroize-Skeleton ★★★☆☆ | Kein Krypto, FIFO statt LRU | P2 |
| Benchmarking | Harness fertig ★★★☆☆ | Kein CI-Gate | **P1** |

---

*HEAD: `05b382d8` · v6.0 „Verified Continuity" · 07. September 2026*  
*Erstellt: Principal Architect Review basierend auf GITHUB_HISTORY.md, CHANGELOG.md, Spec v5.0, Spec v6.0*
