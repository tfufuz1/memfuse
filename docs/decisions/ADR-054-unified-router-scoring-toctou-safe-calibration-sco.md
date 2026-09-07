# ADR-054: Unified Router Scoring & TOCTOU-Safe Calibration Scope

*   **Datum**: 2026-09-03
*   **Status**: ✅ Final
*   **Entscheidung**:
    1. `SlmProfile::domain_communities` nutzt `HashSet<u64>` für O(1) Community-Lookups mit deterministischer `sorted_u64_set` Serde-Unterstützung.
    2. Candidate Scoring wird in der zentralen Hilfsfunktion `score_profile()` mit der benannten Konstante `COMMUNITY_RELEVANCE_BOOST = 1.2` konsolidiert.
    3. Die legacy `recalibrate()` Methode wird aus `ProfileCalibrationState` entfernt.
    4. Routing-Entscheidung, Scoring und Kalibrierungs-Updates in `RouterEngine::route()` erfolgen atomar innerhalb einer einzigen Schreib-Lock-Akquise.
*   **Begründung**: Schließt Race Conditions (TOCTOU) bei parallelem Routing, eliminiert redundante Scoring-Implementierungen und vereinheitlicht die Konformal-Kalibrierung.

---
