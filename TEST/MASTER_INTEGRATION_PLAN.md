# MemFuse Fault-Injection & Chaos-Test Integrationsplan (v2)

> **Ersetzt:** `TEST/MASTER_INTEGRATION_PLAN.md` (v1, 7 Hebel)
> **Status:** Hebel 1–4, 6, 7 verworfen und aus `TEST/` entfernt (Architektur-Review 2026-09-05).
> **Verbleibende Basis:** Hebel 5 — *Chaos Engineering & Crash-Resilienz*, adaptiert aus `chimeraDB` SPEC-035.
> **Geltungsbereich:** ausschließlich `crates/memfuse-store` (Layer 1, WAL/MVCC/LSM). Kein neues Workspace-Crate, keine neue Runtime-Abhängigkeit, keine Änderung an produktivem Code ohne separate ADR.

---

## 1. Warum dieser Plan anders aussieht als das Original

Das Original (`chimera-chaos`, SPEC-035) wurde für **ChimeraDB** geschrieben — ein verteiltes, Multi-Node-System mit Raft-Sync, gRPC-API und CRC32C-only-Integrität. MemFuse ist laut `AGENTS.md` §1 explizit **kein** verteiltes System ("no Docker, no HTTP server as production component") und nutzt HMAC-SHA256-Chaining statt reinem CRC32C (`rules/wal_crypto.md`). Zwei der zehn Original-Szenarien (`NetworkDegradation`, `RogueAgentFlood`) beschreiben Infrastruktur, die in MemFuse nicht existiert, und wurden ersatzlos gestrichen bzw. lokal reinterpretiert (siehe §3).

Zusätzlich existiert bereits substanzielle Fault-Injection-Testabdeckung in `crates/memfuse-store/tests/`:

| Datei | Deckt bereits ab |
|---|---|
| `wal_fuzzing.rs` | Randomisierte Bit-Flips **im WAL**, CRC-Feld-Korruption, Header-Korruption (proptest-basiert) |
| `wal_robustness.rs` | HMAC-Chain-Verletzung, V2→V3-Migration, Length-Extension-Resistenz |
| `crash_recovery.rs` | WAL-Replay nach Neustart, partielle Writes, atomarer MemTable-Flush |
| `flush_crash_simulation.rs` | Flush-Reihenfolge (WAL-Löschung erst nach SSTable-Persistenz) |
| `resource_leaks.rs` | RSS-Footprint unter Dauerlast, FD-Leak-Erkennung |
| `executor_starvation_test.rs` | Multi-/Single-Thread-Executor unter Last |

Der Plan importiert deshalb **nicht** den kompletten Chimera-Scope, sondern schließt gezielt die verbleibenden Lücken:

1. Kein Test korrumpiert bisher **SSTable**-Dateien (Block/Bloom/Index-CRC) — nur WAL.
2. Kein Test simuliert einen **echten Prozessabbruch** (SIGKILL) während eines aktiven WAL-Writes — alle bisherigen "Crash"-Tests simulieren Korruption nachträglich auf Byte-Ebene, nicht durch echten Prozesstod.
3. Kein Test bricht **Tokio-Tasks aktiv mitten in einer Transaktion ab** (`TaskMassacre`).
4. Kein Test treibt `ResourceTracker`/`TokenBudget` (`crates/memfuse-core/src/types/budget.rs`) gezielt unter **konkurrierender Schreiblast** in `MemFuseError::MemoryBudgetExceeded`, um Backpressure ohne Datenkorruption zu beweisen.
5. Es gibt keine **kombinierte** Fehler-Matrix (mehrere Fault-Typen gleichzeitig/sequenziell in randomisierter Reihenfolge) — nur isolierte Einzeltests.

Das ist der tatsächliche, nicht-redundante Mehrwert von Hebel 5.

---

## 2. Architekturentscheidung: Test-only, kein neues Crate

`AGENTS.md` §3 fixiert 15 Workspace-Crates. Ein neues Crate `memfuse-chaos` (wie `chimera-chaos`) würde die DAG-Topologie ändern und braucht laut §5 ("ASK: Add new external dependencies") explizite menschliche Freigabe — dafür gibt es hier keinen Grund, weil sich der komplette Umfang als `tests/`- und `examples/`-Code in `crates/memfuse-store` abbilden lässt, **ohne** eine Zeile Produktionscode in `src/` zu ändern.

Begründung pro Konstrukt:

- **`FaultInjector`/`ChaosScenario` aus dem Original wird nicht 1:1 portiert.** Es war für synchrone Injection-Points *im Produktionscode* gedacht (`FaultInjector::inject_sync("point")` an beliebigen Stellen in `chimera-storage`). Das würde in MemFuse bedeuten, Injection-Hooks in `wal.rs`/`sstable.rs` einzubauen — das ist laut `AGENTS.md` §5 ASK-pflichtig ("Change public API signatures" / neue `unsafe`- bzw. Kontrollfluss-Hooks im Hot-Path) und bietet gegenüber externer Dateimanipulation (siehe unten) keinen Mehrwert.
- **Reale Fehler statt simulierter Hooks, wo möglich:** Für `PowerCutSimulation` wird ein echter `SIGKILL` via `std::process::Child::kill()` (Standardbibliothek, keine neue Dependency) gegen einen separaten Worker-Prozess (`examples/chaos_writer.rs`) verwendet — realistischer als ein simulierter Abbruch und ohne jede Prod-Code-Änderung.
- **`DroppedWrite` ohne Syscall-Interception:** Statt eines Fault-Injection-Hooks wird die WAL-Datei/das Verzeichnis testseitig `chmod`-readonly gesetzt, um einen echten `EACCES`/`EROFS`-Fehler vom OS zu erzwingen. Prüft exakt dieselbe Invariante ("fsync-Fehler MÜSSEN propagiert werden", `crates/memfuse-store/AGENTS.md` §3) ohne Produktionscode anzufassen.
- **`IOLatency` und `NetworkDegradation` werden explizit NICHT umgesetzt** (siehe §3) — sie würden einen Injection-Hook im Hot-Path erfordern, für den es aktuell keinen belegten Bedarf gibt (kein User-Report zu Slow-Disk-Verhalten, keine Netzwerkschicht vorhanden).

---

## 3. Szenario-Mapping: Original → MemFuse-Realität

| Original-Szenario (Chimera) | Entscheidung | Umsetzung in MemFuse |
|---|---|---|
| `TaskMassacre` | ✅ Übernehmen | `tests/chaos_task_massacre.rs` — echte `JoinHandle::abort()` gegen konkurrierende Writer auf `LsmStorage` |
| `BitFlipInjection` | ✅ Übernehmen, erweitert | `tests/chaos_bitflip_sstable.rs` — Lücke schließen: SSTable-Block/Bloom/Index-CRC statt nur WAL (WAL bereits durch `wal_fuzzing.rs` abgedeckt) |
| `PowerCutSimulation` | ✅ Übernehmen, härter | `tests/chaos_power_cut.rs` + `examples/chaos_writer.rs` — echter Prozess-Kill statt Byte-Truncation-Simulation |
| `TruncatedWALFile` | ⚠️ Bereits abgedeckt | `test_wal_recovery_from_partial_write` (`wal_robustness.rs`) und `test_partial_write_is_detected` (`crash_recovery.rs`) decken das ab — kein neuer Test nötig |
| `MemoryExhaustion` / `OOMGuardTriggered` | ✅ Übernehmen | `tests/chaos_memory_pressure.rs` — nutzt reales `ResourceTracker`/`LsmConfig`-Budget, kein synthetischer Fehler |
| `DroppedWrite` | ✅ Übernehmen, real | `tests/chaos_dropped_write.rs` — `chmod`-basierter echter I/O-Fehler |
| `RogueAgentFlood` | 🔄 Reinterpretiert | Kein Netzwerk/Multi-Agent-RPC in MemFuse. Ersetzt durch `ConcurrentWriteFlood` **innerhalb** von `chaos_memory_pressure.rs` (viele lokale Tasks fluten `LsmStorage` gleichzeitig) — testet dieselbe Invariante (Backpressure schützt vor Überlastung), ohne erfundene Netzwerkschicht |
| `IOLatency` | ❌ Verworfen | Erfordert Hot-Path-Hook (ASK-pflichtig laut `AGENTS.md` §5), kein belegter Bedarf. Bei Bedarf: separater ADR-Vorschlag mit konkretem Use-Case (z. B. Netzwerk-Storage-Support) |
| `NetworkDegradation` | ❌ Verworfen | Keine Netzwerk-/Multi-Node-Schicht in MemFuse (ADR-010: stdio-only). Nicht anwendbar. |

Zusätzlich neu (nicht im Original, aber aus der Lückenanalyse in §1 folgend):

| Neu | Umsetzung |
|---|---|
| Kombinierte Fault-Matrix | `tests/chaos_matrix.rs` — führt 2+ Szenarien aus §3 in randomisierter Reihenfolge/Seed aus, `#[ignore]`-gated, nur in nightly CI |

---

## 4. Governance & Dokumentation

1. **Neue ADR** (Nummer live ermitteln: `grep -oP '(?<=^## ADR-)\d+' DECISIONS.md | sort -n | tail -1`, NICHT aus diesem Dokument übernehmen) mit Titel *"Fault-Injection-Testsuite für WAL V3/MVCC (adaptiert aus chimeraDB SPEC-035)"*. Muss dokumentieren:
   - Warum kein neues Crate (siehe §2)
   - Warum `IOLatency`/`NetworkDegradation` verworfen wurden (siehe §3)
   - CI-Kadenz-Entscheidung (siehe Punkt 3 unten)
2. **Neue Regel-Datei** `rules/chaos_testing.md` (MemFuse-natives Format, **kein** SPEC-Import) mit: Szenario-Tabelle aus §3, Ground-Truth-Pflicht (siehe Anti-Test-Mirroring, `rules/testing.md`), Verbot von Produktionscode-Änderungen in dieser Test-Suite ohne separate ADR.
3. **CI-Kadenz:** Diese Tests laufen **nicht** bei jedem Commit (60–100 Commits/Tag laut Projekt-Historie würden das Feedback zu langsam machen), sondern:
   - `chaos_power_cut`, `chaos_task_massacre`, `chaos_bitflip_sstable`, `chaos_dropped_write`, `chaos_memory_pressure`: normale `#[tokio::test]`, laufen in `cargo test --workspace` mit (kurze Einzellaufzeit).
   - `chaos_matrix.rs`: `#[ignore]`-gated, eigener `just chaos-test`-Recipe, eigener nightly GitHub-Actions-Workflow (`.github/workflows/chaos.yml`, `schedule: cron`), **blockiert keine PRs**.
4. `TEST/README.md` und dieses Dokument ersetzen `TEST/MASTER_INTEGRATION_PLAN.md` vollständig; Hebel 1–4/6/7-Verzeichnisse sind bereits gelöscht.

---

## 5. Beobachtung außerhalb des Kernplans (nicht Teil dieser Implementierung)

Bei der Code-Analyse für `chaos_memory_pressure.rs` fiel auf: `LsmStorage::commit` (`lsm.rs:1004f.`) behandelt einen Fehler von `budget.consume_memory()` aktuell nur mit `tracing::warn!`, nicht mit Fehler-Propagation. Das ist möglicherweise beabsichtigt (Soft-Accounting statt Hard-Reject an dieser Stelle, während `has_memory_capacity()` an anderer Stelle hart prüft), aber es sollte **explizit** in `chaos_memory_pressure.rs` mitgetestet und im PR-Kommentar benannt werden — nicht stillschweigend als Bug behandeln oder stillschweigend ignorieren (vgl. `.jules/AUDIT_INTAKE_PROTOCOL.md`). Falls sich daraus ein echtes Risiko ergibt, gehört das in eine eigene, separate ADR-Diskussion — nicht in diesen Plan gemischt.

---

## 6. Phasenplan

| Phase | Datei(en) | Aufwand | Abhängigkeit |
|---|---|---|---|
| 0 | ADR-Entwurf + `rules/chaos_testing.md` | 0,5 Tag | keine |
| 1 | `crates/memfuse-store/examples/chaos_writer.rs` | 0,5 Tag | keine |
| 2 | `tests/chaos_power_cut.rs` | 1 Tag | Phase 1 |
| 3 | `tests/chaos_task_massacre.rs` | 0,5 Tag | keine |
| 4 | `tests/chaos_bitflip_sstable.rs` | 1 Tag | keine |
| 5 | `tests/chaos_dropped_write.rs` | 0,5 Tag | keine |
| 6 | `tests/chaos_memory_pressure.rs` | 1 Tag | keine |
| 7 | `tests/chaos_matrix.rs` + `justfile`-Recipe + `.github/workflows/chaos.yml` | 1 Tag | Phasen 2–6 |

Gesamt: ~6 Personentage solo — realistisch neben dem laufenden Tagesgeschäft, ohne neue Abhängigkeiten, ohne Architektur-Risiko.

---

## 7. Implementierungs-Prompts

Jeder Block ist als eigenständiger Prompt für einen Coding-Agenten (Claude Code o. ä.) innerhalb dieses Repos gedacht. Reihenfolge = Phasenreihenfolge aus §6. Jeder Prompt geht davon aus, dass `.jules/SESSION_BOOTSTRAP.md` bereits gelaufen ist.

### Prompt 0 — ADR + Regel-Datei

```markdown
Kontext: Du arbeitest im MemFuse-Repo. Lies zuerst `AGENTS.md`, `DECISIONS.md` (nur die
letzten 5 ADRs) und `rules/testing.md`, `rules/wal_crypto.md`.

Aufgabe:
1. Ermittle die aktuell höchste ADR-Nummer live mit:
   `grep -oP '(?<=^## ADR-)\d+' DECISIONS.md | sort -n | tail -1`
   Verwende NIEMALS eine Nummer aus einem Prompt oder einer alten Analyse.
2. Füge in `DECISIONS.md` einen neuen ADR-Eintrag an (Format wie bestehende ADRs,
   siehe z.B. ADR-010) mit Titel:
   "Fault-Injection-Testsuite für WAL V3/MVCC (adaptiert aus chimeraDB SPEC-035)"
   Inhalt MUSS enthalten:
   - Entscheidung: Test-only Integrationstests + ein `examples/`-Binary in
     `crates/memfuse-store`, KEIN neues Workspace-Crate, KEINE Änderung an
     `crates/memfuse-store/src/**`.
   - Alternativen: chimera-chaos-artiges eigenes Crate mit Produktions-Hooks
     (`FaultInjector::inject_sync`) — verworfen, da ASK-pflichtige API-/Hot-Path-
     Änderung ohne belegten Bedarf.
   - Explizit verworfene Szenarien: `IOLatency`, `NetworkDegradation` — Begründung:
     keine Netzwerkschicht in MemFuse (ADR-010), kein belegter Slow-Disk-Use-Case.
   - CI-Kadenz: Einzeltests laufen in `cargo test --workspace`, die kombinierte
     Fault-Matrix (`chaos_matrix.rs`) NUR nightly, `#[ignore]`-gated, blockiert
     keine PRs.
3. Erstelle `rules/chaos_testing.md` (neue Datei, KEIN SPEC-Format, sondern im
   Stil der bestehenden `rules/*.md`-Dateien) mit:
   - Der Szenario-Tabelle aus diesem Plan (§3), gekürzt auf das Wesentliche.
   - Regel: "Jeder Chaos-Test MUSS einen von der Implementierung unabhängigen
     Ground-Truth-Wert verwenden (siehe `rules/testing.md` Anti-Test-Mirroring),
     z.B. eine externe Log-Datei mit tatsächlich geschriebenen Werten VOR jedem
     Schreibversuch, NICHT eine aus dem WAL selbst rekonstruierte Erwartung."
   - Regel: "Diese Testsuite darf ausschließlich `tests/` und `examples/` in
     `crates/memfuse-store` verändern. Jede Änderung an `src/` im Rahmen dieser
     Suite erfordert eine eigene, separate ADR."
4. Trage das neue Dokument in die `rules/*.md`-Referenztabelle in `AGENTS.md` §7
   ein (Tabelle "Governance Documents").
5. Ersetze `TEST/MASTER_INTEGRATION_PLAN.md` durch dieses Dokument (Dateiname
   beibehalten) und aktualisiere `TEST/README.md` so, dass es nur noch Hebel 5
   referenziert; entferne alle Verweise auf Hebel 1–4, 6, 7.
6. `just sync-docs` NICHT nötig für diesen Prompt (reine Doku-Änderung außerhalb
   der Auto-generierten Dateien), aber `just sync-docs-check` am Ende ausführen
   zur Sicherheit.

Nicht tun: Keine Code-Datei in `crates/memfuse-store/src/` anfassen. Keine neue
Cargo-Dependency hinzufügen.
```

### Prompt 1 — Chaos-Writer-Worker (Grundlage für echten Prozess-Kill)

```markdown
Kontext: Lies `crates/memfuse-store/AGENTS.md` vollständig, insbesondere die
Abschnitte zu `LsmStorage`, WAL-First-Regel und Atomic-Rename-Pattern. Lies
`crates/memfuse-store/src/lsm.rs` (Funktionssignaturen von `LsmStorage::open`
und `commit`/`put`) BEVOR du irgendetwas schreibst — API könnte sich seit
diesem Prompt geändert haben.

Aufgabe: Erstelle `crates/memfuse-store/examples/chaos_writer.rs`.

Anforderungen:
1. CLI-Args (via `std::env::args`, KEINE neue Dependency wie `clap` — das ist
   ein internes Test-Werkzeug, kein Produkt-Feature): `<storage_dir> <n_writes>
   <ground_truth_log_path>`.
2. Öffnet eine `LsmStorage` am angegebenen Pfad (reale, produktive API — keine
   Mocks).
3. Schreibt in einer Schleife `n_writes` Einträge mit monoton steigenden Keys
   und zufälligem Payload (Werte via `rand`, bereits Workspace-Dependency).
4. VOR jedem `commit`/`put`-Aufruf: schreibt Key+Value+laufenden Zähler
   synchron (mit `fsync`) in `ground_truth_log_path` (einfaches Zeilenformat,
   ein Eintrag pro Zeile). Das ist die Ground-Truth-Referenz für den
   aufrufenden Test — sie darf NICHT aus derselben `LsmStorage`-Instanz
   gelesen werden (Anti-Test-Mirroring, `rules/chaos_testing.md`).
5. Nach jedem Commit: kurzer `tokio::time::sleep` (wenige Millisekunden,
   konfigurierbar über eine Konstante oben im File) — gibt dem aufrufenden
   Test genug Zeitfenster, um den Prozess an einem zufälligen Punkt zu killen.
6. Alle Fehler via `?`/`MemFuseError` propagieren — `.unwrap()`/`.expect()`
   sind in diesem Binary NICHT erlaubt (`AGENTS.md` §5: "NEVER: .expect() in
   Produktionscode" — auch wenn es ein Test-Werkzeug ist, es kompiliert im
   selben Crate und unterliegt dem `debt-audit`).
7. Kein `unsafe`.

Verifikation: `cargo build --example chaos_writer -p memfuse-store` muss grün
sein. `just debt-audit` darf für diese neue Datei keine neuen Findings zeigen.
```

### Prompt 2 — Echter Power-Cut-Test

```markdown
Kontext: Lies `crates/memfuse-store/tests/crash_recovery.rs` und
`crates/memfuse-store/tests/wal_robustness.rs` vollständig — dein neuer Test
darf NICHTS duplizieren, was dort schon geprüft wird (Zweck ist der ECHTE
Prozess-Kill, nicht erneute Byte-Level-Korruption). Lies außerdem
`examples/chaos_writer.rs` aus Prompt 1.

Aufgabe: Erstelle `crates/memfuse-store/tests/chaos_power_cut.rs`.

Anforderungen:
1. Für mehrere Iterationen (z.B. 10, mit unterschiedlichem Random-Seed pro
   Iteration):
   a. Erzeuge ein `tempfile::tempdir()` für Storage + eine separate
      Ground-Truth-Log-Datei.
   b. Starte `chaos_writer` als Subprozess via
      `std::process::Command::new(env!("CARGO_BIN_EXE_..."))` — für
      `[[example]]`-Targets: `Command::new(assert_cmd`-freien Weg via
      `std::env::current_exe()`-Trick ODER direkt
      `cargo run --example chaos_writer --` via `Command::new("cargo")` mit
      `--quiet`. Prüfe zuerst, welcher Weg in diesem Workspace bereits an
      anderer Stelle für Beispiel-Binaries verwendet wird (`grep -rn
      "CARGO_BIN_EXE\|--example" crates/*/tests/*.rs`), und folge dem
      bestehenden Muster für Konsistenz statt einen dritten Weg einzuführen.
   c. Warte eine randomisierte Zeitspanne (z.B. 50–500ms, `rand`-basiert).
   d. Rufe `child.kill()` auf (Standardbibliothek — sendet auf Unix SIGKILL,
      auf Windows TerminateProcess; KEINE neue Dependency wie `nix`
      einführen).
   e. Öffne die `LsmStorage` am selben Pfad erneut (reale Reopen-/Recovery-
      Logik, kein Mock).
   f. Lies die Ground-Truth-Log-Datei. Für jeden dort protokollierten
      Eintrag: wenn der Eintrag laut Log VOR dem Kill vollständig
      geschrieben UND von `chaos_writer` als "committed" bestätigt wurde
      (letzte vollständige Zeile im Log vor Prozessende), MUSS er nach dem
      Reopen über die reguläre `get`-API lesbar sein.
   g. Kein Eintrag NACH der letzten vollständigen Ground-Truth-Zeile darf
      sichtbar sein (kein "Phantom-Commit" durch einen Torn Write).
   h. Der Reopen-Vorgang selbst darf NIEMALS painicken oder einen anderen
      Fehler als einen dokumentierten `MemFuseError` werfen.
2. Nutze `#[tokio::test]` pro Iteration oder eine Schleife innerhalb eines
   einzigen Tests — orientiere dich an der Struktur bestehender Tests in
   `crash_recovery.rs`.
3. Assertions MÜSSEN gegen die externe Ground-Truth-Datei prüfen, NIEMALS
   gegen eine aus `LsmStorage` selbst abgeleitete Erwartung (sonst
   Test-Mirroring, `rules/testing.md`).

Verifikation: `cargo test -p memfuse-store --test chaos_power_cut` grün,
lokal mindestens 3x hintereinander (`just triple-test`-Prinzip) ohne Flakes.
Falls Flakes auftreten: NICHT den Test durch größere Sleep-Werte "reparieren"
ohne den Grund zu verstehen — Flakiness in diesem Test ist ein Signal für
eine echte Race Condition im WAL, kein Test-Problem per Default-Annahme.
```

### Prompt 3 — Task-Massacre-Test

```markdown
Kontext: Lies `crates/memfuse-store/tests/executor_starvation_test.rs`
(bestehendes Muster für konkurrierende Writer) und die Abschnitte
"last_committed_tx — Single Load Rule" sowie "TOMBSTONE_BIT-Disziplin" in
`crates/memfuse-store/AGENTS.md`.

Aufgabe: Erstelle `crates/memfuse-store/tests/chaos_task_massacre.rs`.

Anforderungen:
1. Öffne eine reale `LsmStorage` in einem `tempdir()`.
2. Spawne N (z.B. 50) `tokio::spawn`-Tasks, jede führt eine Sequenz von
   Commits mit eindeutigen, vorab bekannten Keys durch. Führe VOR dem Spawn
   Buch über alle geplanten (Key, Value)-Paare in einem lokalen `Vec`
   außerhalb der Tasks — das ist die Ground-Truth.
3. Nach einer kurzen Anlaufzeit: `.abort()` auf eine zufällige Teilmenge
   (z.B. 30%) der `JoinHandle`s, WÄHREND die übrigen Tasks weiterlaufen.
4. Warte auf alle verbleibenden (nicht abgebrochenen) Tasks
   (`JoinHandle::await`, `Err` bei `JoinError::is_cancelled()` ignorieren,
   andere Fehler propagieren/failen lassen).
5. Assertions:
   a. Für jeden Key, dessen zugehöriger Task NICHT abgebrochen wurde UND
      dessen Commit laut Rückgabewert erfolgreich war: Wert MUSS über `get`
      korrekt lesbar sein.
   b. Für Keys aus abgebrochenen Tasks: entweder vollständig sichtbar
      (Commit war vor dem Abort bereits durch) oder vollständig unsichtbar
      (kein Teilzustand) — kein Wert darf halb geschrieben oder korrupt sein.
   c. Ein anschließender vollständiger `LsmStorage`-Reopen (Drop + neu
      öffnen) darf nicht panicken und muss exakt denselben Datenstand wie
      vor dem Reopen liefern (Determinismus der Recovery).
6. Kein `unsafe`, keine neue Dependency.

Verifikation: `cargo test -p memfuse-store --test chaos_task_massacre`,
mindestens 3x wiederholt lokal wegen der Nichtdeterminismus-Natur von
Task-Scheduling. Bei Instabilität: Ursache im Kommentar dokumentieren, nicht
den Test stillschweigend entschärfen.
```

### Prompt 4 — SSTable-Bit-Flip-Fuzzing (echte Lücke)

```markdown
Kontext: Lies `crates/memfuse-store/tests/wal_fuzzing.rs` VOLLSTÄNDIG als
Stil- und Strukturvorbild (dort existiert das Muster bereits für WAL-Dateien
— du überträgst es NICHT 1:1, sondern baust die SSTable-Variante, die aktuell
fehlt). Lies außerdem `crates/memfuse-store/src/sstable.rs`, insbesondere die
Abschnitte zu `BloomFilter::to_bytes`/`from_bytes`, Block-CRC
(`crc32fast::hash`) und Index-CRC — zitiere KEINE Zeilennummern aus diesem
Prompt, sondern verifiziere sie live, die Datei kann sich geändert haben.

Aufgabe: Erstelle `crates/memfuse-store/tests/chaos_bitflip_sstable.rs`.

Anforderungen:
1. Schreibe über die reale `LsmStorage`-API genug Einträge, dass mindestens
   ein Flush zu einer SSTable-Datei auf Disk stattfindet (prüfe die
   bestehende Flush-Schwelle/Konfiguration, z.B. via `LsmConfig`, wie es
   `flush_crash_simulation.rs` bereits tut — folge diesem Muster).
2. Mit `proptest` (bereits Dev-Dependency) generiere randomisierte
   Bit-Flip-Positionen und wende sie GEZIELT in drei separaten Testfällen an:
   a. Innerhalb eines Daten-**Blocks** (nicht im Block-CRC-Feld selbst).
   b. Innerhalb des **Bloom-Filter**-Bytebereichs.
   c. Innerhalb des **Index**-Bytebereichs.
   Nutze dafür direktes Byte-Patching der SSTable-Datei auf Disk (wie
   `inject_bit_flip` in `memfuse_chaos_test.rs` aus `TEST/` es vormacht —
   diese Hilfsfunktion darfst du als Vorlage für die Byte-Patch-Logik
   wiederverwenden, aber OHNE Chimera-Branding/Kommentare, direkt an
   MemFuse-Konventionen angepasst).
3. Nach der Korruption: Reopen der `LsmStorage` bzw. Lesezugriff auf den
   betroffenen Key.
4. Assertions (pro Fall unterschiedlich, weil die drei Bereiche
   unterschiedlich abgesichert sind):
   a. Block-Korruption → `MemFuseError::ChecksumMismatch` MUSS zurückkommen,
      NIEMALS ein stillschweigend falscher Wert.
   b. Bloom-Filter-Korruption → darf NIEMALS zu einem falschen "definitiv
      nicht vorhanden" für einen tatsächlich vorhandenen Key führen, der zu
      einem Datenverlust führt (False Negative ist bei Bloom-Filtern per
      Design ausgeschlossen — falls ein korrupter Bloom-Filter das doch
      verursacht, ist das ein valider, meldepflichtiger Fund, kein Testfehler
      — nicht "reparieren" durch Anpassen der Assertion).
   c. Index-Korruption → MUSS entweder einen dokumentierten Fehler werfen
      oder (falls der Index redundant aus dem Block selbst rekonstruierbar
      ist) korrekt degradieren — recherchiere das tatsächliche Verhalten im
      Code, erfinde keine Erwartung.
5. Ground-Truth: der ursprünglich geschriebene Wert MUSS unabhängig vom
   SSTable-Lesepfad vorgehalten werden (z.B. als `Vec<(Key, Value)>` vor dem
   Flush) — nicht aus der SSTable selbst zurückgelesen und dann verglichen.

Verifikation: `cargo test -p memfuse-store --test chaos_bitflip_sstable`.
Wenn ein Fall ein Verhalten aufdeckt, das nicht in `crates/memfuse-store/
AGENTS.md` dokumentiert ist (z.B. unklares Bloom-Filter-Korruptionsverhalten):
das explizit im PR-Beschreibungstext benennen, nicht stillschweigend nur den
Test grün machen.
```

### Prompt 5 — Echter Dropped-Write-Test (chmod-basiert)

```markdown
Kontext: Lies den Abschnitt "fsync Error Propagation (ABSOLUT)" in
`crates/memfuse-store/AGENTS.md` — das ist die Invariante, die dieser Test
beweisen soll. Lies `crates/memfuse-store/src/util.rs` (Atomic-Rename-Pattern)
zum Verständnis, WELCHE Datei/welches Verzeichnis du schreibgeschützt machen
musst, damit der Fehler an der richtigen Stelle auftritt.

Aufgabe: Erstelle `crates/memfuse-store/tests/chaos_dropped_write.rs`.

Anforderungen:
1. Öffne eine reale `LsmStorage` in einem `tempdir()`, committe einige
   Einträge erfolgreich.
2. Setze das WAL-Verzeichnis (oder die aktive WAL-Datei — recherchiere im
   Code, welches Ziel einen echten Fehler in `commit()`/`append()` erzeugt,
   ohne dass die Storage-Öffnung selbst schon vorher fehlschlägt) via
   Unix-Filesystem-Permissions (`std::fs::Permissions`,
   `PermissionsExt::from_mode(0o444)` unter `#[cfg(unix)]`) auf read-only.
   Für Windows: nutze das plattformspezifische Äquivalent oder markiere den
   Testfall mit `#[cfg(unix)]`, wenn ein plattformübergreifendes Äquivalent
   unverhältnismäßigen Aufwand bedeuten würde — dokumentiere diese
   Einschränkung im Testkommentar.
3. Versuche einen weiteren Commit. Erwartung: `Err(MemFuseError::Io(_))`
   (oder eine andere bereits existierende, passende Variante — verifiziere
   live in `crates/memfuse-core/src/error.rs`, welche Variante tatsächlich
   zurückkommt, erfinde keine neue).
4. Setze die Permissions zurück auf beschreibbar.
5. Assertions:
   a. Der fehlgeschlagene Commit MUSS `last_committed_tx` unverändert lassen
      (kein Fortschritt bei propagiertem Fehler).
   b. Nach Zurücksetzen der Permissions muss ein erneuter Commit desselben
      Inhalts erfolgreich sein.
   c. Alle vor dem Fehler erfolgreich committeten Einträge müssen weiterhin
      korrekt lesbar sein (kein Kollateralschaden durch den fehlgeschlagenen
      Schreibversuch).
6. `.unwrap()` ist in `#[cfg(test)]`-Code erlaubt (`rules/testing.md`), aber
   NICHT für den Fehlerfall selbst, den du gerade testest — dort MUSS explizit
   auf die erwartete `MemFuseError`-Variante gematcht werden, sonst ist der
   Test nicht aussagekräftig (Mutation-Survival-Check aus `rules/testing.md`
   anwenden: würde der Test auch fehlschlagen, wenn statt `Err` fälschlich
   `Ok` zurückkäme?).

Verifikation: `cargo test -p memfuse-store --test chaos_dropped_write`. Führe
den Test auch lokal als Nicht-Root-User aus — als Root werden Unix-Permission-
Restriktionen ignoriert, das würde den Test fälschlich grün erscheinen lassen.
```

### Prompt 6 — Memory-Pressure- & Concurrent-Flood-Test

```markdown
Kontext: Lies `crates/memfuse-core/src/types/budget.rs` VOLLSTÄNDIG
(`ResourceBudget`, `ResourceTracker`, `try_reserve`, `consume_memory`,
`has_memory_capacity`, `MemoryBudgetExceeded`). Lies außerdem in
`crates/memfuse-store/src/lsm.rs` live nach, WIE `LsmConfig` den
`ResourceBudget` aktuell konfigurierbar macht (Feldname kann sich geändert
haben) — und beachte den in §5 des Plans dokumentierten Beobachtungspunkt:
ein Fehler von `budget.consume_memory()` wird in `commit()` aktuell nur
geloggt (`tracing::warn!`), nicht propagiert. Das ist FÜR DIESEN TEST
maßgeblich — teste das tatsächliche Verhalten, nicht ein angenommenes.

Aufgabe: Erstelle `crates/memfuse-store/tests/chaos_memory_pressure.rs`.

Anforderungen:
1. Öffne eine `LsmStorage` mit einem `LsmConfig`, dessen `ResourceBudget`
   künstlich sehr klein ist (z.B. wenige hundert KB) — so klein, dass er
   nach wenigen Commits mit realistischen Payload-Größen sicher überschritten
   wird.
2. Teil A — Sequenziell: Committe in einer Schleife, bis ein Fehler auftritt
   oder ein Schwellwert an Iterationen erreicht ist. Assertions:
   a. Es MUSS irgendwann ein Fehlerpfad ausgelöst werden, der auf
      Speicherdruck zurückzuführen ist (verifiziere anhand des tatsächlichen
      Rückgabewerts/Loggings — je nach Ergebnis der Recherche oben entweder
      ein propagierter `MemFuseError::MemoryBudgetExceeded`/`Storage(...)`
      ODER, falls bestätigt "nur geloggt, nicht propagiert": ein
      `tracing`-Subscriber-basierter Test, der das Auftreten der Warnung
      nachweist. Wähle die Variante basierend auf dem TATSÄCHLICHEN Verhalten,
      nicht basierend auf einer Wunschvorstellung.
   b. Der Prozess darf zu keinem Zeitpunkt real out-of-memory gehen — nutze
      denselben RSS-Messansatz wie `resource_leaks.rs`
      (`get_rss_bytes()`-Hilfsfunktion, ggf. dorthin auslagern in ein
      gemeinsames `tests/support/mod.rs`, falls noch nicht vorhanden — prüfe
      erst, ob es das schon gibt, bevor du dupliziert).
3. Teil B — `ConcurrentWriteFlood` (Ersatz für das entfallene
   `RogueAgentFlood`-Szenario): Spawne N konkurrierende Tasks (z.B. 20), die
   gleichzeitig gegen dasselbe knapp bemessene Budget schreiben. Assertions:
   a. Kein Deadlock (Test MUSS mit Timeout laufen, z.B.
      `tokio::time::timeout`).
   b. Alle Tasks, die einen Fehler zurückbekommen, dürfen dadurch KEINEN
      inkonsistenten Zustand für andere, erfolgreiche Tasks verursachen —
      prüfe das über die Ground-Truth-Liste erfolgreich committeter Keys wie
      in `chaos_task_massacre.rs`.
   c. Nach Abschluss aller Tasks: `LsmStorage`-Reopen liefert exakt den
      Datenstand der tatsächlich erfolgreichen Commits, kein mehr, kein
      weniger.

Verifikation: `cargo test -p memfuse-store --test chaos_memory_pressure`.
Falls Teil A den in der Kontext-Notiz beschriebenen Soft-Fail (nur Logging,
keine Propagation) tatsächlich bestätigt: dokumentiere das explizit als
Finding im PR-Text mit Verweis auf `lsm.rs` (Zeile live ermitteln) — NICHT
stillschweigend nur den Test daran anpassen, ohne es zu erwähnen. Das ist
laut `.jules/AUDIT_INTAKE_PROTOCOL.md` meldepflichtig.
```

### Prompt 7 — Fault-Matrix + CI-Integration

```markdown
Kontext: Lies `justfile` vollständig (bestehende Recipe-Konventionen,
insbesondere `triple-test` und den nix-Fallback-Mechanismus aus `AGENTS.md`
§2). Lies einen bestehenden `.github/workflows/*.yml` als Stilvorbild für
Job-Struktur/Caching. Voraussetzung: Prompts 2–6 sind bereits gemergt.

Aufgabe:
1. Erstelle `crates/memfuse-store/tests/chaos_matrix.rs`:
   - `#[ignore]`-gated Test(s), die mehrere der in Prompt 2–6 gebauten
     Szenarien in randomisierter Reihenfolge und mit randomisiertem,
     GELOGGTEM Seed kombinieren (z.B. `TaskMassacre` gleichzeitig mit
     `MemoryExhaustion`, oder `BitFlipInjection` direkt gefolgt von einem
     `PowerCutSimulation`-Zyklus auf demselben Storage-Verzeichnis).
   - Der verwendete Seed MUSS beim Testlauf via `println!`/`tracing::info!`
     ausgegeben werden, damit ein Fehlschlag reproduzierbar ist (kein
     "flaky and unreproducible" akzeptieren).
   - Jede Kombination MUSS dieselben Ground-Truth-Disziplinen einhalten wie
     die Einzeltests (kein Test-Mirroring).
2. Füge in `justfile` eine neue Recipe hinzu, konsistent mit dem
   nix-Fallback-Muster der bestehenden Recipes:
   ```
   chaos-test:
       nix develop -c cargo test -p memfuse-store --test chaos_matrix -- --ignored --test-threads=1 || \
       cargo test -p memfuse-store --test chaos_matrix -- --ignored --test-threads=1
   ```
   (Passe an bestehenden Stil an, falls der tatsächliche Fallback-Mechanismus
   in `justfile` anders aussieht als hier angenommen — live prüfen, nicht
   blind übernehmen.)
3. Erstelle `.github/workflows/chaos.yml`:
   - `on: schedule` (z.B. täglich nachts) + `workflow_dispatch` für manuelles
     Triggern. AUSDRÜCKLICH NICHT `on: pull_request` — dieser Workflow darf
     keine PRs blockieren (siehe ADR aus Prompt 0).
   - Job ruft `just chaos-test` auf.
   - Bei Fehlschlag: Issue-Erstellung oder zumindest deutliche Job-Failure-
     Markierung, damit es nicht unbemerkt bleibt, obwohl es keine PRs
     blockiert.
4. Aktualisiere `rules/chaos_testing.md` (aus Prompt 0) um einen Abschnitt
   "CI-Ausführung", der auf `just chaos-test` und den nightly-Workflow
   verweist.
5. `just sync-docs` ausführen, danach `just sync-docs-check` — muss grün
   sein, bevor der PR als vollständig gilt.

Nicht tun: Diesen Workflow nicht in bestehende Pre-Commit- oder PR-Gates
einhängen. Keine neue Dependency für Issue-Erstellung hinzufügen, wenn dafür
bereits GitHub-Actions-Bordmittel (`actions/github-script` o.ä.) ausreichen —
falls unklar, lieber ohne automatische Issue-Erstellung starten und das als
offenen Punkt im PR benennen, statt eine neue Action-Dependency ungefragt
einzuführen.
```

---

## 8. Definition of Done

Diese Migration gilt erst als abgeschlossen, wenn:

- [ ] `TEST/hebel_1..4,6,7*` vollständig gelöscht (laut Nutzerangabe bereits erfolgt).
- [ ] `TEST/MASTER_INTEGRATION_PLAN.md` durch dieses Dokument ersetzt, `TEST/README.md` entsprechend gekürzt.
- [ ] Neuer ADR-Eintrag in `DECISIONS.md` mit live ermittelter Nummer.
- [ ] `rules/chaos_testing.md` existiert und ist in `AGENTS.md` §7 verlinkt.
- [ ] Alle 6 neuen Testdateien + `examples/chaos_writer.rs` grün, `just triple-test` bestanden.
- [ ] `just chaos-test`-Recipe + `.github/workflows/chaos.yml` vorhanden, nicht PR-blockierend.
- [ ] Kein Diff in `crates/memfuse-store/src/**`, `crates/memfuse-core/src/**` oder einem anderen Produktions-Modul — es sei denn, ein Finding aus §5/Prompt 6 hat eine EIGENE, separate ADR + eigenen PR ausgelöst.
- [ ] `just sync-docs-check` grün.
