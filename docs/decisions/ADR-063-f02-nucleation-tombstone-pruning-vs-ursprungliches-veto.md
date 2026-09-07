# ADR-063: F-02 Nucleation — Tombstone-Pruning-Variante vs. ursprüngliches Rebuild-Veto

*   **Datum**: 2026-09-07
*   **Status**: Eingeschränkt akzeptiert (mit hartem Gate)

## Kontext
Frühere Architekturanalysen (siehe externe Analyse-Session 2026-09-07) sprachen ein permanentes Veto gegen partiellen HNSW-Rebuild aus. Begründung: Delaunay-Nachbarschaftszerstörung bei aktivem Re-Wiring unter RwLock-Contention führt zu Recall-Kollaps.

Die tatsächliche Implementierung in `hnsw.rs:1812` (`rebuild_region()`) führt KEIN aktives Re-Wiring durch. Sie entfernt ausschließlich Referenzen auf tombstonierte Knoten aus Nachbarschaftslisten aktiver Knoten — ohne Ersatzkanten oder erneutes RNG-Pruning. Dies umgeht den im ursprünglichen Veto beschriebenen Worst Case (Lock-Contention durch aktive Neuverdrahtung), erzeugt aber ein separates, bislang ungetestetes Risiko: dauerhafter Grad-Verlust betroffener Knoten.

## Entscheidung
Die Tombstone-Pruning-Variante wird NICHT als generelles Veto-Verstoß behandelt, da sie technisch different von dem ist, wovor das ursprüngliche Veto warnte. Sie bleibt jedoch hinter `physio-nucleation` (non-default) UND darf erst dann in irgendeinem Kontext default-aktiviert werden, wenn:
1. `test_nucleation_recall_regression()` (siehe Test-PR) seit ≥ 30 Tagen stabil grün ist
2. Eine Grad-Wiederherstellungsstrategie evaluiert wurde (z.B. periodischer Vollrebuild als Sicherheitsnetz bei > X% Grad-Verlust in einer Region)

## Verworfene Alternativen
- Vollständige Entfernung von rebuild_region(): Verwirft Arbeit ohne technischen Grund, da das eigentliche Veto-Risiko (aktives Re-Wiring) nicht vorliegt.
- Sofortige Default-Aktivierung: Kein Recall-Nachweis vorhanden — abgelehnt.

## Konsequenzen
- `physio-nucleation` bleibt non-default bis Bedingungen erfüllt
- CI-Check (siehe Folge-PR VETOES.md) muss diesen ADR referenzieren können
- Grad-Verlust-Monitoring sollte in zukünftigem Observability-Sprint ergänzt werden
