# ADR-033: Bi-temporale Zeitachsen (Validitätszeit + Transaktionszeit) im Wissensgraphen (Phase 2 Roadmap)


*   **Datum**: 2026-08-28
*   **Status**: ✅ Final
*   **Entscheidung**:
    - Der öffentliche Edge-Typ in `memfuse-core` (`pub struct Edge`) wird additiv um `valid_from: Option<TxId>` und `valid_to: Option<TxId>` mit `#[serde(default)]` erweitert.
    - `valid_from = None` signalisiert "seit jeher gültig", `valid_to = None` signalisiert "weiterhin gültig".
    - `TxId` wird ausnahmslos als Träger der fachlichen Zeitachsen verwendet (Einhaltung des `SystemTime`-Verbots gemäß AGENTS.md Abschnitt 4).
    - Der `GraphIndex`-Trait erhält die Methode `traverse_at_time(&self, start: EntityId, max_hops: usize, as_of: TxId) -> Result<Vec<(EntityId, f32)>>` mit Fail-Safe Default-Implementierung `Err(MemFuseError::PolicyViolation(...))`.
    - `CsrGraph` implementiert `traverse_at_time` konkret: Traversierung filtert Kanten heraus, für die `as_of < valid_from` oder `valid_to.is_some_and(|t| as_of >= t)` gilt.
*   **Alternativen**:
    - Verwendung von Wall-Clock timestamps (`SystemTime` / Unix Nanos). Verworfen, da `SystemTime` im gesamten Workspace für Sequenzierung strikt verboten ist (AGENTS.md).
    - Anlegen eines separaten `TemporalEdge`-Typs. Verworfen, um Typ-Explosion zu vermeiden und abwärtskompatible Deserialisierung Altdaten über `#[serde(default)]` zu sichern.
*   **Begründung**:
    - Ermöglicht präzise historische Wissensgraph-Abfragen ("was galt zum Zeitpunkt TxId X") ohne Breaking Changes bei bestehenden SSTable-Daten.
*   **Konsequenzen**:
    - `Edge`-Initialisierungen und Deserialisierung bleiben abwärtskompatibel.
    - CSR-Graph speichert und persistiert Validitätsbereiche.

---
