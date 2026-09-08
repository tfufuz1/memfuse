# ADR-070: PID-Regler min_pool_size Kalibrierung und Default-Konsolidierung

* **Status:** Akzeptiert
* **Datum:** 2026-09-08
* **Anforderung / Referenz:** Gesamtspezifikation v7.0 §B.4, Technische Schulden A.8, arXiv:2604.01733

## Kontext & Problemstellung

In `crates/memfuse-calibration/src/pid.rs` steuert `PidController` dynamisch die Kandidatenpool-Größe für das Reranking zur Einhaltung des Latenzbudgets. Das Feld `min_pool_size` besaß im Quellcode unvollständig dokumentierte Werte und eine Inkonsistenz:

1. `PidController::default()` definierte `min_pool_size: 10`.
2. In Teststrukturen und partiellen Overrides existierten unbegründete Magic Numbers (`min_pool_size: 20`).

Keiner dieser Werte verfügte über eine dokumentierte empirische Grundlage. Die Gesamtspezifikation v7.0 §B.4 und die Studie arXiv:2604.01733 berichten jedoch, dass stabile Recall@5-Werte (0.888) erst ab einer Kandidatenpool-Größe von mindestens 100 erreicht werden. Ein zu kleiner Pool (10 oder 20) beeinträchtigt die Retrieval-Qualität drastisch, während ein zu großer Pool das p95-Latenzbudget überschreiten kann. Die Diskrepanz zwischen Quellcode-Defaults und Literaturbefunden stellte eine unzureichend dokumentierte Abweichung dar (Schuld A.8).

## Entscheidung

1. **Konsolidierung des Produktions-Defaults auf `PID_MIN_POOL_SIZE_DEFAULT = 50`:**
   Wir setzen den Default-Wert für `min_pool_size` in `PidController::default()` auf einen konservativen Mittelwert von 50 über die explizit publizierte Konstante `pub const PID_MIN_POOL_SIZE_DEFAULT: usize = 50;`.
2. **Begründung für den Übergangswert 50:**
   - **Verbesserung gegenüber 10/20:** Der Wert 50 liegt deutlich näher an der Literatur-Empfehlung ($\ge 100$) und verhindert drastische Recall-Einbrüche bei niedrigen Latenzen.
   - **Latenz-Schutz:** Der Wert bleibt vorerst unter 100, um eine Überlastung der p95-Reranking-Latenz auf ressourcenbeschränkten Systemen zu vermeiden, bis empirische Messungen auf MemFuse-Korpora vorliegen.
   - **Geltung bis Benchmark-Sweep (B.6):** Dieser ADR fixiert den Übergangsdefault. Ein anstehender Benchmark-Sweep via `memfuse-bench` (LongMemEval) über $min\_pool\_size \in \{10, 20, 50, 100\}$ wird die finale Pareto-Front zwischen Recall@5 und p95-Latenz ermitteln und den Default bei Bedarf via Folge-ADR anpassen.
3. **Beseitigung von Magic-Number-Literalen:**
   Der Default in `PidController::default()` nutzt ausschließlich `PID_MIN_POOL_SIZE_DEFAULT` und `PID_MAX_POOL_SIZE_DEFAULT`. Test-Overrides in Unit-Tests wurden explizit als solche kommentiert.

## Konsequenzen

- `PidController::default().min_pool_size` ist nun einheitlich 50.
- Im Crate `crates/memfuse-calibration` existieren keine undokumentierten Magic-Number-Produktions-Defaults für `min_pool_size`.
- **Follow-up (B.6):** Ein empirischer LongMemEval-Benchmark-Sweep zur Bestimmung des exakten Pareto-Optimums ($min\_pool\_size \in \{10, 20, 50, 100\}$) ist für das nächste Ingestion/Retrieval-Release einzuplanen.
