# MemFuse — Die Jules-Perfektions-Strategie

**Repository:** `https://github.com/tfufuz1/memfuse`
**Analysiert:** frischer Klon, HEAD nach Merge von K-1/K-2/M-4 (Cargo.lock, Tauri-CI, Router-DAG-Fix — siehe ADR-045)
**Zweck:** Nicht die Bugs selbst reparieren, sondern das **System reparieren, das die Bugs erzeugt und
übersieht** — Google Jules' Governance-Umgebung (`AGENTS.md`, `justfile`, CI-Gates, `.jules/*`) so
härten, dass architektonische Schulden strukturell nicht mehr entstehen können, statt sie nachträglich
per Audit-Dokument einzusammeln.

---

## Teil A — Bestandsaufnahme: Was bereits exzellent ist (NICHT anfassen)

Bevor ich Änderungen vorschlage, halte ich fest, was dein aktuelles System bereits leistet — das ist
für ein Solo-Entwickler-Projekt außergewöhnlich ausgereift und sollte als Fundament respektiert werden:

- **`AGENTS.md` v6.0** mit klaren Judgment-Boundaries (ALWAYS/ASK/NEVER), Session-Protokoll, AI-TAG-System
- **`.jules/AUDIT_INTAKE_PROTOCOL.md`** — verhindert genau das Problem, das du in deinem letzten Workflow
  hattest (blinde Übernahme veralteter Audit-Findings). Das ist bereits die richtige Antwort auf
  "Audit-Dokument → Prompt → Jules löst es", nur wird es noch nicht konsequent genug erzwungen (siehe Teil B).
- **`.jules/COMMON_LLM_ERRORS.md`** — ein hervorragend konkretes Halluzinations-Gedächtnis
- **`context-gates.yml`** mit 9 automatisierten Gates (kritische AI-TAGs, Unwrap-Baseline-Diff,
  Silent-IO, ADR-010-Enforcement, TODO-Grammatik, TS/SESSION-Pflichtfelder, Doku-Drift, Review-Coverage,
  Konsistenzprüfung) — das ist bereits genau das "Architektur-Linter"-Konzept, das ich sonst empfehlen würde
- **`dag-check.yml`** — vollständige, automatisierte DAG-Grenzen-Prüfung pro Crate, inkl. Tracking bekannter
  Ausnahmen (DAG-002, DAG-003) statt sie zu verstecken
- **ADR-045** zeigt: der DAG-Fix (Router↔MCP) aus meiner vorherigen Analyse wurde bereits sauber und mit
  korrekter Dokumentation umgesetzt
- **`rules/*.md`** (SIMD-Safety, WAL-Crypto, Async-IO, Test-Mirroring-Erkennung) — Domain-Wissen ist
  bereits externalisiert statt nur im Prompt-Verlauf verloren zu gehen
- **`rules/detect_nested_locks.yml`** via `ast-grep` — echte AST-Analyse statt reinem Grep/Regex

**Das bedeutet:** Deine Prompts an Jules müssen dieses System nicht neu erfinden, sondern **Lücken darin
schließen** und **tote/kaputte Teile davon reparieren**, die aktuell nur so aussehen, als würden sie greifen.

---

## Teil B — Verifizierte, konkrete Lücken (gegen aktuellen Code geprüft)

| ID | Schwere | Befund | Beleg |
|---|---|---|---|
| **G-1** | Kritisch | `just check`, `just test`, `just dag-check`, `just triple-test` sowie alle `just check-*`-Targets rufen unconditional `nix develop -c ...` auf — **ohne Fallback**. Jules' VM hat laut eigener Diagnose **kein `nix`** installiert (weder im `PATH` noch im Setup-Skript). Jede dieser Aufrufe schlägt mit `nix: command not found` fehl. | `justfile` Zeilen 8–17, 82–150; `.jules/setup/environment_script.sh` (kein `nix`); Environment-Diagnostics (kein `nix` in Tool-Tabelle) |
| **G-2** | Kritisch | Direkte Konsequenz von G-1: `AGENTS.md §2` dokumentiert `just check` als "Lint + format" und `just dag-check` als "DAG enforcement" — Jules kann **beide dokumentierten Golden-Path-Befehle nicht ausführen**. `SESSION_BOOTSTRAP.md Phase 5` weicht dem bereits (unbewusst?) aus, indem es rohe `cargo`-Befehle nutzt statt `just`-Targets — es ruft aber `dag-check` und `debt-audit` **gar nicht auf**. | `AGENTS.md` §2, §6; `.jules/SESSION_BOOTSTRAP.md` Phase 5 |
| **G-3** | Kritisch | `just debt-audit` (Unwrap-Scan außerhalb Baseline, Unsafe-Scope-Audit, `std::fs`-Scan, AST-Lock-Analyse via `ast-grep`, `cargo-audit`) ist in **keinem** CI-Workflow verdrahtet. Es existiert nur als **unverifizierte Checkbox** im PR-Template. Niemand/nichts erzwingt, dass Jules diese Checkbox ehrlich ausfüllt. | `.github/pull_request_template.md` Zeile 25; keine Referenz auf `debt-audit` in `.github/workflows/*.yml` |
| **G-4** | Wichtig | `justfile` referenziert `cargo xtask context-tags {{ARGS}}` — dieses xtask-Subcommand **existiert nicht**. `xtask/src/main.rs` kennt nur `sync-docs`, `check-review-coverage`, `check-consistency`, `run-community-detection`. Der Aufruf endet in `Unknown xtask command`. | `justfile` Zeile ~43; `xtask/src/main.rs` `match subcommand` |
| **G-5** | Wichtig | `just spec NAME` kopiert `docs/specs/TEMPLATE_MICRO_SPEC.md` — diese Datei **existiert nicht im Repository**. Jeder Versuch, per Spec-Driven-Development einen neuen Workpackage zu starten, schlägt fehl. | `justfile` `spec`-Rezept; `docs/specs/` enthält keine `TEMPLATE_MICRO_SPEC.md` |
| **G-6** | Wichtig | `.jules/JULES_CONTEXT.md` ist ein manuell datiertes Ambient-Snapshot-Dokument ("Stand: 2026-08-29") ohne jede automatisierte Freshness-Prüfung. Es nennt z. B. nicht ADR-045 (die neueste ADR) — es driftet bereits jetzt vom tatsächlichen Stand ab, obwohl es als "immer geladener" Kontext für jede Session dient. | `.jules/JULES_CONTEXT.md` Header vs. `DECISIONS.md` (46 ADRs, neueste: ADR-045) |
| **G-7** | Strategisch | Keine Fault-Injection-/Concurrency-Test-Infrastruktur (`loom`) trotz mehrerer projektkritischer Lock-Primitive (`commit_mutex` in `memfuse-store/src/lsm.rs`, `insert_lock`, `quantizer: RwLock` in `memfuse-index/src/hnsw.rs`). `proptest` ist in 9 Crates vorhanden (gut), aber Nebenläufigkeits-Modelle werden nirgends exhaustiv geprüft. Genau diese Lücke hat NEU-02 (`try_write()`-Race in der SQ8-Bound-Expansion) unentdeckt gelassen. | `grep -rl loom **/Cargo.toml` → 0 Treffer; `memfuse-index/src/hnsw.rs` `do_insert()` |
| **G-8** | Strategisch | Keine Mutation-Testing-Automatisierung (`cargo-mutants`), obwohl `rules/testing.md` bereits das **Prinzip** "Mutation Survival Check" manuell vorschreibt ("Wenn ich `<` zu `<=` ändere, schlägt dann ein Test fehl?"). Das Prinzip existiert nur als Text, nicht als erzwungenes Gate. | `rules/testing.md` Abschnitt "Mutation Survival Check"; keine `cargo-mutants`-Referenz im Repo |
| **G-9** | Strategisch | Kein einziger `schedule:`-getriggerter GitHub-Actions-Workflow existiert. Die von dir (bzw. deinem externen Berater) gewünschte "proaktive, asynchrone Audit-Nutzung von Jules" (Freitagabend-Cron-Job) ist aktuell nicht automatisiert — jeder Audit läuft nur reaktiv, wenn du manuell ein Dokument wie `memfuse_neue_befunde.md` erstellst und einfügst. | `grep -l "schedule:" .github/workflows/*.yml` → 0 Treffer |
| **G-10** | Strategisch | Keine standardisierte "Sperrzonen"-Vorlage für Feature-Prompts existiert im Repo (nur in deinem externen P01–P16-Workflow, nicht versioniert/wiederverwendbar). Kein CI-Gate verifiziert, dass ein PR nur Dateien innerhalb der im PR-Body deklarierten Scope-Zone ändert. | Keine `docs/specs/TASK_PROMPT_TEMPLATE.md` o. ä.; PR-Template hat kein Scope-Feld |
| **G-11** | Strategisch | `rust-ci.yml` und `context-gates.yml` laufen ausschließlich auf `ubuntu-latest`. Windows/macOS werden **nur** im Release-Workflow (`tauri-release.yml`, Trigger: `tags: ["v*"]`) gebaut — also erst beim eigentlichen Release, nicht während der PR-Entwicklung. Cross-Platform-Regressionen (z. B. Windows-ACL-Handling für den WAL-Integritätsschlüssel) werden dadurch erst extrem spät sichtbar. | `.github/workflows/rust-ci.yml` (kein Matrix-Build); `.github/workflows/tauri-release.yml` Trigger |

**Kernaussage:** G-1/G-2/G-3 sind der eigentliche Grund, warum architektonische Schulden bei dir
"durchrutschen", obwohl das Regelwerk auf dem Papier vollständig aussieht — **die wichtigsten Gates
(`just check`, `just dag-check`, `just debt-audit`) laufen faktisch nie**, weder automatisch (nicht in
CI verdrahtet) noch manuell (nix fehlt in der Jules-VM). Jules folgt AGENTS.md's Session-Protokoll so gut
es kann, stößt aber bei jedem `just check`/`just dag-check`-Aufruf auf einen Fehler, den es entweder
stillschweigend überspringt oder durch rohe `cargo`-Befehle kompensiert — wodurch `debt-audit`
(Unsafe-Scope, Nested-Locks, `cargo-audit`) **nie** ausgeführt wird, weder lokal noch in CI.

---

## Teil C — Die Perfektions-Strategie (Meta-Prinzipien)

Bevor die einzelnen Prompts kommen, drei Regeln, wie du sie einsetzt:

### 1. Reihenfolge ist nicht optional
G-1/G-2/G-3 zuerst. Alles andere (Fault-Injection, Mutation-Testing, Cron-Audits) ist wertlos, solange
die Basis-Gates nicht mal ausführbar sind — ein Mutation-Test, der nie in CI läuft, ist nur Show.

### 2. Jeder Prompt bekommt seine eigene Sperrzone
Genau wie dein externer Berater es empfohlen hat, aber jetzt **auf die Governance-Infrastruktur selbst
angewendet**: Da mehrere Prompts `justfile` und `.github/workflows/*.yml` anfassen, bekommt jeder Prompt
unten einen expliziten `Erlaubte Dateien`/`Sperrzonen`-Block. Führe sie **sequenziell** aus (nicht parallel),
da G-1, G-3, G-4, G-5 alle `justfile` anfassen — parallele Jules-Sessions auf derselben Datei erzeugen
Merge-Konflikte.

### 3. Der letzte Prompt (P-J10) ist das eigentliche Ziel
P-J10 erzeugt eine wiederverwendbare **Task-Prompt-Vorlage mit Sperrzonen-Feld**, die du ab sofort für
*jede* zukünftige Feature-Anforderung an Jules nutzt. Das ist die Antwort auf deine ursprüngliche Frage
("wie nutze ich seine Umgebung jeweils") — nicht neue Prompts pro Bug, sondern eine Vorlage, die
Scope Creep strukturell verhindert, bevor er entsteht.

---

## Teil D — Die Prompts

Reihenfolge = Ausführungsreihenfolge. Jeder Prompt ist ein eigenständiger Jules-Task/eine eigene Session.

### Inhaltsverzeichnis

1. [P-J1: Nix-Fallback in justfile — Jules-Kompatibilität wiederherstellen](#p-j1-nix-fallback-in-justfile--jules-kompatibilität-wiederherstellen)
2. [P-J2: `just debt-audit` als Pflicht-CI-Gate verdrahten](#p-j2-just-debt-audit-als-pflicht-ci-gate-verdrahten)
3. [P-J3: Kaputte Governance-Tools reparieren](#p-j3-kaputte-governance-tools-reparieren-context-tags-template_micro_specmd)
4. [P-J4: Automatisiertes Freshness-Gate für JULES_CONTEXT.md](#p-j4-automatisiertes-freshness-gate-für-jules_contextmd)
5. [P-J5: Concurrency-Fault-Injection-Harness mit `loom`](#p-j5-concurrency-fault-injection-harness-mit-loom)
6. [P-J6: Mutation-Testing mit `cargo-mutants` automatisieren](#p-j6-mutation-testing-mit-cargo-mutants-automatisieren)
7. [P-J7: Scheduled Proaktiv-Audit-Workflow (Cron) einrichten](#p-j7-scheduled-proaktiv-audit-workflow-cron-einrichten)
8. [P-J8: Windows/macOS-Testlane in die reguläre PR-CI aufnehmen](#p-j8-windowsmacos-testlane-in-die-reguläre-pr-ci-aufnehmen)
9. [P-J9: NEU-01/NEU-02 fixen — als Referenzfall für das neue Fault-Injection-Gate](#p-j9-neu-01neu-02-fixen--als-referenzfall-für-das-neue-fault-injection-gate)
10. [P-J10: Die Sperrzonen-Task-Prompt-Vorlage (Ziel-Deliverable)](#p-j10-die-sperrzonen-task-prompt-vorlage-ziel-deliverable)

---

### P-J1: Nix-Fallback in justfile — Jules-Kompatibilität wiederherstellen

```md
# Persona
Du bist ein Build-Tooling-Engineer mit Erfahrung in portablen Task-Runnern (`just`, `make`) über
heterogene Entwicklungsumgebungen hinweg (lokale Nix-Shell vs. CI-Runner vs. autonome Cloud-VM ohne Nix).

# Kontext
Datei: `justfile` (Workspace-Root)

Verifizierter Ist-Zustand: Die Rezepte `check`, `test`, `check-core`, `check-store`, `check-index`,
`check-db`, `check-text`, `check-py`, `check-tauri`, `check-embed`, `dag-check` und `triple-test`
(welches von `check` abhängt) rufen ausschließlich `nix develop -c <befehl>` auf — ohne Fallback,
falls `nix` nicht im `PATH` verfügbar ist:

    check:
        nix develop -c cargo fmt --all -- --check
        nix develop -c cargo clippy --all-targets -- -D warnings
        nix develop -c cargo check --all-targets --workspace

Bereits vorhandene, korrekte Referenzimplementierung im selben `justfile` (Fallback-Pattern, das
funktioniert):

    sync-docs:
        nix develop -c cargo xtask sync-docs || cargo xtask sync-docs

Diese Umgebung, in der die meisten Sitzungen laufen — die Google-Jules-VM — hat kein `nix`
installiert (kein `nix`-Eintrag im `PATH`, kein `nix` im dokumentierten CLI-Tool-Inventar der VM,
das Setup-Skript `.jules/setup/environment_script.sh` installiert kein Nix). Das bedeutet: jedes
`nix develop -c ...`-Rezept schlägt in Jules-Sessions mit `nix: command not found` fehl. Damit sind
ausgerechnet die beiden in `AGENTS.md §2` als Golden-Path dokumentierten Befehle
(`just check` für Lint+Format, `just dag-check` für DAG-Enforcement) für Jules faktisch unbenutzbar.

# Aufgabe
1. Ändere jedes Rezept im `justfile`, das aktuell unconditional `nix develop -c <befehl>` aufruft,
   auf das bereits im Repo etablierte Fallback-Pattern `nix develop -c <befehl> || <befehl>`. Betrifft
   mindestens: `check`, `test`, `check-core`, `check-store`, `check-index`, `check-db`, `check-text`,
   `check-py`, `check-tauri`, `check-embed`, `dag-check`, `triple-test` (bzw. dessen internen
   `cargo test`-Aufruf).
   Beispiel für `check`:

       check:
           nix develop -c cargo fmt --all -- --check || cargo fmt --all -- --check
           nix develop -c cargo clippy --all-targets -- -D warnings || cargo clippy --all-targets -- -D warnings
           nix develop -c cargo check --all-targets --workspace || cargo check --all-targets --workspace

2. Achte darauf, dass bei `dag-check` (ein Bash-Rezept mit `#!/usr/bin/env bash` Shebang und mehreren
   `cargo tree`-Aufrufen) das Fallback-Pattern konsistent auf jeden einzelnen `cargo`-Unteraufruf
   angewendet wird, nicht nur auf den ersten — oder alternativ: füge am Rezept-Anfang eine einmalige
   Umgebungs-Erkennung ein, die eine Shell-Variable setzt:

       #!/usr/bin/env bash
       set -euo pipefail
       if command -v nix &> /dev/null && nix develop -c true &> /dev/null; then
           RUNNER="nix develop -c"
       else
           RUNNER=""
       fi
       echo "=== DAG Integrity Check (Runner: ${RUNNER:-cargo direkt}) ==="
       $RUNNER cargo tree -p memfuse-core --edges no-dev | ...

   Wähle das zweite Muster (einmalige Erkennung + `$RUNNER`-Variable) für `dag-check`, `check` und
   `triple-test`, da diese mehrere Unteraufrufe haben — das ist wartbarer als N-fache `||`-Verkettung.
3. Verifiziere, dass das Verhalten in einer Umgebung MIT `nix` (falls du selbst lokal mit Nix
   entwickelst) unverändert bleibt — `nix develop -c` muss weiterhin bevorzugt versucht werden, der
   Fallback greift nur bei fehlendem `nix`-Binary oder fehlgeschlagenem `nix develop -c true`-Probe.
4. Aktualisiere `AGENTS.md §2` (Toolchain-Tabelle) NICHT inhaltlich (die Befehle bleiben `just check`
   etc.), ergänze aber eine Fußnote unterhalb der Tabelle:

   > **Hinweis:** Alle `just`-Rezepte funktionieren sowohl mit als auch ohne installiertes `nix` —
   > bei fehlendem `nix` wird automatisch auf direkte `cargo`-Aufrufe zurückgefallen.

5. Aktualisiere `.jules/SESSION_BOOTSTRAP.md` Phase 5 ("Session-Ende"), sodass sie wieder die
   `just`-Rezepte referenziert statt der aktuell dort hart codierten rohen `cargo`-Befehle — jetzt, wo
   `just check`/`just dag-check` garantiert funktionieren, soll die Bootstrap-Checkliste konsistent mit
   `AGENTS.md §6` sein und explizit auch `just dag-check` und `just debt-audit` als Pflichtschritte vor
   dem letzten Commit auflisten (Debt-Audit-Verdrahtung selbst ist Aufgabe von P-J2 — hier nur den
   Aufruf in die Checkliste aufnehmen, das Rezept existiert bereits).

# Akzeptanzkriterien
- `nix --version` in dieser Umgebung liefert "command not found" (verifiziere das zuerst, um zu
  bestätigen, dass du in einer Nix-losen Umgebung testest — falls doch Nix verfügbar ist, entferne es
  testweise aus dem `PATH` für den Verifikationsschritt, z. B. via `PATH=/usr/bin:/bin just check`).
- `just check`, `just dag-check`, `just test`, `just triple-test` laufen in dieser Nix-losen Umgebung
  erfolgreich durch (Exit-Code 0 bei sauberem Code) statt mit `nix: command not found` zu scheitern.
- Bestehendes Verhalten mit `nix` (falls testbar) bleibt unverändert.
- `.jules/SESSION_BOOTSTRAP.md` Phase 5 nutzt konsistent `just`-Rezepte statt roher `cargo`-Befehle
  und listet `just dag-check` sowie `just debt-audit` explizit auf.

# Verifikation
    PATH=/usr/bin:/bin:/usr/local/bin just check
    echo "Exit-Code: $?"
    PATH=/usr/bin:/bin:/usr/local/bin just dag-check
    echo "Exit-Code: $?"

Beide Exit-Codes müssen `0` sein (bzw. der reguläre Fehler-Exit-Code, falls der Code selbst einen
Clippy-/DAG-Verstoß enthält — entscheidend ist, dass der Fehler von `cargo`/`clippy` kommt, nicht von
`nix: command not found`).
```

---

### P-J2: `just debt-audit` als Pflicht-CI-Gate verdrahten

```md
# Persona
Du bist ein CI/CD-Engineer mit Fokus auf "Shift-Left"-Sicherheits- und Qualitäts-Gates — du sorgst
dafür, dass bereits vorhandene lokale Prüf-Skripte nicht nur auf dem Papier existieren, sondern bei
jedem PR zwingend und automatisiert durchlaufen.

# Kontext
Voraussetzung: P-J1 muss bereits gemerged sein (sonst schlägt `just debt-audit` in der CI ggf. am
Nix-Problem fehl, falls du es versehentlich als `just`-Aufruf statt Bash-Skript einbindest — prüfe das
im ersten Schritt).

Datei: `justfile`, Rezept `debt-audit` (Zeile ~166) — vollständig funktionsfähiges Bash-Skript, das
bereits vier Kategorien prüft:
1. `.unwrap()` außerhalb von Test-Code
2. `unsafe` außerhalb von `distance.rs`
3. `std::fs::` in Produktionscode (Soft-Warning, kein Hard-Fail)
4. Verschachtelte Locks via `ast-grep` (`rules/detect_nested_locks.yml`)
5. `cargo audit` (Dependency-Sicherheitslücken)

Aktuell wird dieses Rezept nirgends in `.github/workflows/*.yml` aufgerufen. Es existiert nur als
unverifizierte Checkbox in `.github/pull_request_template.md` Zeile 25
(`- [ ] just debt-audit grün (zero-unwrap, zero-unsafe, zero-std::fs)`), die von Jules oder dir manuell
angehakt werden müsste, ohne dass irgendetwas die Wahrheit dieser Aussage prüft.

Wichtig: Teile dieser Prüfung überschneiden sich bereits mit `context-gates.yml` (Gate 2 prüft
`.unwrap()`/`.expect()` gegen `.unwrap-baseline.txt`) — dedupliziere NICHT die bestehende
Baseline-Logik in `context-gates.yml`, sondern ergänze die dort fehlenden Prüfungen (Unsafe-Scope,
Nested-Locks, `cargo audit`), um Redundanz zu vermeiden.

# Aufgabe
1. Prüfe, ob `ast-grep` (`sg`) im CI-Runner-Image (`ubuntu-latest`) standardmäßig verfügbar ist. Falls
   nicht: ergänze einen Installationsschritt (`cargo install ast-grep --locked` oder via
   `curl`/Release-Binary — bevorzuge die schnellere Variante, prüfe `rules/dependency_audit.md` für
   das Prozedere bei neuen Tool-Abhängigkeiten in CI, das ist kein Cargo-Dependency-Zusatz im Projekt
   selbst, aber die gleiche Sorgfaltspflicht gilt).
2. Erstelle einen neuen Job `debt-audit` in `.github/workflows/context-gates.yml` (nicht in
   `rust-ci.yml`, da `context-gates.yml` bereits die kanonische Quelle für Governance-Gates ist,
   siehe Kommentar am Ende von `rust-ci.yml`):

       debt-audit:
         name: "Tech-Debt Audit (unwrap/unsafe/locks/cargo-audit)"
         runs-on: ubuntu-latest
         steps:
           - uses: actions/checkout@v4
           - uses: dtolnay/rust-toolchain@stable
           - uses: Swatinem/rust-cache@v2
           - name: Install ast-grep
             run: cargo install ast-grep --locked || true
           - name: Install cargo-audit
             run: cargo install cargo-audit --locked || true
           - name: Install just
             run: cargo install just --locked || true
           - name: Run debt-audit
             run: just debt-audit

3. Prüfe, ob `just debt-audit` in seiner aktuellen Form nur `unsafe`-Vorkommen außerhalb von
   `distance.rs` moniert — laut `AGENTS.md §4` sind aber auch `diskann.rs` und `persistence.rs`
   erlaubte `unsafe`-Zonen (und ein Test-only-Fall in `anti_tamper.rs`). Falls das `debt-audit`-Rezept
   diese beiden zusätzlichen Dateien noch nicht in seinem Grep-Ausschluss berücksichtigt (aktueller
   Ausschluss: nur `crates/memfuse-index/src/distance\.rs`), erweitere den Grep-Filter im `justfile`
   entsprechend, damit das Gate nicht false-positive auf bereits genehmigten `unsafe`-Code anschlägt:

       UNSAFE=$(grep -rn "unsafe " crates/ --include="*.rs" \
           | grep -v "crates/memfuse-index/src/distance\.rs" \
           | grep -v "crates/memfuse-index/src/diskann\.rs" \
           | grep -v "crates/memfuse-index/src/persistence\.rs" \
           | grep -v "crates/memfuse-crypto/src/anti_tamper\.rs" \
           | grep -v "#\[allow(unsafe_code)\]" \
           | grep -v "//.*unsafe" \
           || true)

   Verifiziere die exakten Dateinamen zuerst per `find crates/ -name "anti_tamper.rs" -o -name "diskann.rs" -o -name "persistence.rs"`,
   bevor du den Filter änderst — nutze die tatsächlichen Pfade, nicht die hier geschriebenen als Annahme.
4. Aktualisiere `.github/pull_request_template.md`: ersetze die manuelle Checkbox
   `- [ ] just debt-audit grün (...)` durch einen Hinweistext, dass dies jetzt automatisch von CI
   geprüft wird, z. B.:

       ## Debt-Audit
       > Wird automatisch von der CI (`debt-audit`-Job in `context-gates.yml`) erzwungen — keine manuelle
       > Checkbox mehr nötig.

5. Mache den `cargo audit`-Schritt innerhalb von `debt-audit` NICHT hart fehlschlagend gegenüber
   bereits bekannten, akzeptierten Advisories (falls es welche gibt) — prüfe zuerst mit `cargo audit`,
   ob es aktuell Findings gibt. Falls ja: entweder beheben (falls trivial) oder in einer
   `.cargo/audit.toml`-Ignore-Liste mit Begründungskommentar festhalten, statt das neue Gate von Tag 1
   an rot zu lassen.

# Akzeptanzkriterien
- Ein neuer, benannter CI-Job `debt-audit` läuft bei jedem `push`/`pull_request` automatisch.
- `just debt-audit` läuft in CI grün (Exit-Code 0) auf dem aktuellen `main`-Branch-Stand, oder alle
  gefundenen Verstöße wurden im selben PR behoben bzw. dokumentiert begründet.
- Der `unsafe`-Scope-Filter im `debt-audit`-Rezept deckt alle laut `AGENTS.md §4` genehmigten
  Ausnahmen ab (keine False Positives auf bereits abgesegnetem `unsafe`-Code).
- PR-Template verweist auf das automatische Gate statt auf eine manuelle Checkbox.

# Verifikation
Öffne einen Test-PR (oder nutze den PR dieser Aufgabe selbst) und verifiziere im GitHub-Actions-Log,
dass der `debt-audit`-Job erscheint, alle 5 Unterprüfungen durchläuft und mit Exit-Code 0 endet.
```

---

### P-J3: Kaputte Governance-Tools reparieren (context-tags, TEMPLATE_MICRO_SPEC.md)

```md
# Persona
Du bist ein Developer-Experience-Engineer, der dafür sorgt, dass interne Tooling-Kommandos, die im
`justfile` dokumentiert sind, auch tatsächlich funktionieren — kaputte Tooling-Referenzen sind
besonders für einen autonomen Agenten wie Jules gefährlich, da er ihnen ohne Rückfragemöglichkeit
vertraut und bei einem Fehlschlag Zeit in einer Sackgasse verliert.

# Kontext
Zwei unabhängige, verifizierte Lücken:

Lücke A — `just context-tags`:

    # Zeigt alle Context-Tags als NDJSON (filterbar nach Crate, Severity, Status)
    context-tags *ARGS:
        cargo xtask context-tags {{ARGS}}

`xtask/src/main.rs` implementiert im `match subcommand { ... }`-Block ausschließlich `"sync-docs"`,
`"check-review-coverage"`, `"check-consistency"`, `"run-community-detection"`. Es gibt keinen
`"context-tags"`-Arm. Jeder Aufruf von `just context-tags` (oder `cargo xtask context-tags`) endet im
`other => { eprintln!("Unknown xtask command: {}", other); ... process::exit(1); }`-Zweig.

Lücke B — `just spec NAME`:

    spec NAME:
        #!/usr/bin/env bash
        set -euo pipefail
        TIMESTAMP=$(date +%Y%m%d)
        TARGET="docs/specs/SPEC-${TIMESTAMP}-{{NAME}}.md"
        mkdir -p docs/specs
        cp docs/specs/TEMPLATE_MICRO_SPEC.md "$TARGET"
        ...

`docs/specs/TEMPLATE_MICRO_SPEC.md` existiert nicht im Repository. `cp` schlägt fehl
(`No such file or directory`), das Rezept bricht ab.

# Aufgabe

## Teil 1 — `context-tags` implementieren oder entfernen
1. Entscheide anhand des restlichen Kontextsystems (`AI-TAG`, `ANCHOR`, `REVIEW-PASS`-Kommentare mit
   `TS:`/`SESSION:`-Feldern, siehe `rules/tag_taxonomy.md`), ob ein NDJSON-Export dieser Tags
   tatsächlichen Mehrwert hätte (z. B. für externe Dashboards, oder um `session-context` im `justfile`
   zu ersetzen, das aktuell ähnliche Grep-Logik dupliziert).
2. Falls Implementierung sinnvoll: Implementiere in `xtask/src/main.rs` einen neuen Match-Arm
   `"context-tags"`, der:
   - alle `.rs`-Dateien unter `crates/` nach `AI-TAG[...]`, `ANCHOR[...]` und `REVIEW-PASS`-Kommentaren
     durchsucht (nutze die bereits vorhandene `scan_tags()`-Funktion aus `xtask/src/main.rs`, die auch
     von `check-review-coverage` genutzt wird — keine Parsing-Logik duplizieren),
   - jede Zeile als NDJSON-Objekt mit Feldern `crate`, `file`, `line`, `tag_type`, `domain`, `severity`,
     `id`, `status`, `ts`, `session` ausgibt,
   - optionale Filter-Argumente `--crate=<name>`, `--severity=<LEVEL>`, `--status=<RESOLVED|OPEN>`
     unterstützt (passend zum in `justfile` dokumentierten `*ARGS`-Verhalten).
   - Ergänze mindestens 2 Unit-Tests in `xtask` (Filterung nach Crate, Filterung nach Severity).
   Falls kein klarer Mehrwert gegenüber dem bereits existierenden `just session-context`-Rezept
   erkennbar ist: Entferne das `context-tags`-Rezept ersatzlos aus dem `justfile`, statt eine
   Attrappe zu implementieren — dokumentiere die Entscheidung in der Commit-Message.
3. Falls implementiert: Aktualisiere den Kommentar über dem `justfile`-Rezept, der aktuell fälschlich
   behauptet, ein "früher referenziertes Context-Engineering-Framework-Dokument und context-cli"
   existierten nicht mehr — nach Implementierung stimmt das nicht mehr für `context-tags` selbst.

## Teil 2 — `TEMPLATE_MICRO_SPEC.md` erstellen
1. Erstelle `docs/specs/TEMPLATE_MICRO_SPEC.md` mit einer Struktur, die zum bereits etablierten
   Spec-Driven-Development-Prozess des Projekts passt (Workpackages mit `WP-X.Y`-Nummerierung,
   funktionale Anforderungen `FR-XXX`, siehe PR-Template-Referenzen "Welche Spec-Anforderungen werden
   erfüllt?"). Mindestinhalt:

       # SPEC-<DATUM>-<NAME>

       ## Kontext & Motivation
       <!-- Warum wird dieses Feature/dieser Fix benötigt? -->

       ## Funktionale Anforderungen
       - FR-001: <Anforderung>
       - FR-002: <Anforderung>

       ## Nicht-Ziele (explizit ausgeschlossen)
       <!-- Was gehört NICHT zu diesem Workpackage -->

       ## Sperrzonen (Scope-Lock)
       **Erlaubte Dateien/Verzeichnisse:** <Liste>
       **Verboten:** <Liste — z.B. andere Crates, CI-Workflows, falls nicht explizit Teil der Aufgabe>

       ## Workpackages
       - [ ] WP-1.1: <Beschreibung> — Status: OFFEN

       ## Test-Strategie
       <!-- Welche Tests beweisen, dass die FRs erfüllt sind? -->

       ## Implementierungsnotizen
       <!-- Wird während der Umsetzung befüllt -->

       ## Änderungsprotokoll
       | Datum | Änderung |
       |---|---|

   Referenziere in dieser Vorlage explizit das Sperrzonen-Konzept aus P-J10 dieses Dokuments (falls
   P-J10 bereits gemerged ist — andernfalls nur den Abschnitt strukturell anlegen, er wird von P-J10
   inhaltlich verfeinert).
2. Verifiziere `just spec test-feature` läuft jetzt fehlerfrei durch und erzeugt
   `docs/specs/SPEC-<heutiges-datum>-test-feature.md` — lösche diese Testdatei danach wieder.

# Akzeptanzkriterien
- `just context-tags` (oder sein bewusster, dokumentierter Entfernung) endet nicht mehr in
  "Unknown xtask command".
- `just spec <beliebiger-name>` erzeugt erfolgreich eine neue Spec-Datei aus der Vorlage.
- `cargo test -p xtask` (falls neue Tests für `context-tags` hinzugefügt wurden) grün.

# Verifikation
    just spec verification-test && ls docs/specs/SPEC-*-verification-test.md && rm docs/specs/SPEC-*-verification-test.md
    just context-tags --severity=CRITICAL   # oder: git log zeigt bewussten Entfernungs-Commit
```

---

### P-J4: Automatisiertes Freshness-Gate für JULES_CONTEXT.md

```md
# Persona
Du bist ein Documentation-Engineer, der "lebende Dokumentation" von "totem Text, der so aussieht wie
lebende Dokumentation" unterscheidet — und Mechanismen baut, die Drift technisch unmöglich machen statt
auf Disziplin zu hoffen.

# Kontext
Datei: `.jules/JULES_CONTEXT.md` — explizit als "Permanent Ambient Context für Jules Sessions" markiert,
mit eigenem Disclaimer:

> ⚠️ FRISCHEGARANTIE: Diese Datei ist ein Kurzzeit-Snapshot.
> Bei Widerspruch gilt immer: `WORKING_STATE.md` (autogeneriert) > Code > diese Datei.
> Aktualisiere diesen Header-Timestamp wenn du dieses File bearbeitest.

Verifizierter Drift-Beweis: Der Header nennt "Stand: 2026-08-29", die Tabelle "Kritische ADRs" listet
ADR-010 bis ADR-030, aber `DECISIONS.md` enthält inzwischen 46 ADRs bis ADR-045
("Entkopplung von memfuse-router und memfuse-mcp durch IPC JSON-RPC Typverschiebung") — genau die Art
von architektonisch wichtiger Information, die in einer Ambient-Context-Datei fehlen darf, wenn sie
nicht regelmäßig aktualisiert wird. Der Disclaimer sagt zwar "bei Widerspruch gilt WORKING_STATE.md",
aber das hilft nicht gegen fehlende (nicht widersprüchliche) Information — Jules liest die Datei
und weiß schlicht nichts von ADR-045, weil kein Widerspruch, sondern eine Lücke vorliegt.

# Aufgabe
1. Erstelle in `xtask/src/main.rs` einen neuen Match-Arm `"check-jules-context-freshness"`, der:
   - Das `Stand:`-Datum aus dem Header von `.jules/JULES_CONTEXT.md` per Regex extrahiert.
   - Das Datum des letzten Eintrags in `DECISIONS.md` ermittelt (aus dem Änderungsprotokoll/Git-Log der
     Datei — nutze `git log -1 --format=%aI -- DECISIONS.md` als robusteste Quelle, nicht Textparsing
     von ADR-Titeln, die kein einheitliches Datumsformat garantieren).
   - Falls `DECISIONS.md` seit dem `Stand:`-Datum in `JULES_CONTEXT.md` verändert wurde (Git-Commit-
     Zeitstempel > geparster Stand): Fehler ausgeben und mit `exit 1` abbrechen, MIT einer klaren
     Handlungsanweisung im Output: "❌ JULES_CONTEXT.md ist veraltet (Stand: <X>, DECISIONS.md zuletzt
     geändert: <Y>). Aktualisiere den Header-Timestamp UND den ADR-Tabellen-Abschnitt in
     .jules/JULES_CONTEXT.md manuell, dann erneut committen."
   - Führe dieselbe Prüfung zusätzlich gegen `WORKING_STATE.md` durch (welches bereits automatisch via
     `sync-docs` aktuell gehalten wird) — falls `JULES_CONTEXT.md`'s "Aktueller Projektstatus"-Abschnitt
     älter ist als der letzte `WORKING_STATE.md`-Sync, ebenfalls fehlschlagen.
2. Ergänze dieses neue Check als Job/Step in `.github/workflows/context-gates.yml` (als neues
   "Gate 10"), analog zum bestehenden Stil der anderen 9 Gates dort.
3. Aktualisiere `.jules/JULES_CONTEXT.md` selbst EINMALIG jetzt vollständig:
   - Header-`Stand:` auf das heutige Datum.
   - "Kritische ADRs"-Tabelle um mindestens ADR-039, ADR-041, ADR-043, ADR-044, ADR-045 ergänzen
     (lies deren tatsächlichen Titel/Konsequenz aus `DECISIONS.md`, keine Vermutung).
   - "Aktueller Projektstatus"-Abschnitt gegen den tatsächlichen Stand von `WORKING_STATE.md`
     abgleichen und ggf. korrigieren (z. B. falls Phase 2 "Cognitive Memory" inzwischen begonnen hat).
4. Ergänze in `AGENTS.md §7` (Governance Documents Tabelle) eine Zeile für dieses neue Gate:

       | `.jules/JULES_CONTEXT.md` | Freshness automatisch geprüft via `xtask check-jules-context-freshness` (Gate 10) |

# Akzeptanzkriterien
- Neues xtask-Subcommand `check-jules-context-freshness` existiert und wird von einem neuen CI-Gate
  aufgerufen.
- Das Gate schlägt aktuell (vor dem manuellen Update in Schritt 3) korrekt fehl — verifiziere das ZUERST
  als Beweis, dass die Erkennung funktioniert, BEVOR du `JULES_CONTEXT.md` aktualisierst.
- Nach dem manuellen Update in Schritt 3 ist das Gate grün.
- Ein zukünftiger PR, der `DECISIONS.md` ändert, ohne `.jules/JULES_CONTEXT.md` anzufassen, lässt das
  Gate rot werden (teste das mit einem Dummy-ADR-Eintrag, den du danach wieder entfernst).

# Verifikation
    cargo run -p xtask -- check-jules-context-freshness; echo "Exit vor Update: $?"
    # ... Update von JULES_CONTEXT.md durchführen ...
    cargo run -p xtask -- check-jules-context-freshness; echo "Exit nach Update: $?"

Erster Exit-Code muss `1` sein (Beweis der Fehlererkennung), zweiter muss `0` sein.
```

---

### P-J5: Concurrency-Fault-Injection-Harness mit `loom`

```md
# Persona
Du bist ein Rust-Concurrency-Spezialist mit `loom`-Erfahrung (exhaustives Modell-Checking für
nebenläufigen Code durch systematisches Interleaving aller möglichen Thread-Ausführungsreihenfolgen).
Du weißt, dass `loom` reale Nebenläufigkeitsfehler findet, die klassische Tests wegen Timing-Abhängigkeit
nur sporadisch reproduzieren — genau die Art Bug, die in `memfuse_neue_befunde.md` (NEU-02) beschrieben ist.

# Kontext
Motivation (verifiziert, siehe `memfuse_neue_befunde.md` NEU-02): In
`crates/memfuse-index/src/hnsw.rs`, Funktion `do_insert()`, wird der SQ8-Quantizer per
`self.quantizer.try_write()` (best-effort) statt eines garantierten Locks aktualisiert. Ob eine
Grenzerweiterung (`expand_bounds_to_fit`) stattfindet, hängt vom Scheduling ab — ein klassischer
"funktioniert meistens, bricht unter Last" Nebenläufigkeitsfehler, der mit normalen `#[tokio::test]`-
Tests kaum deterministisch reproduzierbar ist.

Ähnlich kritische Lock-Primitive im Projekt (laut `justfile`-Kommentaren und Interface-Dokumentation):
- `commit_mutex` in `crates/memfuse-store/src/lsm.rs` (serialisiert alle Commits)
- `insert_lock` in `crates/memfuse-db/src/collection/crud.rs` (TOCTOU-Schutz für DocId-Kollisionsprüfung)
- `quantizer: RwLock<...>` in `crates/memfuse-index/src/hnsw.rs`

`proptest` ist bereits in 9 Crates vorhanden (gut für Werte-Räume), deckt aber keine
Interleaving-Räume nebenläufiger Ausführung ab — das ist eine andere Testkategorie mit anderem Werkzeug.

# Aufgabe
1. Füge `loom` als dev-dependency (nicht Produktions-Dependency!) zu `crates/memfuse-index/Cargo.toml`
   und `crates/memfuse-store/Cargo.toml` hinzu:

       [dev-dependencies]
       loom = "0.7"

   Beachte `rules/dependency_audit.md`: führe die dortige Checkliste aus (Lizenz MIT/Apache-2.0,
   Maintenance-Status, `cargo audit`) auch für diese Dev-Dependency, auch wenn sie nicht in Produktion
   läuft — dokumentiere kurz in der Commit-Message, dass die Checkliste durchlaufen wurde.
2. `loom` erfordert, dass der zu testende Code über `loom::sync::{Mutex, RwLock}` statt
   `std::sync`/`parking_lot`/`tokio::sync` abstrahiert wird, meist via `#[cfg(loom)]`-Konditionalisierung
   oder einer eigenen Sync-Abstraktionsschicht. Prüfe zuerst, welches Lock-Primitiv `quantizer` in
   `hnsw.rs` tatsächlich nutzt (`parking_lot::RwLock` laut `memfuse_neue_befunde.md`). Richte dafür ein
   minimal-invasives Test-only-Modul ein, z. B. `crates/memfuse-index/tests/loom_quantizer.rs`, das die
   Kernlogik der Bound-Expansion-Race in einer `loom`-kompatiblen, isolierten Nachbildung reproduziert
   (nicht den kompletten `HnswIndexCore`, das wäre für `loom` zu komplex/langsam — `loom` explodiert
   kombinatorisch bei komplexem State). Konzentriere dich auf: zwei "Threads" (loom-Modell), einer
   simuliert `search()` (hält Read-Lock), der andere simuliert `do_insert()`
   (`try_write()`-Fallback-Pfad), und der Test verifiziert die aktuell fehlerhafte Eigenschaft:
   "Es gibt eine Ausführungsreihenfolge, unter der `expand_bounds_to_fit` NICHT aufgerufen wird, obwohl
   ein Out-of-Range-Vektor eingefügt wurde."
3. Schreibe den `loom`-Test so, dass er mit dem AKTUELLEN Code (vor dem NEU-02-Fix) fehlschlägt
   (bzw. das fehlerhafte Verhalten als reproduzierbar nachweist), und nach Anwendung des NEU-02-Fixes
   (`try_write()` → `write()`, siehe `memfuse_neue_befunde.md` Fix-Vorschlag 1) erfolgreich ist. Dies
   ist die konkrete Umsetzung des Prinzips deines externen Beraters: "Wenn Jules einen fehlerhaften
   nebenläufigen Zugriff schreibt, muss ein Test das erzwingen, nicht ein Review."
4. Ergänze ein analoges, minimales `loom`-Testmodul für `commit_mutex` in `memfuse-store` (mindestens:
   verifiziere, dass zwei "nebenläufige" `commit()`-Aufrufe niemals gleichzeitig eine Sequenznummer
   doppelt vergeben — das ist die Kerninvariante, die `commit_mutex` garantieren soll laut
   Modul-Doc-Kommentar in `lsm.rs`).
5. Füge in `.github/workflows/rust-ci.yml` einen neuen, separaten Job hinzu (loom-Tests sind
   langsamer als normale Tests und sollten nicht die Haupt-Test-Matrix verlangsamen):

       loom-concurrency-tests:
         name: Loom Concurrency Model-Checking
         runs-on: ubuntu-latest
         steps:
           - uses: actions/checkout@v4
           - uses: dtolnay/rust-toolchain@stable
           - uses: Swatinem/rust-cache@v2
           - name: Run loom tests (memfuse-index)
             run: RUSTFLAGS="--cfg loom" cargo test --release -p memfuse-index --test loom_quantizer
           - name: Run loom tests (memfuse-store)
             run: RUSTFLAGS="--cfg loom" cargo test --release -p memfuse-store --test loom_commit_mutex

6. Dokumentiere das neue Werkzeug in `rules/testing.md` unter einem neuen Abschnitt
   "Concurrency Model-Checking (`loom`)" — wann es zu nutzen ist (bei jedem neuen/geänderten
   `Mutex`/`RwLock`, der eine Nebenläufigkeits-Invariante garantieren soll) und wie es sich von
   `proptest` unterscheidet.

# Akzeptanzkriterien
- `loom` ist als dev-dependency in `memfuse-index` und `memfuse-store` vorhanden.
- Mindestens 2 `loom`-Tests existieren (Quantizer-Bound-Race, Commit-Mutex-Sequenznummer-Eindeutigkeit).
- Der Quantizer-`loom`-Test beweist nachweislich das NEU-02-Problem VOR dessen Fix (roter Test) und ist
  grün NACH dessen Fix — dokumentiere beide Zustände im PR (z. B. als zwei separate Commits: "test: add
  failing loom test for NEU-02" gefolgt von "fix: apply NEU-02 fix, loom test now green").
- Neuer CI-Job `loom-concurrency-tests` läuft separat und grün.
- `rules/testing.md` dokumentiert das neue Werkzeug.

# Verifikation
    RUSTFLAGS="--cfg loom" cargo test --release -p memfuse-index --test loom_quantizer -- --nocapture
    RUSTFLAGS="--cfg loom" cargo test --release -p memfuse-store --test loom_commit_mutex -- --nocapture

Beide müssen grün sein NACH Anwendung des NEU-02-Fixes (siehe P-J9).
```

---

### P-J6: Mutation-Testing mit `cargo-mutants` automatisieren

```md
# Persona
Du bist ein Test-Qualitäts-Ingenieur, der das Prinzip "Testabdeckung ≠ Testqualität" operationalisiert.
Du kennst Mutation-Testing als Methode, um Tests zu finden, die zwar Codezeilen ausführen, aber keine
echte Assertion-Kraft haben (genau das Problem, das `rules/testing.md` bereits textuell beschreibt,
aber aktuell nicht automatisiert prüft).

# Kontext
`rules/testing.md` enthält bereits diesen Abschnitt (unverändert lassen, nur operationalisieren):

    ## Mutation Survival Check
    Before marking a test suite as complete, ask:
    > "If I changed `<` to `<=` or `+1` to `+0` in the implementation, would any test fail?"
    If the answer is "no" for a logic branch, the test suite has a gap.

Dieses Prinzip wird aktuell rein manuell/introspektiv angewendet. `cargo-mutants` automatisiert genau
diese Frage: es verändert (mutiert) den Produktionscode systematisch (z. B. `<` → `<=`, `+` → `-`,
`true` → `false`) und prüft, ob die bestehende Testsuite die Mutation "tötet" (also fehlschlägt). Nicht
getötete Mutanten zeigen exakt die Codepfade, für die `rules/testing.md`s Frage mit "nein" beantwortet
werden müsste.

Mutation-Testing über den GESAMTEN Workspace ist für 15 Crates / ~80.000 Zeilen zu teuer für jeden PR
(Laufzeit typischerweise ein Vielfaches der normalen Testsuite). Beschränke den Scope bewusst.

# Aufgabe
1. Installiere/dokumentiere `cargo-mutants` als Dev-Tool (nicht als Cargo-Dependency, sondern als
   CLI-Tool analog zu `cargo-audit`):

       cargo install cargo-mutants --locked

2. Erstelle eine Konfigurationsdatei `.cargo/mutants.toml` (oder Workspace-Root `mutants.toml`, je nach
   aktueller `cargo-mutants`-Konvention — prüfe die Tool-Dokumentation für die korrekte Datei-Location
   der installierten Version), die den Scope bewusst auf die laut
   `Memfuse-Komplex-Eigenbau.md`-Komplexitätsindex kritischsten Crates beschränkt:

       # Mutation-Testing beschränkt auf die algorithmisch komplexesten, am wenigsten
       # trivial-korrekten Crates (siehe docs/COMPLEXITY_INDEX.md Rang 1-4).
       exclude_globs = [
           "crates/memfuse-tauri/**",
           "crates/memfuse-ollama/**",
           "crates/memfuse-py/**",
           "crates/memfuse-mcp/**",
           "crates/memfuse-router/**",
           "crates/memfuse-embed/**",
           "crates/memfuse-agent/**",
           "crates/memfuse-checkpoint/**",
           "crates/memfuse-crypto/**",
       ]
       # Fokus: memfuse-db (Fusion/RRF/MVCC), memfuse-index (HNSW/SIMD-Dispatch-Logik,
       # NICHT die unsafe SIMD-Intrinsics selbst), memfuse-store (LSM/WAL), memfuse-graph (CSR/PPR)

3. Führe einen initialen lokalen Lauf gegen `memfuse-graph` (kleinster der 4 Fokus-Crates, für einen
   schnellen ersten Baseline-Lauf) durch:

       cargo mutants -p memfuse-graph --timeout 300

   Dokumentiere das Ergebnis (Anzahl gefundener Mutanten, Anzahl überlebender/getöteter Mutanten) als
   neue Datei `docs/audits/MUTATION_TESTING_BASELINE_memfuse-graph.md`.
4. Für jeden überlebenden Mutanten (survived mutant) in diesem initialen Lauf: Öffne die betroffene
   Zeile, verstehe, welche Test-Lücke er aufdeckt, und ergänze EINEN gezielten Test, der diesen
   spezifischen Mutanten tötet. Falls die Anzahl überlebender Mutanten zu groß ist, um sie alle in
   diesem PR zu beheben: behebe mindestens die 5 mit der höchsten laut deiner Einschätzung
   sicherheitskritischsten Auswirkung (z. B. Mutanten in Traversal-Terminierungsbedingungen,
   Zyklenerkennung) und tracke den Rest als `AI-TAG[SMELL][MAJOR]`-Einträge mit Verweis auf die
   Mutation-Testing-Baseline-Datei.
5. Füge `cargo-mutants` NICHT als Pflicht-Gate in `rust-ci.yml`/`context-gates.yml` ein (zu langsam für
   jeden PR), sondern als eigenen, manuell auslösbaren Workflow
   `.github/workflows/mutation-testing.yml` mit `workflow_dispatch`-Trigger UND als Teil des in P-J7 zu
   erstellenden wöchentlichen Cron-Audits:

       name: Mutation Testing (On-Demand + Weekly)
       on:
         workflow_dispatch:
           inputs:
             crate:
               description: "Crate to mutation-test"
               required: true
               default: "memfuse-graph"
       jobs:
         mutants:
           runs-on: ubuntu-latest
           steps:
             - uses: actions/checkout@v4
             - uses: dtolnay/rust-toolchain@stable
             - uses: Swatinem/rust-cache@v2
             - run: cargo install cargo-mutants --locked
             - run: cargo mutants -p ${{ github.event.inputs.crate || 'memfuse-graph' }} --timeout 300 --json > mutants-report.json
             - uses: actions/upload-artifact@v4
               with:
                 name: mutants-report
                 path: mutants-report.json

6. Dokumentiere in `rules/testing.md` unter dem bestehenden "Mutation Survival Check"-Abschnitt, dass
   dies jetzt zusätzlich automatisiert via `cargo mutants` und den Workflow `mutation-testing.yml`
   verifizierbar ist, inkl. Kommando-Referenz für lokale Läufe.

# Akzeptanzkriterien
- `mutants.toml` (oder äquivalente Config-Datei) existiert mit begründetem, dokumentiertem Scope.
- Mindestens ein vollständiger Mutation-Testing-Lauf gegen `memfuse-graph` wurde durchgeführt und als
  Baseline-Dokument festgehalten.
- Mindestens 5 durch diesen Lauf gefundene Test-Lücken wurden mit gezielten neuen Tests geschlossen
  (verifizierbar: erneuter `cargo mutants`-Lauf zeigt diese Mutanten als "killed" statt "survived").
- Neuer On-Demand-CI-Workflow `mutation-testing.yml` existiert und ist manuell auslösbar.

# Verifikation
    cargo mutants -p memfuse-graph --timeout 300 --json | jq '.summary'

Vergleiche `survived`-Zahl vor und nach den in Schritt 4 hinzugefügten Tests — sie muss gesunken sein.
```

---

### P-J7: Scheduled Proaktiv-Audit-Workflow (Cron) einrichten

```md
# Persona
Du bist ein DevOps-Engineer mit Erfahrung in "ChatOps"/Agent-getriebenen, zeitgesteuerten
Automatisierungs-Pipelines. Du weißt, dass der Wert asynchroner Agenten wie Jules erst dann voll
realisiert wird, wenn sie nicht nur reaktiv (auf Prompt/PR-Fehler) arbeiten, sondern proaktiv nach
einem festen Zeitplan.

# Kontext
Aktuell existiert kein `schedule:`-getriggerter Workflow im Repository (`grep -l "schedule:"
.github/workflows/*.yml` liefert keinen Treffer). Jeder Audit (wie die diesem Dokument zugrunde
liegenden `memfuse_audit-analyse.md`, `memfuse_audit-analyse-2.md`, `memfuse_neue_befunde.md`) entstand
bisher, weil DU manuell ein separates Analyse-Dokument erstellt und in einen neuen Jules-Prompt gegossen
hast. Das ist der reaktive Workflow, den dein externer Berater als Strategie #3 ("Proaktive Audits")
adressiert hat.

Wichtig: Google Jules selbst wird nicht über GitHub Actions direkt "aufgerufen" wie ein CLI-Tool —
die Jules-Integration läuft über GitHub-App-Webhooks/API (Issue-Kommentare, `workflow_run`-Events, siehe
Kontext-Dokument "GitHub-Integration"). Dieser Workflow soll daher kein `jules`-CLI aufrufen (das
existiert nicht als Standard-GitHub-Action), sondern stattdessen so bauen, dass er:
(a) ein GitHub-Issue mit einem präzisen, strukturierten Audit-Auftrag erstellt, den du dann (oder eine
    bestehende Jules-GitHub-App-Integration automatisch) aufgreifen kann, ODER
(b) falls eine Jules-GitHub-Action/Webhook-Integration bereits eingerichtet ist (prüfe `.github/` auf
    Hinweise auf eine `jules`-App-Installation, z. B. `.github/jules.yml` o. ä. — falls nicht vorhanden,
    gehe von Fall (a) aus).

# Aufgabe
1. Prüfe zuerst, ob eine existierende Jules-GitHub-App/Action-Integrationskonfiguration im Repository
   vorhanden ist (`find . -iname "*jules*" -not -path "./.jules/*"`). Falls eine solche existiert,
   passe Schritt 2 entsprechend an, um sie zu nutzen, statt eine neue Issue-basierte Eskalation zu bauen.
2. Erstelle `.github/workflows/scheduled-audit.yml`:

       name: Scheduled Proactive Audit
       on:
         schedule:
           # Jeden Freitag 22:00 UTC
           - cron: "0 22 * * 5"
         workflow_dispatch: {}

       jobs:
         prepare-audit-context:
           runs-on: ubuntu-latest
           steps:
             - uses: actions/checkout@v4
               with:
                 fetch-depth: 0
             - name: Sammle Commits der letzten 7 Tage
               id: commits
               run: |
                 echo "commits<<EOF" >> "$GITHUB_OUTPUT"
                 git log --since="7 days ago" --oneline --no-merges >> "$GITHUB_OUTPUT"
                 echo "EOF" >> "$GITHUB_OUTPUT"
             - name: Erstelle Audit-Issue
               uses: actions/github-script@v7
               with:
                 script: |
                   const commits = `${{ steps.commits.outputs.commits }}`;
                   const body = `## Woechentlicher Proaktiv-Audit-Auftrag

                   WICHTIG: Befolge .jules/AUDIT_INTAKE_PROTOCOL.md — verifiziere JEDEN Fund
                   gegen den AKTUELLEN Code, bevor du etwas aenderst.

                   ### Auftrag
                   Analysiere die folgenden Commits der letzten 7 Tage auf:
                   1. Race Conditions / TOCTOU-Fehler (insbesondere in neu hinzugefuegten
                      Mutex/RwLock/Atomic-Nutzungen)
                   2. Neue .unwrap()/.expect() in Produktionscode ausserhalb der Baseline
                      (.unwrap-baseline.txt)
                   3. Silent-Failure-Pattern (let _ = ... bei IO-Operationen)
                   4. DAG-Grenzverletzungen in neuen Cargo.toml-Dependencies

                   Fuer jeden gefundenen, verifizierten Fehler:
                   - Schreibe zuerst einen Test, der den Fehler reproduzierbar beweist (roter Test)
                   - Behebe den Fehler
                   - Verifiziere: Test ist jetzt gruen, just check und just dag-check sind gruen
                   - Oeffne einen PR mit Verweis auf diesen Issue

                   Falls KEIN Fund verifiziert werden kann: Kommentiere das explizit im Issue statt
                   einen PR mit spekulativen Aenderungen zu oeffnen.

                   ### Commits dieser Woche
                   \`\`\`
                   ${commits || "(keine Commits in den letzten 7 Tagen)"}
                   \`\`\`
                   `;
                   await github.rest.issues.create({
                     owner: context.repo.owner,
                     repo: context.repo.repo,
                     title: `Proaktiv-Audit ${new Date().toISOString().slice(0,10)}`,
                     body: body,
                     labels: ["jules-audit", "automated"]
                   });

3. Erstelle das Label `jules-audit` im Repository (falls nicht via Actions automatisch anlegbar, per
   `gh label create jules-audit --description "Automatisch generierter Jules-Audit-Auftrag" --color FBCA04`
   dokumentieren, dass dies einmalig manuell nachzuholen ist).
4. Falls unter `.github/` eine Jules-Webhook-Integration existiert, die auf neue Issues mit einem
   bestimmten Label reagiert: verifiziere, dass das Label `jules-audit` korrekt mit dieser Integration
   verknüpft ist. Falls nicht: dokumentiere in `.jules/JULES_CONTEXT.md` (oder einer neuen Datei
   `.jules/SCHEDULED_AUDIT.md`) den manuellen Schritt, den DU (der Mensch) wöchentlich ausführen musst
   (z. B. "Issue mit `jules-audit`-Label manuell an Jules per Kommentar `@google-jules löse das Problem`
   zuweisen").
5. Ergänze in derselben `scheduled-audit.yml` einen zweiten, unabhängigen Job, der wöchentlich den in
   P-J6 gebauten `mutation-testing.yml`-Workflow per `workflow_dispatch` gegen einen rotierenden Crate
   auslöst (diese Woche `memfuse-graph`, nächste Woche `memfuse-index`, usw. — nutze die Kalenderwoche
   modulo Anzahl Fokus-Crates zur Rotation), sodass über einen Monat alle 4 Fokus-Crates einmal
   durchlaufen wurden, ohne dass jeder Lauf alle Crates auf einmal prüfen muss.

# Akzeptanzkriterien
- `.github/workflows/scheduled-audit.yml` existiert, ist syntaktisch valide (`workflow_dispatch`
  manuell testbar) und der Cron-Ausdruck ist korrekt (`0 22 * * 5` = Freitag 22:00 UTC).
- Ein manueller Testlauf via `workflow_dispatch` erzeugt erfolgreich ein neues GitHub-Issue mit
  korrekt gefülltem Commit-Log der letzten 7 Tage.
- Die Rotation der Mutation-Testing-Crates ist nachvollziehbar dokumentiert.

# Verifikation
Löse den Workflow manuell über die GitHub-Actions-UI (`workflow_dispatch`) aus und verifiziere, dass
ein neues Issue mit Label `jules-audit` und plausiblem Commit-Log-Inhalt erscheint.
```

---

### P-J8: Windows/macOS-Testlane in die reguläre PR-CI aufnehmen

```md
# Persona
Du bist ein Cross-Platform-CI-Engineer. Du weißt, dass Plattform-spezifische Bugs (Datei-Permissions,
Pfad-Separatoren, Zeilenendezeichen, ACLs) am günstigsten VOR dem Merge gefunden werden, nicht erst beim
Release-Tag-Build.

# Kontext
`.github/workflows/tauri-release.yml` baut bereits auf `macos-latest`, `ubuntu-22.04`, `windows-latest` —
aber nur bei `push: tags: ["v*"]`, also erst beim tatsächlichen Release. `rust-ci.yml` und
`context-gates.yml` laufen ausschließlich auf `ubuntu-latest`. Ein Windows-spezifischer Bug (z. B. bei
Datei-Permission-Handling für den WAL-Integritätsschlüssel, oder Pfad-Handling in der
Tauri-Ingestion-Pipeline) würde damit erst beim Release sichtbar — Wochen nach der eigentlichen
Code-Änderung, wenn der ursprüngliche Kontext für Jules (und dich) längst nicht mehr "warm" ist.

# Aufgabe
1. Ergänze in `.github/workflows/rust-ci.yml` einen neuen Job `test-cross-platform`, der die
   Kernbibliotheks-Tests (nicht Tauri, nicht das volle Release-Bundling — das bleibt teuer und
   gehört weiter zu `tauri-release.yml`) auf Windows UND macOS bei jedem PR ausführt:

       test-cross-platform:
         name: Cross-Platform Core Tests
         strategy:
           fail-fast: false
           matrix:
             os: [windows-latest, macos-latest]
         runs-on: ${{ matrix.os }}
         steps:
           - uses: actions/checkout@v4
           - uses: dtolnay/rust-toolchain@stable
           - uses: Swatinem/rust-cache@v2
           - name: Test core storage/crypto crates
             run: cargo test --locked -p memfuse-core -p memfuse-store -p memfuse-crypto -p memfuse-index -p memfuse-text -p memfuse-graph -p memfuse-checkpoint -p memfuse-db

   Beschränke den Scope bewusst auf Layer 0–2 (die plattformkritischsten Crates: Storage, Crypto,
   Index — genau dort, wo Datei-Permissions/Mmap/Pfad-Handling relevant sind), NICHT auf den vollen
   Workspace (Tauri/Python-Bindings/ONNX bleiben zu teuer und komplex für jeden PR auf 2 zusätzlichen
   Plattformen — das würde die PR-Latenz für einen Solo-Entwickler unnötig aufblähen).
2. Setze `fail-fast: false`, damit ein Windows-Fehler nicht den macOS-Job abbricht (und umgekehrt) —
   du willst beide Ergebnisse sehen, nicht nur das erste Scheitern.
3. Füge diesen neuen Job NICHT als Required-Status-Check für den Merge hinzu (das würde die
   Entwicklungsgeschwindigkeit für einen Solo-Entwickler-Workflow zu stark bremsen, insbesondere wenn
   Windows-CI-Runner-Zeiten variieren) — dokumentiere stattdessen in `AGENTS.md §2`, dass dieser Job
   informativ ist und bei Rot manuell geprüft werden sollte, bevor ein Release-Tag gesetzt wird.
4. Falls dieser neue Job bei der ersten Ausführung tatsächliche plattformspezifische Fehlschläge
   aufdeckt: behebe NICHT automatisch in diesem PR (das würde Scope Creep erzeugen, siehe P-J10 für das
   Sperrzonen-Prinzip) — dokumentiere gefundene Fehlschläge stattdessen als neue, separate
   `AI-TAG[SMELL][MAJOR]`-Einträge mit Plattform-Kennzeichnung im betroffenen Code UND liste sie explizit
   im PR-Beschreibungstext auf, damit sie als eigene Folge-Prompts behandelt werden können.

# Akzeptanzkriterien
- Neuer Job `test-cross-platform` (Matrix: `windows-latest`, `macos-latest`) läuft bei jedem PR.
- Job ist NICHT als Required-Check konfiguriert (informativ, nicht blockierend).
- `AGENTS.md §2` dokumentiert den informativen Charakter dieses Jobs.
- Alle bei der ersten Ausführung entdeckten Plattform-spezifischen Fehlschläge sind als AI-TAGs
  dokumentiert, nicht "nebenbei" gefixt.

# Verifikation
Prüfe im GitHub-Actions-Log des PRs, dass beide Matrix-Jobs (`windows-latest`, `macos-latest`) liefen
und deren Ergebnis (grün oder mit dokumentierten AI-TAG-Funden) sichtbar ist.
```

---

### P-J9: NEU-01/NEU-02 fixen — als Referenzfall für das neue Fault-Injection-Gate

```md
# Persona
Du bist ein Senior Rust Engineer, der laut `.jules/AUDIT_INTAKE_PROTOCOL.md` arbeitet: JEDER Finding
wird vor Implementierung gegen den aktuellen Code verifiziert.

# Kontext
Quelle: `memfuse_neue_befunde.md` (bereits gegen Code verifiziert im Rahmen der Erstanalyse dieses
Dokuments, Findings NEU-01 und NEU-02 bestätigt aktiv im aktuellen HEAD).

Dieser Prompt sollte nach P-J5 (loom-Harness) ausgeführt werden, damit der neue
loom-Regressionstest für NEU-02 direkt als Teil dieses Fixes mitgeliefert werden kann, statt in einer
separaten Session nachgezogen zu werden.

# Aufgabe

## NEU-01 — Tombstone-Filter-Bug (KRITISCH)
1. Öffne `crates/memfuse-index/src/hnsw.rs`, Funktion `search_filtered()`, verifiziere die
   `if let Some(f) = filter { ... } else if deleted.contains(...) { ... }`-Struktur gemäß
   `AUDIT_INTAKE_PROTOCOL.md` Schritt 1–2 (Datei/Zeile öffnen, Invariante prüfen).
2. Fixe wie in `memfuse_neue_befunde.md` spezifiziert — Tombstone-Prüfung wird unconditional VOR der
   Custom-Filter-Prüfung durchgeführt:

       if deleted.contains(c.index as u64) {
           continue;
       }
       if let Some(f) = filter {
           if !f(doc_id) {
               continue;
           }
       }

3. Ergänze einen Regressionstest in `crates/memfuse-index/src/hnsw.rs` (Testmodul), der:
   - Einen Vektor einfügt und committed.
   - `rollback_to_tx()` auf einen Zeitpunkt VOR diesem Insert aufruft (Knoten wird laut Analyse als
     `deleted` markiert).
   - `search_filtered()` mit einem Custom-Filter aufruft, der diesen DocId absichtlich als "match"
     zurückgibt (simuliert das in `memfuse_neue_befunde.md` beschriebene Storage/Index-
     Inkonsistenz-Szenario).
   - Verifiziert, dass das Suchergebnis den zurückgerollten Dokument NICHT enthält.
4. Verifiziere zusätzlich (Nicht-Regression): ein bestehender Test mit aktivem Custom-Filter und
   NICHT-gelöschten Dokumenten liefert weiterhin identische Ergebnisse wie vor dem Fix.

## NEU-02 — SQ8-Quantizer-Race (MITTEL)
1. Öffne `crates/memfuse-index/src/hnsw.rs`, Funktion `do_insert()`, verifiziere den
   `try_write()`/`else if let Some(q) = self.quantizer.read()`-Pfad gegen aktuellen Code.
2. Wende Fix-Option 1 aus `memfuse_neue_befunde.md` an (garantiertes `write()` statt `try_write()`):

       let vector_data = if self.config.quantize {
           let mut q_guard = self.quantizer.write();
           if let Some(q) = q_guard.as_mut() {
               q.expand_bounds_to_fit(vector);
               VectorData::U8(q.quantize(vector))
           } else {
               VectorData::F32(vector.to_vec())
           }
       } else {
           VectorData::F32(vector.to_vec())
       };

3. Verifiziere, dass `parking_lot::RwLock::write()` hier unkritisch bezüglich Deadlocks ist: prüfe
   `rules/detect_nested_locks.yml` / führe `just debt-audit` (Nested-Lock-Scan) NACH der Änderung aus,
   um sicherzustellen, dass kein verschachtelter Lock-Erwerb entsteht (z. B. falls `do_insert()` den
   `quantizer`-Lock hält, während es einen anderen Lock zu erwerben versucht, der andernorts in
   umgekehrter Reihenfolge erworben wird).
4. Falls P-J5 bereits gemerged ist: verknüpfe den dort erstellten `loom`-Test
   (`crates/memfuse-index/tests/loom_quantizer.rs`) explizit mit diesem Fix — führe ihn aus, verifiziere
   roten Zustand VOR diesem Fix (falls noch nicht geschehen) und grünen Zustand NACH diesem Fix.
   Falls P-J5 noch nicht gemerged ist: ergänze zusätzlich zum normalen `#[tokio::test]`-
   Regressionstest einen Kommentar `// AI-NOTE: Sobald P-J5 (loom-Harness) gemerged ist, ergänze hier
   einen loom-basierten Nebenläufigkeits-Beweis, siehe memfuse_neue_befunde.md NEU-02.`
5. Ergänze einen klassischen (nicht-loom) Concurrency-Test: parallele `search()`- und `insert()`-
   Aufrufe mit einem absichtlich außerhalb der Trainingsgrenzen liegenden Vektor (via `tokio::join!`
   oder `std::thread::scope`), der verifiziert, dass `expand_bounds_to_fit` in jedem Durchlauf
   angewendet wurde (z. B. durch Prüfen der aktualisierten `min`/`max`-Grenzen des Quantizers nach dem
   Insert, nicht nur durch Beobachten des Verhaltens).

# Akzeptanzkriterien
- NEU-01: Tombstone-Prüfung ist unconditional, neuer Regressionstest ist grün, bestehende Tests
  unverändert grün.
- NEU-02: `try_write()` durch `write()` ersetzt, `just debt-audit` (Nested-Lock-Scan) bleibt grün nach
  der Änderung, neuer Concurrency-Test ist grün.
- Beide Fixes sind im PR jeweils mit einem `AI-TAG[...][RESOLVED]`-Kommentar samt `TS:`/`SESSION:`-
  Feldern an der behobenen Stelle dokumentiert (gemäß `rules/tag_taxonomy.md`).
- `just check`, `just dag-check`, `just debt-audit`, `cargo test -p memfuse-index` sind alle grün.

# Verifikation
    cargo test -p memfuse-index hnsw:: -- --nocapture
    just debt-audit
    just dag-check

Alle drei Kommandos exit mit Code 0.
```

---

### P-J10: Die Sperrzonen-Task-Prompt-Vorlage (Ziel-Deliverable)

```md
# Persona
Du bist ein Prompt-Engineering-Spezialist für autonome Coding-Agenten. Du erstellst eine
wiederverwendbare, versionierte Vorlage, die JEDE zukünftige Feature-/Bugfix-Anforderung an Jules
strukturell vor Scope Creep schützt — das zentrale strukturelle Gegenmittel gegen das Problem, das dein
externer Berater beschrieben hat: KI-Agenten "wollen beim Implementieren eines Features gleich noch drei
andere Dinge verbessern und bauen dabei Fehler ein."

# Kontext
Aktuell existiert dieses Konzept nur in deinem externen, nicht versionierten P01–P16-Workflow (außerhalb
des Repositories). `docs/specs/TEMPLATE_MICRO_SPEC.md` (aus P-J3 neu erstellt) enthält bereits einen
rudimentären "Sperrzonen"-Abschnitt — dieser Prompt baut darauf auf und macht ihn erzwingbar, nicht
nur dokumentarisch.

# Aufgabe

## Teil 1 — Vorlage verfeinern
1. Erweitere `docs/specs/TEMPLATE_MICRO_SPEC.md` (aus P-J3) um einen präzise strukturierten
   Sperrzonen-Block mit maschinenlesbarem Format (damit er in Teil 2 automatisiert geparst werden kann):

       ## Sperrzonen (Scope-Lock)
       <!-- SCOPE-LOCK-START -->
       ALLOWED_PATHS:
         - crates/memfuse-text/src/**
         - rules/testing.md
       FORBIDDEN_PATHS:
         - crates/memfuse-store/**
         - crates/memfuse-graph/**
         - .github/workflows/**
       FORBIDDEN_ACTIONS:
         - "Veraendere unter KEINEN Umstaenden Dateien in memfuse-store oder memfuse-graph, auch wenn dort
            offensichtliche Optimierungspotenziale erkennbar sind — dokumentiere sie stattdessen als
            AI-TAG[SMELL][MINOR] mit Verweis auf ein zukuenftiges separates Workpackage."
       <!-- SCOPE-LOCK-END -->

2. Erstelle zusätzlich eine eigenständige, kompakte Kurz-Vorlage `.jules/TASK_PROMPT_TEMPLATE.md`
   (getrennt von der vollen Micro-Spec, für kleinere Einzel-Prompts wie die in diesem Dokument
   vorliegenden P-J1 bis P-J9 selbst — ein Meta-Beispiel für dich als Nutzer):

       # Task-Prompt-Vorlage fuer Jules (MemFuse)

       ## Persona
       <Rolle + relevante Spezialisierung>

       ## Kontext
       <Datei(en) + verifizierter Ist-Zustand. IMMER am aktuellen Code verifizieren, siehe
       .jules/AUDIT_INTAKE_PROTOCOL.md — auch bei eigenen, frisch geschriebenen Prompts.>

       ## Sperrzonen (PFLICHTFELD)
       **Erlaubte Dateien:** <explizite Liste oder Glob-Pattern>
       **Verboten:** <explizite Liste — Standard-Default falls nicht anders begruendet:
       .github/workflows/**, justfile, CONSTITUTION.md, DECISIONS.md (ausser bei explizitem
       ADR-Auftrag)>
       **Bei Scope-Konflikt:** Wenn die Aufgabe ohne Aenderung einer verbotenen Datei nicht loesbar ist,
       STOPPEN und im PR-Kommentar/Session-Log explizit dokumentieren, WARUM — nicht stillschweigend
       die Sperrzone verletzen.

       ## Aufgabe
       <Nummerierte Schritte>

       ## Akzeptanzkriterien
       <Pruefbare Bedingungen>

       ## Verifikation
       <Exakte Kommandos + erwartetes Ergebnis>

## Teil 2 — Scope-Gate automatisieren
1. Erstelle in `xtask/src/main.rs` einen neuen Match-Arm `"check-scope-lock"`, der:
   - Aus dem PR-Body (via `GITHUB_EVENT_PATH`-Umgebungsvariable in CI, JSON-Payload des
     `pull_request`-Events) den Abschnitt zwischen `<!-- SCOPE-LOCK-START -->` und
     `<!-- SCOPE-LOCK-END -->` extrahiert, FALLS vorhanden (nicht jeder PR muss eine Spec referenzieren
     — mache dieses Gate weich/informativ, nicht hart blockierend, siehe Schritt 3).
   - Die tatsächlich im PR geänderten Dateien ermittelt (`git diff --name-only origin/main...HEAD` oder
     äquivalent in CI via `git diff --name-only ${{ github.event.pull_request.base.sha }}
     ${{ github.event.pull_request.head.sha }}`).
   - Geänderte Dateien gegen `FORBIDDEN_PATHS` abgleicht (Glob-Matching).
   - Bei Treffer: KEINEN Hard-Fail, sondern eine Warnung als GitHub-PR-Kommentar postet (via
     `actions/github-script`), die den Konflikt benennt — Härte des Gates bewusst niedrig halten, da
     nicht jeder PR eine Spec mit Sperrzonen-Block hat und False Positives (z. B. bei
     Cross-Cutting-Fixes wie P-J1–P-J9 selbst, die bewusst mehrere Governance-Dateien anfassen) sonst
     den Workflow blockieren würden.
2. Füge einen neuen, informativen (nicht Required-Status-Check) Job in `context-gates.yml` hinzu:

       scope-lock-advisory:
         name: "Sperrzonen-Hinweis (informativ)"
         runs-on: ubuntu-latest
         if: github.event_name == 'pull_request'
         steps:
           - uses: actions/checkout@v4
             with:
               fetch-depth: 0
           - uses: dtolnay/rust-toolchain@stable
           - run: cargo run -p xtask -- check-scope-lock
             continue-on-error: true

3. Begründe explizit im Code-Kommentar/PR, WARUM dieses Gate `continue-on-error: true` bzw. informativ
   bleibt: Ein hartes Scope-Lock-Gate würde legitime Cross-Cutting-Governance-PRs (wie diese gesamte
   P-J1–P-J10-Serie) systematisch blockieren, da sie per Definition mehrere Bereiche anfassen. Das Gate
   soll Scope Creep bei Feature-PRs sichtbar machen, nicht Governance-Arbeit verhindern.

## Teil 3 — Rückwirkende Dokumentation
1. Aktualisiere `AGENTS.md §7` (Governance Documents Tabelle) um einen Eintrag für
   `.jules/TASK_PROMPT_TEMPLATE.md` mit "Lesen wenn: Formulieren eines neuen Prompts/Auftrags für eine
   Jules-Session (auch für dich selbst als Prompt-Autor)".
2. Ergänze in `AGENTS.md §6` (Session Protocol) einen Hinweis: "Falls diese Session im Rahmen eines
   Prompts läuft, der `.jules/TASK_PROMPT_TEMPLATE.md` folgt: Sperrzonen-Feld des Prompts als bindend
   behandeln, auch wenn `check-scope-lock` nur informativ ist."

# Akzeptanzkriterien
- `docs/specs/TEMPLATE_MICRO_SPEC.md` hat einen maschinenlesbaren Sperrzonen-Block.
- `.jules/TASK_PROMPT_TEMPLATE.md` existiert als eigenständige, sofort nutzbare Vorlage.
- `cargo run -p xtask -- check-scope-lock` läuft ohne Absturz, auch wenn kein Sperrzonen-Block im
  PR-Body gefunden wird (graceful no-op mit informativer Meldung).
- Neuer CI-Job ist informativ (`continue-on-error: true`), kein Required-Check.
- `AGENTS.md` referenziert die neue Vorlage.

# Verifikation
Erstelle einen Test-PR mit einem PR-Body, der einen Sperrzonen-Block mit `FORBIDDEN_PATHS: - justfile`
enthält, ändere testweise `justfile` in diesem PR, und verifiziere, dass der `scope-lock-advisory`-Job
einen Warn-Kommentar postet, den PR aber NICHT blockiert (grüner/gelber, nicht roter Status).
```

---

## Teil E — Wie du das ab jetzt nutzt (Zusammenfassung deines neuen Workflows)

1. **Einmalig:** Führe P-J1 → P-J10 sequenziell aus (ein Prompt = eine Jules-Session, in dieser
   Reihenfolge, da spätere Prompts auf früheren aufbauen — insbesondere P-J9 auf P-J5, P-J7 auf P-J6).
2. **Ab sofort für jedes neue Feature/jeden neuen Bugfix:** Nutze `.jules/TASK_PROMPT_TEMPLATE.md`
   (aus P-J10) als verpflichtenden Ausgangspunkt für jeden neuen Prompt, den du selbst formulierst —
   das Sperrzonen-Feld ist dabei PFLICHT, nicht optional.
3. **Wöchentlich, automatisch:** Der in P-J7 gebaute Cron-Job generiert selbstständig ein
   Audit-Issue — du musst nicht mehr manuell wie bisher ein `memfuse_neue_befunde.md`-artiges Dokument
   erstellen und einfügen. Deine Rolle verschiebt sich von "Auditor, der Findings sammelt" zu "Reviewer,
   der fertige PRs freigibt".
4. **Bei jedem PR, automatisch, hart erzwungen:** `just check`, `just dag-check`, `just debt-audit`
   (jetzt dank P-J1/P-J2 tatsächlich lauffähig und verdrahtet), plus die bestehenden 9
   Context-Gates, plus (neu) das JULES_CONTEXT-Freshness-Gate.
5. **Bei jedem PR, automatisch, informativ:** Cross-Platform-Tests (P-J8), Scope-Lock-Hinweis (P-J10).
6. **On-Demand/wöchentlich rotierend:** Mutation-Testing (P-J6) gegen die architektonisch komplexesten
   Crates.
7. **Bei jeder neuen Nebenläufigkeits-Primitive (neuer `Mutex`/`RwLock` mit einer zu garantierenden
   Invariante):** Ergänze einen `loom`-Test nach dem in P-J5/P-J9 etablierten Muster — mache das zu
   einer neuen Zeile in `AGENTS.md §4` ("Non-Obvious Decisions"), falls du nach P-J5 feststellst, dass
   sich dieses Muster wiederholt.

Der rote Faden: Du bist nicht mehr derjenige, der Bugs findet und Prompts formuliert, um sie zu fixen.
Du bist derjenige, der das System kalibriert, das Bugs verhindert, bevor Jules sie überhaupt in einen
PR schreiben kann — Fehler werden zu Exit-Codes ≠ 0 in Jules' eigener Selbstkorrektur-Schleife, lange
bevor du sie zu Gesicht bekommst.
