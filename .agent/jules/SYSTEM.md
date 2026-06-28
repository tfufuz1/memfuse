# Jules Agent Orchestration — MemFuse Specification v3.0

> **Zweck:** Statische Prompts + dynamischer Repo-State für progressive LLM-Entwicklung.
> **Agent:** Google Jules (10-20 Scheduled Tasks/Tag)
> **Invariante:** Jeder Prompt liest State → handelt → schreibt State zurück.

---

## 1. Architektur

```
STATISCH (Prompt-Text)              DYNAMISCH (Repository-Dateien)
──────────────────────              ─────────────────────────────
"Du bist der Store-Engineer.        docs/STATUS.md      ← Aktuelles WP, Blocker
 Lies zuerst docs/STATUS.md         docs/DAILY_LOG.md   ← Was gestern passiert ist
 und AGENTS.md..."                  docs/audit/CRIT.md  ← Offene Critical Findings
                                    docs/audit/HIGH.md  ← Offene High Findings
                                    .agent/jules/context/HEALTH.md  ← Metriken
                                    .agent/jules/context/CURRENT_API.md ← pub API
```

### Dateihierarchie

```
.agent/jules/
├── SYSTEM.md                  ← Diese Spezifikation
├── CHANGELOG.md               ← Was Jules getan hat (append-only)
├── context/                   ← CI-generiert (nicht manuell editieren)
│   ├── CURRENT_API.md
│   └── HEALTH.md
└── scripts/
    ├── inject-context.sh
    └── health-snapshot.sh

docs/
├── STATUS.md                  ← Täglicher Zustand (Jules schreibt)
├── DAILY_LOG.md               ← Append-only Protokoll
├── audit/
│   ├── CRITICAL.md            ← CRIT-Findings
│   ├── HIGH.md                ← HIGH-Findings
│   └── RESOLVED.md            ← Erledigte Findings
├── roadmap/
│   └── NEXT_WP.md             ← Nächstes Work Package
└── specs/                     ← SPEC-*.md pro WP
```

---

## 2. docs/STATUS.md — Format

```markdown
# MemFuse Daily Status
**Zuletzt aktualisiert:** [ISO-Datum] von [Prompt-ID]
**Compiler:** [GRÜN/ROT — Anzahl Fehler]
**Tests:** [X passed, Y failed]

## Aktives WP
- ID: WP-X.Y
- Crate: memfuse-xxx
- Status: IN_PROGRESS | BLOCKED | NEEDS_REVIEW
- Blockiert durch: [Finding-ID oder "nichts"]

## Letzte Aktion
[1-2 Sätze]

## Nächste Priorität
[1 konkrete Aufgabe]

## Offene CRIT-Findings
[Liste aus docs/audit/CRITICAL.md mit Status=OPEN]
```

## 3. docs/DAILY_LOG.md — Format (append oben)

```markdown
## [ISO-Datum] — [Prompt-ID]
**WP:** WP-X.Y | **Aktion:** [Was getan]
**PR:** #N | **Tests:** X passed | **Nächstes:** [1 Satz]
---
```

---

## 4. Anti-Pattern-Katalog

```
❌  .unwrap()/.expect() außerhalb von #[cfg(test)]
    → .ok_or_else(|| MemFuseError::xxx)? oder ?

❌  Arc<dyn Trait> wenn Trait async-Methoden hat
    → Generische Parameter <S: Trait + Send + Sync + 'static>

❌  std::fs in async-Kontext
    → tokio::fs oder spawn_blocking

❌  Zyklische Dependencies zwischen Crates
    → DAG: core ← L1 ← db ← py

❌  unsafe außerhalb von distance.rs
    → Jeder unsafe-Block braucht // SAFETY: Kommentar

❌  Trait-Signaturen in impl ändern ohne traits.rs zu lesen
    → traits.rs ist Source of Truth

❌  PR ohne DAILY_LOG.md Update
    → CI blockiert Merge

❌  Mehr als 3 Dateien in einem atomaren Fix-PR
    → Ein Problem pro PR

❌  WP als ✅ markieren ohne Triple-Test-Gate
    → cargo check + clippy + test (3x)

❌  Administrative Metadaten in Code-Kommentaren (STATUS:, AGENT:, WP:)
    → Code-Kommentare dürfen keinen Zustand haben. Verwende stattdessen:
      - // INVARIANT: [Systemkritische Regel]
      - // CONSTRAINT: [Technische Limitierung]
      - // INTENT: [Das 'Warum' bei unüblichem Code]
      - // SAFETY: [Beweis für unsafe-Blöcke]
```

---

## 5. Bekannte LLM-Rust-Fehler

### F-001: StorageEngine nicht dyn-kompatibel
```rust
// ❌ FALSCH:
struct Foo { storage: Arc<dyn StorageEngine> }
// ✅ RICHTIG:
struct Foo<S: StorageEngine + Send + Sync + 'static> { storage: Arc<S> }
```

### F-002: Lifetime-Mismatch Trait vs Impl
```rust
// ❌ FALSCH (extra lifetime):
async fn add_entity<'a>(&'a self, tx: TxId, entity: EntityId) -> Result<()>
// ✅ RICHTIG (exakte Kopie aus traits.rs):
async fn add_entity(&self, tx: TxId, entity: EntityId) -> Result<()>
```

### F-003: Blocking IO in async
```rust
// ❌ FALSCH:
let file = std::fs::File::open(&path)?;
// ✅ RICHTIG:
let file = tokio::task::spawn_blocking(move || std::fs::File::open(&path)).await??;
```

---

## 6. CI Scripts

### .agent/jules/scripts/health-snapshot.sh
```bash
#!/usr/bin/env bash
set -euo pipefail
OUT=".agent/jules/context/HEALTH.md"
echo "# Codebase Health" > "$OUT"
echo "Generated: $(date -u --iso-8601=seconds)" >> "$OUT"
echo "" >> "$OUT"
UNWRAPS=$(grep -rn '\.unwrap()\|\.expect(' crates/ --include="*.rs" \
  | grep -v '/tests/' | grep -v 'mod tests' | grep -v 'cfg(test)' | wc -l)
UNSAFE=$(grep -rn 'unsafe ' crates/ --include="*.rs" \
  | grep -v '/tests/' | grep -v 'memfuse_generated.rs' | wc -l)
STDFS=$(grep -rn 'std::fs::' crates/ --include="*.rs" \
  | grep -v '/tests/' | wc -l)
OPEN=$(grep -rn 'STATUS:OPEN\|STATUS:REVIEW\|STATUS:SCAFFOLD' \
  crates/ --include="*.rs" | wc -l)
echo "| Metric | Count |" >> "$OUT"
echo "|--------|-------|" >> "$OUT"
echo "| unwrap/expect (prod) | $UNWRAPS |" >> "$OUT"
echo "| unsafe blocks | $UNSAFE |" >> "$OUT"
echo "| std::fs (blocking) | $STDFS |" >> "$OUT"
echo "| Open anchors | $OPEN |" >> "$OUT"
echo "" >> "$OUT"
echo "## Open Anchors" >> "$OUT"
grep -rn 'STATUS:OPEN\|STATUS:REVIEW\|STATUS:SCAFFOLD' crates/ --include="*.rs" \
  | sed 's|crates/||' >> "$OUT" || echo "None." >> "$OUT"
```

### .agent/jules/scripts/inject-context.sh
```bash
#!/usr/bin/env bash
set -euo pipefail
echo "# MemFuse Public API Snapshot"
echo "Generated: $(date -u --iso-8601=seconds)"
echo ""
for D in crates/memfuse-*/; do
  C=$(basename "$D")
  echo "## $C"
  echo '```rust'
  grep -n 'pub trait \|pub async fn \|pub fn \|pub struct \|pub enum ' \
    "$D"/src/*.rs 2>/dev/null | sort -u || echo "// no pub items"
  echo '```'
  echo ""
done
```

---

## 7. GitHub Actions

### Quality Gate (jules-quality-gate.yml)
```yaml
name: Jules Quality Gate
on:
  pull_request:
    branches: [main, dev]

jobs:
  triple-gate:
    name: Triple-Gate Validator
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with: { components: "rustfmt, clippy" }
      - uses: Swatinem/rust-cache@v2

      - name: "Gate 1 — Compile"
        run: cargo check --workspace --all-targets

      - name: "Gate 2 — Clippy"
        run: |
          cargo clippy --workspace --all-targets -- \
            -D warnings -D clippy::unwrap_used -D clippy::expect_used

      - name: "Gate 3 — Anti-Pattern Scan"
        run: |
          FAIL=0
          if grep -rn '\.unwrap()' crates/*/src/ --include="*.rs" \
            | grep -v '#\[cfg(test)\]' | grep -v '// unwrap-ok:' | grep -q .; then
            echo "❌ .unwrap() in prod code"; FAIL=1; fi
          if grep -rn 'std::fs::' crates/*/src/ --include="*.rs" \
            | grep -v '// std-fs-ok:' | grep -v '/tests/' | grep -q .; then
            echo "⚠️ std::fs in prod code (migrate to tokio::fs)"; fi
          exit $FAIL

      - name: "Gate 4 — Tests (3x)"
        run: |
          for R in 1 2 3; do
            echo "--- Run $R/3 ---"
            cargo test --workspace -- --test-threads=1 || exit 1
          done

      - name: "Gate 5 — DAILY_LOG updated"
        run: |
          if ! git diff origin/main --name-only | grep -q "docs/DAILY_LOG.md"; then
            echo "❌ docs/DAILY_LOG.md not updated"; exit 1; fi

  health-snapshot:
    name: Update Health Context
    runs-on: ubuntu-latest
    needs: triple-gate
    if: success()
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: |
          mkdir -p .agent/jules/context
          bash .agent/jules/scripts/health-snapshot.sh
          bash .agent/jules/scripts/inject-context.sh > .agent/jules/context/CURRENT_API.md
      - run: |
          git config user.name "Jules CI Bot"
          git config user.email "bot@memfuse.dev"
          git add .agent/jules/context/
          git diff --staged --quiet || git commit -m "chore: health snapshot"
          git push || true
```

### PR Template (.github/pull_request_template.md)
```markdown
## Prompt-ID
<!-- P00-P10 -->

## WP-Referenz
<!-- WP-X.Y — Name -->

## Änderungen
<!-- Dateiliste mit 1-Satz Beschreibung -->

## Tests
<!-- cargo check/test/clippy Ergebnisse -->

## Status-Update
<!-- 2 Sätze: Was getan, was kommt als nächstes -->
```

---

## 8. Tages-Zeitplan

```
UTC   Prompt  Rolle                    Frequenz
─────────────────────────────────────────────────
05:00  P00    Daily Audit & Status     Täglich
06:00  P01    Debt Hunter              Täglich
07:00  P02    Core Guardian            Täglich
08:00  P03    Store Engineer           Täglich
09:00  P04    Index Master             Täglich
10:00  P05    Text + Graph Engineer    Täglich
11:00  P06    DB Architect             Täglich
12:00  P07    Python Bridge            Täglich
14:00  P08    QA Cross-Crate           Täglich
16:00  P09    Spec Writer              Mo
17:00  P10    Roadmap Sync             Fr
─────────────────────────────────────────────────
Täglich: 9 Prompts | Mo/Fr: +1 = max 10/Tag
Reserve: 10 weitere Slots für Retry/Fix
```
