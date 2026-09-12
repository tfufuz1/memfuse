# MemFuse Calibration Audit Report (`memfuse-calibration`)

**Stand:** 2026-09-12  
**HEAD:** `8e9e70ae574226d455e9c44731f3ad9b0a75db58`, 2026-09-12 20:00:53 +0200  
**Crate:** `memfuse-calibration` (Layer 3 — Kalibrierung, Uncertainty & Latenz-Regelung)  
**Auditor Persona:** Senior Rust Performance & Systems Engineer (MemFuse)  

---

## 1. INVENTAR-REALITÄTSABGLEICH & COMPONENT STATUS

### Dateistruktur & Umfang
`memfuse-calibration` umfasst insgesamt **1.773 Zeilen** über 4 Quelldateien unter `src/` und 2 Integrationstest-Dateien unter `tests/`:

| Datei | Klasse | LOC | Zweck & Status |
| :--- | :---: | :---: | :--- |
| `src/lib.rs` | S | 14 | Crate-Root, Re-Exports, `#![forbid(unsafe_code)]` Enforcement. |
| `src/isotonic.rs` | L | 555 | `IsotonicCalibrator`: Nicht-parametrische Wahrscheinlichkeits-Kalibrierung via PAVA $O(n)$ amortisiert & ECE. |
| `src/platt.rs` | M | 228 | `PlattScaler`: Parametrische Logit/Score-Kalibrierung (`sigmoid(A * logit + B)`). |
| `src/pid.rs` | L | 451 | `PidController`: Adaptive Candidate Pool-Sizing (F-08 & P11) mit $k_{min} \ge 50$ Floor. |
| `tests/calibration_deep_tests.rs` | T | 290 | Deep Integration- & Property-Tests (Proptest). |
| `tests/isotonic_mutation_hardening_test.rs` | T | 235 | Mutation Hardening & Edge-Case Tests (PAVA / ECE). |

---

## 2. GEKLÄRTER PRODUKTIONSSTATUS: `platt.rs` UND `replicator.rs`

### 2.1 Status von `platt.rs` (`PlattScaler`) — **PRODUKTIV IN VERWENDUNG**
Ein workspace-weiter Scan (`grep -rn "PlattScaler"`) widerlegt die vorherige Vermutung, `PlattScaler` habe keine produktiven Aufrufer. `PlattScaler` ist in `crates/memfuse-embed/src/reranker.rs` (`CrossEncoderReranker`) vollständig im produktiven Reranking-Pfad integriert:

- **Alias & Type Definition (`reranker.rs:21`):** `pub use memfuse_calibration::PlattScaler as PlattScaledSigmoid;`
- **Model Storage (`reranker.rs:433`):** `CrossEncoderReranker` speichert das kalibrierte Modell im Thread-Sicherheits-Lock `fitted_calibration: parking_lot::RwLock<PlattScaler>`.
- **Online Fitting (`reranker.rs:469`):** In `record_implicit_feedback()` wird aus kontinuierlich gesammelten `(logit, is_relevant)` Beobachtungen nach der Warmup-Phase (`calibration_warmup = 50`) das Platt-Modell via `PlattScaler::fit(&buf)` neu angepasst.
- **Logit-Transformation (`reranker.rs:159`, `1017`):** `PlattScaler::transform(logit)` rekalibriert rohe Cross-Encoder Logit-Scores in echte Konfidenzwahrscheinlichkeiten $[0.0, 1.0]$.
- **P8 Reset (`reranker.rs:481`):** `reset_calibration()` setzt das Modell auf `PlattScaler::identity()` zurück.

**Call-Site Klassifikation für `PlattScaler`:**
1. **Produktiv (15 Aufrufe):** `crates/memfuse-embed/src/reranker.rs:17, 21, 47, 63, 73, 159, 433, 453, 469, 481, 487, 516, 947, 1017, 1052, 1059`.
2. **Tests & Modultests (8 Aufrufe):** `crates/memfuse-calibration/tests/calibration_deep_tests.rs:6, 149, 162, 170, 175, 180, 186, 275`.

---

### 2.2 Status von `replicator.rs` (`ReplicatorState`) — **OBSOLET / IN `memfuse-calibration` GELÖSCHT**
Ein Scan nach `replicator.rs` im Crate `memfuse-calibration` zeigt:
- Die Datei `crates/memfuse-calibration/src/replicator.rs` **existiert nicht mehr**.
- In `Cargo.toml` existiert zwar noch das leere Feature-Flag `replicator-dynamics-weights = []`, das Modul selbst wurde jedoch aus `memfuse-calibration` entfernt.
- **Folgefehler in `memfuse-db`:** In `crates/memfuse-db` existieren veraltete Import-Versuche (`use memfuse_calibration::ReplicatorState;` in `query_builder.rs`, `maintenance_scheduler.rs`, `cross_domain_chaos_matrix_test.rs`), die bei Compilierung von `memfuse-db` mit `E0425: cannot find type ReplicatorState in crate memfuse_calibration` fehlschlagen.

---

## 3. PID-CONTROLLER WIRKSAMKEITSNACHWEIS (P11 LATENZBUDGET & APM-2)

### 3.1 Vollständige Datenfluss-Nachverfolgung (Soll vs. Ist)

Der `PidController` in `pid.rs` berechnet die adaptive Pool-Größe $k_{pool}$ zur Einhaltung des Latenzbudgets `target_latency_ms = 150.0` unter Beachtung des unumstößlichen Qualitätsknees $k_{min} \ge 50$ (T2-RAGBench, arXiv:2604.01733).

Die Nachverfolgung der Aufrufskette vom Messpunkt bis zur Suchanfrage ergibt folgendes Bild:

1. **Latenzmessung (`crates/memfuse-db/src/collection/query_builder.rs:453`):**
   Vor dem Reranking wird die Zeit gestartet: `let start_time = std::time::Instant::now();`. Nach Abschluss misst `let _elapsed = start_time.elapsed();` die Latenz.
2. **Aktualisierung des PID-Zustands (`query_builder.rs:472-476`):**
   ```rust
   #[cfg(feature = "adaptive-candidate-pool-sizing")]
   if let Some(ref pid) = self.pid_controller {
       pid.lock().update(_current_pool, _elapsed.as_millis() as f32);
   }
   ```
   `PidController::update` berechnet die Reglerabweichung, wendet Anti-Windup $[-100.0, 100.0]$ an, erzwingt den Hard Floor $k_{min} \ge 50$ und speichert das Ergebnis in `pid.current_pool_size = Some(clamped)`.

3. **Gefundenes Fehlendes Glied (MISSING LINK in `query_builder.rs:392-426`):**
   Vor der Ausführung der Kandidatensuche (`hybrid_search_with_query_at` in Zeile 434) wird das Query-Objekt `HybridQuery` wie folgt aufgebaut:
   ```rust
   let hybrid_query = memfuse_core::HybridQuery {
       ...
       rerank_pool_multiplier: self.rerank_pool_multiplier,
       rerank_pool_max: self.rerank_pool_max, // <-- PID WIRD IGNORIERT!
       has_reranker: _has_reranker,
       k,
   };
   ```
   `execute()` befragt `self.pid_controller` vor dem Retrieval **an keiner Stelle** (`pid.lock().current_pool_size()` wird nicht aufgerufen).
4. **Auswirkung im Search Engine Pfad (`crates/memfuse-db/src/collection/search.rs:906`):**
   In `search.rs` wird `query.rerank_pool_max.unwrap_or(DEFAULT_RERANK_POOL_MAX)` verwendet. Wenn `self.rerank_pool_max` nicht gesetzt ist, fällt die Maximalgrenze des Pools starr auf `200`.

### 3.2 Befund & APM-2 Evaluierung
- **Klassifikation:** **APM-2 (Unwirksame Regelung / Observability-Theater)**
- **Kritikalität:** **MAJOR / P11-Verletzung**. Der PID-Controller misst zwar Latenzen und berechnet korrekte, geregelte Pool-Größen, aber die Rückkopplungsschleife auf die Suchanfrage ist unterbrochen. Bei massiven Latenzüberschreitungen (z.B. 800ms) drosselt der Controller intern zwar auf $k=50$, die Vektorsuche ruft jedoch weiterhin unbeeindruckt 200 Kandidaten ab.

---

## 4. VOLLSTÄNDIGER 6-PUNKTE-PRÜFKATALOG

### Punkt 1: Liniengröße & Crate-Struktur
- `memfuse-calibration` hat 1.773 Zeilen (Quellcode + Tests) und ist damit sehr kompakt.
- Klare Modularisierung in `isotonic.rs`, `platt.rs` und `pid.rs`.

### Punkt 2: Zero-Panic & Unsafe-Doktrin
- `#![forbid(unsafe_code)]` ist im Root `src/lib.rs` verankert (0 `unsafe` Blöcke).
- Zero unhandled `.unwrap()` / `.expect()` im Produktionscode unter `src/`.
- Sämtliche Divisionen und Gleitkommaoperationen sind gegen Division durch Null und Non-Finite Werte (`NaN`, `Infinity`) geschützt.

### Punkt 3: Kalibrierungs-Integrität & P8 Compliance (`ConfigFingerprint`)
- `IsotonicCalibrator::invalidate_on_config_change()` setzt `observations` sofort auf 0 zurück (`INV-CAL-2`), damit veraltete Beobachtungen nach Modellwechsel gelöscht werden.
- `PlattScaler::invalidate_on_config_change()` setzt Parameter auf den unkalibrierten Default ($A=1.0, B=0.0$) zurück (P8 Compliance).
- `GaspValidator` (in `memfuse-candle`) nutzt `IsotonicCalibrator` und löst bei Fingerprint-Drift die Invalidation verlässlich aus.

### Punkt 4: PID-Controller & P11-Latenzbudget Enforcement
- Mathematische Korrektheit von Proportional-, Integral- und Differential-Term verifiziert.
- Anti-Windup Clamping $[-100.0, 100.0]$ verhindert Oszillation und Overshoot.
- Hard Floor $k_{min} \ge 50$ (Qualitätsknee per T2-RAGBench arXiv:2604.01733) ist unumstößlich in `PidController::new` erzwungen.
- Non-finite Latenzwerte (`f32::NAN`, `f32::INFINITY`) lassen den Reglerzustand unverändert.

### Punkt 5: Algorithmen-Korrektheit (PAVA, Platt, ECE)
- **PAVA (`isotonic.rs`):** Pre-Aggregation zusammenfallender Raw-Scores löst Sortierungs-Instabilitäten und macht PAVA strikt deterministisch. Monotonie ist property-test-bestätigt.
- **Platt Scaling (`platt.rs`):** Logistic Sigmoid mit Target Smoothing (Platt, 1999) und L2-Regularisierung schützt vor Overfitting auf trennbaren Stichproben.
- **ECE (`isotonic.rs`):** Expected Calibration Error über 10 Bins korrekt implementiert; Bounding-Checks verhindert Out-of-Bounds Indexing.

### Punkt 6: Thread-Safety & Memory Bounds
- Ringpuffer & Bounded Collections: `IsotonicCalibrator` beschränkt Beobachtungen strikt auf `max_observations` (Default 2000).
- Zero unbounded memory allocations.

---

## 5. QUALITY GATES & TEST-VERIFIKATION

Das Crate wurde mit folgenden Commands erfolgreich verifiziert:

1. **Unit- & Integrationstests (`cargo test -p memfuse-calibration --all-features -- --include-ignored`):**
   ```text
   test result: ok. 38 passed (src/lib.rs)
   test result: ok. 16 passed (tests/calibration_deep_tests.rs)
   test result: ok. 5 passed (tests/isotonic_mutation_hardening_test.rs)
   Gesamt: 59/59 Tests PASSED (100% grün).
   ```
2. **Clippy Linter (`cargo clippy -p memfuse-calibration --all-features -- -D warnings`):**
   ```text
   Finished `dev` profile [unoptimized + debuginfo] target(s) in 6.05s
   0 Warnings / 0 Errors.
   ```
3. **Concurrency Stress Testing:** 10 sequentielle Test-Durchläufe mit `--test-threads=8` zeigten 0 Race Conditions und 0 Deadlocks.

---

## 6. EMPFEHLUNGEN & PRIORISIERTE FOLGE-TASKS

1. **Fix APM-2 in `HybridQueryBuilder::execute()` (`crates/memfuse-db/src/collection/query_builder.rs`):**
   Vor dem Aufruf von `hybrid_search_with_query_at` sollte der PID-Controller befragt werden, falls er vorhanden ist:
   ```rust
   let effective_rerank_pool_max = self
       .rerank_pool_max
       .or_else(|| self.pid_controller.as_ref().and_then(|p| p.lock().current_pool_size()));
   ```
2. **Bereinigung veralteter `ReplicatorState`-Referenzen in `memfuse-db`:**
   Entweder das Modul `replicator.rs` in `memfuse-calibration` wiedereinführen oder veraltete Importe und Felder in `memfuse-db` bereinigen, um den Compilierfehler `E0425` zu beheben.
