# MemFuse — Feature-Veto-Register
> Maschinenlesbar. Wird von `.jules/`-Bootstrap-Sequenz eingelesen und von
> `xtask check-vetoes` gegen neue Commits geprüft (siehe Aufgabe 2).
> Änderung nur via ADR unter `docs/decisions/` mit explizitem Bezug auf den Veto-Eintrag.

## Format
Jeder Eintrag: `feature_id`, `status` (permanent_rejected | conditionally_accepted),
`keywords` (Commit-Message/Code-Grep-Trigger), `reason`, `adr_ref` (falls vorhanden),
`conditional_review_due` (optionales Review-Frist-Datum YYYY-MM-DD für conditionally_accepted Einträge).

---

## VETO-F02

feature_id: F-02
status: conditionally_accepted
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
adr_ref: docs/decisions/ADR-0XX-f02-nucleation-tombstone-pruning-vs-ursprungliches-veto.md
last_verified: 2026-09-07

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
