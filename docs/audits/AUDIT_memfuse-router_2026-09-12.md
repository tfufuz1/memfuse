# AUDIT REPORT: `memfuse-router` (Layer 3 — Kalibriertes Routing & Drift-Erkennung)

**Status:** AUDIT ABGESCHLOSSEN (Strikter Audit-Modus, keine Code-Modifikationen)
**Datum:** 2026-09-12
**Auditor:** Jules (MemFuse Core Engineer)
**Umfang:** `crates/memfuse-router` (5.504 Zeilen, 3 Kernmodule + 5 Testsets)

---

## 1. ZUSAMMENFASSUNG & COMPLIANCE-CHECK
Der systematische Audit von `memfuse-router` bestätigt die mathematische Korrektheit, Thread-Sicherheit und Architektur-Konformität des kalibrierten Routing-Systems. Der proaktive Drift-Wächter (Feature F-11) ist strikt event-driven implementiert, und die mathematischen Berechnungen zur KL-Divergenz und den Lyapunov-Exponenten weisen eine exzellente numerische Stabilität auf.

---

## 2. EVENT-DRIVEN TRIGGER-VERIFIKATION (F-11)

### Befund: **POSITIV VERIFIZIERT (Reaktionsschnell & Event-Driven)**
Die in `crates/memfuse-db/src/maintenance_scheduler.rs` getroffene Aussage:
> *"F-11 (LyapunovDriftWatcher.update()) ist bewusst NICHT hier im periodischen Tick enthalten. F-11 ist stattdessen reaktionsschnell & event-driven direkt nach jeder Routing-Entscheidung in `crates/memfuse-router/src/router.rs` integriert."*

wurde am Code direkt überprüft und vollumfänglich belegt:

1. **Ausschluss aus 60s MaintenanceScheduler-Tick:**
   In `crates/memfuse-db/src/maintenance_scheduler.rs` (Zeile 214–220) wird F-11 explizit vom periodischen Hintergrund-Scheduler ausgeschlossen.
2. **Exakter Event-Auslöser in `router.rs`:**
   In `crates/memfuse-router/src/router.rs` innerhalb von `RouterEngine::route(...)` (Zeilen 330–350) wird direkt nach Ermittlung des Non-Conformity-Scores das Update synchronsiert für die jeweilige Entscheidung getriggert:
   ```rust
   // 5. Update Lyapunov Drift Watcher with non-conformity score
   let drift_status = {
       let watchers = &mut new_state.lyapunov_watchers;
       if let Some(watcher) = watchers.get_mut(&selected_profile.name) {
           watcher.observe_score(non_conformity_score);
           let res = watcher.analyze();
           ...
       }
   };
   ```
3. **Fazit:** Die Drift-Erkennung reagiert **sofort bei jeder Suchanfrage** (Event-Driven) und wartet nicht auf ein periodisches 60s-Intervall. Die Behauptung ist somit faktenbasiert bewiesen und stellt keine getarnte periodische Ausführung dar.

---

## 3. MATHEMATISCHE GRENZFALL-ANALYSE (`lyapunov.rs`)

### 3.1 KL-Divergenz mit Laplace-1-Glättung
Die Kullback-Leibler Divergenz $D_t = \text{KL}(P_{curr} \parallel P_{base})$ wird über ein 10-Bin-Histogramm berechnet:
$$p_i = \frac{n_{curr,i} + 1}{n_{curr} + 10}, \quad q_i = \frac{n_{base,i} + 1}{n_{base} + 10}$$
- **Identische Verteilungen:** Wenn $P_{curr} = P_{base}$, gilt $p_i = q_i \implies \ln(p_i / q_i) = 0 \implies D_t = 0.0$. Dies wurde durch den Unit-Test `test_identical_distributions_zero_kl_divergence` mathematisch und numerisch bestätigt ($D_t < 10^{-5}$).
- **Leere Bins (Zero-Frequency Problem):** Durch die Laplace-1-Glättung ($\alpha = 1.0$) gilt stets $p_i > 0$ und $q_i > 0$. Divisionen durch Null oder $\ln(0)$ sind strukturell unmöglich.
- **Per-Bin Contribution Clipping (`MAX_BIN_KL_CONTRIBUTION = 10.0`):** Ein einzelner Bin kann die Summe nicht sprengen ($C_i = p_i \cdot \ln(p_i / q_i) \le 10.0$).
- **Globales Clipping ($D_t \in [0.0, 100.0]$):** Vor der Abspeicherung in `divergence_history` wird $D_t$ auf den Bereich $[0.0, 100.0]$ geclippt und via `d_t.is_finite()` abgesichert.

### 3.2 Diskreter Lyapunov-Exponent ($\lambda_t$)
Mathematische Schätzung:
$$\lambda_t = \frac{1}{w} \sum_{i=1}^{w} \ln \left| \frac{D_{t-i+1}}{D_{t-i}} \right|$$
- **Division by Zero Guard:** Nenner wird durch `.max(1e-10)` vor $0$-Divisonen geschützt.
- **Logarithmus-Guard:** Das Verhältnis $\frac{\text{num}}{\text{den}}$ wird durch `.abs().max(1e-10)` geschützt, womit $\ln(\le 0)$ ausgeschlossen ist.
- **Datenpunkte $< w$:** Liegen weniger als $w + 1$ Punkte in der Historie vor, gibt der Wächter deterministisch `LyapunovResult::InsufficientData` zurück.

---

## 4. SYSTEMATISCHER 6-PUNKTE-PRÜFKATALOG

### 1. Mathematische Korrektheit & Statistisches Design
- **Conformal Calibration (Gibbs & Candès 2021):** Quantil-Adaption $q_{t+1} = q_t + \gamma (\alpha - \mathbb{I}(s_t > q_t))$ ist in `ConformalCalibrator::update` korrekt implementiert. Invariante `INV-ROUTER-1` (`quantile_threshold` in $[0.0, 1.0]$) wird strikt durch Clamping eingehalten.
- **Warmup Window:** Erfordert mindestens $100$ Samples (`CALIBRATION_WARMUP_WINDOW`) für verlässliche Quantil-Absicherung.

### 2. Concurrency, Race Conditions & State Swaps
- **ArcSwap Dual-State Architektur:** `RouterState` (Profile, Kalibrierung, Drift-Wächter) wird atomar als Einheit via `ArcSwap` ausgetauscht. Read-Queries sind damit 100% lock-free.
- **Decoupled Pending Decisions Map:** `pending_decisions` ist in einem separaten `parking_lot::RwLock` untergebracht, um High-Frequency-Writes abzufangen, ohne teure `RouterState`-Clones bei jedem Routing auszulösen.

### 3. Invarianten & Boundary Validation
- **`SlmProfile::validate()`:** Erzwingt nicht-leere Namen/Endpoints und endliche, nicht-negative Werte für `min_relevance_score` und `resource_cost_estimate`.
- **`ConfigFingerprint` Invalidation (`INV-P8-1`):** `check_and_invalidate_fingerprint()` setzt die Kalibrierungsstatistik unverzüglich zurück, sobald sich der System-Fingerprint ändert oder `None` ist.
- **Pending Map Capacity Guard:** Bounded Map mit Kapazität $10.000$ und $300\text{s}$ TTL verhütet Memory Leaks bei unvollständigen Outcomes.

### 4. Error Handling & Zero-Panic Policy
- Strenge Einhaltung der Zero-Panic-Doktrin im Produktionscode: Kein `.unwrap()` / `.expect()` außerhalb von `#[cfg(test)]`.
- **Distanz-/Relevanz-Schutz:** Relevanzwerte werden auf `is_finite()` geprüft; NaN/Inf-Eingaben in `query_embedding` werfen sofort `MemFuseError::InvalidInput`.

### 5. Performance & Memory Management
- Atomare Thread-Sicherheit ohne Lock-Contention bei Hot-Reloads (`update_profiles`).
- Deterministische JSON-Sortierung für HashSet-Felder (`serde_sorted_u64_set`) garantiert reproduzierbare Serialisierung.

### 6. Architektureinhaltung & ADR-Compliance
- Strikte Trennung zwischen Layer 3 (Router Engine) und Layer 2 (LSM Storage / Vector Index).
- Volle ADR-020 Konformität.

---

## 5. PRIORISIERTE FOLGE-TASKS (RECOMMENDATIONS)

1. **[LOW] Lyapunov Window Allocation Optimization:**
   In `lyapunov.rs` könnte die Instanziierung von `VecDeque` bei `new(window_size)` noch strikter mit `VecDeque::with_capacity(window_size + 1)` ausgelegt werden.
2. **[INFO] Integrations-Test mit H-17:**
   Nach Abschluss des parallelen IMPL-Tasks H-17 sollte die End-to-End-Exposition von `status_str()` über `memfuse-db::stats()` in einem Systemtest abgesichert werden.
