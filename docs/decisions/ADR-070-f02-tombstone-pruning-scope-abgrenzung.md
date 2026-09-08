# ADR-070: F-02 Scope-Abgrenzung — Reines Tombstone-Pruning vs. Ursprüngliches Veto

## Status
Final (2026-09-08)

## Kontext
Im Feature-Veto-Register (`VETOES.md`) verbietet VETO-F02 das partielle Rebuilding von HNSW-Teilgraphen ("Partial HNSW Rebuild" / "Nucleation"). Die Rationale des ursprünglichen Vetos stützt sich auf zwei Hauptrisiken:
1. **Recall-Kollaps durch aktives Re-Wiring:** Die algorithmische Neuverdrahtung von Nachbarschaftskanten in einem lokalen Teilgraphen ohne globale Delaunay-Neukalibrierung zerstört die Navigierbarkeit zu entfernten Randknoten.
2. **RwLock-Contention:** Aktive Graphmodifikationen (Hinzufügen neuer Kanten, Kanten-Heuristiken) unter hoher Last führen zu Sperrkonflikten auf Knotenebene.

In `crates/memfuse-index/src/hnsw.rs` existiert die Funktion `rebuild_region()`. Es bestand eine offene Prozesslücke bezüglich der Frage, ob `rebuild_region()` gegen VETO-F02 verstößt. Eine genaue Code-Analyse zeigt:
`rebuild_region()` (Zeilen 1812–1865) führt **ausschließlich reines Tombstone-Pruning** durch:
- Es werden lediglich existierende Referenzen auf als gelöscht markierte Knoten aus den Kantenlisten aktiver Nachbarn entfernt (`conns.retain(|neighbor_id| !tombstoned_set.contains(neighbor_id))`).
- Es findet **keinerlei aktives Re-Wiring** (Suche neuer Ersatznachbarn oder Einfügen neuer Delaunay-Kanten) statt.

## Entscheidung
1. **Formale Scope-Abgrenzung:** Reines Tombstone-Pruning (Entfernen toter Referenzen ohne Neuverdrahtung von Nachbarschaftskanten) ist **NICHT** vom ursprünglichen VETO-F02 erfasst, da es keine Kantenverbindungen verändert oder neu aufbaut, sondern lediglich ungültige Speicherzeiger/IDs bereinigt.
2. **Feature-Gating & Safety-Guard:** Obwohl reines Tombstone-Pruning algorithmisch sicher bezüglich RwLock-Mutationen ist, birgt das Entfernen von Kanten ohne Ersatz das verbleibende Risiko eines Grad-Verlusts (Reduzierung der Kantenanzahl pro Knoten). Daher bleibt das Feature `partial-index-rebuild` (sowie die Nucleation-Steuerung `physio-nucleation`) **non-default** und darf erst für den Produktionseinsatz freigegeben werden, wenn die Stabilität der Recall-Werte nachgewiesen ist.

## Konsequenzen
- **Technischer Nachweis der Recall-Stabilität:** Der Nachweis, dass `rebuild_region()` den Recall nicht unzulässig degradiert, wird automatisiert über den Regressionstest `crates/memfuse-index/tests/nucleation_recall.rs` (`test_nucleation_recall_regression`) geführt.
- Der Test verifiziert, dass:
  1. `rebuild_region()` den Recall@10 gegenüber reinem Tombstone-Markieren um nicht mehr als 5 Prozentpunkte (5pp) verschlechtert.
  2. Der absolute Recall-Verlust gegenüber dem unveränderten Index unter 15 Prozentpunkten (15pp) bleibt.
- Das Feature bleibt hinter dem Cargo-Feature-Gate `partial-index-rebuild` isoliert.

## enforced_by
- `crates/memfuse-index/src/hnsw.rs:1812` (`pub async fn rebuild_region`)
- `crates/memfuse-index/Cargo.toml` (`[features] partial-index-rebuild = []`)
- `crates/memfuse-index/tests/nucleation_recall.rs` (`test_nucleation_recall_regression`)
