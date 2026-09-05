# T-07 · Metakognitiver Checkpoint für MemFuse

## 1. Warum T-07 Checkpoints unverzichtbar sind
Wenn autonome Agenten komplexe Refactorings oder Feature-Entwicklungen durchführen (z. B. Umbau der WAL-Serialisierung oder Anpassung der MVCC-Snapshot-Isolation), führt ein einziger unbemerkter Zwischenfehler zum Scheitern der gesamten Kette ("Silent Error Propagation").

Der **T-07 Checkpoint** erzwingt das PDCA-Prinzip (Plan-Do-Check-Act):
1. **Plan:** Was wollte der Agent tun?
2. **Do:** Ungefilterter CLI- oder Tool-Output.
3. **Check:** Kritische Selbstprüfung gegen Akzeptanzkriterien und Invarianten.
4. **Act:** Weitergehen oder **Mutation-Enforced Retry**.

## 2. Invarianten-Schutz
Vor dem Weitergehen wird geprüft:
- `preconditions_held`: Vorbedingungen erfüllt?
- `postconditions_met`: Nachbedingungen erreicht?
- `invariants_intact`: System-Invarianten (z. B. Memory-Safety, Lock-Freiheit) unversehrt?

Falls nicht, ist ein einfaches "Nochmal probieren" mit demselben Prompt verboten: Der Agent muss explizit deklarieren, was an seiner Strategie mutiert wurde (`<mutated_hypothesis>`).
