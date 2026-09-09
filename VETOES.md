# MemFuse — Feature-Veto-Register
> Maschinenlesbar. Wird von `.jules/`-Bootstrap-Sequenz eingelesen und von
> `xtask check-vetoes` gegen neue Commits geprüft (siehe Aufgabe 2).
> Fristen in `review_date` (bzw. `conditional_review_due`) werden aktiv im CI überwacht (Warnung 14 Tage vor Ablauf, harter Fehler bei Fristüberschreitung).
> Änderung nur via ADR in `DECISIONS.md` mit explizitem Bezug auf den Veto-Eintrag.

## Format
Jeder Eintrag: `feature_id`, `status` (permanent_rejected | conditionally_accepted),
`keywords` (Commit-Message/Code-Grep-Trigger), `reason`, `adr_ref` (falls vorhanden),
`review_date` (optionales Review-Frist-Datum YYYY-MM-DD für conditionally_accepted Einträge).

---

## VETO-F02

feature_id: F-02
status: conditionally_accepted
review_date: 2026-10-07
conditional_review_due: 2026-10-07
keywords: ["partial hnsw rebuild", "nucleation", "rebuild_region", "F-02"]
reason: >
  Aktives Re-Wiring eines HNSW-Teilgraphen zerstört Delaunay-ähnliche
  Nachbarschaftsbeziehungen zu Randknoten -> Recall-Kollaps. RwLock-Contention
  bei aktiver Neuverdrahtung ist unter Rust ohne Lösung.
scope_note: >
  Reines Tombstone-Pruning (ohne Re-Wiring) ist NICHT vom ursprünglichen Veto
  erfasst, unterliegt aber eigenem Gate (siehe adr_ref). Feature bleibt
  non-default via physio-nucleation bis Recall-Regressionstest 30 Tage stabil.
  Automatisierte 30-Tage-Stabilitätsmessung läuft täglich über .github/workflows/nucleation-recall-history.yml, Verlauf in benchmarks/results/nucleation_recall_history.jsonl, Statusprüfung via cargo xtask check-recall-stability.
adr_ref: DECISIONS.md#adr-070
last_verified: 2026-09-08

## VETO-F10

feature_id: F-10
status: permanent_rejected
keywords: ["cross-tenant", "osmotic knowledge exchange", "tenant knowledge sharing", "F-10"]
reason: >
  Bricht TenantId-Isolationsgarantie, KV-Cache-Bridge-Sicherheitsschicht und
  DeletionProof-Korrektheit. Kryptographische Löschgarantien sind über
  verschwimmende Mandantengrenzen mathematisch nicht beweisbar. DSGVO Art. 17
  Compliance-Risiko. Keine Alternative empfohlen -- Mandantenisolation ist absolut.
scope_note: >
  Kein Ausnahmepfad vorgesehen. Jede Implementierung die Daten zwischen
  TenantId-Kontexten bewegt oder aggregiert ohne expliziten, separat
  ADR-dokumentierten Merge-Operator wird abgelehnt.
adr_ref: null
last_verified: 2026-09-07

## VETO-OP3

feature_id: OP-03
status: conditionally_accepted
review_date: 2027-03-08
conditional_review_due: 2027-03-08
keywords: ["voice assistant", "jarvis", "realtime-audio", "speech-to-text", "voice/jarvis"]
reason: >
  Fokussierung auf PyPI-Library (ADR-077). Voice/Jarvis bindet erhebliche
  Audio-Streaming- und WebSocket-Komplexität, ohne die Kernstärke
  des bi-temporalen Gedächtnissubstrats zu validieren.
scope_note: >
  Formale Veto-Sperre mit 6 Monaten Wiedervorlagefrist (2027-03-08).
adr_ref: DECISIONS.md#adr-077
last_verified: 2026-09-08
